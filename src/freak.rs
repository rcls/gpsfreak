#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![deny(warnings)]
#![allow(internal_features)]
#![allow(unpredictable_function_pointer_comparisons)]
#![feature(associated_type_defaults)]
#![feature(const_clone, const_cmp, const_convert, const_default, const_index,
           const_ops, const_trait_impl)]
#![feature(default_field_values)]
#![feature(derive_const)]
#![feature(format_args_nl)]
#![feature(link_llvm_intrinsics)]

use vcell::UCell;

mod command;
mod cpu;
mod crc;
mod crc32;
mod dma;
mod flash;
mod freak_serial;
mod gps_uart;
mod i2c;
mod led;
mod lmk05318b;
mod provision;
#[macro_use]
mod debug;
mod usb;
#[macro_use]
mod utils;
#[path = "../stm-common/vcell.rs"]
mod vcell;

#[derive_const(Default)]
pub struct FreakUSB;

impl usb::EightEndPoints for FreakUSB {
    type EP0 = usb::control::ControlState;
    type EP1 = crate::freak_serial::FreakUSBSerial;
    type EP2 = crate::freak_serial::FreakUSBSerialIntr;
}

pub static USB_STATE: UCell<usb::USB_State<FreakUSB>> = Default::default();

pub fn main() -> ! {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};

    rcc.AHB2ENR.modify(|_,w| w.GPIOAEN().set_bit().GPIOBEN().set_bit());

    cpu::init();

    cpu::maybe_enter_dfu();

    debug::init();

    crc::init();
    led::init();

    i2c::init();

    gps_uart::init();

    // Spin for ≈100ms to wait for the clock generator and GPS to start.
    for _ in 0 .. cpu::CPU_FREQ / 20 {
        cpu::nothing();
    }

    command::init(
        unsafe {str::from_utf8_unchecked(cpu::SERIAL_NUMBER.as_ref())});

    provision::provision();

    lmk05318b::init();

    unsafe {USB_STATE.as_mut()}.init();

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

    if *cpu::IS_PROTOTYPE {
        // EN_REF2 = PB5, deassert high
        // EN_OUT4 = PB4, assert low
        // nEN_OUT3 = PB8, assert low
        gpiob.BSRR.write(|w| w.BS4().set_bit().BR5().set_bit().BR8().set_bit());
        gpiob.MODER.modify(|_,w| w.MODE4().B_0x1().MODE5().B_0x1().MODE8().B_0x1());
    }

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
