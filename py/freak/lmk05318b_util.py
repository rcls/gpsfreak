#!/usr/bin/python3

from freak import config, lmk05318b, lmk05318b_plan, message, message_util, tics

from .freak_util import Device
from .lmk05318b import MaskedBytes, Register
from .plan_constants import FPD_DIVIDE, REF_FREQ, Hz
from .plan_pll2 import PLLPlan
from .plan_tools import FrequencyTarget, \
    str_to_freq, freq_to_str, fraction_to_str

import argparse
import struct

from fractions import Fraction
from typing import Any, Tuple

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
    'lvds' : (1, 0, 0, 'LVDS 4mA'),
    'lvds4': (1, 0, 0, 'LVDS 4mA'),
    'lvds6': (1, 1, 0, 'LVDS 6mA'),
    'lvds8': (1, 2, 0, 'LVDS 8mA'),
}
CMOS_DRIVES = ('z', 'hi-Z'), ('0', 'low'), ('-', 'inverted'), ('+', 'normal')
for v1, (l1, d1) in enumerate(CMOS_DRIVES):
    for v2, (l2, d2) in enumerate(CMOS_DRIVES):
        DRIVES['cmos' + l1 + l2] = (3, v1, v2, f'CMOS, {d1}+{d2}')
DRIVES_BY_SEL = {
    (s, d, e): (tag, name) for tag, (s, d, e, name) in DRIVES.items()}

def get_ranges(dev: Device, data: MaskedBytes,
               ranges: list[Tuple[int, int]]) -> None:
    for base, span in ranges:
        data.data[base : base + span] = \
            message.lmk05318b_read(dev.get_usb(), base, span)

def do_get(dev: Device, registers: list[Register]) -> None:
    data = MaskedBytes()
    for r in registers:
        data.insert(r, 0)
    ranges = data.ranges(max_block = 32)
    get_ranges(dev, data, ranges)
    for r in registers:
        print(r, '=', data.extract(r), sep='')

def do_dump(dev: Device) -> None:
    # Build the list of registers, skipping reserved/unknown registers,
    # and a couple that have read side-effects.
    registers = []
    for r in lmk05318b.REGISTERS.values():
        if r.name.startswith('UNKNOWN') or r.name.startswith('RESERVED') \
           or r.base_address in (161, 162):
            continue
        registers.append(r)
    do_get(dev, registers)

def complete_partials(dev: Device, data: MaskedBytes) -> None:
    '''Where the data has only part of a byte, fill in the rest.'''
    # TODO - suppress RESERVED 0 fields, or use a 'pristine' source
    # for them?
    ranges = data.ranges(
        max_block = 32, select = lambda x: x != 0 and x != 255)
    gaps = MaskedBytes()
    get_ranges(dev, gaps, ranges)
    for start, length in ranges:
        for i in range(start, start + length):
            data.data[i] = (data.data[i] & data.mask[i]) \
                | (gaps.data[i] & ~data.mask[i])
            data.mask[i] = 255

def masked_write(dev: Device, data: MaskedBytes) -> None:
    for i in 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13 ,14:
        data.mask[i] = 0;
    complete_partials(dev, data)
    ranges = data.ranges(max_block = 32)
    udev = dev.get_usb()
    for base, span in ranges:
        #print(base, span, ':', data.data[base : base+span].hex(' '))
        message.lmk05318b_write(udev, base, data.data[base : base+span])

def do_set(dev: Device, key_values: list[Tuple[Register, int]]) -> None:
    data = MaskedBytes()
    # Build the mask...
    for r, v in key_values:
        data.insert(r, v)
    masked_write(dev, data)

def make_freq_target(args: argparse.Namespace, raw: bool) -> FrequencyTarget:
    freqs = args.FREQ
    channels = CHANNELS_RAW if raw else CHANNELS_COOKED
    result = [Fraction(0)] * max(6, len(args.FREQ))
    # FIXME - this just silently ignores extras.
    for (i, _, _), f in zip(channels, freqs):
        result[i] = f
    return FrequencyTarget(freqs=result, pll2_base=args.pll2)

