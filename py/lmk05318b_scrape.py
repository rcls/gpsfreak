#!/usr/bin/python3

import dataclasses
import lmk05318b
import os
import pickle
import re

from dataclasses import dataclass
from lmk05318b import Address, Field

'''PATH of the pdftotext output.'''
PATH = os.path.dirname(__file__) + '/lmk05318b-registers.txt'

'''RE to match start of a section'''
SECTION_RE = re.compile(r'\d+\.\d+')

'''RE to match the start of a register description section'''
REGSECT_RE = re.compile(r'\d+\.\d+ R(\d+) +\(Offset = (0x[0-9a-fA-F]+)\)')

#HEADER_RE = re.compile(r'\s*Bit\s+Field\s+Type\s+Reset\s+Description\s+$')

'''RE to match the (first line of a) field description.'''
FIELD_RE = re.compile(
    r'\s+(\d+)(:\d+)?\s+([\w:]+)\s+([/\w]+)\s+(0x[0-9a-fA-F]+)\s.*')

'''RE to match a continuation line of a field description, where the field name
is split over multiple lines.'''
CONT_RE = re.compile(r'\s{12,28}([\w:]+)\b')

addresses = []
address = None
# Field currently being processed.
field = None

def eject_field():
    global field, address
    if field is None:
        return
    assert address is not None
    # Reconstitute the field
    address.fields.append(
        Field(field.name, field.byte_hi, field.byte_lo, field.access,
              field.reset, field.address))
    field = None

for L in open(PATH):
    if field is not None:
        # Check for a continuation line.
        c = CONT_RE.match(L)
        if c:
            #print('C', name, c.group(1))
            field.name += c.group(1)

    eject_field()
    if SECTION_RE.match(L):
        #if address is not None:
            #print(address)
        address = None

    if L.startswith('SNAU254C') or L.startswith('Submit Doc') \
       or L.startswith('\f') or L.strip() == '':
        continue

    rs = REGSECT_RE.match(L)
    if rs:
        rnum_dec = int(rs.group(1))
        rnum_hex = int(rs.group(2), 16)
        assert rnum_dec == rnum_hex
        address = Address(rnum_dec, [])
        addresses.append(address)

    f = FIELD_RE.match(L)
    if not f:
        continue
    assert address
    #print(f.groups())
    #address.fields.append(
    s_byte_hi, s_byte_lo, name, access, s_reset = f.groups()
    byte_hi = int(s_byte_hi)
    if s_byte_lo is None:
        byte_lo = byte_hi
    else:
        byte_lo = int(s_byte_lo.removeprefix(':'))
    assert byte_lo <= byte_hi
    #if name in fixups:
    #    name = fixups[name]
    reset = int(s_reset, 0)
    field = Field(name, byte_hi, byte_lo, access, reset, address.address)

eject_field()

for address in addresses:
    address.validate()

registers = lmk05318b.build_registers(addresses)

dump_path = os.path.dirname(__file__) + '/lmk05318b-registers.pickle'
pickle.dump(addresses, open(dump_path, 'wb'))
