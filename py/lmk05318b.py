#!/usr/bin/python3

# Make sure we don't get confused with freak.lmk05318b
assert __name__ == '__main__'

from freak import lmk05318b_plan, message, tics
from freak.lmk05318b import MaskedBytes, Register

from freak.message import Device
from freak.lmk05318b_plan import PLLPlan, str_to_freq, freq_to_str

import argparse
import struct

from fractions import Fraction
from typing import Tuple

CHANNELS_RAW = list(enumerate(
    f'Channel {s:3}' for s in '0_1 2_3 4 5 6 7'.split()))
CHANNELS_COOKED = [
    (1, 'Out 1 [2_3]'),
    (0, 'Out 2 [0_1]'),
    (5, 'Out 3 [7]  '),
    (4, 'Out 4 [6]  '),
    (3, 'U.Fl  [5]  '),
    (2, 'Spare [4]  ')]

argp = argparse.ArgumentParser(description='LMK05318b utility')
subp = argp.add_subparsers(
    dest='command', metavar='COMMAND', required=True, help='Command')

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
plan.add_argument('-r', '--raw', action='store_true',
                  help='Use LMK05318b channel numbering')

freq = subp.add_parser(
    'freq', help='Program frequencies', description='Program frequencies')
freq.add_argument('FREQ', nargs='+', help='Frequencies in MHz')
freq.add_argument('-r', '--raw', action='store_true',
                  help='Use LMK05318b channel numbering')

drive = subp.add_parser(
    'drive', help='Set output drive', description='Set output drive')
drive.add_argument('-d', '--defaults', action='store_true',
                   help='Set default values')
drive.add_argument('DRIVE', type=key_value, nargs='*',
                   metavar='CH=DRIVE', help='Channel and drive type / strength')

upload = subp.add_parser(
    'upload', help='Upload TICS Pro .tcs file',
    description='Upload TICS Pro .tcs file')
upload.add_argument('FILE', help='Name of .tics file')

DRIVES = {
    'off'  : (0, 0, 0, 'Off'),
    'lvds' : (1, 0, 0, 'LVDS, 4mA'),
    'lvds4': (1, 0, 0, 'LVDS, 4mA'),
    'lvds6': (1, 1, 0, 'LVDS, 6mA'),
    'lvds8': (1, 2, 0, 'LVDS, 8mA'),
}
CMOS_DRIVES = ('z', 'hi-z'), ('0', 'low'), ('-', 'inverted'), ('+', 'normal')
for v1, (l1, d1) in enumerate(CMOS_DRIVES):
    for v2, (l2, d2) in enumerate(CMOS_DRIVES):
        DRIVES['cmos' + l1 + l2] = (3, v1, v2, f'CMOS, {d1}, {d2}')

args = argp.parse_args()

def get_ranges(dev: Device, data: MaskedBytes,
               ranges: list[Tuple[int, int]]) -> None:
    for base, span in ranges:
        segment = message.lmk05318b_read(dev, base, span)
        assert len(segment) == span
        #print(segment)
        data.data[base : base + span] = segment

def do_get(KEYS: list[str]) -> None:
    registers = list(Register.get(key) for key in KEYS)
    dev = message.get_device()
    data = MaskedBytes()
    for r in registers:
        data.insert(r, 0)
    ranges = data.ranges(max_block = 30)
    get_ranges(dev, data, ranges)
    for r in registers:
        print(r, data.extract(r))

def complete_partials(dev: Device, data: MaskedBytes) -> None:
    '''Where the data has only part of a byte, fill in the rest.'''
    # TODO - suppress RESERVED 0 fields, or use a 'pristine' source
    # for them?
    ranges = data.ranges(
        max_block = 30, select = lambda x: x != 0 and x != 255)
    gaps = MaskedBytes()
    get_ranges(dev, gaps, ranges)
    for start, length in ranges:
        for i in range(start, start + length):
            data.data[i] = (data.data[i] & data.mask[i]) \
                | (gaps.data[i] & ~data.mask[i])
            data.mask[i] = 255

