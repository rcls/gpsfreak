
unsafe extern "C" {
    static mut __bss_start: u8;
    static mut __bss_end: u8;
    #[cfg(target_os = "none")]
    static end_of_ram: u8;
}

#[cfg(not(target_os = "none"))]
#[allow(non_upper_case_globals)]
static end_of_ram: u8 = 0;

pub fn init() {
    // Clear the BSS.
    if !cfg!(test) {
        crate::vcell::barrier();
        // The rustc memset is hideous.
        let mut p = (&raw mut __bss_start) as *mut u32;
        loop {
            unsafe {*p = 0};
            p = p.wrapping_add(1);
            if p as *mut u8 >= &raw mut __bss_end {
                break;
            }
        }
        crate::vcell::barrier();
    }

    // We use sev-on-pend to avoid trivial interrupt handlers.
    let scb  = unsafe {&*cortex_m::peripheral::SCB::PTR};
    unsafe {scb.scr.write(16)};
}

fn bugger() {
    loop {}
}

pub fn enable_interrupt(n: stm32h503::Interrupt) {
    let nvic = unsafe {&*cortex_m::peripheral::NVIC::PTR};
    let bit: usize = n as usize % 32;
    let idx: usize = n as usize / 32;
    unsafe {nvic.iser[idx].write(1 << bit)};
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
