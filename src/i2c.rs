
use core::marker::PhantomData;

use crate::dma::{Channel, DMA_Channel, Flat};
use crate::vcell::{UCell, VCell};
use crate::cpu::{barrier, interrupt};

pub type I2C = stm32h503::I2C1;

pub type Result = core::result::Result<(), ()>;

/// I2C receive channel on GPDMA1.
const RX_CHANNEL: usize = 1;

/// I2C transmit channel on GPDMA1.
const TX_CHANNEL: usize = 2;

/// Request selection for GPDMA1 Ch1
const RX_MUXIN: u8 = 12;
/// Request selection for GPDMA1 Ch2
const TX_MUXIN: u8 = 13;

fn rx_channel() -> &'static Channel {crate::dma::dma().C(RX_CHANNEL)}
fn tx_channel() -> &'static Channel {crate::dma::dma().C(TX_CHANNEL)}

macro_rules!dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

#[derive_const(Default)]
pub struct I2cContext {
    outstanding: VCell<u8>,
    error: VCell<u8>,
    pending_len: VCell<usize>,
}

#[must_use]
pub struct Wait<'a>(PhantomData<&'a()>);

pub static CONTEXT: UCell<I2cContext> = UCell::default();

const F_I2C: u8 = 1;
const F_DMA_RX: u8 = 2;
const F_DMA_TX: u8 = 4;

pub fn init() {
    let i2c   = unsafe {&*I2C::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC::ptr()};

    // I2C1_SCL is on 29 PB6
    // I2C1_SDA is on 30 PB7

    // Drive the lines up briefly.  FIXME pullups.
    gpiob.BSRR.write(|w| w.BS6().set_bit().BS7().set_bit());
    gpiob.MODER.modify(|_, w| w.MODE6().B_0x1().MODE7().B_0x1());
    gpiob.PUPDR.modify(|_, w| w.PUPD6().B_0x1().PUPD7().B_0x1());

    // Configure the I2C1 clock input to be CSI.
    rcc.CCIPR4().modify(|_,w| w.I2C1SEL().B_0x3());

    // Enable the clocks.
    rcc.AHB1ENR.modify(|_,w| w.GPDMA1EN().set_bit());
    rcc.APB1LENR.modify(|_,w| w.I2C1EN().set_bit());

    // This is ≈ 400kHz from 4MHz.
    i2c.TIMINGR.write(
        |w|w.PRESC().bits(0)
            .SCLL().bits(3).SCLH().bits(5)
            .SDADEL().bits(1).SCLDEL().bits(3));

    // Configure the lines for use.
    gpiob.AFRL.modify(|_,w| w.AFSEL6().B_0x4().AFSEL7().B_0x4());
    gpiob.OTYPER.modify(|_,w| w.OT6().set_bit().OT7().set_bit());
    gpiob.MODER.modify(|_, w| w.MODE6().B_0x2().MODE7().B_0x2());

    // Enable everything.
    i2c.CR1.write(
        |w|w.TXDMAEN().set_bit().RXDMAEN().set_bit().PE().set_bit()
            .NACKIE().set_bit().ERRIE().set_bit().TCIE().set_bit()
            .STOPIE().set_bit());

    rx_channel().read_from(i2c.RXDR.as_ptr() as *const u8, RX_MUXIN);
    tx_channel().writes_to(i2c.TXDR.as_ptr() as *mut   u8, TX_MUXIN);

    if false {
        write_reg(0, 0, &0i16).defer();
        read_reg(0, 0, &mut 0i16).defer();
    }

    use interrupt::*;
    use stm32h503::Interrupt::*;
    enable_priority(I2C1_EV, PRIO_I2C);
    enable_priority(I2C1_ER, PRIO_I2C);
    enable_priority(GPDMA1_CH1, PRIO_I2C);
    enable_priority(GPDMA1_CH2, PRIO_I2C);
}

