
from dataclasses import dataclass
from fractions import Fraction
from math import gcd

from typing import Tuple

# All the frequencies are in MHz.
SCALE = 1000000
MHz = 1000000 // SCALE
assert SCALE * MHz == 1000000
BAW_FREQ = 2_500 * MHz

kHz = Fraction(MHz) / 1000
Hz = kHz / 1000

REF_FREQ: Fraction = 8844582 * Hz

# I've only seen TICS Pro use this, which means it's the only possibility that
# I have PLL2 filter settings for...
FPD_DIVIDE = 18
PLL2_PFD = Fraction(BAW_FREQ, FPD_DIVIDE)

# This is the official range of the LMK05318b...
OFFICIAL_PLL2_LOW = 5_500 * MHz
OFFICIAL_PLL2_HIGH = 6_250 * MHz

# PLL1 frequency range.
BAW_LOW  = 2500 * MHz * Fraction(1000000 - 100, 1000000)
BAW_HIGH = 2500 * MHz * Fraction(1000000 + 100, 1000000)

# We push it by 110MHz in each direction, to cover all frequencies up to
# 800MHz
PLL2_LOW=5340 * MHz
PLL2_HIGH=6410 * MHz

PLL2_MID = (PLL2_LOW + PLL2_HIGH) // 2
# Our numbering of channels:
# 0 = LMK 0,1, GPS Freak 2
# 1 = LMK 2,3, GPS Freak 1
# 2 = LMK 4.
# 3 = LMK 5, GPS Freak U.Fl
# 4 = LMK 6, GPS Freak 4
# 5 = LMK 7, GPS Freak 3, can do 1Hz.
# Index of the output with the stage2 divider.
BIG_DIVIDE = 5

@dataclass
class FrequencyTarget:
    '''Target output frequency list.  Use a frequence of zero for output off.

    pll2_base allows you to constain the PLL2 frequency to be (a multiple of)
    the specified value.'''
    freqs: list[Fraction]
    pll2_base: Fraction|None = None

def fract_lcm(a: Fraction|None, b: Fraction|None) -> Fraction|None:
    if a is None:
        return b
    if b is None:
        return a

    g1 = gcd(a.denominator, b.denominator)
    g2 = gcd(a.numerator, b.numerator)
    u = (a.denominator // g1) * (b.numerator // g2)
    v = (a.numerator // g2) * (b.denominator // g1)
    assert a * u == b * v, f'{a} {b} {u} {v}'
    assert gcd(u, v) == 1
    return a * u

def test_fract_lcm():
    def mf(u):
        x, y = u
        return Fraction(x, y)
    L2 = list(map(mf, [(1,8), (1,4), (1,2), (1,1), (2,1), (4,1), (8,1)]))
    L3 = list(map(mf, [(1,27), (1,9), (1,3), (1,1), (3,1), (9,1), (27,1)]))
    L5 = list(map(mf, [(1,25), (1,5), (1,1), (5,1), (25,1)]))
    L7 = list(map(mf, [(1,49), (1,7), (1,1), (7,1), (49,1)]))

    fracts = []
    for a2 in L2:
        for a3 in L3:
            for a5 in L5:
                for a7 in L7:
                    fracts.append(a2 * a3 * a5 * a7)
    for a in fracts:
        for b in fracts:
            # We rely on the asserts in fract_lcm to actually test!
            fract_lcm(a, b)

def qd_factor(n: int) -> list[int]:
    '''Quick and dirty prime factorisation'''
    assert n > 0
    factors = []
    factor = 2
    while factor * factor <= n:
        if n % factor == 0:
            factors.append(factor)
            n //= factor
            while n % factor == 0:
                n //= factor
        factor = (factor + 1) | 1
    if n > 1:
        factors.append(n)
    return factors

def output_divider(index: int, ratio: int) -> Tuple[int, int] | None:
    if 2 <= ratio <= 256:
        return ratio, 1

    if index != BIG_DIVIDE:
        return None

    # For index 4, the two stage divider must have the fist stage in [6..=256]
    # and the second stage in [1..=(1<<24)].  Prefer an even second stage
    # divider, as this gives 50% duty cycle.  If the second stage is even,
    # keep the first stage as high as possible.  If the second stage is odd,
    # keep the second stage as high as possible to keep the duty cycle near
    # 50%.

    # Try even second stage.
    for first in range(512, 11, -2):
        if ratio % first == 0 and ratio // first <= 1<<23:
            return first // 2, ratio * 2 // first

    # Try any second stage.
    for first in range(6, 257):
        if ratio % first == 0 and ratio // first <= 1<<24:
            return first, ratio // first

    return None

def pll1_divider(index: int, f: Fraction) -> Tuple[int, int] | None:
    if BAW_FREQ % f.numerator == 0:
        return output_divider(index, BAW_FREQ // f)
    else:
        return None

def str_to_freq(s: str) -> Fraction:
    s = s.lower()
    for suffix, scale in ('khz', 1000), ('mhz', 1000_000), \
            ('ghz', 1000_000_000), ('hz', 1):
        if s.endswith(suffix):
            break
        if suffix != 'hz' and s.endswith(suffix[0]):
            suffix = suffix[0]
            break
    else:
        suffix = ''
        scale = 1000000

    return Fraction(s.removesuffix(suffix)) * scale / (1000000 * MHz)

# Set the name of str_to_freq to give sensible argparse help test.
str_to_freq.__name__ = 'frequency'

FRACTIONS = {
    Fraction(0): '',
    Fraction(1, 3): '⅓',
    Fraction(2, 3): '⅔',
    Fraction(1, 6): '⅙',
    Fraction(5, 6): '⅚',
    Fraction(1, 7): '⅐',
    Fraction(1, 9): '⅑',
}

def freq_to_str(freq: Fraction|int|float, precision: int = 0) -> str:
    if freq >= 1000000 * MHz:
        scaled = freq / (Fraction(MHz) * 1000000)
        suffix = 'THz'
    elif freq >= 10_000 * MHz: # Report VCO frequencies in MHz.
        scaled = freq / (Fraction(MHz) * 1000)
        suffix = 'GHz'
    elif freq >= MHz:
        scaled = freq / Fraction(MHz)
        suffix = 'MHz'
    elif freq >= kHz:
        scaled = freq / kHz
        suffix = 'kHz'
    else:
        scaled = freq / Hz
        suffix = 'Hz'

    fract = scaled % 1
    fract_str = None
    if not isinstance(fract, float) and fract in FRACTIONS:
        fract_str = FRACTIONS[fract]

    elif isinstance(fract, Fraction) and (
            fract.denominator in (6, 7, 9) or 11 <= fract.denominator <= 19):
        fract_str = f'+' + str(fract)

    if fract_str is not None:
        return f'{int(scaled)}{fract_str} {suffix}'
    elif precision == 0:
        return f'{float(scaled)} {suffix}'
    else:
        return f'{float(scaled):.{precision}g} {suffix}'
