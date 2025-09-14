/// USB for GPS ref.
/// Endpoints:
/// 0 : Control, as always.
///   OUT: 64 bytes at 0x80 offset.
///   IN : 64 bytes at 0xc0 offset.  TODO - do we use both?
///   CHEP 0
/// 01, 81: CDC ACM data transfer, bulk.
///   OUT: 2 × 64 bytes at 0x100 offset.
///   IN : 8 x 64 bytes at 0x200 offset?
///   CHEP 1,2
/// 82: CDC ACM interrupt IN (to host).
///   64 bytes at 0x40 offset.
///   CHEP 3

use crate::usb_strings::string_index;
use crate::usb_types::{setup_result, SetupHeader, *};
use crate::vcell::{UCell, VCell};

use crate::dbgln;
use stm32h503::Interrupt::USB_FS as INTERRUPT;

const USB_SRAM_BASE: usize = 0x4001_6400;
const CTRL_RX_OFFSET: usize = 0xc0;
const CTRL_TX_OFFSET: usize = 0x80;
#[allow(dead_code)]
const BULK_RX_OFFSET: usize = 0x100;
#[allow(dead_code)]
const BULK_TX_OFFSET: usize = 0x200;
#[allow(dead_code)]
const INTR_TX_OFFSET: usize = 0x40;

const CTRL_RX_BUF: *const u8 = (USB_SRAM_BASE + CTRL_RX_OFFSET) as *const u8;
const CTRL_TX_BUF: *mut   u8 = (USB_SRAM_BASE + CTRL_TX_OFFSET) as *mut   u8;
#[allow(dead_code)]
const BULK_RX_BUF: *const u8 = (USB_SRAM_BASE + BULK_RX_OFFSET) as *const u8;
#[allow(dead_code)]
const BULK_TX_BUF: *mut   u8 = (USB_SRAM_BASE + BULK_TX_OFFSET) as *mut   u8;
#[allow(dead_code)]
const INTR_TX_BUF: *mut   u8 = (USB_SRAM_BASE + INTR_TX_OFFSET) as *mut   u8;

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
struct Config1plus2 {
    config    : ConfigurationDesc,
    assoc     : InterfaceAssociation,
    interface0: InterfaceDesc,
    endp0     : EndpointDesc,
    interface1: InterfaceDesc,
    endp1     : EndpointDesc,
    endp2     : EndpointDesc,
}
/// If the config descriptor gets larger than 63 bytes, then we need to send it
/// in multiple packets.
const _: () = const {assert!(size_of::<Config1plus2>() < 64)};

/// Our main configuration descriptor.
static CONFIG0_DESC: Config1plus2 = Config1plus2{
    config: ConfigurationDesc{
        length             : size_of::<ConfigurationDesc>() as u8,
        descriptor_type    : TYPE_CONFIGURATION,
        total_length       : 1234, // FIXME.
        num_interfaces     : 2,
        configuration_value: 1,
        i_configuration    : string_index("Single ACM"),
        attributes         : 0x80, // Bus powered.
        max_power          : 200, // 400mA
    },
    assoc: InterfaceAssociation{
        length            : size_of::<InterfaceAssociation>() as u8,
        descriptor_type   : TYPE_INTF_ASSOC,
        first_interface   : 0,
        interface_count   : 2,
        function_class    : 255, // FIXME
        function_sub_class: 255, // FIXME
        function_protocol : 255, // FIXME
        i_function        : string_index("CDC not yet"),
    },
    interface0: InterfaceDesc{
        length             : size_of::<InterfaceDesc>() as u8,
        descriptor_type    : TYPE_INTERFACE,
        interface_number   : 0,
        alternate_setting  : 0,
        num_endpoints      : 1,
        interface_class    : 255, // FIXME
        interface_sub_class: 255, // FIXME
        interface_protocol : 255, // FIXME
        i_interface        : string_index("CDC not yet"),
        // CDC header to come!
    },
    endp0: EndpointDesc {
        length             : size_of::<EndpointDesc>() as u8,
        descriptor_type    : TYPE_ENDPOINT,
        endpoint_address   : 0x82, // EP 2 in
        attributes         : 3, // Interrupt.
        max_packet_size    : 64,
        interval           : 4,
    },
    interface1: InterfaceDesc{
        length             : size_of::<InterfaceDesc>() as u8,
        descriptor_type    : TYPE_INTERFACE,
        interface_number   : 1,
        alternate_setting  : 0,
        num_endpoints      : 2,
        interface_class    : 10, // CDC data
        interface_sub_class: 0,
        interface_protocol : 0,
        i_interface        : string_index("CDC DATA interface"),
        // FIXME CDC header to come!
    },
    endp1: EndpointDesc {
        length             : size_of::<EndpointDesc>() as u8,
        descriptor_type    : TYPE_ENDPOINT,
        endpoint_address   : 0x81, // EP 2 in
        attributes         : 2, // Bulk.
        max_packet_size    : 64,
        interval           : 1,
    },
    endp2: EndpointDesc {
        length             : size_of::<EndpointDesc>() as u8,
        descriptor_type    : TYPE_ENDPOINT,
        endpoint_address   : 2, // EP 2 out.
        attributes         : 2, // Bulk.
        max_packet_size    : 64,
        interval           : 1,
    },
};

