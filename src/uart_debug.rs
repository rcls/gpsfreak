
// RX on pin 25. PA15, USART2 RX.
// TX on pin 11. PA5, USART2 TX

// Boot clock flags:
// HSIDIVF - on
// HSIDIV = 0b01 : 32MHz
// HSION

use crate::cpu::WFE;
use crate::vcell::{UCell, VCell, barrier};

pub use stm32h503::USART2 as UART;

pub struct DebugMarker;

/// State for debug logging.  We mark this as no-init and initialize the cells
/// ourselves, to avoid putting the buffer into BSS.
#[unsafe(link_section = ".noinit")]
pub static DEBUG: Debug = Debug::new();

pub struct Debug {
    w: VCell<u8>,
    r: VCell<u8>,
    buf: [UCell<u8>; 256],
}

fn debug_isr() {
    DEBUG.isr();
}

impl Debug {
    const fn new() -> Debug {
        Debug {
            w: VCell::new(0), r: VCell::new(0),
            buf: [const {UCell::new(0)}; 256]
        }
    }
    fn write_bytes(&self, s: &[u8]) {
        let mut w = self.w.read();
        for &b in s {
            while self.r.read().wrapping_sub(w) == 1 {
                self.enable(w);
                self.push();
            }
            // The ISR won't access the array element in question.
            unsafe {*self.buf[w as usize].as_mut() = b};
            w = w.wrapping_add(1);
        }
        self.enable(w);
    }
    fn push(&self) {
        WFE();
        // If the interrupt is pending, call the ISR ourselves.  Read the bit
        // twice in case there is a race condition where we read pending on an
        // enabled interrupt.
        let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
        let bit: usize = stm32h503::Interrupt::USART2 as usize % 32;
        let idx: usize = stm32h503::Interrupt::USART2 as usize / 32;
        if nvic.icpr[idx].read() & 1 << bit == 0 {
            return;
        }
        // It might take a couple of goes for the pending state to clear, so
        // loop.
        while nvic.icpr[idx].read() & 1 << bit != 0 {
            unsafe {nvic.icpr[idx].write(1 << bit)};
            debug_isr();
        }
    }
    fn enable(&self, w: u8) {
        barrier();
        self.w.write(w);

        let uart = unsafe {&*UART::ptr()};
        // Use the FIFO empty interrupt.  Normally we should be fast enough
        // to refill before the last byte finishes.
        uart.CR1.write(
            |w| w.FIFOEN().set_bit().TE().set_bit().UE().set_bit()
                . TXFEIE().set_bit());
    }
    fn isr(&self) {
        let uart = unsafe {&*UART::ptr()};
        let sr = uart.ISR.read();
        if sr.TC().bit() {
            uart.CR1.modify(|_,w| w.TCIE().clear_bit());
        }
        if !sr.TXFE().bit() {
            return;
        }

        const FIFO_SIZE: usize = 8;
        let mut r = self.r.read() as usize;
        let w = self.w.read() as usize;
        let mut done = 0;
        while r != w && done < FIFO_SIZE {
            uart.TDR.write(|w| w.bits(*self.buf[r].as_ref() as u32));
            r = (r + 1) & 0xff;
            done += 1;
        }
        self.r.write(r as u8);
        if r == w {
            uart.CR1.modify(|_,w| w.TXFEIE().clear_bit());
        }
    }
}

pub fn write_str(s: &str) {
    DEBUG.write_bytes(s.as_bytes());
}

impl core::fmt::Write for DebugMarker {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        write_str(s);
        Ok(())
    }
    fn write_char(&mut self, c: char) -> core::fmt::Result {
        let cc = [c as u8];
        DEBUG.write_bytes(&cc);
        Ok(())
    }
}

#[macro_export]
macro_rules! dbg {
    ($($tt:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(
            &mut $crate::uart_debug::DebugMarker, format_args!($($tt)*));
    }}
}

#[macro_export]
macro_rules! dbgln {
    () => {{
        let _ = core::fmt::Write::write_str(
            &mut $crate::uart_debug::DebugMarker, "\n");
        }};
    ($($tt:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(
            &mut $crate::uart_debug::DebugMarker, format_args_nl!($($tt)*));
        }};
}

pub fn init() {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let uart = unsafe {&*UART::ptr()};

    DEBUG.w.write(0);
    DEBUG.r.write(0);

    gpioa.AFRL.modify(|_,w| w.AFSEL5().B_0x9());
    gpioa.AFRH.modify(|_,w| w.AFSEL15().B_0x9());
    gpioa.MODER.modify(|_,w| w.MODE5().B_0x2().MODE15().B_0x2());

    // 32e6 / 115200 ≈ 277.
    uart.BRR.write(|w| w.bits(277));

    uart.CR1.write(|w| w.FIFOEN().set_bit().TE().set_bit().UE().set_bit());

    crate::cpu::enable_interrupt(stm32h503::Interrupt::USART2);

    if false {
        dbg!("");
        dbgln!("");
    }
}

impl crate::cpu::VectorTable {
    pub const fn debug(&mut self) -> &mut Self {
        self.isr(stm32h503::Interrupt::USART2, debug_isr)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[stm32h503::Interrupt::USART2 as usize]
            == debug_isr);
}