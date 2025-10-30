#!/usr/bin/python3

from fractions import Fraction

from .plan_dpll import DPLLPlan, dpll_plan
from .plan_pll2 import PLLPlan, pll2_plan, pll2_plan_low
from .plan_target import *
from .plan_tools import fail, fract_lcm

def add_pll1(target: FrequencyTarget,
             plan: PLLPlan, freqs: list[Fraction]) -> None:
    for i, f in enumerate(freqs):
        if not f:
            continue
        od = plan.dpll.pll1_divider(i, f)
        assert od is not None
        plan.freqs[i] = plan.dpll.baw / od[0] / od[1]
        plan.dividers[i] = 0, od[0], od[1]

def plan(target: FrequencyTarget) -> PLLPlan:
    # Do the DPLL planning first...
    dpll = dpll_plan(target)
    # First pull out the divisors of 2.5G...
    pll1: list[Fraction] = []
    pll2: list[Fraction] = []
    zero = Fraction(0)
    for i, f in enumerate(target.freqs):
        if not f:
            pll1.append(zero)
            pll2.append(zero)
        elif dpll.pll1_divider(i, f):
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
        plan.freqs = [zero] * len(target.freqs)
        plan.dividers = [(0, 0, 0)] * len(target.freqs)
    # Above about 50 kHz we can brute force the ≈1GHz VCO range within a
    # reasonable time.
    elif pll2_lcm >= SMALL:
        plan = pll2_plan(target, dpll, pll2, pll2_lcm)
    elif target.freqs[BIG_DIVIDE]:
        assert all(not f for i, f in enumerate(pll2) if i != BIG_DIVIDE)
        plan = pll2_plan_low(target, dpll, target.freqs[BIG_DIVIDE])

    if any(pll1):
        add_pll1(target, plan, pll1)
    return plan
