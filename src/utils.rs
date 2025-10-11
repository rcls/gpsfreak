use core::ptr::{read_volatile, write_volatile};

/// Calling this function will cause a linker error when building the firmware,
/// unless the compiler optimises it away completely.
///
/// This is used to build assertions that are evaluated at compile time but
/// aren't officially Rust const code.
pub fn unreachable() -> ! {
    #[cfg(target_os = "none")]
    unsafe {
        // This will cause a compiler error if not removed by the optimizer.
        unsafe extern "C" {fn nowayjose();}
        nowayjose();
    }
    panic!();
}

pub unsafe fn vcopy_aligned(dest: *mut u8, src: *const u8, length: usize) {
    let mix = dest as usize | src as usize | length;
    if mix & 3 == 0 {
        let dest = dest as *mut u32;
        let src = src as *mut u32;
        for i in (0..length).step_by(4) {
            unsafe {write_volatile(dest.wrapping_byte_add(i),
                                   read_volatile(src.wrapping_byte_add(i)))};
        }
    }
    else if mix & 1 == 0 {
        let dest = dest as *mut u16;
        let src = src as *mut u16;
        for i in (0..length).step_by(2) {
            unsafe {write_volatile(dest.wrapping_byte_add(i),
                                   read_volatile(src.wrapping_byte_add(i)))};
        }
    }
    else {
        for i in 0 .. length {
            unsafe {write_volatile(dest.wrapping_byte_add(i),
                                   read_volatile(src.wrapping_byte_add(i)))};
        }
    }
}

/// Cause a build time error if the condition fails and the code path is not
/// optimized out.  For test builds this is converted to a run-time check.
#[macro_export]
macro_rules! link_assert {
    ($e:expr) => { if !$e {$crate::utils::unreachable()} }
}
