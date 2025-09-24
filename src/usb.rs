/// USB for GPS ref.
/// Endpoints:
/// 0 : Control, as always.
///   OUT: 64 bytes at 0x80 offset.
///   IN : 64 bytes at 0xc0 offset.  TODO - do we use both?
///   CHEP 0
/// 01, 81: CDC ACM data transfer, bulk, double buffer..
///   OUT (RX): 2 × 64 bytes at 0x100 offset. Chep 1.
///     DTOGRX==0 -> RX into RXTX buf. 1×2 + 1 = 3.
///     DTOGRX==1 -> RX into TXRX buf. 1×2 + 0 = 2.
///   IN  (TX): 8 x 64 bytes at 0x200 offset. Chep 2.
///     DTOGTX==0 -> TX from TXRX buf. 2×2 + 0 = 4.
///     DTOGTX==1 -> TX from RXTX buf. 2×2 + 1 = 5.
/// 82: CDC ACM interrupt IN (to host).
///   64 bytes at 0x40 offset.
///   CHEP 3

mod control;
mod hardware;
mod strings;
mod types;

use control::{CONTROL_STATE, ControlState};
use hardware::*;
use strings::string_index;
use types::*;

use crate::cpu::{barrier, interrupt, nothing};
use crate::dbgln;
use crate::vcell::UCell;

use stm32h503::Interrupt::USB_FS as INTERRUPT;

macro_rules!ctrl_dbgln {($($tt:tt)*) => {if true  {dbgln!($($tt)*)}};}
macro_rules!intr_dbgln {($($tt:tt)*) => {if true  {dbgln!($($tt)*)}};}
macro_rules!srx_dbgln  {($($tt:tt)*) => {if true  {dbgln!($($tt)*)}};}
macro_rules!stx_dbgln  {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}
macro_rules!usb_dbgln  {($($tt:tt)*) => {if true  {dbgln!($($tt)*)}};}
macro_rules!fast_dbgln {($($tt:tt)*) => {if false {dbgln!($($tt)*)}};}

pub(crate) use {ctrl_dbgln, intr_dbgln, srx_dbgln, stx_dbgln, usb_dbgln};

static DEVICE_DESC: DeviceDesc = DeviceDesc{
    length            : size_of::<DeviceDesc>() as u8,
    descriptor_type   : TYPE_DEVICE,
    usb               : 0x200,
    device_class      : 239, // Miscellaneous device
    device_sub_class  : 2, // Unknown
    device_protocol   : 1, // Interface association
    max_packet_size0  : 64,
    vendor            : 0xf055, // FIXME
    product           : 0xd448, // FIXME
    device            : 0x100,
    i_manufacturer    : string_index("Ralph"),
    i_product         : string_index("GPS REF"),
    i_serial          : string_index("0000"),
    num_configurations: 1,
};

#[repr(packed)]
#[allow(dead_code)]
struct Config1ACMCDCplus2 {
    config    : ConfigurationDesc,
    assoc     : InterfaceAssociation,
    interface0: InterfaceDesc,
    cdc_header: CDC_Header,
    call_mgmt : CallManagementDesc,
    acm_ctrl  : AbstractControlDesc,
    union_desc: UnionFunctionalDesc<1>,
    endp0     : EndpointDesc,
    interface1: InterfaceDesc,
    endp1     : EndpointDesc,
    endp2     : EndpointDesc,
}

