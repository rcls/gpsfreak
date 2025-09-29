
const POLY: u16 = 0x1021;

pub fn init() {
    let crc = unsafe {&*stm32h503::CRC::ptr()};
    let rcc = unsafe {&*stm32h503::RCC::ptr()};

    rcc.AHB1ENR.modify(|_,w| w.CRCEN().set_bit());

    crc.POL.write(|w| w.bits(POLY as u32));
    crc.INIT.write(|w| w.CRC_INIT().bits(0));
}

pub fn compute(bytes: &[u8]) -> u16 {
    if cfg!(target_os = "none") {
        hw_compute(bytes)
    }
    else {
        sw_compute(0, bytes)
    }
}

pub fn hw_compute(bytes: &[u8]) -> u16 {
    let crc = unsafe {&*stm32h503::CRC::ptr()};
    crc.CR.write(|w|w.POLYSIZE().B_0x1().RESET().set_bit());
    // TODO - word by word!
    for &b in bytes {
        // Be careful, we want to write single byts.
        let dr = crc.DR.as_ptr() as *mut u8;
        unsafe {core::ptr::write_volatile(dr, b)};
    }
    crc.DR.read().bits() as u16
}

static TABLE: [u16; 256] = {
    let mut table = [0; _];
    let mut i = 0;
    while i < 128 {
        let prev = table[i];
        let dbl = prev << 1 ^ if prev & 0x8000 != 0 {POLY} else {0};
        table[2*i] = dbl;
        table[2*i+1] = dbl ^ POLY;
        i += 1;
    }
    table
};

pub fn sw_compute(iv: u16, bytes: &[u8]) -> u16 {
    let mut v = iv;
    for b in bytes {
        v = TABLE[v as usize >> 8 ^ *b as usize] ^ v << 8;
    }
    v
}

#[cfg(test)]
fn by_bit(iv: u16, bytes: &[u8]) -> u16 {
    let mut v = iv;
    for b in bytes {
        for i in (0 ..= 7).rev() {
            v = v << 1 ^ if v & 0x8000 != 0 {POLY} else {0};
            v ^= *b as u16 >> i & 1
        }
    }
    v
}

#[test]
fn table() {
    for b in 0 ..= 255 {
        let v = by_bit(0, &[b, 0, 0]);
        let t = TABLE[b as usize];
        println!("{b} {v:#06x} {t:#06x}");
        assert_eq!(v, t);
    }
}

#[test]
fn basic() {
    let bytes = b"123456789";
    let v = by_bit(0, bytes);
    println!("{v:#06x}");
    let v = by_bit(v, &[0, 0]);
    println!("{v:#06x}");
    let u = sw_compute(0, bytes);
    println!("{u:#06x}");
    assert_eq!(v, u);
    assert_eq!(v, 0x31c3);              // Canned value.
    let mut bb = [0; 11];
    bb[0..9].copy_from_slice(bytes);
    bb[9] = (u >> 8) as u8;
    bb[10] = u as u8;
    assert_eq!(sw_compute(0, &bb), 0);
}