pub fn i2c_isr() {
    let i2c = unsafe {&*I2C::ptr()};
    let context = unsafe {CONTEXT.as_mut()};

    let status = i2c.ISR.read();
    dbgln!("I2C ISR {:#x}", status.bits());
    let todo = *context.pending_len.as_mut();
    *context.pending_len.as_mut() = 0;

    if todo != 0 && status.TC().bit() {
        // Assume write -> read transition.
        dbgln!("I2C now read {todo} bytes [{:#x}]", status.bits());
        let cr2 = i2c.CR2.read();
        i2c.CR2.write(
            |w|w.NBYTES().bits(todo as u8).START().set_bit()
                .AUTOEND().set_bit().RD_WRN().set_bit()
                .SADD().bits(cr2.SADD().bits()));
    }
    else if status.STOPF().bit() {
        // FIXME - if we see a stop when waiting for the above, we'll hang.
        dbgln!("I2C STOPF");
        i2c.ICR.write(|w| w.STOPCF().set_bit());
        *context.outstanding.as_mut() &= !F_I2C;
    }
    else if status.ARLO().bit() || status.BERR().bit() || status.NACKF().bit() {
        dbgln!("I2C Error");
        i2c.ICR.write(
            |w| w.ARLOCF().set_bit().BERRCF().set_bit().NACKCF().set_bit());
        *context.outstanding.as_mut() = 0;
        *context.error.as_mut() = 1;
    }
    else {
        panic!("Unexpected I2C ISR {:#x} {:#x}",
               status.bits(), i2c.CR2.read().bits());
    }

    // Stop the ISR from prematurely retriggering.  Otherwise we may return
    // from the ISR before the update has propagated through the I2C subsystem,
    // leaving the interrupt line high.
    i2c.ISR.read();

    dbgln!("I2C ISR done, {}", context.outstanding.read());
}

fn dma_rx_isr() {
    dbgln!("I2C DMA RX ISR");
    let ch = rx_channel();
    let sr = ch.SR().read();
    ch.FCR().write(|w| w.bits(sr.bits())); // Clear flags.
    if sr.TCF().bit() {
        unsafe {*CONTEXT.as_mut().outstanding.as_mut() &= !F_DMA_RX};
    }
}

fn dma_tx_isr() {
    dbgln!("I2C DMA TX ISR");
    let ch = tx_channel();
    let sr = ch.SR().read();
    ch.FCR().write(|w| w.bits(sr.bits())); // Clear flags.
    if sr.TCF().bit() {
        unsafe {*CONTEXT.as_mut().outstanding.as_mut() &= !F_DMA_TX};
    }
}

impl I2cContext {
    fn read_reg_start(&self, addr: u8, reg: u8, data: usize, len: usize) {
        // Should only be called while I2C idle...
        let i2c = unsafe {&*I2C::ptr()};
        self.arm(F_I2C | F_DMA_RX);
        self.pending_len.write(len);

        // Synchronous I2C start for the reg ptr write.
        // No DMA write is active so the dma req. hopefully just gets ignored.
        i2c.CR2.write(
            |w|w.START().set_bit().NBYTES().bits(1).SADD().bits(addr as u16));
        i2c.TXDR.write(|w| w.bits(reg as u32));

        rx_channel().read(data, len);
    }
    #[inline(never)]
    fn read_start(&self, addr: u8, data: usize, len: usize) {
        let i2c = unsafe {&*I2C::ptr()};

        interrupt::disable_all();
        rx_channel().read(data, len);
        self.arm(F_I2C | F_DMA_RX);
        i2c.CR2.write(
            |w|w.START().set_bit().AUTOEND().bit(true).SADD().bits(addr as u16)
                .RD_WRN().set_bit().NBYTES().bits(len as u8));
        // Do the DMA set-up in the shadow of the address handling.  In case
        // we manage to get an I2C error before the DMA set-up is done, we have
        // interrupts disabled.
        interrupt::enable_all();
    }
    #[inline(never)]
    fn write_reg_start(&self, addr: u8, reg: u8, data: usize, len: usize) {
        let i2c = unsafe {&*I2C::ptr()};

        interrupt::disable_all();
        i2c.CR2.write(
            |w| w.START().set_bit().AUTOEND().set_bit()
                . SADD().bits(addr as u16).NBYTES().bits(len as u8 + 1));
        i2c.TXDR.write(|w| w.TXDATA().bits(reg));
        tx_channel().write(data, len);
        self.arm(F_I2C | F_DMA_TX);
        interrupt::enable_all();
    }
    #[inline(never)]
    fn write_start(&self, addr: u8, data: usize, len: usize, last: bool) {
        let i2c = unsafe {&*I2C::ptr()};

        interrupt::disable_all();
        i2c.CR2.write(
            |w|w.START().set_bit().AUTOEND().bit(last)
                . SADD().bits(addr as u16).NBYTES().bits(len as u8));
        // Do the DMA set-up in the shadow of the address handling.  In case
        // we manage to get an I2C error before the DMA set-up is done, we have
        // interrupts disabled.
        tx_channel().write(data, len);
        self.arm(F_I2C | F_DMA_TX);
        interrupt::enable_all();
    }
    #[inline(never)]
    fn write_read_start(&self, addr: u8, wdata: usize, wlen: usize,
                        rdata: usize, rlen: usize) {
        let i2c = unsafe {&*I2C::ptr()};
        tx_channel().write(wdata, wlen);
        rx_channel().read (rdata, rlen);
        self.pending_len.write(rlen);
        self.arm(F_I2C | F_DMA_TX | F_DMA_RX);
        i2c.CR2.write(
            |w|w.START().set_bit().SADD().bits(addr as u16)
                .NBYTES().bits(wlen as u8));
    }
    fn arm(&self, flags: u8) {
        self.error.write(0);
        self.outstanding.write(flags);
    }
    fn done(&self) -> bool {self.outstanding.read() == 0}
    fn wait(&self) {
        while !self.done() {
            crate::cpu::WFE();
        }
        if self.error.read() != 0 {
            self.error_cleanup();
        }
        barrier();
    }
    fn error_cleanup(&self) {
        dbgln!("I2C error cleanup");
        let i2c = unsafe {&*I2C::ptr()};
        // Clean-up the DMA and reset the I2C.
        i2c.CR1.write(|w| w.PE().clear_bit());
        tx_channel().abort();
        rx_channel().abort();
        rx_channel().read_from(i2c.RXDR.as_ptr() as *const u8, RX_MUXIN);
        tx_channel().writes_to(i2c.TXDR.as_ptr() as *mut   u8, TX_MUXIN);
        i2c.CR1.write(
            |w|w.TXDMAEN().set_bit().RXDMAEN().set_bit().PE().set_bit()
                .NACKIE().set_bit().ERRIE().set_bit().TCIE().set_bit()
                .STOPIE().set_bit());
    }
}

