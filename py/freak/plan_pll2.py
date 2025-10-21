
from .plan_target import *
from .plan_tools import factor_splitting, fail, fract_lcm, qd_factor

import dataclasses

from dataclasses import dataclass
from fractions import Fraction
from math import ceil, floor
from typing import Tuple

__all__ = 'PLLPlan', 'fail', 'pll2_plan', 'pll2_plan_low'

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
        # duty cycle.  FIXME - do evens on all dividers...
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

def postdiv_mask(div: int) -> int:
    assert 2 <= div <= 7
    return 0x0101010101010101 << div | 0xfe << 8 * div

def pll2_plan_low1(target: FrequencyTarget, freq: Fraction,
                   post_div: int, stage1_div: int,
                   mult_den: int, stage2_div: int) -> PLLPlan | None:
    '''Try and create a PLL2 plan for a single output using the given data.
    We multiply stage2_div to get the VCO frequency in the supported range.'''
    ratio = freq / PLL2_PFD
    total_divide = mult_den * post_div * stage1_div * stage2_div
    assert total_divide % ratio.denominator == 0

    output_divide = post_div * stage1_div * stage2_div

    # Now attempt to multiply stage2_div by something to get us
    # into the VCO range.
    max_extra = min((1<<24) // stage2_div, PLL2_HIGH // freq // output_divide)
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

def pll2_plan_low_exact(target: FrequencyTarget, freq: Fraction, fast: bool,
                        ratio: Fraction, factors: list[int]) -> PLLPlan | None:
    '''Search for a PLL2 plan generating the given frequency.

    ratio is the overall PDF-to-output multiplier.  factors should contain all
    the prime factors of ratio.denominator.  fast enables a heuristic that
    almost always succeeds and that slashes the run-time.'''

    # We definitely can't cope with any prime factors > 1<<24.
    if factors[-1] >= 1<<24:
        print(f"Can't acheive {freq} exactly: denominator factor {factors[-1]} is too big")
        return None

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
            s2_max = min(1 << 24, PLL2_HIGH // freq // post_div // stage1_div)
            if bigden > s2_max << 24:
                continue                # Not acheivable.

            s2_min = ceil(PLL2_LOW / freq / post_div / stage1_div)
            # s2_min doesn't give a lower bound on the search, because we apply
            # an extra multiplier to bring the stage2_div into range.  However,
            # we can reject non-feasible values.
            if s2_min > 1 << 24:
                continue                # Not acheivable.

            # As a heuristic, limiting the denominator usually works and makes
            # the search much faster.  Or maybe we just shouldn't use python.
            if fast:
                den_max = min(1 << 24, bigden // s2_min)
            else:
                den_max = 1 << 24

            for stage2_div, mult_den in \
                    factor_splitting(bigden, factors, s2_max, den_max):
                plan = pll2_plan_low1(target, freq,
                                      post_div, stage1_div,
                                      mult_den, stage2_div)
                if best is None or plan is not None and plan < best:
                    best = plan
    return best

def pll2_plan_low(target: FrequencyTarget, freq: Fraction) -> PLLPlan:
    '''Plan for the special case where we only have the BIG_DIVIDE output, and
    the stage2 divider is definitely needed.

    Avoid a complete brute force search over the PLL frequency range.  If
    possible, achieve the exact frequency based on factorising the frequency
    ratio.  If that fails, then multiply the frequency by arbitrary factors to
    get into a sensible range, and then use the normal PLL2 planning.'''
    assert freq < Fraction(PLL2_LOW, 7 * 256)
    assert freq == target.freqs[BIG_DIVIDE]

    # Just like TICS Pro, assume a FPD divider of 18.  So this gives the overall
    # multiple of the PLL2 PFD frequency.
    ratio = freq / PLL2_PFD

    # Factorize the denominator.
    factors = qd_factor(ratio.denominator)

    # We only get called for frequencies well below PLL2_PFD = BAW_FREQ/18!
    assert len(factors) != 0

    plan = pll2_plan_low_exact(target, freq, True, ratio, factors)
    if plan is not None:
        return plan

    plan = pll2_plan_low_exact(target, freq, False, ratio, factors)
    if plan is not None:
        return plan

    # Now find a multiple of pll2_freq that puts us into a sensible range for a
    # search of the VCO range.  First try multiplying by factors of the
    # frequency denominator.

    MIN = 100 * kHz

    pll2_lcm = freq
    ratio_denominator = ratio.denominator
    for p in reversed(factors):
        while ratio_denominator % p == 0:
            next = pll2_lcm * p
            if ceil(Fraction(OFFICIAL_PLL2_LOW) / next) > \
               floor(Fraction(OFFICIAL_PLL2_HIGH) / next):
                break
            pll2_lcm = next
            ratio_denominator //= p
            if pll2_lcm >= MIN:
                break
        if pll2_lcm >= MIN:
            break

    # Now just multiply by a power of 2 to get us over 100kHz.
    while pll2_lcm < MIN:
        pll2_lcm *= 2

    return pll2_plan(target, [Fraction(0)] * BIG_DIVIDE + [freq], pll2_lcm)

def pll2_plan1(target: FrequencyTarget, freqs: list[Fraction],
               pll2_freq: Fraction) -> PLLPlan | None:
    '''Try and create a plan using a particular PLL2 frequency.  Note that
    the frequency list might not include all the frequencies in the target.'''
    assert PLL2_LOW <= pll2_freq <= PLL2_HIGH
    # Bit mask of what post-divider pairs are usable.
    postdivs = (1 << 64) - 1
    # Bit mask of what post-divider pairs are usable.  Ditto, but with the
    # constraint that they are even.  FIXME - do we care about even postdivs?
    # the output dividers are what we care about.
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
    '''Create a frequency plan using PLL2 for a list of frequencies.

    This does a brute force search, and to get sane run times, we need there to
    be a limited number of multiples of pll2_lcm in the VCO range.  Use
    pll2_plan_low instead for the case where pll2_lcm is low.'''
    # Firstly, if frequency is too high, then we can't do it.  Good luck
    # actually getting 3125MHz through the output drivers!
    maxf = max(f for f in freqs if f)
    if maxf > Fraction(PLL2_HIGH, 4):
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
