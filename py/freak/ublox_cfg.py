#!/usr/bin/python3

import difflib
import struct

from dataclasses import dataclass
from typing import Any, Tuple

UBX_TYPES = {
    'I1': 'b', 'I2': 'h', 'I4': 'i',
    'U1': 'B', 'U2': 'H', 'U4': 'I',
    'E1': 'B', # 'E2': 'H', 'E4': 'I',
    'X1': 'B', 'X2': 'H', 'X4': 'I', 'X8': 'Q',
    'L' : '?',            'R4': 'f', 'R8': 'd',
}

CONFIGS_BY_KEY :dict[int, UBloxCfg] = {}
CONFIGS_BY_NAME:dict[str, UBloxCfg] = {}

def val_byte_len(key: int) -> int:
    return (0, 1, 1, 2, 4, 8)[key >> 28]

def get_cfg(key: int) -> UBloxCfg:
    if key in CONFIGS_BY_KEY:
        return CONFIGS_BY_KEY[key]
    vb = val_byte_len(key)
    if key >> 28 == 1:
        ty = 'L'
    else:
        ty = f'X{vb}'
    return UBloxCfg(f'UNKNOWN-{key:08x}', key, ty)

@dataclass(slots=True, frozen=True)
class UBloxCfg:
    name: str
    key : int
    typ : str
    def __post_init__(self):
        assert self.typ in UBX_TYPES, self.typ
        assert 0 <= self.key < 1<<32
        assert self.key & 0x8f00f000 == 0
        assert (self.key >> 28, self.typ[-1]) in (
            (1, 'L'), (2, '1'), (3, '2'), (4, '4'), (5, '8')), \
            f'{self.key:#x} {self.typ}'

    def val_byte_len(self) -> int:
        return val_byte_len(self.key)

    def encode_value(self, v: int|float|bool) -> bytes:
        return struct.pack('<' + UBX_TYPES[self.typ], v)

    def encode_key_value(self, v: int|float|bool) -> bytes:
        return struct.pack('<I' + UBX_TYPES[self.typ], self.key, v)

    def decode_value(self, v: bytes) -> Any:
        return struct.unpack('<' + UBX_TYPES[self.typ], v)[0]

    def to_value(self, s: Any) -> Any:
        '''Typically, s will be a string, but can be anything castable.'''
        match self.typ[0]:
            case 'I'|'U'|'E'|'X':
                return int(s, 0)
            case 'L': return bool(s)
            case 'R': return float(s)
            case _  : assert False

    def __str__(self) -> str:
        return 'CFG-' + self.name

    def __repr__(self) -> str:
        return f'UBloxCfg({self.name!r:}, {self.key:#010x}, {self.typ!r:})'

    @staticmethod
    def decode_from(b: bytes) -> Tuple[UBloxCfg, Any, int]:
        '''Returns (key, value, length)
           The length is the total byte length of the key+value.'''
        cfg = CONFIGS_BY_KEY[struct.unpack('<I', b[:4])[0]]
        length = 4 + cfg.val_byte_len()
        return cfg, cfg.decode_value(b[4:length]), length

    @staticmethod
    def get(key: int|str|UBloxCfg) -> UBloxCfg:
        if type(key) == UBloxCfg:
            return key
        if type(key) == int:
            return CONFIGS_BY_KEY[key]
        assert type(key) == str
        key = key.removeprefix('CFG-')
        try:
            return CONFIGS_BY_NAME[key]
        except KeyError:
            print('Did you mean?',
                  difflib.get_close_matches(key, CONFIGS_BY_NAME))
            raise

def add_cfg_list(l: list[UBloxCfg]) -> None:
    for cfg in l:
        CONFIGS_BY_NAME[cfg.name] = cfg
        CONFIGS_BY_KEY [cfg.key ] = cfg

import freak.ublox_lists
