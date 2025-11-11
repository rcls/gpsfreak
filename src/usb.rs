/// USB for GPS ref.
/// Endpoints:
/// 0 : Control, as always.
///   OUT: 64 bytes at 0x80 offset.
///   IN : 64 bytes at 0xc0 offset.  TODO - do we use both?
///   CHEP 0
/// 01, 81: CDC ACM data transfer, bulk
///   OUT (RX): 2 × 64 bytes at 0x100 offset.
///     DTOGRX==0 -> RX into RXTX buf. 1×2 + 1 = 3.
///     DTOGRX==1 -> RX into TXRX buf. 1×2 + 0 = 2.
///   IN  (TX): 8 x 64 bytes at 0x200 offset.
///     DTOGTX==0 -> TX from TXRX buf. 2×2 + 0 = 4.
///     DTOGTX==1 -> TX from RXTX buf. 2×2 + 1 = 5.
/// 82: CDC ACM interrupt IN (to host).
///   64 bytes at 0x40 offset.
///   CHEP 2

mod control;
mod descriptor;
mod hardware;
mod strings;
mod types;

use control::{CONTROL_STATE, ControlState};
use hardware::*;
use types::*;

use crate::cpu::{barrier, interrupt, nothing};
use crate::link_assert;
use crate::vcell::{UCell, VCell};

use stm32h503::Interrupt::USB_FS as INTERRUPT;

