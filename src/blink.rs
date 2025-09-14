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
mod usb;
mod usb_strings;
#[allow(dead_code)]
mod usb_types;
mod vcell;

mod uart_debug;

pub fn main() -> ! {
    let rcc = unsafe {&*stm32h503::RCC::ptr()};
    rcc.AHB2ENR.modify(|_,w| w.GPIOAEN().set_bit().GPIOBEN().set_bit());
    rcc.APB1LENR.modify(|_,w| w.USART3EN().set_bit());

    // Done in usb.rs...
    // rcc.APB2ENR.modify(|_,w| w.USBFSEN().set_bit());

    cpu::init();

    uart_debug::init();

    usb::init();

    dbgln!("Entering main loop!");
    let mut last_cntr = 0;
    let mut last_istr = 0;
    let mut last_chp0 = 0;

    loop {
        if true {continue}
        let usb = unsafe {&*stm32h503::USB::ptr()};
        let cntr = usb.CNTR.read().bits();
        let istr = usb.ISTR.read().bits();
        let chp0 = usb.CHEPR[0].read().bits();
        if cntr != last_cntr || istr != last_istr || chp0 != last_chp0 {
            dbgln!("CNTR={cntr:#010x} ISTR={istr:#010x} CHP0={chp0:#010x}");
            last_cntr = cntr;
            last_istr = istr;
            last_chp0 = chp0;
        }
    }
}


#[used]
#[unsafe(link_section = ".vectors")]
pub static VECTORS: cpu::VectorTable = *cpu::VectorTable::default()
    .debug().usb();
