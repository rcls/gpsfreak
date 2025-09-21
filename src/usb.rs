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

use crate::cpu::{barrier, interrupt, nothing};
use crate::usb_strings::string_index;
use crate::usb_hw::*;
use crate::usb_types::{setup_result, SetupHeader, *};
use crate::vcell::UCell;

use stm32h503::Interrupt::USB_FS as INTERRUPT;

macro_rules!usb_dbgln  {($($tt:tt)*) => {if true  {crate::dbgln!($($tt)*)}};}
macro_rules!ctrl_dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}
macro_rules!srx_dbgln  {($($tt:tt)*) => {if true  {crate::dbgln!($($tt)*)}};}
macro_rules!stx_dbgln  {($($tt:tt)*) => {if true  {crate::dbgln!($($tt)*)}};}
macro_rules!intr_dbgln {($($tt:tt)*) => {if true  {crate::dbgln!($($tt)*)}};}

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
/// If the config descriptor gets larger than 63 bytes, then we need to send it
/// in multiple packets.
//const _: () = const {assert!(size_of::<Config1plus2>() < 64)};

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
        function_class    : 255, // FIXME
        function_sub_class: 255, // FIXME
        function_protocol : 255, // FIXME
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
        sub_type           : 2,         // Abstract Control Management
        cdc                : 0x0110,
    },
    call_mgmt: CallManagementDesc{
        length             : size_of::<CallManagementDesc>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 1,         // Call management
        capabilities       : 3,         // Call management [sic], data.
        data_interface     : 1,
    },
    acm_ctrl: AbstractControlDesc{
        length             : size_of::<AbstractControlDesc>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 2,         // Abstract Control Mgmt Functional Desc
        capabilities       : 0,         // FIXME
    },
    union_desc: UnionFunctionalDesc::<1>{
        length             : size_of::<UnionFunctionalDesc<1>>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 6,          // Union Functional Desc,
        control_interface  : 0,
        sub_interface      : [1],
    },
    endp0: EndpointDesc {
        length             : size_of::<EndpointDesc>() as u8,
        descriptor_type    : TYPE_ENDPOINT,
        endpoint_address   : 0x82,      // End point 2 IN
        attributes         : 3,         // Interrupt.
        max_packet_size    : 64,
        interval           : 4,
    },
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
    endp1: EndpointDesc {
        length             : size_of::<EndpointDesc>() as u8,
        descriptor_type    : TYPE_ENDPOINT,
        endpoint_address   : 0x81,      // EP 1 IN
        attributes         : 2,         // Bulk.
        max_packet_size    : 64,
        interval           : 1,
    },
    endp2: EndpointDesc {
        length             : size_of::<EndpointDesc>() as u8,
        descriptor_type    : TYPE_ENDPOINT,
        endpoint_address   : 1,         // EP 1 OUT.
        attributes         : 2,         // Bulk.
        max_packet_size    : 64,
        interval           : 1,
    },
};

#[derive_const(Default)]
struct ControlState {
    /// Address received in a SET ADDRESS.  On TX ACK, we apply this.
    pending_address: Option<u8>,
    /// Set-up data to send.  On TX ACK we send the next block.
    setup_data: Option<&'static [u8]>,
    /// Index of next byte(s) of set-up data to send.
    setup_next: usize,
    /// If set, the setup data is shorter than the requested data and we must
    /// end with a zero-length packet if needed.
    setup_short: bool,
}

static CONTROL_STATE: UCell<ControlState> = Default::default();

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

    // TODO - need to cope with USB bus reset.

    // For double buffering, two CHEPnR registers must be used, one for each
    // direction.

    // 7 chep registers....
    // 0 : Control
    // 1 : CDC ACM interrupt, barf.
    // 2 : CDC ACM IN (us to host)
    // 3 : CDC ACM OUT (host to us)
    // Don't bring up 1,2,3 till the set-up is done?

    usb_initialize();

    interrupt::set_priority(INTERRUPT, 0xff);
    interrupt::enable(INTERRUPT);
}

