#!/usr/bin/python3

import argparse
import sys
import ublox_defs

from ublox_cfg import UBloxCfg
from ublox_msg import UBloxMsg

argp = argparse.ArgumentParser(description='UBLOX utility')
subp = argp.add_subparsers(dest='command', required=True, help='Command')

argp.add_argument('--binary', '-b', action='store_true', help='Output binary')

def key_value(s):
    if not '=' in s:
        raise ValueError('Key/value pairs must be in the form KEY=VALUE')
    return s.split('=', 1)

valset = subp.add_parser('set', description='VALSET message')
valset.add_argument('KV', type=key_value, nargs='+', help='KEY=VALUE pairs')

args = argp.parse_args()

if args.command == 'set':
    payload = bytes((0, 1, 0, 0))
    for K, V in args.KV:
        cfg = UBloxCfg.get(K)
        val = cfg.to_value(V)
        payload += cfg.encode_key_value(val)
    msg = UBloxMsg.get('CFG-VALSET')
    message = msg.frame_payload(payload)
    if args.binary:
        sys.stdout.buffer.write(message)
    else:
        print(message.hex(' '))
