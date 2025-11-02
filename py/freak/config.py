
from . import crc32, lmk05318b, message, ublox_defs, ublox_msg

from .freak_util import Device
from .lmk05318b import MaskedBytes, Register
from .message import lmk05318b_read, lmk05318b_write
from .ublox_cfg import UBloxCfg
from .ublox_msg import UBloxReader
from .ublox_defs import get_config_changes

import argparse, struct

from collections.abc import ByteString
from dataclasses import dataclass
from typing import Any, Generator, Tuple
from usb.core import Device as USBDevice

# U+03A6 GREEK CAPITAL LETTER PHI UTF-8: 0xCE 0xA6
# r 0x72
# K 0x4b
# ΦrK UTF-8 is CE A6 72 4B
'''Magic number for the config structure.'''
MAGIC = 0x4b72a6ce

'''Configuration format version.'''
VERSION = 1

# For the LMK05318b, we skip some feedback and the NVM related addresses.
SKIP = list(range(12))         # Not writeable.
SKIP += 13, 14, 17, 18, 19, 20 # LOL flags and their interrupts.
SKIP += 123, 124, 125, 126, 127 # PLL1 volatile.
SKIP += 155, 156, 157, 158, 159, 161, 162, 164 # NVM.
SKIP += 168, # DPLL status.

SKIP_ABOVE = 352

# Addresses of the configs in flash.
ADDRESSES = list(range(0x0800c000, 0x08010000, 2048)) + \
    list(range(0x0801c000, 0x08020000, 2048))
assert len(ADDRESSES) == 16

@dataclass
class Config:
    address: int
    magic: int
    version: int
    generation: int
    length: int
    content: bytes|None = None

    def is_valid(self) -> bool:
        return self.magic == MAGIC and 20 <= self.length <= 2048

    def fetch(self, dev: USBDevice) -> bytes:
        if self.content is not None:
            return self.content
        self.content = message.peek(dev, self.address, self.length)
        return self.content

Configs = list[Config]

def load_lmk05318b(dev: USBDevice) -> MaskedBytes:
    data = MaskedBytes()

    for a in lmk05318b.ADDRESSES:
        if a.address < SKIP_ABOVE and not a.address in SKIP:
            data.mask[a.address] = 0xff
    # Now grab the data...
    for address, length in data.ranges(max_block = 32):
        segment = lmk05318b_read(dev, address, length)
        #print(f'@ {address} : {segment.hex(" ")}')
        assert len(segment) == length, f'{length} {segment.hex(" ")}'
        data.data[address : address+length] = segment
    assert len(data.data) == len(data.mask)
    return data

# Get all the provisioning headers.
def get_headers(dev: USBDevice) -> Configs:
    '''Load all the config headers from the device'''
    headers = []
    for address in ADDRESSES:
        peek = message.peek(dev, address, 16)
        headers.append(Config(address, *struct.unpack('<IIII', peek)))
    return headers

def active_header(dev: USBDevice, headers: Configs) -> Config|None:
    '''Find the current header to load.  Take the valid block with the
    highest generation number.'''
    best = [h for h in headers if h.is_valid()]
    best.sort(key = lambda h: (h.generation, h.address))
    #print('Best list', best)

    # The active header should be last in the list.  Before we return it, verify
    # the checksum.
    for h in reversed(best):
        if message.crc(dev, h.address, h.length) == crc32.VERIFY_MAGIC:
            return h

    return None

CRC_EMPTY_CONFIG = 0xfe8baafc
def test_crc_empty_config() -> None:
    assert CRC_EMPTY_CONFIG == crc32.crc32(b'\xff' * 2048)

def config_is_empty(dev: USBDevice, h: Config) -> bool:
    '''Check if a config is empty.  First check the header, if that's ok, CRC
    the block, and then read the entire block.'''
    E = 0xffffffff
    return h.magic == E and h.version == E and \
        h.generation == E and h.length == E \
        and message.crc(dev, h.address, 2048) == CRC_EMPTY_CONFIG \
        and message.peek(dev, h.address, 2048) == b'\xff' * 2048