def masked_write(dev: Device, data: MaskedBytes) -> None:
    complete_partials(dev, data)
    ranges = data.ranges(max_block = 30)
    for base, span in ranges:
        #print(base, span, data.data[base : base+span].hex(' '))
        message.lmk05318b_write(dev, base, data.data[base : base+span])

def do_set(KV: list[Tuple[str, str]]) -> None:
    registers = list((Register.get(K), int(V, 0)) for K, V in args.KV)
    dev = message.get_device()
    data = MaskedBytes()
    # Build the mask...
    for r, v in registers:
        data.insert(r, v)
    masked_write(dev, data)

def report_plan(plan: PLLPlan) -> None:
    if plan.freq_target == 0:
        print('PLL2 not used')
    else:
        print(f'PLL2: multiplier = /{plan.fpd_divide} * {plan.multiplier}, VCO2 {freq_to_str(plan.freq)}', end='')
        if plan.freq == plan.freq_target:
            print()
        else:
            print(f' (target {float(plan.freq_target)} MHz) error={plan.error()}')

    channels = CHANNELS_RAW if args.raw else CHANNELS_COOKED
    for index, name in channels:
        f = plan.freqs[index]
        pd, s1, s2 = plan.dividers[index]
        if not f:
            continue
        pll = 1 if pd == 0 else 2
        print(f'    {name} PLL{pll} {freq_to_str(f)} dividers', end='')
        if pll == 2:
            print(f' {pd}', end='')
        print(f' {s1}', end='')
        if s2 == 1:
            print()
        else:
            print(f' {s2}')

def make_freq_list(freqs: list[str]) -> list[Fraction]:
    channels = CHANNELS_RAW if args.raw else CHANNELS_COOKED
    result = [Fraction(0)] * 6
    for (i, _), f in zip(channels, freqs):
        result[i] = str_to_freq(f)
    return result

def freq_make_data(plan: PLLPlan) -> dict[str, int]:
    data = { }
    postdiv1 = 0
    postdiv2 = 0
    for pd, _, _ in plan.dividers:
        if pd == 0:
            continue
        if postdiv1 == 0:
            postdiv1 = pd
            postdiv2 = pd
        elif pd != postdiv1 and postdiv2 == postdiv1:
            postdiv2 = pd
        else:
            assert pd == postdiv1 or pd == postdiv2
    if postdiv1 == 0:
        postdiv1 = 2
    if postdiv2 == 0:
        postdiv2 = 2
    data['PLL2_P1'] = postdiv1 - 1
    data['PLL2_P2'] = postdiv2 - 1
    chtag = '0_1', '2_3', '4', '5', '6', '7'
    for i, (pd, s1, s2) in enumerate(plan.dividers):
        t = chtag[i]
        if s1 == 0:                     # Disabled.
            data[f'CH{t}_PD'] = 1
            continue
        data[f'CH{t}_PD'] = 0
        # Source.
        if pd == 0:
            data[f'CH{t}_MUX'] = 0
        elif pd == postdiv1:
            data[f'CH{t}_MUX'] = 2
        elif pd == postdiv2:
            data[f'CH{t}_MUX'] = 3
        else:
            assert 'This should never happen' == None
        assert 1 <= s1 <= 256
        data[f'OUT{t}_DIV'] = s1 - 1
        if i == 5:
            assert 1 <= s2 <= 1<<24
            data[f'OUT7_STG2_DIV'] = s2 - 1
        else:
            assert s2 == 1

    # Power down PLL2 for now, whether or not we need it.  We'll power it up
    # after configuring it.
    data['PLL2_PDN'] = 1
    if plan.freq_target == 0:
        return data

    # PLL2 setup...
    pll2_den = plan.multiplier.denominator
    pll2_int = plan.multiplier.numerator // pll2_den
    pll2_num = plan.multiplier.numerator % pll2_den
    if plan.fixed_denom():
        data['APLL2_DEN_MODE'] = 0
        assert (1<<24) % pll2_den == 0
        pll2_num = pll2_num * (1<<24) // pll2_den
        pll2_den = 0
    else:
        data['APLL2_DEN_MODE'] = 1
    data['PLL2_NDIV'] = pll2_int
    data['PLL2_NUM']  = pll2_num
    data['PLL2_DEN']  = pll2_den
    # Canned values...
    data['PLL2_RCLK_SEL'] = 0
    data['PLL2_RDIV_PRE'] = 0
    data['PLL2_RDIV_SEC'] = 5
    data['PLL2_DISABLE_3RD4TH'] = 15
    data['PLL2_CP'] = 1
    data['PLL2_LF_R2'] = 2
    data['PLL2_LF_C1'] = 0
    data['PLL2_LF_R3'] = 1
    data['PLL2_LF_R4'] = 1
    data['PLL2_LF_C4'] = 7
    data['PLL2_LF_C3'] = 7
    return data

