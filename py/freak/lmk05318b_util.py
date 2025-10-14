#!/usr/bin/python3

from freak import lmk05318b, lmk05318b_plan, message, message_util, tics
from .lmk05318b import MaskedBytes, Register

from .freak_util import Device
from .lmk05318b_plan import PLLPlan, SCALE, str_to_freq, freq_to_str

import argparse
import struct

from fractions import Fraction
from typing import Tuple

CHANNELS_RAW = list(
    (i, f'Channel {s:3}', s) for i, s in enumerate('0_1 2_3 4 5 6 7'.split()))
CHANNELS_COOKED = [
    (1, 'Out 1 [2_3]', '2_3'),
    (0, 'Out 2 [0_1]', '0_1'),
    (5, 'Out 3 [7]  ', '7'),
    (4, 'Out 4 [6]  ', '6'),
    (3, 'U.Fl  [5]  ', '5'),
    (2, 'Spare [4]  ', '4')]

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

def get_ranges(dev: Device, data: MaskedBytes,
               ranges: list[Tuple[int, int]]) -> None:
    for base, span in ranges:
        segment = message.lmk05318b_read(dev.get_usb(), base, span)
        assert len(segment) == span
        #print(segment)
        data.data[base : base + span] = segment

def do_get(dev: Device, KEYS: list[str]) -> None:
    registers = list(Register.get(key) for key in KEYS)
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
    udev = dev.get_usb()
    for base, span in ranges:
        #print(base, span, data.data[base : base+span].hex(' '))
        message.lmk05318b_write(udev, base, data.data[base : base+span])

def do_set(dev: Device, KV: list[Tuple[str, str]]) -> None:
    registers = list((Register.get(K), int(V, 0)) for K, V in KV)
    data = MaskedBytes()
    # Build the mask...
    for r, v in registers:
        data.insert(r, v)
    masked_write(dev, data)

def report_plan(plan: PLLPlan, raw: bool) -> None:
    if plan.freq_target == 0:
        print('PLL2 not used')
    else:
        print(f'PLL2: VCO {freq_to_str(plan.freq)}, multiplier = /{plan.fpd_divide} * {plan.multiplier}', end='')
        if plan.freq == plan.freq_target:
            print()
        else:
            print(f' (target {float(plan.freq_target)} MHz) error={plan.error()}')

    channels = CHANNELS_RAW if raw else CHANNELS_COOKED
    for index, name, _ in channels:
        f = plan.freqs[index]
        pd, s1, s2 = plan.dividers[index]
        if not f:
            continue
        pll = 1 if pd == 0 else 2
        print(f'    {name} {freq_to_str(f)} PLL{pll} dividers', end='')
        if pll == 2:
            print(f' {pd}', end='')
        print(f' {s1}', end='')
        if s2 == 1:
            print()
        else:
            print(f' {s2}')

def make_freq_list(freqs: list[str], raw: bool) -> list[Fraction]:
    channels = CHANNELS_RAW if raw else CHANNELS_COOKED
    result = [Fraction(0)] * max(6, len(freqs))
    for (i, _, _), f in zip(channels, freqs):
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

def do_freq(dev: Device, freq_str: list[str], raw: bool) -> None:
    plan = lmk05318b_plan.plan(make_freq_list(freq_str, raw))
    report_plan(plan, raw)
    data = MaskedBytes()
    for K, V in freq_make_data(plan).items():
        data.insert(Register.get(K), V)
    # Software reset.
    message.lmk05318b_write(dev, 12, 12)
    # Write the registers.
    masked_write(dev, data)
    # If PLL2 is in use, power it up now.
    if plan.freq_target != 0:
        message.lmk05318b_write(dev.get_usb(), 100, data.data[100] & 0xfe)
    # Remove software reset.
    message.lmk05318b_write(dev.get_usb(), 12, 2)

def do_drive(dev: Device, drives: list[Tuple[str, str]],
             defaults: bool) -> None:
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

    masked_write(dev, data)

def report_drive(dev: Device) -> None:
    data = MaskedBytes()
    base, length = 50, 24
    drives_data = message.lmk05318b_read(dev.get_usb(), base, length)
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