def freq_make_data(plan: PLLPlan) -> MaskedBytes:
    data = MaskedBytes()
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
    data.PLL2_P1 = postdiv1 - 1
    data.PLL2_P2 = postdiv2 - 1
    chtag = '0_1', '2_3', '4', '5', '6', '7'
    for i, (pd, s1, s2) in enumerate(plan.dividers):
        t = chtag[i]
        if s1 == 0:                     # Disabled.
            data.insert(f'CH{t}_PD', 1)
            continue
        data.insert(f'CH{t}_PD', 0)
        # Source.
        if pd == 0:
            data.insert(f'CH{t}_MUX', 0)
        elif pd == postdiv1:
            data.insert(f'CH{t}_MUX', 2)
        elif pd == postdiv2:
            data.insert(f'CH{t}_MUX', 3)
        else:
            assert 'This should never happen' == None
        assert 1 <= s1 <= 256
        data.insert(f'OUT{t}_DIV', s1 - 1)
        if i == 5:
            assert 1 <= s2 <= 1<<24
            data.OUT7_STG2_DIV = s2 - 1
        else:
            assert s2 == 1

    data.DPLL_PRIREF_RDIV = 1
    data.DPLL_REF_FB_PRE_DIV = plan.dpll.fb_prediv - 2
    div = plan.dpll.fb_div.numerator // plan.dpll.fb_div.denominator
    num = plan.dpll.fb_div.numerator % plan.dpll.fb_div.denominator
    den = plan.dpll.fb_div.denominator
    mult = ((1 << 40) - 1) // den
    data.DPLL_REF_FB_DIV = div
    data.DPLL_REF_NUM = num * mult
    data.DPLL_REF_DEN = den * mult

    if plan.pll2_target == 0:
        data.LOL_PLL2_MASK = 1
        data.MUTE_APLL2_LOCK = 0
        data.PLL2_PDN = 1
        return data

    # PLL2 setup...
    data.PLL2_PDN  = 0
    data.LOL_PLL2_MASK = 0
    data.MUTE_APLL2_LOCK = 1
    pll2_den = plan.multiplier.denominator
    pll2_int = plan.multiplier.numerator // pll2_den
    pll2_num = plan.multiplier.numerator % pll2_den
    if plan.fixed_denom():
        data.APLL2_DEN_MODE = 0
        assert (1<<24) % pll2_den == 0
        pll2_num = pll2_num * (1<<24) // pll2_den
        pll2_den = 0
    else:
        data.APLL2_DEN_MODE = 1
    data.PLL2_NDIV = pll2_int
    data.PLL2_NUM  = pll2_num
    data.PLL2_DEN  = pll2_den
    # Canned values... (Should we rely on these being preprogrammed?)
    data.PLL2_RCLK_SEL = 0
    data.PLL2_RDIV_PRE = 0
    data.PLL2_RDIV_SEC = 5
    data.PLL2_DISABLE_3RD4TH = 15
    data.PLL2_CP = 1
    data.PLL2_LF_R2 = 2
    data.PLL2_LF_C1 = 0
    data.PLL2_LF_R3 = 1
    data.PLL2_LF_R4 = 1
    data.PLL2_LF_C4 = 7
    data.PLL2_LF_C3 = 7
    return data

def do_freq(dev: Device, args: argparse.Namespace, raw: bool) -> None:
    target = make_freq_target(args, raw)
    plan = lmk05318b_plan.plan(target)
    data = freq_make_data(plan)
    report_freq(data, raw, False)

    # Software reset.
    data.RESET_SW = 1
    # Write the registers.
    masked_write(dev, data)

    # Remove software reset.
    data.RESET_SW = 0
    message.lmk05318b_write(dev.get_usb(), 12, data.data[12])
    # Force an update of the status LEDs.
    message.lmk05318b_status(dev.get_usb())

def set_drives(dev: Device, drives: list[str | None]) -> None:
    data = MaskedBytes()

    assert len(drives) == 8
    for ch, drive in enumerate(drives):
        if drive is None:
            continue
        if drive.startswith('cmos'):
            assert ch >= 4
        assert drive in DRIVES
        sel, mode1, mode2, _ = DRIVES[drive]
        data.insert(f'OUT{ch}_SEL', sel)
        data.insert(f'OUT{ch}_MODE1', mode1)
        data.insert(f'OUT{ch}_MODE2', mode2)

    masked_write(dev, data)


def do_drive(dev: Device, drives: list[Tuple[str, str]],
             defaults: bool) -> None:
    if defaults:
        drives = [('0', 'lvds8'), ('1', 'off'), ('2', 'lvds8'), ('3', 'off'),
                  ('4', 'off'), ('5', 'off'), ('6', 'cmos+z'), ('7', 'lvds8')] \
                  + drives
    expanded: list[None|str] = [None] * 8
    for ch, drive in drives:
        assert len(ch) == 1 and ch >= '0' and ch < '8'
        if drive.startswith('cmos'):
            assert ch >= '4'
        if drive.startswith('2'):
            assert ch == '0' or ch == '2'
            drive = drive[1:]
            expanded[int(ch)+1] = drive
        expanded[int(ch)] = drive

    set_drives(dev, expanded)