pub fn serial_tx_push32(word: u32) {
    //stx_dbgln!("USB TX push");
    let chep = chep_tx().read();
    if chep.STATTX().bits() == 0 {
        // stx_dbgln!("USB TX push drop {:#06x}", chep.bits());
        return;                         // Not initialized, or reset.
    }
    let toggle = chep.DTOGRX().bit(); // The toggle for software.
    let bd = bd_serial_tx(toggle).read();
    let ptr = chep_bd_ptr_mut(bd);
    let len = chep_bd_len(bd) as usize;
    if len > 60 {
        //stx_dbgln!("USB TX jammed");
        return;                         // We're full. Drop it.
    }
    // stx_dbgln!("USB TX push to {:#?} ({:#010x}) {len}, CHEP={:#06x}",
    //            ptr, bd, chep.bits());
    unsafe {*(ptr.wrapping_byte_add(len)) = word};
    bd_serial_tx(toggle).write(bd + 0x40000); // Update the length.
    // If we're full and USB TX is idle, then send the buffer.
    if len == 60 && chep.DTOGTX().bit() == toggle {
        chep_tx().write(|w| w.serial().DTOGRX().set_bit());
        bd_serial_tx_init(!toggle);
        stx_dbgln!("USB TX push CHEP now {:#06x} was {:#06x}",
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
    let ptr = chep_bd_ptr_mut(bd);
    let len = chep_bd_len(bd);
    if len <= 60 {
        let (word, bytes) = crate::gps_uart::serial_rx_flush();
        if bytes != 0 {
            // Store the new data.
            stx_dbgln!("USB TX flush to {:#?} ({:#010x}) {len}", ptr, bd);
            unsafe {*ptr.wrapping_byte_add(len as usize) = word};
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
    stx_dbgln!("SOF start TX, CHEP now {:#x} was {:#x}",
               chep_tx().read().bits(), chep.bits());
}

fn usb_isr() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let mut istr = usb.ISTR.read();
    ctrl_dbgln!("*** USB isr ISTR = {:#010x} FN={}",
                istr.bits(), usb.FNR.read().FN().bits());
    // Write zero to the interrupt bits we wish to acknowledge.
    usb.ISTR.write(|w| w.bits(!istr.bits() & !0x37fc0));

    if istr.SOF().bit() {
        start_of_frame();
    }

    // FIXME - is this CHEP or endpoint?
    while false && istr.CTR().bit() {
        match istr.bits() & 31 {
            0  => control_tx_handler(),
            16 => control_rx_handler(),
            1  => serial_tx_handler(),
            17 => serial_rx_handler(),
            2  => interrupt_handler(),
            _  => break,  // FIXME, this will hang!
        }
        istr = usb.ISTR.read();
    }
    while true && istr.CTR().bit() {
        match istr.bits() & 31 {
            0  => control_tx_handler(),
            16 => control_rx_handler(),
            1  => serial_rx_handler(),
            18 => serial_tx_handler(),
            3  => interrupt_handler(),
            _  => break,  // FIXME, this will hang!
        }
        istr = usb.ISTR.read();
    }

    if istr.RST_DCON().bit() {
        usb_initialize();
    }

    ctrl_dbgln!("CHEP0 now {:#010x}", chep_ctrl().read().bits());
    ctrl_dbgln!("***");
}

fn control_tx_handler() {
    let chep = chep_ctrl().read();
    ctrl_dbgln!("Control TX handler CHEP0 = {:#010x}", chep.bits());

    if !chep.VTTX().bit() {
        ctrl_dbgln!("Bugger!");
        return;
    }

    let control_state = unsafe {CONTROL_STATE.as_mut()};

    if setup_next_data() {
        return;
    }

    if let Some(address) = control_state.pending_address {
        do_set_address(address);
        control_state.pending_address = None;
        // Clear the VTTX bit.  Make sure STATRX is enabled.
        // FIXME - race with incoming?
    }

    chep_ctrl().write(
        |w|w.control().VTTX().clear_bit().rx_valid(&chep).dtogrx(&chep, false));
}

fn control_rx_handler() {
    let chep = chep_ctrl().read();
    ctrl_dbgln!("Control RX handler CHEP0 = {:#010x}", chep.bits());

    if !chep.VTRX().bit() {
        ctrl_dbgln!("Bugger");
        return;
    }

    if !chep.SETUP().bit() {
        ctrl_dbgln!("Control RX handler, CHEP0 = {:#010x}, non-setup", chep.bits());
        // Make sure we are ready for another read.
        chep_ctrl().write(
            |w|w.control().VTRX().clear_bit().rx_valid(&chep)
                .stat_tx(&chep, 2) // Nak
            );
        return;
    }

    // The USBSRAM only supports 32 bit accesses.  However, that only makes a
    // difference to the AHB bus on writes, not reads.  So access the setup
    // packet in place.
    barrier();
    let setup = unsafe {&*(CTRL_RX_BUF as *const SetupHeader)};
    // FIXME what about SETUP + OUT?
    if let Ok(data) = setup_rx_handler(setup) {
        setup_send_data(setup, data);
    }
    else {
        ctrl_dbgln!("Set-up error");
        // Set STATTX to 1 (stall).  FIXME - clearing DTOGRX should not be
        // needed.  FIXME - do we really want to stall TX, or just NAK?
        chep_ctrl().write(
            |w|w.control().VTRX().clear_bit()
                .rx_valid(&chep).stat_tx(&chep, 1)
                .DTOGRX().bit(chep.DTOGRX().bit()));
    };
}

fn setup_rx_handler(setup: &SetupHeader) -> SetupResult {
    // Cancel any pending set-address and set-up data.
    let cs = unsafe {CONTROL_STATE.as_mut()};
    cs.pending_address = None;
    cs.setup_data = None;
    cs.setup_next = 0;

    let bd = bd_control_rx().read();
    let len = bd >> 16 & 0x03ff;
    if len < 8 {
        ctrl_dbgln!("Rx setup len = {len} < 8");
        return Err(());
    }
    ctrl_dbgln!("Rx setup {:02x} {:02x} {:02x} {:02x} -> {}",
           setup.request_type, setup.request, setup.value_lo, setup.value_hi,
           setup.length);
    match (setup.request_type, setup.request) {
        (0x80, 0x00) => setup_result(&0u16), // Status.
        (0x80, 0x06) => match setup.value_hi { // Get descriptor.
            1 => setup_result(&DEVICE_DESC),
            2 => setup_result(&CONFIG0_DESC),
            3 => crate::usb_strings::get_descriptor(setup.value_lo),
            // 6 => setup_result(), // Device qualifier.
            _ => Err(()),
        },
        (0x00, 0x05) => set_address(setup.value_lo), // Set address.
        // We enable our only config when we get an address, so we can
        // just ACK the set interface message.
        (0x00, 0x09) => set_configuration(setup.value_lo),
        (0x01, 0x0b) => setup_result(&()), // Set interface

        // (0x21, 0x00) => Err(()), // FIXME DFU detach.
        // FIXME - 0xa1 0x03 appears to be GET_COMM_FEATURE?
        // (0xa1, 0x03) => setup_result(&[0u8, 100, 0, 0, 0, 0]), // DFU status.

        //(0x21, 0x20) => set_line_coding(setup),
        //(0xa1, 0x21) => get_line_coding(setup),

        (0x21, 0x22) => setup_result(&()), // Set Control Line State - useful?
        _ => {
            usb_dbgln!("Unknown setup {:02x} {:02x} {:02x} {:02x} -> {}",
                       setup.request_type, setup.request,
                       setup.value_lo, setup.value_hi, setup.length);
            Err(())
        },
    }
}

fn setup_send_data(setup: &SetupHeader, data: &'static [u8]) {
    let cs = unsafe {CONTROL_STATE.as_mut()};

    cs.setup_short = data.len() < setup.length as usize;
    let len = if cs.setup_short {data.len()} else {setup.length as usize};
    ctrl_dbgln!("Setup response length = {} -> {} [{:02x} {:02x}]",
                data.len(), len,
                if data.len() > 0 {unsafe{*data.as_ptr()}} else {0},
                unsafe{*CTRL_TX_BUF});

    cs.setup_data = Some(&data[..len]);
    cs.setup_next = 0;

    setup_next_data(); // FIXME - time for methods!
}

/// Send the next data from the control state.  Return True if something sent,
/// False if nothing sent.
fn setup_next_data() -> bool {
    let cs = unsafe {CONTROL_STATE.as_mut()};
    let Some(data) = cs.setup_data else {return false};

    let start = cs.setup_next;
    let len = data.len() - start;
    let is_short = len < 64;
    let len = if is_short {len} else {64};
    ctrl_dbgln!("Setup TX {len} @ {} of {}", start, data.len());

    let next = start + len;
    if next != data.len() || !is_short && cs.setup_short {
        cs.setup_next = next;
    }
    else {
        cs.setup_data = None;
    }

    // Copy the data into the control TX buffer.
    unsafe {copy_by_dest32(data[start..].as_ptr(), CTRL_TX_BUF, len)};
    // Setup the control transfer.  CHEP 0, TX is first in BD.
    // If the length is zero, then we are sending an ack.  If the length
    // is non-zero, then we are sending data and expect an ack.  We
    // ignore the ack, and ignore whether or not we get one...
    bd_control_tx().write(chep_bd_tx(CTRL_TX_OFFSET, len));
    let chep = chep_ctrl().read();
    chep_ctrl().write(|w| w.control().VTRX().clear_bit().tx_valid(&chep));

    true
}

fn set_address(address: u8) -> SetupResult {
    usb_dbgln!("Set addr received {address}");
    let cs = unsafe {CONTROL_STATE.as_mut()};
    cs.pending_address = Some(address);
    setup_result(&())
}

fn do_set_address(address: u8) {
    usb_dbgln!("Set address apply {address}");
    let usb = unsafe {&*stm32h503::USB::ptr()};
    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(address));
}

fn set_configuration(cfg: u8) -> SetupResult {
    usb_dbgln!("Set configuration {cfg}");

    clear_buffer_descs(); // FIXME - do we really want to do this?

    if cfg == 0 {
        let rx = chep_rx().read();
        chep_rx().write(|w| w.serial().init(&rx));
        let tx = chep_tx().read();
        chep_tx().write(|w| w.serial().init(&tx));
        let intr = chep_intr().read();
        chep_intr().write(|w| w.interrupt().init(&intr));
        return setup_result(&());
    }
    if cfg != 1 {
        return Err(());
    }

    // Serial RX.  USB OUT.  FIXME - this can stomp on an in-use buffer.
    let rx = chep_rx().read();
    chep_rx().write(|w| w.serial().init(&rx).rx_valid(&rx));
    // According to the datasheet, in double buffer mode, we should be able to
    // rely on STAT_TX=VALID and just use the toggles.
    // FIXME what happens if the buffer is in use?
    let tx = chep_tx().read();
    chep_tx().write(|w| w.serial().init(&tx).tx_valid(&tx));
    stx_dbgln!("CHEP TX inited to {:#06x}", chep_tx().read().bits());

    let intr = chep_intr().read();
    // 2 = NAK.
    chep_intr().write(|w| w.interrupt().init(&intr).stat_tx(&intr, 2));

    setup_result(&())
}

/// Initialize all the BD entries, except for the control ones.
fn clear_buffer_descs() {
    bd_serial_rx_init(true);
    bd_serial_rx_init(false);

    bd_serial_tx_init(false);
    bd_serial_tx_init(true);

    bd_interrupt().write(chep_block::<0>(INTR_TX_OFFSET));
}

fn usb_initialize() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    usb_dbgln!("USB initialize...");
    let cs = unsafe {CONTROL_STATE.as_mut()};
    cs.pending_address = None;

    usb.CNTR.write(
        |w|w.PDWN().clear_bit().USBRST().clear_bit()
            .RST_DCONM().set_bit().CTRM().set_bit().SOFM().set_bit());

    usb.BCDR.write(|w| w.DPPU_DPD().set_bit().DCDEN().set_bit());
    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(0));

    bd_control_tx().write(chep_block::<64>(CTRL_TX_OFFSET));
    bd_control_rx().write(chep_block::<64>(CTRL_RX_OFFSET));
    clear_buffer_descs();

    // TODO - do we need a bus barrier here?

    chep_ctrl().modify(|r,w| w.control().rx_valid(r));
}

