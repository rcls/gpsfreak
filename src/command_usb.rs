use crate::{link_assert, usb::{EndPointPair, hardware::{CheprReader, CheprWriter, MAIN_RX_BUF, MAIN_TX_BUF, bd_main, chep_bd_len, chep_main, copy_by_dest32}}};

#[derive_const(Default)]
pub struct CommandUSB;

macro_rules!main_dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

pub fn init() {
    // We use the PENDSV exception to dispatch some work at lower priority.
    let scb = unsafe {&*cortex_m::peripheral::SCB::PTR};
    let pendsv_prio = &scb.shpr[10];
    // Cortex-M crate has two different ideas of what the SHPR is, make sure we
    // are built with the correct one.
    link_assert!(pendsv_prio as *const _ as usize == 0xe000ed22);
    unsafe {pendsv_prio.write(crate::cpu::interrupt::PRIO_APP)};
}

impl EndPointPair for CommandUSB {
    fn rx_handler(&mut self) {
        let chep = chep_main().read();
        if !chep.VTRX().bit() {
            main_dbgln!("main: Spurious RX interrupt, CHEP {:#6x}", chep.bits());
            return;
        }
        main_dbgln!("main: RX interrupt, CHEP {:#6x}", chep.bits());

        // We notify the application by triggering PendSV.  The application
        // can notify completion, either by transmitting a message or by
        // by calling the completion function.
        let scb = unsafe {&*cortex_m::peripheral::SCB::PTR};
        unsafe {scb.icsr.write(1 << 28)};

        chep_main().write(|w| w.main().VTRX().clear_bit());
    }

    /// We have finished processing a message by sending a response. Rearm the RX.
    /// TODO: maybe we should also re-arm on a timeout, in case the user doesn't
    /// read the response?
    fn tx_handler(&mut self) {
        let chep = chep_main().read();
        if !chep.VTTX().bit() {
            main_dbgln!("main: Spurious TX interrupt, CHEP {:#6x}", chep.bits());
            return;
        }
        chep_main().write(|w| w.main().rx_valid(&chep).VTTX().clear_bit());
        main_dbgln!("main: TX done CHEP {:#06x} was {:#06x}",
                    chep_main().read().bits(), chep.bits());
    }
}

/// PendSV ISR for handling device commands at appropriate priority.
fn command_handler() {
    main_dbgln!("Command handler entry");

    // Get a point to the message.  TODO - copy!
    let message = unsafe {&*(MAIN_RX_BUF as *const crate::command::MessageBuf)};
    crate::command::command_handler(
        &message, chep_bd_len(bd_main().rx.read()), main_tx_response);
}

// Called at lower priority and can get interrupted!
fn main_tx_response(message: &[u8]) {
let chep = chep_main().read();
    if message.len() == 0 {
        main_dbgln!("main_tx_response, no data, rearm");
        chep_main().write(|w| w.main().rx_valid(&chep));
        return;
    }
    // For now we don't support long messages.
    let len = message.len().min(64);
    unsafe {copy_by_dest32(message.as_ptr(), MAIN_TX_BUF, message.len())};

    bd_main().tx_set(MAIN_TX_BUF, len);

    let chep = chep_main().read();
    chep_main().write(|w| w.main().tx_valid(&chep));

    main_dbgln!("main tx {len} bytes, {}CHEP now {:#06x} was {:#06x}",
                if chep.tx_active() {"INCORRECT STATE "} else {""},
                chep_main().read().bits(),
                chep.bits());
}

impl crate::cpu::VectorTable {
    pub const fn command_usb(&mut self) -> &mut Self {
        self.pendsv = command_handler;
        self
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.pendsv == command_handler);
}
