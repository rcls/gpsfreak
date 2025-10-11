
import array

POLY = 0x04c11db7
CRCTAB = array.array('I', (0, POLY))

for i in range(1, 128):
    assert len(CRCTAB) == 2 * i
    dbl = CRCTAB[i] << 1
    dbl = min(dbl, dbl ^ POLY ^ 0x100000000)
    CRCTAB.extend((dbl, dbl ^ POLY))
assert len(CRCTAB) == 256

def crc(bb: bytes) -> int:
    result = 0xffffffff
    for b in bb:
        result = result << 8 & 0xffffff00 ^ CRCTAB[result >> 24 ^ b]
    return result ^ 0xffffffff
