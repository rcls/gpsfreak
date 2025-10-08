#!/usr/bin/python3

import dataclasses
import sys

from dataclasses import dataclass
from fractions import Fraction
from math import ceil, floor, gcd
from typing import Generator, NoReturn, Tuple

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

# All the frequencies are in MHz.
SCALE = 1000000
MHz = 1000000 // SCALE
assert SCALE * MHz == 1000000
BAW_FREQ = 2_500 * MHz
PLL2_LOW = 5_500 * MHz
PLL2_HIGH = 6_250 * MHz
# 1-based index of the big-divide.
BIG_DIVIDE = 3

@dataclass(order=True)
class PLL2Plan:
    # Absolute error.
    error: float = 0.0
    # Is stage 2 divider odd (only BIG_DIVIDE has stage2!)
    stage2_odd: bool = False
    # Is the denominator fixed or variable?
    apll2_den_mode: bool = False
    # The actual frequency of PLL2, in MHz
    freq: Fraction = Fraction(0)
    # The target frequency of PLL2, in MHz.
    freq_target: Fraction = Fraction(0)
    # Fpd divider between BAW and PLL2.  Currently only 18 is supported.
    fpd_divide: int = 18
    # Feedback divider value.
    multiplier: Fraction = Fraction(0)
    # Postdivider mask.
    postdiv_mask: int = 0
    # Target output frequency list.
    freqs: list[Fraction|None] = dataclasses.field(default_factory = lambda: [])

def output_divider(index: int, ratio: int) -> Tuple[int, int] | None:
    if ratio <= 256:
        return ratio, 1

    if index != BIG_DIVIDE:
        return None

    # For index 4, the two stage divider must have the fist stage in [6..=256]
    # and the second stage in [1..=(1<<24)].  Prefer an even second stage
    # divider, as this gives 50% duty cycle.  If the second stage is even,
    # keep the first stage as high as possible.  If the second stage is odd,
    # keep the second stage as high as possible to keep the duty cycle at
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

def pll2_plan_low(freqs: list[Fraction|None], freq: Fraction) -> PLL2Plan|None:
    '''Plan for the special case where we only have the BIG_DIVIDE output, and
    the stage2 divider is definitely needed.  Avoid a brute force search.'''
    assert freq < Fraction(PLL2_LOW, 7 * 256)
    assert freq == freqs[BIG_DIVIDE - 1]
    assert all(not f for i, f in enumerate(freqs, 1) if i != BIG_DIVIDE)
    # Just like TICS Pro, assume a FPD divider of 18.
    ratio = freq / Fraction(BAW_FREQ, 18)
    factors = qd_factor(ratio.denominator)
    # We only get called for frequencies well below BAW_FREQ/18!
    assert len(factors) != 0
    # We definitely can't cope with any prime factos > 1<<24.
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
                   or extra * stage2_div >= (1<<24):
                    #print(f'BAD extra = {extra}')
                    continue
                # Attempt to make the stage2 divide even...
                if stage2_div % 2 != 0 and \
                   (extra+1) * output_divide * freq <= PLL2_HIGH:
                    extra += 1
                stage2_div *= extra
                vco_freq = freq * post_div * stage1_div * stage2_div
                assert PLL2_LOW <= vco_freq <= PLL2_HIGH
                multiplier = vco_freq / Fraction(BAW_FREQ, 18)
                assert multiplier.denominator <= 1<<24
                plan = PLL2Plan(
                    error = 0.0,
                    stage2_odd = stage2_div % 2 != 0,
                    apll2_den_mode = (1<<24) % multiplier.denominator == 0,
                    freq = vco_freq,
                    freq_target = vco_freq,
                    fpd_divide = 18,
                    multiplier = multiplier,
                    postdiv_mask = postdiv_mask(post_div),
                    freqs = freqs)
                if best is None or plan < best:
                    best = plan
    return best