fn serial_rx_handler() {
    srx_dbgln!("serial_rx_handler");
    let chep = chep_rx().read();
    if !chep.VTRX().bit() {
        srx_dbgln!("serial_rx_handler spurious!");
        return;
    }
    let toggle = chep.DTOGRX().bit();
    let bd = bd_serial_rx(toggle).read();
    // If we have data, and the GPS TX DMA is idle, then schedule the DMA now.
    let len = chep_bd_len(bd);
    if len != 0 && false { // !gps_tx_dma_active {
        crate::gps_uart::dma_tx(chep_bd_ptr(bd) as *const u8, len as usize);
    }
    // Now inspect the next bd.  If it is idle (marked by len==0), then enable
    // USB to receive it.
    let next = bd_serial_rx(!toggle).read();
    let go_next = chep_bd_len(next) != 0;
    chep_rx().write(|w| w.serial().VTRX().clear_bit().DTOGTX().bit(go_next));
}

/// Notification from the consumer that we have finished processing a buffer and
/// are ready for the next.
pub fn serial_rx_done(buf_end: *const u8) {
    srx_dbgln!("serial_rx_done");
    let chep = chep_rx().read();

    let toggle = chep.DTOGTX().bit();
    let bd_ref = bd_serial_rx(toggle);
    let mut bd = bd_ref.read();

    // FIXME - find a better way of error handling!
    let bd_end = chep_bd_ptr(bd).wrapping_add(chep_bd_len(bd) as usize);
    if chep_bd_ptr(bd) <= buf_end && bd_end == buf_end {
        // Mark len=0 in the BD to indicate that we're done with it.
        bd_ref.write(bd & !0x3ff0000);

        // If the RX is blocked, flip the TX toggle to allow the next RX.
        if chep.DTOGRX().bit() == toggle {
            chep_rx().write(|w| w.serial().DTOGTX().set_bit());
            bd = bd_serial_rx(!toggle).read();
        }
    }
    else {
        srx_dbgln!("Serial RX buf out of sync");
    }

    // Dispatch the next RX buffer if it is not empty.
    let len = chep_bd_len(bd);
    if len != 0 {
        crate::gps_uart::dma_tx(chep_bd_ptr(bd), len as usize);
    }
}

