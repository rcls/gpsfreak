#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![deny(warnings)]
#![allow(internal_features)]
#![allow(unpredictable_function_pointer_comparisons)]
#![feature(const_clone, const_cmp, const_default,const_index, const_trait_impl)]
#![feature(default_field_values)]
#![feature(derive_const)]
#![feature(format_args_nl)]
#![feature(link_llvm_intrinsics)]

#![feature(const_convert)]
#![feature(const_ops)]

mod command;
mod cpu;
mod crc;
mod crc32;
mod dma;
mod flash;
mod gps_uart;
mod i2c;
mod led;
mod lmk05318b;
mod provision;
#[macro_use]
mod uart_debug;
mod usb;
#[macro_use]
mod utils;
mod vcell;

pub fn main() -> ! {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};

    rcc.AHB2ENR.modify(|_,w| w.GPIOAEN().set_bit().GPIOBEN().set_bit());

    cpu::init();

    cpu::maybe_enter_dfu();

    uart_debug::init();

    crc::init();
    led::init();

    i2c::init();

    gps_uart::init();
    provision::provision();

    lmk05318b::init();

    usb::init();

    // Enable FPU.  We aren't using it yet!!!
    // unsafe {scb.cpacr.write(0x00f00000)};

    // LMK05318b PDN pin is on PA4.
    gpioa.PUPDR.write(|w| w.PUPD4().B_0x1());    // Pull-up
    gpioa.OTYPER.modify(|_,w| w.OT4().B_0x1());  // Open drain
    gpioa.BSRR.write(|w| w.BS4().set_bit());     // High
    gpioa.MODER.modify(|_,w| w.MODE4().B_0x1()); // Output

    // PB1 is the GPS reset.
    gpiob.PUPDR.modify(|_,w| w.PUPD1().B_0x1()); // Pull up
    gpiob.OTYPER.modify(|_,w| w.OT1().B_0x1());  // Open drain
    gpiob.BSRR.write(|w| w.BS1().set_bit());     // High
    gpiob.MODER.modify(|_,w| w.MODE1().B_0x1()); // Output

    // EN_REF2 = PB5, deassert high
    // EN_OUT4 = PB4, assert low
    // nEN_OUT3 = PB8, assert low
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
    *vtor.debug().gps_uart().i2c().led().lmk05318b().usb()
};
