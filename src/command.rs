#![allow(dead_code)]

//! Commands are binary encoded.
//!
//! The format is
//!
//!     magic   : u16 // Bytes CE 93
//!     code    : u16 // First byte is grouping
//!     len     : u16
//!     payload : [u8; len]
//!     checksum: u16
//!
//! The magic is the byte sequence CE 93 (little endian 0x93ce), which is
//! UTF-8 for 'Γ' (GREEK UPPER CASE GAMMA).
//!
//! The code is two bytes, the first byte defines groupings of messages.
//! The high bit of the first byte is used as a direction: 0 is to the device,
//! 1 is from the device.  Spontaneous messages from the device always have the
//! two highest bits set (0xC0 ..= 0xFF) [are we going to do any?].
//!
//! Any request to the device gets a response, a ACK or NAK if nothing else.
//!
//! Note that if the device gets a request code indicating a message from the
//! device, then it does not respond.  This avoids
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
//! Command groups:
//! 00 xx : Overall device control
//!    00 00 : PING.  Arbitrary payload.  Response is 80 00 and echos the
//!            payload.  By sending a random token, you can check that
//!            messages are synchronised.
//!    80 00 : ACK. Generic Acknowledgement.  Payload is generally empty.
//!            Ping responses echo the payload.  Otherwise if non-empty,
//!            then is an informational UTF-8 string.
//!
//!    80 01 : NAK. Generic failure.  A request could not be
//!            successfully executed.  Two u16 payload fields.  error and detail
//!            See below for the error enumeration.
//!
//!    00 10 : CPU reboot.  No response.
//!    00 11 : GPS reset.  Response is generic ACK.
//!    00 12 : Clock gen reset.  Response is generic ACK (or NACK).
//!
//! 0E xx : Low level operations.
//!    0E 01 : peek.
//!    0E 02 : poke.
//!
//! 0F xx : Raw I²C bus operations
//!    Note that because we are the (only) bus controller, we don't worry about
//!    repeated starts.
//!
//!    xx is the address as per the I²C bus: the low bit determines write (0) or
//!    read (1).
//!
//!    Write operations carry the I²C bytes as a payload.  (length = number of
//!    bytes).  Write responses are generic acks on successes, or a NAK with
//!    the error detail giving the number of bytes before the I²C NACK
//!    (or other failure).
//!
//!    Read operations carry the u16 transaction length as a payload.
//!    (length=2).  Response are 8F xx followed by the data transfered.  If
//!    an error occurs, then the payload is short.

use crate::i2c;

mod crc;

macro_rules!dbgln {($($tt:tt)*) => {if true {crate::dbgln!($($tt)*)}};}

pub fn init() {
    crc::init();
}

/// Error codes for Nack responses.
#[repr(u16)]
#[derive(Debug, Default)]
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
}

/// Maximum message payload size for a message.  Currently we only support
/// messages up to 64 bytes total.
const MAX_PAYLOAD: usize = 56;

/// A struct representing a message.
#[repr(C)]
pub struct MessageBuf {
    magic  : u16,
    code   : u16,
    len    : u16,
    /// The payload includes the CRC.
    payload: [u8; MAX_PAYLOAD + 2],
}
const _: () = assert!(size_of::<MessageBuf>() == 64);

#[derive(Debug, Default)]
#[repr(C)]
pub struct Message<P> {
    magic  : u16,
    code   : u16,
    len    : u16,
    payload: P,
    crc    : u16,
}

impl MessageBuf {
    fn start(code: u16) -> MessageBuf {
        MessageBuf{magic: MAGIC, code, len: 0, payload: [0; _]}
    }
    fn new(code: u16, payload: &[u8]) -> MessageBuf{
        let len = payload.len();
        let mut result = MessageBuf{
            magic: MAGIC, code, len: len as u16, payload: [0; _],
        };
        result.payload[..len].copy_from_slice(payload);
        let crc = crc::compute(unsafe {core::slice::from_raw_parts(
            &result as *const Self as _, 6 + len
        )});
        result.payload[len] = (crc >> 8) as u8;
        result.payload[len + 1] = crc as u8;
        result
    }
    fn send(&self) {
        let len = 6 + self.len + 2;     // Include header and CRC.
        crate::usb::main_tx_response(
            unsafe {core::slice::from_raw_parts(
                self as *const Self as _, len as usize)});
    }
    fn set_crc(&mut self) {
        let len = self.len as usize;
        let crc = crc::compute(unsafe {core::slice::from_raw_parts(
            self as *const Self as _, 6 + len
        )});
        self.payload[len] = (crc >> 8) as u8;
        self.payload[len + 1] = crc as u8;
    }
    fn get_payload(&self) -> &[u8] {
        &self.payload[.. self.len as usize]
    }
    fn check_len(&self, len: u16) -> Result<&MessageBuf> {
        if self.len == len {Ok(self)} else {Err((Error::BadFormat, 0))}
    }
}

