#!/usr/bin/python3

from freak import config, message, message_util, serhelper
from .freak_util import Device
from .ublox_defs import parse_key_list, get_config_changes, get_config
from .ublox_cfg import UBloxCfg
from .ublox_msg import UBloxMsg, UBloxReader

import argparse, struct, time
import usb.core # type: ignore

# My current changes:
# CFG-TP-PULSE_DEF 0x01 was 0x00
# CFG-TP-FREQ_LOCK_TP1 8844582 0x0086f526 was 1 0x00000001
# CFG-TP-DUTY_LOCK_TP1 50.0 was 10.0
# CFG-TP-PULSE_LENGTH_DEF 0x00 was 0x01
# CFG-SBAS-USE_TESTMODE True 0x01 was False 0x00
# CFG-SBAS-PRNSCANMASK 0x0000000000000000 was 0x000000000003ab88
# CFG-UART1-BAUDRATE 230400 0x00038400 was 9600 0x00002580
# CFG-MSGOUT-NMEA_ID_GSV_UART1 1 0x01 was 5 0x05

from typing import Any, Tuple

def add_to_argparse(argp: argparse.ArgumentParser,
                    dest: str = 'command', metavar: str = 'COMMAND') -> None:
    argp.add_argument(
        '-s', '--serial',
        help="Serial port for GPS comms (don't use with GPS Freak)")
    argp.add_argument('-b', '--baud', type=int,
                      help='Baud rate for serial (Default is no change)')

    subp = argp.add_subparsers(dest=dest, metavar=metavar,
                               required=True, help='Sub-command')

    subp.add_parser('info', description='Basic GPS unit info.',
                    help='Basic GPS unit info')

    subp.add_parser('status', description='Basic GPS status info.',
                    help='Basic GPS status info')

    def key_value(s: str) -> Tuple[UBloxCfg, Any]:
        if not '=' in s:
            raise ValueError('Key/value pairs must be in the form KEY=VALUE')
        K, V = s.split('=', 1)
        try:
            cfg = UBloxCfg.get(K)
        except KeyError:
            raise ValueError()
        return cfg, cfg.to_value(V)
    key_value.__name__ = 'configuration KEY=VALUE pair'

    def int_key(s: str) -> int:
        try:
            return UBloxCfg.get_int_key(s)
        except KeyError:
            raise ValueError()
    int_key.__name__ = 'configuration key'

    valset = subp.add_parser('set', description='Set configuration values.',
                             help='Set configuration values.')
    valset.add_argument('KV', type=key_value, nargs='+',
                        metavar='KEY=VALUE', help='KEY=VALUE pairs')

    valget = subp.add_parser('get', description='Get configuration values.',
                             help='Get configuration values.')
    valget.add_argument('KEY', nargs='+', type=int_key, help='KEYs')
    valget.add_argument('-l', '--layer', default=0, type=int,
                        help='Configuration layer to retrieve.')

    dump = subp.add_parser('dump', description='Retrieve entire config',
                           help='Retrieve entire config.')
    dump.add_argument('-l', '--layer', default=0, type=int,
                      help='Configuration layer to retrieve.')

    save = subp.add_parser(
        'save', help='Save GPS configuration to flash',
        description='''Save currently running GPS configuration to flash.  This
        will preserve other configuration saved in flash, such as that for the
        LMK05318b clock generator.''')
    save.add_argument('-n', '--dry-run', action='store_true', default=False,
                      help="Don't actually write to flash.")

    message_util.add_reset_command(subp, 'GPS unit')

    subp.add_parser(
        'changes', help='Report changed config items',
        description='''Report changed config items.  Running configuration items
        that differ from the GPS unit factory default configuration are listed.
        Note that listed changes are not necessarily saved in flash.''')

    subp.add_parser(
        'release', help='Release serial port back to OS',
        description='''Release the GPS serial port back to the operating system,
        making it available to other applications.''')

    baud = subp.add_parser(
        'baud', help='Get/set baud rate for GPS',
        description='''Get/set baud rate between the CPU and the GPS module.''',
        epilog='''This attempts to set the baud rate in the GPS module, and
        always changes the baud rate on the CPU.  If anything goes wrong, then
        there is a change that the two will be left inconsistent.''')
    baud.add_argument('BAUD', type=int, nargs='?', help='Baud rate to set')

    scrape = subp.add_parser('scrape', description='Scrape pdftotext output',
                             help='Scrape pdftotext output')

    scrape.add_argument('FILE', help='Text file to parse')

