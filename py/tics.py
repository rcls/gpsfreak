
import configparser
import freak
import struct
import re

from lmk05318b import BundledBytes, MaskedBytes

def read_tcs_file(path: str) -> MaskedBytes:
    config = configparser.ConfigParser(strict=False)
    result = MaskedBytes()
    fs = config.read((path,))
    assert len(fs) != 0
    NAME_RE = re.compile(r'name\d+$')
    REG_ADDR_RE = re.compile(r'R\d+$')
    modes = config['MODES']
    for name, reg_addr_s in modes.items():
        if not NAME_RE.match(name):
            continue
        assert REG_ADDR_RE.match(reg_addr_s)
        reg_addr = int(reg_addr_s[1:])
        reg_value = int(modes['value' + name.removeprefix('name')])
        assert reg_value >> 8 == reg_addr

        result.data[reg_addr] = reg_value & 255
        result.mask[reg_addr] = 255
    return result

def make_i2c_transactions(reg_block_list: BundledBytes) -> list[freak.Message]:
    messages = []
    for R, B in reg_block_list.items():
        messages.append(
            freak.Message(0xc80f, struct.pack('>H', R) + bytes(B)))
    return messages
