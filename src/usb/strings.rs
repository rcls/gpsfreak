
use super::types::SetupResult;

mod string_table;

crate::define_usb_strings!{}

type Offset = u8;
pub const STRING_LIST: [&str; 8] = [
    "\u{0409}", // Languages.
    "Ralph", "GPS Freak", "Device Configuration",
    "CDC", "CDC DATA interface", "Device Control", "DFU",
];

pub const IDX_SERIAL_NUMBER: u8 = NUM_STRINGS as u8;

pub fn get_descriptor(idx: u8) -> SetupResult {
    if idx != IDX_SERIAL_NUMBER {
        return _get_descriptor(idx);
    }
    // Special case.
    let data = crate::command::USB_NAME.as_ref();
    let byte_len = data[0] & 0xff;
    let data = unsafe {
        core::slice::from_raw_parts(data as *const u16 as *const u8,
                                    byte_len as usize)};
    return SetupResult::Tx(data);
}