/// Our main configuration descriptor.
static CONFIG0_DESC: Config1ACMCDCplus2 = Config1ACMCDCplus2{
    config: ConfigurationDesc{
        length             : size_of::<ConfigurationDesc>() as u8,
        descriptor_type    : TYPE_CONFIGURATION,
        total_length       : size_of::<Config1ACMCDCplus2>() as u16,
        num_interfaces     : 2,
        configuration_value: 1,
        i_configuration    : string_index("Single ACM"),
        attributes         : 0x80,      // Bus powered.
        max_power          : 200,       // 400mA
    },
    assoc: InterfaceAssociation{
        length            : size_of::<InterfaceAssociation>() as u8,
        descriptor_type   : TYPE_INTF_ASSOC,
        first_interface   : 0,
        interface_count   : 2,
        function_class    : 2,          // Communications
        function_sub_class: 2,          // Abstract (Modem [sic])
        function_protocol : 0,
        i_function        : string_index("CDC"),
    },
    interface0: InterfaceDesc{
        length             : size_of::<InterfaceDesc>() as u8,
        descriptor_type    : TYPE_INTERFACE,
        interface_number   : 0,
        alternate_setting  : 0,
        num_endpoints      : 1,
        interface_class    : 2,         // Communications
        interface_sub_class: 2,         // Abstract
        interface_protocol : 1,         // AT Commands [sic]
        i_interface        : string_index("CDC"),
    },
    cdc_header: CDC_Header{
        length             : size_of::<CDC_Header>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 0,         // CDC Header Functional Descriptor
        cdc                : 0x0110,
    },
    call_mgmt: CallManagementDesc{
        length             : size_of::<CallManagementDesc>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 1,         // Call management [sic]
        capabilities       : 3,         // Call management, data.
        data_interface     : 1,
    },
    acm_ctrl: AbstractControlDesc{
        length             : size_of::<AbstractControlDesc>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 2,         // Abstract Control Mgmt Functional Desc
        capabilities       : 6,         // "Line coding and serial state"
    },
    union_desc: UnionFunctionalDesc::<1>{
        length             : size_of::<UnionFunctionalDesc<1>>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 6,         // Union Functional Desc,
        control_interface  : 0,
        sub_interface      : [1],
    },
    endp0: EndpointDesc::new(0x82, 3, 64, 4), // IN 2, Interrupt.
    interface1: InterfaceDesc{
        length             : size_of::<InterfaceDesc>() as u8,
        descriptor_type    : TYPE_INTERFACE,
        interface_number   : 1,
        alternate_setting  : 0,
        num_endpoints      : 2,
        interface_class    : 10,        // CDC data
        interface_sub_class: 0,
        interface_protocol : 0,
        i_interface        : string_index("CDC DATA interface"),
    },
    endp1: EndpointDesc::new(0x81, 2, 64, 1), // IN 1, Bulk.
    endp2: EndpointDesc::new(0x01, 2, 64, 1), // OUT 82, Bulk.
};

pub fn init() {
    let crs   = unsafe {&*stm32h503::CRS  ::ptr()};
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let rcc   = unsafe {&*stm32h503::RCC  ::ptr()};
    let usb   = unsafe {&*stm32h503::USB  ::ptr()};

    // Bring up the HSI48 clock.
    rcc.CR.modify(|_,w| w.HSI48ON().set_bit());
    while !rcc.CR.read().HSI48RDY().bit() {
    }
    // Route the HSI48 to USB.
    rcc.CCIPR4.write(|w| w.USBFSSEL().B_0x3());

    // Configure pins (PA11, PA12).  (PA9 = VBUS?)
    gpioa.AFRH.modify(|_,w| w.AFSEL11().B_0xA().AFSEL12().B_0xA());
    gpioa.MODER.modify(|_,w| w.MODE11().B_0x2().MODE12().B_0x2());

    // Enable CRS and USB clocks.
    rcc.APB1LENR.modify(|_,w| w.CRSEN().set_bit());
    rcc.APB2ENR.modify(|_,w| w.USBFSEN().set_bit());

    // crs_sync_in_2 USB SOF selected - default.
    crs.CR.modify(|_,w| w.AUTOTRIMEN().set_bit());
    crs.CR.modify(|_,w| w.AUTOTRIMEN().set_bit().CEN().set_bit());

    usb.CNTR.modify(|_,w| w.PDWN().clear_bit());
    // Wait t_startup (1µs).
    for _ in 0 .. crate::cpu::CPU_FREQ / 2000000 {
        nothing();
    }
    usb.CNTR.write(|w| w.PDWN().clear_bit().USBRST().clear_bit());

    // Clear any spurious interrupts.
    usb.ISTR.write(|w| w);

    usb_initialize(unsafe {CONTROL_STATE.as_mut()});

    interrupt::enable_priority(INTERRUPT, 0xff);
}

