
use super::descriptor::{CONFIG0_DESC, DEVICE_DESC, INTF_DFU};
use super::hardware::*;
use super::types::*;

use crate::cpu::barrier;
use crate::usb::EndPointPair;

use super::{ctrl_dbgln, usb_dbgln};

#[derive_const(Default)]
pub struct ControlState {
    /// If request(Type) are non-zero, then we are waiting RX data for this
    /// setup request.
    setup: SetupHeader = SetupHeader::default(),
    /// Set-up data to send.  On TX ACK we send the next block.
    setup_data: SetupResult = SetupResult::default(),
    /// If set, the TX setup data is shorter than the requested data and we must
    /// end with a zero-length packet if needed.
    setup_short: bool = false,
    /// Address received in a SET ADDRESS.  On TX ACK, we apply this.
    pending_address: Option<u8> = None,
    /// Are we configured?
    configured: bool = false,
    /// Do we have a pending DFU reboot?
    pending_dfu: bool = false,
}

impl EndPointPair for ControlState {
    fn tx_handler(&mut self) {
        let chep = chep_ctrl().read();
        ctrl_dbgln!("Control TX handler CHEP0 = {:#010x}", chep.bits());

        if !chep.VTTX().bit() {
            ctrl_dbgln!("Bugger!");
            return;
        }

        if let SetupResult::Tx(data) = self.setup_data {
            self.setup_next_data(data);
            chep_ctrl().write(
                |w| w.control().VTTX().clear_bit().tx_valid(&chep));
            return;
        }

        if self.pending_dfu {
            unsafe {crate::cpu::trigger_dfu()};
        }

        chep_ctrl().write(
            |w|w.control().VTTX().clear_bit().rx_valid(&chep).dtogrx(&chep, false));

        if let Some(address) = self.pending_address {
            Self::do_set_address(address);
            self.pending_address = None;
            // FIXME - race with incoming?
        }
    }

    fn rx_handler(&mut self) {
        let chep = chep_ctrl().read();
        ctrl_dbgln!("Control RX handler CHEP0 = {:#010x}", chep.bits());

        if !chep.VTRX().bit() {
            ctrl_dbgln!("Bugger");
            return;
        }

        if !chep.SETUP().bit() {
            ctrl_dbgln!("Control RX handler, CHEP0 = {:#010x}, non-setup",
                        chep.bits());

            if self.setup.length == 0 {
                // Either it's an ACK to our data, or we weren't expecting this.
                // Just drop it and flush any outgoing data.
                self.setup_data = SetupResult::default();
                chep_ctrl().write(
                    |w|w.control().VTRX().clear_bit().rx_valid(&chep)
                        .stat_tx(&chep, 2));
                return;
            }

            let ok = self.setup_rx_data();
            self.setup = SetupHeader::default();
            // Send either a zero-length ACK or an error stall.
            bd_control().tx.write(chep_bd_tx(CTRL_TX_OFFSET, 0));
            chep_ctrl().write(
                |w|w.control().VTRX().clear_bit()
                    .stat_tx(&chep, if ok {3} else {1})
                    .rx_valid(&chep));
            return;
        }

        // The USBSRAM only supports 32 bit accesses.  However, that only makes
        // a difference to the AHB bus on writes, not reads.  So access the
        // setup packet in place.
        barrier();
        let setup = unsafe {SetupHeader::from_ptr(CTRL_RX_BUF)};
        // FIXME what about SETUP + OUT?
        let result = self.setup_rx_handler(&setup);
        match result {
            SetupResult::Tx(data) => self.setup_send_data(&setup, data),
            SetupResult::Rx(len) if len == setup.length as usize && len != 0 => {
                // Receive some data.  TODO: is the length match guarenteed?
                self.setup = setup;
                chep_ctrl().write(
                    |w|w.control().VTRX().clear_bit().rx_valid(&chep)
                        .dtogrx(&chep, true) //.dtogtx(&chep, true)
                );
                ctrl_dbgln!("Set-up data rx armed {len}, CHEP = {:#x}",
                            chep_ctrl().read().bits());
            },
            SetupResult::Rx(_) => {
                ctrl_dbgln!("Set-up error");
                // Set STATTX to 1 (stall).  FIXME - clearing DTOGRX should not
                // be needed.  FIXME - do we really want to stall TX, or just
                // NAK?
                chep_ctrl().write(
                    |w|w.control().VTRX().clear_bit()
                        .stat_rx(&chep, 1).stat_tx(&chep, 1));
            },
        }
    }

    fn usb_initialize(&mut self) {
        *self = Self::default();
    }
}

