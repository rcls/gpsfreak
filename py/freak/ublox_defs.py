
import os
import re
import struct

from typing import Any, Sequence, Tuple

from .ublox_cfg import UBloxCfg
from .ublox_msg import UBloxMsg, UBloxReader

def parse_key_list(doc_path: str) -> Tuple[list[UBloxCfg], list[UBloxMsg]]:
    configs = []
    messages = []

    last_config = None

    msg_line_re = re.compile(r' *3\.\d+.\d+')
    msg_sect_re = re.compile(r'3\.\d+.\d+$')
    msg_name_re = re.compile(r'UBX-\w+-\w+$')
    msg_num1_re = re.compile(r'\((0x[0-9a-f]{2})', flags=re.I)
    msg_num2_re = re.compile(r'(0x[0-9a-f]{2})\)(\.+\d*)?', flags=re.I)

    cfg_name_re = re.compile(r'CFG-[\w_-]+$')
    cfg_key_re  = re.compile(r'0x[0-9a-f]{8}$', flags=re.I)
    cfg_cont_re = re.compile(r'[\w_-]+ {40}')

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
        if cfg_cont_re.match(L) and last_config is not None:
            configs[-1] = UBloxCfg(
                last_config.name + w[0], last_config.key, last_config.typ)
        last_config = None

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
            last_config = UBloxCfg(name, key, ty)
            configs.append(last_config)

    return configs, messages

def get_config(reader: UBloxReader, layer: int,
               keys: Sequence[int|str|UBloxCfg]) \
        -> list[Tuple[UBloxCfg, Any]]:
    assert len(keys) <= 64
    start = 0
    items = []

    key_bin = bytes()
    for key in keys:
        key_bin += struct.pack('<I', UBloxCfg.get_int_key(key))

    valget = UBloxMsg.get('CFG-VALGET')
    while True:
        result = reader.transact(
            valget, struct.pack('<BBH', 0, layer, start) + key_bin, ack = True)
        assert struct.unpack('<H', result[2:4])[0] == start
        offset = 4
        num_items = 0
        while offset < len(result):
            num_items += 1
            assert len(result) - offset > 4
            key = struct.unpack('<I', result[offset:offset + 4])[0]
            cfg = UBloxCfg.get_key_for_int(key)
            val_byte_len = cfg.val_byte_len()
            #print(repr(cfg), val_byte_len)
            offset += 4 + val_byte_len
            assert offset <= len(result)
            value = cfg.decode_value(result[offset - val_byte_len:offset])
            items.append((cfg, value))
        start += num_items
        if num_items < 64:
            return items

def get_config_changes(dev: UBloxReader, key: int = 0xffffffff) \
        -> list[Tuple[UBloxCfg, Any, Any]]:
    live = get_config(dev, 0, [key])
    rom  = get_config(dev, 7, [key])
    live.sort(key=lambda x: x[0].key & 0x0fffffff)
    rom .sort(key=lambda x: x[0].key & 0x0fffffff)

    assert len(live) == len(rom )
    result = []
    for (cfg_l, value_l), (cfg_r, value_r) in zip(live, rom):
        assert cfg_l == cfg_r
        if value_l != value_r:
            result.append((cfg_l, value_l, value_r))
    return result
