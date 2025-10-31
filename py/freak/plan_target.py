
from dataclasses import dataclass
from fractions import Fraction
from math import gcd

from .plan_tools import is_multiple_of

from typing import Tuple

# All the frequencies are in MHz.
MHz = Fraction(1)
kHz = MHz / 1000
Hz = kHz / 1000

REF_FREQ = 8844582 * Hz

# I've only seen TICS Pro use this, which means it's the only possibility that
# I have PLL2 filter settings for...
FPD_DIVIDE = 18

# PLL1 frequency range.  ±50ppm
BAW_FREQ = 2_500 * MHz
BAW_LOW  = BAW_FREQ - BAW_FREQ * 50 / 1000000
BAW_HIGH = BAW_FREQ + BAW_FREQ * 50 / 1000000

# This is the official range of the LMK05318b...
OFFICIAL_PLL2_LOW = 5_500 * MHz
OFFICIAL_PLL2_HIGH = 6_250 * MHz

# We push it by 110MHz in each direction, to cover all frequencies up to
# 800MHz
PLL2_LOW = 5340 * MHz
PLL2_HIGH = 6410 * MHz

# Small frequencies...
SMALL = 50 * kHz

PLL2_MID = (PLL2_LOW + PLL2_HIGH) / 2
# Clamp the length of a PLL2 brute force search to ±MAX_HALF_RANGE attempts
# around the mid-point.  This is ±10700 (i.e., 21401 total).
MAX_HALF_RANGE = (PLL2_HIGH - PLL2_LOW) // 2 // SMALL

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

    pll{1|2}_base allows you to constain the PLL{1|2} frequency to be (a
    multiple of) the specified value.'''
    freqs: list[Fraction]
    pll1_base: Fraction|None = None
    pll2_base: Fraction|None = None

    def force_pll2(self, freq: Fraction) -> bool:
        if not self.pll2_base:
            return False
        return is_multiple_of(self.pll2_base, freq)

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
        scaled = freq / (MHz * 1000000)
        suffix = 'THz'
    elif freq >= 10_000 * MHz: # Report VCO frequencies in MHz.
        scaled = freq / (MHz * 1000)
        suffix = 'GHz'
    elif freq >= MHz:
        scaled = freq / MHz
        suffix = 'MHz'
    elif freq >= kHz:
        scaled = freq / kHz
        suffix = 'kHz'
    else:
        scaled = freq / Hz
        suffix = 'Hz'

    rounded = round(scaled)
    fract = scaled % 1
    fract_str = None
    if not isinstance(fract, float) and fract in FRACTIONS:
        fract_str = FRACTIONS[fract]

    elif isinstance(fract, Fraction) and (
            fract.denominator in (6, 7, 9) or 11 <= fract.denominator <= 19):
        fract_str = f'+' + str(fract)
    elif isinstance(scaled, Fraction) and rounded != scaled and rounded != 0 \
         and abs(rounded - scaled) < 1e-5:
        if rounded < scaled:
            fract_str = f' + {float(scaled - rounded):.6g}'
        else:
            fract_str = f' - {float(rounded - scaled):.6g}'
        scaled = rounded

    if fract_str is not None:
        return f'{int(scaled)}{fract_str} {suffix}'
    elif precision == 0:
        return f'{float(scaled)} {suffix}'
    else:
        return f'{float(scaled):.{precision}g} {suffix}'

def fraction_to_str(f: Fraction, paren: bool = True) -> str:
    if f.is_integer() or f < 1:
        return str(f)
    d = f.denominator
    i = f.numerator // d
    n = f.numerator % d
    if paren:
        return f'({i} + {n}/{d})'
    else:
        return f'{i} + {n}/{d}'
