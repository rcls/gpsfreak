#![allow(unused)]

use crate::cpu::barrier;

pub type Dma = stm32h503::gpdma1::RegisterBlock;
pub type Channel = stm32h503::gpdma1::c::C;

pub fn dma() -> &'static Dma {unsafe {&*stm32h503::GPDMA1::ptr()}}

#[allow(non_camel_case_types)]
pub trait DMA_Channel {
    /// Write to peripheral.  DADDR should be initialized.
    fn write(&self, data: usize, len: usize);

    /// Read from peripheral. SADDR should be initialized.
    fn read(&self, data: usize, len: usize);

    /// Configure to write to a peripheral from memory.
    fn writes_to(&self, dst: *mut   u8, request: u8);
    /// Configure to read from a peripheral to memory.
    fn read_from(&self, src: *const u8, request: u8);

    /// Stop and cancel an in-process transfer.
    fn abort(&self);

    /// Is the channel busy?
    fn busy(&self) -> bool;
}

impl DMA_Channel for Channel {
    fn write(&self, data: usize, len: usize) {
        self.SAR().write(|w| w.SA().bits(data as u32));
        self.BR1.write(|w| w.BNDT().bits(len as u16));
        self.CR.write(|w| w.EN().set_bit().TCIE().set_bit());
    }
    fn read(&self, data: usize, len: usize) {
        self.DAR().write(|w| w.DA().bits(data as u32));
        self.BR1.write(|w| w.BNDT().bits(len as u16));
        self.CR.write(|w| w.EN().set_bit().TCIE().set_bit());
    }
    fn writes_to(&self, dst: *mut u8, request: u8) {
        self.DAR().write(|w| w.DA().bits(dst as u32));
        self.TR1.write(|w| w.SINC().set_bit());
        self.TR2.write(|w| w.REQSEL().bits(request));
    }
    fn read_from(&self, src: *const u8, request: u8) {
        self.SAR().write(|w| w.SA().bits(src as u32));
        self.TR1.write(|w| w.DINC().set_bit());
        self.TR2.write(|w| w.REQSEL().bits(request));
    }
    fn abort(&self) {
        if self.CR.read().EN().bit() {
            self.CR.write(|w| w.SUSP().set_bit());
            while !self.SR.read().SUSPF().bit() {}
            self.CR.write(|w| w.RESET().set_bit());
            self.FCR.write(|w| w.bits(!0));
        }
    }
    fn busy(&self) -> bool {
        self.CR.read().EN().bit()
    }
}

/// Trait Flat is used to check that we pass sane types to read/write.
pub trait Flat {
    fn addr(&self) -> usize {(self as *const Self).addr()}
}

//impl<T> !Flat for &T {}
// impl<T> !Flat for &mut T {}

impl Flat for u8 {}
impl<const N: usize, T: Flat> Flat for [T; N] {}
//impl<const N: usize> Flat for [u16; N] {}
impl Flat for i16 {}
impl Flat for u16 {}
impl Flat for [u8] {}
//impl Flat for u32 {}
//impl Flat for u64 {}
