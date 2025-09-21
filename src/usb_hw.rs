use stm32h503::usb::chepr::{R as CheprR, W as CheprW};

use crate::vcell::VCell;

pub trait Chepr {
    fn control(&mut self) -> &mut Self {self.endpoint(0, 1, false)}
    // The serial data rx & tx are double-buffered CHEPs.
    fn serial(&mut self) -> &mut Self {self.endpoint(1, 0, true)}
    fn interrupt(&mut self) -> &mut Self {self.endpoint(2, 3, false)}

    fn init(&mut self, c: &CheprR) -> &mut Self {
        self.stat_rx(c, 0).stat_tx(c, 0).dtogrx(c, false).dtogtx(c, false)
    }

    fn rx_valid(&mut self, c: &CheprR) -> &mut Self {self.stat_rx(c, 3)}
    fn tx_valid(&mut self, c: &CheprR) -> &mut Self {self.stat_tx(c, 3)}

    fn endpoint(&mut self, ea: u8, utype: u8, epkind: bool) -> &mut Self;

    fn stat_rx(&mut self, c: &CheprR, s: u8) -> &mut Self;
    fn stat_tx(&mut self, c: &CheprR, s: u8) -> &mut Self;

    fn dtogrx(&mut self, c: &CheprR, t: bool) -> &mut Self;
    fn dtogtx(&mut self, c: &CheprR, t: bool) -> &mut Self;
}

impl Chepr for CheprW {
    fn stat_rx(&mut self, c: &CheprR, v: u8) -> &mut Self {
        self.STATRX().bits(c.STATRX().bits() ^ v)
    }
    fn stat_tx(&mut self, c: &CheprR, v: u8) -> &mut Self {
        self.STATTX().bits(c.STATTX().bits() ^ v)
    }
    fn dtogrx(&mut self, c: &CheprR, v: bool) -> &mut Self {
        self.DTOGRX().bit(c.DTOGRX().bit() ^ v)
    }
    fn dtogtx(&mut self, c: &CheprR, v: bool) -> &mut Self {
        self.DTOGTX().bit(c.DTOGTX().bit() ^ v)
    }
    fn endpoint(&mut self, ea: u8, utype: u8, epkind: bool) -> &mut Self {
        self.UTYPE().bits(utype).EPKIND().bit(epkind).EA().bits(ea)
            .VTTX().set_bit().VTRX().set_bit()
    }
}

const USB_SRAM_BASE: usize = 0x4001_6400;
pub const CTRL_RX_OFFSET: usize = 0xc0;
pub const CTRL_TX_OFFSET: usize = 0x80;
#[allow(dead_code)]
pub const BULK_RX_OFFSET: usize = 0x100;
#[allow(dead_code)]
pub const BULK_TX_OFFSET: usize = 0x200;
#[allow(dead_code)]
pub const INTR_TX_OFFSET: usize = 0x40;

pub const CTRL_RX_BUF: *const u8 = (USB_SRAM_BASE + CTRL_RX_OFFSET) as *const u8;
pub const CTRL_TX_BUF: *mut   u8 = (USB_SRAM_BASE + CTRL_TX_OFFSET) as *mut   u8;
#[allow(dead_code)]
pub const BULK_RX_BUF: *const u8 = (USB_SRAM_BASE + BULK_RX_OFFSET) as *const u8;
#[allow(dead_code)]
pub const BULK_TX_BUF: *mut   u8 = (USB_SRAM_BASE + BULK_TX_OFFSET) as *mut   u8;
#[allow(dead_code)]
pub const INTR_TX_BUF: *mut   u8 = (USB_SRAM_BASE + INTR_TX_OFFSET) as *mut   u8;

pub fn chep_ctrl() -> &'static stm32h503::usb::CHEPR {chep_ref(0)}
pub fn chep_rx  () -> &'static stm32h503::usb::CHEPR {chep_ref(1)}
pub fn chep_tx  () -> &'static stm32h503::usb::CHEPR {chep_ref(2)}
pub fn chep_intr() -> &'static stm32h503::usb::CHEPR {chep_ref(3)}

fn chep_ref(n: usize) -> &'static stm32h503::usb::CHEPR {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    &usb.CHEPR[n]
}

fn chep_bd() -> &'static [VCell<u32>; 16] {
    unsafe {&*(USB_SRAM_BASE as *const _)}
}

pub type BD = &'static VCell<u32>;
pub fn bd_control_tx() -> BD {&chep_bd()[0]}
pub fn bd_control_rx() -> BD {&chep_bd()[1]}
pub fn bd_serial_rx(toggle: bool) -> BD {&chep_bd()[3 - toggle as usize]}
pub fn bd_serial_tx(toggle: bool) -> BD {&chep_bd()[4 + toggle as usize]}
pub fn bd_interrupt() -> BD {&chep_bd()[6]}

pub fn bd_serial_rx_init(toggle: bool) {
    bd_serial_rx(toggle).write(chep_block::<64>(
        BULK_RX_OFFSET + 64 - 64 * toggle as usize));
}
pub fn bd_serial_tx_init(toggle: bool) {
    bd_serial_tx(toggle).write(BULK_TX_OFFSET as u32 + 64 * toggle as u32);
}

/// Return a Buffer Descriptor value for a RX block.
pub fn chep_block<const BLK_SIZE: usize>(offset: usize) -> u32 {
    assert!(offset + BLK_SIZE <= 2048);
    let block = if BLK_SIZE == 1023 {
        0xfc000000
    }
    else if BLK_SIZE % 32 == 0 && BLK_SIZE > 0 && BLK_SIZE <= 1024 {
        BLK_SIZE / 32 + 31 << 26
    }
    else if BLK_SIZE % 2 == 0 && BLK_SIZE < 64 {
        BLK_SIZE / 2 << 26
    }
    else {
        panic!();
    };
    (block + offset) as u32
}

/// Create a Buffer Descriptor value for TX.
pub fn chep_bd_tx(offset: usize, len: usize) -> u32 {
    offset as u32 + len as u32 * 65536
}

/// Return the byte count from a Buffer Descriptor value.
pub fn chep_bd_len(bd: u32) -> u32 {
    bd >> 16 & 0x3ff
}

/// Return pointer to the buffer for a Buffer Descriptor.
pub fn chep_bd_ptr(bd: u32) -> *const u8 {
    (USB_SRAM_BASE + (bd as usize & 0xffff)) as *const u8
}

/// Return pointer to the next write location for a (TX) Buffer Descriptor.
pub fn chep_bd_tail(bd: u32) -> *mut u32 {
    let bd = bd as usize;
    (USB_SRAM_BASE + (bd & 0xffff) + (bd >> 16 & 0x3ff)) as *mut u32
}
