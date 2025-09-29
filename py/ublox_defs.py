#!/usr/bin/python3

import os
import re
import ublox_cfg, ublox_msg, ublox_lists

from ublox_cfg import UBloxCfg
from ublox_msg import UBloxMsg

def parse_key_list(doc_path: str) -> (list[UBloxCfg], list[UBloxMsg]):
    configs = []
    messages = []

    msg_line_re = re.compile(r' *3\.\d+.\d+')
    msg_sect_re = re.compile(r'3\.\d+.\d+$')
    msg_name_re = re.compile(r'UBX-\w+-\w+$')
    msg_num1_re = re.compile(r'\((0x[0-9a-f]{2})', flags=re.I)
    msg_num2_re = re.compile(r'(0x[0-9a-f]{2})\)(\.+\d*)?', flags=re.I)

    cfg_name_re = re.compile(r'CFG-[\w_-]+$')
    cfg_key_re  = re.compile(r'0x[0-9a-f]{8}$', flags=re.I)

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
            messages.append(UBloxMsg(name, code))
        if L.startswith('CFG-'):
            if len(w) < 3:
                continue
            assert cfg_name_re.match(w[0]), w
            if not cfg_key_re.match(w[1]):
                continue
            name = w[0].removeprefix('CFG-')
            key  = int(w[1], 0)
            ty   = w[2]
            configs.append(UBloxCfg(name, key, ty))
    return configs, messages
