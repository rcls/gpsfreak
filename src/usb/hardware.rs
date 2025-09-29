pub use stm32h503::usb::chepr::{R as CheprR, W as CheprW};

use crate::{cpu::barrier, vcell::VCell};

pub trait CheprWriter {
    fn control  (&mut self) -> &mut Self {self.endpoint(0, 1)}
    fn serial   (&mut self) -> &mut Self {self.endpoint(1, 0)}
    fn interrupt(&mut self) -> &mut Self {self.endpoint(2, 3)}
    fn main     (&mut self) -> &mut Self {self.endpoint(3, 0)}

    fn init(&mut self, c: &CheprR) -> &mut Self {
        self.stat_rx(c, 0).stat_tx(c, 0).dtogrx(c, false).dtogtx(c, false)
    }

    fn rx_valid(&mut self, c: &CheprR) -> &mut Self {self.stat_rx(c, 3)}
    fn tx_valid(&mut self, c: &CheprR) -> &mut Self {self.stat_tx(c, 3)}
    fn tx_nak  (&mut self, c: &CheprR) -> &mut Self {self.stat_tx(c, 2)}

    fn endpoint(&mut self, ea: u8, utype: u8) -> &mut Self;

    fn stat_rx(&mut self, c: &CheprR, s: u8) -> &mut Self;
    fn stat_tx(&mut self, c: &CheprR, s: u8) -> &mut Self;

    fn dtogrx(&mut self, c: &CheprR, t: bool) -> &mut Self;
    fn dtogtx(&mut self, c: &CheprR, t: bool) -> &mut Self;
}

impl CheprWriter for CheprW {
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
    fn endpoint(&mut self, ea: u8, utype: u8) -> &mut Self {
        self.UTYPE().bits(utype).EA().bits(ea).VTTX().set_bit().VTRX().set_bit()
    }
}

pub trait CheprReader {
    fn rx_disabled(&self) -> bool {self.stat_rx() == 0}
    fn rx_nakking (&self) -> bool {self.stat_rx() == 2}

    fn tx_nakking (&self) -> bool {self.stat_tx() == 2}
    fn tx_active  (&self) -> bool {self.stat_tx() == 3}

    fn stat_rx(&self) -> u8;
    fn stat_tx(&self) -> u8;
}

impl CheprReader for CheprR {
    fn stat_rx(&self) -> u8 {self.STATRX().bits()}
    fn stat_tx(&self) -> u8 {self.STATTX().bits()}
}

const USB_SRAM_BASE: usize = 0x4001_6400;
pub const CTRL_RX_OFFSET: usize = 0xc0;
pub const CTRL_TX_OFFSET: usize = 0x80;
pub const BULK_RX_OFFSET: usize = 0x100;
pub const BULK_TX_OFFSET: usize = 0x180;
pub const INTR_TX_OFFSET: usize = 0x40;
pub const MAIN_RX_OFFSET: usize = 0x200;
pub const MAIN_TX_OFFSET: usize = 0x240;

pub const CTRL_RX_BUF: *mut u8 = (USB_SRAM_BASE + CTRL_RX_OFFSET) as *mut u8;
pub const CTRL_TX_BUF: *mut u8 = (USB_SRAM_BASE + CTRL_TX_OFFSET) as *mut u8;
pub const BULK_RX_BUF: *mut u8 = (USB_SRAM_BASE + BULK_RX_OFFSET) as *mut u8;
pub const BULK_TX_BUF: *mut u8 = (USB_SRAM_BASE + BULK_TX_OFFSET) as *mut u8;
pub const INTR_TX_BUF: *mut u8 = (USB_SRAM_BASE + INTR_TX_OFFSET) as *mut u8;
pub const MAIN_RX_BUF: *mut u8 = (USB_SRAM_BASE + MAIN_RX_OFFSET) as *mut u8;
pub const MAIN_TX_BUF: *mut u8 = (USB_SRAM_BASE + MAIN_TX_OFFSET) as *mut u8;

pub fn chep_ctrl() -> &'static stm32h503::usb::CHEPR {chep_ref(0)}
pub fn chep_ser () -> &'static stm32h503::usb::CHEPR {chep_ref(1)}
pub fn chep_intr() -> &'static stm32h503::usb::CHEPR {chep_ref(2)}
pub fn chep_main() -> &'static stm32h503::usb::CHEPR {chep_ref(3)}

pub struct BD {
    pub tx: VCell<u32>,
    pub rx: VCell<u32>,
}

impl BD {
    pub fn tx_set(&self, ptr: *const u8, len: usize) {
        let offset = ptr as usize - USB_SRAM_BASE;
        self.tx.write((len * 65536 + offset) as u32);
    }
    pub fn rx_set<const BLK_SIZE: usize>(&self, ptr: *mut u8) {
        self.rx.write(chep_block::<BLK_SIZE>(ptr as usize - USB_SRAM_BASE))
    }
}

fn chep_ref(n: usize) -> &'static stm32h503::usb::CHEPR {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    &usb.CHEPR[n]
}

fn chep_bd() -> &'static [BD; 8] {
    unsafe {&*(USB_SRAM_BASE as *const _)}
}

pub fn bd_control()   -> &'static BD {&chep_bd()[0]}
pub fn bd_serial()    -> &'static BD {&chep_bd()[1]}
pub fn bd_interrupt() -> &'static BD {&chep_bd()[2]}
pub fn bd_main()      -> &'static BD {&chep_bd()[3]}

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
pub fn chep_bd_len(bd: u32) -> usize {
    (bd >> 16 & 0x3ff) as usize
}

/// Return pointer to the buffer for a Buffer Descriptor.
pub fn chep_bd_ptr(bd: u32) -> *const u8 {
    (USB_SRAM_BASE + (bd as usize & 0xffff)) as *const u8
}

/// The USB SRAM is finicky about 32bit accesses, so we need to jump through
/// hoops to copy into it.  We assume that we are passed an aligned destination.
pub unsafe fn copy_by_dest32(s: *const u8, d: *mut u8, len: usize) {
    barrier();
    let mut s = s as *const u32;
    let mut d = d as *mut   u32;
    for _ in (0 .. len).step_by(4) {
        // We potentially overrun the source buffer by up to 3 bytes, which
        // should be harmless, as long as the buffer is not right at the end
        // of flash or RAM.
        unsafe {*d = core::ptr::read_unaligned(s)};
        d = d.wrapping_add(1);
        s = s.wrapping_add(1);
    }
    barrier();
}
