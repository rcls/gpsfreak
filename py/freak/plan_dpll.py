'''Planning for the LMK05318b DPLL.'''

from __future__ import annotations

from .plan_constants import *
from .plan_tools import Target, is_multiple_of, output_divider

from dataclasses import dataclass
from fractions import Fraction
from math import ceil
from typing import Generator, Tuple

__all__ = 'DPLLPlan', 'dpll_plan'

@dataclass
class DPLLPlan:
    # The actual BAW frequency
    baw: Fraction = BAW_FREQ
    # The target BAW frequency.  This is what we use for down-stream
    # calculations?
    baw_target: Fraction = BAW_FREQ
    # Input reference frequency.
    reference: Fraction = REF_FREQ
    # Reference divider.
    ref_div: int = 1
    # Variable predivider 2 to 17.  This is actually post the main divider.
    fb_prediv: int = 2
    # The main ΣΔ divider.  As well as the predivider, there is a fixed divde
    # by two.
    fb_div: Fraction = BAW_FREQ / REF_FREQ / 2 / 2

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
        #print(index, self.baw_target / f)
        if is_multiple_of(self.baw_target, f):
            #print(output_divider(index, self.baw_target // f))
            return output_divider(index, self.baw_target // f)
        else:
            return None

    def pll2_pfd(self) -> Fraction:
        # Just like TICS Pro, assume a FPD divider of 18.  We are assuming that
        # the only use of that is to get the PFD frequency into the supported
        # sub-150MHz range.
        return self.baw / FPD_DIVIDE

    def validate(self) -> None:
        assert self.fb_div.denominator < 1 << 40
        assert self.baw == \
            self.reference / self.ref_div * 2 * self.fb_prediv * self.fb_div
        assert abs(self.baw - self.baw_target) < 1 * Hz

# Check that our defaults match the TI calculated values...
assert DPLLPlan().fb_div == 70 + Fraction(730877267270, 1099509789039)

def baw_plan_for_freq(target: Target, freq: Fraction) -> DPLLPlan:
    '''Make a DPLL plan for the given frequency.  Note that the frequency is
    not validated.'''
    best = None
    ratio = freq / target.reference
    for pre_div in range(2, 17+1):
        fb_div_target = ratio / 2 / pre_div
        fb_div = fb_div_target.limit_denominator((1 << 40) - 1)
        plan = DPLLPlan(
            baw = target.reference * 2 * pre_div * fb_div,
            baw_target = freq,
            reference = target.reference,
            fb_prediv = pre_div,
            fb_div = fb_div)
        if plan.baw == freq:
            return plan                 # If we're exact, that's good enough.
        if plan < best:
            best = plan
    assert best is not None
    return best

def baw_plan_low_approx(target: Target, freq: Fraction) -> DPLLPlan | None:
    '''Brute force search.  We've given up on finding an exact solution,
    so just try the best of a limited range.'''
    half_range = 1000
    best  = None
    error = BAW_HIGH
    start = ceil(BAW_LOW / freq)
    end   = BAW_HIGH // freq
    mid   = BAW_FREQ // freq
    end   = min(end  , mid + half_range, 1 << 32)
    start = max(start, end - 2 * half_range)
    # FIXME - as we get close to the bottom of the possible range, fewer
    # of the stage1*stage2 possibilities are feasible, so we are wasting
    # time trying infeasible values of m.
    for prediv in range(2, 17 + 1):
        ref_mult = target.reference * 2 * prediv
        ratio_target = freq / ref_mult
        for m in range(start, end + 1):
            fb_div = ratio_target * m
            fb_div = fb_div.limit_denominator(1 << 40)
            baw = fb_div * ref_mult
            e = abs(baw / m - freq)
            if e < error and output_divider(5, m):
                error = e
                best = DPLLPlan(
                    baw = baw, baw_target = freq * m,
                    reference = target.reference,
                    fb_prediv = prediv, fb_div = fb_div)
    return best

def sym_range(f: Fraction, low: Fraction, high: Fraction,
              limit: int) -> Generator[int]:
    '''Iterate over all multipliers of `f` that give a product in the range
    between `low` and `high` inclusive.  But limit the multiplier to `limit`.
    Return even multipliers before odd multipliers, and then return multipliers
    closer to mid-range first.'''
    mid = (low + high) / 2
    offset = mid // f
    start = ceil(low / f)
    end = min(limit, high // f)
    if start > end:
        return
    initial = max(0, offset - end, start - offset)
    parity = (offset + initial) & 1
    final = max(end - offset, offset - start)
    for p in parity, 1 - parity:
        for i in range(initial + p, 1 + final, 2):
            if start <= offset - i <= end:
                yield offset - i
                if i != 0 and start <= offset + i <= end:
                    yield offset + i

def baw_plan_low_exact(target: Target, freq: Fraction) -> DPLLPlan | None:
    '''Brute force for an exact solution of getting a low frequency out of
    the BAW.  We assume that the stage2 divider is needed.

    We gain speed by not doing the song and dance needed for approximation.'''
    for stage1 in range(6, 256 + 1):
        base = freq * stage1
        for prediv in range(2, 17 + 1):
            post_fb_div = target.reference * 2 * prediv
            fb_base = base / post_fb_div
            for stage2 in sym_range(base, BAW_LOW, BAW_HIGH, 1<<24):
                fb_div = fb_base * stage2
                if fb_div.denominator < 1<<40:
                    assert post_fb_div * fb_div == freq * stage1 * stage2
                    return DPLLPlan(
                        baw = post_fb_div * fb_div,
                        baw_target = post_fb_div * fb_div,
                        reference = target.reference,
                        fb_prediv = prediv,
                        fb_div = fb_div)
    return None

def baw_plan_low(target: Target, freq: Fraction) -> DPLLPlan | None:
    print('Try BAW LF exact brute force')
    exact = baw_plan_low_exact(target, freq)
    if exact:
        return exact
    print('Try BAW LF inexact brute force.')
    return baw_plan_low_approx(target, freq)

def single_baw_mult(freq: Fraction) -> int | None:
    '''If there is exactly one multiple of freq in the BAW range, then return
    it.  Else return None.'''
    m = ceil(BAW_LOW / freq)
    if m == BAW_HIGH // freq:
        return m
    else:
        return None

def dpll_plan(target: Target) -> DPLLPlan:
    # If we are given a DPLL target, then use it.
    if target.pll1_base:
        m = single_baw_mult(target.pll1_base)
        assert m is not None
        return baw_plan_for_freq(target, m * target.pll1_base)

    default = baw_plan_for_freq(target, BAW_FREQ)
    default.baw_target = default.baw

    # TODO we could do better, by looking for a BAW frequency that leaves
    # PLL2_LCM achievable?
    counts: dict[Fraction, int] = {}
    for i, f in enumerate(target.freqs):
        if not f:
            continue

        if target.pll2_base and is_multiple_of(target.pll2_base, f):
            # Skip frequencies that are requested on PLL2.
            continue

        if is_multiple_of(default.baw, f) \
           and output_divider(i, default.baw // f) is not None:
            # If we can use the default BAW_FREQ for anything, then we do so.
            #print(f'Use default for {f}')
            return default

        m = single_baw_mult(f)
        if m is not None and output_divider(i, m) is not None:
            baw = m * f
            #print(f'For {f}, multiple {m} is {baw}')
            counts[baw] = 1 + counts.get(baw, 0)

    # Re-assess the B_D frequency...
    bd = target.freqs[BIG_DIVIDE] if BIG_DIVIDE < len(target.freqs) else Fraction()
    if bd and not target.force_pll2(bd):
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
        if bd and not target.force_pll2(bd) and bd <= BAW_HIGH - BAW_LOW:
            #not any(f for i, f in enumerate(target.freqs) if i != BIG_DIVIDE):
            plan = baw_plan_low(target, bd)
            if plan is not None:
                return plan
        #print('Nothing doing on PLL1')
        return default

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
        plan = baw_plan_for_freq(target, freq)
        if plan < best:
            best = plan
    assert best is not None
    return best

def test_single_baw_mult():
    m = single_baw_mult(100 * MHz + 100 * Hz)
    assert m is not None

def test_default():
    plan = baw_plan_for_freq(Target(freqs = []), 2500_000 * kHz)
    assert plan == DPLLPlan()

def test_exact():
    f = 2500 * MHz + 25001 * Hz
    plan = baw_plan_for_freq(Target(freqs = []), f)
    assert plan.baw == REF_FREQ * 2 * plan.fb_div * plan.fb_prediv
    assert plan.baw_target == f
    assert plan.baw == f

def test_inexact():
    f = 2500 * MHz + 25000 * Hz + Hz/37217
    plan = baw_plan_for_freq(Target(freqs = []), f)
    assert 0 < plan.fb_prediv.denominator <= 1<<40
    assert plan.baw == REF_FREQ * 2 * plan.fb_div * plan.fb_prediv
    assert plan.baw != plan.baw_target
    print(plan)
    print(float(plan.baw - plan.baw_target))
    assert abs(plan.baw - plan.baw_target) < 1e-15 * Hz
