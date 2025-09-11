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
use crate::vcell::{UCell, VCell};
use crate::usb_types::{setup_result, *};

const USB_SRAM_BASE: usize = 0x4001_6400;
const CTRL_RX_OFFSET: usize = 0x80;
const CTRL_TX_OFFSET: usize = 0xc0;
const BULK_RX_OFFSET: usize = 0x100;
const BULK_TX_OFFSET: usize = 0x200;
const INTR_TX_OFFSET: usize = 0x40;

const CTRL_RX_BUF: *const u8 = (USB_SRAM_BASE + CTRL_RX_OFFSET) as *const u8;
const CTRL_TX_BUF: *mut   u8 = (USB_SRAM_BASE + CTRL_TX_OFFSET) as *mut   u8;
const BULK_RX_BUF: *const u8 = (USB_SRAM_BASE + BULK_RX_OFFSET) as *const u8;
const BULK_TX_BUF: *mut   u8 = (USB_SRAM_BASE + BULK_TX_OFFSET) as *mut   u8;
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

// /// A pair of USBSRAM descriptors.  The low 16 bits of each value are the
// /// address offset, the high 16 bits contain the 10 bit number of bytes.
// ///  0..=15 : Address offset, must be 4-byte aligned.
// /// 16..=25 : Byte count
// /// 26..=30 : NUM_BLOCK
// /// 31      : BLSIZE (0 : NUM_BLOCK * 2, 1 : NUM_BLOCK * 32 + 32)
// /// For RX the byte count is written by HW.  For TX the byte count is written
// /// by SW and the block info appears to be ignored.
// #[repr(C)]
// struct Chepx {
//     /// TX pointer for single buffered, TOGGLE=0 pointer for double buffered.
//     tx: VCell<u32>,
//     /// RX pointer for single buffered, TOGGLE=1 pointer for double buffered.
//     rx: VCell<u32>,
// }

fn chep_bd() -> &'static [VCell<u32>; 16] {
    unsafe {&*(USB_SRAM_BASE as *const _)}
}

fn chep_ptr(d: u32) -> *mut u8 {
    (USB_SRAM_BASE + (d as usize & 0xffff)) as _
}

fn chep_len(d: u32) -> usize {
    d as usize >> 16 & 0x3ff
}

fn chep_offset(p: *const u8) -> u32 {
    (p as usize - USB_SRAM_BASE) as u32 & 0xffff
}
//const CHEP: &'static mut [Chep; 8] = unsafe {&mut * (0x40041200 as *mut _)};

const fn chep_block<const BLK_SIZE: u32>(offset: u32) -> u32 {
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
    block + offset
}

unsafe fn chep_to_slice(d: u32) -> &'static [u8] {
    unsafe {core::slice::from_raw_parts(chep_ptr(d), chep_len(d))}
}



pub fn init() {
    let rcc = unsafe {&*stm32h503::RCC::ptr()};
    let crs = unsafe {&*stm32h503::CRS::ptr()};
    let usb = unsafe {&*stm32h503::USB::ptr()};

    // Bring up the HSI48 clock.
    rcc.CR.modify(|_,w| w.HSI48ON().set_bit());
    while !rcc.CR.read().HSI48RDY().bit() {
    }

    rcc.APB1LENR.modify(|_,w| w.CRSEN().set_bit());

    // crs_sync_in_2 USB SOF selected - default.
    crs.CR.modify(|_,w| w.AUTOTRIMEN().set_bit().CEN().set_bit());

    rcc.APB2ENR.modify(|_,w| w.USBFSEN().set_bit());

    usb.CNTR.modify(|_,w| w.PDWN().clear_bit());
    // FIXME Wait t_startup....
    usb.CNTR.modify(|_,w| w.USBRST().clear_bit());
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

    crate::cpu::enable_interrupt(stm32h503::Interrupt::USB_FS);
    // Software trigger it to do the set-up....
    let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
    unsafe {nvic.stir.write(stm32h503::Interrupt::USB_FS as u32)};
}

fn usb_isr() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let cntr = usb.CNTR.read();
    if cntr.USBRST().bit() {
        // Reset....
        usb_initialize();
        return;
    }
    // Normal interrupt.
    let istr = usb.ISTR.read();
    // Write zero to the interrupt bits we wish to acknowledge.
    usb.ISTR.write(|w| w.bits(!istr.bits() & !0x37fc0));

    if istr.CTR().bit() {
        match istr.bits() & 31 {
            0  => control_tx_handler(),
            16 => control_rx_handler(),
            1  => serial_tx_handler(),
            18 => serial_rx_handler(),
            2  => interrupt_tx_handler(),
            _ => (),
        }
    }
}

fn control_tx_handler() {
}