fn chep_bd() -> &'static [VCell<u32>; 16] {
    unsafe {&*(USB_SRAM_BASE as *const _)}
}

const fn chep_block<const BLK_SIZE: usize>(offset: usize) -> u32 {
    assert!(offset + BLK_SIZE <= 2048);
    let block = if BLK_SIZE == 1023 {
        0xfc000000
    }
    else if BLK_SIZE % 32 == 0 && BLK_SIZE > 0 && BLK_SIZE <= 1024 {
        BLK_SIZE / 32 + 31 << 26
    }
    else if BLK_SIZE % 2 == 0 && BLK_SIZE < 64 {
        BLK_SIZE / 2 << 26
    }
    else {
        panic!();
    };
    (block + offset) as u32
}

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
    gpioa.AFRH.modify(|_,w| w.AFSEL11().bits(10).AFSEL12().bits(10));
    gpioa.MODER.modify(|_,w| w.MODE11().B_0x2().MODE12().B_0x2());

    // Enable CRS and USB clocks.
    rcc.APB1LENR.modify(|_,w| w.CRSEN().set_bit());
    rcc.APB2ENR.modify(|_,w| w.USBFSEN().set_bit());

    // crs_sync_in_2 USB SOF selected - default.
    crs.CR.modify(|_,w| w.AUTOTRIMEN().set_bit());
    crs.CR.modify(|_,w| w.AUTOTRIMEN().set_bit().CEN().set_bit());

    usb.CNTR.modify(|_,w| w.PDWN().clear_bit());
    // Wait t_startup (1µs).
    for _ in 0..48 {
        crate::vcell::nothing();
    }
    usb.CNTR.write(
        |w|w.PDWN().clear_bit().USBRST().clear_bit()
            .RST_DCONM().set_bit().CTRM().set_bit());

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

    crate::cpu::enable_interrupt(INTERRUPT);
}

fn usb_isr() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let istr = usb.ISTR.read();
    dbgln!("*** USB isr ISTR = {:#010x}", istr.bits());
    // Write zero to the interrupt bits we wish to acknowledge.
    usb.ISTR.write(|w| w.bits(!istr.bits() & !0x37fc0));
    if istr.RST_DCON().bit() {
        usb_initialize();
    }

    match istr.bits() & 15 {
        0 => control_handler(),
        1 => serial_tx_handler(),
        2 => serial_rx_handler(),
        3 => interrupt_handler(),
        _ => (),
    }
    dbgln!("***");
}