# FIXME - add defaults.
def do_drive_out(dev: Device, drives: list[Tuple[str, str]]) -> None:
    indexes = {'1': 2, '2': 0, '3': 7, '4': 6, '5': 5, '4': 4}
    expanded: list[str | None] = [None] * 8
    for ch, drive in drives:
        index = indexes[ch]
        if index >= 4:
            expanded[index] = drive
        elif drive.startswith('2'):
            expanded[index] = expanded[index+1] = drive[1:]
        elif drive.startswith('lvds'):
            split = {'': '40', '4': '40', '6': '60', '8': '80',
                     '10': '64', '12': '66', '14': '86', '16': '88'}[drive[4:]]
            expanded[index] = f'lvds{split[0]}' if split[0] != '0' else 'off'
            expanded[index+1] = f'lvds{split[1]}' if split[1] != '0' else 'off'
        else:
            expanded[index] = drive
            expanded[index+1] = 'off'

    set_drives(dev, expanded)

def drive_description(sel: int, mode1: int, mode2: int) -> str:
    '''Return a description of a LMK05313b output, given the three config
    registers for it.'''
    try:
        tag, name = DRIVES_BY_SEL[sel, mode1, mode2]
        return f'{name} [{tag}]'
    except KeyError:
        return f'sel={sel} mode1={mode1} mode2={mode2}'

def drive_config(data: MaskedBytes, num: int|str) -> Tuple[int, int, int]:
    '''Return the three config registers for a LMK05313b output.'''
    sel   = data.extract(f'OUT{num}_SEL')
    mode1 = data.extract(f'OUT{num}_MODE1')
    mode2 = data.extract(f'OUT{num}_MODE2')
    return sel, mode1, mode2

def report_drive(dev: Device) -> None:
    data = MaskedBytes()
    base, length = 50, 24
    drives_data = message.lmk05318b_read(dev.get_usb(), base, length)
    data.data[base : base + length] = drives_data

    pdowns = '0_1 0_1 2_3 2_3 4 5 6 7'.split()

    for i, pd in enumerate(pdowns):
        pdown = ' Power down,' if data.extract(f'CH{pd}_PD') else ''
        print(f'Channel {i}:{pdown}',
              drive_description(*drive_config(data, i)))

def report_driveout(dev: Device) -> None:
    '''Describe the device output drives.'''
    data = MaskedBytes()
    base, length = 50, 24
    drives_data = message.lmk05318b_read(dev.get_usb(), base, length)
    data.data[base : base + length] = drives_data

    for _, name, outs in CHANNELS_COOKED:
        pdown = ' Power down,' if data.extract(f'CH{outs}_PD') else ''
        sel, mode1, mode2 = drive_config(data, outs[0])
        if len(outs) == 1:
            # The simple case, a single LMK05318b output drives the device
            # output.
            print(f'{name}:{pdown}', drive_description(sel, mode1, mode2))
            continue
        selb, mode1b, mode2b = drive_config(data, outs[2])
        # If both are LVDS, then report the total current.  Otherwise
        # just report ad hoc.
        if sel in (0, 3) and selb in (0, 3):
            print(f'{name}:{pdown} Off [2off]')
            continue
        if not sel == 2 and not selb == 2:
            # One is LVDS and the other is either LVDS or off.
            ca = (4, 6, 8, 8)[mode1] if sel == 1 else 0
            cb = (4, 6, 8, 8)[mode1b] if selb == 1 else 0
            c = ca + cb
            if ca == cb:
                tag = f'2lvds{ca}'
            else:
                ta = f'lvds{ca}' if ca else 'off'
                tb = f'lvds{cb}' if cb else 'off'
                tag = f'{ta} {tb}'
            print(f'{name}:{pdown} LVDS {c}mA [{tag}]')
            continue
        print(f'{name}:{pdown}', drive_description(sel, mode1, mode2),
              '+', drive_description(selb, mode1b, mode2b))

def report_live_freq(dev: Device, raw: bool) -> None:
    # FIXME - retrieve the reference frequency!
    d = MaskedBytes()
    # For now, pull everything...
    for a in lmk05318b.ADDRESSES:
        d.mask[a.address] = 0xff
    ranges = d.ranges(max_block = 30)
    get_ranges(dev, d, ranges)

    report_freq(d, raw, True)