def do_freq(freq_str: list[str]) -> None:
    plan = lmk05318b_plan.plan(make_freq_list(freq_str))
    report_plan(plan)
    data = MaskedBytes()
    for K, V in freq_make_data(plan).items():
        data.insert(Register.get(K), V)
    dev = message.get_device()
    # Software reset.
    message.lmk05318b_write(dev, 12, 12)
    # Write the registers.
    masked_write(dev, data)
    # If PLL2 is in use, power it up now.
    if plan.freq_target != 0:
        message.lmk05318b_write(dev, 100, data.data[100] & 0xfe)
    # Remove software reset.
    message.lmk05318b_write(dev, 12, 2)

def do_drive(drives: list[Tuple[str, str]], defaults: bool) -> None:
    data = MaskedBytes()
    if defaults:
        drives = [('0', 'lvds8'), ('1', 'off'), ('2', 'lvds8'), ('3', 'off'),
                  ('4', 'off'), ('5', 'off'), ('6', 'cmos+z'), ('7', 'lvds8')] \
                  + drives
    for ch, drive in drives:
        assert len(ch) == 1 and ch >= '0' and ch < '8'
        channels = [ch]
        if drive.startswith('2'):
            assert ch == '0' or ch == '2'
            channels.append('1' if ch == '0' else '3')
            drive = drive[1:]
        if drive.startswith('cmos'):
            assert ch >= '4'
        assert drive in DRIVES
        sel, mode1, mode2, _ = DRIVES[drive]
        for c in channels:
            data.insert(Register.get(f'OUT{c}_SEL'), sel)
            data.insert(Register.get(f'OUT{c}_MODE1'), mode1)
            data.insert(Register.get(f'OUT{c}_MODE2'), mode2)
    dev = message.get_device()
    masked_write(dev, data)

def report_drive() -> None:
    dev = message.get_device()
    data = MaskedBytes()
    base, length = 50, 24
    drives_data = message.lmk05318b_read(dev, base, length)
    assert len(drives_data) == length
    data.data[base : base + length] = drives_data

    drives = []

    pdowns = '0_1 0_1 2_3 2_3 4 5 6 7'.split()
    for i in range(8):
        pdown = data.extract(f'CH{pdowns[i]}_PD')
        sel   = data.extract(f'OUT{i}_SEL')
        mode1 = data.extract(f'OUT{i}_MODE1')
        mode2 = data.extract(f'OUT{i}_MODE2')
        drives.append((pdown, sel, mode1, mode2))

    for i, (pdown, sel, mode1, mode2) in enumerate(drives):
        print(f'Channel {i}: ', end='')
        if pdown:
            print('Power down, ', end='')
        for tag, (s, m1, m2, name) in DRIVES.items():
            if sel == s and mode1 == m1 and mode2 == m2:
                print(name, f'[{tag}]')
                break
        else:
            print(f'sel={sel} mode1={mode1} mode2={mode2}')

def do_upload(path: str) -> None:
    dev = message.get_device()
    tcs = tics.read_tcs_file(path)
    masked_write(dev, tcs)

if args.command == 'get':
    do_get(args.KEY)

elif args.command == 'set':
    do_set(args.KV)

elif args.command == 'plan':
    report_plan(lmk05318b_plan.plan(make_freq_list(args.FREQ)))

elif args.command == 'freq':
    do_freq(args.FREQ)

elif args.command == 'drive':
    if args.DRIVE or args.defaults:
        do_drive(args.DRIVE, args.defaults)
    else:
        report_drive()

elif args.command == 'upload':
    do_upload(args.FILE)

else:
    assert None, 'This should never happen'
