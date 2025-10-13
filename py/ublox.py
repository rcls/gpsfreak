#!/usr/bin/python3

from freak import serhelper, ublox_cfg
from freak.ublox_defs import parse_key_list, get_cfg_changes, get_cfg_multi
from freak.ublox_cfg import UBloxCfg
from freak.ublox_msg import UBloxMsg, UBloxReader

import argparse
import struct
import sys

# My current changes:
# CFG-TP-PULSE_DEF 0x01 was 0x00
# CFG-TP-FREQ_LOCK_TP1 8844582 0x0086f526 was 1 0x00000001
# CFG-TP-DUTY_LOCK_TP1 50.0 was 10.0
# CFG-TP-PULSE_LENGTH_DEF 0x00 was 0x01
# CFG-SBAS-USE_TESTMODE True 0x01 was False 0x00
# CFG-SBAS-PRNSCANMASK 0x0000000000000000 was 0x000000000003ab88
# CFG-UART1-BAUDRATE 115200 0x0001c200 was 9600 0x00002580


from typing import Any, Tuple

argp = argparse.ArgumentParser(description='UBLOX utility')

subp = argp.add_subparsers(
    dest='command', metavar='COMMAND', required=True, help='Command')

def key_value(s: str) -> Tuple[str, str]:
    if not '=' in s:
        raise ValueError('Key/value pairs must be in the form KEY=VALUE')
    K, V = s.split('=', 1)
    return K, V

valset = subp.add_parser(
    'set', description='Set configuration values.',
    help='Set configuration values.')

valget = subp.add_parser(
    'get', description='Get configuration values.',
    help='Get configuration values.')

dump = subp.add_parser('dump', description='Retrieve entire config',
                       help='Retrieve entire config')
dump.add_argument('-l', '--layer', default=0, type=int,
                  help='Configuration layer to retrieve')

info = subp.add_parser('info', description='Basic GPS unit info',
                       help='Basic GPS unit info')

changes = subp.add_parser('changes', description='Report changed config items',
                          help='Report changed config items')

scrape = subp.add_parser('scrape', description='Scrape pdftotext output',
                         help='Scrape pdftotext output')

for a in valset, valget, dump, changes, info:
    a.add_argument('DEVICE', help='Serial port to talk to device')

valget.add_argument('KEY', nargs='+', help='KEYs')
valset.add_argument('KV', type=key_value, nargs='+',
                    metavar='KEY=VALUE', help='KEY=VALUE pairs')

scrape.add_argument('FILE', help='Text file to parse')

args = argp.parse_args()

def do_set(KV: list[Tuple[str, str]]) -> None:
    payload = bytes((0, 1, 0, 0))
    for K, V in KV:
        cfg = UBloxCfg.get(K)
        val = cfg.to_value(V)
        payload += cfg.encode_key_value(val)
    msg = UBloxMsg.get('CFG-VALSET')
    message = msg.frame_payload(payload)

    device = serhelper.Serial(args.DEVICE)
    reader = UBloxReader(device)
    reader.command(message)

def fmt_cfg_value(cfg: UBloxCfg, value: Any) -> str:
    hd = cfg.val_byte_len() * 2 + 2
    if cfg.typ[0] in 'EX':
        return f'{value:#0{hd}x}'
    elif isinstance(value, int):
        return f'{value} {value:#0{hd}x}'
    else:
        return f'{value}'

def do_get(KEYS: list[str]) -> None:
    payload = b'\0\0\0\0'
    cfg_list = [UBloxCfg.get(K) for K in KEYS]
    for cfg in cfg_list:
        payload += struct.pack('<I', cfg.key)
    msg = UBloxMsg.get('CFG-VALGET')
    message = msg.frame_payload(payload)

    device = serhelper.Serial(args.DEVICE)
    reader = UBloxReader(device)
    result = reader.transact(message, ack=True)
    assert result[:4] == b'\1\0\0\0'

    pos = 4
    for cfg in cfg_list:
        key, = struct.unpack('<I', result[pos:pos+4])
        assert key == cfg.key
        pos += 4
        val_byte_len = cfg.val_byte_len()
        value = cfg.decode_value(result[pos : pos + val_byte_len])
        pos += val_byte_len
        print(cfg, '=', fmt_cfg_value(cfg, value))

def do_dump() -> None:
    assert args.DEVICE is not None
    device = serhelper.Serial(args.DEVICE)
    reader = UBloxReader(device)

    items = get_cfg_multi(reader, args.layer, [0xffffffff])
    items.sort(key=lambda x: x[0].key & 0x0fffffff)
    for cfg, value in items:
        print(cfg, fmt_cfg_value(cfg, value))

def do_changes() -> None:
    assert args.DEVICE is not None
    device = serhelper.Serial(args.DEVICE)
    reader = UBloxReader(device)

    for cfg, now, rom in get_cfg_changes(reader):
        print(cfg, fmt_cfg_value(cfg, now), 'was', fmt_cfg_value(cfg, rom))

def do_info() -> None:
    def binstr(b: bytes) -> str:
        b = b.rstrip(b'\0')
        try:
            return str(b, 'utf-8')
        except:
            return b.hex(' ')

    assert args.DEVICE is not None
    device = serhelper.Serial(args.DEVICE)
    reader = UBloxReader(device)

    msg = UBloxMsg.get('MON-VER')
    message = msg.frame_payload(b'')
    result = reader.transact(message, ack=False)
    assert len(result) % 30 == 10 and len(result) >= 40
    swVersion = binstr(result[:30])
    hwVersion = binstr(result[30:40])
    print(f'Software version {swVersion}, hardware version {hwVersion}')
    for i in range(40, len(result), 30):
        print('Extension', binstr(result[i : i+10]))

    msg = UBloxMsg.get('MON-HW3')
    message = msg.frame_payload(b'')
    result = reader.transact(message, ack=False)
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


if args.command == 'set':
    do_set(args.KV)

elif args.command == 'get':
    do_get(args.KEY)

elif args.command == 'dump':
    do_dump()

elif args.command == 'changes':
    do_changes()

elif args.command == 'info':
    do_info()

elif args.command == 'scrape':
    do_scrape(args.FILE)

else:
    assert False, 'This should never happen'
