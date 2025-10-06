#!/usr/bin/python3

import struct
import serhelper

from freak import message, ublox_cfg, ublox_defs, ublox_msg

ser = serhelper.Serial('/dev/ttyACM3', 9600)

reader = ublox_msg.UBloxReader(ser)

GETCFG = ublox_msg.MESSAGES_BY_NAME['CFG-VALGET']
MONVER = ublox_msg.MESSAGES_BY_NAME['MON-VER']
ACK    = ublox_msg.MESSAGES_BY_NAME['ACK-ACK']

if False:
    msg = MONVER.frame_payload(b'')
    result = reader.transact(msg, ack=False)
    print([x for x in result.split(b'\0') if x != b''])
    import sys
    sys.exit(0)

items = []

start = 0
while True:
    payload = struct.pack('<BBHI', 0, 0, start, 0xffffffff)
    assert len(payload) == 8
    msg = GETCFG.frame_payload(payload)
    payload = reader.transact(msg)
    #code, payload = reader.get_msg()
    #print(f'{code:#010x}', payload.hex(' '))
    #assert code == GETCFG.code
    assert struct.unpack('<H', payload[2:4])[0] == start
    #ack_code, _ = reader.get_msg()
    #assert ack_code == ACK.code
    offset = 4
    num_items = 0
    while offset < len(payload):
        num_items += 1
        assert len(payload) - offset > 4
        key = struct.unpack('<I', payload[offset:offset + 4])[0]
        cfg = ublox_cfg.get_cfg(key)
        val_bytes = cfg.val_bytes()
        #print(repr(cfg), val_bytes)
        offset += 4 + val_bytes
        assert offset <= len(payload)
        value = cfg.decode_value(payload[offset - val_bytes:offset])
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