pub fn serial_tx_push32(word: u32) {
    let chep = chep_tx().read();
    if chep.STATTX().bits() == 0 {
        return;                         // Not initialized, or reset.
    }
    let toggle = chep.DTOGRX().bit();   // The toggle for software.
    let bd = bd_serial_tx(toggle).read();
    let len = chep_bd_len(bd) as usize;
    if len > 60 {
        return;                         // We're full. Drop it.
    }

    unsafe {*chep_bd_tail(bd) = word};
    bd_serial_tx(toggle).write(bd + 0x40000); // Update the length.
    // If we're full and USB TX is idle, then send the buffer.
    if len == 60 && chep.DTOGTX().bit() == toggle {
        chep_tx().write(|w| w.serial().DTOGRX().set_bit());
        bd_serial_tx_init(!toggle);
        fast_dbgln!("USB TX push CHEP now {:#06x} was {:#06x}",
                    chep_tx().read().bits(), chep.bits());
    }
    else if len == 60 {
        stx_dbgln!("USB TX push now full CHEP {:#06x}", chep.bits());
    }
}

/// On a start-of-frame interrupt, if the serial IN end-point is idle, we
/// push through any pending data.  Hopefully quickly enough for the actual
/// IN request.
fn start_of_frame() {
    // If serial TX is idle, then push through any pending data.
    let chep = chep_tx().read();
    if chep.STATTX().bits() == 0 {
        return;                         // Not initialized or reset.
    }
    let toggle = chep.DTOGRX().bit();   // Software toggle.
    if chep.DTOGTX().bit() != toggle {
        return;                         // Already scheduled, just let it go.
    }
    let bd_ref = bd_serial_tx(toggle);
    let bd = bd_ref.read();
    let len = chep_bd_len(bd);
    if len <= 60 {
        let (word, bytes) = crate::gps_uart::serial_rx_flush();
        if bytes != 0 {
            // Store the new data.
            fast_dbgln!("USB TX flush {bytes}, BD {bd}");
            unsafe {*chep_bd_tail(bd) = word};
            bd_ref.write(bd + bytes as u32 * 65536);
        }
        else if len == 0 {
            return;                     // Nothing to do.
        }
    }
    barrier();
    // Start the TX.
    chep_tx().write(|w| w.serial().DTOGRX().set_bit());
    bd_serial_tx_init(!toggle);
    fast_dbgln!("SOF start TX, CHEP now {:#x} was {:#x}",
               chep_tx().read().bits(), chep.bits());
}

fn usb_isr() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let mut istr = usb.ISTR.read();
    let not_only_sof = istr.RST_DCON().bit() || istr.CTR().bit();
    if not_only_sof {
        fast_dbgln!("*** USB isr ISTR = {:#010x} FN={}",
                    istr.bits(), usb.FNR.read().FN().bits());
    }
    // Write zero to the interrupt bits we wish to acknowledge.
    usb.ISTR.write(|w| w.bits(!istr.bits() & !0x37fc0));

    if istr.SOF().bit() {
        start_of_frame();
    }

    if istr.RST_DCON().bit() {
        usb_initialize(unsafe {CONTROL_STATE.as_mut()});
    }

    while istr.CTR().bit() {
        match istr.bits() & 31 {
            0  => unsafe {CONTROL_STATE.as_mut()}.control_tx_handler(),
            16 => unsafe {CONTROL_STATE.as_mut()}.control_rx_handler(),
            17 => serial_rx_handler(),
            2  => serial_tx_handler(),
            3  => interrupt_handler(),
            _  => {
                dbgln!("Bugger endpoint?, ISTR = {:#010x}", istr.bits());
                break;  // FIXME, this will hang!
            },
        }
        istr = usb.ISTR.read();
    }

    if not_only_sof {
        fast_dbgln!("CHEP0 now {:#010x}\n***", chep_ctrl().read().bits());
    }
}

