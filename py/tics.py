
import configparser
import freak
import struct
import re

DATA_SIZE = 500

def skip(R):
    return R < 8 or R >= 353 or R in (12, 157, 164)

BundledBytes = dict[int, bytearray]

class MaskedBytes:
    data: bytearray
    mask: bytearray
    def __init__(self):
        self.data = bytearray(DATA_SIZE)
        self.mask = bytearray(DATA_SIZE)

    def bundle(self, ro:bool = True, max_block:int = 1000,
               defaults:MaskedBytes|None = None) -> BundledBytes:
        result = {}
        current_addr = 0
        current_data = None
        for i in range(DATA_SIZE):
            data = self.data[i]
            mask = self.mask[i]
            if mask == 0:
                continue
            if mask != 255 and defaults is not None:
                data = data & mask | defaults.data[i] & ~mask
            if not ro and skip(i):
                continue # FIXME - use the data definitions instead.
            if current_data is not None \
               and i == current_addr + len(current_data) \
               and len(current_data) < max_block:
                current_data.append(data)
            else:
                current_addr = i
                current_data = bytearray((data,))
                result[current_addr] = current_data
        return result

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

def read_hex_txt_file(path: str, ro: bool = False,
                      max_block: int = 1000) -> MaskedBytes:
    result = MaskedBytes()
    for L in open(path):
        r_name, rv_hex = L.split();
        assert(r_name).startswith('R')
        reg = int(r_name[1:])
        value = int(rv_hex.strip(), 0)
        assert value >> 8 == reg;
        result.data[reg] = value & 0xff
        result.mask[reg] = 0xff
    return result

def make_i2c_transactions(reg_block_list: BundledBytes) -> list[freak.Message]:
    messages = []
    for R, B in reg_block_list.items():
        messages.append(
            freak.Message(0xc80f, struct.pack('>H', R) + bytes(B)))
    return messages