macro_rules!ctrl_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!intr_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!main_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!srx_dbgln  {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!stx_dbgln  {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!usb_dbgln  {($($tt:tt)*) => {if true  {dbgln!($($tt)*)}};}
macro_rules!fast_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}

pub(crate) use {ctrl_dbgln, usb_dbgln};

#[allow(non_camel_case_types)]
#[derive_const(Default)]
struct USB_State {
    /// Base of the ACM CDC TX buffer we are accumulating.
    tx_base: *mut u32 = BULK_TX_BUF as _,
    /// Current number of bytes in TX buffer we are accumulating.
    tx_len: usize,
    /// Accumulating bytes into 32 bit words.
    tx_part: u32,
    /// Is software still processing a received buffer?
    rx_processing: RxProcessing = RxProcessing::Idle,
}

/// Status of processing received CDC ACM serial data.
#[derive(PartialEq)]
enum RxProcessing {
    /// Nothing is being processed.
    Idle,
    /// The serial handling is currently forwarding some data.
    Processing,
    /// The serial handling is currently busy with somebody elses data, but
    /// we have data for it.
    Blocked,
}

unsafe impl Sync for USB_State {}

static USB_STATE: UCell<USB_State> = Default::default();

/// Operating systems appear to think that changing baud rates on serial ports
/// at random is fine.  It is not.  So we ignore the CDC ACM baud rate
/// and do our own thing.  But we still fake baud rate responses just to
/// keep random OSes happy.
static FAKE_BAUD: VCell<u32> = VCell::new(9600);

pub fn init() {
    let crs   = unsafe {&*stm32h503::CRS  ::ptr()};
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let usb   = unsafe {&*stm32h503::USB  ::ptr()};

    // Bring up the HSI48 clock.
    rcc.CR.modify(|_,w| w.HSI48ON().set_bit());
    while !rcc.CR.read().HSI48RDY().bit() {
    }
    // Route the HSI48 to USB.
    rcc.CCIPR4.modify(|_,w| w.USBFSSEL().B_0x3());

    // Configure pins (PA11, PA12).  (PA9 = VBUS?)
    gpioa.AFRH.modify(|_,w| w.AFSEL11().B_0xA().AFSEL12().B_0xA());
    gpioa.MODER.modify(|_,w| w.MODE11().B_0x2().MODE12().B_0x2());

    // Enable CRS and USB clocks.
    rcc.APB1LENR.modify(|_,w| w.CRSEN().set_bit());
    rcc.APB2ENR.modify(|_,w| w.USBFSEN().set_bit());

    // crs_sync_in_2 USB SOF selected - default.
    crs.CR.modify(|_,w| w.AUTOTRIMEN().set_bit());
    crs.CR.modify(|_,w| w.AUTOTRIMEN().set_bit().CEN().set_bit());

    usb.CNTR.write(|w| w.PDWN().clear_bit().USBRST().set_bit());
    // Wait t_startup (1µs).
    for _ in 0 .. crate::cpu::CPU_FREQ / 2000000 {
        nothing();
    }
    usb.CNTR.write(|w| w.PDWN().clear_bit().USBRST().clear_bit());
    usb.BCDR.write(|w| w.DPPU_DPD().set_bit());

    // Clear any spurious interrupts.
    usb.ISTR.write(|w| w);

    usb_initialize(unsafe {CONTROL_STATE.as_mut()});

    interrupt::enable_priority(INTERRUPT, interrupt::PRIO_COMMS);

    // We use the PENDSV exception to dispatch some work at lower priority.
    let scb = unsafe {&*cortex_m::peripheral::SCB::PTR};
    let pendsv_prio = &scb.shpr[10];
    // Cortex-M crate has two different ideas of what the SHPR is, make sure we
    // are built with the correct one.
    link_assert!(pendsv_prio as *const _ as usize == 0xe000ed22);
    unsafe {pendsv_prio.write(crate::cpu::interrupt::PRIO_APP)};
}

fn usb_isr() {
    unsafe{USB_STATE.as_mut()}.isr();
}

pub fn serial_tx_byte(byte: u8) {
    unsafe{USB_STATE.as_mut()}.serial_tx_byte(byte);
}

pub fn serial_rx_done() {
    unsafe{USB_STATE.as_mut()}.serial_rx_done();
}

fn set_configuration(cfg: u8) {
    usb_dbgln!("Set configuration {cfg}");

    clear_buffer_descs();

    // Serial.
    let ser = chep_ser().read();
    chep_ser().write(|w|w.serial().init(&ser).rx_valid(&ser).tx_nak(&ser));

    // Interrupt.
    let intr = chep_intr().read();
    chep_intr().write(|w| w.interrupt().init(&intr).tx_nak(&intr));

    // Main.  FIXME - this can happen underneath processing a message, leaving
    // us in inconsistent state.  We should recover!
    let main = chep_main().read();
    chep_main().write(|w| w.main().init(&main).rx_valid(&main).tx_nak(&main));
}

impl USB_State {
    fn isr(&mut self) {
        let usb = unsafe {&*stm32h503::USB::ptr()};
        let mut istr = usb.ISTR.read();
        let not_only_sof = istr.RST_DCON().bit() || istr.CTR().bit();
        if not_only_sof {
            fast_dbgln!("*** USB isr ISTR = {:#010x} FN={}",
                        istr.bits(), usb.FNR.read().FN().bits());
        }
        // Write zero to the interrupt bits we wish to acknowledge.
        usb.ISTR.write(|w| w.bits(!istr.bits() & !0x37fc0));

        if istr.SOF().bit() {
            self.start_of_frame();
        }

        if istr.RST_DCON().bit() {
            usb_initialize(unsafe {CONTROL_STATE.as_mut()});
        }

        while istr.CTR().bit() {
            if istr.DIR().bit() {
                errata_delay();
            }
            match istr.bits() & 31 {
                0  => unsafe {CONTROL_STATE.as_mut()}.control_tx_handler(),
                16 => unsafe {CONTROL_STATE.as_mut()}.control_rx_handler(),
                1  => self.serial_tx_handler(),
                17 => self.serial_rx_handler(),
                2  => self.interrupt_handler(),
                3  => main_tx_handler(),
                19 => main_rx_handler(),
                _  => {
                    dbgln!("Bugger endpoint?, ISTR = {:#010x}", istr.bits());
                    break;  // FIXME, this will hang!
                },
            }
            istr = usb.ISTR.read();
        }

        if not_only_sof {
            crate::led::BLUE.pulse(true);
            fast_dbgln!("CHEP0 now {:#010x}\n***", chep_ctrl().read().bits());
        }
    }

    /// On a start-of-frame interrupt, if the serial IN end-point is idle, we
    /// push through any pending data.  Hopefully quickly enough for the actual
    /// IN request.
    fn start_of_frame(&mut self) {
        // If serial TX is idle, then push through any pending data.
        let chep = chep_ser().read();
        if !chep.tx_nakking() || self.tx_len == 0 {
            return;                         // Not ready for data.
        }

        // Store any sub-word bytes.
        if self.tx_len & 3 != 0 {
            let ptr = self.tx_base.wrapping_byte_add(self.tx_len & !3);
            unsafe {*ptr = self.tx_part >> 32 - 8 * (self.tx_len & 3)};
        }

        self.send_tx_buffer(chep);
    }

    fn serial_tx_handler(&mut self) {
        let chep = chep_ser().read();
        if !chep.VTTX().bit() {
            stx_dbgln!("serial tx spurious CHEP {:#06x}", chep.bits());
            return;
        }

        if !chep.tx_nakking() || self.tx_len < 64 {
            stx_dbgln!("STX wait for more.  CHEP {:#06x}", chep.bits());
            chep_ser().write(|w| w.serial().VTTX().clear_bit());
            return;
        }

        self.send_tx_buffer(chep);
    }

    fn serial_tx_byte(&mut self, byte: u8) {
        fast_dbgln!("serial_tx_byte {byte:02x}");
        if self.tx_len >= 64 {
            return;                     // We're full.  Drop it.
        }
        self.tx_part = (self.tx_part >> 8) + ((byte as u32) << 24);
        if self.tx_len & 3 == 3 {
            let ptr = self.tx_base.wrapping_byte_add(self.tx_len - 3);
            unsafe {*ptr = self.tx_part};
        }
        self.tx_len += 1;
        if self.tx_len < 64 {
            return;
        }

        let chep = chep_ser().read();
        if chep.rx_disabled() {
            return;                         // Not initialized, or reset.
        }
        if chep.tx_active() {
            stx_dbgln!("USB TX push now full CHEP {:#06x}", chep.bits());
        }

        self.send_tx_buffer(chep);
    }

    fn send_tx_buffer(&mut self, chep: hardware::CheprR) {
        bd_serial().tx_set(self.tx_base as _, self.tx_len);
        // It's OK to clear VTTX even if we haven't handled the interrupt yet
        // - we are doing just what the ISR would.
        chep_ser().write(|w| w.serial().VTTX().clear_bit().tx_valid(&chep));
        stx_dbgln!("serial tx arm, len {} CHEP {:#06x} was {:#06x}",
                   self.tx_len, chep_ser().read().bits(), chep.bits());

        // We have two TX buffers, differing only by a single bit.
        self.tx_base = (self.tx_base as usize ^ 64) as _;
        self.tx_len = 0;
    }

    fn serial_rx_handler(&mut self) {
        let chep = chep_ser().read();
        if !chep.VTRX().bit() {
            srx_dbgln!("SRX spurious! CHEP {:#06x}", chep.bits());
            return;
        }
        if !chep.rx_nakking() {
            chep_ser().write(|w| w.serial().VTRX().clear_bit());
            srx_dbgln!("SRX extra! CHEP {:#06x} was {:#06x}",
                       chep_ser().read().bits(), chep.bits());
            return;
        }

        let bd = bd_serial().rx.read();
        let len = chep_bd_len(bd);
        if len == 0 {
            // Just kick off the same block again.
            chep_ser().write(|w| w.serial().VTRX().clear_bit().rx_valid(&chep));
            srx_dbgln!("SRX, Zero size CHEP={:#06x} was {:#06x}",
                       chep_ser().read().bits(), chep.bits());
            return;
        }

        if self.rx_processing != RxProcessing::Idle {
            srx_dbgln!("SRX jammed! CHEP = {:#06x}", chep.bits());
            chep_ser().write(|w| w.serial().VTRX().clear_bit());
            return;
        }

        // Start an RX into the other buffer.  It's OK to clear VTRX here!
        bd_serial().rx.write(bd ^ 64);
        chep_ser().write(|w| w.serial().VTRX().clear_bit().rx_valid(&chep));
        srx_dbgln!("SRX continue CHEP {:#06x} was {:#06x}",
                   chep.bits(), chep.bits());

        // Dispatch the block.
        if crate::gps_uart::dma_tx(chep_bd_ptr(bd), len) {
            self.rx_processing = RxProcessing::Processing;
        }
        else {
            self.rx_processing = RxProcessing::Blocked;
        }
    }

    /// Notification from the consumer that we have finished processing a buffer
    /// and are ready for the next.  This is also called when the serial
    /// handling has processed someone elses data.
    fn serial_rx_done(&mut self) {
        // If another guy is sending stuff out the serial, then we may get
        // spurious calls.
        if self.rx_processing == RxProcessing::Idle {
            return;
        }

        let chep = chep_ser().read();

        if !chep.rx_nakking() {
            self.rx_processing = RxProcessing::Idle;
            srx_dbgln!("serial_rx_done, now idle.  CHEP={:#06x}", chep.bits());
            return;                     // RX is in progress (or not wanted).
        }

        self.rx_processing = RxProcessing::Processing;
        let bd = bd_serial().rx.read();
        // Start processing the pending block.
        crate::gps_uart::dma_tx(chep_bd_ptr(bd), chep_bd_len(bd));

        // Start an RX.  It's OK to clear VTRX here.
        bd_serial().rx.write(bd ^ 64);
        chep_ser().write(|w| w.serial().VTRX().clear_bit().rx_valid(&chep));
        srx_dbgln!("serial_rx_done, unblocked, CHEP {:#06x} was {:#06x}",
                   chep_ser().read().bits(), chep.bits());
    }

    /// This handles USB interrupt pipe VTTX not CPU interrupts!
    fn interrupt_handler(&mut self) {
        // TODO - nothing here yet!
        let chep = chep_intr().read();
        chep_intr().write(|w| w.interrupt().VTTX().clear_bit());
        intr_dbgln!("interrupt_tx_handler CHEP now {:#06x} was {:#06x}",
                    chep_intr().read().bits(), chep.bits());
    }
}

fn main_rx_handler() {
    let chep = chep_main().read();
    if !chep.VTRX().bit() {
        main_dbgln!("main: Spurious RX interrupt, CHEP {:#6x}", chep.bits());
        return;
    }
    main_dbgln!("main: RX interrupt, CHEP {:#6x}", chep.bits());

    // We notify the application by triggering PendSV.  The application
    // can notify completion, either by transmitting a message or by
    // by calling the completion function.
    let scb = unsafe {&*cortex_m::peripheral::SCB::PTR};
    unsafe {scb.icsr.write(1 << 28)};

    chep_main().write(|w| w.main().VTRX().clear_bit());
}

/// We have finished processing a message by sending a response. Rearm the RX.
/// TODO: maybe we should also re-arm on a timeout, in case the user doesn't
/// read the response?
fn main_tx_handler() {
    let chep = chep_main().read();
    if !chep.VTTX().bit() {
        main_dbgln!("main: Spurious TX interrupt, CHEP {:#6x}", chep.bits());
        return;
    }
    chep_main().write(|w| w.main().rx_valid(&chep).VTTX().clear_bit());
    main_dbgln!("main: TX done CHEP {:#06x} was {:#06x}",
                chep_main().read().bits(), chep.bits());
}

// Called at lower priority and can get interrupted!
fn main_tx_response(message: &[u8]) {
let chep = chep_main().read();
    if message.len() == 0 {
        main_dbgln!("main_tx_response, no data, rearm");
        chep_main().write(|w| w.main().rx_valid(&chep));
        return;
    }
    // For now we don't support long messages.
    let len = message.len().min(64);
    unsafe {copy_by_dest32(message.as_ptr(), MAIN_TX_BUF, message.len())};

    bd_main().tx_set(MAIN_TX_BUF, len);

    let chep = chep_main().read();
    chep_main().write(|w| w.main().tx_valid(&chep));

    main_dbgln!("main tx {len} bytes, {}CHEP now {:#06x} was {:#06x}",
                if chep.tx_active() {"INCORRECT STATE "} else {""},
                chep_main().read().bits(),
                chep.bits());
}

fn set_control_line_state(_value: u8) -> SetupResult {
    usb_tx_interrupt();
    SetupResult::no_data()
}

fn set_line_coding() -> bool {
    let line_coding: LineCoding = unsafe {
        core::mem::transmute_copy (
            &* (CTRL_RX_BUF as *const (u32, u32))
        )
    };
    ctrl_dbgln!("USB Set Line Coding, Baud = {}", line_coding.dte_rate);
    FAKE_BAUD.write(line_coding.dte_rate);
    true
}

fn get_line_coding() -> SetupResult {
    ctrl_dbgln!("USB Get Line Coding");
    static LINE_CODING: UCell<LineCoding> = Default::default();
    let lc = unsafe {LINE_CODING.as_mut()};
    *lc = LineCoding {
        // "Yes honey, whatever you say."
        dte_rate: FAKE_BAUD.read(),
        char_format: 0, parity_type: 0, data_bits: 8};
    SetupResult::tx_data(lc)
}

fn usb_tx_interrupt() {
    intr_dbgln!("Sending USB interrupt");
    // Just send a canned response, because USB sucks.  We don't care if one
    // response stomps on a previous one, because we always send the same data.
    #[allow(dead_code)]
    #[repr(C)]
    struct LineState{header: SetupHeader, state: u16}
    static LINE_STATE: LineState = LineState{
        header: SetupHeader {
            request_type: 0xa1, request: 0x20, value_lo: 3,
            value_hi: 0, index: 0, length: 2},
        state: 3,
    };
    unsafe {copy_by_dest32(&LINE_STATE as *const _ as *const _,
                           INTR_TX_BUF, size_of::<LineState>())};
    barrier();
    bd_interrupt().tx.write(chep_bd_tx(INTR_TX_OFFSET, size_of::<LineState>()));
    let chep = chep_intr().read();
    chep_intr().write(|w| w.interrupt().tx_valid(&chep));
    intr_dbgln!("INTR CHEP now {:#06x} was {:#06x}",
                chep_intr().read().bits(), chep.bits());
}

/// Initialize all the RX BD entries, except for the control ones.
fn clear_buffer_descs() {
    bd_serial().rx_set::<64>(BULK_RX_BUF);
    bd_main()  .rx_set::<64>(MAIN_RX_BUF);
}

/// PendSV ISR for handling device commands at appropriate priority.
fn command_handler() {
    main_dbgln!("Command handler entry");

    // Get a point to the message.  TODO - copy!
    let message = unsafe {&*(MAIN_RX_BUF as *const crate::command::MessageBuf)};
    crate::command::command_handler(
        &message, chep_bd_len(bd_main().rx.read()), main_tx_response);
}

fn usb_initialize(cs: &mut ControlState) {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    usb_dbgln!("USB initialize...");

    cs.initialize();

    usb.CNTR.write(
        |w|w.PDWN().clear_bit().USBRST().clear_bit()
            .RST_DCONM().set_bit().CTRM().set_bit().SOFM().set_bit());

    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(0));

    bd_control().rx.write(chep_block::<64>(CTRL_RX_OFFSET));
    clear_buffer_descs();

    let ctrl = chep_ctrl().read();
    chep_ctrl().write(
        |w| w.control().dtogrx(&ctrl, false).dtogtx(&ctrl, false)
             .rx_valid(&ctrl));
}

fn errata_delay() {
    // ERRATA:
    //
    // During OUT transfers, the correct transfer interrupt (CTR) is
    // triggered a little before the last USB SRAM accesses have completed.
    // If the software responds quickly to the interrupt, the full buffer
    // contents may not be correct.
    //
    // Workaround: Software should ensure that a small delay is included
    // before accessing the SRAM contents. This delay should be
    // 800 ns in Full Speed mode and 6.4 μs in Low Speed mode.
    for _ in 0 .. crate::cpu::CPU_FREQ / 1250000 / 2 {
        nothing();
    }
}

impl crate::cpu::VectorTable {
    pub const fn usb(&mut self) -> &mut Self {
        self.pendsv = command_handler;
        self.isr(INTERRUPT, usb_isr)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.pendsv == command_handler);
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == usb_isr);
}
