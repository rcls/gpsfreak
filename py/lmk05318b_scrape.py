#!/usr/bin/python3

# To generate the input to this script, pdftotext on the TI PDF of the LMK05318b
# registers.

from freak import lmk05318b, tics
from freak.lmk05318b import Address, Field, Register

import argparse
import pickle
import re
import sys

from typing import Any

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

addresses: dict[int, Address] = {}
address: Address | None = None
# Field currently being processed.
field: Field | None = None

for L in open(args.INPUT):
    if field is not None:
        # Check for a continuation line.
        c = CONT_RE.match(L)
        if c:
            field.name += c.group(1).upper()
            # Recalculate the bit ranges.
            field.__post_init__()
        field = None

    if SECTION_RE.match(L):
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
        assert not rnum_dec in addresses
        addresses[rnum_dec] = address

    f = FIELD_RE.match(L)
    if not f:
        continue

    assert address is not None

    s_byte_hi, s_byte_lo, name, access, s_reset = f.groups()
    byte_hi = int(s_byte_hi)
    if s_byte_lo is None:
        byte_lo = byte_hi
    else:
        byte_lo = int(s_byte_lo.removeprefix(':'))
    assert byte_lo <= byte_hi

    reset = int(s_reset, 0)
    field = Field(name.upper(), byte_hi, byte_lo,
                  access, reset, address.address)
    address.fields.append(field)

# Validate what we read from the .txt file.
for address in addresses.values():
    address.validate()

def extra_field(field: Field) -> None:
    if field.address in addresses:
        address = addresses[field.address]
        address.fields.append(field)
    else:
        address = Address(field.address, [field])
        addresses[field.address] = address

    # Now redo the reserved fields...
    unseen = [True] * 8
    reset = 0
    new_fields: list[Field] = []
    for f in address.fields:
        reset |= f.reset << f.byte_lo
        if f.name != 'RESERVED':
            new_fields.append(f)
            for i in range(f.byte_lo, f.byte_hi + 1):
                unseen[i] = False

    base: int|None = None
    for i, u in enumerate(unseen):
        if not u:
            continue
        if base is None:
            base = i
        if i == 7 or not unseen[i+1]:
            rst = reset >> base & (1 << i - base + 1) - 1
            new_fields.append(Field(
                'RESERVED', i, base, 'R', rst, address.address))
            base = None
    new_fields.sort(key = lambda f: -f.byte_lo)
    address.fields = new_fields
    address.validate()

# Not all are documented..
#
# The DPLL_PL_{LOCK|UNLK}_THRESH: Not sure how many bits these actually are!
# The mapping from value to time appears to depend on the loop B/W and appears
# to be exponential.
# (They appear to be six bits.)

# From the TICS GUI:
extra_field(Field('DPLL_PL_LOCK_THRESH', 5, 0, 'R/W', 0, 301))
extra_field(Field('DPLL_PL_UNLK_THRESH', 5, 0, 'R/W', 0, 302))

extra_field(Field('SYNC_SW', 6, 6, 'R/W', 0, 12))
extra_field(Field('SYNC_MUTE', 3, 3, 'R/W', 0, 12))
extra_field(Field('SYNC_AUTO_APLL', 4, 4, 'R/W', 0, 12))
extra_field(Field('PLL2_ORDER', 2, 0, 'R/W', 0, 139))
extra_field(Field('PLL2_DTHRMODE', 4, 3, 'R/W', 0, 139))
extra_field(Field('PLL2_CLSDWAIT', 2, 3, 'R/W', 0, 105))

# From the datasheet, MEMADDR sounds like a 13 bit register, but that doesn't
# make sense because only 8 bits are needed?  TICS/Pro does actually use it,
# but I've only seen values of zero.
# extra_field(Field('MEMADR_12:8', 4, 0, 'R/W', 0, 159))

# From the datasheet...
extra_field(Field('DPLL_FDEV_REG_UPDATE', 0, 0, 'R/W', 0, 0x160))

# TICS/Pro XO tab.
extra_field(Field('XO_TYPE', 6, 3, 'R/W', 0, 43))

# TICS/Pro reference tab.
extra_field(Field('DPLL_SWITCH_MODE', 1, 0, 'R/W', 0, 251))
extra_field(Field('DPLL_REF_MAN_REG_SEL', 4, 4, 'R/W', 0, 251))
extra_field(Field('DPLL_REF_MAN_SEL', 5, 5, 'R/W', 0, 251))

# TICS/Pro APLL1 tab.
extra_field(Field('PLL1_CP_BAW', 3, 0, 'R/W', 8, 68))
extra_field(Field('PLL1_DTHRMODE', 4, 3, 'R/W', 0, 115))
extra_field(Field('PLL1_ORDER', 2, 0, 'R/W', 3, 115))