fn set_configuration(cfg: u8) -> SetupResult {
    usb_dbgln!("Set configuration {cfg}");

    clear_buffer_descs();

    if cfg == 0 {
        let rx = chep_rx().read();
        chep_rx().write(|w| w.serial().init(&rx));
        let tx = chep_tx().read();
        chep_tx().write(|w| w.serial().init(&tx));
        let intr = chep_intr().read();
        chep_intr().write(|w| w.interrupt().init(&intr));
        return SetupResult::no_data();
    }
    if cfg != 1 {
        return SetupResult::error();
    }

    // Serial RX.  USB OUT.  Set TOGRX=0. TOGTX=1 to kick off the first read.
    let rx = chep_rx().read();
    chep_rx().write(
        |w|w.serial().init(&rx).rx_valid(&rx)
            .dtogrx(&rx, false).dtogtx(&rx, true));
    // According to the datasheet, in double buffer mode, we should be able to
    // rely on STAT_TX=VALID and just use the toggles.
    // FIXME what happens if the buffer is in use?
    let tx = chep_tx().read();
    chep_tx().write(|w| w.serial().init(&tx).tx_valid(&tx));
    stx_dbgln!("CHEP TX inited to {:#06x}", chep_tx().read().bits());

    let intr = chep_intr().read();
    // 2 = NAK.
    chep_intr().write(|w| w.interrupt().init(&intr).stat_tx(&intr, 2));

    SetupResult::no_data()
}

/// Initialize all the BD entries, except for the control ones.
fn clear_buffer_descs() {
    bd_serial_rx_init(true);
    bd_serial_rx_init(false);

    bd_serial_tx_init(false);
    bd_serial_tx_init(true);

    bd_interrupt().write(chep_block::<0>(INTR_TX_OFFSET));
}

fn usb_initialize(cs: &mut ControlState) {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    usb_dbgln!("USB initialize...");

    cs.initialize();

    usb.CNTR.write(
        |w|w.PDWN().clear_bit().USBRST().clear_bit()
            .RST_DCONM().set_bit().CTRM().set_bit().SOFM().set_bit());

    usb.BCDR.write(|w| w.DPPU_DPD().set_bit().DCDEN().set_bit());
    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(0));

    bd_control_tx().write(chep_block::<64>(CTRL_TX_OFFSET));
    bd_control_rx().write(chep_block::<64>(CTRL_RX_OFFSET));
    clear_buffer_descs();

    let chep = chep_ctrl().read();
    chep_ctrl().write(|w| w.control().rx_valid(&chep));
}

fn serial_rx_handler() {
    let chep = chep_rx().read();
    srx_dbgln!("serial_rx_handler, CHEP = {:#06x}", chep.bits());
    if !chep.VTRX().bit() {
        srx_dbgln!("serial_rx_handler spurious! CHEP = {:#06x}", chep.bits());
        return;
    }

    // On the RX interrupt, the two toggles should be identical, check that.
    let toggle = chep.DTOGRX().bit();
    if toggle != chep.DTOGTX().bit() {
        chep_rx().write(|w| w.serial().VTRX().clear_bit());
        usb_dbgln!(
            "serial_rx_handler toggle mismatch, CHEP={:#06x} was {:#06x}",
            chep_rx().read().bits(), chep.bits());
        return;
    }

    if !serial_rx_unblock(toggle) {
        // Clear the VTRX anyway.
        chep_rx().write(|w| w.serial().VTRX().clear_bit());
    }
}

/// Notification from the consumer that we have finished processing a buffer and
/// are ready for the next.
pub fn serial_rx_done(toggle: bool) {
    let chep = chep_rx().read();

    // FIXME - this can get called after a configure and poison new stuff!
    srx_dbgln!("serial_rx_done, toggle={toggle}, CHEP={:#06x}", chep.bits());
    assert!(chep.DTOGTX().bit() != toggle);

    if chep.DTOGRX().bit() == chep.DTOGTX().bit() {
        // We are jammed, waiting for us.  Kick things off.
        serial_rx_unblock(!toggle);
    }
}

/// We must have the two toggles equal when this is called.  If VTRX is set,
/// then this is what is needed from the ISR.  So we just clear it, even if
/// not called from the USB isr.
fn serial_rx_unblock(toggle: bool) -> bool {
    // If we have data, start the DMA, else just start the next receive.
    let bd = bd_serial_rx(toggle).read();
    let len = chep_bd_len(bd);
    if len != 0 {
        if !crate::gps_uart::dma_tx(chep_bd_ptr(bd), len as usize, toggle) {
            srx_dbgln!("Unblock. DMA still busy, wait.");
            return false;
        }
    }

    chep_rx().write(|w| w.serial().VTRX().clear_bit().DTOGTX().bit(true));
    srx_dbgln!("Unblock. DMA {len} bytes, continue CHEP = {:#06x}",
               chep_rx().read().bits());
    true
}

