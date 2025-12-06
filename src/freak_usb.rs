use crate::usb::hardware::{BD, USB_SRAM_BASE, chep_bd, chep_ref};

pub trait CheprWriter : crate::usb::hardware::CheprWriter {
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
