use crate::vcell::{UCell, VCell};

pub const CPU_FREQ: u32 = 160_000_000;

#[cfg(target_os = "none")]
const SYS_VTOR: u32 = 0x0bf87000;
const BKPSRAM_BASE: u32 = 0x40036400;
const DFU_MAGIC: u32 = 0x52434C76;

unsafe extern "C" {
    static mut __bss_start: u8;
    static mut __bss_end: u8;
    static mut __data_start: u8;
    static mut __data_end: u8;
    static mut __rom_data_start: u8;
    #[cfg(target_os = "none")]
    static end_of_ram: u8;
}

#[cfg(not(target_os = "none"))]
#[allow(non_upper_case_globals)]
static end_of_ram: u8 = 0;

const SERIAL_LEN: usize = 18;
const USB_SERIAL_LEN: usize = SERIAL_LEN * 2 + 2;
pub static SERIAL_NUMBER: UCell<[u8; SERIAL_LEN]> = UCell::new([0; _]);
pub static USB_SERIAL_NUMBER: UCell<[u8; USB_SERIAL_LEN]> = UCell::new([0; _]);


pub fn init() {
    let flash  = unsafe {&*stm32h503::FLASH ::ptr()};
    let icache = unsafe {&*stm32h503::ICACHE::ptr()};
    let pwr    = unsafe {&*stm32h503::PWR   ::ptr()};
    let rcc    = unsafe {&*stm32h503::RCC   ::ptr()};
    let scb    = unsafe {&*cortex_m::peripheral::SCB::PTR};

    // Copy the RW data and clear the BSS. The rustc memset is hideous so we
    // copy by hand.
    if !cfg!(test) {
        barrier();
        let src = &raw const __rom_data_start;
        let dst = &raw mut __data_start;
        let len = &raw mut __data_end as usize - dst as usize;
        for i in 0 .. len as usize {
            unsafe {*dst.wrapping_add(i) = *src.wrapping_add(i)}
        }


        let bss = &raw mut __bss_start as *mut u32;
        let len = &raw mut __bss_end as usize - bss as usize;
        for i in 0 .. len {
            unsafe {*bss.wrapping_add(i) = 0};
        }
        barrier();
    }

    // Use PLL1 in integer mode with even divider.
    const {assert!(CPU_FREQ % 8_000_000 == 0)};
    // Run PLL1 in wide range, 128MHz to 560MHz VCO.  Use the lowest VCO that
    // is an even multiple of CPU_FREQ.
    const PDIV_BY_2: u32 = 128_000_000u32.div_ceil(CPU_FREQ * 2);
    const PDIV: u32 = PDIV_BY_2 * 2;
    const {assert!(PDIV <= 255)};
    const IN_FREQ: u32 = 32_000_000;
    const PFD_FREQ: f64 = 2_000_000.;
    const MDIV: u32 = (IN_FREQ as f64 / PFD_FREQ + 0.5) as u32;
    const {assert!(MDIV >= 1 && MDIV <= 63)};
    const VCO_FREQ: u32 = CPU_FREQ * PDIV;
    const {assert!(VCO_FREQ <= 560_000_000)};
    const {assert!(VCO_FREQ >= 120_000_000)};
    const MULT: u32 = (VCO_FREQ as f64 / PFD_FREQ + 0.5) as u32;
    const {assert!(MULT >= 4)};
    const {assert!(MULT <= 512)};
    const {assert!(IN_FREQ as u64 * MULT as u64
        == CPU_FREQ as u64 * PDIV as u64 * MDIV as u64)};

    const RGE: u8 = if PFD_FREQ > 16_000_000. {panic!()}
        else if PFD_FREQ >= 8_000_000. {3}
        else if PFD_FREQ >= 4_000_000. {2}
        else if PFD_FREQ >= 2_000_000. {1}
        else if PFD_FREQ >= 1_000_000. {0} else {panic!()};

    // Increase core voltage if needed.
    // Max freq: VOS0: 250MHz, VOS1: 200MHz, VOS2: 150MHz, VOS3: 100MHz.
    const VOS: u8 = if CPU_FREQ > 200_000_000 {0}
        else if CPU_FREQ > 150_000_000 {1}
        else if CPU_FREQ > 100_000_000 {2}
        else {3};
    if VOS != 3 {
        pwr.VOSCR.write(|w| w.VOS().bits(VOS));
        loop {
            let vossr = pwr.VOSSR().read();
            if vossr.ACTVOS().bits() == VOS && vossr.ACTVOSRDY().bit() {
                break;
            }
        }
    }

    // Leave us on the 32MHz default clock if possible!
    if CPU_FREQ != 32_000_000 {
        // Set up PLL1.
        rcc.PLL1CFGR.write(
            |w|w.PLL1SRC().bits(1).PLL1M().bits(MDIV as u8)
                .PLL1RGE().bits(RGE).PLL1PEN().set_bit());
        rcc.PLL1DIVR.write(
            |w|w.PLL1N().bits(MULT as u16 - 1)
                .PLL1P().bits(PDIV as u8 - 1));
        // Enable the PLL.
        rcc.CR.modify(|_,w| w.PLL1ON().set_bit());
        // Configure flash wait states.  Below 80MHz change these!
        const {assert!(CPU_FREQ == 32_000_000 || CPU_FREQ >= 80_000_000)};
        let ws = if CPU_FREQ <= 150_000_000 {4} else {5};
        flash.ACR.write(
            |w| w.PRFTEN().set_bit().WRHIGHFREQ().bits(2).LATENCY().bits(ws));
        // The docs say to read-back ACR.
        flash.ACR.read();
        // WRHIGH FREQ = 2
        // Wait for the PLL to become ready...
        while !rcc.CR.read().PLL1RDY().bit() {}
        // Change the main system clock.
        rcc.CFGR1.write(|w| w.SW().bits(3));
    }

    // Enable the CSI (4MHz) for I2C.
    rcc.CR.modify(|_,w| w.CSION().set_bit());
    while !rcc.CR.read().CSIRDY().bit() {}

    // Generate the USB serial number.  ST notes claim that we will hard fault
    // if we do this with ICACHE enabled.
    let sn = unsafe {&*(0x8fff800 as *const [u32; 3])};
    format_serial_number(sn, unsafe {SERIAL_NUMBER.as_mut()},
                         unsafe {USB_SERIAL_NUMBER.as_mut()});

    // Enable ICACHE.
    while icache.SR.read().BUSYF().bit() {
    }
    icache.CR.write(|w| w.WAYSEL().set_bit().EN().set_bit());

    // We use sev-on-pend to avoid trivial interrupt handlers.
    unsafe {scb.scr.write(16)};
}

