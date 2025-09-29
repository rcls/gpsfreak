#!/usr/bin/python3

import difflib
import struct

from dataclasses import dataclass

MESSAGES_BY_CODE: dict[int, UBloxMsg] = {}
MESSAGES_BY_NAME: dict[str, UBloxMsg] = {}

def ublox_frame(data: bytes) -> bytes:
    ckA = 0;
    ckB = 0;
    for b in data:
        ckA = (ckA + b) & 255
        ckB = (ckB + ckA) & 255
    #ckA = sum(data)
    #ckB = sum((len(data) - n) * b & 255 for (n, b) in enumerate(data))
    return bytes((0xb5, 0x62)) + data + bytes((ckA & 255, ckB & 255))

def test_ublox_frame_simple() -> None:
    raw = bytes((0x06, 0x8A, 0x09, 0x00, 0x00, 0x01, 0x00, 0x00,
                 0x09, 0x00, 0x05, 0x10, 0x01))
    framed = ublox_frame(raw)
    assert len(framed) == len(raw) + 4
    assert framed[0] == ord('µ')
    assert framed[1] == ord('b')
    assert framed[2: -2] == raw
    assert framed[-2] == 0xb9
    assert framed[-1] == 0x8e

def message_key_check(k: int) -> None:
    assert k & 0x8f00f000 == 0

@dataclass(slots=True, frozen=True)
class UBloxMsg:
    name: str
    code: int
    def frame_payload(self, b: bytes) -> bytes:
        return ublox_frame(struct.pack('<HH', self.code, len(b)) + b)
    @staticmethod
    def get(key: int|str|UBloxMsg):
        if type(key) == UBloxMsg:
            return key
        if type(key) == int:
            return MESSAGES_BY_CODE[key]
        assert type(key) == str
        key = key.removeprefix('UBLOX-')
        try:
            return MESSAGES_BY_NAME[key]
        except KeyError:
            print('Did you mean?',
                  difflib.get_close_matches(key, MESSAGES_BY_NAME))
            raise
