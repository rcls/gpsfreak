

#[macro_export]
macro_rules! define_usb_strings{
    {} => {
        pub const fn string_index(s: &str) -> u8 {
            let mut i = 0;
            loop {
                if STRING_LIST[i as usize] == s {
                    return i;
                }
                i += 1;
            }
        }

        pub const NUM_STRINGS: usize = STRING_LIST.len();

        const LENGTHS: [usize; NUM_STRINGS] = konst::iter::collect_const!(
            usize => STRING_LIST, map($crate::usb::string::str_utf16_count));

        const TOTAL_LENGTH: usize = OFFSETS[NUM_STRINGS - 1] as usize
            + LENGTHS[NUM_STRINGS - 1] + NUM_STRINGS;

        static OFFSETS: [Offset; NUM_STRINGS] = {
            let mut o = [0; _];
            let mut p = 0;
            let mut i = 0;
            while i < NUM_STRINGS {
                o[i] = p;
                p += LENGTHS[i] as Offset + 1;
                i += 1;
            }
            o
        };

        static DATA: [u16; TOTAL_LENGTH] = {
            let mut d = [0; _];
            let mut i = 0;
            while i < NUM_STRINGS {
                let start = OFFSETS[i] as usize;
                let end = start + LENGTHS[i] as usize;
                // Byte count (length*2+2), Descriptor type (3), as LE word.
                d[start] = LENGTHS[i] as u16 * 2 + 2 + 0x300;
                $crate::usb::string::str_to_utf16_inplace(
                    &mut d[start + 1 ..= end], STRING_LIST[i]);
                i += 1;
            }
            d
        };

        pub fn _get_descriptor(idx: u8) -> SetupResult {
            if idx as usize >= NUM_STRINGS {
                return SetupResult::error();
            }
            let offset = OFFSETS[idx as usize] as usize;
            let len = DATA[offset] as usize & 255;
            let data: &[u8] = unsafe{core::slice::from_raw_parts(
                &DATA[offset] as *const u16 as *const u8, len)};
            SetupResult::Tx(data, None)
        }
    }
}

pub const fn str_utf16_count(s: &str) -> usize {
    let mut i = konst::string::chars(s);
    let mut n = 0;
    while let Some(c) = i.next() {
        n += if c  < '\u{10000}' {1} else {2};
    }
    n
}

pub const fn str_to_utf16_inplace(u: &mut [u16], s: &str) {
    let mut i: usize = 0;
    let mut j = konst::string::chars(s);
    while let Some(x) = j.next() {
        let c: char = x; // ?
        if c < '\u{10000}' {
            u[i] = c as u16;
        }
        else {
            let excess = c as u32 - 0x10000;
            u[i] = (excess >> 10 & 0x3ff) as u16 + 0xd800;
            i += 1;
            u[i] = (excess & 0x3ff) as u16 + 0xdc00;
        }
        i += 1;
    }
}

#[test]
fn test_utf16() {
    for s in ["abcd123456", "12🔴3🟥4🛑56🚫7🚨8😷"] {
        let len = str_utf16_count(s);
        let mut place = Vec::new();
        place.resize(len, 0);
        str_to_utf16_inplace(&mut place, s);
        let utf16: Vec<u16> = s.encode_utf16().collect();
        assert_eq!(place, utf16);
    }
}
