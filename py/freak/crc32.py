
import array
import struct

POLY = 0x04c11db7
CRCTAB = array.array('I', (0, POLY))
VERIFY_MAGIC = 0x38fb2284

for i in range(1, 128):
    assert len(CRCTAB) == 2 * i
    dbl = CRCTAB[i] << 1
    dbl = min(dbl, dbl ^ POLY ^ 0x100000000)
    CRCTAB.extend((dbl, dbl ^ POLY))
assert len(CRCTAB) == 256

def crc32(bb: bytes) -> int:
    result = 0xffffffff
    for b in bb:
        result = result << 8 & 0xffffff00 ^ CRCTAB[result >> 24 ^ b]
    return result ^ 0xffffffff

def test_crc():
    first = crc32(bytes())
    second = crc32(struct.pack('>I', first))
    assert second == VERIFY_MAGIC

    data = b'This is a test string 123456789'
    data += struct.pack('>I', crc(data))
    assert crc32(data) == VERIFY_MAGIC
