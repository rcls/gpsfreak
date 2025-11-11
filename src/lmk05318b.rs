//! LMK05318b handling.
//!
//! Mostly, the LMK05318b clock generator is handled via the host, or start-up
//! configuration, sending I²C commands.  This is basically just the status
//! LED handling.

use crate::cpu::interrupt;

macro_rules!dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

use stm32h503::Interrupt::EXTI0 as INTERRUPT;
use stm32h503::Interrupt::TIM6 as TIM_INTERRUPT;
use interrupt::PRIO_STATUS as PRIORITY;

/// I²C address of the LMK05318(B).
pub const LMK05318: u8 = 0xc8;

pub type TIM = stm32h503::TIM6;

pub fn init() {
    let exti  = unsafe {&*stm32h503::EXTI ::PTR};
    let gpiob = unsafe {&*stm32h503::GPIOB::PTR};
    let rcc   = unsafe {&*stm32h503::RCC  ::PTR};
    let tim = unsafe {&*TIM::PTR};

    rcc.APB1LENR.modify(|_,w| w.TIM6EN().set_bit());

    // PB0 is STATUS1 interrupt pin from LMK05318b.  Set it to an input.
    gpiob.MODER.modify(|_,w| w.MODE0().B_0x0());
    // Trigger on rising edge.
    exti.RTSR1.modify(|_,w| w.RT0().set_bit());
    exti.EXTICR1.modify(|_,w| w.EXTI0().B_0x1());
    exti.IMR1.modify(|_,w| w.IM0().set_bit()); // This should be default!

    // Timer for rate limiting.
    const PRESCALE: u32 = crate::cpu::CPU_FREQ / 10000 - 1; // 10kHz
    const {assert!(PRESCALE >= 1 && PRESCALE < 65536)};
    tim.PSC.write(|w| w.PSC().bits(PRESCALE as u16));
    tim.ARR.write(|w| w.ARR().bits(400 - 1)); // 25Hz.
    tim.CNT.write(|w| w.CNT().bits(400 - 1));
    tim.DIER.write(|w| w.UIE().set_bit());

    // This needs to run at the same priority as the command code, because both
    // access I²C.
    interrupt::enable_priority(INTERRUPT, PRIORITY);
    interrupt::enable_priority(TIM_INTERRUPT, PRIORITY);
    // Software trigger the EXTI0 interrupt to kick things off.  TODO - could
    // just call it!
    let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
    unsafe {nvic.stir.write(INTERRUPT as u32)};
}

pub fn update_status() {
    dbgln!("exti6_isr");
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let exti  = unsafe {&*stm32h503::EXTI ::ptr()};
    let tim   = unsafe {&*TIM::PTR};

    dbgln!("PB0 is {}", gpiob.IDR().read().ID0().bit());
    dbgln!("pending = {:#06x}", exti.RPR1.read().bits());
    // Clear the pending bit.
    exti.RPR1.write(|w| w.RPIF0().set_bit());
    tim.SR.write(|w| w.bits(0));

    use crate::led::RED_GREEN;

    let (good, changes, flicker)
        = lmk05318b_status().unwrap_or((false, true, false));
    if !good || changes || flicker {
        dbgln!("Set red");
        RED_GREEN.set(false);
    }
    if good {
        dbgln!("Set green");
        RED_GREEN.set(true);
    }

    // Hopefully we have cleared the interrupt line, but if not, software
    // trigger the interrupt.  FIXME - this should be rate limited.
    if flicker || gpiob.IDR().read().ID0().bit() {
        dbgln!("Flicker {flicker} and/or PB0 is still high");
        tim.CR1.write(|w| w.OPM().set_bit().CEN().set_bit());
    }
}

fn lmk05318b_status() -> Result<(bool, bool, bool), ()> {
    // FIXME - error handling.
    // Read status, 13 through 20.
    let mut data = [0u16; 4];
    crate::i2c::write_read(LMK05318, &13u16.to_be(), &mut data).wait()?;
    let [bits, mask, pol, intr] = data;
    dbgln!("bits {bits:#06x} mask {mask:#06x} pol {pol:#06x} intr {intr:#06x}");

    // Set the polarities to the opposite of what we just read, and clear
    // the interrupts.
    let polarity_and_clear = [17u16.to_be(), !bits, !intr];
    crate::i2c::write(LMK05318, &polarity_and_clear).wait()?;

    // Re-read bits, just in case they changed underneath us.  This is racey.
    // We attempt to deal with that in our caller, by redoing the status read.
    let mut new_bits = 0u16;
    crate::i2c::write_read(LMK05318, &13u16.to_be(), &mut new_bits).wait()?;
    dbgln!("bits {new_bits:#06x}");

    let good = new_bits & !mask == 0; // Everything good.
    // Note that we get called pre-emptively in various situations.  So note
    // whether or not the LMK05318b thought it was giving us an interrupt.
    let changes = intr & !mask != 0;          // This was a real interrupt.
    let flicker = new_bits & !mask != bits & !mask; // WTF.
    Ok((good, changes, flicker))
}

impl crate::cpu::VectorTable {
    pub const fn lmk05318b(&mut self) -> &mut Self {
        self.isr(INTERRUPT, update_status).isr(TIM_INTERRUPT, update_status)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == update_status);
}