def report_freq(dev: Device, raw: bool) -> None:
    data = MaskedBytes()
    # For now, pull everything...
    for a in lmk05318b.ADDRESSES:
        data.mask[a.address] = 0xff
    ranges = data.ranges(max_block = 30)
    get_ranges(dev, data, ranges)
    # FIXME - retrieve it!
    reference = Fraction(8844582, SCALE)
    dpll_priref_rdiv    = data.extract('DPLL_PRIREF_RDIV')
    dpll_ref_fb_pre_div = data.extract('DPLL_REF_FB_PRE_DIV') + 2
    dpll_ref_fb_div     = data.extract('DPLL_REF_FB_DIV')
    dpll_ref_num        = data.extract('DPLL_REF_NUM')
    dpll_ref_den        = data.extract('DPLL_REF_DEN')

    pll2_rdiv_pre       = data.extract('PLL2_RDIV_PRE') + 3
    pll2_rdiv_sec       = data.extract('PLL2_RDIV_SEC') + 1
    pll2_ndiv           = data.extract('PLL2_NDIV')
    pll2_num            = data.extract('PLL2_NUM')
    apll2_den_mode      = data.extract('APLL2_DEN_MODE')
    if apll2_den_mode == 0:
        pll2_den = 1 << 24
    else:
        pll2_den        = data.extract('PLL2_DEN')
    pll2_p1             = data.extract('PLL2_P1') + 1
    pll2_p2             = data.extract('PLL2_P2') + 1

    assert dpll_priref_rdiv != 0
    baw_freq = Fraction(reference) / dpll_priref_rdiv \
        * 2 * dpll_ref_fb_pre_div * (
            dpll_ref_fb_div + Fraction(dpll_ref_num, dpll_ref_den))
    print(f'BAW frequency = {freq_to_str(baw_freq)}')

    pll2_freq = baw_freq / pll2_rdiv_pre / pll2_rdiv_sec * (
        pll2_ndiv + Fraction(pll2_num, pll2_den))
    print(f'PLL2 frequency = {freq_to_str(pll2_freq)}')

    for _, name, ch in CHANNELS_RAW if raw else CHANNELS_COOKED:
        mux = data.extract(f'CH{ch}_MUX')
        div = data.extract(f'OUT{ch}_DIV') + 1
        s2div = 1
        if ch == '7':
            s2div = data.extract('OUT7_STG2_DIV') + 1
        muxed = baw_freq
        if mux == 2:
            muxed = pll2_freq / pll2_p1
        elif mux == 3:
            muxed = pll2_freq / pll2_p2
        ch_freq = muxed / div / s2div
        if data.extract(f'CH{ch}_PD'):
            pd = ' (power down)'
        else:
            pd = ''
        print(f'{name} frequency = {freq_to_str(ch_freq)}{pd}')

def do_upload(dev: Device, path: str) -> None:
    tcs = tics.read_tcs_file(path)
    masked_write(dev, tcs)

def add_to_argparse(argp: argparse.ArgumentParser,
                    dest: str = 'command', metavar: str = 'COMMAND') -> None:
    def key_value(s: str) -> Tuple[str, str]:
        if not '=' in s:
            raise ValueError('Key/value pairs must be in the form KEY=VALUE')
        K, V = s.split('=', 1)
        return K, V

    subp = argp.add_subparsers(
        dest=dest, metavar=metavar, required=True, help='Sub-command')

    plan = subp.add_parser(
        'plan', help='Frequency planning',
        description='''Compute and print a frequency plan without programming it
        to the device.''')
    plan.add_argument('FREQ', nargs='+', help='Frequencies in MHz')

    freq = subp.add_parser(
        'freq', aliases=['frequency'], help='Program/report frequencies',
        description='''Program or frequencies If a list of frequencies is given,
        these are programmed to the device.  With no arguments, report the
        current device frequencies.''')
    freq.add_argument('FREQ', nargs='*', help='Frequencies in MHz')

    drive = subp.add_parser('drive', help='Set/get output drive',
                            description='Set/get output drive')
    drive.add_argument('-d', '--defaults', action='store_true',
                       help='Set default values')
    drive.add_argument('DRIVE', type=key_value, nargs='*', metavar='CH=DRIVE',
                       help='Channel and drive type / strength')

    message_util.add_reset_command(subp, 'LMK05318b')

    upload = subp.add_parser(
        'upload', help='Upload TICS Pro .tcs file',
        description='Upload TICS Pro .tcs file')
    upload.add_argument('FILE', help='Name of .tics file')

    valset = subp.add_parser(
        'set', help='Set registers', description='Set registers')
    valset.add_argument('KV', type=key_value, nargs='+',
                        metavar='KEY=VALUE', help='KEY=VALUE pairs')

    valget = subp.add_parser(
        'get', help='Get registers', description='Get registers')
    valget.add_argument('KEY', nargs='+', help='KEYs')

def run_command(args: argparse.Namespace, device: Device, command: str) -> None:
    if command == 'get':
        do_get(device, args.KEY)

    elif command == 'set':
        do_set(device, args.KV)

    elif command == 'plan':
        plan = lmk05318b_plan.plan(make_freq_list(args.FREQ, True))
        report_plan(plan, True)

    elif command == 'freq':
        if len(args.FREQ) != 0:
            do_freq(device, args.FREQ, True)
        else:
            report_freq(device, True)

    elif command == 'drive':
        if args.DRIVE or args.defaults:
            do_drive(device, args.DRIVE, bool(args.defaults))
        else:
            report_drive(device)

    elif command == 'reset':
        message_util.do_reset_line(device, message.LMK05318B_PDN, args)

    elif command == 'upload':
        do_upload(device, args.FILE)

    else:
        print(args)
        assert None, f'This should never happen: {command}'

if __name__ == '__main__':
    argp = argparse.ArgumentParser(description='LMK05318b utility')
    add_to_argparse(argp)

    args = argp.parse_args()
    run_command(args, Device(args), args.command)
