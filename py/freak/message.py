# Freak framing.

import array
import struct

from collections.abc import ByteString
from dataclasses import dataclass
from typing import TypeAlias
from usb.core import Device

Target: TypeAlias = Device | bytearray

MAGIC = b'\xce\x93'

ACK=0x80
NACK=0x81

PING=0x00
GET_PROTOCOL_VERSION=0x02
GET_SERIAL_NUMBER=0x03

CPU_REBOOT=0x10
GPS_RESET=0x11
LMK05318B_PDN=0x12

SERIAL_SYNC=0x1e
GET_SET_BAUD=0x1f

LMK05318B_WRITE=0x60
LMK05318B_READ=0x61
LMK05318B_STATUS=0x68

TMP117_WRITE=0x62
TMP117_READ=0x63

PEEK=0x71
POKE=0x72
GET_CRC=0x73
FLASH_ERASE=0x74

class RequestFailed(RuntimeError):
    pass

@dataclass
class Message:
    # Magic is implicit.
    code: int
    # len is implicit in payload.
    payload: bytes
    # CRC is implied.
    def frame(self) -> bytes:
        return frame(self.code, self.payload)
    def __str__(self) -> str:
        return f'{self.code:#06x} ' + self.payload.hex(' ')

POLY = 0x1021
CRCTAB = array.array('H', (0, POLY))

for i in range(1, 128):
    assert len(CRCTAB) == 2 * i
    dbl = CRCTAB[i] << 1
    dbl = min(dbl, dbl ^ POLY ^ 0x10000)
    CRCTAB.extend((dbl, dbl ^ POLY))
assert len(CRCTAB) == 256

def crc16(bb: ByteString) -> int:
    result = 0
    for b in bb:
        result = result << 8 & 0xff00 ^ CRCTAB[result >> 8 ^ b]
    return result

assert crc16(b'123456789') == 0x31c3

def frame(code: int, payload: ByteString, limit: int = 64) -> bytes:
    assert len(payload) + 6 <= limit
    message = MAGIC + bytes((code, len(payload))) + payload
    return message + struct.pack('>H', crc16(message))

def deframe(message: bytes) -> Message:
    if len(message) < 6:
        raise ValueError('Under-length message')
    if message[:2] != MAGIC:
        raise ValueError('Incorrect magic')
    if crc16(message) != 0:
        raise ValueError('Bad CRC')
    code = message[2]
    length = message[3]
    if len(message) != length + 6:
        raise ValueError('Length mismatch')

    return Message(code, message[4:-2])

def test_simple() -> None:
    code = 0x12
    payload = b'This is a test'
    assert deframe(frame(code, payload)) == Message(code, payload)

def command(dev: Target, code: int, payload: ByteString,
            expect: int = ACK) -> Message:
    data = frame(code, payload)
    if isinstance(dev, bytearray):
        dev += data
        return Message(ACK, b'')
    dev.write(0x03, frame(code, payload)) # type: ignore
    result = deframe(bytes(dev.read(0x83, 64, 10000))) # type: ignore
    if expect != NACK and result.code == NACK:
        raise RequestFailed(f'Result code is NACK ' + result.payload.hex(' '))
    if result.code != expect:
        raise RequestFailed(f'Result code is {result.code:#04x}')
    return result

def retrieve(dev: Device, code: int, payload: bytes = b'') -> Message:
    return command(dev, code, payload, expect = code | 0x80)

def ping(dev: Device, payload: bytes) -> bytes:
    resp = retrieve(dev, PING, payload)
    assert resp.payload == payload
    return resp.payload

def get_protocol_version(dev: Device) -> int:
    data = retrieve(dev, GET_PROTOCOL_VERSION, b'')
    return struct.unpack('<I', data.payload)[0]

def get_serial_number(dev: Device) -> bytes:
    return retrieve(dev, GET_SERIAL_NUMBER, b'').payload

def serial_sync(dev: Target, microseconds: int) -> None:
    command(dev, SERIAL_SYNC, struct.pack('<I', microseconds))

def set_baud(dev: Target, baud: int) -> None:
    command(dev, GET_SET_BAUD, struct.pack('<I', baud), GET_SET_BAUD | 0x80)

def get_baud(dev: Device) -> int:
    resp = retrieve(dev, GET_SET_BAUD, b'')
    return struct.unpack('<I', resp.payload)[0]

def peek(dev: Device, address: int, length: int) -> bytes:
    result = b''
    while len(result) < length:
        todo = min(length - len(result), 48)
        a = address + len(result)
        data = retrieve(dev, PEEK, struct.pack('<II', a, todo))
        aa = struct.unpack('<I', data.payload[:4])[0]
        assert a == aa
        assert len(data.payload) == todo + 4
        result += data.payload[4:]
    return result

def poke(dev: Target, address: int, data: ByteString, chunk_size: int = 32) -> None:
    base = 0
    while base < len(data):
        todo = min(chunk_size, len(data) - base)
        #print(f'POKE @ {address+base:#010x} + {todo}')
        command(dev, POKE,
                struct.pack('<I', address + base) + data[base:base + todo])
        base += todo

def crc(dev: Device, address: int, length: int) -> int:
    data = retrieve(dev, GET_CRC, struct.pack('<II', address, length))
    a, l, crc = struct.unpack('<III', data.payload)
    assert a == address
    assert l == length
    return crc

def flash_erase(dev: Target, address: int) -> None:
    command(dev, FLASH_ERASE, struct.pack('<I', address))

def lmk05318b_read(dev: Device, address: int, length: int) -> bytes:
    r = retrieve(dev, LMK05318B_READ, struct.pack('>BH', length, address))
    assert len(r.payload) == length
    return r.payload

def lmk05318b_write(dev: Target, address: int, *data: ByteString|int) -> None:
    def bb(x: ByteString|int) -> ByteString:
        return bytes((x,)) if isinstance(x, int) else x
    total = b''.join(map(bb, data))
    command(dev, LMK05318B_WRITE, struct.pack('>H', address) + total)

def lmk05318b_status(dev: Target) -> None:
    command(dev, LMK05318B_STATUS, b'')

def tmp117_read(dev: Device, address: int, length: int = 1) -> bytes:
    r = retrieve(dev, TMP117_READ, bytes((length, address)))
    assert len(r.payload) == length
    return r.payload

def tmp117_write(dev: Target, address: int, data: bytes) -> None:
    command(dev, TMP117_WRITE, bytes((address,)) + data)
