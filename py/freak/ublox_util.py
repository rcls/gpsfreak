#!/usr/bin/python3

from freak import config, message, message_util, serhelper, ublox_cfg
from .freak_util import Device
from .ublox_defs import parse_key_list, get_cfg_changes, get_cfg_multi
from .ublox_cfg import UBloxCfg
from .ublox_msg import UBloxMsg, UBloxReader

import argparse, usb, struct, sys, time

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
    subp = argp.add_subparsers(dest=dest, metavar=metavar,
                               required=True, help='Sub-command')

    info = subp.add_parser('info', description='Basic GPS unit info.',
                           help='Basic GPS unit info')

    def key_value(s: str) -> Tuple[str, str]:
        if not '=' in s:
            raise ValueError('Key/value pairs must be in the form KEY=VALUE')
        K, V = s.split('=', 1)
        return K, V

    valset = subp.add_parser(
        'set', description='Set configuration values.',
        help='Set configuration values.')
    valset.add_argument('KV', type=key_value, nargs='+',
                        metavar='KEY=VALUE', help='KEY=VALUE pairs')

    valget = subp.add_parser(
        'get', description='Get configuration values.',
        help='Get configuration values.')
    valget.add_argument('KEY', nargs='+', help='KEYs')
    valget.add_argument('-l', '--layer', default=0, type=int,
                        help='Configuration layer to retrieve.')

    dump = subp.add_parser('dump', description='Retrieve entire config',
                           help='Retrieve entire config.')
    dump.add_argument('-l', '--layer', default=0, type=int,
                      help='Configuration layer to retrieve.')

    save = subp.add_parser(
        'save', help='Save GPS configuration to flash',
        description='''Save currently running GPS configuration to flash.  Other
        configuration saved in flash, such as the LMK05318b clock generator
        configuration, will be preserved.''')
    save.add_argument('-n', '--dry-run', action='store_true', default=False,
                      help="Don't actually write to flash.")

    message_util.add_reset_command(subp, 'GPS unit')

    changes = subp.add_parser(
        'changes', help='Report changed config items',
        description='''Report changed config items.  Running configuration items
        that differ from the GPS unit factory default configuration are listed.
        Note that listed changes are not necessarily saved in flash.''')

    release = subp.add_parser(
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

def do_set(reader: UBloxReader, KV: list[Tuple[str, str]]) -> None:
    # TODO - this only copes with 64 values!
    payload = bytes((0, 1, 0, 0))
    for K, V in KV:
        cfg = UBloxCfg.get(K)
        val = cfg.to_value(V)
        payload += cfg.encode_key_value(val)
    msg = UBloxMsg.get('CFG-VALSET')

    message = msg.frame_payload(payload)
    reader.command(message)

def fmt_cfg_value(cfg: UBloxCfg, value: Any) -> str:
    hd = cfg.val_byte_len() * 2 + 2
    if cfg.typ[0] in 'EX':
        return f'{value:#0{hd}x}'
    elif isinstance(value, int):
        return f'{value} {value:#0{hd}x}'
    else:
        return f'{value}'

def do_get(reader: UBloxReader, layer: int, KEYS: list[str]) -> None:
    for key, value in get_cfg_multi(reader, 0, KEYS):
        print(key, '=', fmt_cfg_value(key, value))

def do_dump(reader: UBloxReader, layer: int) -> None:
    items = get_cfg_multi(reader, layer, [0xffffffff])
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
    serhelper.flushread(device.get_serial())
    time.sleep(0.1)
    message.set_baud(device.get_usb(), baud)
    time.sleep(0.1)
    # Now try the UBX again, check the response.
    device.get_ublox().command(msg)

def do_changes(reader: UBloxReader) -> None:
    for cfg, now, rom in get_cfg_changes(reader):
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
    version, nPins, flags = result[:3]
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

def do_scrape(FILE):
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
            device.get_usb().attach_kernel_driver(0)
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
