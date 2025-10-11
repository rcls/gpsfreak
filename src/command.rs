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
//! Commands:
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
//!    03 : Get serial number.  Response is 83 with ASCII string payload.
//!
//!    10 : CPU reboot.  No response.
//!    11 : GPS reset. u8 payload.
//!            - 0 assert reset low, 1 deassert reset high, others pulse reset.
//!    12 : Clock gen PDN (reset), u8 payload:
//!            - 0 power down, 1 power up, ≥2 reset & power back up.
//!    1f : Set baud rate, u32 payload.  Useful for provisioning, normal
//!         use cases can use USB CDC ACM.
//!
//!    60 : LMK05318b I²C write.  Payload is sent in a I²C write transaction.
//!
//!    61 : LMK05318b I²C read.  First byte of payload is number of bytes,
//!         if there are subsequent bytes, then these are sent as a write
//!         before a repeated-start read.  Reply is E1 with the read bytes
//!         as payload.
//!
//!    62 : TMP117 I²C write.  Just like 60, but to the TMP117.
//!    63 : TMP117 I²C read.  Just like 61, but to the TMP117.
//!
//!    64, 65 : Reserved for GPS I²C.
//!
//!    ?70 : Unsafe operation unlock.  Needs correct magic data payload.
//!    71 : peek.  Payload is u32 address followed by u32 length.  Response is
//!         F1 with address + data payload.
//!    72 : poke.  Payload is u32 address followed by data bytes.
//!    * Sometime we'll overload this to write to flash?
//!
//!            Both peek and poke will do 32-bit or 16-bit transfers if address
//!            and length are both sufficiently aligned.  Neither guard against
//!            crashing the device or making irreversable changes.

use core::ptr::{read_volatile, write_volatile};

use crate::{gps_uart::GpsPriority, i2c};

mod crc;

macro_rules!dbgln {($($tt:tt)*) => {if true {crate::dbgln!($($tt)*)}};}

pub fn init() {
    crc::init();
}

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
    /// Special number used to indicate that an ACK (not a NACK) should be sent.
    Succeeded      = 6,
}

type Result<T = ()> = core::result::Result<T, Error>;

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

