#!/usr/bin/python3

# Make sure we don't get confused with freak.lmk05318b
assert __name__ == '__main__'

from freak.lmk05318b import MaskedBytes, Register
from freak.message import LMK05318B_READ, LMK05318B_WRITE, transact
import freak.lmk05318b_plan as lmk05318b_plan

import argparse
import struct
import usb

from typing import Tuple

argp = argparse.ArgumentParser(description='LMK05318b utility')
subp = argp.add_subparsers(dest='command', required=True, help='Command')

def key_value(s: str) -> Tuple[str, str]:
    if not '=' in s:
        raise ValueError('Key/value pairs must be in the form KEY=VALUE')
    K, V = s.split('=', 1)
    return K, V

valset = subp.add_parser(
    'set', help='Set registers', description='Set registers')
valset.add_argument('KV', type=key_value, nargs='+',
                    metavar='KEY=VALUE', help='KEY=VALUE pairs')

valget = subp.add_parser(
    'get', help='Get registers', description='Get registers')
valget.add_argument('KEY', nargs='+', help='KEYs')

plan = subp.add_parser(
    'plan', help='Frequency planning', description='Frequency planning')
plan.add_argument('FREQ', nargs='+', help='Frequencies in MHz')

args = argp.parse_args()

def get_ranges(dev, data: MaskedBytes, ranges: list[Tuple[int, int]]) -> None:
    for base, span in ranges:
        segment = transact(dev, LMK05318B_READ,
                           struct.pack('<H', span) + struct.pack('>H', base))
        assert len(segment.payload) == span
        #print(segment)
        data.data[base : base + span] = segment.payload

def do_get(KEYS: list[str]) -> None:
    registers = list(Register.get(key) for key in KEYS)
    dev = usb.core.find(idVendor=0xf055, idProduct=0xd448)
    data = MaskedBytes()
    for r in registers:
        data.insert(r, 0)
    ranges = data.ranges(max_block = 30)
    get_ranges(dev, data, ranges)
    for r in registers:
        print(r, data.extract(r))

def do_set(KV: list[Tuple[str, str]]) -> None:
    registers = list((Register.get(K), int(V, 0)) for K, V in args.KV)
    dev = usb.core.find(idVendor=0xf055, idProduct=0xd448)
    data = MaskedBytes()
    # Build the mask...
    for r, v in registers:
        data.insert(r, v)
    # Get the partial byte values.  TODO - suppress RESERVED 0 fields.
    ranges = data.ranges(
        max_block = 30, select = lambda x: x != 0 and x != 255)
    get_ranges(dev, data, ranges)
    # Reinsert the register values.
    for r, v in registers:
        data.insert(r, v)
    # Now do the set...
    ranges = data.ranges(max_block = 30)
    for base, span in ranges:
        transact(dev, LMK05318B_WRITE,
                 struct.pack('>H', base) + data.data[base : base+span])

def do_plan(freq_str):
    from fractions import Fraction
    pll1, pll2 = lmk05318b_plan.plan([Fraction(f) for f in freq_str])
    print(f'PLL1')
    for i, f in enumerate(pll1.freqs, 1):
        if not f:
            continue
        od = lmk05318b_plan.output_divider(i, Fraction(2500) // f)
        assert od != None
        s1, s2 = od
        if s2 == 1:
            print(f'    Channel {i} freq {f} divider {s1}')
        else:
            print(f'    Channel {i} freq {f} dividers {s1} {s2}')

    if pll2 is None:
        return

    print(f'PLL2: multiplier = /{pll2.fpd_divide} * {pll2.multiplier}, VCO = {float(pll2.freq)}', end='')
    if pll2.freq == pll2.freq_target:
        print()
    else:
        print(f' (target {float(pll2.freq_target)}) error={pll2.error}')

    for i, (f, (pd, s1, s2)) in enumerate(zip(pll2.freqs, pll2.dividers), 1):
        if f:
            if s2 == 1:
                print(f'    Channel {i} freq {f} dividers {pd} {s1}')
            else:
                print(f'    Channel {i} freq {f} dividers {pd} {s1} {s2}')

if args.command == 'get':
    do_get(args.KEY)

if args.command == 'set':
    do_set(args.KV)

if args.command == 'plan':
    do_plan(args.FREQ)
