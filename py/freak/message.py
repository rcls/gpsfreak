# Freak framing.

import array
import struct
import usb

from dataclasses import dataclass

MAGIC = b'\xce\x93'

ACK=0x0080
NACK=0x0180

PING=0x0000

GET_PROTOCOL_VERSION=0x0200
GET_PROTOCOL_VERSION_RESULT=0x0280
GET_SERIAL_NUMBER=0x0300
GET_SERIAL_NUMBER_RESULT=0x0380

CPU_REBOOT=0x1000
GPS_RESET=0x1100
LMK05318B_PDN=0x1200

PEEK=0x010e
PEEK_DATA=0x810e
POKE=0x020e

LMK05318B_WRITE=0xc80f
LMK05318B_READ=0xc90f
LMK05318B_READ_RESULT=0xc98f

TMP117_WRITE=0x920f
TMP117_READ=0x930f
TMP117_READ_RESULT=0x938f

class RequestFailed(RuntimeError):
    pass

@dataclass
class Message:
    # Magic is implicit.
    code: int
    # len is implicit in payload.
    payload: bytes
    # CRC is implied.
    def frame(self):
        return frame(self.code, self.payload)
    def __str__(self):
        return f'{self.code:#06x} ' + self.payload.hex(' ')

POLY = 0x1021
CRCTAB = array.array('H', (0, POLY))

for i in range(1, 128):
    assert len(CRCTAB) == 2 * i
    dbl = CRCTAB[i] << 1
    dbl = min(dbl, dbl ^ POLY ^ 0x10000)
    CRCTAB.extend((dbl, dbl ^ POLY))
assert len(CRCTAB) == 256

def crc(bb: bytes) -> int:
    result = 0
    for b in bb:
        result = result << 8 & 0xff00 ^ CRCTAB[result >> 8 ^ b]
    return result

assert crc(b'123456789') == 0x31c3
def frame(code: int, payload: bytes) -> bytes:
    message = MAGIC + struct.pack('<HH', code, len(payload)) + payload
    return message + struct.pack('>H', crc(message))

def deframe(message: bytes) -> Message:
    if len(message) < 8:
        raise ValueError('Under-length message')
    if message[:2] != MAGIC:
        raise ValueError('Incorrect magic')
    if crc(message) != 0:
        raise ValueError('Bad CRC')
    code, length = struct.unpack('<HH', message[2:6])
    if len(message) != length + 8:
        raise ValueError('Length mismatch')

    return Message(code, message[6:-2])

def flush(dev: usb.Device) -> None:
    # Flush any stale data.
    try:
        dev.read(0x83, 64, 100)
    except usb.core.USBTimeoutError:
        pass

def transact(dev: usb.Device, code: int, payload: bytes,
             expect: int|None = None) -> Message:
    dev.write(0x03, frame(code, payload))
    result = deframe(bytes(dev.read(0x83, 64, 10000)))
    if expect is None and result.code == NACK:
        raise RequestFailed()
    if expect is not None and result.code != expect:
        raise RequestFailed()
    return result

def command(dev: usb.Device, code: int, payload: bytes = b'') -> Message:
    return transact(dev, code, payload, expect=ACK)

def retrieve(dev: usb.Device, code: int, payload: bytes = b'') -> Message:
    return transact(dev, code, payload, expect= code | 0x0080)

def test_simple():
    code = 0x1234
    payload = b'This is a test'
    assert deframe(frame(code, payload)) == Message(code, payload)
