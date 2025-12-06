//! Commands are binary encoded.
//!
//! The format is
//!
//!     magic   : u16 // Bytes CE 93
//!     code    : u8
//!     len     : u8
//!     payload : [u8; len]
//!     checksum: u16
//!
//! The magic is the byte sequence CE 93 (little endian 0x93ce), which is
//! UTF-8 for 'Γ' (GREEK UPPER CASE GAMMA).
//!
//! The code bit identifies the message purpose and format.  The high bit
//! indicates the direction: 0 is to the device, 1 is from the device.
//!
//! Any request to the device gets a response, a ACK or NAK if nothing else.
//!
//! Note that if the device gets a request code indicating a message from the
//! device, then it does not respond.  This avoids message loops!
//!
//! `len` is the length of the payload, in bytes.
//!
//! Checksum is a CRC-16, polynomial 0x(1)1021, with IV 0 and no inversion.
//! Earlier bytes in the payload have larger polynomial exponents, and within
//! each byte, the MSB has highest exponent and the LSB lowest.  Just like God
//! intended.
//!
//! The CRC is computed by placing 0 in the checksum field and then over all
//! bytes including the magic and the checksum.  Note that the calculation
//! routine below includes the final two zeros implicitly.
//!
//! The CRC is then checked by computing the CRC over the entire message, and
//! checking that the result is zero.
//!
//! Commands (codes are hex):
//!    00 : PING.  Arbitrary payload.  Response is 80 and echos the payload.
//!         By sending a random token, you can check that messages are
//!         synchronised.
//!    80 : ACK. Generic Acknowledgement.  Payload is generally empty.
//!         Ping responses echo the payload.  Otherwise if non-empty, then is an
//!         informational UTF-8 string.
//!
//!    81 : NAK. Generic failure.  A request could not be successfully executed.
//!         A u16 payload field.  See below for the error enumeration.
//!
//!    02 : Get protocol version.  Response is 82 with u32 payload.
//!    03 : Get CPU serial number.  Response is 83 with ASCII string payload.
//!    04 : Get/set device name.  Response is 84 with UTF-8 payload.
//!         This string is also used as the USB serial number.
//!
//!    10 : CPU reboot.  No response.
//!    11 : GPS reset. u8 payload.
//!            - 0 assert reset low, 1 deassert reset high, others pulse reset.
//!    12 : Clock gen PDN (reset), u8 payload:
//!            - 0 power down, 1 power up, ≥2 reset & power back up.
//!    1e : Serial sync / delay.  Used in provisioning.
//!    1f : Get/Set baud rate, optional u32 payload has baud rate, Response
//!         is 9f with baud rate.
//!
//!    60 : LMK05318b I²C write.  Payload is sent in a I²C write transaction.
//!
//!    61 : LMK05318b I²C read.  First byte of payload is number of bytes,
//!         if there are subsequent bytes, then these are sent as a write
//!         before a repeated-start read.  Reply is E1 with the read bytes
//!
//!    62 : TMP117 I²C write.  Just like 60, but to the TMP117.
//!    63 : TMP117 I²C read.  Just like 61, but from the TMP117.
//!
//!    64, 65 : Reserved for GPS I²C.
//!
//!    68 : Update LMK05318b status LED.  Use this to make the firmware catch
//!         up after sending I²C commands that alter the status flag handling.
//!
//!    71 : peek.  Payload is u32 address followed by u32 length.  Response is
//!         F1 with address + data payload.
//!    72 : poke.  Payload is u32 address followed by data bytes.
//!         As well as memory writes, flash writes of an aligned 32 byte block
//!         is supported.
//!    73 : crc.  Payload is u32 address followed by u32 length.  Response is
//!         F3 with a 32 bit CRC payload.
//!
//!            Both peek and poke will do 32-bit or 16-bit transfers if address
//!            and length are both sufficiently aligned.  Neither guard against
//!            crashing the device or making irreversable changes.

use stm_common::vcell::UCell;

use crate::{gps_uart::GpsPriority, i2c};
use crate::utils::vcopy_aligned;

mod crc16;

pub type Responder = fn(&[u8]);

macro_rules!dbgln {($($tt:tt)*) => {if false {crate::dbgln!($($tt)*)}};}

/// I²C address of the TMP117.  ADD0 on the TMP117 connects to 3V3.
pub const TMP117: u8 = 0x92;

/// Error codes for Nack responses.
#[repr(u16)]
#[derive(Debug, Default, Eq, PartialEq)]
enum Error {
    #[default]
    /// Generic request attempted but failed.
    Failed         = 1,
    /// Incorrectly framed message.
    FramingError   = 2,
    /// Message has unknown request code.
    UnknownMessage = 3,
    /// Payload format incorrect.
    BadFormat      = 4,
    /// Payload parameter value bogus.
    BadParameter   = 5,
    /// Special value used to indicate that an ACK (not a NACK) should be sent.
    Succeeded      = 6,
}

