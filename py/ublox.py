#!/usr/bin/python3

from freak import serhelper, ublox_cfg
from freak.ublox_defs import parse_key_list, get_cfg_multi
from freak.ublox_cfg import UBloxCfg
from freak.ublox_msg import UBloxMsg, UBloxReader

import argparse
import struct
import sys

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

changes = subp.add_parser('changes', description='Report changed config items',
                          help='Report changed config items')

scrape = subp.add_parser('scrape', description='Scrape pdftotext output',
                         help='Scrape pdftotext output')

for a in valset, valget, dump, changes:
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

    live = get_cfg_multi(reader, 0, [0xffffffff])
    rom  = get_cfg_multi(reader, 7, [0xffffffff])
    live.sort(key=lambda x: x[0].key & 0x0fffffff)
    rom .sort(key=lambda x: x[0].key & 0x0fffffff)

    assert len(live) == len(rom )
    for (cfg_l, value_l), (cfg_r, value_r) in zip(live, rom):
        assert cfg_l == cfg_r
        if value_l != value_r:
            print(cfg_l, fmt_cfg_value(cfg_l, value_l), 'was',
                  fmt_cfg_value(cfg_r, value_r))

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

elif args.command == 'scrape':
    do_scrape(args.FILE)

else:
    assert False, 'This should never happen'