impl ControlState {
    fn setup_rx_handler(&mut self, setup: &SetupHeader) -> SetupResult {
        // Cancel any pending set-address and set-up data.
        self.pending_address = None;
        self.setup_data = SetupResult::default();
        self.setup = SetupHeader::default();

        let bd = bd_control().rx.read();
        let len = bd >> 16 & 0x03ff;
        if len < 8 {
            ctrl_dbgln!("Rx setup len = {len} < 8");
            return SetupResult::error();
        }
        ctrl_dbgln!("Rx setup {:02x} {:02x} {:02x} {:02x} -> {}",
               setup.request_type, setup.request, setup.value_lo, setup.value_hi,
               setup.length);
        match (setup.request_type, setup.request) {
            (0x80, 0x00) => SetupResult::tx_data(&0u16), // Status.
            (0x21, 0x00) => {self.pending_dfu = true; SetupResult::no_data()}
            (0x00, 0x05) => self.set_address(setup.value_lo), // Set address.
            (0x80, 0x06) => match setup.value_hi { // Get descriptor.
                1 => SetupResult::tx_data(&DEVICE_DESC),
                2 => SetupResult::tx_data(&CONFIG0_DESC),
                3 => super::strings::get_descriptor(setup.value_lo),
                // 6 => setup_result(), // Device qualifier.
                desc => {
                    usb_dbgln!("Unsupported get descriptor {desc}");
                    SetupResult::error()
                }
            },
            (0x00, 0x09) => self.set_configuration(setup.value_lo),
            // We enable our only config when we get an address, so we can
            // just ACK the set interface message.
            (0x01, 0x0b) => SetupResult::no_data(), // Set interface

            (0xa1, 0x03) => if setup.index == INTF_DFU as u16 { // DFU
                SetupResult::tx_data(&[0u8, 100, 0, 0, 0, 0])
            }
            else {
                SetupResult::error()
            },

            (0x21, 0x20) => SetupResult::Rx(7), // Set Line Coding.
            (0xa1, 0x21) => crate::freak_serial::get_line_coding(),

            // We could flush buffers on a transition from line-down to line-up...
            (0x21, 0x22) => crate::freak_serial::set_control_line_state(setup.value_lo),
            _ => {
                usb_dbgln!("Unknown setup {:02x} {:02x} {:02x} {:02x} -> {}",
                           setup.request_type, setup.request,
                           setup.value_lo, setup.value_hi, setup.length);
                SetupResult::error()
            },
        }
    }

    /// Process just received setup OUT data.
    fn setup_rx_data(&mut self) -> bool {
        // First check that we really were expecting data.
        match (self.setup.request_type, self.setup.request) {
            (0x21, 0x20) => return crate::freak_serial::set_line_coding(),
            _ => return false,
        }
    }

    // Note that data should be a tx_data or no_data.
    fn setup_send_data(&mut self, setup: &SetupHeader, data: &'static [u8]) {
        self.setup_short = data.len() < setup.length as usize;
        let len = if self.setup_short {data.len()} else {setup.length as usize};
        ctrl_dbgln!("Setup response length = {} -> {}", data.len(), len);

        self.setup_next_data(&data[..len]);

        let chep = chep_ctrl().read();
        chep_ctrl().write(|w| w.control().VTRX().clear_bit().tx_valid(&chep));
    }

    /// Send the next data from the control state.  Return True if something sent,
    /// False if nothing sent.
    fn setup_next_data(&mut self, data: &'static [u8]) {
        let len = data.len();
        let is_short = len < 64;
        let len = if is_short {len} else {64};
        ctrl_dbgln!("Setup TX {len} of {}", data.len());

        // Copy the data into the control TX buffer.
        unsafe {copy_by_dest32(data.as_ptr(), CTRL_TX_BUF, len)};

        if len != data.len() || !is_short && self.setup_short {
            self.setup_data = SetupResult::Tx(&data[len..]);
        }
        else {
            self.setup_data = SetupResult::default();
        }

        // If the length is zero, then we are sending an ack.  If the length
        // is non-zero, then we are sending data and expect an ack.
        bd_control().tx.write(chep_bd_tx(CTRL_TX_OFFSET, len));
    }

    fn set_address(&mut self, address: u8) -> SetupResult {
        usb_dbgln!("Set addr received {address}");
        self.pending_address = Some(address);
        SetupResult::no_data()
    }

    fn do_set_address(address: u8) {
        usb_dbgln!("Set address apply {address}");
        let usb = unsafe {&*stm32h503::USB::ptr()};
        usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(address));
    }

    fn set_configuration(&mut self, config: u8) -> SetupResult {
        if config == 0 {
            usb_dbgln!("Set configuration 0 - ignore");
        }
        else if config != 1 {
            usb_dbgln!("Set configuration {config} - error");
            return SetupResult::error();
        }
        else {
            super::set_configuration(config);
            self.configured = true;
        }
        SetupResult::no_data()
    }
}
