'''Planning for the LMK05318b DPLL.'''

from .plan_target import *
from .plan_tools import is_multiple_of, factor_splitting, qd_factor

from dataclasses import dataclass
from fractions import Fraction
from math import ceil, floor

__all__ = 'DPLLPlan', 'dpll_plan'

@dataclass
class DPLLPlan:
    # The actual BAW frequency
    baw: Fraction = BAW_FREQ
    # The target BAW frequency.  This is what we use for down-stream
    # calculations?
    baw_target: Fraction = BAW_FREQ
    # Variable predivider 2 to 17.  This is actually post the main divider.
    fb_prediv: int = 2
    # The main ΣΔ divider.
    fb_div: Fraction = BAW_FREQ / REF_FREQ / 2 / 2
    def __post_init__(self) -> None:
        assert self.fb_div.denominator < 1 << 40
    # There is also a hard divider of 2...
    def __lt__(self, b: DPLLPlan | None) -> bool:
        '''Less is better.  I.e., return True if self is better than b.'''
        if b is None:
            return True
        # Prefer exact.
        a_error = self.baw - self.baw_target
        b_error = b.baw - self.baw_target
        if a_error != b_error:
            return a_error < b_error
        # Prefer smaller predivs.
        if self.fb_prediv != b.fb_prediv:
            return self.fb_prediv < b.fb_prediv
        # Now we're pretty arbitrary...
        if self.fb_div.denominator != b.fb_div.denominator:
            return self.fb_div.denominator < b.fb_div.denominator
        return self.fb_div < b.fb_div
    def pll1_divider(self, index: int, f: Fraction) -> Tuple[int, int] | None:
        '''Try and get an output frequency by dividing the BAW frequency.

        For this, we use the target frequency not the actual.'''
        if is_multiple_of(self.baw_target, f):
            return output_divider(index, self.baw_target // f)
        else:
            return None
    def pll2_pfd(self) -> Fraction:
        # Just like TICS Pro, assume a FPD divider of 18.  We are assuming that
        # the only use of that is to get the PFD frequency into the supported
        # sub-150MHz range.
        return self.baw / FPD_DIVIDE

# Check that our defaults match the TI calculated values...
assert DPLLPlan().fb_div == 70 + Fraction(730877267270, 1099509789039)

def baw_plan_for_freq(freq: Fraction) -> DPLLPlan:
    '''Make a DPLL plan for the given frequency.  Note that the frequency is
    not validated.'''
    best = None
    ratio = freq / REF_FREQ
    for pre_div in range(2, 17+1):
        fb_div = ratio / 2 / pre_div
        fb_div = fb_div.limit_denominator((1 << 40) - 1)
        plan = DPLLPlan(
            baw = REF_FREQ * 2 * pre_div * fb_div,
            baw_target = freq,
            fb_prediv = pre_div,
            fb_div = fb_div)
        if plan < best:
            best = plan
    assert best is not None
    return best

def baw_plan_low_exact1(freq: Fraction,
                        fb_prediv: int, stage1_div: int,
                        stage2_div: int, den: int) -> DPLLPlan | None:
    '''Attempt to create a plan with the given dividers.  stage2_div may
    be multiplied to get the BAW frequency in range.'''
    min_extra = ceil(BAW_LOW / stage1_div / stage2_div)
    max_extra = BAW_HIGH // stage1_div // stage2_div
    max_extra = min(max_extra, (1 << 24) // stage2_div)
    if min_extra > max_extra:
        return None                     # Impossible
    extra = floor(BAW_FREQ / stage1_div / stage2_div)
    extra = max(extra, min_extra)
    # Attempt to make stage2_div even...
    if stage2_div % 2 != 0 and extra % 2 != 0:
        if extra < max_extra:
            extra = extra + 1
        elif extra > min_extra:
            extra = extra - 1
    baw = freq * extra * stage2_div * stage1_div
    assert BAW_LOW <= baw <= BAW_HIGH
    # FIXME - is fb_div.denominator in range guarenteed?
    plan = DPLLPlan(
        baw = baw,
        baw_target = baw,
        fb_prediv = fb_prediv,
        fb_div = baw / fb_prediv / REF_FREQ)
    assert den % plan.fb_div.denominator == 0
    assert plan.fb_div.denominator < 1 << 40
    return plan

def baw_plan_low_exact(freq: Fraction, fast: bool) -> DPLLPlan | None:
    '''Exact planning for a low frequency output.  We assume that the
    stage2 divider is actually required.'''
    ratio = freq / REF_FREQ
    factors = qd_factor(ratio.denominator, hint = qd_factor(REF_FREQ.numerator))
    best = None
    # FIXME - this pretty much duplicates work depending on the GCD...
    # Also it is going to take a long time....
    for fb_prediv in range(2, 17+1):
        for stage1_div in range(6, 256+1):
            bigden = ratio.denominator // gcd(
                ratio.denominator, fb_prediv * stage1_div)

            s2_max = min(BAW_HIGH // freq // stage1_div, 1 << 24)
            if bigden >= s2_max << 40:
                continue                # Not achievable.

            s2_min = ceil(BAW_LOW / freq / fb_prediv / stage1_div)
            if s2_min > 1 << 24:
                continue

            # The fast heuristic is to limit stage2_div below.  The slow case
            # is to allow an extra multiplier to be applied to get stage2_div
            # into range.
            den_max = (1 << 40) - 1
            if fast:
                den_max = min(den_max, ceil(bigden / s2_min))

            for stage2_div, den in factor_splitting(
                    bigden, factors, s2_max, den_max):
                plan = baw_plan_low_exact1(freq, fb_prediv, stage1_div,
                                           stage2_div, den)
                # FIXME - this doesn't take into account whether or not we got
                # an even stage2_div.
                if plan is not None and plan < best:
                    best = plan
    return plan

def baw_plan_low_search(freq: Fraction) -> DPLLPlan | None:
    '''Brute force search.  We've given up on finding an exact solution,
    so just try the best of a limited range.'''
    best  = None
    start = ceil(BAW_LOW / freq)
    end   = BAW_HIGH // freq
    mid   = BAW_FREQ // freq
    start = max(start, mid - MAX_HALF_RANGE)
    end   = min(end  , mid + MAX_HALF_RANGE)
    #print(f'BAW low search {start} {end} {end - start}')
    # FIXME - this doesn't take into account output divider feasibility.
    for m in range(start, end + 1):
        baw = freq * m
        for prediv in range(2, 17 + 1):
            fb_div = baw / REF_FREQ / 2 / prediv
            fb_div = fb_div.limit_denominator(1 << 40)
            plan = DPLLPlan(
                baw = REF_FREQ * 2 * prediv * fb_div,
                baw_target = baw,
                fb_prediv = prediv,
                fb_div = fb_div)
            if plan < best:
                best = plan
    #print(f'BAW low search done')
    return best

def baw_plan_low(freq: Fraction) -> DPLLPlan | None:
    if (BAW_HIGH - BAW_LOW) // freq > 2 * MAX_HALF_RANGE:
        # Attempt a divisor based search....
        pass
    return baw_plan_low_search(freq)

def single_baw_mult(freq: Fraction) -> int | None:
    '''If there is exactly one multiple of freq in the BAW range, then return
    it.  Else return None.'''
    m = ceil(BAW_LOW / freq)
    if m == BAW_HIGH // freq:
        return m
    else:
        return None

def dpll_plan(target: FrequencyTarget) -> DPLLPlan:
    # If we are given a DPLL target, then use it.
    if target.pll1_base:
        m = single_baw_mult(target.pll1_base)
        assert m is not None
        return baw_plan_for_freq(m * target.pll1_base)

    # TODO we could do better, by looking for a BAW frequency that leaves
    # PLL2_LCM achievable?
    counts: dict[Fraction, int] = {}
    for i, f in enumerate(target.freqs):
        if not f:
            continue

        if (BAW_FREQ / f).is_integer() \
           and output_divider(i, BAW_FREQ // f) is not None:
            # If we can use the default BAW_FREQ for anything, then we do so.
            print(f'Use default for {f}')
            return DPLLPlan()

        m = single_baw_mult(f)
        if m is not None and output_divider(i, m) is not None:
            baw = m * f
            print(f'For {f}, multiple {m} is {baw}')
            counts[baw] = 1 + counts.get(baw, 0)

    # Re-assess the B_D frequency...
    bd = target.freqs[BIG_DIVIDE] if BIG_DIVIDE < len(target.freqs) else Fraction()
    if bd:
        m1 = ceil(BAW_LOW / bd)
        m2 = BAW_HIGH // bd
        if m1 < m2:
            for f in counts:
                if is_multiple_of(f, bd) \
                   and output_divider(BIG_DIVIDE, f // bd):
                    counts[f] += 1

    if len(counts) == 0:
        # Nothing could be uniquely divided from the BAW range.  Recheck the
        # bd frequency to see if it's worth searching...
        if bd <= BAW_HIGH - BAW_LOW and \
           not any(f for i, f in enumerate(target.freqs) if i != BIG_DIVIDE):
            plan = baw_plan_low(bd)
            if plan is not None:
                return plan
        print('Nothing doing on PLL1')
        return DPLLPlan()

    # The feedback divider has three stages:
    # * A fixed /2
    # Then the rational DPLL_REF_FB_DIV + DPLL_REF_NUM / DPLL_REF_DEN
    # And finally DPLL_REF_FB_PRE_DIV in the range 2..=17
    #
    # We use an R-DIV of 1 and so a PFD frequency of 8844582.
    possible = [(count, freq) for freq, count in counts.items()]
    possible.sort(reverse=True)

    best = None
    for _, freq in possible:
        plan = baw_plan_for_freq(freq)
        if plan < best:
            best = plan
    assert best is not None
    return best

def test_single_baw_mult():
    m = single_baw_mult(100 * MHz + 100 * Hz)
    assert m is not None

def test_default():
    plan = baw_plan_for_freq(2500_000 * kHz)
    assert plan == DPLLPlan()

def test_exact():
    f = 2500 * MHz + 25001 * Hz
    plan = baw_plan_for_freq(f)
    assert plan.baw == REF_FREQ * 2 * plan.fb_div * plan.fb_prediv
    assert plan.baw_target == f
    assert plan.baw == f

def test_inexact():
    f = 2500 * MHz + 25000 * Hz + Hz/37217
    plan = baw_plan_for_freq(f)
    assert 0 < plan.fb_prediv.denominator <= 1<<40
    assert plan.baw == REF_FREQ * 2 * plan.fb_div * plan.fb_prediv
    assert plan.baw != plan.baw_target
    print(plan)
    print(float(plan.baw - plan.baw_target))
    assert abs(plan.baw - plan.baw_target) < 1e-15 * Hz
