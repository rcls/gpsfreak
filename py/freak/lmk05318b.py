
from __future__ import annotations

import dataclasses
import difflib
import os
import pickle
import struct
import re

from collections.abc import ByteString
from dataclasses import dataclass
from typing import Any, Callable, Tuple

'''Extract the bit position suffix from a field name.'''
SUBNAME_RE = re.compile(r'([^:]+)(_(\d+):(\d+))?$')

BundledBytes = dict[int, bytearray]

@dataclass
class Field:
    name: str
    byte_hi: int
    byte_lo: int
    access: str
    reset: int
    address: int
    basename: str = dataclasses.field(init=False)
    reg_hi: int = dataclasses.field(init=False)
    reg_lo: int = dataclasses.field(init=False)

    def __post_init__(self) -> None:
        sn = SUBNAME_RE.match(self.name)
        assert sn is not None
        basename, _, hi, lo = sn.groups()
        self.basename = basename
        if hi:
            self.reg_hi = int(hi)
            self.reg_lo = int(lo)
        else:
            self.reg_hi = self.byte_hi - self.byte_lo
            self.reg_lo = 0

    def mask(self) -> int:
        return (1 << self.byte_hi + 1) - (1 << self.byte_lo)
    def __str__(self) -> str:
        return self.name

@dataclass
class Address:
    address: int
    fields: list[Field]
    read_only: bool = True

    def __str__(self) -> str:
        return f'R{self.address}'
    def validate(self) -> None:
        mask = 0
        for field in self.fields:
            assert field.access in ('R', 'R/W', 'R/WSC'), field.access
            if 'W' in field.access:
                self.read_only = False
            f_mask = field.mask()
            assert f_mask & mask == 0, f'{self} {field} {f_mask} {mask}'
            mask |= f_mask
        assert mask == 255, f'{self} {mask}'

@dataclass
class Register:
    name: str
    fields: list[Field]
    base_address: int = 0
    byte_span: int = 0
    shift: int = 0
    width: int = 0
    access: str = 'RW'
    reset: int = 0

    def __str__(self) -> str:
        return self.name

    def validate(self) -> None:
        '''Check that the fields pack sensibly.'''
        self.fields.sort(key=lambda f: f.reg_lo)
        assert self.fields[0].reg_lo == 0
        # Everything appears to be big endian
        for a, b in zip(self.fields, self.fields[1:]):
            assert a.reg_hi + 1 == b.reg_lo
            assert a.byte_hi == 7, self
            assert b.byte_lo == 0
            assert a.address == b.address + 1
        first = self.fields[ 0]
        last  = self.fields[-1]
        self.base_address = last.address
        self.byte_span = first.address - last.address + 1
        self.shift = first.byte_lo
        self.width = last.reg_hi + 1
        self.access = self.fields[0].access
        assert all(self.access == field.access for field in self.fields)
        # All multibyte fields have any partial byte in the high byte.
        if self.byte_span > 1:
            assert self.shift == 0
        assert self.width == sum(f.byte_hi - f.byte_lo + 1 for f in self.fields)
        reset = 0
        for f in self.fields:
            reset |= f.reset << f.reg_lo
        self.reset = reset

    def extract(self, bb: ByteString) -> int:
        b = bb[self.base_address : self.base_address + self.byte_span]
        value = struct.unpack('>Q', (b'\0\0\0\0\0\0\0' + b)[-8:])[0]
        value = value >> self.shift
        value &= (1 << self.width) - 1
        return value

    @staticmethod
    def get(key: str) -> Register:
        key = key.upper().replace('-', '_')
        try:
            return REGISTERS[key]
        except KeyError:
            prompt = ' '.join(difflib.get_close_matches(key, REGISTERS))
            if prompt:
                print(f'Did you mean: {prompt}?')
            raise

DATA_SIZE = 500

def skip(R: int) -> bool:
    return R < 8 or R >= 353 or R in (12, 157, 164)

