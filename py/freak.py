# Freak framing.

import array
import struct

from dataclasses import dataclass

MAGIC = b'\xce\x93'

@dataclass
class Message:
    # Magic is implicit.
    code: int
    # len is implicit in payload.
    payload: bytes
    # CRC is implied.

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

def test_simple():
    code = 0x1234
    payload = b'This is a test'
    assert deframe(frame(code, payload)) == Message(code, payload)