impl<P: core::fmt::Debug> Message<P> {
    fn new(code: u16, payload: P) -> Message<P> {
        let mut result = Message{
            magic: MAGIC, code, len: size_of::<P>() as u16, payload, crc: 0};
        result.set_crc();
        result
    }
    fn compute_crc(&self) -> u16 {
        let p: *const u8 = self as *const Self as _;
        let len = size_of::<Self>() - 2;
        crc::compute(unsafe {core::slice::from_raw_parts(p, len)})
    }
    fn set_crc(&mut self) {
        self.crc = self.compute_crc().to_be();
    }
    fn send(&self) {
        dbgln!("Freak TX: @{:?} {:?}", self as *const _, self);
        crate::usb::main_tx_response(
            unsafe {core::slice::from_raw_parts(
                self as *const Self as _, size_of::<Self>())});
    }
    fn from_buf(message: &MessageBuf) -> Result<&Self> {
        message.check_len(size_of::<P>() as u16)?;
        Ok(unsafe {&*(message as *const MessageBuf as *const Self)})
    }
}

type Ack  = Message<()>;
type Nack = Message<(Error, u16)>;

type I2CRead = Message<u16>;

const MAGIC: u16 = 0xce93u16.to_be();

type Result<T = ()> = core::result::Result<T, (Error, u16)>;

pub fn command_handler(message: &MessageBuf, len: usize) {
    if let Err(ed) = command_dispatch(message, len) {
        Nack::new(0x8001u16.to_be(), ed).send();
    }
}

fn command_dispatch(message: &MessageBuf, len: usize) -> Result {
    dbgln!("Command handler dispatch {:x?}",
           unsafe {core::slice::from_raw_parts(message as *const _ as *const u8, len)});
    if len < 8 || message.len as usize != len - 8 {
        dbgln!("Length problem {len} {}", message.len);
        return Err((Error::FramingError, 0));
    }
    if message.code & 0x80 != 0 {
        // Wrong message direction, ignore.
        return Ok(());
    }
    if crc::compute(unsafe {core::slice::from_raw_parts(
        message as *const _ as _, len)}) != 0 {
        dbgln!("CRC error {len} {}", message.len);
        return Err((Error::FramingError, 0));
    }

    match (message.code & 0xff, message.code >> 8) {
        (0x00, 0x00) => ping(message),
        (0x00, 0x01) => crate::cpu::reboot(),
        (0x0f, _)    => i2c_transact(message),
        _ => Err((Error::UnknownMessage, 0))
    }
}

fn ping(message: &MessageBuf) -> Result {
    // Send a generic ACK with the same payload.
    MessageBuf::new(0x0080, message.get_payload()).send();
    Ok(())
}

fn i2c_transact(message: &MessageBuf) -> Result {
    // Low bit is RD/WRn.
    let address = (message.code >> 8) as u8;
    if address & 1 == 0 {
        dbgln!("I2C write {address:#04x} length {}", message.len);
        // Write.
        if let Ok(()) = i2c::write(address, message.get_payload()).wait() {
            Ack::new(0x0080, ()).send();
            Ok(())
        }
        else {
            // FIXME - detail.
            Err((Error::Failed, 0))
        }
    }
    else {
        // Get the length...
        // Read.
        let message = Message::<u16>::from_buf(message)?;
        let len = message.payload as usize;
        dbgln!("I2C read {address:#04x} length {}", len);
        if len > MAX_PAYLOAD {
            return Err((Error::Failed, 0));
        }
        let mut result = MessageBuf::start(message.code | 0x0080);
        result.len = len as u16;
        if let Ok(()) = i2c::read(address, &mut result.payload[..len]).wait() {
            result.set_crc();
            result.send();
            Ok(())
        }
        else {
            // FIXME - detail.
            Err((Error::Failed, 0))
        }
    }
}