#[derive(Debug)]
pub struct Priority<const P: u8> {
    old: u8,
}

impl<const P: u8> Priority<P> {
    pub fn new() -> Self {
        let old;
        if !cfg!(target_os = "none") {
            old = cortex_m::register::basepri::read();
            unsafe {cortex_m::register::basepri::write(P)};
        }
        else {
            old = 0;
        }
        Priority{old}
    }
    pub fn wfe(&self) {
        if !cfg!(target_os = "none") {
            unsafe {cortex_m::register::basepri::write(self.old)};
            WFE();
            unsafe {cortex_m::register::basepri::write(P)};
        }
    }
}

impl<const P: u8> Drop for Priority<P> {
    fn drop(&mut self) {
        if !cfg!(target_os = "none") {
            unsafe {cortex_m::register::basepri::write(self.old)};
        }
    }
}

fn bugger() {
    interrupt::disable_all();
    let fp = unsafe {frameaddress(0)};
    // The exception PC is at +0x18, but then LLVM pushes an additional 8
    // bytes to form the frame.
    let pcp = fp.wrapping_add(0x20);
    let pc = unsafe {*(pcp as *const u32)};
    crate::dbgln!("Crash @ {pc:#010x}");
    loop {
        crate::uart_debug::debug_isr();
    }
}

#[inline(always)]
pub fn barrier() {
    // The rust library compile_fence isn't behaving as expected.  Use the ASM
    // version.
    unsafe {core::arch::asm!("")}
}

#[inline(always)]
pub fn nothing() {
    unsafe {core::arch::asm!("", options(nomem))}
}

#[allow(dead_code)]
pub mod interrupt {
    /// Interrupt priority for uart debug, the is the highest priority as debug
    /// can get used everywhere.
    pub const PRIO_DEBUG: u8 = 0;

    /// Interrupt priority for USB.
    pub const PRIO_USB  : u8 = 0x40;

    /// Interrupt priority for I2C.  Same as USB.
    pub const PRIO_I2C  : u8 = 0x40;

    /// Interrupt priority for systick and application processing.
    pub const PRIO_APP  : u8 = 0x80;


    // We don't use disabling interrupts to transfer ownership, so no need for
    // the enable to be unsafe.
    pub fn enable_all() {
        #[cfg(target_arch = "arm")]
        unsafe{cortex_m::interrupt::enable()}
    }
    pub fn disable_all() {
        #[cfg(target_arch = "arm")]
        cortex_m::interrupt::disable()
    }

    pub fn enable_priority(n: stm32h503::Interrupt, p: u8) {
        let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
        unsafe {nvic.ipr[n as usize].write(p)};

        let bit: usize = n as usize % 32;
        let idx: usize = n as usize / 32;
        unsafe {nvic.iser[idx].write(1u32 << bit)};
    }
}

#[inline(always)]
#[allow(non_snake_case)]
pub fn WFE() {
    if cfg!(target_arch = "arm") {
        unsafe {
            core::arch::asm!("wfe", options(nomem, preserves_flags, nostack))};
    }
    else {
        panic!("wfe!");
    }
}

