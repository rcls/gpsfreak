//! USB for GPS ref.
//! Endpoints:
//! 0 : Control, as always.
//!   OUT: 64 bytes at 0x80 offset.
//!   IN : 64 bytes at 0xc0 offset.  TODO - do we use both?
//!   CHEP 0
//! 01, 81: CDC ACM data transfer, bulk
//!   OUT (RX): 2 × 64 bytes at 0x100 offset.
//!     DTOGRX==0 -> RX into RXTX buf. 1×2 + 1 = 3.
//!     DTOGRX==1 -> RX into TXRX buf. 1×2 + 0 = 2.
//!   IN  (TX): 8 x 64 bytes at 0x200 offset.
//!     DTOGTX==0 -> TX from TXRX buf. 2×2 + 0 = 4.
//!     DTOGTX==1 -> TX from RXTX buf. 2×2 + 1 = 5.
//! 82: CDC ACM interrupt IN (to host).
//!   64 bytes at 0x40 offset.
//!   CHEP 2

use crate::cpu::interrupt;

use stm32h503::Interrupt::USB_FS as INTERRUPT;

fn usb_isr() {
    if unsafe{super::USB_STATE.as_mut()}.isr() {
        crate::led::BLUE.pulse(true);
    }
}

pub fn init() {
    unsafe{super::USB_STATE.as_mut()}.init();

    interrupt::enable_priority(INTERRUPT, interrupt::PRIO_COMMS);
}

impl crate::cpu::Config {
    pub const fn usb(&mut self) -> &mut Self {
        self.isr(INTERRUPT, usb_isr)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == usb_isr);
}
