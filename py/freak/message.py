# Freak framing.

import array
import struct
import usb

from dataclasses import dataclass

MAGIC = b'\xce\x93'

ACK=0x80
NACK=0x81

PING=0x00

GET_PROTOCOL_VERSION=0x02
GET_PROTOCOL_VERSION_RESULT=0x82
GET_SERIAL_NUMBER=0x03
GET_SERIAL_NUMBER_RESULT=0x83

CPU_REBOOT=0x10
GPS_RESET=0x11
LMK05318B_PDN=0x12

PEEK=0x71
PEEK_DATA=0xf1
POKE=0x72

LMK05318B_WRITE=0x60
LMK05318B_READ=0x61
LMK05318B_READ_RESULT=0xe1

TMP117_WRITE=0x62
TMP117_READ=0x63
TMP117_READ_RESULT=0xe3

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

def frame(code: int, payload: bytes, limit: int = 64) -> bytes:
    assert len(payload) + 6 <= limit
    message = MAGIC + bytes((code, len(payload))) + payload
    return message + struct.pack('>H', crc(message))

def deframe(message: bytes) -> Message:
    if len(message) < 6:
        raise ValueError('Under-length message')
    if message[:2] != MAGIC:
        raise ValueError('Incorrect magic')
    if crc(message) != 0:
        print(message.hex(' '))
        exp0 = crc(message[:-4])
        exp1 = crc(message[:-3])
        exp2 = crc(message[:-2])
        exp3 = crc(message[:-2] + b'\x00\x00')
        print(f'{exp0:#06x} {exp1:#06x} {exp2:#06x} {exp3:#06x}')
        raise ValueError('Bad CRC')
    code = message[2]
    length = message[3]
    if len(message) != length + 6:
        raise ValueError('Length mismatch')

    return Message(code, message[4:-2])

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
    if expect != NACK and result.code == NACK:
        raise RequestFailed(f'Result code is NACK ' + result.payload.hex(' '))
    if expect is not None and result.code != expect:
        raise RequestFailed(f'Result code is {result.code:#04x}')
    return result

def command(dev: usb.Device, code: int, payload: bytes = b'') -> Message:
    return transact(dev, code, payload, expect=ACK)

def retrieve(dev: usb.Device, code: int, payload: bytes = b'') -> Message:
    return transact(dev, code, payload, expect= code | 0x80)

def test_simple():
    code = 0x1234
    payload = b'This is a test'
    assert deframe(frame(code, payload)) == Message(code, payload)