def report_freq(d: MaskedBytes, raw: bool, report_pd: bool) -> None:
    pll2_rdiv = (d.PLL2_RDIV_PRE + 3) * (d.PLL2_RDIV_SEC + 1)
    dpll_fb_pre_div = d.DPLL_REF_FB_PRE_DIV + 2

    if d.DPLL_REF_DEN:
        dpll_fb_div = d.DPLL_REF_FB_DIV + Fraction(d.DPLL_REF_NUM, d.DPLL_REF_DEN)
    else:
        dpll_fb_div = Fraction(0)

    if d.DPLL_PRIREF_RDIV:
        baw_freq = REF_FREQ / d.DPLL_PRIREF_RDIV * 2 * dpll_fb_pre_div \
            * dpll_fb_div
    else:
        baw_freq = Fraction(0)

    if d.PLL2_DEN == 0:
        pll2_freq = Fraction(0)
        pll2_fb_div = Fraction(0)
    else:
        pll2_fb_div = d.PLL2_NDIV + Fraction(d.PLL2_NUM, d.PLL2_DEN)
        pll2_freq = baw_freq / pll2_rdiv * pll2_fb_div

    for _, name, ch in CHANNELS_RAW if raw else CHANNELS_COOKED:
        if not d.extract(f'CH{ch}_PD'):
            pd = ''
        elif report_pd:
            pd = ' (power down)'
        else:
            continue

        dividers = []
        mux = d.extract(f'CH{ch}_MUX')
        if mux == 2:
            dividers = [d.PLL2_P1 + 1]
        elif mux == 3:
            dividers = [d.PLL2_P2 + 1]

        dividers.append(d.extract(f'OUT{ch}_DIV') + 1)
        if ch == '7':
            dividers.append(d.OUT7_STG2_DIV + 1)

        source = 'BAW'
        muxed = baw_freq
        if mux >= 2:
            source = 'PLL2'
            muxed = pll2_freq

        ch_freq = muxed
        for div in dividers:
            ch_freq /= div

        print(f'{name} {freq_to_str(ch_freq)}{pd} {source} dividers',
              *dividers)

    print()
    print(f'BAW         {freq_to_str(baw_freq)} = {freq_to_str(REF_FREQ)}',
          f'/ {d.DPLL_PRIREF_RDIV} * 2 * {dpll_fb_pre_div} *',
          fraction_to_str(dpll_fb_div))
    if report_pd or not d.PLL2_PDN:
        print(f'PLL2        {freq_to_str(pll2_freq)}',
              f'= BAW / {pll2_rdiv} * {fraction_to_str(pll2_fb_div)}')

def do_status(dev: Device) -> None:
    message.lmk05318b_status(dev.get_usb())
    data = MaskedBytes()
    get_ranges(dev, data, [(13, 17)])
    #print(data.data[13:20])
    # Pair up all the LOL and MASK flags...
    addresses = [lmk05318b.ADDRESS_BY_NUM[i] for i in range(13, 17)]
    lols = [f for a in addresses[:2] for f in a.fields
            if f.basename != 'RESERVED']
    msks = [f for a in addresses[2:] for f in a.fields
            if f.basename != 'RESERVED']
    assert len(lols) == len(msks)
    lols.sort(key = lambda f: (f.address, f.byte_lo))
    msks.sort(key = lambda f: (f.address, f.byte_lo))
    for lol, msk in zip(lols, msks):
        assert msk.address == lol.address + 2
        assert msk.byte_lo == lol.byte_lo
        is_lol = data.extract(lol.basename)
        is_msk = data.extract(msk.basename)
        # ❌❗🔴🟥🛑🚫🚨😷
        if is_lol:
            mark = '✖ ' if is_msk else '❌'
        else:
            mark = '✅'
        masked = ' 😷' if is_msk else ''
        print(f'{lol.basename:11} {mark}{masked}')

def do_upload(dev: Device, path: str) -> None:
    tcs = tics.read_tcs_file(path)
    masked_write(dev, tcs)

def add_freq_commands(subp: Any, short: str, long: str) -> None:
    epilog = f'''Each frequency on the command line corresponds to a {long}.
    Use 0 to turn an output off.  The frequency can be specified as either
    fraction (315/88) or a decimal number (3.579545), with an optional unit that
    defaults to MHz.'''
    freq = subp.add_parser(
        'freq', aliases=['frequency'], help='Program/report frequencies',
        description='''Program or report frequencies.  If a list of frequencies
        is given, these are programmed to the device.  With no arguments, report
        the current device frequencies.''',
        epilog=epilog)
    plan = subp.add_parser(
        'plan', help='Frequency planning', epilog = epilog,
        description='''Compute and print a frequency plan without programming it
        to the device.''')

    for p, n in (freq, '*'), (plan, '+'):
        p.add_argument('FREQ', nargs=n, type=str_to_freq,
                       help=f'Frequencies for each {short}')
        p.add_argument('-2', '--pll2', type=str_to_freq,
                       help=f'Forced divisor of PLL2 frequency')

