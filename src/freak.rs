#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![deny(warnings)]
#![allow(internal_features)]
#![allow(unpredictable_function_pointer_comparisons)]
#![feature(const_clone, const_cmp, const_default,const_index, const_trait_impl)]
#![feature(derive_const)]
#![feature(format_args_nl)]
#![feature(link_llvm_intrinsics)]

#![feature(const_convert)]
#![feature(const_ops)]

mod command;
mod cpu;
mod crc;
mod dma;
mod gps_uart;
mod i2c;
#[macro_use]
mod uart_debug;
mod usb;
#[macro_use]
mod utils;
mod vcell;

// I²C address of the LMK05318(B).
// const LMK05318: u8 = 0xc8;

// I²C address of the TMP117.  ADD0 on the TMP117 connects to 3V3.
// const TMP117: u8 = 0x92;

pub fn main() -> ! {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};

    rcc.AHB2ENR.modify(|_,w| w.GPIOAEN().set_bit().GPIOBEN().set_bit());

    cpu::init();

    uart_debug::init();

    crc::init();

    i2c::init();
    usb::init();
    if true {gps_uart::init();}

    // FIXME - this races with interrupts using debug!
    dbgln!("Debug is up!");

    cpu::maybe_enter_dfu();

    if false {gps_uart::gps_reset();}

    // Enable FPU.
    // unsafe {scb.cpacr.write(0x00f00000)};

    // LMK05318b PDN pin is on PA4.
    gpioa.BSRR.write(|w| w.BS4().set_bit());
    gpioa.MODER.modify(|_,w| w.MODE4().B_0x1());

    // PB1 is the GPS reset.
    gpiob.BSRR.write(|w| w.BS1().set_bit());
    gpiob.MODER.modify(|_,w| w.MODE1().B_0x1());

    // Blue/red/green are PA1,2,3.
    gpioa.BSRR.write(|w| w.bits(0xe));
    gpioa.MODER.modify(|_,w| w.MODE1().B_0x1().MODE2().B_0x1().MODE3().B_0x1());

    // EN_REF2 = PB5, deassert high
    // EN_OUT4 = PB4, assert low
    // nEN_OUT3 = PB8, assert low
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    gpiob.BSRR.write(|w| w.BS4().set_bit().BR5().set_bit().BR8().set_bit());
    gpiob.MODER.modify(|_,w| w.MODE4().B_0x1().MODE5().B_0x1().MODE8().B_0x1());

    loop {
        cpu::WFE();
    }
}

#[used]
#[unsafe(link_section = ".vectors")]
pub static VECTORS: cpu::VectorTable = {
    let mut vtor = cpu::VectorTable::default();
    *vtor.debug().usb().gps_uart().i2c()
};
