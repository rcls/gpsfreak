#!/usr/bin/python3

from fractions import Fraction

from .plan_dpll import DPLLPlan, dpll_plan
from .plan_pll2 import PLLPlan, pll2_plan, pll2_plan_low
from .plan_target import *
from .plan_tools import fail, fract_lcm

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
        yield Fraction(intf, 1)
    if f != intf:
        for inner in cont_frac_approx(1 / (f - intf)):
            yield intf + 1 / inner

assert list(cont_frac_approx(Fraction(5,3))) == [1, 2, Fraction(5,3)]

def rejig_pll1(base: PLLPlan) -> PLLPlan:
    if base.pll2 == base.pll2_target:
        return base                     # Nothing to improve.

    best = base

    # Back calculate the BAW frequency from the target.  Ratios with smaller
    # numerator & denomoninator may be easier to achieve, so work through the
    # continued fraction expansion of the multiplier.
    for multiplier in cont_frac_approx(base.multiplier):
        target = base.pll2_target / multiplier * base.fpd_divide
        if not BAW_LOW <= target <= BAW_HIGH:
            continue
        for pre_div in range(2, 17 + 1):
            fb_div = target / REF_FREQ / 2 / pre_div
            fb_div = fb_div.limit_denominator((1 << 40) - 1)
            baw = REF_FREQ * 2 * pre_div * fb_div
            dpll = DPLLPlan(baw=baw, baw_target=baw,
                            fb_prediv = pre_div, fb_div=fb_div)
            pll2 = baw / base.fpd_divide * multiplier
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
    zero = Fraction(0)
    for i, f in enumerate(target.freqs):
        if not f:
            pll1.append(zero)
            pll2.append(zero)
        elif not target.force_pll2(f) and dpll.pll1_divider(i, f):
            pll1.append(f)
            pll2.append(zero)
        elif i == BIG_DIVIDE or f >= PLL2_LOW / (7 * 256):
            pll1.append(zero)
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
