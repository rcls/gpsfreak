
// RX on pin 25. PA15, USART3 RX.
// TX on pin 26. PB3, USART3 TX

use crate::cpu::interrupt::PRIO_DEBUG;

use stm_common::debug;
use debug::{Debug, Meta};

use stm32h503::Interrupt::USART3 as INTERRUPT;

pub const BAUD: u32 = 115200;
const BRR: u32 = (crate::cpu::CPU_FREQ + BAUD/2) / BAUD;
const _: () = assert!(BRR < 65536);

/// State for debug logging.
pub static DEBUG: Debug<DebugMeta> = Debug::default();

#[derive_const(Default)]
pub struct DebugMeta;

impl Meta for DebugMeta {
    fn debug() -> &'static Debug<Self> {&DEBUG}

    fn uart(&self) -> &'static stm32h503::usart1::RegisterBlock {
        unsafe {&*stm32h503::USART3::PTR}}
    /// We don't support lazy initialization.  Provide a dummy hook for
    /// debug_core.
    fn lazy_init(&self) {}
    /// We don't support lazy initialization.  Provide a dummy hook for
    /// debug_core.
    fn is_init(&self) -> bool {true}
    fn interrupt(&self) -> u32 {INTERRUPT as u32}

    const ENABLE: bool = crate::DEBUG_ENABLE;
}

pub fn debug_isr() {
    DEBUG.isr();
}

pub fn init() {
    if !crate::DEBUG_ENABLE {
        return;
    }

    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let uart = DebugMeta.uart();

    rcc.APB1LENR.modify(|_,w| w.USART3EN().set_bit());

    gpioa.AFRH.modify(|_,w| w.AFSEL15().B_0xD());
    gpiob.AFRL.modify(|_,w| w.AFSEL3().B_0xD());
    gpioa.MODER.modify(|_,w| w.MODE15().B_0x2());
    gpiob.MODER.modify(|_,w| w.MODE3().B_0x2());

    uart.BRR.write(|w| w.bits(BRR));

    uart.CR1.write(
        |w|w.FIFOEN().set_bit().TE().set_bit().UE().set_bit());

    stm_common::interrupt::enable_priority(INTERRUPT, PRIO_DEBUG);
}

#[cfg(target_os = "none")]
#[panic_handler]
fn ph(info: &core::panic::PanicInfo) -> ! {
    stm_common::dbgln!("{info}");
    loop {
        stm_common::debug::flush::<DebugMeta>();
    }
}

impl crate::cpu::Config {
    pub const fn debug(&mut self) -> &mut Self {
        if crate::DEBUG_ENABLE {
            self.vectors.isr(INTERRUPT, debug_isr);
        }
        self
    }
}

#[test]
fn check_isr() {
    if crate::DEBUG_ENABLE {
        assert!(crate::VECTORS.isr[INTERRUPT as usize] == debug_isr);
    }
}
