
use crate::vcell::UCell;

use core::num::Wrapping as W;

/// The timer we use for blinking.
///
/// We use a prescaler to get the counter running at 10kHz (100µs).  It is
/// 16-bit, and we use signed wrapping arithmetic, giving a maximum timeout
/// of just over 3 seconds.
use stm32h503::TIM3 as TIM;
use stm32h503::Interrupt::TIM3 as INTERRUPT;

use crate::cpu::interrupt::PRIO_LED as PRIORITY;
type Priority = crate::cpu::Priority<PRIORITY>;

type FiveHz<T> = UCell<Led<1000, 1000, T>>;

pub static BLUE : FiveHz<Blue> = Default::default();
pub static RED_GREEN: FiveHz<RedGreen> = Default::default();

macro_rules!dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

pub fn init() {
    let gpioa = unsafe {&*stm32h503::GPIOA::PTR};
    let rcc = unsafe {&*stm32h503::RCC::PTR};
    let tim = unsafe {&*TIM::PTR};

    // Blue/green/red are PA1,2,3.
    gpioa.BSRR.write(|w| w.bits(0xe));
    gpioa.MODER.modify(|_,w| w.MODE1().B_0x1().MODE2().B_0x1().MODE3().B_0x1());

    rcc.APB1LENR.modify(|_,w| w.TIM3EN().set_bit());
    // Set ARR to 0?
    tim.DIER.write(|w| w.CC1IE().set_bit());
    tim.CCMR1_Output().write(|w| w.OC1CE().set_bit().OC1M1().B_0x1());
    const PSC: u16 = (crate::cpu::CPU_FREQ / 10000) as u16 - 1;
    const {assert!((PSC as u32 + 1) * 10000 == crate::cpu::CPU_FREQ)};
    tim.PSC.write(|w| w.bits(PSC));
    tim.CR1.write(|w| w.CEN().set_bit());

    use crate::cpu::interrupt;
    // Both the interrupt, and the callers into this code, should run at the
    // same priority.
    interrupt::enable_priority(INTERRUPT, PRIORITY);
}

#[derive_const(Default)]
pub struct Blue;

impl LedTrait for Blue {
    fn set(&mut self, state: bool) {
        let gpioa = unsafe{&*stm32h503::GPIOA::PTR};
        // Negative polarity: we drive low for on.
        gpioa.BSRR.write(|w| w.BR1().set_bit().BS1().bit(!state));
    }
}

/// Pair of red and green LEDs.  Positive logic indicator, true = good = green.
/// Red is PA3, green is PA2.
#[derive_const(Default)]
pub struct RedGreen;

impl LedTrait for RedGreen {
    fn set(&mut self, state: bool) {
        let gpioa = unsafe{&*stm32h503::GPIOA::PTR};
        // This gives negative polarity as wanted.
        gpioa.BSRR.write(
            |w|w.BR2().set_bit().BR3().set_bit()
                .BS2().bit(!state).BS3().bit(state));
    }
}

pub trait LedTrait {
    fn set(&mut self, state: bool);
}

#[derive_const(Default)]
pub struct Led<const ON: i16, const OFF: i16, T> {
    timer: LedTimer<ON, OFF>,
    led: T,
}

