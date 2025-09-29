#!/usr/bin/python3

import os
import re
import ublox_cfg, ublox_msg

from ublox_cfg import UBloxCfg, CONFIGS_BY_KEY, CONFIGS_BY_NAME
from ublox_msg import UBloxMsg, MESSAGES_BY_CODE, MESSAGES_BY_NAME

def parse_key_list(doc_path: str) -> None:
    msg_line_re = re.compile(r' *3\.\d+.\d+')
    msg_sect_re = re.compile(r'3\.\d+.\d+$')
    msg_name_re = re.compile(r'UBX-\w+-\w+$')
    msg_num1_re = re.compile(r'\((0x[0-9a-f]{2})')
    msg_num2_re = re.compile(r'(0x[0-9a-f]{2})\)(\.+\d*)?')

    cfg_name_re = re.compile(r'CFG-[\w_-]+$')
    cfg_key_re  = re.compile(r'0x[0-9A-F]{8}$')

    assert msg_sect_re.match('3.9.1')
    assert msg_name_re.match('UBX-NAV2-TIMEUTC')
    assert msg_num1_re.match('(0x05')
    assert msg_num2_re.match('0x01)')
    assert msg_num2_re.match('0x01)....')
    assert msg_num2_re.match('0x01)....64')
    assert cfg_name_re.match('CFG-ABCD-FOO_BAR')
    assert cfg_key_re.match('0x12345678')
    assert not cfg_key_re.match('0x1234567')
    assert not cfg_key_re.match('0x123456789')

    for L in open(doc_path):
        w = L.strip().split()
        if msg_line_re.match(L):
            if len(w) < 4:
                continue
            if not msg_name_re.match(w[1]):
                continue
            assert msg_sect_re.match(w[0]), L
            num1 = msg_num1_re.match(w[2])
            num2 = msg_num2_re.match(w[3])
            assert num1, L
            assert num2, (L, w[3])
            name = w[1].removeprefix('UBX-')
            # Little endian!
            code = int(num1.group(1), 0) + 256 * int(num2.group(1), 0)
            message = UBloxMsg(name, code)
            MESSAGES_BY_CODE[code] = message
            MESSAGES_BY_NAME[name] = message
            #print(f'{name} {code:#06x}')
        if L.startswith('CFG-'):
            if len(w) < 3:
                continue
            assert cfg_name_re.match(w[0]), w
            if not cfg_key_re.match(w[1]):
                continue
            name = w[0].removeprefix('CFG-')
            key  = int(w[1], 0)
            ty   = w[2]
            config = UBloxCfg(name, key, ty)
            CONFIGS_BY_KEY [key]  = config
            CONFIGS_BY_NAME[name] = config
            #print(f'{name} {key:#x} {ty}')

parse_key_list(os.path.dirname(__file__) + '/F10-intf.txt')

#cfg, val, length = UBloxCfg.decode_from(bytes.fromhex('2500054080969800'))
#print(f'{cfg} {val} {length}')

#key = UBloxCfg.get(sys.argv[1])
#value = key.to_value(sys.argv[2])

#b = key.encode_key_value(value)
#print(b.hex())
#print(UBloxCfg.decode_from(b))

#msg = UBloxMsg.get('CFG-VALSET')

#m = msg.frame_payload(bytes((0, 1, 0 ,0)) + b)
#print(m.hex(' '))

#sys.stdout.buffer.write(m)