class MaskedBytes:
    data: bytearray
    mask: bytearray
    def __init__(self):
        self.data = bytearray(DATA_SIZE)
        self.mask = bytearray(DATA_SIZE)

    def bundle(self, ro: bool = True, max_block: int = 1000,
               defaults:MaskedBytes|None = None) -> BundledBytes:
        result: BundledBytes = {}
        current_addr = 0
        current_data: bytearray|None = None
        for i in range(DATA_SIZE):
            data = self.data[i]
            mask = self.mask[i]
            if mask == 0:
                continue
            if mask != 255 and defaults is not None:
                data = data & mask | defaults.data[i] & ~mask
            if not ro and skip(i):
                continue
            if current_data is not None \
               and i == current_addr + len(current_data) \
               and len(current_data) < max_block:
                current_data.append(data)
            else:
                current_addr = i
                current_data = bytearray((data,))
                result[current_addr] = current_data
        return result

    def ranges(self, select: Callable[[int], bool] = lambda m: m != 0,
               max_block: int = 1000) -> list[Tuple[int, int]]:
        '''Return a list of (start, count) of indexes with non-zero mask.'''
        result: list[Tuple[int, int]] = []
        addr = None
        span = 0
        for i, m in enumerate(self.mask):
            if not select(m):
                continue
            if addr is not None and addr + span == i and span < max_block:
                span += 1
                continue
            if addr is not None:
                result.append((addr, span))
            addr = i
            span = 1
        if addr is not None:
            result.append((addr, span))
        return result

    def extract(self, r: Register|str) -> int:
        if isinstance(r, str):
            r = Register.get(r)
        return r.extract(self.data)

    def extract_mask(self, r: Register|str) -> int:
        if isinstance(r, str):
            r = Register.get(r)
        return r.extract(self.mask)

    def insert(self, r: Register|str, value: int) -> None:
        if isinstance(r, str):
            r = Register.get(r)
        value = value << r.shift
        vmask = (1 << r.width) - 1 << r.shift
        data = struct.pack('>Q', value)[-r.byte_span:]
        mask = struct.pack('>Q', vmask)[-r.byte_span:]
        for i in range(r.byte_span):
            j = r.base_address + i
            self.data[j] = (self.data[j] & ~mask[i]) | (data[i] & mask[i])
            self.mask[j] |= mask[i]

    def __getattr__(self, key: str) -> int:
        try:
            reg = REGISTERS[key]
        except KeyError:
            raise AttributeError()
        return self.extract(reg)

    def __setattr__(self, key: str, value: Any) -> None:
        if key in ('data', 'mask'):
            super().__setattr__(key, value)
            return
        try:
            reg = REGISTERS[key]
        except KeyError:
            raise AttributeError()
        self.insert(reg, int(value))

def validate_addresses(addresses: list[Address]) -> None:
    '''Check that each Address has the bytes exactly covered.'''
    for address in addresses:
        address.validate()

def build_registers(addresses: list[Address]) -> dict[str, Register]:
    '''Aggregate fields into registers.'''
    registers: dict[str, Register] = {}

    for address in addresses:
        for field in address.fields:
            if field.name == 'RESERVED':
                continue
            if field.basename in registers:
                registers[field.basename].fields.append(field)
            else:
                registers[field.basename] = Register(field.basename, [field])

    for register in registers.values():
        register.validate()

    # TI did wierd shit here.  These overlap other registers, and some DWIM'ing
    # determines what actually happens.
    #hack = Register('DPLL_REF_UNLOCKDET_CNTSTRT', fields = [],
    #                base_address = 332, byte_span = 4, width = 30)
    #registers[hack.name] = hack
    hack = Register('DPLL_REF_UNLOCKDET_VCO_CNTSTRT', fields = [],
                    base_address = 336, byte_span = 4, width = 30)
    registers[hack.name] = hack

    return registers

ADDRESSES: list[Address] = pickle.load(
    open(os.path.dirname(__file__) + '/lmk05318b-registers.pickle', 'rb'))

ADDRESS_BY_NUM: dict[int, Address] = {}

for address in ADDRESSES:
    address.validate()
    ADDRESS_BY_NUM[address.address] = address

REGISTERS = build_registers(ADDRESSES)
