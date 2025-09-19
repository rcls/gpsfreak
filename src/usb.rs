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
use crate::usb_types::{setup_result, SetupHeader, *};
use crate::vcell::{UCell, VCell};

use stm32h503::Interrupt::USB_FS as INTERRUPT;
use stm32h503::usb::chepr::{R as CheprR, W as CheprW};

macro_rules!dbgln {
    ($($tt:tt)*) => {if true {crate::dbgln!($($tt)*)}};
}

trait Chepr {
    fn utype(&mut self, ut: u8) -> &mut Self;
    fn control(&mut self) -> &mut Self {self.utype(1)}
    fn bulk(&mut self) -> &mut Self {self.utype(0)}
    fn interrupt(&mut self) -> &mut Self {self.utype(3)}

    fn stat_rx(&mut self, c: &CheprR, s: u8) -> &mut Self;
    fn stat_tx(&mut self, c: &CheprR, s: u8) -> &mut Self;
    fn rx_valid(&mut self, c: &CheprR) -> &mut Self {self.stat_rx(c, 3)}
    fn tx_valid(&mut self, c: &CheprR) -> &mut Self {self.stat_tx(c, 3)}

    fn dtogrx(&mut self, c: &CheprR, t: bool) -> &mut Self;
    fn dtogtx(&mut self, c: &CheprR, t: bool) -> &mut Self;
}

impl Chepr for CheprW {
    fn utype(&mut self, ut: u8) -> &mut Self {
        self.UTYPE().bits(ut).VTTX().set_bit().VTRX().set_bit()
    }
    fn stat_rx(&mut self, c: &CheprR, v: u8) -> &mut Self {
        self.STATRX().bits(c.STATRX().bits() ^ v)
    }
    fn stat_tx(&mut self, c: &CheprR, v: u8) -> &mut Self {
        self.STATTX().bits(c.STATTX().bits() ^ v)
    }
    fn dtogrx(&mut self, c: &CheprR, v: bool) -> &mut Self {
        self.DTOGRX().bit(c.DTOGRX().bit() ^ v)
    }
    fn dtogtx(&mut self, c: &CheprR, v: bool) -> &mut Self {
        self.DTOGTX().bit(c.DTOGTX().bit() ^ v)
    }
}

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
        total_length       : size_of::<Config1plus2>() as u16,
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

const fn chep_tx(offset: usize, len: usize) -> u32 {
    offset as u32 + len as u32 * 65536
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
        nothing();
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

    // FIXME interrupt::set_priority(INTERRUPT, 0xff);
    interrupt::enable(INTERRUPT);
}

fn usb_isr() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let mut istr = usb.ISTR.read();
    dbgln!("*** USB isr ISTR = {:#010x} FN={}", istr.bits(), usb.FNR.read().FN().bits());
    // Write zero to the interrupt bits we wish to acknowledge.
    usb.ISTR.write(|w| w.bits(!istr.bits() & !0x37fc0));

    // FIXME - is this CHEP or endpoint?
    while istr.CTR().bit() {
        match istr.bits() & 31 {
            0    => control_tx_handler(),
            16   => control_rx_handler(),
            1    => serial_tx_handler(),
            17   => serial_rx_handler(),
            2    => interrupt_handler(),
            _    => break,  // FIXME, this will hang!
        }
        istr = usb.ISTR.read();
    }

    if istr.RST_DCON().bit() {
        usb_initialize();
    }

    dbgln!("CHEP0 now {:#010x}", usb.CHEPR[0].read().bits());
    dbgln!("***");
}

fn control_tx_handler() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let chep = usb.CHEPR[0].read();
    dbgln!("Control TX handler CHEP0 = {:#010x}", chep.bits());

    if !chep.VTTX().bit() {
        dbgln!("Bugger!");
        return;
    }

    let address = unsafe {PENDING_ADDRESS.as_mut()};
    if let Some(a) = *address {
        do_set_address(a);
        *address = None;
        // Clear the VTTX bit.  Make sure STATRX is enabled.
        // FIXME - race with incoming?
    }
    let tx_len = chep_bd()[0].read() >> 16 & 0x03ff;
    // Despite what the docs say, setting EPKIND here hangs the USB, and
    // the stm cube code doesn't appear to set it.
    let real_tx = false && tx_len != 0;
    usb.CHEPR[0].write(
        |w|w.control().VTTX().clear_bit().rx_valid(&chep)
            .EPKIND().bit(real_tx)
            .DTOGRX().bit(chep.DTOGRX().bit() && !real_tx)
        );
}

