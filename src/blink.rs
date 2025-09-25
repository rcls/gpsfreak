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

    usb::init();
    i2c::init();
    if true {gps_uart::init();}

    // FIXME - this races with interrupts using debug!
    dbgln!("Debug is up!");

    cpu::maybe_enter_dfu();

    if false {gps_reset();}

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
    dbgln!("Systick handler");

    let mut temp: i16 = 0;
    let _ = i2c::read_reg(TMP117, 0, &mut temp).wait();
    let temp = i16::from_be(temp);

    dbgln!("Temp {temp:#x} {}", temp as i32 * 100 / 128);

    // In EEPROM mode, the LMK05318B can support up to four different I2C
    // addresses depending on the GPIO1 pins. The 7-bit I2C address is
    // 11001xxb, where the two LSBs are determined by the GPIO1 input levels
    // sampled at device POR and the five MSBs (11001b) are initialized from
    // the EEPROM. In ROM mode, the two LSBs are fixed to 00b, while the
    //  five MSB (11001b) are initialized from the EEPROM. The five MSBs
    // (11001b) can be changed with new EEPROM programming.
    //
    // HW_SW=GND EEPROM + I²C.
    // GPIO0 = 3V3.
    // GPIO1 = GND.  Low = 00.
    // So I2C address is 1100100b. 0xc8, 0xc9.
    let _ = i2c::write(LMK05318, &13u16.to_be()).wait();
    // dbgln!("Send LMK reg# {:?}", r);
    let mut regs = [0u8; 16];
    let _ = i2c::read(LMK05318, &mut regs).wait();
    // dbgln!("Read LMK regs {:?}", r);
    dbgln!("Regs {:x?}", regs);
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

fn gps_reset() {
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
static VECTORS: cpu::VectorTable = {
    let mut vtor = cpu::VectorTable::default();
    vtor.debug().usb().gps_uart().i2c();
    vtor.systick = systick_handler;
    vtor
};