impl Wait<'_> {
    pub fn new() -> Self {Wait(PhantomData)}
    pub fn defer(self) {core::mem::forget(self);}
    pub fn wait(self) -> Result {
        CONTEXT.wait();
        if CONTEXT.error.read() == 0 {Ok(())} else {Err(())}
    }
}

impl Drop for Wait<'_> {
    fn drop(&mut self) {let _ = CONTEXT.wait();}
}

pub fn waiter<'a, T: ?Sized>(_: &'a T) ->Wait<'a> {
    Wait::new()
}

pub fn write<'a, T: Flat + ?Sized>(addr: u8, data: &'a T) -> Wait<'a> {
    CONTEXT.write_start(addr & !1, data.addr(), size_of_val(data), true);
    waiter(data)
}

pub fn write_reg<'a, T: Flat + ?Sized>(addr: u8, reg: u8, data: &'a T) -> Wait<'a> {
    CONTEXT.write_reg_start(addr & !1, reg, data.addr(), size_of_val(data));
    waiter(data)
}

pub fn read<'a, T: Flat + ?Sized>(addr: u8, data: &'a mut T) -> Wait<'a> {
    CONTEXT.read_start(addr | 1, data.addr(), size_of_val(data));
    Wait::new()
}

pub fn read_reg<'a, T: Flat + ?Sized>(addr: u8, reg: u8, data: &'a mut T) -> Wait<'a> {
    CONTEXT.read_reg_start(addr | 1, reg, data.addr(), size_of_val(data));
    Wait::new()
}

pub fn write_read<'a, T: Flat + ?Sized, U: Flat + ?Sized>(
    addr: u8, wdata: &'a T, rdata: &'a mut U) -> Wait<'a> {
    CONTEXT.write_read_start(addr, wdata.addr(), size_of_val(wdata),
                             rdata.addr(), size_of_val(rdata));
    Wait::new()
}


impl crate::cpu::VectorTable {
    pub const fn i2c(&mut self) -> &mut Self {
        use stm32h503::Interrupt::*;
        self.isr(GPDMA1_CH1, dma_rx_isr)
            .isr(GPDMA1_CH2, dma_tx_isr)
            .isr(I2C1_EV, i2c_isr)
            .isr(I2C1_ER, i2c_isr)
    }
}

#[test]
fn check_isr() {
    use stm32h503::Interrupt::*;
    assert_eq!(RX_CHANNEL, 1);
    assert_eq!(TX_CHANNEL, 2);
    assert!(crate::VECTORS.isr[GPDMA1_CH1 as usize] == dma_rx_isr);
    assert!(crate::VECTORS.isr[GPDMA1_CH2 as usize] == dma_tx_isr);
}
