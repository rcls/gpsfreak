#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![deny(warnings)]
#![allow(internal_features)]
#![allow(unpredictable_function_pointer_comparisons)]
#![feature(const_clone, const_cmp, const_default,const_index, const_trait_impl)]
#![feature(derive_const)]
#![feature(format_args_nl)]
#![feature(link_llvm_intrinsics)]
// #![feature(negative_impls)]
// #![allow(incomplete_features)]
// #![feature(specialization)]
// #![feature(with_negative_coherence)]

mod command;
mod cpu;
mod dma;
mod gps_uart;
mod i2c;
#[macro_use]
mod uart_debug;
mod usb;
#[macro_use]
mod utils;
mod vcell;

#[allow(non_snake_case)]
mod clock_config;

/// I²C address of the LMK05318(B).
const LMK05318: u8 = 0xc8;

/// I²C address of the TMP117.  ADD0 on the TMP117 connects to 3V3.
const TMP117: u8 = 0x92;

pub fn main() -> ! {
    let rcc = unsafe {&*stm32h503::RCC::ptr()};

    rcc.AHB2ENR.modify(|_,w| w.GPIOAEN().set_bit().GPIOBEN().set_bit());

    cpu::init();

    uart_debug::init();

    command::init();

    i2c::init();
    usb::init();
    if true {gps_uart::init();}

    // FIXME - this races with interrupts using debug!
    dbgln!("Debug is up!");

    cpu::maybe_enter_dfu();

    if false {gps_uart::gps_reset();}

    // Set the SYSTICK handler priority.
    let scb = unsafe {&*cortex_m::peripheral::SCB::PTR};
    // In the ARM docs, this is the high byte of SHPR3 (0xE000ED20), so byte
    // address 0xE000ED23.
    link_assert!(&scb.shpr[11] as *const _ as usize == 0xe000ed23);
    unsafe {scb.shpr[11].write(0xffu8)};
    unsafe {scb.cpacr.write(0x00f00000)};

    // Systick counts at 20MHz.
    unsafe {
        let syst = &*cortex_m::peripheral::SYST::PTR;
        syst.rvr.write(0xffffff);
        syst.cvr.write(0xffffff);
        syst.csr.write(3);
    }

    // Setup the oscillator.
    let r = clock_setup();
    dbgln!("Clock setup result = {r:?}");

    loop {
        cpu::WFE();
    }
}

fn systick_handler() {
    let mut temp: i16 = 0;
    if let Ok(_) = i2c::read_reg(TMP117, 0, &mut temp).wait() {
        let temp = i16::from_be(temp);
        dbgln!("Temp {temp:#x} {}", temp as i32 * 100 / 128);
    }
}

fn clock_setup() -> i2c::Result {
    for block in clock_config::REG_BLOCKS {
        i2c::write(LMK05318, block).wait()?;
    }
    // Read R12...
    i2c::write(LMK05318, &[0u8, 12]).wait()?;
    let mut r12 = 0u8;
    i2c::read(LMK05318, &mut r12).wait()?;
    // Write back with PLL cascade and reset.
    i2c::write(LMK05318, &[0u8, 12, r12 | 0x12]).wait()?;
    // Write back with PLL cascade.
    i2c::write(LMK05318, &[0u8, 12, r12 | 0x02]).wait()?;

    Ok(())
}

#[used]
#[unsafe(link_section = ".vectors")]
pub static VECTORS: cpu::VectorTable = {
    let mut vtor = cpu::VectorTable::default();
    vtor.systick = systick_handler;
    *vtor.debug().usb().gps_uart().i2c()
};
