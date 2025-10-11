#!/usr/bin/python3

import dataclasses
import sys

from dataclasses import dataclass
from fractions import Fraction
from math import ceil, floor, gcd
from typing import Generator, NoReturn, Tuple

# All the frequencies are in MHz.
SCALE = 1000000
MHz = 1000000 // SCALE
assert SCALE * MHz == 1000000
BAW_FREQ = 2_500 * MHz
PLL2_LOW = 5_500 * MHz
PLL2_HIGH = 6_250 * MHz
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
class PLLPlan:
    # The actual frequency of PLL2, in MHz
    freq: Fraction = Fraction(0)
    # The target frequency of PLL2, in MHz.
    freq_target: Fraction = Fraction(0)
    # Fpd divider between BAW and PLL2.  Currently only 18 is supported.
    fpd_divide: int = 18
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
        a_error = abs(self.error())
        b_error = abs(b.error())
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

    def error(self) -> float:
        return float(self.freq / self.freq_target - 1)

    def fixed_denom(self) -> bool:
        return (1 << 24) % self.multiplier.denominator == 0

    def stage2_even(self) -> bool:
        if BIG_DIVIDE > len(self.dividers):
            return True
        _, _, stage2 = self.dividers[BIG_DIVIDE]
        return stage2 == 1 or stage2 % 2 == 0

def fail(*args, **kwargs) -> NoReturn:
    print(*args, **kwargs)
    sys.exit(1)

