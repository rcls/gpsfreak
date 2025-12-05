
use crate::cpu::interrupt;
use crate::dma::{Channel, DMA_Channel};
use i2c_core::{CONTEXT, F_DMA_RX, F_DMA_TX};

pub use i2c_core::{read, write, write_read};

#[path = "../stm-common/i2c_core.rs"]
mod i2c_core;

/// Interrupt priority for the I2C and its DMA interrupt handlers.  Users of
/// this code should run at no higher than that priority.
use interrupt::PRIO_COMMS as PRIORITY;

pub type I2C = stm32h503::I2C1;

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
        i2c_core::write_reg(0, 0, &0i16).defer();
        i2c_core::read_reg(0, 0, &mut 0i16).defer();
    }

    use interrupt::*;
    use stm32h503::Interrupt::*;
    enable_priority(I2C1_EV, PRIORITY);
    enable_priority(I2C1_ER, PRIORITY);
    enable_priority(GPDMA1_CH1, PRIORITY);
    enable_priority(GPDMA1_CH2, PRIORITY);
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

impl crate::cpu::VectorTable {
    pub const fn i2c(&mut self) -> &mut Self {
        use stm32h503::Interrupt::*;
        self.isr(GPDMA1_CH1, dma_rx_isr)
            .isr(GPDMA1_CH2, dma_tx_isr)
            .isr(I2C1_EV, i2c_core::i2c_isr)
            .isr(I2C1_ER, i2c_core::i2c_isr)
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
