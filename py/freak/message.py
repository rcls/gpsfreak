# Freak framing.

import array
import struct
import usb

from dataclasses import dataclass
from typing import TypeAlias
from usb import Device

Target: TypeAlias = Device|bytearray

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
SET_BAUD=0x1f

LMK05318B_WRITE=0x60
LMK05318B_READ=0x61

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

def crc16(bb: bytes) -> int:
    result = 0
    for b in bb:
        result = result << 8 & 0xff00 ^ CRCTAB[result >> 8 ^ b]
    return result

assert crc16(b'123456789') == 0x31c3

def frame(code: int, payload: bytes, limit: int = 64) -> bytes:
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

def flush(dev: Device) -> None:
    # Flush any stale data.
    try:
        dev.read(0x83, 64, 10)
    except usb.core.USBTimeoutError:
        pass

def transact(dev: Target, code: int, payload: bytes,
             expect: int|None = None) -> Message:
    data = frame(code, payload)
    if isinstance(dev, bytearray):
        dev += data
        return Message(ACK, b'')
    dev.write(0x03, frame(code, payload))
    result = deframe(bytes(dev.read(0x83, 64, 10000)))
    if expect != NACK and result.code == NACK:
        raise RequestFailed(f'Result code is NACK ' + result.payload.hex(' '))
    if expect is not None and result.code != expect:
        raise RequestFailed(f'Result code is {result.code:#04x}')
    return result

def command(dev: Target, code: int, payload: bytes = b'') -> Message:
    return transact(dev, code, payload, expect=ACK)

def retrieve(dev: Device, code: int, payload: bytes = b'') -> Message:
    return transact(dev, code, payload, expect= code | 0x80)

def get_device() -> Device:
    dev = usb.core.find(idVendor=0xf055, idProduct=0xd448)
    # Flush any stale data.
    flush(dev)
    return dev

def ping(dev: Target, payload: bytes) -> bytes:
    resp = retrieve(dev, PING, payload)
    assert resp.payload == payload
    return resp.payload

def get_protocol_version(dev: Target) -> int:
    data = retrieve(dev, GET_PROTOCOL_VERSION, b'')
    return struct.unpack('<I', data.payload)[0]

def get_serial_number(dev: Target) -> bytes:
    return retrieve(dev, GET_SERIAL_NUMBER, b'').payload

def serial_sync(dev: Target, microseconds: int) -> None:
    command(dev, SERIAL_SYNC, struct.pack('<I', microseconds))

def set_baud(dev: Target, baud: int) -> None:
    command(dev, SET_BAUD, struct.pack('<I', baud))

def peek(dev: Target, address: int, length: int) -> bytes:
    data = retrieve(dev, PEEK, struct.pack('<II', address, length))
    a = struct.unpack('<I', data.payload[:4])[0]
    assert a == address
    assert len(data.payload) == length + 4
    return data.payload[4:]

def poke(dev: Target, address: int, data: bytes, chunk_size: int = 32) -> None:
    base = 0
    while base < len(data):
        todo = min(chunk_size, len(data) - base)
        command(dev, POKE,
                struct.pack('<I', address + base) + data[base:base + todo])
        base += todo

def crc(dev: Target, address: int, length: int) -> int:
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

def lmk05318b_write(dev: Target, address: int, *data: bytes|int) -> None:
    def bb(x: bytes|int) -> bytes:
        return bytes((x,)) if isinstance(x, int) else x
    total = b''.join(map(bb, data))
    command(dev, LMK05318B_WRITE, struct.pack('>H', address) + total)

def tmp117_read(dev: Target, address: int, length: int = 1) -> bytes:
    r = retrieve(dev, TMP117_READ, bytes((length, address)))
    assert len(r.payload) == length
    return r.payload

def tmp117_write(dev: Target, address: int, data: bytes) -> None:
    command(dev, TMP117_WRITE, bytes((address,)) + data)
