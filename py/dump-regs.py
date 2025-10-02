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
from typing import Tuple

argp = argparse.ArgumentParser(description='LMK05318b register dump')
argp.add_argument('--usb', '-u', action='store_true',
                  help='Read USB attached device')
argp.add_argument('--tics', '-t', metavar='FILE', help='Read TICS .tcs file')

args = argp.parse_args()

assert 'usb' in args or 'file' in args

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

if 'file' in args:
    data = tics.read_tcs_file(sys.argv[1])
    for i, m in enumerate(data.mask):
        if m != 0 and not i in lmk05318b.ADDRESS_BY_NUM:
            print(f'R{i} is unwanted value = {data.data[i]:02x}')
else:
    dev = usb.core.find(idVendor=0xf055, idProduct=0xd448)
    freak.flush(dev)
    data = tics.MaskedBytes()
    for address, length in lmk05318b_bundles():
        transact(dev, freak.LMK05318B_WRITE, struct.pack('<H', address))
        segment = transact(dev, freak.LMK05318B_READ, struct.pack('<H', length))
        for a, b in enumerate(segment.payload, address):
            data.data[a] = b
            data.mask[a] = 255

config = {}

for r in lmk05318b.REGISTERS.values():
    base = r.base_address
    bb = data.data[base : base + r.byte_span]
    match r.byte_span:
        case 1:
            value = bb[0]
        case 2:
            value = struct.unpack('>H', bb)[0]
        case 3:
            value = struct.unpack('>I', b'0' + bb)[0]
        case 4:
            value = struct.unpack('>I', bb)[0]
        case 5:
            value = struct.unpack('>Q', b'000' + bb)[0]
        case _:
            assert False, f'span = {r.byte_span}'
    value = value >> r.shift
    value &= (1 << r.width) - 1
    print(r.name, value, f'{value:#x}')
    config[r.name] = value

DPLL_PRIREF_RDIV = config['DPLL_PRIREF_RDIV']
DPLL_REF_FB_DIV  = config['DPLL_REF_FB_DIV']
DPLL_REF_NUM     = config['DPLL_REF_NUM']
DPLL_REF_DEN     = config['DPLL_REF_DEN']
DPLL_REF_FB_PRE_DIV = config['DPLL_REF_FB_PRE_DIV']

ref = 6553600
#ref = 6400000

if DPLL_REF_DEN == 0:
    DPLL_REF_DEN = 1 << 40

print(Fraction(ref) / DPLL_PRIREF_RDIV
      * (DPLL_REF_FB_PRE_DIV + 2) * 2
      * (DPLL_REF_FB_DIV + Fraction(DPLL_REF_NUM)/DPLL_REF_DEN))

print(repr(lmk05318b.ADDRESS_BY_NUM[12]))
