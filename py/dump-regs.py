#!/usr/bin/python3

import fractions
import struct
import sys

import freak
import lmk05318b
import tics

from fractions import Fraction

wanted_bytes = [False] * 500

for a in lmk05318b.ADDRESSES:
    wanted_bytes[a.address] = True

ranges = []
range = None

for n, b in enumerate(wanted_bytes):
    if not b:
        pass
    elif range is None or range[1] != n or n - range[0] >= 50:
        range = [n, n + 1]
        ranges.append(range)
    else:
        range[1] = n + 1

print(ranges)
print(len(ranges))
print(max(r[1] - r[0] for r in ranges))

data = tics.read_tcs_file(sys.argv[1])

for i, m in enumerate(data.mask):
    if m != 0 and not i in lmk05318b.ADDRESS_BY_NUM:
        print(f'R{i} is unwanted value = {data.data[i]:02x}')

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
