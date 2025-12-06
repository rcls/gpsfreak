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

pub mod control;
mod descriptor;
pub mod hardware;
mod strings;
pub mod types;

use hardware::*;
use types::*;

use crate::cpu::{interrupt, nothing};

use stm32h503::Interrupt::USB_FS as INTERRUPT;

macro_rules!ctrl_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!usb_dbgln  {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!fast_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}

pub(crate) use {ctrl_dbgln, usb_dbgln};

pub trait EndPointPair: const Default {
    fn rx_handler(&mut self) {}
    fn tx_handler(&mut self) {}
    fn start_of_frame(&mut self) {}
    fn usb_initialize(&mut self) {} // Control endpoints only?
}

pub trait EightEndPoints: const Default {
    type EP0: EndPointPair = DummyEndPoint;
    type EP1: EndPointPair = DummyEndPoint;
    type EP2: EndPointPair = DummyEndPoint;
    type EP3: EndPointPair = DummyEndPoint;
    type EP4: EndPointPair = DummyEndPoint;
    type EP5: EndPointPair = DummyEndPoint;
    type EP6: EndPointPair = DummyEndPoint;
    type EP7: EndPointPair = DummyEndPoint;
}

#[derive_const(Default)]
pub struct DummyEndPoint;
impl EndPointPair for DummyEndPoint {}

#[allow(non_camel_case_types)]
pub struct USB_State<EPS: EightEndPoints> {
    pub ep0: EPS::EP0,
    pub ep1: EPS::EP1,
    pub ep2: EPS::EP2,
    pub ep3: EPS::EP3,
    pub ep4: EPS::EP4,
    pub ep5: EPS::EP5,
    pub ep6: EPS::EP6,
    pub ep7: EPS::EP7,
}

impl<EPS: EightEndPoints> const Default for USB_State<EPS> {
    fn default() -> Self {Self{
        ep0: EPS::EP0::default(),
        ep1: EPS::EP1::default(),
        ep2: EPS::EP2::default(),
        ep3: EPS::EP3::default(),
        ep4: EPS::EP4::default(),
        ep5: EPS::EP5::default(),
        ep6: EPS::EP6::default(),
        ep7: EPS::EP7::default(),
    }}
}

unsafe impl<EPS: EightEndPoints> Sync for USB_State<EPS> {}

fn usb_isr() {
    unsafe{super::USB_STATE.as_mut()}.isr();
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

impl<EPS: EightEndPoints> USB_State<EPS> {
    pub fn init(&mut self) {
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

        self.usb_initialize();

        interrupt::enable_priority(INTERRUPT, interrupt::PRIO_COMMS);
    }

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
            self.usb_initialize();
        }

        while istr.CTR().bit() {
            if istr.DIR().bit() {
                errata_delay();
            }
            match istr.bits() & 31 {
                0  => self.ep0.tx_handler(),
                1  => self.ep1.tx_handler(),
                2  => self.ep2.tx_handler(),
                3  => self.ep3.tx_handler(),
                4  => self.ep4.tx_handler(),
                5  => self.ep5.tx_handler(),
                6  => self.ep6.tx_handler(),
                7  => self.ep7.tx_handler(),
                16 => self.ep0.rx_handler(),
                17 => self.ep1.rx_handler(),
                18 => self.ep2.rx_handler(),
                19 => self.ep3.rx_handler(),
                20 => self.ep4.rx_handler(),
                21 => self.ep5.rx_handler(),
                22 => self.ep6.rx_handler(),
                23 => self.ep7.rx_handler(),
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
        self.ep0.start_of_frame();
        self.ep1.start_of_frame();
        self.ep2.start_of_frame();
        self.ep3.start_of_frame();
        self.ep4.start_of_frame();
        self.ep5.start_of_frame();
        self.ep6.start_of_frame();
        self.ep7.start_of_frame();
    }

    fn usb_initialize(&mut self) {
        let usb = unsafe {&*stm32h503::USB::ptr()};
        usb_dbgln!("USB initialize...");

        self.ep0.usb_initialize();

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
}

/// Initialize all the RX BD entries, except for the control ones.
fn clear_buffer_descs() {
    bd_serial().rx_set::<64>(BULK_RX_BUF);
    bd_main()  .rx_set::<64>(MAIN_RX_BUF);
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
        self.isr(INTERRUPT, usb_isr)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == usb_isr);
}
