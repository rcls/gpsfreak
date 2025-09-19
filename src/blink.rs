#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![deny(warnings)]
#![allow(internal_features)]
#![allow(unpredictable_function_pointer_comparisons)]
#![feature(const_cmp)]
#![feature(const_default)]
#![feature(const_index)]
#![feature(const_trait_impl)]
#![feature(derive_const)]
#![feature(format_args_nl)]
#![feature(link_llvm_intrinsics)]

mod cpu;
mod gps_uart;
mod uart_debug;
mod usb;
mod usb_strings;
#[allow(dead_code)]
mod usb_types;
mod vcell;

pub fn main() -> ! {
    let rcc = unsafe {&*stm32h503::RCC::ptr()};
    rcc.AHB2ENR.modify(|_,w| w.GPIOAEN().set_bit().GPIOBEN().set_bit());

    cpu::init();

    uart_debug::init();

    usb::init();
    if false {gps_uart::init();}

    // FIXME - this races with interrupts using debug!
    dbgln!("Entering main loop!");

    if false {gps_reset();}

    loop {
        cpu::WFE();
    }
}

pub fn gps_reset() {
    // PB1 is GPS reset.  Pulse it low briefly.
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    // Enable the pullup and open drain.
    gpiob.PUPDR.modify(|_,w| w.PUPD1().B_0x1()); // Pull up
    gpiob.OTYPER.modify(|_,w| w.OT1().B_0x1());  // Open drain
    gpiob.BSRR.write(|w| w.BS1().set_bit());     // Drive high.
    gpiob.MODER.modify(|_,w| w.MODE1().B_0x1()); // Enable output.

    // Pulse it.
    gpiob.BSRR.write(|w| w.BR1().set_bit());
    for _ in 0..320000 {
        cpu::nothing();
    }
    gpiob.BSRR.write(|w| w.BS1().set_bit());
}

#[used]
#[unsafe(link_section = ".vectors")]
pub static VECTORS: cpu::VectorTable = *cpu::VectorTable::default()
    .debug().usb().gps_uart();
