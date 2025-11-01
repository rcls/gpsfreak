#!/usr/bin/python3

from fractions import Fraction

from .plan_dpll import DPLLPlan, dpll_plan
from .plan_pll2 import PLLPlan, pll2_plan, pll2_plan_low
from .plan_constants import *
from .plan_tools import FrequencyTarget, \
    fail, fract_lcm, freq_to_str, str_to_freq

from typing import Generator

def add_pll1(target: FrequencyTarget,
             plan: PLLPlan, freqs: list[Fraction]) -> None:
    for i, f in enumerate(freqs):
        if not f:
            continue
        od = plan.dpll.pll1_divider(i, f)
        assert od is not None
        plan.dividers[i] = 0, od[0], od[1]

def cont_frac_approx(f: Fraction) -> Generator[Fraction]:
    '''Generate the sequence of continued fraction approximations to f.'''
    intf = int(f)
    if intf:
        yield Fraction(intf)
    if f != intf:
        for inner in cont_frac_approx(1 / (f - intf)):
            yield intf + 1 / inner

def test_cont_frac() -> None:
    assert list(cont_frac_approx(Fraction(5,3))) == [1, 2, Fraction(5,3)]
    import math
    expect = []
    n, d = 1, 1
    for _ in range(21):
        expect.append(Fraction(n, d))
        n, d = n + d * 2, n + d
    approx = list(cont_frac_approx(Fraction(math.sqrt(2))))
    assert expect == approx[:len(expect)], f'{expect}\n\n{approx}'

def rejig_pll1(base: PLLPlan) -> PLLPlan:
    '''Attempt to make PLL2 more accurate, by tweaking the DPLL frequency.

    Simplifying the PLL2 multiplier ratio may well be helpful, so scan through
    the continued fraction expansion and pick the best possibility.'''
    if base.pll2 == base.pll2_target:
        return base                     # Nothing to improve.

    best = base

    # Back calculate the BAW frequency from the target.  Ratios with smaller
    # numerator & denomoninator may be easier to achieve, so work through the
    # continued fraction expansion of the multiplier.
    assert base.multiplier ==  base.pll2 / base.dpll.baw * FPD_DIVIDE
    target_multiplier = base.pll2_target / base.dpll.baw * FPD_DIVIDE
    for multiplier in cont_frac_approx(target_multiplier):
        target = base.pll2_target / multiplier * FPD_DIVIDE
        if not BAW_LOW <= target <= BAW_HIGH:
            continue
        for pre_div in range(2, 17 + 1):
            fb_div = target / REF_FREQ / 2 / pre_div
            fb_div = fb_div.limit_denominator((1 << 40) - 1)
            baw = REF_FREQ * 2 * pre_div * fb_div
            dpll = DPLLPlan(baw=baw, baw_target=baw,
                            fb_prediv = pre_div, fb_div=fb_div)
            pll2 = baw / FPD_DIVIDE * multiplier
            if not PLL2_LOW <= pll2 <= PLL2_HIGH:
                continue
            #print(freq_to_str(pll2))
            plan = PLLPlan(
                dpll = dpll, pll2 = pll2, pll2_target = base.pll2_target,
                multiplier = multiplier, dividers = base.dividers)
            if plan < best:
                best = plan

    return best

def plan(target: FrequencyTarget) -> PLLPlan:
    # Do the DPLL planning first.
    dpll = dpll_plan(target)
    # First pull out the divisors of 2.5G...
    pll1: list[Fraction] = []
    pll2: list[Fraction] = []
    for i, f in enumerate(target.freqs):
        if not f:
            pll1.append(ZERO)
            pll2.append(ZERO)
        elif not target.force_pll2(f) and dpll.pll1_divider(i, f):
            pll1.append(f)
            pll2.append(ZERO)
        elif i == BIG_DIVIDE or f >= PLL2_LOW / (7 * 256):
            pll1.append(ZERO)
            pll2.append(f)
        else:
            fail(f'Frequency {freq_to_str(f)} is not achievable on {i}')

    # Find the LCM of all the pll2 frequencies...
    pll2_lcm = target.pll2_base
    # TODO - we should be able to take this through to pll2_plan_low!
    assert pll2_lcm is None or pll2_lcm >= SMALL

    for f in pll2:
        if f:
            pll2_lcm = fract_lcm(pll2_lcm, f)

    if pll2_lcm is None:
        # Don't use PLL2...
        plan = PLLPlan(dpll = dpll)
        plan.dividers = [(0, 0, 0)] * len(target.freqs)

    # Above about 50 kHz we can brute force the ≈1GHz VCO range within a
    # reasonable time.
    elif pll2_lcm >= SMALL:
        print('Normal pll2 plan')
        plan = pll2_plan(target, dpll, pll2, pll2_lcm)
    elif target.freqs[BIG_DIVIDE]:
        assert all(not f for i, f in enumerate(pll2) if i != BIG_DIVIDE)
        print('Low pll2 plan')
        plan = pll2_plan_low(target, dpll, target.freqs[BIG_DIVIDE])

    if any(pll1):
        add_pll1(target, plan, pll1)
    else:
        plan = rejig_pll1(plan)

    return plan

def test_32k() -> None:
    target = FrequencyTarget(
        freqs = [ZERO] * 5 + [str_to_freq('32768.298Hz')])
    assert float(target.freqs[5] / Hz) == 32768.298
    p = plan(target)
    # We should get an exact result using PLL1.
    assert p.freq(5) == target.freqs[5]
    assert p.pll2 == 0
    assert p.dpll.baw == p.dpll.baw_target
    assert BAW_LOW <= p.dpll.baw <= BAW_HIGH
    # Work without assuming our units...
    assert REF_FREQ == 8844582 * Hz
    assert 8844582 * 2 * p.dpll.fb_prediv * p.dpll.fb_div / (
        p.dividers[5][1] * p.dividers[5][2]) == Fraction('32768.298')

def test_32k_11M() -> None:
    target = FrequencyTarget(
        [11 * MHz] + [ZERO] * 4 + [3276829 * Hz / 100])
    p = plan(target)
    # One should be exact....
    assert p.freq(0) == target.freqs[0] or p.freq(5) == target.freqs[5]
    # Errors should be less than a nano hertz.
    nHz = Hz / 1000_000_000
    assert abs(p.freq(0) - target.freqs[0]) < nHz
    assert abs(p.freq(5) - target.freqs[5]) < nHz

def test_11M_33M() -> None:
    target = FrequencyTarget([11 * MHz, 33333 * kHz])
    p = plan(target)
    assert p.freq(0) == target.freqs[0]
    assert p.freq(1) == target.freqs[1]