pub fn maybe_enter_dfu() {
    let pwr = unsafe {&*stm32h503::PWR::ptr()};
    let rcc = unsafe {&*stm32h503::RCC::ptr()};
    // Enable BKPSRAM access,
    pwr.DBPCR.write(|w| w.DBP().set_bit());

    // Check for magic in the BKPSRAM to reboot into DFU.
    rcc.AHB1ENR.modify(|_,w| w.BKPRAMEN().set_bit());
    let magic: &'static VCell<u32> = unsafe {magic_reboot_config()};
    if magic.read() == DFU_MAGIC {
        magic.write(0);
        // Only do this on a software reboot!
        if rcc.RSR.read().SFTRSTF().bit() {
            unsafe {goto_sys_flash()};
        }
    }
    rcc.AHB1ENR.modify(|_,w| w.BKPRAMEN().clear_bit());
}

pub unsafe fn trigger_dfu() -> ! {
    let rcc = unsafe {&*stm32h503::RCC::ptr()};

    // crate::uart_debug::DEBUG.flush();

    rcc.AHB1ENR.modify(|_,w| w.BKPRAMEN().set_bit());

    // Set up the magic number.
    let magic = unsafe {magic_reboot_config()};
    magic.write(DFU_MAGIC);

    reboot();
}

pub fn reboot() -> ! {
    let scb = unsafe {&*cortex_m::peripheral::SCB::PTR};
    // Reboot by writing the appropriate magic number to AIRCR.
    loop {
        unsafe {scb.aircr.write(0x05fa0004)};
    }
}

pub unsafe fn goto_sys_flash() -> ! {
    // Reboot into DFU.
    #[cfg(target_os = "none")]
    unsafe {
        let scb = &*cortex_m::peripheral::SCB::PTR;
        scb.vtor.write(SYS_VTOR);
        let sp = *(SYS_VTOR as *const u32);
        let entry = *((SYS_VTOR + 4) as *const u32);
        core::arch::asm!(
            "mov sp, {sp}",
            "bx {entry}",
            sp = in(reg) sp,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
    #[cfg(not(target_os = "none"))]
    panic!("Only on device!");
}

unsafe fn magic_reboot_config() -> &'static VCell<u32> {
    unsafe {&*(BKPSRAM_BASE as *const VCell<u32>)}
}

fn format_serial_number(sn: &[u32; 3], text: &mut [u8; SERIAL_LEN],
                        usb: &mut [u8; USB_SERIAL_LEN]) {
    // Little endian, start from high address.
    // 0x08fff808 :
    //     ASCII lot number.
    // 0x08fff804 :
    //     low 8 bits, wafer number, convert to hex.
    //     high 24 bits, ASCII lot number
    // 0x08fff800 : 32 bit binary, X&Y wafer coords, convert to hex.
    // XXXXXXX-XXXXXXXXXX
    let lot = ((sn[2] as u64) << 32 | sn[1] as u64) >> 8;
    for i in 0..7 {
        text[i] = (lot >> i * 8) as u8;
    }
    text[7] = '-' as u8;
    let binary = (sn[1] as u64) << 32 | sn[0] as u64;
    for i in 0..10 {
        let hex = binary >> 36 - i * 4 & 15;
        text[8 + i] = hex as u8 + b'0' + if hex > 9 {b'a' - b'9' - 1} else {0};
    }
    usb[0] = usb.len() as u8;
    usb[1] = 3;
    for i in 0..SERIAL_LEN {
        usb[i * 2 + 2] = text[i];
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct VectorTable {
    pub stack     : *const u8,
    pub reset     : fn() -> !,
    pub nmi       : fn(),
    pub hard_fault: fn(),
    pub reserved1 : [u32; 7],
    pub svcall    : fn(),
    pub reserved2 : [u32; 2],
    pub pendsv    : fn(),
    pub systick   : fn(),
    pub isr       : [fn(); 134],
}

/// !@#$!@$#
unsafe impl Sync for VectorTable {}

impl const Default for VectorTable {
    fn default() -> Self {
        VectorTable{
            stack     : &raw const end_of_ram,
            reset     : crate::main,
            nmi       : bugger,
            hard_fault: bugger,
            reserved1 : [0; _],
            svcall    : bugger,
            reserved2 : [0; _],
            pendsv    : bugger,
            systick   : bugger,
            isr       : [bugger; _],
        }
    }
}

impl VectorTable {
    pub const fn isr(&mut self,
                     n: stm32h503::Interrupt, handler: fn()) -> &mut Self {
        self.isr[n as usize] = handler;
        self
    }
}

unsafe extern "C" {
    #[allow(unused)]
    #[link_name = "llvm.frameaddress"]
    fn frameaddress(level: i32) -> *const u8;
}

#[test]
fn test_sn() {
    let sn = [0x006b0028, 0x31335105, 0x30393436];
    let mut text = [0; SERIAL_LEN];
    let mut usb = [0; USB_SERIAL_LEN];
    format_serial_number(&sn, &mut text, &mut usb);
    assert_eq!(usb[0] as usize, size_of_val(&usb));
    assert_eq!(usb[1], 3);
    let mut bytes = [0; USB_SERIAL_LEN / 2 - 1];
    for (i, &c) in usb[2..].iter().step_by(2).enumerate() {
        bytes[i] = c as u8;
    }
    let str = str::from_utf8(&bytes).unwrap();
    assert_eq!(str, str::from_utf8(&text).unwrap());
    assert_eq!(str, "Q316490-05006b0028");
}
