
// RX on pin 25. PA15, USART3 RX.
// TX on pin 26. PB3, USART3 TX

use crate::cpu::{WFE, barrier, interrupt};
use crate::vcell::{UCell, VCell};

pub use stm32h503::USART3 as UART;
pub use stm32h503::Interrupt::USART3 as INTERRUPT;

pub struct DebugMarker;

pub const ENABLE: bool = true;
pub const BAUD: u32 = 115200;
const BRR: u32 = (crate::cpu::CPU_FREQ + BAUD/2) / BAUD;
const _: () = assert!(BRR < 65536);

const FIFO_SIZE: usize = 8;

/// State for debug logging.
pub static DEBUG: Debug = Debug::new();

/// Guard for running at the priority for accessing debug.
pub type DebugPriority = crate::cpu::Priority::<{interrupt::PRIO_COMMS}>;

type Index = u8;
const BUF_SIZE: usize = 256;
const BUF_MASK: Index = (BUF_SIZE - 1) as Index;
const _: () = assert!(BUF_SIZE <= Index::MAX as usize + 1);
const _: () = assert!(BUF_SIZE & BUF_SIZE - 1 == 0);

pub struct Debug {
    w: VCell<Index>,
    r: VCell<Index>,
    buf: [UCell<u8>; BUF_SIZE],
}

pub fn debug_isr() {
    DEBUG.isr();
}

impl Debug {
    const fn new() -> Debug {
        Debug {
            w: VCell::new(0), r: VCell::new(0),
            buf: [const {UCell::new(0)}; _]
        }
    }
    fn write_bytes(&self, s: &[u8]) {
        let mut w = self.w.read();
        for &b in s {
            while self.r.read().wrapping_sub(w) & BUF_MASK == 1 {
                self.enable(w);
                self.push();
            }
            // The ISR won't access the array element in question.
            unsafe {*self.buf[w as usize].as_mut() = b};
            w = w.wrapping_add(1) & BUF_MASK;
        }
        self.enable(w);
    }
    fn push(&self) {
        WFE();
        // If the interrupt is pending, call the ISR ourselves.  Read the bit
        // twice in case there is a race condition where we read pending on an
        // enabled interrupt.
        let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
        let bit: usize = INTERRUPT as usize % 32;
        let idx: usize = INTERRUPT as usize / 32;
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
        uart.CR1.modify(|_,w| w.TXFEIE().set_bit());
    }
    fn isr(&self) {
        let uart = unsafe {&*UART::ptr()};
        let isr = uart.ISR.read();
        if isr.TC().bit() {
            uart.CR1.modify(|_,w| w.TCIE().clear_bit());
        }
        if isr.TXFE().bit() {
            self.isr_tx();
        }
    }

    fn isr_tx(&self) {
        let uart = unsafe {&*UART::ptr()};
        let mut r = self.r.read() as usize;
        let w = self.w.read() as usize;
        let mut done = 0;
        while r != w && done < FIFO_SIZE {
            uart.TDR.write(|w| w.bits(*self.buf[r].as_ref() as u32));
            r = r + 1 & BUF_SIZE - 1;
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
        if $crate::uart_debug::ENABLE {
            let _raise = $crate::uart_debug::DebugPriority::new();
            let _ = core::fmt::Write::write_fmt(
                &mut $crate::uart_debug::DebugMarker, format_args!($($tt)*));
    }}}
}

#[macro_export]
macro_rules! dbgln {
    () => {{
        if $crate::uart_debug::ENABLE {
            let _raise = $crate::uart_debug::DebugPriority::new();
            let _ = core::fmt::Write::write_str(
                &mut $crate::uart_debug::DebugMarker, "\n");
        }}};
    ($($tt:tt)*) => {{
        if $crate::uart_debug::ENABLE {
            let _raise = $crate::uart_debug::DebugPriority::new();
            let _ = core::fmt::Write::write_fmt(
                &mut $crate::uart_debug::DebugMarker, format_args_nl!($($tt)*));
        }}};
}

pub fn init() {
    if !ENABLE {
        return;
    }

    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let uart  = unsafe {&*UART::ptr()};

    rcc.APB1LENR.modify(|_,w| w.USART3EN().set_bit());

    gpioa.AFRH.modify(|_,w| w.AFSEL15().B_0xD());
    gpiob.AFRL.modify(|_,w| w.AFSEL3().B_0xD());
    gpioa.MODER.modify(|_,w| w.MODE15().B_0x2());
    gpiob.MODER.modify(|_,w| w.MODE3().B_0x2());

    uart.BRR.write(|w| w.bits(BRR));

    uart.CR1.write(
        |w|w.FIFOEN().set_bit().TE().set_bit()
            .UE().set_bit().RXFNEIE().set_bit());

    interrupt::enable_priority(INTERRUPT, interrupt::PRIO_DEBUG);

    if false {
        dbg!("");
        dbgln!("");
    }
}

#[cfg(target_os = "none")]
#[panic_handler]
fn ph(info: &core::panic::PanicInfo) -> ! {
    dbgln!("{info}");
    loop {
        if ENABLE {
            DEBUG.push();
        }
    }
}

impl crate::cpu::VectorTable {
    pub const fn debug(&mut self) -> &mut Self {
        if ENABLE {
            self.isr(INTERRUPT, debug_isr)
        }
        else {
            self
        }
    }
}

#[test]
fn check_isr() {
    if ENABLE {
        assert!(crate::VECTORS.isr[INTERRUPT as usize] == debug_isr);
    }
}