fn control_rx_handler() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let chep = usb.CHEP0R.read();
    if chep.SETUP().bit() {
        if let Ok(data) = setup_rx_handler() {
            // TODO - what happens on repeated setup?
            // TODO - what do we do on zero size?
            // TODO - get the ack correct.
            // TODO - clamp length?
            // Copy the data into the control TX buffer.
            unsafe {core::ptr::copy_nonoverlapping(
                data.as_ptr(), CTRL_TX_BUF as *mut u8, data.len())};
            // Setup the control transfer.  CHEP 0, TX is first in BD.
            // If the length is zero, then we are sending an ack.  If the length
            // is non-zero, then we are sending data and expect an ack.  We
            // ignore the ack, and ignore whether or not we get one...
            chep_bd()[0].write((CTRL_TX_OFFSET + data.len() * 65536) as u32);
            // FIXME address.  FIXME DTOG.
            // FIXME bits to preserve?
            // FIXME status-out correct handling.
            let chep0 = usb.CHEP0R.read();
            usb.CHEP0R.write(
                |w| w.DEVADDR().bits(chep0.DEVADDR().bits()).UTYPE().bits(1)
                    .EPKIND().bit(data.len() == 0));
        }
    }
}

fn setup_rx_handler() -> SetupResult {
    let bd = chep_bd()[1].read();
    let len = bd >> 16 & 0x03ff;
    crate::vcell::barrier();
    // We own the buffer now so safe to take a reference.
    // Its easier to do exact length checks by hand, so just take the entire
    // buffer segment.
    let buffer = unsafe {core::slice::from_raw_parts(
        CTRL_RX_BUF as *const u8, 64)};
    if len < 8 {
        return Err(());
    }
    match (buffer[0], buffer[1]) {
        (0x80, 0x00) => setup_result(&0u16), // Status.
        (0x80, 0x06) => match buffer[2] { // Get descriptor.
            1 => setup_result(&DEVICE_DESC),
            2 => setup_result(&CONFIG0_DESC),
            3 => crate::usb_strings::get_descriptor(buffer[4]),
            // 6 => setup_result(), // Device qualifier.
            _ => Err(()),
        },
        (0x21, 0x00) => Err(()), // FIXME DFU detach.
        (0xa1, 0x03) => setup_result(&[0u8, 100, 0, 0, 0, 0]), // DFU status.
        (0x00, 0x05) => set_address(buffer[2]), // Set address.
        // We just enable our only config when we get an address, so we can
        // just ACK the set config / set intf messages.
        (0x00, 0x09) => setup_result(&()), // Set configuration
        (0x01, 0x0b) => setup_result(&()), // Set interface
        _ => Err(()),
    }
}

static PENDING_ADDRESS: UCell<Option<u8>> = Default::default();

fn set_address(address: u8) -> SetupResult {
    *unsafe {PENDING_ADDRESS.as_mut()} = Some(address);
    setup_result(&())
}

fn do_set_address() {
    let Some(address) = *PENDING_ADDRESS.as_ref() else {return};
    let usb = unsafe {&*stm32h503::USB::ptr()};
    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(address));
    // Set-up all cheps.  FIXME - what do we do on repeated set-address?
    // I think it officially just clears enabled ocnfig anyway....
    // TODO - in device mode do we need to set the address in each CHEP?
    usb.CHEP0R.write(|w| w.UTYPE().bits(1).DEVADDR().bits(address));
    // FIXME double buffered TX
    usb.CHEP1R.write(
        |w| w.UTYPE().bits(0).DEVADDR().bits(address).EA().bits(1));
    // FIXME double buffered TX
    usb.CHEP2R.write(
        |w| w.UTYPE().bits(0).DEVADDR().bits(address).EA().bits(1));
    usb.CHEP3R.write(
        |w| w.UTYPE().bits(3).DEVADDR().bits(address).EA().bits(2));
}

fn serial_tx_handler() {
    // CHEP 1, BD 2 & 3.  Clear VTTX?  What STATTX value do we want?
}

fn serial_rx_handler() {
    // CHEP 2, BD 4 & 5.
    // Re-arm the RX.  Clear VTRX.  What STATRX value do we want?
}

fn interrupt_tx_handler() {
    // CHEP 3, BD 6 & 7. [6 = TX, I think]
}

fn usb_initialize() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    crate::dbgln!("USB initialize...\n");

    usb.CNTR.write(
        |w|w.PDWN().clear_bit().USBRST().clear_bit()
            .RST_DCONM().set_bit().CTRM().set_bit());

    // FIXME statrx bits!!!
    usb.CHEP0R.write(|w| w.UTYPE().bits(1));

    // We want DTOGTX=1, DTOGRX=0? ... looks like HW manages that.
    //usb.CHEP0R.modify(
    //    |r,w| w.DTOGTX().bit(!r.DTOGTX().bit())
    //        .DTOGRX().bit(r.DTOGRX().bit()));
    // Set EF bit in DADDR.

    let chep = chep_bd();
    chep[0].write(chep_block::<64>(0x80)); // Control TX
    chep[1].write(chep_block::<64>(0xc0)); // Control RX
}

impl crate::cpu::VectorTable {
    pub const fn usb(&mut self) -> &mut Self {
        self.isr(stm32h503::Interrupt::USB_FS, usb_isr)
    }
}
