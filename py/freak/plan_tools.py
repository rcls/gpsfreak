
from .plan_constants import BIG_DIVIDE, Hz, MHz, REF_FREQ, kHz, XO_FREQ

from dataclasses import dataclass
from fractions import Fraction
from math import ceil, gcd
from typing import Generator, NoReturn, Tuple

class PlanningFailed(RuntimeError):
    pass

@dataclass
class Target:
    '''Target output frequency list.  Use a frequency of zero for output off.

    pll{1|2}_base allows you to constrain the PLL{1|2} frequency to be (a
    multiple of) the specified value.'''
    freqs: list[Fraction]
    pll1_pfd : Fraction = 2 * XO_FREQ   # Frequency at the PLL1 PFD.
    reference: Fraction = REF_FREQ
    pll1_base: Fraction|None = None
    pll2_base: Fraction|None = None

    def force_pll2(self, freq: Fraction) -> bool:
        if not self.pll2_base:
            return False
        return is_multiple_of(self.pll2_base, freq)

def fail(why: str) -> NoReturn:
    raise PlanningFailed(why)

def is_multiple_of(a: Fraction, b: Fraction) -> bool:
    if not b:
        return False
    return a.numerator % b.numerator == 0 and \
        b.denominator % a.denominator == 0

def do_factor_splitting(left: int, right: int, maxL: int, maxR: int, \
                        primes: list[int], index: int) \
        -> Generator[Tuple[int, int]]:
    '''Worker function for factor_splitting below'''
    if index >= len(primes):
        if left <= maxL and right <= maxR:
            yield left, right
        return
    prime = primes[index]
    while True:
        yield from do_factor_splitting(
            left, right, maxL, maxR, primes, index + 1)
        if right % prime != 0:
            return
        left *= prime
        if left > maxL:
            return
        right //= prime

def factor_splitting(number: int, primes: list[int], maxL: int, maxR: int) \
        -> Generator[Tuple[int, int]]:
    '''Return all possible factorisations of number into two factors, with the
    constraint that both are less than maxL or maxR.  The list primes should
    contain at least all prime factors of number.'''
    # It's more efficient to put the smaller maximum first.
    if maxL <= maxR:
        yield from do_factor_splitting(1, number, maxL, maxR, primes, 0)
    else:
        for a, b in do_factor_splitting(1, number, maxR, maxL, primes, 0):
            yield b, a

def fract_lcm(a: Fraction | None, b: Fraction | None) -> Fraction | None:
    if a is None:
        return b
    if b is None:
        return a

    u = a.denominator * b.numerator
    v = a.numerator * b.denominator
    g = gcd(u, v)
    au = u // g * a
    assert au == v // g * b, f'{a} {b} {u} {v}'
    return au

def test_fract_lcm():
    L2 = list(map(Fraction, '1/8 1/4 1/2 1 2 4 8'.split()))   # pyrefly: ignore
    L3 = list(map(Fraction, '1/27 1/9 1/3 1 3 9 27'.split())) # pyrefly: ignore
    L5 = list(map(Fraction, '1/25 1/5 1 5 25'.split()))       # pyrefly: ignore
    L7 = list(map(Fraction, '1/49 1/7 1 7 49'.split()))       # pyrefly: ignore

    # 7 * 7 * 5 * 5 = 1225
    fracts: list[Fraction] = []
    for a2 in L2:
        for a3 in L3:
            for a5 in L5:
                for a7 in L7:
                    fracts.append(a2 * a3 * a5 * a7)
    # ≈1.4 million checks.
    for a in fracts:
        for b in fracts:
            # We rely on the asserts in fract_lcm to actually test!
            fract_lcm(a, b)

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

    base = max(6, ceil(Fraction(ratio, 1 << 24)))
    result = None
    for first in range(256, base - 1, -1):
        if ratio % first == 0:
            second = ratio // first
            result = first, second
            if second & 1 == 0:
                return result           # If its even, prefer it.

    return result                       # We may have an odd stage2.

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

def freq_to_str(freq: Fraction, precision: int = 0) -> str:
    if freq >= 1000_000 * MHz:
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
    fract = Fraction(scaled % 1)
    fract_str = None
    if fract in FRACTIONS:
        fract_str = FRACTIONS[fract]

    elif fract.denominator in (6, 7, 9) or 11 <= fract.denominator <= 19:
        fract_str = f'+{fract}'

    elif rounded != scaled and rounded != 0 and abs(rounded - scaled) < 1e-5:
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
    if f.denominator == 1 or f < 1:
        return str(f)
    d = f.denominator
    i = f.numerator // d
    n = f.numerator % d
    if paren:
        return f'({i} + {n}/{d})'
    else:
        return f'{i} + {n}/{d}'

def test_factor_split():
    f = list(factor_splitting(12, [2, 3], 20, 20))
    f.sort()
    assert f == [(1,12), (2,6), (3,4), (4,3), (6,2), (12,1)]