# TICS/Pro output tab.
extra_field(Field('OUT0_MUTE_LVL', 1, 0, 'R/W', 1, 23))
extra_field(Field('OUT1_MUTE_LVL', 3, 2, 'R/W', 1, 23))
extra_field(Field('OUT2_MUTE_LVL', 5, 4, 'R/W', 1, 23))
extra_field(Field('OUT3_MUTE_LVL', 7, 6, 'R/W', 1, 23))
extra_field(Field('OUT4_MUTE_LVL', 1, 0, 'R/W', 1, 24))
extra_field(Field('OUT5_MUTE_LVL', 3, 2, 'R/W', 1, 24))
extra_field(Field('OUT6_MUTE_LVL', 5, 4, 'R/W', 1, 24))
extra_field(Field('OUT7_MUTE_LVL', 7, 6, 'R/W', 1, 24))

# FDEV_EN is 0x15a bit 0.  TICS/Pro also uses bit 1.
# INC/DEC by pins: R346 = 1
# INC/DEC by registers: R346 = 3
# INC/DEC by DPLL numerator (absolute): R346 = 2
# I'm not sure what the extra bit acheives?
extra_field(Field('DPLL_FDEV_EXTRA', 1, 1, 'R/W', 0, 0x15a))

# More from TICS/Pro DCO & SYNC tab.
extra_field(Field('DPLL_REF_SYNC_OUT7_EN', 7, 7, 'R/W', 0, 252))
extra_field(Field('DPLL_REF_SYNC_OUT7_NDIV_RST_DIV', 6, 6, 'R/W', 0, 252))
extra_field(Field('DPLL_REF_SYNC_PHASE_OFFSET_39:32', 7, 0, 'R/W', 0, 340))
extra_field(Field('DPLL_REF_SYNC_PHASE_OFFSET_31:24', 7, 0, 'R/W', 0, 341))
extra_field(Field('DPLL_REF_SYNC_PHASE_OFFSET_23:16', 7, 0, 'R/W', 0, 342))
extra_field(Field('DPLL_REF_SYNC_PHASE_OFFSET_15:8', 7, 0, 'R/W', 0, 343))
extra_field(Field('DPLL_REF_SYNC_PHASE_OFFSET', 7, 0, 'R/W', 0, 344))

extra_field(Field('GPIO_FDEV_EN', 7, 7, 'R/W', 0, 50))

# TICS/Pro status tab.
extra_field(Field('GPIO_STAT0_OUT', 0, 0, 'R/W', 0, 36))
extra_field(Field('GPIO_STAT1_OUT', 1, 1, 'R/W', 0, 36))
extra_field(Field('PLL1_VM_INSIDE', 5, 5, 'R', 0, 357))
extra_field(Field('PLL2_VM_INSIDE', 5, 5, 'R', 0, 367))
extra_field(Field('PRIREF_VALSTAT', 2, 2, 'R', 0, 411))
extra_field(Field('SECREF_VALSTAT', 3, 3, 'R', 0, 411))
extra_field(Field('DPLL_REFSEL_STAT', 1, 0, 'R', 0, 167))

# TICS/Pro APLL1 tab.
extra_field(Field('OSCIN_DBLR_EN', 4, 4, 'R/W', 0, 42))
extra_field(Field('OSCIN_RDIV', 4, 0, 'R/W', 0, 44))

# Various undocumented fields are set in the TICS file.  Some are observed to
# change with the configuration, and influence outputs.
if args.tics:
    tf = tics.read_tcs_file(args.tics)
    for a, m in enumerate(tf.mask):
        if m != 0 and not a in addresses:
            val = tf.data[a]
            addresses[a] = \
                Address(a, [Field(f'UNKNOWN{a}', 7, 0, 'R/W', val, a)])

for address in addresses.values():
    address.validate()

address_list = list(addresses.values())
address_list.sort(key = lambda a: a.address)

registers = lmk05318b.build_registers(address_list)

def print_list_file(out: Any, registers: dict[str, Register]) -> None:
    regs = list(registers.values())
    regs.sort(key = lambda r: (r.base_address, -r.shift))
    for r in regs:
        print(f'{r.name:20}: {r.access:3} {r.base_address:3}', file=out, end='')
        if r.shift != 0 or r.width < 8:
            print(f'.{r.shift}', file=out, end='')
        print(f':{r.width}', file=out, end='')
        if r.byte_span != 1:
            print(f' ({r.byte_span})', file=out, end='')
        if r.reset != 0:
            if r.width <= 4:
                print(f' = {r.reset}', file=out, end='')
            else:
                w = (r.width + 3) // 4 + 2
                print(f' = {r.reset:#0{w}x}', file=out, end='')
        print(file=out)

if args.list is not None:
    print_list_file(open(args.list, 'w'), registers)
else:
    print_list_file(sys.stdout, registers)

if args.output is not None:
    pickle.dump(address_list, open(args.output, 'wb'))
