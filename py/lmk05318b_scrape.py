#!/usr/bin/python3

from freak import lmk05318b, tics
from freak.lmk05318b import Address, Field, Register

import argparse
import dataclasses
import os
import pickle
import re
import sys

from dataclasses import dataclass

argp = argparse.ArgumentParser(description='LMK05318b scraper')
argp.add_argument('INPUT', help='Text file from pdftotext run')
argp.add_argument('--tics', '-t', help='TICS Pro .tcs file')
argp.add_argument('--output', '-o', help='Output pickle file')
argp.add_argument('--list', '-l', help='Output list file')

args = argp.parse_args()

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

for L in open(args.INPUT):
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

# Not all are documented..
#
# The DPLL_PL_{LOCK|UNLK}_THRESH: Not sure how many bits these actually are!
# The mapping from value to time appears to depend on the loop B/W and appears
# to be exponential.
addresses.append(
    Address(301, [Field('DPLL_PL_LOCK_THRESH', 7, 0, 'R/W', 0, 301)]))
addresses.append(
    Address(302, [Field('DPLL_PL_UNLK_THRESH', 7, 0, 'R/W', 0, 302)]))

# Various undocumented fields are set in the TICS file.  The follow are
# observed to change with the DPLL B/W: 271, 275, 277, 278, 280, 281, 282, 283,
# 296, 297, 298.
if args.tics:
    seen = set(address.address for address in addresses)
    tf = tics.read_tcs_file(args.tics)
    for a, m in enumerate(tf.mask):
        if m != 0 and not a in seen:
            val = tf.data[a]
            addresses.append(
                Address(a, [Field(f'UNKNOWN{a}', 7, 0, 'R', val, a)]))

addresses.sort(key = lambda address: address.address)

for address in addresses:
    address.validate()

registers = lmk05318b.build_registers(addresses)

def print_list_file(out, registers: dict[str, Register]) -> None:
    for r in registers.values():
        print(f'{r.name:20}: {r.access:3} {r.base_address:3}', file=out, end='')
        if r.shift != 0:
            print(f'.{r.shift}', file=out, end='')
        print(f':{r.width}', file=out, end='')
        if r.byte_span != 1:
            print(f' ({r.byte_span})', file=out, end='')
        if r.reset != 0:
            if r.width <= 4:
                print(f' = {r.reset:}', file=out, end='')
            else:
                w = (r.width + 3) // 4 + 2
                print(f' = {r.reset:#0{w}x}', file=out, end='')
        print(file=out)

if args.list is not None:
    print_list_file(open(args.list, 'w'), registers)
else:
    print_list_file(sys.stdout, registers)

if args.output is not None:
    pickle.dump(addresses, open(args.output, 'wb'))
