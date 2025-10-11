use crate::crc::POLY16;

pub fn compute(bytes: &[u8]) -> u16 {
    if cfg!(target_os = "none") {
        hw_compute(bytes)
    }
    else {
        crate::crc::sw_compute(&TABLE, 0, bytes)
    }
}

pub fn hw_compute(bytes: &[u8]) -> u16 {
    let crc = unsafe {&*stm32h503::CRC::ptr()};
    crc.POL.write(|w| w.bits(POLY16 as u32));
    crc.INIT.write(|w| w.CRC_INIT().bits(0));
    crc.CR.write(|w| w.POLYSIZE().B_0x1().RESET().set_bit());
    // TODO - word by word!
    for &b in bytes {
        // Be careful, we need to write as the correct width.
        let dr = crc.DR.as_ptr() as *mut u8;
        unsafe {core::ptr::write_volatile(dr, b)};
    }
    crc.DR.read().bits() as u16
}

static TABLE: [u16; 256] = crate::crc::crc_table(POLY16, 16);

#[test]
fn basic() {
    let bytes = b"123456789";
    let v = crate::crc::sw_compute(&TABLE, 0, bytes);
    assert_eq!(v, 0x31c3);              // Canned value.
    let mut bb = [0; 11];
    bb[0..9].copy_from_slice(bytes);
    bb[9] = (v >> 8) as u8;
    bb[10] = v as u8;
    assert_eq!(crate::crc::sw_compute(&TABLE, 0, &bb), 0);
}
