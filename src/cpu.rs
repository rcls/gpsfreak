
pub const CPU_FREQ: u32 = 160_000_000;

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
        // WRHIGH FREQ = 2
        // Wait for the PLL to become ready...
        while !rcc.CR.read().PLL1RDY().bit() {}
        // Change the main system clock.
        rcc.CFGR1.write(|w| w.SW().bits(3));
    }

    // Enable ICACHE.
    while icache.SR.read().BUSYF().bit() {
    }
    icache.CR.write(|w| w.WAYSEL().set_bit().EN().set_bit());

    // We use sev-on-pend to avoid trivial interrupt handlers.
    unsafe {scb.scr.write(16)};
}

fn bugger() {
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
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

#[inline(always)]
pub fn nothing() {
    unsafe {core::arch::asm!("", options(nomem))}
}

#[allow(dead_code)]
pub mod interrupt {
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

    pub fn enable(n: stm32h503::Interrupt) {
        let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
        let bit: usize = n as usize % 32;
        let idx: usize = n as usize / 32;
        unsafe {nvic.iser[idx].write(1u32 << bit)};
    }

    pub fn set_priority(n: stm32h503::Interrupt, p: u8) {
        let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
        unsafe {nvic.ipr[n as usize].write(p)};
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