fn control_handler() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let chep = usb.CHEPR[0].read();
    dbgln!("Control handler CHEP0 = {:#010x}, FNR = {}",
           chep.bits(), usb.FNR.read().FN().bits());

    if chep.VTTX().bit() {
        dbgln!("Control VTTX!");
        let address = unsafe {PENDING_ADDRESS.as_mut()};
        if let Some(a) = *address {
            do_set_address(a);
            *address = None;
        }
    }
    if !chep.VTRX().bit() {
        dbgln!("Control no VTRX :-(");
    }
    else if chep.SETUP().bit() {
        crate::vcell::barrier();
        let setup = unsafe {&*(CTRL_RX_BUF as *const SetupHeader)};
        if let Ok(data) = setup_rx_handler(setup) {
            // TODO - what do we do on zero size?
            // TODO - get the ack correct.
            // Copy the data into the control TX buffer.
            let len = data.len().min(setup.length as usize);
            jfc_bytes(data.as_ptr(), CTRL_TX_BUF, len);
            dbgln!("Setup response length = {} -> {} [{:02x} {:02x}]",
                   data.len(), len,
                   if data.len() > 0 {unsafe{*data.as_ptr()}} else {0},
                   unsafe{*CTRL_TX_BUF});

            // Setup the control transfer.  CHEP 0, TX is first in BD.
            // If the length is zero, then we are sending an ack.  If the length
            // is non-zero, then we are sending data and expect an ack.  We
            // ignore the ack, and ignore whether or not we get one...
            chep_bd()[0].write((CTRL_TX_OFFSET + len * 65536) as u32);
            // FIXME address.  FIXME DTOG.
            // FIXME bits to preserve?
            // FIXME status-out correct handling.
            let chep = usb.CHEPR[0].read();
            usb.CHEPR[0].write(
                |w|w.DEVADDR().bits(chep.DEVADDR().bits()).UTYPE().bits(1)
                    .STATTX().bits(chep.STATTX().bits() ^ 3)
                    .EPKIND().bit(data.len() == 0));
        }
        else {
            dbgln!("Set-up error");
            // Set STATTX to 1 (stall)
            let chep = usb.CHEPR[0].read();
            usb.CHEPR[0].write(
                |w|w.DEVADDR().bits(chep.DEVADDR().bits()).UTYPE().bits(1)
                    .STATTX().bits(chep.STATTX().bits() ^ 1));
        }
    }
    else {
        dbgln!("Control RX handler, CHEP0 = {:#010x}, non-setup", chep.bits());
    }
    let chep = usb.CHEPR[0].read();
    if chep.STATTX().bits() != 3 {
        // Leave DTOGRX=0, STATRX=3.
        usb.CHEPR[0].write(
            |w|w.DEVADDR().bits(chep.DEVADDR().bits()).EA().bits(0)
                .UTYPE().bits(1).STATRX().bits(chep.STATRX().bits() ^ 3)
                .DTOGRX().bit(chep.DTOGRX().bit())
                //.DTOGTX().bit(chep.DTOGTX().bit())
            );
    }

    dbgln!("CHEP0 now {:#010x}", usb.CHEPR[0].read().bits());
}

fn setup_rx_handler(setup: &SetupHeader) -> SetupResult {
    let bd = chep_bd()[1].read();
    let len = bd >> 16 & 0x03ff;
    if len < 8 {
        dbgln!("Rx setup len = {len} < 8");
        return Err(());
    }
    dbgln!("Rx setup {:02x} {:02x} {:02x} {:02x} -> {}",
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
        (0x21, 0x00) => Err(()), // FIXME DFU detach.
        (0xa1, 0x03) => setup_result(&[0u8, 100, 0, 0, 0, 0]), // DFU status.
        (0x00, 0x05) => set_address(setup.value_lo), // Set address.
        // We just enable our only config when we get an address, so we can
        // just ACK the set config / set intf messages.
        (0x00, 0x09) => setup_result(&()), // Set configuration
        (0x01, 0x0b) => setup_result(&()), // Set interface
        _ => Err(()),
    }
}

static PENDING_ADDRESS: UCell<Option<u8>> = Default::default();

fn set_address(address: u8) -> SetupResult {
    dbgln!("Set addr rqst {address}");
    *unsafe {PENDING_ADDRESS.as_mut()} = Some(address);
    setup_result(&())
}