def next_header(dev: USBDevice, headers: Configs,
                current: Config|None) -> Config:
    '''Get the index of the next header to write.  Erase a flash sector if
    necessary.'''
    # Prefer entries in the same sector as the current config, if it is in the
    # second bank.
    if current is not None and current.address >= 0x0801e000:
        scan = [12, 13, 14, 15, 8, 9, 10, 11]
    else:
        scan = list(range(8, 16))

    test_crc_empty_config()
    for i in scan:
        h = headers[i]
        if config_is_empty(dev, h):
            return h

    erase_base = headers[scan[4]]
    assert erase_base.address in (0x0801c000, 0x0801e000)
    #assert False, f'Would erase {erase_base.address:#010x}'
    message.flash_erase(dev, erase_base.address)
    return erase_base

def compare_config(dev: USBDevice, h: Config, new: ByteString) -> bool:
    magic, version, generation, length = struct.unpack('<IIII', new[:16])
    if magic != h.magic or version != h.version or length != h.length:
        #print('Old header different', h)
        return False
    # We can't just compare the checksums in the header, because that covers
    # the generation, and those will not match.
    assert crc32.crc32(new) == crc32.VERIFY_MAGIC
    new_csum = crc32.crc32(new[16:-4])
    old_csum = message.crc(dev, h.address + 16, h.length - 20)
    if new_csum != old_csum:
        #print('Old checksum mismatch')
        return False

    old_data = h.fetch(dev)
    if old_data[16:-4] == new[16:-4]:
        #print('Config matches')
        return True
    else:
        #print('Old config differs')
        return False

def add_live_lmk05318b(dev: USBDevice, b: bytearray) -> None:
    lmk_config = load_lmk05318b(dev)

    orig_reset_sw = lmk_config.RESET_SW

    lmk_config.RESET_SW = 1

    # Send the RESET_SW first.
    lmk05318b_write(b, 12, lmk_config.data[12])

    for address, chunk in lmk_config.bundle(max_block = 32).items():
        #print(f'@ {address} : {chunk.hex(" ")}')
        lmk05318b_write(b, address, chunk)

    # Clear RESET_SW.
    if orig_reset_sw != 1:
        lmk_config.RESET_SW = orig_reset_sw
        lmk05318b_write(b, 12, lmk_config.data[12])

def set_ubx(b: bytearray, kv: list[Tuple[UBloxCfg, Any]]) -> None:
    # We could be smarter about the chunking...
    for base in range(0, len(kv), 8):
        payload = bytearray(b'\x00\x01\x00\x00')
        for cfg, val in kv[base : min(base + 8, len(kv))]:
            #print(cfg, val)
            payload += cfg.encode_key_value(val)
        msg = ublox_msg.UBloxMsg.get('CFG-VALSET')
        b.extend(msg.frame_payload(payload))

def add_live_baud_rate(ubx: UBloxReader, b: bytearray) -> None:
    # Load the GPS config.  First, the baud rates...
    cfg_baud = UBloxCfg.get('UART1-BAUDRATE')
    (_, baud_rom), = ublox_defs.get_config(ubx, 7, [cfg_baud])
    (_, baud_now), = ublox_defs.get_config(ubx, 0, [cfg_baud])

    message.set_baud(b, baud_rom)
    message.serial_sync(b, 100000)
    #message.serial_sync(b, 10000)
    if baud_now != baud_rom:
        set_ubx(b, [(cfg_baud, baud_now)])
        message.serial_sync(b, 10000)
        message.set_baud(b, baud_now)
        message.serial_sync(b, 100000)

def add_live_ublox(ubx: UBloxReader, b: bytearray) -> None:
    cfg_baud = UBloxCfg.get('UART1-BAUDRATE')
    changes = [(key, now) for key, now, _ in get_config_changes(ubx)
               if key != cfg_baud]
    set_ubx(b, changes)