def do_set(reader: UBloxReader, KV: list[Tuple[UBloxCfg, Any]]) -> None:
    # TODO - this only copes with 64 values!
    # Also, layers other than live might be useful?
    payload = bytes((0, 1, 0, 0))
    for cfg, value in KV:
        payload += cfg.encode_key_value(value)

    reader.command('CFG-VALSET', payload)

def fmt_cfg_value(cfg: UBloxCfg, value: Any) -> str:
    hd = cfg.val_byte_len() * 2 + 2
    if cfg.typ[0] in 'EX':
        return f'{value:#0{hd}x}'
    elif isinstance(value, int):
        return f'{value} {value:#0{hd}x}'
    else:
        return f'{value}'

def do_get(reader: UBloxReader, layer: int, KEYS: list[int]) -> None:
    for key, value in get_config(reader, 0, KEYS):
        print(key, '=', fmt_cfg_value(key, value))

def do_dump(reader: UBloxReader, layer: int) -> None:
    items = get_config(reader, layer, [0xffffffff])
    items.sort(key=lambda x: x[0].key & 0x0fffffff)
    for cfg, value in items:
        print(cfg, fmt_cfg_value(cfg, value))

def do_baud(device: Device, baud: int) -> None:
    # Send the baud message to the GPS unit, don't worry about the response.
    baudrate = UBloxCfg.get('UART1-BAUDRATE')
    payload = bytes((0, 1, 0, 0)) + baudrate.encode_key_value(baud)
    valset = UBloxMsg.get('CFG-VALSET')
    msg = valset.frame_payload(payload)
    serhelper.writeall(device.get_serial(), msg)
    time.sleep(0.1)
    serhelper.flushread(device.get_serial())
    message.set_baud(device.get_usb(), baud)
    time.sleep(0.1)
    # Now try the UBX again, check the response.
    device.get_ublox().command(valset, payload)

def do_changes(reader: UBloxReader) -> None:
    for cfg, now, rom in get_config_changes(reader):
        print(cfg, fmt_cfg_value(cfg, now), 'was', fmt_cfg_value(cfg, rom))

def do_info(reader: UBloxReader) -> None:
    def binstr(b: bytes) -> str:
        b = b.rstrip(b'\0')
        try:
            return str(b, 'utf-8')
        except:
            return b.hex(' ')

    result = reader.transact('MON-VER')
    assert len(result) % 30 == 10 and len(result) >= 40
    swVersion = binstr(result[:30])
    hwVersion = binstr(result[30:40])
    print(f'Software version {swVersion}, hardware version {hwVersion}')
    for i in range(40, len(result), 30):
        print('Extension', binstr(result[i : i+30]))

    result = reader.transact('UBX-SEC-UNIQID')
    # The UBX docs give version 1 with a 9 byte payload and 5 byte id, we
    # actually get version 2 with a 10 byte payload and 6 byte id.
    assert len(result) > 0
    assert result[0] in (1, 2)
    assert len(result) == 8 + result[0]
    uniq_id = result[4:].hex(' ')
    print(f'Unique ID {uniq_id}')

    result = reader.transact('MON-HW3')
    _version, nPins, flags = result[:3]
    hwVersion = binstr(result[3:13])
    print(f'HW Version = {hwVersion}')
    print(f'RTC is {"" if flags & 1 else "NOT "}calibrated')
    print(f'Boot mode is {"Safe" if flags & 2 else "Normal"}')
    print(f'XTAL is {"Absent" if flags & 4 else "Present"}')
    assert len(result) == 22 + nPins * 6
    for i in range(nPins):
        data = result[22 + i * 6 : 28 + i * 6]
        _, pinId, pinMask0, pinMask1, VP, _ = data
        pio = 'PIO' if pinMask0 & 1 else 'Peripheral'
        bank = 'ABCDEFGH'[pinMask0 & 14 >> 1]
        direction = 'Output' if pinMask0 & 16 else 'Input'
        value = 'High' if pinMask0 & 32 else 'Low'
        virtual = 'Virtual' if pinMask0 & 64 else 'Non-Virtual'
        irq_enabled = 'Enabled' if pinMask0 & 128 else 'Disabled'
        if pinMask1 & 3 == 1:
            pull = ' Pull-up'
        elif pinMask1 & 3 == 2:
            pull = ' Pull-down'
        elif pinMask1 & 3 == 3:
            pull = ' Pull-both'
        else:
            pull = ''

        print(f'Pin {pinId} {pio} bank {bank} {direction} {value} {virtual} IRQ {irq_enabled} Virt.Pin {VP}{pull}')