fn serial_tx_handler() {
    let chep = chep_tx().read();
    stx_dbgln!("serial_tx_handler {:#010x}", chep.bits());
    if chep.STATTX().bits() == 0 {
        stx_dbgln!("STX interrupt but STATTX==0, CHEP={:#06x}", chep.bits());
        return;                         // Not initialized or reset.
    }
    let toggle = chep.DTOGTX().bit();
    if !chep.VTTX().bit() || chep.DTOGRX().bit() != toggle {
        // The state doesn't make sense, it doesn't look like we just wrote
        // something.
        stx_dbgln!("serial tx spurious");
        return;
    }

    // Clear the length field.
    let bd = bd_serial_tx(toggle);
    bd.write(bd.read() & 0xffff);

    // CHEP 1, BD 2 & 3.  Clear VTTX?  What STATTX value do we want?
    // If the next BD is full, then schedule it now.  Else just clear the
    // interrupt.
    let next = bd_serial_tx(!toggle).read();
    let tx_next = chep_bd_len(next) == 64;
    chep_tx().write(|w| w.serial().VTTX().clear_bit().DTOGRX().bit(tx_next));
    if tx_next {
        bd_serial_tx_init(!toggle);
        stx_dbgln!("USB TX next CHEP {:#06x} was {:#06x}",
                   chep_tx().read().bits(), chep.bits());
    }
}

fn interrupt_handler() {
    intr_dbgln!("interrupt_tx_handler");
    // TODO - nothing here yet!
    let chep = chep_intr();
    chep.write(|w| w.interrupt().VTTX().clear_bit());
}

/// The USB SRAM is finicky about 32bit accesses, so we need to jump through
/// hoops to copy into it.  We assume that we are passed an aligned destination.
unsafe fn copy_by_dest32(s: *const u8, d: *mut u8, len: usize) {
    barrier();
    let mut s = s as *const u32;
    let mut d = d as *mut   u32;
    for _ in (0 .. len).step_by(4) {
        // We potentially overrun the source buffer by up to 3 bytes, which
        // should be harmless, as long as the buffer is not right at the end
        // of flash or RAM.
        unsafe {*d = core::ptr::read_unaligned(s)};
        d = d.wrapping_add(1);
        s = s.wrapping_add(1);
    }
    barrier();
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
