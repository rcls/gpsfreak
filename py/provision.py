#!/usr/bin/python3

from freak import crc32, lmk05318b, message, serhelper, ublox_msg
from freak.lmk05318b import ADDRESSES, MaskedBytes, Register
from freak.message import Device, lmk05318b_write

import struct
import sys

from dataclasses import dataclass
from typing import Tuple

ser = serhelper.Serial('/dev/ttyACM1')
ubx = ublox_msg.UBloxReader(ser)

# U+03C1 GREEK SMALL LETTER RHO UTF-8: 0xCF 0x81
# U+03A6 GREEK CAPITAL LETTER PHI UTF-8: 0xCE 0xA6
# r 0x72
# K 0x4b
# ΦrK is ce a6 72 4b

MAGIC=0x4b72a6ce
VERSION=1

@dataclass
class Header:
    magic: int
    version: int
    generation: int
    length: int

    def valid(self) -> bool:
        return self.magic == MAGIC and 20 <= self.length <= 2048

Headers = list[Header]

def config_address(i: int) -> int:
    return 0x0800c000 + 2048 * (i & 7) + 8192 * (i & 8)

# Get all the provisioning headers.
def get_headers(dev: Device) -> Headers:
    '''Load all the config headers from the device'''
    headers = []
    for i in range(16):
        peek = message.peek(dev, config_address(i), 16)
        headers.append(Header(*struct.unpack('<IIII', peek)))
    return headers

def best_header(dev: Device, headers: Headers) -> int|None:
    '''Find the current header to load.  Take the valid block with the
    highest generation number.'''
    best = [i for i in reversed(range(len(headers))) if headers[i].valid()]
    best.sort(key = lambda i: headers[i].generation)
    print('Best list', best)

    for i in reversed(best):
        header = headers[i]
        if message.crc(dev, config_address(i), header.length) == crc32.VERIFY_MAGIC:
            return i
    return None

CRC_EMPTY_CONFIG = 0xfe8baafc
def test_crc_empty_config() -> None:
    assert CRC_EMPTY_CONFIG == crc32.crc32(b'\xff' * 2048)

def header_is_empty(dev: Device, headers: Headers, i: int) -> bool:
    '''Check if a config is empty.  First check the header, if that's ok,
    CRC the block, and then read the entire block.'''
    h = headers[i]
    E = 0xffffffff
    if h.magic != E or h.version != E or h.generation != E or h.length != E:
        return False
    address = config_address(i)
    length = 2048
    if message.crc(dev, address, length) != CRC_EMPTY_CONFIG:
        return False
    while length > 0:
        todo = min(length, 48)
        if message.peek(dev, address, todo) != b'\xff' * todo:
            return False
        address += todo
        length -= todo
    return True

def next_header(dev: Device, headers: Headers, current: int) -> int:
    '''Get the index of the next header to write.  Erase a flash sector if
    necessary.'''
    # Prefer entries in the same sector as the current config, if it is in the
    # second bank.
    if current is not None and current >= 12:
        scan = [12, 13, 14, 15, 8, 9, 10, 11]
    else:
        scan = list(range(8, 16))
    for i in scan:
        if header_is_empty(dev, headers, i):
            return i
    index = scan[4]
    sector = config_address(index)
    assert False, f'Would erase {config_address(index):#010x}'
    message.erase_flash(dev, config_address(index))
    return index

def compare_data(dev: Device, headers: Headers, index: int|None,
                 new: bytes) -> bool:
    if index is None:
        print('No old to compare to')
        return False
    h = headers[index]
    magic, version, generation, length = struct.unpack('<IIII', new[:16])
    if magic != h.magic or version != h.version or length != h.length:
        print('Old header different')
        return False
    address = config_address(index)
    new_csum = crc32.crc32(new[16:-4])
    old_csum = message.crc(dev, address + 16, len(new) - 20)
    if new_csum != old_csum:
        print('Old checksum mismatch')
        return False
    base = 16
    end = len(new) - 4
    while base < end:
        todo = min(48, end - base)
        data = message.peek(dev, address + base, todo)
        if data != new[base : base + todo]:
            print('Difference @ {todo}')
            return False
        base += todo
    print('Config matches')
    return True

# Skip the NVM related addresses.
skip = 155, 156, 157, 158, 159, 161, 162, 164

def load_lmk05318b() -> MaskedBytes:
    # Loadup the whole damn thing...
    data = MaskedBytes()

    for a in lmk05318b.ADDRESSES:
        if a.address >= 12 and not a.address in skip:
            data.mask[a.address] = 0xff
    # Now grab the data...
    for address, length in data.ranges(max_block = 32):
        segment = message.lmk05318b_read(dev, address, length)
        print(f'@ {address} : {segment.hex(" ")}')
        assert len(segment) == length, f'{length} {segment.hex(" ")}'
        data.data[address : address+length] = segment
    assert len(data.data) == len(data.mask)
    return data

test_crc_empty_config()

dev = message.get_device()

lmk_config = load_lmk05318b()
print('Config', lmk_config.data.hex(' '))

headers = get_headers(dev)
current = best_header(dev, headers)

generation = 1 if current is None else headers[current].generation + 1
print(f'Current = {current}, next generation {generation}')

config = bytearray(struct.pack('<IIII', MAGIC, VERSION, generation, 0))

sw_reset = lmk05318b.Register.get('RESET_SW')
pll1_pdn = lmk05318b.Register.get('PLL1_PDN')
pll2_pdn = lmk05318b.Register.get('PLL2_PDN')

orig_sw_reset = lmk_config.extract(sw_reset)
orig_pll1_pdn = lmk_config.extract(pll1_pdn)
orig_pll2_pdn = lmk_config.extract(pll2_pdn)

lmk_config.insert(sw_reset, 0)
lmk_config.insert(pll1_pdn, 1)
lmk_config.insert(pll2_pdn, 1)

def set_reg(r: Register) -> None:
    lmk05318b_write(config, r.base_address, lmk_config.data[r.base_address])

set_reg(sw_reset)

for address, chunk in lmk_config.bundle(max_block = 32).items():
    print(f'@ {address} : {chunk.hex(" ")}')
    lmk05318b_write(config, address, chunk)

lmk_config.insert(sw_reset, orig_sw_reset)
lmk_config.insert(pll1_pdn, orig_pll1_pdn)
lmk_config.insert(pll2_pdn, orig_pll2_pdn)

if orig_sw_reset != 0:
    set_reg(sw_reset)
if orig_pll1_pdn != 1:
    set_reg(pll1_pdn)
if orig_pll2_pdn != 1:
    set_reg(pll2_pdn)

config[12:16] = struct.pack('<I', len(config) + 4)
config += struct.pack('>I', crc32.crc32(config))

print(len(config))
print(config.hex(' '))

assert crc32.crc32(config) == crc32.VERIFY_MAGIC
print('Best header', current)

if compare_data(dev, headers, current, config):
    print('No config changes - not saving')
    sys.exit(0)

index = next_header(dev, headers, current)
address = config_address(index)
print(f'Next header {index} @ {address:#010x}')

config += b'\xff' * (31 & -len(config))

assert len(config) % 32 == 0
#message.poke(dev, address, config)