def do_status(reader: UBloxReader) -> None:
    status = reader.transact('NAV-STATUS')

    gpsFix: int
    iTOW, gpsFix, flags, fixStat, flags2, ttff, msss \
        = struct.unpack('<IBBBBII', status)

    print(f'iTOW = {iTOW / 1000} seconds')

    FIXES = 'no fix,dead reckoning,2D-fix,3D-fix,GPS + dr,Time only'.split(',')
    if gpsFix < len(FIXES):
        fix = FIXES[gpsFix]
    else:
        fix = f'{gpsFix:#04x}'
    print(f'GPS Fix: {fix}')

    flags_bits = [flags & 1 << i != 0 for i in range(8)]
    print(f'GPS Fix OK: {flags_bits[0]}')
    print(f'Diff soln applied: {flags_bits[1]}')
    print(f'Week number valid: {flags_bits[2]}')
    print(f'Time of week valid: {flags_bits[3]}')
    fixStat_bits = [fixStat & 1 << i != 0 for i in range(8)]
    print(f'Differential Corr.: {fixStat_bits[0]}')
    print(f'Carrier phase solution valid: {fixStat_bits[1]}')
    print(f'Map matching: {fixStat_bits[3] * 2 + fixStat_bits[2]}')

    print(f'Power save mode:', flags2 & 3)
    print(f'Spoof detection mode:', flags2 >> 2 & 3)
    print(f'Carrier phase range status:', flags2 >> 4 & 3)
    print(f'Time to first fix: {ttff / 1000} seconds')
    print(f'Seconds since start: {msss / 1000} seconds')

    clock = reader.transact('NAV-CLOCK')
    _, clkB, clkD, tAcc, fAcc = struct.unpack('<IiiII', clock)
    print(f'Clock bias  {clkB:6} ns')
    print(f'Clock drift {clkD:6} ppb')
    print(f'Time accuracy ±{tAcc:3} ns')
    print(f'Freq accuracy ±{fAcc:3} ppt')

    dop = reader.transact('NAV-DOP')
    _, gDOP, pDOP, tDOP, vDOP, hDOP, nDOP, eDOP = struct.unpack('<IHHHHHHH', dop)

    print(f'DOP: G{gDOP*0.01:5.2f} P{pDOP*0.01:5.2f} T{tDOP*0.01:5.2f} V{vDOP*0.01:5.2f} H{hDOP*0.01:5.2f} N{nDOP*0.01:5.2f} E{eDOP*0.01:5.2f}')

def do_scrape(FILE: str) -> None:
    configs, messages = parse_key_list(FILE)
    print('from freak import ublox_cfg, ublox_msg')
    print('from .ublox_cfg import UBloxCfg')
    print('from .ublox_msg import UBloxMsg')
    for tag, items in ('cfg', configs), ('msg', messages):
        print()
        print(f'ublox_{tag}.add_{tag}_list([')
        for item in items:
            print(f'    {item!r},')
        print('])')

def run_command(args: argparse.Namespace, device: Device, command: str) -> None:
    if command == 'info':
        do_info(device.get_ublox())

    elif command == 'status':
        do_status(device.get_ublox())

    elif command == 'set':
        do_set(device.get_ublox(), args.KV)

    elif command == 'get':
        do_get(device.get_ublox(), args.layer, args.KEY)

    elif command == 'dump':
        do_dump(device.get_ublox(), args.layer)

    elif command == 'save':
        config.save_config(device, save_ubx=True, save_lmk = False,
                           dry_run = args.dry_run)

    elif command == 'baud':
        if args.BAUD is None:
            print('Baud rate is', message.get_baud(device.get_usb()))
        else:
            do_baud(device, args.BAUD)

    elif command == 'changes':
        do_changes(device.get_ublox())

    elif command == 'release':
        try:
            device.get_usb().attach_kernel_driver(0) # type: ignore
        except usb.core.USBError:
            pass

    elif command == 'reset':
        message_util.do_reset_line(device.get_usb(), message.GPS_RESET, args)

    elif command == 'scrape':
        do_scrape(args.FILE)

    else:
        assert False, f'This should never happen {command}'

if __name__ == '__main__':
    argp = argparse.ArgumentParser(description='UBlox GPS utilities')
    add_to_argparse(argp)
    args = argp.parse_args()
    run_command(args, Device(args), args.command)
