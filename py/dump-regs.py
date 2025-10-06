#!/usr/bin/python3

import argparse
import fractions
import struct
import sys
import usb

import freak
import lmk05318b
import tics

from fractions import Fraction
from freak import transact
from lmk05318b import Address, MaskedBytes
from typing import Tuple

argp = argparse.ArgumentParser(
    description='''
LMK05318b register dump.  This tool can either read a TICS file, or else
extract the registers via USB from a GPS Freak device.''')

argp.add_argument('-u', '--usb', action='store_true',
                  help='Read USB attached device.')
argp.add_argument('-t', '--tics', metavar='FILE',
                  help='Read TICS Pro .tcs file.')
argp.add_argument('-a', '--all', action='store_true',
                  help='Also report values unchanged from datasheet defaults.')

args = argp.parse_args()

assert args.usb or args.tics is not None

def lmk05318b_bundles() -> list[Tuple[int, int]]:
    bundles = []
    address = -100;
    length = 0;
    for a in lmk05318b.ADDRESSES:
        if a.address == address + length and length < 30:
            length += 1
            continue
        if address >= 0:
            bundles.append((address, length))
        address = a.address
        length = 1
    if address >= 0:
        bundles.append((address, length))
    return bundles

def usb_load() -> MaskedBytes:
    dev = usb.core.find(idVendor=0xf055, idProduct=0xd448)
    freak.flush(dev)
    data = MaskedBytes()
    for address, length in lmk05318b_bundles():
        transact(dev, freak.LMK05318B_WRITE, struct.pack('>H', address))
        segment = transact(dev, freak.LMK05318B_READ, struct.pack('<H', length))
        for a, b in enumerate(segment.payload, address):
            data.data[a] = b
            data.mask[a] = 255
    return data

if args.tics is not None:
    data = tics.read_tcs_file(args.tics)
else:
    data = usb_load()

config = {}

# Report the register values.
for r in lmk05318b.REGISTERS.values():
    value = r.extract(data.data)
    if args.all or value != r.reset:
        print(r.name, value, f'{value:#x}')
    config[r.name] = value

# Report on reserved values & RO.
def report_changed_ro(data: MaskedBytes, a: Address):
    if data.mask[a.address] == 0:
        print(f'Address {a.address} is not set')
        return
    res_mask = 0
    res_data = 0
    ro_mask = 0
    ro_data = 0
    for f in a.fields:
        if f.name == 'RESERVED':
            res_mask |= f.mask()
            res_data |= f.reset << f.byte_lo
        elif not 'W' in f.access:
            ro_mask |= f.mask()
            ro_data |= f.reset << f.byte_lo
    data_res = data.data[a.address] & res_mask
    if data_res != res_data:
        print(f'Reserved @ {a.address} {data_res:#04x} not {res_data:#04x}')
    data_ro = data.data[a.address] & ro_mask
    if data_ro != ro_data:
        print(f'R/O @ {a.address} {data_ro:#04x} not {ro_data:#04x}')

for a in lmk05318b.ADDRESSES:
    report_changed_ro(data, a)

for i, mask in enumerate(data.mask):
    if mask != 0 and not i in lmk05318b.ADDRESS_BY_NUM:
        val = data.data[i]
        print(f'Unknown @ {i} = {val:#04x}')

# DPLL report.
DPLL_PRIREF_RDIV = config['DPLL_PRIREF_RDIV']
DPLL_REF_FB_DIV  = config['DPLL_REF_FB_DIV']
DPLL_REF_NUM     = config['DPLL_REF_NUM']
DPLL_REF_DEN     = config['DPLL_REF_DEN']
DPLL_REF_FB_PRE_DIV = config['DPLL_REF_FB_PRE_DIV']

ref = 8844582

if DPLL_REF_DEN == 0:
    DPLL_REF_DEN = 1 << 40

print(Fraction(ref) / DPLL_PRIREF_RDIV
      * (DPLL_REF_FB_PRE_DIV + 2) * 2
      * (DPLL_REF_FB_DIV + Fraction(DPLL_REF_NUM)/DPLL_REF_DEN))
