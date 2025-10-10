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
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};

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
    unsafe {scb.shpr[11].write(cpu::interrupt::PRIO_APP)};
    unsafe {scb.cpacr.write(0x00f00000)};

    // Systick counts at 20MHz.
    unsafe {
        let syst = &*cortex_m::peripheral::SYST::PTR;
        syst.rvr.write(0xffffff);
        syst.cvr.write(0xffffff);
        syst.csr.write(3);
    }

    // LMK05318b PDN pin is on PA4.
    gpioa.BSRR.write(|w| w.BS4().set_bit());
    gpioa.MODER.modify(|_,w| w.MODE4().B_0x1());

    // PB1 is the GPS reset.
    gpiob.BSRR.write(|w| w.BS1().set_bit());
    gpiob.MODER.modify(|_,w| w.MODE1().B_0x1());

    // Setup the oscillator.
    if false {
        let r = clock_setup();
        dbgln!("Clock setup result = {r:?}");
    }

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

fn systick_handler() {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    const NEXT: [u8; 8] = [0xe, 4, 8, 0xa, 0, 0xc, 2, 0x6];
    let led = NEXT[gpioa.ODR().read().bits() as usize >> 1 & 7];
    gpioa.BSRR.write(|w| w.bits(0xe0000 + led as u32));

    let mut temp: i16 = 0;
    if let Ok(_) = i2c::read_reg(TMP117, 0, &mut temp).wait() {
        let temp = i16::from_be(temp);
        dbgln!("Temp {temp:#x} {}", temp as i32 * 100 / 128);
    }
    else {
        dbgln!("Temp read fail");
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
