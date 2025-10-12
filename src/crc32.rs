//! This is similar to the Ethernet CRC, with the only difference being
//! opposite bit-ordering of each byte.

use crate::crc::POLY32;

pub const VERIFY_MAGIC: u32 = 0x38fb2284;

pub fn compute(address: *const u8, length: usize) -> u32 {
    if cfg!(target_os = "none") {
        hw_compute(address, length)
    }
    else {
        !crate::crc::sw_compute(
            &TABLE, !0, unsafe{core::slice::from_raw_parts(address, length)})
    }
}

pub fn hw_compute(address: *const u8, length: usize) -> u32 {
    let crc = unsafe {&*stm32h503::CRC::ptr()};
    crc.POL.write(|w| w.bits(POLY32 as u32));
    crc.INIT.write(|w| w.CRC_INIT().bits(!0));
    crc.CR.write(|w| w.POLYSIZE().B_0x0().RESET().set_bit());
    // TODO - word by word!  And share with the other identical code.
    for i in 0 .. length {
        // Be careful, we need to write as the correct width.
        let dr = crc.DR.as_ptr() as *mut u8;
        let b = unsafe{*address.wrapping_add(i)};
        unsafe {core::ptr::write_volatile(dr, b)};
    }
    !crc.DR.read().bits()
}

const TABLE: [u32; 256] = crate::crc::crc_table(POLY32, 32);

#[test]
fn check_magic() {
    let first = compute(&[] as _, 0).to_be_bytes();
    let second = compute(&first as _, 4);

    assert_eq!(second, VERIFY_MAGIC);

    let mut more = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0];
    let len = more.len() - 4;
    let crc = compute(&more as *const u8, len);
    more[len ..].copy_from_slice(&crc.to_be_bytes());

    assert_eq!(compute(&more as *const u8, more.len()), VERIFY_MAGIC);
}