impl<const ON: i16, const OFF: i16, T: LedTrait> Led<ON, OFF, T> {
    fn isr(&mut self, now: i16) {
        self.timer.isr(now);
        self.led.set(self.timer.led);
    }
    pub fn set(&mut self, state: bool) {
        let tim = unsafe {&*TIM::PTR};

        let _guard = Priority::new();
        schedule(self.timer.set(state, tim.CNT.read().bits() as i16));
        self.led.set(self.timer.led);
    }
    pub fn pulse(&mut self, state: bool) {
        let tim = unsafe {&*TIM::PTR};

        let _guard = Priority::new();
        schedule(self.timer.pulse(state, tim.CNT.read().bits() as i16));
        self.led.set(self.timer.led);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[derive_const(Default)]
struct LedTimer<const ON: i16, const OFF: i16> {
    /// Current desired state of the LED.  Various methods update this field,
    /// the wrapping methods in `Led` update the physical LED to match.
    led: bool,
    /// Next state to move to.
    next: bool,
    /// State we want to end up on.
    target: bool,
    /// Time at which the current setting expires.
    expiry: Option<W<i16>>,
}

fn schedule(deadline: Option<W<i16>>) {
    let tim = unsafe {&*TIM::PTR};
    let Some(deadline) = deadline else {return};
    // Only update the timer if we want to bring the expiry forwards.  We are
    // not called from the ISR, so equality doesn't need an update.
    if deadline - W(tim.CCR1.read().bits() as i16) < W(0) {
        trigger(deadline);
    }
}

fn trigger(deadline: W<i16>) {
    let tim = unsafe {&*TIM::PTR};
    tim.CCR1.write(|w| w.bits(deadline.0 as u16 as u32));
    let now = tim.CNT.read().bits() as i16;
    if W(now) - deadline >= W(0) {
        // We've already expired.  Instead of potentially recursing, do a
        // software trigger.
        dbgln!("Immediate");
        let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
        unsafe {nvic.stir.write(INTERRUPT as u32)};
    }
}

impl<const ON: i16, const OFF: i16> LedTimer<ON, OFF> {
    fn isr(&mut self, now: i16) {
        let Some(expiry) = self.expiry else {return};
        if W(now) - expiry < W(0) {
            return;
        }
        if self.next != self.led {
            self.update(self.next, self.target, now);
        }
        else {
            debug_assert!(self.next == self.target);
            // We should have led==next==target, but just in case something
            // funning is going on, defensively process it.
            self.led = self.target;
            self.next = self.target;
            self.expiry = None;
        }
    }

    fn duration(&self, state: bool) -> i16 {
        if state {ON} else {OFF}
    }

    fn set(&mut self, state: bool, now: i16) -> Option<W<i16>> {
        self.target = state;
        if self.expiry != None {
            if self.led == self.next || self.duration(self.next) == 0 {
                self.next = state;
            }
            return None;
        }
        if self.led == state {
            return None;               // Nothing to do.
        }

        // No timer is running; just apply everything now.
        self.update(state, state, now)
    }

    fn pulse(&mut self, state: bool, now: i16) -> Option<W<i16>> {
        // Equivalent to: request(state) ; request(!state)
        self.target = !state;
        let next = state ^ (
            self.duration(state) == 0 || self.led == state);
        if self.expiry != None {
            self.next = next;
            return None;
        }
        self.update(next, !state, now)
    }

    /// Set self.led and return the timer expiry time.
    fn update(&mut self, state: bool, next: bool, now: i16) -> Option<W<i16>> {
        self.led = state;
        self.next = next;
        let duration = self.duration(state);
        if duration != 0 {
            self.expiry = Some(W(now) + W(duration));
        }
        else {
            self.expiry = None;
        }
        self.expiry
    }
}

fn isr() {
    dbgln!("LED isr");
    let tim = unsafe {&*TIM::PTR};
    tim.SR.write(|w| w.bits(0));

    let now = tim.CNT.read().bits() as i16;

    let min = |a: W<i16>, b: Option<W<i16>>| -> W<i16> {
        match (a, b) {
            (a, Some(b)) => if a - b < W(0) {a} else {b},
            (a, None) => a,
        }
    };

    let (blue, red_green) = unsafe{(BLUE.as_mut(), RED_GREEN.as_mut())};
    blue.isr(now);
    red_green.isr(now);

    // We make sure that the time is always scheduled in the future, even if
    // there is nothing to do.  Otherwise we need to deal with the ambiguity
    // between timers in the past being processed or late.
    let deadline = W(now) + W(30000);
    let deadline = min(deadline, blue.timer.expiry);
    let deadline = min(deadline, red_green.timer.expiry);
    dbgln!("LED {now} {deadline}");

    trigger(deadline);
}

impl crate::cpu::VectorTable {
    pub const fn led(&mut self) -> &mut Self {
        self.isr(INTERRUPT, isr)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == isr);
}

#[test]
fn test_fast() {
    let mut lt = LedTimer::<10, 10>::default();

    let mut exp = lt;

    assert_eq!(lt.set(false, 0), None);
    assert_eq!(lt, exp);
    lt.isr(1);
    assert_eq!(lt, exp);

    assert_ne!(lt.set(true, 10), None);
    exp.led = true;
    exp.next = true;
    exp.target = true;
    exp.expiry = Some(W(20));
    assert_eq!(lt, exp);

    assert_eq!(lt.set(true, 11), None);
    assert_eq!(lt, exp);

    assert_eq!(lt.set(false, 12), None);
    exp.next = false;
    exp.target = false;
    assert_eq!(lt, exp);

    assert_eq!(lt.set(true, 13), None);
    exp.target = true;
    assert_eq!(lt, exp);

    assert_eq!(lt.set(false, 14), None);
    exp.target = false;
    assert_eq!(lt, exp);

    assert_eq!(lt.set(true, 15), None);
    exp.target = true;
    assert_eq!(lt, exp);

    lt.isr(20);
    exp.led = exp.next;
    exp.next = exp.target;
    exp.expiry = Some(W(30));
    assert_eq!(lt, exp);

    assert_eq!(lt.set(false, 21), None);
    exp.target = false;
    assert_eq!(lt, exp);

    assert_eq!(lt.set(true, 22), None);
    exp.target = true;
    assert_eq!(lt, exp);

    lt.isr(29);
    assert_eq!(lt, exp);

    lt.isr(30);
    exp.led = exp.next;
    exp.next = exp.target;
    exp.expiry = Some(W(40));
    assert_eq!(lt, exp);

    lt.isr(39);
    assert_eq!(lt, exp);

    lt.isr(40);
    exp.led = exp.next;
    exp.next = exp.target;
    exp.expiry = None;
    assert_eq!(lt, exp);
    lt.isr(41);
    assert_eq!(lt, exp);
}

#[test]
fn test_zero() {
    let mut lt = LedTimer::<10, 0>::default();
    let mut exp = lt;
    lt.set(false, 0);
    assert_eq!(lt, exp);

    lt.set(true, 10);
    exp.led = true;
    exp.next = true;
    exp.target = true;
    exp.expiry = Some(W(20));
    assert_eq!(lt, exp);

    lt.set(true, 11);
    assert_eq!(lt, exp);

    lt.set(false, 12);
    exp.next = false;
    exp.target = false;
    assert_eq!(lt, exp);

    lt.set(true, 13);
    exp.next = true;
    exp.target = true;
    assert_eq!(lt, exp);

    lt.set(false, 14);
    exp.next = false;
    exp.target = false;
    assert_eq!(lt, exp);

    lt.isr(19);
    assert_eq!(lt, exp);

    lt.isr(20);
    exp.led = false;
    exp.expiry = None;
    assert_eq!(lt, exp);
    lt.isr(21);
    assert_eq!(lt, exp);
}

#[cfg(test)]
impl<const ON: i16, const OFF: i16> LedTimer<ON, OFF> {
    fn test_pulse1() {
        for led in [false, true] {
            for next in [false, true] {
                for target in [false, true] {
                    for expiry in [None, Some(W(10))] {
                        if expiry != None && ON == 0 && OFF == 0 {
                            continue;
                        }
                        let l = Self{led, next, target, expiry};
                        for request in [false, true] {
                            let (mut r, mut p) = (l, l);
                            r.set(request, 5);
                            r.set(!request, 5);
                            p.pulse(request, 5);
                            assert_eq!(r, p, "{l:?} {request}");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn test_pulse() {
    LedTimer::< 0,  0>::test_pulse1();
    LedTimer::< 0, 10>::test_pulse1();
    LedTimer::<10,  0>::test_pulse1();
    LedTimer::<10, 10>::test_pulse1();
}