fn control_rx_handler() {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    let chep = usb.CHEPR[0].read();
    dbgln!("Control RX handler CHEP0 = {:#010x}", chep.bits());

    if !chep.VTRX().bit() {
        dbgln!("Bugger");
        return;
    }

    if !chep.SETUP().bit() {
        dbgln!("Control RX handler, CHEP0 = {:#010x}, non-setup", chep.bits());
        // Make sure we are ready for another read.
        usb.CHEPR[0].write(
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
    let Ok(data) = setup_rx_handler(setup) else {
        dbgln!("Set-up error");
        // Set STATTX to 1 (stall).  FIXME - clearing DTOGRX should not be
        // needed.
        usb.CHEPR[0].write(
            |w|w.control().VTRX().clear_bit()
                .rx_valid(&chep).stat_tx(&chep, 1)
                .DTOGRX().bit(chep.DTOGRX().bit()));
        return;
    };

    // Copy the data into the control TX buffer.
    let len = data.len().min(setup.length as usize);
    unsafe {copy_by_dest32(data.as_ptr(), CTRL_TX_BUF, len)};
    dbgln!("Setup response length = {} -> {} [{:02x} {:02x}]",
           data.len(), len,
           if data.len() > 0 {unsafe{*data.as_ptr()}} else {0},
           unsafe{*CTRL_TX_BUF});
    // Setup the control transfer.  CHEP 0, TX is first in BD.
    // If the length is zero, then we are sending an ack.  If the length
    // is non-zero, then we are sending data and expect an ack.  We
    // ignore the ack, and ignore whether or not we get one...
    chep_bd()[0].write(chep_tx(CTRL_TX_OFFSET, len));
    // FIXME what about SETUP + OUT?
    usb.CHEPR[0].write(|w| w.control().VTRX().clear_bit().tx_valid(&chep));
}

fn setup_rx_handler(setup: &SetupHeader) -> SetupResult {
    // Cancel any pending set-address.
    unsafe {*PENDING_ADDRESS.as_mut() = None};
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
        (0x00, 0x09) => set_configuration(setup.value_lo),
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
}

fn set_configuration(cfg: u8) -> SetupResult {
    let usb = unsafe {&*stm32h503::USB::ptr()};
    if cfg == 0 {
        let chep1 = usb.CHEPR[1].read();
        usb.CHEPR[1].write(|w| w.bulk().stat_rx(&chep1, 0).stat_rx(&chep1, 0));
        let chep2 = usb.CHEPR[2].read();
        usb.CHEPR[2].write(|w| w.bulk().stat_rx(&chep2, 0).stat_rx(&chep2, 0));
        let chep3 = usb.CHEPR[3].read();
        usb.CHEPR[3].write(
            |w| w.interrupt().stat_rx(&chep3, 0).stat_rx(&chep3, 0));
        return setup_result(&());
    }
    if cfg != 1 {
        return Err(());
    }

    // Buffer descriptors for USB OUT.
    chep_bd()[2].write(chep_block::<64>(CTRL_RX_OFFSET));
    chep_bd()[3].write(chep_block::<64>(CTRL_RX_OFFSET + 64));

    // Initialize the others.
    chep_bd()[4].write(chep_tx(CTRL_TX_OFFSET, 0));
    chep_bd()[5].write(chep_tx(CTRL_TX_OFFSET + 64, 0));
    chep_bd()[6].write(chep_tx(INTR_TX_OFFSET, 0));

    // Serial RX.  USB OUT.  Double buffer.
    let chep1 = usb.CHEPR[1].read();
    usb.CHEPR[1].write(
        |w|w.bulk().rx_valid(&chep1).stat_tx(&chep1, 0).EPKIND().set_bit()
            .dtogrx(&chep1, false).dtogtx(&chep1, false));
    // Serial TX.  USB IN.  Double buffer.
    let chep2 = usb.CHEPR[2].read();
    usb.CHEPR[2].write(
        |w|w.bulk().tx_valid(&chep2).stat_rx(&chep2, 0).EPKIND().set_bit()
            .dtogrx(&chep2, false).dtogtx(&chep2, false));
    // Interrupt.  USB IN.
    let chep3 = usb.CHEPR[3].read();
    usb.CHEPR[3].write(
        |w|w.interrupt().stat_rx(&chep3, 0).stat_tx(&chep3, 0)
            .dtogrx(&chep3, false));

    setup_result(&())
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

    usb.CHEPR[0].modify(|r,w| w.control().rx_valid(r));
    usb.DADDR.write(|w| w.EF().set_bit().ADD().bits(0));

    let bd = chep_bd();
    barrier();
    bd[0].write(chep_block::<64>(CTRL_TX_OFFSET)); // Control TX
    bd[1].write(chep_block::<64>(CTRL_RX_OFFSET)); // Control RX
    barrier();
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
