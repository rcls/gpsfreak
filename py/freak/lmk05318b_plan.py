#!/usr/bin/python3

import dataclasses
import sys

from dataclasses import dataclass
from fractions import Fraction
from math import ceil, floor, gcd
from typing import Any, Generator, NoReturn, Tuple

# All the frequencies are in MHz.
SCALE = 1000000
MHz = 1000000 // SCALE
assert SCALE * MHz == 1000000
BAW_FREQ = 2_500 * MHz

kHz = Fraction(MHz) / 1000
Hz = kHz / 1000

# I've only seen TICS Pro use this, which means it's the only possibility that
# I have PLL2 filter settings for...
FPD_DIVIDE = 18
PLL2_PFD = Fraction(BAW_FREQ, FPD_DIVIDE)

# This is the official range of the LMK05318b...
OFFICIAL_PLL2_LOW = 5_500 * MHz
OFFICIAL_PLL2_HIGH = 6_250 * MHz

# We push it by 110MHz in each direction, to cover all frequencies up to
# 800MHz
PLL2_HIGH=6410 * MHz
PLL2_LOW=5340 * MHz

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
    # Target output frequency list.  Use zero for output off.
    freqs: list[Fraction]
    pll2_base: Fraction|None = None

@dataclass
class PLLPlan:
    # The actual frequency of PLL2, in MHz
    freq: Fraction = Fraction(0)
    # The target frequency of PLL2, in MHz.
    freq_target: Fraction = Fraction(0)
    # Fpd divider between BAW and PLL2.  Currently only 18 is supported.
    fpd_divide: int = FPD_DIVIDE
    # Feedback divider value for PLL2.
    multiplier: Fraction = Fraction(0)
    # Postdivider mask.
    postdiv_mask: int = 0
    # Post & output dividers by channel.  A post-divider of zero means that
    # the source is PLL1, otherwise PLL2 is used.
    dividers: list[Tuple[int, int, int]] \
        = dataclasses.field(default_factory = lambda: [])
    # Target output frequency list.  Use zero for output off.
    freqs: list[Fraction] \
        = dataclasses.field(default_factory = lambda: [])

    def __lt__(self, b: PLLPlan) -> bool:
        '''Less is better.  I.e., return True if self is better than b.'''
        # Prefer no error!
        a_error = abs(self.error_ratio())
        b_error = abs(b.error_ratio())
        if (a_error == 0) != (b_error == 0):
            return a_error == 0
        # Prefer to be in the officially supported range.
        if self.is_official() != b.is_official():
            return self.is_official()

        # Prefer smaller errors.
        if a_error != b_error:
            return a_error < b_error
        # Prefer an even stage2 divider (or one): this gives exactly 50/50
        # duty cycle.
        a_even = self.stage2_even()
        b_even = b.stage2_even()
        if a_even != b_even:
            return a_even
        # Prefer power-of-two (fixed) denomoninator.
        a_fixed = self.fixed_denom()
        b_fixed = b.fixed_denom()
        if a_fixed != b_fixed:
            return a_fixed
        # Prefer VCO2 near the middle of its range.
        a_df = abs(self.freq - PLL2_MID)
        b_df = abs(b   .freq - PLL2_MID)
        if a_df != b_df:
            return a_df < b_df

        if self.freq != b.freq:
            return self.freq < b.freq

        return False

    def validate(self):
        assert self.freq == self.multiplier * BAW_FREQ / fpd_divide

    def error_ratio(self) -> float:
        return float(self.freq / self.freq_target - 1)
    def error(self) -> Fraction:
        return self.freq - self.freq_target

    def is_official(self) -> bool:
        return OFFICIAL_PLL2_LOW <= self.freq <= OFFICIAL_PLL2_HIGH

    def fixed_denom(self) -> bool:
        return (1 << 24) % self.multiplier.denominator == 0

    def stage2_even(self) -> bool:
        if BIG_DIVIDE > len(self.dividers):
            return True
        _, _, stage2 = self.dividers[BIG_DIVIDE]
        return stage2 == 1 or stage2 % 2 == 0

