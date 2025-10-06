#!/usr/bin/python3

import argparse
import serhelper
import struct
import sys

from ublox_defs import parse_key_list
from ublox_cfg import UBloxCfg
from ublox_msg import UBloxMsg, UBloxReader

argp = argparse.ArgumentParser(description='UBLOX utility')
subp = argp.add_subparsers(dest='command', required=True, help='Command')

argp.add_argument('--binary', '-b', action='store_true', help='Output binary')
argp.add_argument('--device', '-d', help='Device (or file) to write')

def key_value(s):
    if not '=' in s:
        raise ValueError('Key/value pairs must be in the form KEY=VALUE')
    return s.split('=', 1)

valset = subp.add_parser('set', description='VALSET message')
valset.add_argument('KV', type=key_value, nargs='+', help='KEY=VALUE pairs')

valset = subp.add_parser('get', description='VALSET message')
valset.add_argument('KEY', nargs='+', help='KEYs')

dump = subp.add_parser('scrape', description='Scrape pdf2txt output')
dump.add_argument('FILE', help='Text file to parse')

args = argp.parse_args()

if args.command == 'set':
    payload = bytes((0, 1, 0, 0))
    for K, V in args.KV:
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

if args.command == 'get':
    payload = b'\0\0\0\0'
    cfg_list = [UBloxCfg.get(K) for K in args.KEY]
    for cfg in cfg_list:
        payload += struct.pack('<I', cfg.key)
    msg = UBloxMsg.get('CFG-VALGET')
    message = msg.frame_payload(payload)
    if args.device:
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
    elif args.binary:
        sys.stdout.buffer.write(message)
    else:
        print(message.hex(' '))

if args.command == 'scrape':
    configs, messages = \
        parse_key_list(args.FILE)
    print('import ublox_msg')
    print('import ublox_cfg')
    print('from ublox_cfg import UBloxCfg')
    print('from ublox_msg import UBloxMsg')
    for tag, items in ('cfg', configs), ('msg', messages):
        print()
        print(f'ublox_{tag}.add_{tag}_list([')
        for item in items:
            print(f'    {item!r},')
        print('])')
