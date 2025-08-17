
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
}

fn bugger() {
    loop {}
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

unsafe extern "C" {
    #[allow(unused)]
    #[link_name = "llvm.frameaddress"]
    fn frameaddress(level: i32) -> *const u8;
}

#[used]
#[unsafe(link_section = ".vectors")]
pub static VECTORS: VectorTable = VectorTable {
    stack     : &raw const end_of_ram,
    reset     : crate::main,
    nmi       : bugger,
    hard_fault: bugger,
    reserved1 : [0; _],
    svcall    : bugger,
    reserved2 : [0; _],
    pendsv    : bugger,
    systick   : bugger,
    isr       : [bugger; _]
};
