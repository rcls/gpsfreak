use crate::cpu::barrier;
use crate::freak_usb::{
    BULK_RX_BUF, BULK_TX_BUF, INTR_TX_BUF, INTR_TX_OFFSET,
    bd_interrupt, bd_serial, chep_intr, chep_ser};
use crate::freak_usb::{CheprWriter as _};
use crate::usb;
use crate::dbgln;
use crate::freak_descriptors::{INTF_ACM_DATA, INTF_ACM_INTR};
use crate::vcell::{UCell, VCell};

use super::USB_STATE;

use usb::{EndpointPair, ctrl_dbgln};
use usb::types::{LineCoding, SetupHeader, SetupResult};
use usb::hardware::{
    CTRL_RX_BUF, CheprR, CheprReader, CheprWriter,
    chep_bd_len, chep_bd_ptr, chep_bd_tx, copy_by_dest32};

macro_rules!srx_dbgln  {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}
macro_rules!stx_dbgln  {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}
macro_rules!fast_dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}
macro_rules!intr_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}

/// Operating systems appear to think that changing baud rates on serial ports
/// at random is fine.  It is not.  So we ignore the CDC ACM baud rate and do
/// our own thing.  But we still fake baud rate responses just to keep random
/// OSes happy.
static FAKE_BAUD: VCell<u32> = VCell::new(9600);

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

#[derive_const(Default)]
pub struct FreakUSBSerial {
    /// Base of the ACM CDC TX buffer we are accumulating.
    tx_base: *mut u32 = BULK_TX_BUF as _,
    /// Current number of bytes in TX buffer we are accumulating.
    tx_len: usize,
    /// Accumulating bytes into 32 bit words.
    tx_part: u32,

    /// Is software still processing a received buffer?
    rx_processing: RxProcessing = RxProcessing::Idle,
}

pub fn serial_rx_done() {
    unsafe{USB_STATE.as_mut()}.eps.ep1.serial_rx_done();
}

pub fn serial_tx_byte(byte: u8) {
    unsafe{USB_STATE.as_mut()}.eps.ep1.serial_tx_byte(byte);
}

impl usb::EndpointPair for FreakUSBSerial {
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

    fn tx_handler(&mut self) {
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

    fn rx_handler(&mut self) {
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

    fn setup_wanted(&mut self, h: &SetupHeader) -> bool {
        // I believe that the requests should come for INTF_ACM_INTR, but we
        // also support the DATA interface because I can't see it in the spec
        // and I have no confidence people will agree with me.
        h.index == INTF_ACM_INTR as u16 || h.index == INTF_ACM_DATA as u16
    }

    fn setup_handler(&mut self, setup: &SetupHeader) -> SetupResult {
        match (setup.request_type, setup.request) {
            // Set Line Coding.
            (0x21, 0x20) => SetupResult::rx_data_cb(7, set_line_coding),
            (0xa1, 0x21) => get_line_coding(),

            // We could flush buffers on a transition from line-down to line-up.
            (0x21, 0x22) => set_control_line_state(setup.value_lo),
            _ => SetupResult::error(),
        }
    }

    fn initialize() {
        bd_serial().rx_set::<64>(BULK_RX_BUF);

        let ser = chep_ser().read();
        chep_ser().write(|w|w.serial().init(&ser).rx_valid(&ser).tx_nak(&ser));
    }
}

impl FreakUSBSerial {
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

    pub fn serial_tx_byte(&mut self, byte: u8) {
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

    fn send_tx_buffer(&mut self, chep: CheprR) {
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
}

fn set_control_line_state(_value: u8) -> SetupResult {
    usb_tx_interrupt();
    SetupResult::no_data()
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

#[derive_const(Default)]
pub struct FreakUSBSerialIntr;

impl EndpointPair for FreakUSBSerialIntr {
    /// This handles USB interrupt pipe VTTX not CPU interrupts!
    fn tx_handler(&mut self) {
        // TODO - nothing here yet!
        let chep = chep_intr().read();
        chep_intr().write(|w| w.interrupt().VTTX().clear_bit());
        intr_dbgln!("interrupt_tx_handler CHEP now {:#06x} was {:#06x}",
                    chep_intr().read().bits(), chep.bits());
    }

    fn initialize() {
        // Interrupt.
        let intr = chep_intr().read();
        chep_intr().write(|w| w.interrupt().init(&intr).tx_nak(&intr));
    }
}