impl MessageBuf {
    fn start(code: u8) -> MessageBuf {
        MessageBuf{magic: MAGIC, code, len: 0, payload: [0; _]}
    }
    fn new(code: u8, payload: &[u8]) -> MessageBuf{
        let len = payload.len();
        let mut result = MessageBuf{
            magic: MAGIC, code, len: len as u8, payload: [0; _],
        };
        result.payload[..len].copy_from_slice(payload);
        result.set_crc();
        result
    }
    fn send(&self) -> Result {
        let len = 4 + self.len as usize + 2;     // Include header and CRC.
        crate::usb::main_tx_response(
            unsafe {core::slice::from_raw_parts(
                self as *const Self as _, len)});
        Ok(())
    }
    fn set_crc(&mut self) {
        let len = self.len as usize;
        let crc = crc::compute(unsafe {core::slice::from_raw_parts(
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
        let mut result = Message{
            magic: MAGIC, code, len: size_of::<P>() as u8, payload,
            crc0: 0, crc1: 0};
        result.set_crc();
        result
    }
    fn set_crc(&mut self) {
        let p: *const u8 = self as *const Self as _;
        let len = size_of::<P>() + 4;
        let crc = crc::compute(unsafe {core::slice::from_raw_parts(p, len)});
        self.crc0 = (crc >> 8) as u8;
        self.crc1 = crc as u8;
    }
    fn send(&self) -> Result {
        dbgln!("Freak TX: @{:?} {:?}", self as *const _, self);
        crate::usb::main_tx_response(
            unsafe {core::slice::from_raw_parts(
                self as *const Self as _, 6 + size_of::<P>())});
        Ok(())
    }
    fn from_buf(message: &MessageBuf) -> Result<&Self> {
        message.check_len(size_of::<P>() as u8)?;
        Ok(unsafe {&*(message as *const MessageBuf as *const Self)})
    }
}

pub fn command_handler(message: &MessageBuf, len: usize) {
    match command_dispatch(message, len) {
        Err(Error::Succeeded) => {let _ = Ack::new(0x80, ()).send();}
        Err(err) => {let _ = Nack::new(0x81, err).send();}
        _ => (),
    }
}

fn command_dispatch(message: &MessageBuf, len: usize) -> Result {
    // dbgln!("Command handler dispatch {:x?}",
    //       unsafe {core::slice::from_raw_parts(message as *const _ as *const u8, len)});
    if message.len as usize + 6 != len {
        dbgln!("Length problem {len} {}", message.len);
        return Err(Error::FramingError);
    }
    if message.code & 0x80 != 0 {
        // Wrong message direction, ignore.
        crate::usb::main_rx_rearm();
        return Ok(());
    }
    if crc::compute(unsafe {core::slice::from_raw_parts(
        message as *const _ as _, len)}) != 0 {
        dbgln!("CRC error {len} {}", message.len);
        return Err(Error::FramingError);
    }

    match message.code {
        0x00 => ping(message),
        0x02 => get_protocol_version(message),
        0x03 => get_serial_number(message),

        0x10 => crate::cpu::reboot(),
        0x11 => gps_reset(message),
        0x12 => lmk_powerdown(message),
        0x1f => set_baud(message),

        0x60 => i2c_write(0xc8, message),
        0x61 => i2c_read (0xc9, message),
        0x62 => i2c_write(0x92, message),
        0x63 => i2c_read (0x93, message),

        0x71 => peek(message),
        0x72 => poke(message),
        0x78 => test_gps_write(message),

        _ => Err(Error::UnknownMessage)
    }
}

fn ping(message: &MessageBuf) -> Result {
    // Send a generic ACK with the same payload.
    MessageBuf::new(0x80, message.get_payload()).send()
}

fn get_protocol_version(message: &MessageBuf) -> Result {
    Message::<()>::from_buf(message)?;
    Message::new(0x82, 1u32).send()
}

fn get_serial_number(message: &MessageBuf) -> Result {
    Message::<()>::from_buf(message)?;
    let sn = crate::cpu::SERIAL_NUMBER.as_ref();
    Message::new(0x83, *sn).send()
}

fn gps_reset(message: &MessageBuf) -> Result {
    let gpioa = unsafe {&*stm32h503::GPIOA::ptr()};
    let message = Message::<u8>::from_buf(message)?;
    if message.payload != 1 {
        gpioa.BSRR.write(|w| w.BR4().set_bit());
    }
    if message.payload > 1 {
        // Sleep for approx, 1ms.
        for _ in 0 .. crate::cpu::CPU_FREQ / 2000 {
            crate::cpu::nothing();
        }
    }
    if message.payload != 0 {
        gpioa.BSRR.write(|w| w.BS4().set_bit());
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

fn set_baud(message: &MessageBuf) -> Result {
    let message = Message::<u32>::from_buf(message)?;
    let _prio = GpsPriority::new();
    crate::gps_uart::set_baud_rate(message.payload);
    SEND_ACK
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

fn i2c_read(address: u8, message: &MessageBuf) -> Result {
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
        result.set_crc();
        result.send()
    }
    else {
        Err(Error::Failed)
    }
}

fn peek(message: &MessageBuf) -> Result {
    let message = Message::<(u32, u32)>::from_buf(message)?;
    let (address, length) = message.payload;
    let length = length as usize;
    if length > MAX_PAYLOAD {
        return Err(Error::BadParameter);
    }
    let mut result = MessageBuf::start(0xf1);
    unsafe {vcopy_aligned(address as *mut u8, &result.payload as *const u8,
                          length)};
    result.set_crc();
    result.send()
}

fn poke(message: &MessageBuf) -> Result {
    if message.len < 4 {
        return Err(Error::BadParameter);
    }
    let address = unsafe {* (&message.payload as *const _ as *const u32)};
    unsafe {
        vcopy_aligned(address as *mut u8, &message.payload[4] as *const u8,
                      message.len as usize - 4)};
    SEND_ACK
}

unsafe fn vcopy_aligned(dest: *mut u8, src: *const u8, length: usize) {
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

fn test_gps_write(message: &MessageBuf) -> Result {
    dbgln!("test_gps_write");
    let prio = crate::gps_uart::GpsPriority::new();
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