def fract_lcm(a: Fraction, b: Fraction) -> Tuple[Fraction, int, int]:
    g1 = gcd(a.denominator, b.denominator)
    g2 = gcd(a.numerator, b.numerator)
    u = (a.denominator // g1) * (b.numerator // g2)
    v = (a.numerator // g2) * (b.denominator // g1)
    assert a * u == b * v, f'{a} {b} {u} {v}'
    assert gcd(u, v) == 1
    return a * u, u, v

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

@dataclass
class PLL1Plan:
    freqs: list[Fraction] = dataclasses.field(default_factory = lambda: [])

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
    return do_factor_splitting(1, number, primes, 0)

def pll2_plan_low(freqs: list[Fraction], freq: Fraction) -> PLLPlan:
    '''Plan for the special case where we only have the BIG_DIVIDE output, and
    the stage2 divider is definitely needed.  Avoid a brute force search.'''
    assert freq < Fraction(PLL2_LOW, 7 * 256)
    assert freq == freqs[BIG_DIVIDE]
    assert all(not f for i, f in enumerate(freqs) if i != BIG_DIVIDE)
    # Just like TICS Pro, assume a FPD divider of 18.
    ratio = freq / Fraction(BAW_FREQ, 18)
    factors = qd_factor(ratio.denominator)
    # We only get called for frequencies well below BAW_FREQ/18!
    assert len(factors) != 0
    # We definitely can't cope with any prime factors > 1<<24.
    if factors[-1] >= 1<<24:
        fail("Can't acheive {freq}: denominator factor {den_fact[-1][0]} is too big")
    #print(f'freq={freq}, ratio={ratio}, factors={factors}')
    # We need to partition the factors over:
    # * The PLL2 multiplier denominator. (1 ..= 1<<24).
    # * The post divider (2 ..= 7)
    # * stage1 divider (6 ..= 256)
    # * stage2 divider (1 ..= 1<<24)
    # Scan over post dividers and the stage1 output divider.
    best = None
    for post_div in range(2, 7+1):
        for stage1_div in range(6, 256+1):
            #print(post_div, stage1_div)
            bigden = ratio.denominator
            #print(f'bigden = {bigden}')
            bigden //= gcd(bigden, post_div * stage1_div)
            #print(f'bigden = {bigden}')
            # What we are left with needs to be factored into the PLL2
            # denominator, and the stage2 divider.
            if bigden > 1<<48:
                # Not achievable.
                #print('Bigden bad')
                continue
            for mult_den, stage2_div in factor_splitting(bigden, factors):
                #print(f'Split {mult_den} {stage2_div}')
                # Ok, we have the total division.
                total_divide = mult_den * post_div * stage1_div * stage2_div
                assert total_divide % ratio.denominator == 0
                output_divide = post_div * stage1_div * stage2_div
                # Now attempt to multiply stage2_div by something to get us
                # into the VCO range.
                extra = ceil(Fraction(PLL2_LOW / freq) / output_divide)
                #print(f'extra = {extra}')
                if extra * output_divide * freq > PLL2_HIGH \
                   or extra * stage2_div > (1<<24):
                    #print(f'BAD extra = {extra}')
                    continue
                # Attempt to make the stage2 divide even...
                if stage2_div % 2 != 0 \
                   and (extra+1) * output_divide * freq <= PLL2_HIGH \
                   and (extra+1) * stage2_div <= (1<<24):
                    extra += 1
                stage2_div *= extra
                vco_freq = freq * post_div * stage1_div * stage2_div
                assert PLL2_LOW <= vco_freq <= PLL2_HIGH
                multiplier = vco_freq / Fraction(BAW_FREQ, 18)
                assert multiplier.denominator <= 1<<24
                dividers = [(0, 0, 0)] * (BIG_DIVIDE + 1)
                dividers[BIG_DIVIDE] = (post_div, stage1_div, stage2_div)
                plan = PLLPlan(
                    freq = vco_freq,
                    freq_target = vco_freq,
                    fpd_divide = 18,
                    multiplier = multiplier,
                    postdiv_mask = postdiv_mask(post_div),
                    dividers = dividers,
                    freqs = freqs)
                if best is None or plan < best:
                    best = plan
    if best is None:
        fail(f'PLL2 planning failed, frequency {freq}')
    return best

def pll2_plan(freqs: list[Fraction], pll2_lcm: Fraction) -> PLLPlan:
    # Firstly, if frequency is too high, then we can't do it.  Good luck
    # actually getting 3125MHz through the output drivers!
    maxf = max(f for f in freqs if f)
    if maxf > Fraction(PLL2_HIGH, 2):
        fail('Max frequency too high: {maxf} {float(maxf)}')

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
        postdivs = (1 << 64) - 1
        postdive = (1 << 64) - 1
        for i, f in enumerate(freqs):
            if not f:                   # Not needed.
                continue
            assert (pll2_freq / f).is_integer()
            ratio = int(pll2_freq / f)
            # FIXME - we only have two post-dividers!
            if ratio <= 1:
                postdivs = 0
                break
            # Now break the ratio into a post-divider and output divider.
            # FIXME - we should track which give a good stage2!
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
                break                   # Doesn't work
        # Compute the multipliers.
        mult_exact = pll2_freq / Fraction (BAW_FREQ, 18)
        mult_actual = mult_exact.limit_denominator(1 << 24)
        # Compute the post-dividers.  Use the highest possible pair.
        if postdivs == 0:
            continue                    # Doesn't work
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
            ratio = int(pll2_freq / f)
            od = None
            if ratio % p1 == 0:
                od = output_divider(i, ratio // p1)
            if od == None:
                od = output_divider(i, ratio // p2)
                assert od is not None
            dividers[i] = p2, od[0], od[1]
        # TODO - does it matter which is which?
        plan = PLLPlan(
            freq = Fraction(BAW_FREQ, 18) * mult_actual,
            freq_target = pll2_freq,
            fpd_divide = 18,
            multiplier = mult_actual,
            postdiv_mask = postdivs,
            dividers = dividers,
            freqs = freqs)

        if best is None or plan < best:
            best = plan
    if best is None:
        fail(f'PLL2 planning failed, LCM = {pll2_lcm}')
    return best

def add_pll1(plan: PLLPlan, freqs: list[Fraction]) -> None:
    for i, f in enumerate(freqs):
        if not f:
            continue
        od = pll1_divider(i, f)
        assert od is not None
        plan.freqs[i] = f
        plan.dividers[i] = 0, od[0], od[1]

def plan(freqs: list[Fraction]) -> PLLPlan:
    # First pull out the divisors of 2.5G...
    pll1: list[Fraction] = []
    pll2: list[Fraction] = []
    zero = Fraction(0)
    for i, f in enumerate(freqs):
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
            fail(f'Frequency {f} {float(f)} is not achievable on {i}')

    # Find the LCM of all the pll2 frequencies...
    pll2_lcm = None
    for f in pll2:
        if not f:
            pass
        elif pll2_lcm is None:
            pll2_lcm = f
        else:
            pll2_lcm, _, _ = fract_lcm(pll2_lcm, f)

    if pll2_lcm is None:
        plan = PLLPlan()
        plan.freqs = [zero] * len(freqs)
        plan.dividers = [(0, 0, 0)] * len(freqs)
        add_pll1(plan, pll1)
        return plan

    if pll2_lcm > 3 * MHz:
        plan = pll2_plan(pll2, pll2_lcm)
    else:
        plan = pll2_plan_low(pll2, pll2_lcm)

    add_pll1(plan, pll1)
    return plan

def str_to_freq(s: str) -> Fraction:
    s = s.lower()
    for suffix, scale in ('hz', 1), ('khz', 1000), \
            ('mhz', 1000_000), ('ghz', 1000_000_000):
        if s.endswith(suffix):
            break
        if suffix != 'hz' and s.endswith(suffix[0]):
            suffix = suffix[0]
            break
    else:
        suffix = ''
        scale = 1000000

    return Fraction(s.removesuffix(suffix)) * scale / (1000000 * MHz)

def freq_to_str(freq: Fraction|int|float) -> str:
    if freq >= 1000000 * MHz:
        scaled = float(freq / (MHz * 1000000))
        suffix = 'THz'
    elif freq >= 10_000 * MHz: # Report VCO frequencies in MHz.
        scaled = float(freq / (MHz * 1000))
        suffix = 'GHz'
    elif freq >= MHz:
        scaled = float(freq / MHz)
        suffix = 'MHz'
    elif freq * 1000 >= MHz:
        scaled = float(freq * 1000 / MHz)
        suffix = 'kHz'
    else:
        scaled = float(freq * 1000000 / MHz)
        suffix = 'Hz'

    if scaled == int(scaled):
        return f'{int(scaled)} {suffix}'
    else:
        return f'{scaled} {suffix}'