def fail(*args: Any, **kwargs: Any) -> NoReturn:
    print(*args, **kwargs)
    sys.exit(1)

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
    if ratio <= 256:
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
        if ratio % first == 0 and ratio / first <= 1<<24:
            return first, ratio // first

    return None

def pll1_divider(index: int, f: Fraction) -> Tuple[int, int] | None:
    if BAW_FREQ % f.numerator == 0:
        return output_divider(index, BAW_FREQ // f)
    else:
        return None

def postdiv_mask(div: int) -> int:
    assert 2 <= div <= 7
    return 0x0101010101010101 << div | 0xfe << 8 * div

def do_factor_splitting(left: int, right: int, primes: list[int], index: int) \
        -> Generator[Tuple[int, int]]:
    '''Worker function for factor_splitting below'''
    if index >= len(primes):
        if left <= 1<<24 and right <= 1<<24:
            yield left, right
        return
    prime = primes[index]
    while True:
        yield from do_factor_splitting(left, right, primes, index + 1)
        if right % prime != 0:
            return
        left *= prime
        if left > 1<<24:
            return
        right //= prime

def factor_splitting(number: int, primes: list[int]) \
        -> Generator[Tuple[int, int]]:
    '''Return all possible factorisations of number into two factors, with
    the constraint that both are less than pow(2,24).  The list primes should
    contain at least all prime factors of number.'''
    return do_factor_splitting(1, number, primes, 0)

def pll2_plan_low1(target: FrequencyTarget, freq: Fraction,
                   post_div: int, stage1_div: int,
                   mult_den: int, stage2_div: int) -> PLLPlan | None:
    ratio = freq / PLL2_PFD
    total_divide = mult_den * post_div * stage1_div * stage2_div
    assert total_divide % ratio.denominator == 0

    output_divide = post_div * stage1_div * stage2_div

    # Now attempt to multiply stage2_div by something to get us
    # into the VCO range.
    max_extra = min((1<<24) // stage2_div,
                    floor(Fraction(PLL2_HIGH) / freq / output_divide))
    min_extra = ceil(Fraction(PLL2_LOW) / freq / output_divide)
    if min_extra > max_extra:
        return None                     # Impossible.

    extra = floor(Fraction(PLL2_MID) / freq / output_divide)
    extra = max(extra, min_extra)

    # Attempt to make the stage2 divide even...
    if stage2_div % 2 != 0 and extra % 2 != 0:
        if extra < max_extra:
            extra = extra + 1
        elif extra > min_extra:
            extra = extra - 1

    stage2_div *= extra
    vco_freq = freq * post_div * stage1_div * stage2_div
    multiplier = vco_freq / PLL2_PFD
    dividers = [(0, 0, 0)] * BIG_DIVIDE
    dividers.append((post_div, stage1_div, stage2_div))
    freqs = [Fraction(0)] * BIG_DIVIDE
    freqs.append(PLL2_PFD * multiplier / post_div / stage1_div / stage2_div)

    assert PLL2_LOW <= vco_freq <= PLL2_HIGH
    assert multiplier.denominator <= 1<<24
    assert freqs[-1] == freq

    return PLLPlan(
        freq = vco_freq,
        freq_target = vco_freq,
        fpd_divide = FPD_DIVIDE,
        multiplier = multiplier,
        postdiv_mask = postdiv_mask(post_div),
        dividers = dividers,
        freqs = freqs)

def pll2_plan_low(target: FrequencyTarget, freq: Fraction) -> PLLPlan:
    '''Plan for the special case where we only have the BIG_DIVIDE output, and
    the stage2 divider is definitely needed.  Avoid a brute force search.'''
    assert freq < Fraction(PLL2_LOW, 7 * 256)
    assert freq == target.freqs[BIG_DIVIDE]

    # Just like TICS Pro, assume a FPD divider of 18.  So this gives the overall
    # multiple of the PLL2 PFD frequency.
    ratio = freq / PLL2_PFD

    # Factorize the denominator.
    factors = qd_factor(ratio.denominator)

    # We only get called for frequencies well below BAW_FREQ/18!
    assert len(factors) != 0
    # We definitely can't cope with any prime factors > 1<<24.
    if factors[-1] >= 1<<24:
        fail("Can't acheive {freq}: denominator factor {den_fact[-1][0]} is too big")

    #print(f'freq={freq}, ratio={ratio}, factors={factors}')
    # We need to partition the denominator of the ratio over:
    # * The PLL2 multiplier denominator. (1 ..= 1<<24).
    # * The post divider (2 ..= 7)
    # * stage1 divider (6 ..= 256)
    # * stage2 divider (1 ..= 1<<24)
    # Scan over post dividers and the stage1 output divider.
    best = None
    for post_div in range(2, 7+1):
        for stage1_div in range(6, 256+1):
            # What we are left with needs to be factored into the PLL2
            # multiplier, and the stage2 divider.  Do a brute force search of
            # the denominator of that.
            bigden = ratio.denominator // gcd(ratio.denominator,
                                              post_div * stage1_div)
            if bigden > 1 << 48:
                continue                # Not acheivable.

            for mult_den, stage2_div in factor_splitting(bigden, factors):
                plan = pll2_plan_low1(target, freq,
                                      post_div, stage1_div,
                                      mult_den, stage2_div)
                if best is None or plan is not None and plan < best:
                    best = plan

    if best is not None:
        return best

    MIN = Fraction(MHz, 10)

    # Now find a multiple of pll2_freq that puts us into a sensible range for a
    # search of the VCO range.  First try multiplying by factors of the
    # frequency denominator.

    pll2_lcm = freq
    for p in reversed(factors):
        while pll2_lcm.denominator % p == 0:
            next = pll2_lcm * p
            if ceil(Fraction(OFFICIAL_PLL2_LOW) / 10 / next) > \
               floor(Fraction(OFFICIAL_PLL2_HIGH) / 10 / next):
                break
            pll2_lcm = next
            if pll2_lcm >= MIN:
                break
        if pll2_lcm >= MIN:
            break

    # Now just multiply by powers of 2 to get us over 100kHz.
    while pll2_lcm < MIN:
        pll2_lcm *= 2

    return pll2_plan(target, [Fraction(0)] * BIG_DIVIDE + [freq], pll2_lcm)