def add_to_argparse(argp: argparse.ArgumentParser,
                    dest: str = 'command', metavar: str = 'COMMAND') -> None:

    def register_lookup(name: str) -> Register:
        try:
            return Register.get(name)
        except KeyError:
            raise ValueError
    register_lookup.__name__ = 'register name'

    def reg_key_value(s: str) -> Tuple[Register, int]:
        if not '=' in s:
            raise ValueError('Key/value pairs must be in the form KEY=VALUE')
        K, V = s.split('=', 1)
        return register_lookup(K), int(V, 0)
    reg_key_value.__name__ = 'register key=value pair'

    def key_value(s: str) -> Tuple[str, str]:
        if not '=' in s:
            raise ValueError('Key/value pairs must be in the form KEY=VALUE')
        k, v =  s.split('=', 1)
        return k, v
    key_value.__name__ = 'key=value pair'

    subp = argp.add_subparsers(
        dest=dest, metavar=metavar, required=True, help='Sub-command')

    add_freq_commands(subp, 'channel', 'LMK0531b channel')
    FREQ_EPILOG='''Each frequency on the command line corresponds to a LMK05318b
    channel.  Use 0 to turn an output off.  The frequency can be specified as
    either fraction (315/88) or a decimal number (3.579545), with an optional
    unit that defaults to MHz.'''

    drive = subp.add_parser('drive', help='Set/report output drive',
                            description='Set/port output drive')
    drive.add_argument('-d', '--defaults', action='store_true',
                       help='Set default values')
    drive.add_argument('DRIVE', type=key_value, nargs='*', metavar='CH=DRIVE',
                       help='Channel and drive type / strength')

    status = subp.add_parser('status', help='Report oscillator status',
                             description='Report oscillator status.')

    save = subp.add_parser(
        'save', help='Save clock gen config to flash.',
        description='''Save running LMK05318b configuration to CPU flash.
        Other configuration saved in flash, such as GPS, will be preserved.''')
    save.add_argument('-n', '--dry-run', action='store_true', default=False,
                      help="Don't actually write to flash.")

    message_util.add_reset_command(subp, 'LMK05318b')

    upload = subp.add_parser(
        'upload', help='Upload TICS Pro .tcs file',
        description='Upload TICS Pro .tcs file')
    upload.add_argument('FILE', help='Name of .tics file')

    valget = subp.add_parser(
        'get', help='Get registers', description='Get registers')
    valget.add_argument('KEY', type=register_lookup, nargs='+', help='KEYs')

    dump = subp.add_parser(
        'dump', help='Get all registers', description='Get all registers')

    valset = subp.add_parser(
        'set', help='Set registers', description='Set registers')
    valset.add_argument('KV', type=reg_key_value, nargs='+',
                        metavar='KEY=VALUE', help='KEY=VALUE pairs')

def run_command(args: argparse.Namespace, device: Device, command: str) -> None:
    if command == 'freq':
        if len(args.FREQ) != 0:
            do_freq(device, args, True)
        else:
            report_live_freq(device, True)

    elif command == 'plan':
        target = make_freq_target(args, True)
        plan = lmk05318b_plan.plan(target)
        data = freq_make_data(plan)
        report_freq(data, True, False)

    elif command == 'drive':
        if args.DRIVE or args.defaults:
            do_drive(device, args.DRIVE, bool(args.defaults))
        else:
            report_drive(device)

    elif command == 'status':
        do_status(device)

    elif command == 'save':
        config.save_config(device, save_ubx=False, save_lmk = True,
                           dry_run = args.dry_run)

    elif command == 'reset':
        message_util.do_reset_line(device.get_usb(),
                                   message.LMK05318B_PDN, args)

    elif command == 'upload':
        do_upload(device, args.FILE)

    elif command == 'get':
        do_get(device, args.KEY)

    elif command == 'dump':
        do_dump(device)

    elif command == 'set':
        do_set(device, args.KV)

    else:
        print(args)
        assert None, f'This should never happen: {command}'

if __name__ == '__main__':
    argp = argparse.ArgumentParser(description='LMK05318b utility')
    add_to_argparse(argp)

    args = argp.parse_args()
    run_command(args, Device(args), args.command)
