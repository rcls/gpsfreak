
import os
import pickle
import re

import dataclasses
from dataclasses import dataclass

'''Extract the bit position suffix from a field name.'''
SUBNAME_RE = re.compile(r'([^:]+)(_(\d+):(\d+))?$')

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
    read_only: int = True
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

ADDRESSES: list[Address] = []

@dataclass
class Register:
    name: str
    fields: list[Field]
    base_address: int = 0
    byte_span: int = 0
    # shift and width are applied before byte swapping.  FIXME - want after.
    shift: int = 0
    width: int = 0
    def __str__(self) -> str:
        return self.name
    def validate(self) -> None:
        '''Check that the fields pack sensibly.'''
        self.fields.sort(key=lambda f: f.reg_lo)
        assert self.fields[0].reg_lo == 0
        for a, b in zip(self.fields, self.fields[1:]):
            assert a.reg_hi + 1 == b.reg_lo
            assert a.byte_hi == 7
            assert b.byte_lo == 0
        first = self.fields[ 0]
        last  = self.fields[-1]
        # Everything appears to be big endian
        assert first.address >= last.address, self
        self.base_address = last.address
        for a, b in zip(self.fields, self.fields[1:]):
            assert a.address == b.address + 1
        self.byte_span = first.address - last.address + 1
        self.shift = first.byte_lo
        self.width = last.reg_hi + 1
        # All multibyte fields have any partial byte in the high byte.
        if self.byte_span > 1:
            assert self.shift == 0
        assert self.width == sum(f.byte_hi - f.byte_lo + 1 for f in self.fields)

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

    return registers

ADDRESSES = pickle.load(
    open(os.path.dirname(__file__) + '/lmk05318b-registers.pickle', 'rb'))

ADDRESS_BY_NUM = {}

for address in ADDRESSES:
    address.validate()
    ADDRESS_BY_NUM[address.address] = address

REGISTERS = build_registers(ADDRESSES)