def parse_config(dev: USBDevice, h: Config | None) \
        -> Generator[Tuple[str, bytes]]:
    if h is None:
        return
    assert h.is_valid()
    data = h.fetch(dev)
    done = 16
    end = h.length - 4
    while done < end:
        first = data[done]
        # UBlox magic is B5 62
        if first == 0xb5:
            # Looks like a UBX message.
            assert end - done >= 8, 'Too short'
            assert data[done + 1] == 0x62, 'Wrong magic'
            length = data[done + 4] + data[done + 5] * 256
            total = length + 8
            assert total <= end - done, f'{total}; {done} .. {end}'
            msg = data[done : done + total]
            done += total
            ckA, ckB = ublox_msg.checksum(msg[2:-2])
            #print(msg.hex(' '))
            assert ckA == msg[-2]
            assert ckB == msg[-1]
            yield 'UBX', msg
            continue
        # Freak magic is CE 93
        assert first == 0xce, 'Unknown message'
        assert end - done >= 8
        assert data[done + 1] == 0x93
        length = data[done + 3]
        total = 4 + length + 2
        assert total <= end - done
        msg = data[done : done + total]
        done += total
        assert message.crc16(msg) == 0
        if msg[2] == message.LMK05318B_WRITE:
            yield 'LMK', msg
        elif msg[2] in (message.GET_SET_BAUD, message.SERIAL_SYNC):
            yield 'UBX', msg
        else:
            yield 'UNKNOWN', msg

def make_config(device: Device, headers: Configs, active: Config | None,
                save_ubx: bool, save_lmk: bool, force: bool) -> bytearray|None:
    dev = device.get_usb()
    ubx = device.get_ublox()

    generation = 1 if active is None else active.generation + 1
    #print(f'Active = {active}, next generation {generation}')

    config = bytearray(struct.pack('<IIII', MAGIC, VERSION, generation, 0))

    if save_lmk:
        print('Add LMK05318b configuration.')
        add_live_lmk05318b(dev, config)
    elif active is None:
        print('(No saved config to conserve.)')
    else:
        print('Preserve LMK05318b configuration.')
        for typ, msg in parse_config(dev, active):
            if typ == 'LMK':
                config += msg
    if save_ubx:
        print('Add UBlox GPS configuration.')
        add_live_baud_rate(ubx, config)
        add_live_ublox(ubx, config)
    elif active is None:
        print('(No saved config to conserve.)')
    else:
        print('Preserve UBlox GPS configuration.')
        assert active is not None, 'No active config to preserve'
        for typ, msg in parse_config(dev, active):
            if typ == 'UBX':
                config += msg

    if active is not None:
        unknown = 0
        for typ, msg in parse_config(dev, active):
            if not typ in ('LMK', 'UBX'):
                config += msg
                unknown += 1
        if unknown != 0:
            print(f'Note : preserving {unknown} unexpected config messages.')

    config[12:16] = struct.pack('<I', len(config) + 4)
    config += struct.pack('>I', crc32.crc32(config))
    assert crc32.crc32(config) == crc32.VERIFY_MAGIC

    if active is not None and not force:
        print('Compare with saved configuration.')
        if compare_config(dev, active, config):
            #print('No changes - not saving')
            return None

    config += b'\xff' * (31 & -len(config))

    #print('Changed!')
    return config

def write_config(dev: USBDevice, headers: Configs, active: Config | None,
                 config: ByteString) -> None:
    where = next_header(dev, headers, active)
    message.poke(dev, where.address, config)

def save_config(device: Device, save_ubx: bool, save_lmk: bool,
                dry_run: bool = False) -> bool:
    dev = device.get_usb()
    #ubx = device.get_ublox()
    print('Retrieving saved configuration state.')
    headers = get_headers(dev)
    active = active_header(dev, headers)
    cfg = make_config(device, headers, active, save_ubx, save_lmk, False)
    if cfg is None:
        print('No config changes.  Not writing to device.')
        return False
    if dry_run:
        print('Dry run, not writing config')
    else:
        print('Writing config to flash')
        write_config(dev, headers, active, cfg)
    return True

def add_to_argparse(argp: argparse.ArgumentParser,
                    dest: str = 'command', metavar: str = 'COMMAND') -> None:
    subp = argp.add_subparsers(dest=dest, metavar=metavar,
                               required=True, help='Sub-command')
    save = subp.add_parser(
        'save', help='Save all device config to flash',
        description='''Save full device configuration to flash.  This saves
        changes both to the clock generator and GPS configuration.''')
    save.add_argument('-n', '--dry-run', action='store_true', default=False,
                      help="Don't actually write to flash.")

def run_command(args: argparse.Namespace, device: Device, command: str) -> None:
    if command == 'save':
        save_config(device, True, True, args.dry_run)

    else:
        assert False, f'This should never happen: {command}'
