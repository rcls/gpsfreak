#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![deny(warnings)]
#![allow(internal_features)]
// #![allow(unpredictable_function_pointer_comparisons)]
#![feature(const_default)]
#![feature(const_trait_impl)]
#![feature(derive_const)]
// #![feature(format_args_nl)]
#![feature(link_llvm_intrinsics)]

mod cpu;
#[allow(dead_code)]
mod usb;
mod vcell;

pub fn main() -> ! {
    cpu::init();

    usb::dummy();

    loop {}
}


#[cfg(target_os = "none")]
#[panic_handler]
fn ph(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
