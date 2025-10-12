#!/usr/bin/python3

import difflib
import io
import os
from freak import serhelper
import struct

from dataclasses import dataclass
from typing import Tuple

MESSAGES_BY_CODE: dict[int, UBloxMsg] = {}
MESSAGES_BY_NAME: dict[str, UBloxMsg] = {}

def checksum(data: bytes) -> Tuple[int, int]:
    ckA = 0;
    ckB = 0;
    for b in data:
        ckA = (ckA + b) & 255
        ckB = (ckB + ckA) & 255
    #ckA = sum(data)
    #ckB = sum((len(data) - n) * b & 255 for n, b in enumerate(data))
    return ckA, ckB

def ublox_frame(data: bytes) -> bytes:
    ckA, ckB = checksum(data)
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


@dataclass(slots=True, frozen=True)
class UBloxMsg:
    name: str
    code: int
    def frame_payload(self, b: bytes) -> bytes:
        return ublox_frame(struct.pack('<HH', self.code, len(b)) + b)
    @staticmethod
    def get(key: int|str|UBloxMsg) -> UBloxMsg:
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
    def __repr__(self) -> str:
        return f'UBloxMsg({self.name!r}, {self.code:#06x})'

def add_msg_list(l: list[UBloxMsg]) -> None:
    for msg in l:
        MESSAGES_BY_NAME[msg.name] = msg
        MESSAGES_BY_CODE[msg.code] = msg
MAX_LENGTH = 1024

class UBloxReader:
    source: io.FileIO
    current: bytearray
    def __init__(self, source: io.FileIO):
        self.source = source
        self.current = bytearray()
    def read_more(self) -> None:
        data = self.source.read(1024)
        if data == '':
            raise EOFError()
        #print(repr(data))
        self.current += data
    def get_msg(self) -> Tuple[int, bytes]:
        more = False
        while True:
            if more:
                self.read_more()
            more = True
            mu = self.current.find(b'\xb5')
            if mu < 0:
                self.current = bytearray()
                continue
            del self.current[:mu]
            if len(self.current) < 8:   # Minimum packet length.
                continue
            if self.current[1] != 0x62:
                #print('No b', self.current)
                del self.current[:2]
                continue
            length, = struct.unpack('<H', self.current[4:6])
            if length > MAX_LENGTH:
                #print(f'Too long {length}')
                del self.current[:2]
                continue
            msg_len = length + 8
            if len(self.current) < msg_len:
                #print(f'Need more {msg_len}')
                continue
            # Ok, it looks like we have a message.
            message = bytes(self.current[:msg_len])
            #print('Try', message)
            del self.current[:msg_len]
            ckA, ckB = checksum(message[2:-2])
            if message[-2] == ckA and message[-1] == ckB:
                return struct.unpack('<H', message[2:4])[0], message[6:-2]
            #print('Checksum fail')
            more = False

    def command(self, b: bytes) -> None:
        serhelper.flushread(self.source)
        serhelper.writeall(self.source, b)
        self.source.flush()
        self.get_ack(b)

    def get_ack(self, b: bytes) -> None:
        code, payload = self.get_msg()
        # Check we have the correct ACK.
        assert code == 0x0105, f'{code:#x}'
        assert len(payload) == 2
        assert payload[0] == b[2]
        assert payload[1] == b[3]

    def transact(self, b: bytes, ack: bool = True) -> bytes:
        serhelper.flushread(self.source)
        serhelper.writeall(self.source, b)
        self.source.flush()
        code, payload = self.get_msg()
        assert code == struct.unpack('<H', b[2:4])[0], f'{code:#x}'
        if ack:
            self.get_ack(b)

        return payload

import freak.ublox_lists