fn serial_tx_handler() {
    let chep = chep_tx().read();
    stx_dbgln!("serial_tx_handler CHEP = {:#06x}", chep.bits());
    if chep.STATTX().bits() == 0 {
        stx_dbgln!("STX interrupt but STATTX==0");
        chep_tx().write(|w| w.serial().VTTX().clear_bit());
        return;                         // Not initialized or reset.
    }
    let toggle = chep.DTOGTX().bit();
    if !chep.VTTX().bit() {
        stx_dbgln!("serial tx spurious");
        return;
    }

    if chep.DTOGRX().bit() != toggle {
        // The state doesn't make sense, it doesn't look like we just wrote
        // something.
        stx_dbgln!("serial tx no toggle, CHEP={:#06x}", chep.bits());
        chep_tx().write(|w| w.serial().VTTX().clear_bit());
        return;
    }

    // Clear the length field of the buffer we just sent.
    let bd = bd_serial_tx(!toggle);
    bd.write(bd.read() & 0xffff);

    // CHEP 1, BD 2 & 3.  Clear VTTX?  What STATTX value do we want?
    // If the next BD is full, then schedule it now.  Else just clear the
    // interrupt.
    let next = bd_serial_tx(toggle).read();
    let tx_next = chep_bd_len(next) == 64;
    chep_tx().write(|w| w.serial().VTTX().clear_bit().DTOGRX().bit(tx_next));
    if tx_next {
        bd_serial_tx_init(toggle);
        stx_dbgln!("USB TX next CHEP {:#06x} was {:#06x}",
                   chep_tx().read().bits(), chep.bits());
    }
}

/// This handles USB interrupt pipe VTTX not CPU interrupts!
fn interrupt_handler() {
    // TODO - nothing here yet!
    let chep = chep_intr().read();
    chep_intr().write(|w| w.interrupt().VTTX().clear_bit());
    intr_dbgln!("interrupt_tx_handler CHEP now {:#06x} was {:#06x}",
                chep_intr().read().bits(), chep.bits());
}

fn set_control_line_state(_value: u8) -> SetupResult {
    usb_tx_interrupt();
    SetupResult::no_data()
}

fn set_line_coding() -> bool {
    let line_coding: LineCoding = unsafe {
        core::mem::transmute_copy (
            &* (CTRL_RX_BUF as *const (u32, u32))
        )
    };
    ctrl_dbgln!("USB Set Line Coding, Baud = {}", line_coding.dte_rate);
    // We ignore everything except the baud rate.
    crate::gps_uart::set_baud_rate(line_coding.dte_rate)
}

fn get_line_coding() -> SetupResult {
    ctrl_dbgln!("USB Get Line Coding");
    static LINE_CODING: UCell<LineCoding> = Default::default();
    let lc = unsafe {LINE_CODING.as_mut()};
    *lc = LineCoding {
        dte_rate: crate::gps_uart::get_baud_rate(),
        char_format: 0, parity_type: 0, data_bits: 8};
    SetupResult::tx_data(lc)
}

fn usb_tx_interrupt() {
    intr_dbgln!("Sending USB interrupt");
    // Just send a canned response, because USB sucks.
    #[allow(dead_code)]
    #[repr(C)]
    struct LineState{header: SetupHeader, state: u16}
    static LINE_STATE: LineState = LineState{
        header: SetupHeader {
            request_type: 0xa1, request: 0x20, value_lo: 3,
            value_hi: 0, index: 0, length: 2},
        state: 3,
    };
    unsafe {copy_by_dest32(&LINE_STATE as *const _ as *const _,
                           INTR_TX_BUF, size_of::<LineState>())};
    barrier();
    bd_interrupt().write(chep_bd_tx(INTR_TX_OFFSET, size_of::<LineState>()));
    let chep = chep_intr().read();
    chep_intr().write(|w| w.interrupt().tx_valid(&chep));
    intr_dbgln!("INTR CHEP now {:#06x} was {:#06x}",
                chep_intr().read().bits(), chep.bits());
}

impl crate::cpu::VectorTable {
    pub const fn usb(&mut self) -> &mut Self {
        self.isr(INTERRUPT, usb_isr)
    }
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == usb_isr);
}
