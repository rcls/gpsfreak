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
#[allow(dead_code)]
mod usb;
mod usb_strings;
#[allow(dead_code)]
mod usb_types;
mod vcell;

mod uart_debug;

pub fn main() -> ! {
    let rcc = unsafe {&*stm32h503::RCC::ptr()};
    rcc.AHB2ENR.modify(|_,w| w.GPIOAEN().set_bit().GPIOBEN().set_bit());
    rcc.APB1LENR.modify(|_,w| w.USART2EN().set_bit());

    cpu::init();

    uart_debug::init();

    usb::init();

    loop {
        dbgln!("This is a test!")
    }
}


#[cfg(target_os = "none")]
#[panic_handler]
fn ph(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[used]
#[unsafe(link_section = ".vectors")]
pub static VECTORS: cpu::VectorTable = *cpu::VectorTable::default()
    .debug();
