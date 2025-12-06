use core::ptr::{read_volatile, write_volatile};

/// Like memcpy(), but guarentee using the largest of u32, u16 or u8 compatible
/// with the alignment and length.
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
