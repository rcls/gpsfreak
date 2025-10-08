#!/usr/bin/python3

from freak import serhelper, ublox_cfg
from freak.ublox_defs import parse_key_list
from freak.ublox_cfg import UBloxCfg
from freak.ublox_msg import UBloxMsg, UBloxReader

import argparse
import struct
import sys

from typing import Tuple

argp = argparse.ArgumentParser(description='UBLOX utility')
subp = argp.add_subparsers(dest='command', required=True, help='Command')

argp.add_argument('--binary', '-b', action='store_true', help='Output binary')
argp.add_argument('--device', '-d', help='Device (or file) to write')

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

dump = subp.add_parser('dump', description='Retrieve entire config')

scrape = subp.add_parser('scrape', description='Scrape pdftotext output')
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
    if args.device:
        device = serhelper.Serial(args.device)
        reader = UBloxReader(device)
        reader.command(message)
    elif args.binary:
        sys.stdout.buffer.write(message)
    else:
        print(message.hex(' '))

def do_get(KEYS: list[str]) -> None:
    payload = b'\0\0\0\0'
    cfg_list = [UBloxCfg.get(K) for K in KEYS]
    for cfg in cfg_list:
        payload += struct.pack('<I', cfg.key)
    msg = UBloxMsg.get('CFG-VALGET')
    message = msg.frame_payload(payload)
    if args.binary:
        sys.stdout.buffer.write(message)
        return
    elif args.device is None:
        print(message.hex(' '))
        return

    device = serhelper.Serial(args.device)
    reader = UBloxReader(device)
    result = reader.transact(message, ack=True)
    assert result[:4] == b'\1\0\0\0'
    print('Got', result.hex(' '))
    pos = 4
    for cfg in cfg_list:
        key, = struct.unpack('<I', result[pos:pos+4])
        assert key == cfg.key
        pos += 4
        val_bytes = cfg.val_bytes()
        value = cfg.decode_value(result[pos : pos + val_bytes])
        pos += val_bytes
        if isinstance(value, int):
            hd = val_bytes * 2 + 2
            print(f'{cfg.name} = {value} {value:#0{hd}x}')
        else:
            print(f'{cfg.name} = {value}')
        assert pos == len(result)

def do_dump() -> None:
    assert args.device is not None
    device = serhelper.Serial(args.device)
    reader = UBloxReader(device)
    start = 0
    items = []

    while True:
        msg = UBloxMsg.get('CFG-VALGET').frame_payload(
            struct.pack('<BBHI', 0, 0, start, 0xffffffff))
        result = reader.transact(msg)
        assert struct.unpack('<H', result[2:4])[0] == start
        offset = 4
        num_items = 0
        while offset < len(result):
            num_items += 1
            assert len(result) - offset > 4
            key = struct.unpack('<I', result[offset:offset + 4])[0]
            cfg = ublox_cfg.get_cfg(key)
            val_bytes = cfg.val_bytes()
            #print(repr(cfg), val_bytes)
            offset += 4 + val_bytes
            assert offset <= len(result)
            value = cfg.decode_value(result[offset - val_bytes:offset])
            if cfg.typ[0] in 'EX':
                w = int(cfg.typ[1]) * 2 + 2
                vstr = f'{value:#0{w}x}'
            else:
                vstr = f'{value}'
            items.append((cfg, vstr))
        start += num_items
        if num_items < 64:
            break

    items.sort(key=lambda x: x[0].key & 0x0fffffff)
    for cfg, value in items:
        print(cfg, value)

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

if args.command == 'get':
    do_get(args.KEY)

if args.command == 'dump':
    do_dump()

if args.command == 'scrape':
    do_scrape(args.FILE)
