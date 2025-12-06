//! USB for GPS ref.
//! Endpoints:
//! 0 : Control, as always.
//!   OUT: 64 bytes at 0x80 offset.
//!   IN : 64 bytes at 0xc0 offset.  TODO - do we use both?
//!   CHEP 0
//! 01, 81: CDC ACM data transfer, bulk
//!   OUT (RX): 2 × 64 bytes at 0x100 offset.
//!     DTOGRX==0 -> RX into RXTX buf. 1×2 + 1 = 3.
//!     DTOGRX==1 -> RX into TXRX buf. 1×2 + 0 = 2.
//!   IN  (TX): 8 x 64 bytes at 0x200 offset.
//!     DTOGTX==0 -> TX from TXRX buf. 2×2 + 0 = 4.
//!     DTOGTX==1 -> TX from RXTX buf. 2×2 + 1 = 5.
//! 82: CDC ACM interrupt IN (to host).
//!   64 bytes at 0x40 offset.
//!   CHEP 2

use crate::cpu::interrupt;

use stm_common::usb;
use stm_common::vcell::UCell;

use usb::hardware::{BD, USB_SRAM_BASE, chep_bd, chep_ref};
use usb::types::{SetupHeader, SetupResult};

use stm32h503::Interrupt::USB_FS as INTERRUPT;

pub mod command;
mod descriptors;
pub mod serial;

#[derive_const(Default)]
struct FreakUSB;

#[derive_const(Default)]
struct TriggerDFU;

static USB_STATE: UCell<usb::USB_State<FreakUSB>>
    = Default::default();

impl usb::USBTypes for FreakUSB {
    fn get_device_descriptor(&mut self) -> SetupResult {
        SetupResult::tx_data(&descriptors::DEVICE_DESC)
    }
    fn get_config_descriptor(&mut self, _: &SetupHeader) -> SetupResult {
        // Always return CONFIG0 ....
        SetupResult::tx_data(&descriptors::CONFIG0_DESC)
    }
    fn get_string_descriptor(&mut self, idx: u8) -> SetupResult {
        descriptors::get_string(idx)
    }

    type EP1 = serial::FreakUSBSerial;
    type EP2 = serial::FreakUSBSerialIntr;
    type EP3 = command::CommandUSB;
    type EP7 = TriggerDFU; // Not a real end-point, just a setup handler.

    const CPU_FREQ: u32 = crate::cpu::CPU_FREQ;
}

impl usb::EndpointPair for TriggerDFU {
    fn setup_wanted(&mut self, setup: &SetupHeader) -> bool {
        setup.index == descriptors::INTF_DFU as u16
    }
    fn setup_handler(&mut self, setup: &SetupHeader) -> SetupResult {
        match (setup.request_type, setup.request) {
            (0x21, 0x00) => unsafe {crate::cpu::trigger_dfu()},
            (0xa1, 0x03) => SetupResult::tx_data(&[0u8, 100, 0, 0, 0, 0]),
            _ => SetupResult::error(),
        }
    }
}

fn usb_isr() {
    if unsafe{USB_STATE.as_mut()}.isr() {
        crate::led::BLUE.pulse(true);
    }
}

pub fn init() {
    unsafe{USB_STATE.as_mut()}.init();

    interrupt::enable_priority(INTERRUPT, interrupt::PRIO_COMMS);
}

impl crate::cpu::Config {
    pub const fn usb(&mut self) -> &mut Self {
        self.isr(INTERRUPT, usb_isr)
    }
}

pub trait CheprWriter: usb::hardware::CheprWriter {
    fn serial   (&mut self) -> &mut Self {self.endpoint(1, 0)}
    fn interrupt(&mut self) -> &mut Self {self.endpoint(2, 3)}
    fn main     (&mut self) -> &mut Self {self.endpoint(3, 0)}
}

impl CheprWriter for stm32h503::usb::chepr::W {
}

pub const BULK_RX_OFFSET: usize = 0x100;
pub const BULK_TX_OFFSET: usize = 0x180;
pub const INTR_TX_OFFSET: usize = 0x40;
pub const MAIN_RX_OFFSET: usize = 0x200;
pub const MAIN_TX_OFFSET: usize = 0x240;

pub const BULK_RX_BUF: *mut u8 = (USB_SRAM_BASE + BULK_RX_OFFSET) as *mut u8;
pub const BULK_TX_BUF: *mut u8 = (USB_SRAM_BASE + BULK_TX_OFFSET) as *mut u8;
pub const INTR_TX_BUF: *mut u8 = (USB_SRAM_BASE + INTR_TX_OFFSET) as *mut u8;
pub const MAIN_RX_BUF: *mut u8 = (USB_SRAM_BASE + MAIN_RX_OFFSET) as *mut u8;
pub const MAIN_TX_BUF: *mut u8 = (USB_SRAM_BASE + MAIN_TX_OFFSET) as *mut u8;

pub fn chep_ser () -> &'static stm32h503::usb::CHEPR {chep_ref(1)}
pub fn chep_intr() -> &'static stm32h503::usb::CHEPR {chep_ref(2)}
pub fn chep_main() -> &'static stm32h503::usb::CHEPR {chep_ref(3)}

pub fn bd_serial()    -> &'static BD {&chep_bd()[1]}
pub fn bd_interrupt() -> &'static BD {&chep_bd()[2]}
pub fn bd_main()      -> &'static BD {&chep_bd()[3]}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == usb_isr);
}
