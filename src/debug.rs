
// RX on pin 25. PA15, USART3 RX.
// TX on pin 26. PB3, USART3 TX

use crate::cpu::interrupt;

#[path = "../stm-common/debug_core.rs"]
#[macro_use]
mod debug_core;
use debug_core::Debug;
pub use debug_core::debug_isr;

use stm32h503::USART3 as UART;
use stm32h503::Interrupt::USART3 as INTERRUPT;

pub const ENABLE: bool = true;
pub const BAUD: u32 = 115200;
const BRR: u32 = (crate::cpu::CPU_FREQ + BAUD/2) / BAUD;
const _: () = assert!(BRR < 65536);

/// State for debug logging.
pub static DEBUG: Debug = Debug::default();

/// Guard for running at the priority for accessing debug.
type DebugMarker = crate::cpu::Priority::<{interrupt::PRIO_COMMS}>;
pub fn debug_marker() -> DebugMarker {
    return DebugMarker::new();
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
        |w|w.FIFOEN().set_bit().TE().set_bit().UE().set_bit());

    interrupt::enable_priority(INTERRUPT, interrupt::PRIO_DEBUG);

    if false {
        dbg!("");
        dbgln!("");
        debug_core::flush();
    }
}

/// We don't support lazy initialization.  Provide a dummy hook for debug_core.
fn lazy_init() {}
/// We don't support lazy initialization.  Provide a dummy hook for debug_core.
fn is_init() -> bool {true}

#[cfg(target_os = "none")]
#[panic_handler]
fn ph(info: &core::panic::PanicInfo) -> ! {
    dbgln!("{info}");
    loop {
        debug_core::flush();
    }
}

impl crate::cpu::VectorTable {
    pub const fn debug(&mut self) -> &mut Self {
        if ENABLE {
            self.isr(INTERRUPT, debug_core::debug_isr)
        }
        else {
            self
        }
    }
}

#[test]
fn check_isr() {
    if ENABLE {
        assert!(crate::VECTORS.isr[INTERRUPT as usize] == debug_core::debug_isr);
    }
}
