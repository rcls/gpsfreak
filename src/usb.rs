
pub const STRING_LIST: [&str; 10] = [
    "FOO", "BAR", "ME1", "LONGER", "X",
    "BURP", "STEAK", "VEGETABLES", "BREAKFAST", "SAUSAGE",
];

pub const NUM_STRINGS: usize = STRING_LIST.len();

pub const STRING_LENGTHS: [usize; NUM_STRINGS] = konst::iter::collect_const!(
    usize => STRING_LIST, map(str_utf16_count));

pub const TOTAL_LENGTHS: u16 = 0;

pub const A_LEN: usize = str_utf16_count("abcdefg");

const fn str_utf16_count(s: &str) -> usize {
    let mut i = konst::string::chars(s);
    let mut n = 0;
    while let Some(c) = i.next() {
        n += if c  < '\u{10000}' {1} else {2};
    }
    n
}

const fn str_to_utf16_inplace(u: &mut [u16], s: &str) {
    let mut i: usize = 0;
    let mut j = konst::string::chars(s);
    while let Some(x) = j.next() {
        let c: char = x; // ?
        if c < '\u{10000}' {
            u[i] = c as u16;
        }
        else {
            u[i] = ((c as u32) >> 10 & 0x3ff) as u16 + 0xd800;
            i += 1;
            u[i] = (c as u16 & 0x3ff) + 0xdc00;
        }
        i += 1;
    }
}

pub fn dummy() {
    let _ = NUM_STRINGS;
    let _ = A_LEN;
}