def pll2_plan1(target: FrequencyTarget, freqs: list[Fraction],
               pll2_freq: Fraction) -> PLLPlan | None:
    '''Try and create a plan using a particular PLL2 frequency.  Note that
    the frequency list might not include all the frequencies in the target.'''
    assert PLL2_LOW <= pll2_freq <= PLL2_HIGH
    postdivs = (1 << 64) - 1
    postdive = (1 << 64) - 1
    for i, f in enumerate(freqs):
        if not f:                       # Not needed.
            continue
        assert (pll2_freq / f).is_integer()
        ratio = int(pll2_freq / f)
        if ratio <= 1:
            postdivs = 0
            break                       # Impossible.

        # Now break the ratio into a post-divider and output divider.
        # Attempt to track which gives an even final stage divider, for
        # 50% duty cycle - fixme we don't get that quite right.
        postdivs1 = 0
        postdive1 = 0
        for postdiv in range(2, 8):
            if ratio % postdiv != 0:
                continue
            od = output_divider(i, ratio // postdiv)
            if od is None:
                continue
            s1, s2 = od
            postdivs1 |= postdiv_mask(postdiv)
            if s2 == 1 or s2 % 2 == 0:
                postdive |= postdiv_mask(postdiv)
        postdivs &= postdivs1
        postdive &= postdive1
        if postdivs == 0:
            break                       # Doesn't work

    # Compute the multipliers.
    mult_exact = pll2_freq / PLL2_PFD
    mult_actual = mult_exact.limit_denominator(1 << 24)
    # Compute the post-dividers.  Use the highest possible pair.
    if postdivs == 0:
        return None                     # Doesn't work
    if postdive != 0:
        postdiv_bit = postdive.bit_length() - 1
    else:
        postdiv_bit = postdivs.bit_length() - 1
    p1 = postdiv_bit >> 3 & 7
    p2 = postdiv_bit & 7
    dividers = [(0, 0, 0)] * len(freqs)
    for i, f in enumerate(freqs):
        if not f:
            continue
        ratio = round(pll2_freq / f)
        assert isinstance(ratio, int)
        od = None
        if ratio % p1 == 0:
            od = output_divider(i, ratio // p1)
        if od is not None:
            dividers[i] = p1, od[0], od[1]
        else:
            assert ratio % p2 == 0
            od = output_divider(i, ratio // p2)
            assert od is not None
            dividers[i] = p2, od[0], od[1]

    return PLLPlan(
        freq = PLL2_PFD * mult_actual,
        freq_target = pll2_freq,
        fpd_divide = FPD_DIVIDE,
        multiplier = mult_actual,
        postdiv_mask = postdivs,
        dividers = dividers,
        freqs = [f / mult_exact * mult_actual for f in freqs])

def pll2_plan(target: FrequencyTarget,
              freqs: list[Fraction], pll2_lcm: Fraction) -> PLLPlan:
    '''Create a frequency plan using PLL2 for a list of frequencies.'''
    # Firstly, if frequency is too high, then we can't do it.  Good luck
    # actually getting 3125MHz through the output drivers!
    maxf = max(f for f in freqs if f)
    if maxf > Fraction(PLL2_HIGH, 2):
        fail('Max frequency too high: {freq_to_str(maxf)}')

    # Check that some multiple of the LCM is in rangle.
    if ceil(PLL2_LOW / pll2_lcm) > floor(PLL2_HIGH / pll2_lcm):
        fail(f'PLL2 needs to be a multiple of {freq_to_str(pll2_lcm)} which is not in range')

    # Range to try for multipliers.
    start = ceil(PLL2_LOW / pll2_lcm)
    end = floor(PLL2_HIGH / pll2_lcm)
    best = None
    for mult in range(ceil(PLL2_LOW / pll2_lcm),
                      floor(PLL2_HIGH / pll2_lcm) + 1):
        pll2_freq = mult * pll2_lcm
        assert PLL2_LOW <= pll2_freq <= PLL2_HIGH
        plan = pll2_plan1(target, freqs, pll2_freq)
        if best is None or plan is not None and plan < best:
            best = plan

    if best is None:
        fail(f'PLL2 planning failed, LCM = {freq_to_str(pll2_lcm)}')
    return best

def add_pll1(target: FrequencyTarget,
             plan: PLLPlan, freqs: list[Fraction]) -> None:
    for i, f in enumerate(freqs):
        if not f:
            continue
        od = pll1_divider(i, f)
        assert od is not None
        plan.freqs[i] = f
        plan.dividers[i] = 0, od[0], od[1]

def plan(target: FrequencyTarget) -> PLLPlan:
    # First pull out the divisors of 2.5G...
    pll1: list[Fraction] = []
    pll2: list[Fraction] = []
    zero = Fraction(0)
    for i, f in enumerate(target.freqs):
        if not f:
            pll1.append(zero)
            pll2.append(zero)
        elif pll1_divider(i, f):
            pll1.append(f)
            pll2.append(zero)
        elif i == BIG_DIVIDE or f >= Fraction(PLL2_LOW, 7 * 256):
            pll1.append(zero)
            pll2.append(f)
        else:
            fail(f'Frequency {freq_to_str(f)} is not achievable on {i}')

    SMALL = Fraction(MHz, 20)

    # Find the LCM of all the pll2 frequencies...
    pll2_lcm = target.pll2_base
    assert pll2_lcm is None or pll2_lcm >= SMALL

    for f in pll2:
        if f:
            pll2_lcm = fract_lcm(pll2_lcm, f)

    if pll2_lcm is None:
        # Don't use PLL2...
        plan = PLLPlan()
        plan.freqs = [zero] * len(target.freqs)
        plan.dividers = [(0, 0, 0)] * len(target.freqs)
    # Above about 50 kHz we can brute force the ≈1GHz VCO range within a
    # reasonable time.
    elif pll2_lcm > Fraction(MHz, 20):
        plan = pll2_plan(target, pll2, pll2_lcm)
    elif target.freqs[BIG_DIVIDE]:
        assert all(not f for i, f in enumerate(pll2) if i != BIG_DIVIDE)
        plan = pll2_plan_low(target, target.freqs[BIG_DIVIDE])

    add_pll1(target, plan, pll1)
    return plan

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
