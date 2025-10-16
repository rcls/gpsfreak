//! Table driven CRC code, only used for unittests.  Which is a bit silly.

use core::ops::{BitAnd, BitXor, Shl};

pub const POLY16: u16 = 0x1021;
pub const POLY32: u32 = 0x04c11db7;

pub fn init() {
    let rcc = unsafe {&*stm32h503::RCC::ptr()};

    rcc.AHB1ENR.modify(|_,w| w.CRCEN().set_bit());

}

pub const fn crc_table<T> (poly: T, bits: u32) -> [T; 256] where
    T: Copy + [const] PartialEq + [const] From<u8>,
    T: [const] BitAnd<Output=T> + [const] BitXor<Output=T>,
    T: [const] Shl<u32, Output=T>,
{
    let zero = T::from(0);
    let mut table = [zero; 256];
    let mut i = 0;
    let mask = T::from(1) << bits - 1;
    while i < 128 {
        let prev = table[i];
        let dbl = prev << 1 ^ if prev & mask != zero {poly} else {zero};
        table[2 * i] = dbl;
        table[2 * i + 1] = dbl ^ poly;
        i += 1;
    }
    table
}

pub fn sw_compute<T>(table: &[T; 256], iv: T, bytes: &[u8]) -> T where
    T: Copy + Shl<usize, Output=T> + BitXor<Output=T> + Into<u32>,
{
    let mut v = iv;
    let shift = size_of::<T>() * 8 - 8;
    for b in bytes {
        v = table[v.into() as usize >> shift ^ *b as usize] ^ v << 8;
    }
    v
}

// Unit tests using the CRC16 polynomial.

#[cfg(test)]
static TABLE: [u16; 256] = crate::crc::crc_table(POLY16, 16);

#[cfg(test)]
fn by_bit(iv: u16, bytes: &[u8]) -> u16 {
    let mut v = iv;
    for b in bytes {
        for i in (0 ..= 7).rev() {
            v = v << 1 ^ if v & 0x8000 != 0 {POLY16} else {0};
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
    let u = crate::crc::sw_compute(&TABLE, 0, bytes);
    println!("{u:#06x}");
    assert_eq!(v, u);
    assert_eq!(v, 0x31c3);              // Canned value.
}