fn do_set_address(address: u8) {
    dbgln!("Set address to {address}");
    let usb = unsafe {&*stm32h503::USB::ptr()};
    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(address));
    // Set-up all cheps.  FIXME - what do we do on repeated set-address?
    // I think it officially just clears enabled config anyway....
    // TODO - in device mode do we need to set the address in each CHEP?
    // FIXME - it seems that it doesn't even get stored!
    usb.CHEPR[0].write(
        |w|w.UTYPE().bits(1).DEVADDR().bits(address));
    // FIXME proper double buffered TX toggles etc.
    // STATTX==0 : Nothing to send yet.
    // Set DTOGTX to 1 and DTOGRX to 0.
    usb.CHEPR[1].modify(
        |r,w| w.UTYPE().bits(0).EPKIND().set_bit().STATTX().bits(0)
            .DEVADDR().bits(address).EA().bits(1)
            .DTOGTX().bit(!r.DTOGTX().bit()).DTOGRX().bit(r.DTOGRX().bit()));
    // FIXME proper double buffer RX
    usb.CHEPR[2].modify(
        |r,w| w.UTYPE().bits(0).EPKIND().set_bit().STATRX().bits(3)
            .DEVADDR().bits(address).EA().bits(1)
            .DTOGTX().bit(r.DTOGTX().bit()).DTOGRX().bit(!r.DTOGRX().bit()));
    usb.CHEPR[3].modify(
        |r,w| w.UTYPE().bits(3).STATTX().bits(3)
            .DEVADDR().bits(address).EA().bits(2)
            .DTOGTX().bit(!r.DTOGTX().bit()));
    dbgln!("After set address, CHEP0 = {:#010x}", usb.CHEPR[0].read().bits());
}

fn serial_tx_handler() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let chep = usb.CHEPR[1].read();
    dbgln!("serial_tx_handler {:#010x}", chep.bits());

    // CHEP 1, BD 2 & 3.  Clear VTTX?  What STATTX value do we want?
}

fn serial_rx_handler() {
    dbgln!("serial_rx_handler")
    // CHEP 2, BD 4 & 5.
    // Re-arm the RX.  Clear VTRX.  What STATRX value do we want?
}

fn interrupt_handler() {
    dbgln!("interrupt_tx_handler")
    // CHEP 3, BD 6 & 7. [6 = TX, I think]
}

fn usb_initialize() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    crate::dbgln!("USB initialize...");
    unsafe {*PENDING_ADDRESS.as_mut() = None};

    usb.CNTR.write(
        |w|w.PDWN().clear_bit().USBRST().clear_bit()
            .RST_DCONM().set_bit().CTRM().set_bit());

    usb.BCDR.write(|w| w.DPPU_DPD().set_bit().DCDEN().set_bit());

    usb.CHEPR[0].write(|w| w.UTYPE().bits(1).STATRX().bits(3));
    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(0));

    // We want DTOGTX=1, DTOGRX=0? ... looks like HW manages that.
    //usb.CHEP0R.modify(
    //    |r,w| w.DTOGTX().bit(!r.DTOGTX().bit())
    //        .DTOGRX().bit(r.DTOGRX().bit()));
    // Set EF bit in DADDR.

    let bd = chep_bd();
    crate::vcell::barrier();
    bd[0].write(chep_block::<64>(CTRL_TX_OFFSET)); // Control TX
    bd[1].write(chep_block::<64>(CTRL_RX_OFFSET)); // Control RX
    crate::vcell::barrier();
}

impl crate::cpu::VectorTable {
    pub const fn usb(&mut self) -> &mut Self {
        self.isr(INTERRUPT, usb_isr)
    }
}

fn jfc_bytes(s: *const u8, d: *mut u8, len: usize) {
    crate::vcell::barrier();
    // The USBRAM must be accessed with 32 bit accesses, just to make life fun.
    // We always have an aligned dest.
    let mut s = s as *const u32;
    let mut d = d as *mut   u32;
    for _ in (0 .. len).step_by(4) {
        unsafe {*d = core::ptr::read_unaligned(s)};
        d = d.wrapping_add(1);
        s = s.wrapping_add(1);
    }
    crate::vcell::barrier();
}

#[test]
fn check_isr() {
    assert!(crate::VECTORS.isr[INTERRUPT as usize] == usb_isr);
}