type Result<T = ()> = core::result::Result<T, Error>;

impl From<()> for Error {
    fn from(_: ()) -> Error {Error::Failed}
}

const SEND_ACK: Result = Err(Error::Succeeded);

type Ack  = Message<()>;
type Nack = Message<Error>;

/// Magic number for message header.
const MAGIC: u16 = 0xce93u16.to_be();

/// Maximum message payload size for a message.  Currently we only support
/// messages up to 64 bytes total.
const MAX_PAYLOAD: usize = 58;

/// A struct representing a message.
#[repr(C, align(4))]
pub struct MessageBuf {
    magic  : u16,
    code   : u8,
    len    : u8,
    /// The payload includes the CRC.
    payload: [u8; MAX_PAYLOAD + 2],
}
const _: () = assert!(size_of::<MessageBuf>() == 64);

impl const Default for MessageBuf {
    fn default() -> MessageBuf {
        MessageBuf{magic: 0, code: 0, len: 0, payload: [0; _]}
    }
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct Message<P> {
    magic  : u16,
    code   : u8,
    len    : u8,
    payload: P,
    crc0   : u8,
    crc1   : u8,
}

/// Assigned device name, as a message.
static NAME: UCell<MessageBuf> = Default::default();
/// Assigned device name, in USB format.
pub static USB_NAME: UCell<[u16; 32]> = UCell::new([0; _]);

pub fn init(serial: &str) {
    let name = unsafe {NAME.as_mut()};
    name.magic = MAGIC;
    name.code = 0x84;
    let sbytes = serial.as_bytes();
    let len = sbytes.len();
    name.len = len as u8;
    name.payload[..len].copy_from_slice(sbytes.as_ref());
    str_to_usb(unsafe {USB_NAME.as_mut()}, serial);
}

fn str_to_usb(out: &mut [u16], s: &str) {
    let mut w = out.iter_mut();
    let Some(head) = w.next() else {return};
    let mut bytes = 0x302;
    for code in s.chars() {
        let code = code as u32;
        if code < 0x10000 {
            let Some(c) = w.next() else {break};
            *c = code as u16;
            bytes += 2;
        }
        else {
            let Some(c1) = w.next() else {break};
            let Some(c2) = w.next() else {break};
            let code = code - 0x10000;
            *c1 = (code >> 10 & 0x3ff) as u16 + 0xd800;
            *c2 = (code & 0x3ff) as u16 + 0xdc00;
            bytes += 4;
        }
    }
    *head = bytes;
}

impl MessageBuf {
    fn start(code: u8) -> MessageBuf {
        MessageBuf{magic: MAGIC, code, len: 0, payload: [0; _]}
    }
    fn send(&mut self, r: Responder) -> Result {
        self.set_crc();
        let len = 4 + self.len as usize + 2;     // Include header and CRC.
        r(unsafe {core::slice::from_raw_parts(
            self as *const Self as _, len)});
        Ok(())
    }
    fn set_crc(&mut self) {
        let len = self.len as usize;
        let crc = crc16::compute(unsafe {core::slice::from_raw_parts(
            self as *const Self as _, 4 + len)});
        self.payload[len] = (crc >> 8) as u8;
        self.payload[len + 1] = crc as u8;
    }
    fn get_payload(&self) -> &[u8] {
        &self.payload[.. self.len as usize]
    }
    fn check_len(&self, len: u8) -> Result<&MessageBuf> {
        if self.len == len {Ok(self)} else {Err(Error::BadFormat)}
    }
}

impl<P: core::fmt::Debug> Message<P> {
    fn new(code: u8, payload: P) -> Message<P> {
        Message{magic: MAGIC, code, len: size_of::<P>() as u8,
                payload, crc0: 0, crc1: 0}
    }
    fn set_crc(&mut self) {
        let p: *const u8 = self as *const Self as _;
        let len = size_of::<P>() + 4;
        let crc = crc16::compute(unsafe {core::slice::from_raw_parts(p, len)});
        self.crc0 = (crc >> 8) as u8;
        self.crc1 = crc as u8;
    }
    fn send(&mut self, r: Responder) -> Result {
        dbgln!("Freak TX: @{:?} {:?}", self as *const _, self);
        self.set_crc();
        r(unsafe {core::slice::from_raw_parts(
            self as *const Self as _, 6 + size_of::<P>())});
        Ok(())
    }
    fn from_buf(message: &MessageBuf) -> Result<&Self> {
        message.check_len(size_of::<P>() as u8)?;
        Ok(unsafe {&*(message as *const MessageBuf as *const Self)})
    }
}

pub fn command_handler(message: &MessageBuf, len: usize, r: Responder) {
    match command_dispatch(message, len, r) {
        Err(Error::Succeeded) => {let _ = Ack::new(0x80, ()).send(r);}
        Err(err) => {let _ = Nack::new(0x81, err).send(r);}
        _ => (),
    }
}

fn command_dispatch(message: &MessageBuf, len: usize, r: Responder) -> Result {
    // dbgln!("Command handler dispatch {:x?}",
    //       unsafe {core::slice::from_raw_parts(message as *const _ as *const u8, len)});
    if message.len as usize + 6 != len {
        dbgln!("Length problem {len} {}", message.len);
        return Err(Error::FramingError);
    }
    if message.code & 0x80 != 0 {
        // Wrong message direction, ignore.
        r(&[]);
        return Ok(());
    }
    if crc16::compute(unsafe {core::slice::from_raw_parts(
        message as *const _ as _, len)}) != 0 {
        dbgln!("CRC error {len} {}", message.len);
        return Err(Error::FramingError);
    }

    match message.code {
        0x00 => ping(message, r),
        0x02 => get_protocol_version(message, r),
        0x03 => get_serial_number(message, r),
        0x04 => set_get_name(message, r),

        0x10 => crate::cpu::reboot(),
        0x11 => gps_reset(message),
        0x12 => lmk_powerdown(message),

        0x1e => serial_sync(message),
        0x1f => set_get_baud(message, r),

        0x60 => i2c_write(crate::lmk05318b::LMK05318 & !1, message),
        0x61 => i2c_read (crate::lmk05318b::LMK05318 |  1, message, r),
        0x62 => i2c_write(TMP117 & !1, message),
        0x63 => i2c_read (TMP117 |  1, message, r),

        0x68 => lmk05318b_status(message),

        0x71 => peek(message, r),
        0x72 => poke(message),
        0x73 => get_crc(message, r),
        0x74 => flash_erase(message),
        0x78 => test_gps_write(message),

        _ => Err(Error::UnknownMessage)
    }
}

fn ping(message: &MessageBuf, r: Responder) -> Result {
    // Send a generic ACK with the same payload.
    let mut resp = MessageBuf::start(0x80);
    let len = message.len as usize;
    resp.len = len as u8;
    resp.payload[..len].copy_from_slice(&message.payload[..len]);
    resp.send(r)
}

fn get_protocol_version(message: &MessageBuf, r: Responder) -> Result {
    Message::<()>::from_buf(message)?;
    Message::new(0x82, 1u32).send(r)
}

fn get_serial_number(message: &MessageBuf, r: Responder) -> Result {
    Message::<()>::from_buf(message)?;
    let sn = crate::cpu::SERIAL_NUMBER.as_ref();
    Message::new(0x83, *sn).send(r)
}

fn set_get_name(message: &MessageBuf, r: Responder) -> Result {
    let name = unsafe {NAME.as_mut()};
    let len = message.len as usize;
    if len > MAX_PAYLOAD {
        // Our callers have actually validated the length but lets be safe.
        return Err(Error::FramingError);
    }
    if len > 0 {
        let payload = &message.payload[..len];
        let Ok(utf8) = str::from_utf8(payload)
            else {return Err(Error::BadParameter)};
        str_to_usb(unsafe {USB_NAME.as_mut()}, utf8);
        name.len = len as u8;
        name.payload[..len].copy_from_slice(payload);
    }
    name.send(r)
}

fn gps_reset(message: &MessageBuf) -> Result {
    let gpiob = unsafe {&*stm32h503::GPIOB::ptr()};
    let message = Message::<u8>::from_buf(message)?;
    if message.payload != 1 {
        gpiob.BSRR.write(|w| w.BR1().set_bit());
    }
    if message.payload > 1 {
        // Sleep for approx, 1ms.
        for _ in 0 .. crate::cpu::CPU_FREQ / 2000 {
            crate::cpu::nothing();
        }
    }
    if message.payload != 0 {
        gpiob.BSRR.write(|w| w.BS1().set_bit());
    }
    SEND_ACK
}

fn lmk_powerdown(message: &MessageBuf) -> Result {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let message = Message::<u8>::from_buf(message)?;
    if message.payload != 1 {
        gpioa.BSRR.write(|w| w.BR4().set_bit());
    }
    if message.payload > 1 {
        // Sleep for approx, 1µs.
        for _ in 0 .. crate::cpu::CPU_FREQ / 2000000 {
            crate::cpu::nothing();
        }
    }
    if message.payload != 0 {
        gpioa.BSRR.write(|w| w.BS4().set_bit());
    }
    SEND_ACK
}

fn serial_sync(message: &MessageBuf) -> Result {
    let message = Message::<u32>::from_buf(message)?;
    if message.payload > 1000000 {
        return Err(Error::BadParameter);
    }
    crate::gps_uart::wait_for_tx_idle();
    for _ in 0 .. message.payload * (crate::cpu::CPU_FREQ / 2000000) {
        crate::cpu::nothing();
    }
    crate::gps_uart::wait_for_tx_idle();
    SEND_ACK
}

fn set_get_baud(message: &MessageBuf, r: Responder) -> Result {
    let _prio = GpsPriority::default();
    if message.len > 0 {
        let message = Message::<u32>::from_buf(message)?;
        crate::gps_uart::set_baud_rate(message.payload);
    }
    Message::<u32>::new(0x9f, crate::gps_uart::get_baud_rate()).send(r)
}

fn i2c_write(address: u8, message: &MessageBuf) -> Result {
    dbgln!("I2C write {address:#04x} length {}", message.len);
    // Write.
    if let Ok(()) = i2c::write(address, message.get_payload()).wait() {
        SEND_ACK
    }
    else {
        Err(Error::Failed)
    }
}

fn i2c_read(address: u8, message: &MessageBuf, r: Responder) -> Result {
    // Get the length...
    let mlen = message.len as usize;
    if mlen < 1 {
        return Err(Error::BadFormat);
    }
    let rlen = message.payload[0] as usize;
    dbgln!("I2C read {address:#04x} wlen {} rlen {}", mlen - 2, rlen);
    if rlen > MAX_PAYLOAD {
        return Err(Error::BadParameter);
    }
    let mut result = MessageBuf::start(message.code | 0x80);
    result.len = rlen as u8;
    let w;
    if mlen == 1 {
        w = i2c::read(address, &mut result.payload[..rlen]);
    }
    else {
        w = i2c::write_read(address, &message.payload[1..mlen],
                            &mut result.payload[..rlen]);
    }
    if let Ok(()) = w.wait() {
        result.send(r)
    }
    else {
        Err(Error::Failed)
    }
}

fn lmk05318b_status(message: &MessageBuf) -> Result {
    Message::<()>::from_buf(message)?;
    // We run at the correct priority, so we can just call the appropriate ISR
    // directly!
    crate::lmk05318b::update_status();
    SEND_ACK
}

fn peek(message: &MessageBuf, r: Responder) -> Result {
    let message = Message::<(u32, u32)>::from_buf(message)?;
    let (address, length) = message.payload;
    let length = length as usize;
    if length > MAX_PAYLOAD - 4 {
        return Err(Error::BadParameter);
    }
    let mut result = MessageBuf::start(0xf1);
    // Place the address at the start of the response.
    result.len = length as u8 + 4;
    result.payload[..4].copy_from_slice(&address.to_le_bytes());
    unsafe {vcopy_aligned(&mut result.payload[4] as *mut u8,
                          address as *const u8, length)};
    result.send(r)
}

fn poke(message: &MessageBuf) -> Result {
    if message.len < 4 {
        return Err(Error::BadParameter);
    }
    let address = unsafe {* (&message.payload as *const _ as *const usize)};
    // Special case writes to flash.
    if address < 0x20000000 {
        let message = Message::<(u32, [u32; 8])>::from_buf(message)?;
        unsafe {crate::flash::program32(address, &message.payload.1)}?;
    }
    else {
        unsafe {
            vcopy_aligned(address as *mut u8, &message.payload[4] as *const u8,
                          message.len as usize - 4)};
    }
    SEND_ACK
}

fn get_crc(message: &MessageBuf, r: Responder) -> Result {
    let (address, length) = Message::<(u32, u32)>::from_buf(message)?.payload;
    let crc = crate::crc32::compute(address as *const u8, length as usize);
    Message::new(0xf3, (address, length, crc)).send(r)
}

fn flash_erase(message: &MessageBuf) -> Result {
    let address = Message::<usize>::from_buf(message)?.payload;
    crate::flash::erase(address)?;
    SEND_ACK
}

fn test_gps_write(message: &MessageBuf) -> Result {
    dbgln!("test_gps_write");
    let prio = crate::gps_uart::GpsPriority::default();
    while !crate::gps_uart::dma_tx(
            &message.payload as *const u8, message.len as usize) {
        prio.wfe();
    }
    // FIXME - we should wait just for our TX not all GPS TX!
    while crate::gps_uart::dma_tx_busy() {
        prio.wfe();
    }
    SEND_ACK
}


#[test]
fn test_utf16() {
    for s in ["abcd123456", "12🔴3🟥4🛑56🚫7🚨8😷"] {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        let mut place = Vec::new();
        place.resize(utf16.len() + 1, 0);
        str_to_usb(&mut place, s);
        let p0b = place[0].to_ne_bytes();
        assert_eq!(p0b[0] as usize, utf16.len() * 2 + 2);
        assert_eq!(p0b[1], 3);
        assert_eq!(&place[1..], utf16);
    }
}
