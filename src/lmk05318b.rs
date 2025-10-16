//! LMK05318b handling.
//!
//! Mostly, the LMK05318b clock generator is handled via the host, or start-up
//! configuration, sending I²C commands.  This is basically just the status
//! LED handling.

use crate::cpu::interrupt;

macro_rules!dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

use stm32h503::Interrupt::EXTI6 as INTERRUPT;
use interrupt::PRIO_APP as PRIORITY;

/// I²C address of the LMK05318(B).
pub const LMK05318: u8 = 0xc8;

pub fn init() {
    let exti  = unsafe {&*stm32h503::EXTI ::ptr()};
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};

    // PA6 is STATUS1 interrupt pin from LMK05318b.  Set it to an input.
    gpioa.MODER.modify(|_,w| w.MODE6().B_0x0());
    // Trigger on rising and falling edge.
    exti.RTSR1.modify(|_,w| w.RT6().set_bit());
    exti.FTSR1.modify(|_,w| w.FT6().set_bit());
    exti.EXTICR2.modify(|_,w| w.EXTI6().B_0x0()); // This is the default!
    exti.IMR1.modify(|_,w| w.IM6().set_bit()); // This should be default too!
    // This needs to run at the same priority as the command code, because both
    // access I²C.
    interrupt::enable_priority(INTERRUPT, PRIORITY);
    // Software trigger the EXTI6 interrupt to kick things off.  TODO - could
    // just call it!
    let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
    unsafe {nvic.stir.write(INTERRUPT as u32)};
}

pub fn update_status() {
    dbgln!("exti6_isr");
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    dbgln!("PA6 is {}", gpioa.IDR().read().ID6().bit());
    let exti  = unsafe {&*stm32h503::EXTI ::ptr()};
    // Clear the pending bits.
    exti.RPR1.write(|w| w.RPIF6().set_bit());
    exti.FPR1.write(|w| w.FPIF6().set_bit());
    dbgln!("pending = {:#06x} {:#06x}",
           exti.RPR1.read().bits(), exti.FPR1.read().bits());

    let error = lmk05318b_status().is_err();
    unsafe {crate::led::RED.as_mut()}.set(error);
    dbgln!("PA6 is {}", gpioa.IDR().read().ID6().bit());
}

fn lmk05318b_status() -> Result<(), ()> {
    // Read the interrupt sticky status, registers 19 and 20.
    let mut interrupts = 0u16;
    // FIXME - error handling.
    crate::i2c::write_read(LMK05318, &19u16.to_be(), &mut interrupts).wait()?;
    dbgln!("INTR = {interrupts:#06x}");

    // Now read the actual status, registers 13 and 14 and the mask bits
    // in 15 and 16.
    let mut bit_and_mask = [0u16; 2];
    crate::i2c::write_read(LMK05318, &13u16.to_be(), &mut bit_and_mask).wait()?;
    let masked = bit_and_mask[0] & !bit_and_mask[1];
    dbgln!("LOL = {:#06x}, MASK = {:#06x}", bit_and_mask[0], bit_and_mask[1]);

    // Set the polarities to the opposite of what we just read, register 17 and
    // 18.  Clear the interrupt flags we just saw, register 19 and 20.
    let polarity_and_clear = [17u16.to_be(), !bit_and_mask[0], !interrupts];
    crate::i2c::write(LMK05318, &polarity_and_clear).wait()?;

    // Re-read bits just for logging.
    crate::i2c::write_read(LMK05318, &19u16.to_be(), &mut interrupts).wait()?;
    dbgln!("INTR = {interrupts:#06x}");

    if masked != 0 {Err(())} else {Ok(())}
}

impl crate::cpu::VectorTable {
    pub const fn lmk05318b(&mut self) -> &mut Self {
        self.isr(INTERRUPT, update_status)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == update_status);
}