def pll2_plan(freqs: list[Fraction|None], pll2_lcm: Fraction) -> PLL2Plan|None:
    # Do the PLL2 planning.  Firstly, if frequency is too high, then we can't
    # do it.
    maxf = max(f for f in freqs if f)
    if maxf > Fraction(PLL2_HIGH, 2):
        fail('PLL2 frequencies too high: {maxf} {float(maxf)}')

    # Check that some multiple of the LCM is in rangle.
    if ceil(PLL2_LOW / pll2_lcm) > floor(PLL2_HIGH / pll2_lcm):
        fail(f'PLL2 needs to be a multiple of {float(pll2_lcm)} which is not in range')

    # Range to try for multipliers.
    start = ceil(PLL2_LOW / pll2_lcm)
    end = floor(PLL2_HIGH / pll2_lcm)
    # Limit the search time.  FIXME, we should be able to do better than this.
    if end > start + 20000000:
        end = start + 20000000

    best = None
    for mult in range(start, end + 1):
        pll2_freq = mult * pll2_lcm
        assert PLL2_LOW <= pll2_freq <= PLL2_HIGH
        postdivs = (1 << 64) - 1
        for i, f in enumerate(freqs, 1):
            if f is None:               # Not needed.
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
            for postdiv in range(2, 8):
                if ratio % postdiv == 0 \
                   and output_divider(i, ratio // postdiv) is not None:
                    postdivs1 |= postdiv_mask(postdiv)
            postdivs &= postdivs1
            if postdivs == 0:
                break
        if postdivs == 0:
            continue                    # Doesn't work
        # Now check the ratio with the BAW.
        # FIXME - variable predividers.  TICS Pro seems to always use /18
        # on the PLL2 pre-divider and a PLL2 denominator of 1<<24.
        mult_exact = pll2_freq / Fraction (BAW_FREQ, 18)
        mult_actual = mult_exact.limit_denominator(1 << 24)
        error = abs(float(mult_actual / mult_exact - 1))
        plan = PLL2Plan(
            error = error,
            stage2_odd = False, # FIXME
            apll2_den_mode = (1<<24) % mult_actual.denominator == 0,
            freq = Fraction(BAW_FREQ, 18) * mult_actual,
            freq_target = pll2_freq,
            fpd_divide = 18,
            multiplier = mult_actual,
            postdiv_mask = postdivs,
            freqs = freqs)

        if best is None or plan < best:
            best = plan
    return best

def plan(freqs: list[Fraction|None]):
    # First pull out the divisors of 2.5G...
    pll1: list[Fraction|None] = []
    pll2: list[Fraction|None] = []
    for i, f in enumerate(freqs, 1):
        if not f:
            pll1.append(None)
            pll2.append(None)
        elif pll1_divider(i, f):
            pll1.append(f)
            pll2.append(None)
        elif i == BIG_DIVIDE or f >= Fraction(PLL2_LOW, 7 * 256):
            pll1.append(None)
            pll2.append(f)
        else:
            fail(f'Frequency {f} {float(f)} is not achievable {i}')

    pll1_f = [float(f) for f in pll1 if f]
    pll2_f = [float(f) for f in pll2 if f]

    # Find the LCM of all the pll2 frequencies...
    first = None
    for f in pll2:
        if f:
            first = f
            break

    if not first:
        print(f'PLL1 only: {pll1_f}')
        return

    pll2_lcm = first
    for f in pll2:
        if f is not None:
            pll2_lcm, _, _ = fract_lcm(pll2_lcm, f)

    if pll2_lcm > 3 * MHz:
        pll2_p = pll2_plan(pll2, pll2_lcm)
    else:
        pll2_p = pll2_plan_low(pll2, pll2_lcm)

    if pll2_p is None:
        fail(f'PLL2 planning failed {pll2_f}')

    print(f'PLL1: {pll1_f}, PLL2: {pll2_f} error={pll2_p.error} mult=/{pll2_p.fpd_divide} *{pll2_p.multiplier}, VCO={pll2_p.freq}={float(pll2_p.freq)}', end='')
    if pll2_p.freq != pll2_p.freq_target:
        print(f' (target {float(pll2_p.freq_target)})', end='')
    print(f' postdiv {pll2_p.postdiv_mask:#x}')


if __name__ == '__main__':
    plan([Fraction(s) for s in sys.argv[1:]])
