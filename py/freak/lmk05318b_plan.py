#!/usr/bin/python3

import argparse
from fractions import Fraction
from typing import Generator, Tuple

from .lmk05318b import MaskedBytes, REGISTERS
from .plan_dpll import DPLLPlan, dpll_plan
from .plan_pll2 import PLLPlan, pll2_plan, pll2_plan_low
from .plan_constants import *
from .plan_tools import Target, fail, fract_lcm, fraction_to_str, freq_to_str, \
    str_to_freq

CHANNELS_RAW = list(
    (i, f'Channel {s:3}', s) for i, s in enumerate('0_1 2_3 4 5 6 7'.split()))
CHANNELS_COOKED = [
    (1, 'Out 1 [2_3]', '2_3'),
    (0, 'Out 2 [0_1]', '0_1'),
    (5, 'Out 3 [7]  ', '7'),
    (4, 'Out 4 [6]  ', '6'),
    (3, 'U.Fl  [5]  ', '5'),
    (2, 'Spare [4]  ', '4')]

def make_freq_target(args: argparse.Namespace, raw: bool) -> Target:
    reference = args.reference if args.reference else REF_FREQ
    freqs = args.FREQ
    channels = CHANNELS_RAW if raw else CHANNELS_COOKED
    result = [Fraction(0)] * max(6, len(args.FREQ))
    # FIXME - this just silently ignores extras.
    for (i, _, _), f in zip(channels, freqs):
        result[i] = f
    return Target(freqs=result, pll2_base=args.pll2, reference=reference)

def make_freq_data(plan: PLLPlan) -> MaskedBytes:
    data = MaskedBytes()
    postdiv1 = 0
    postdiv2 = 0
    for pd, _, _ in plan.dividers:
        if pd == 0:
            continue
        if postdiv1 == 0:
            postdiv1 = pd
            postdiv2 = pd
        elif pd != postdiv1 and postdiv2 == postdiv1:
            postdiv2 = pd
        else:
            assert pd == postdiv1 or pd == postdiv2
    if postdiv1 == 0:
        postdiv1 = 2
    if postdiv2 == 0:
        postdiv2 = 2

    chtag = '0_1', '2_3', '4', '5', '6', '7'
    for i, (postdiv, stage1, stage2) in enumerate(plan.dividers):
        t = chtag[i]
        if stage1 == 0:                     # Disabled.
            data.insert(f'CH{t}_PD', 1)
            continue
        data.insert(f'CH{t}_PD', 0)
        # Source.
        if postdiv != 0:
            assert 2 <= postdiv <= 7
            assert plan.pll2 != 0

        if postdiv == 0:
            data.insert(f'CH{t}_MUX', 1)
        elif postdiv == postdiv1:
            data.insert(f'CH{t}_MUX', 2)
        elif postdiv == postdiv2:
            data.insert(f'CH{t}_MUX', 3)
        else:
            assert 'This should never happen' == None
        assert 1 <= stage1 <= 256
        data.insert(f'OUT{t}_DIV', stage1 - 1)
        if i == 5:
            assert 1 <= stage2 <= 1<<24
            data.OUT7_STG2_DIV = stage2 - 1
        else:
            assert stage2 == 1

    data.DPLL_PRIREF_RDIV = 1
    data.DPLL_REF_FB_PRE_DIV = plan.dpll.fb_prediv - 2
    div = plan.dpll.fb_div.numerator // plan.dpll.fb_div.denominator
    num = plan.dpll.fb_div.numerator % plan.dpll.fb_div.denominator
    den = plan.dpll.fb_div.denominator
    mult = ((1 << 40) - 1) // den
    data.DPLL_REF_FB_DIV = div
    data.DPLL_REF_NUM = num * mult
    data.DPLL_REF_DEN = den * mult

    # PLL1 seeding.
    data.PLL1_NDIV, data.PLL1_NUM = plan.dpll.pll1_ratio()

    # Frequency lock detection.
    baw_lock_xo, baw_lock_vco = plan.dpll.baw_lock_det()
    data.BAW_LOCK_CNTSTRT = baw_lock_xo
    data.BAW_LOCK_VCO_CNTSTRT = baw_lock_vco
    data.BAW_UNLK_CNTSTRT = baw_lock_xo
    data.BAW_UNLK_VCO_CNTSTRT = baw_lock_vco

    dpll_lock_ref, dpll_lock_vco = plan.dpll.dpll_lock_det()
    data.DPLL_REF_LOCKDET_CNTSTRT = dpll_lock_ref
    data.DPLL_REF_LOCKDET_VCO_CNTSTRT = dpll_lock_vco
    # Note that these alias other registers, and get overwritten if the other
    # registers are in use.  The ...VCO_CNTSTRT is definitely necessary, not
    # sure about the other.
    #data.DPLL_REF_UNLOCK_CNTSTRT = dpll_lock_ref
    data.DPLL_REF_UNLOCKDET_VCO_CNTSTRT = dpll_lock_vco

    if plan.pll2_target == 0:
        data.LOL_PLL2_MASK = 1
        data.MUTE_APLL2_LOCK = 0
        data.PLL2_PDN = 1
        return data

    # PLL2 post dividers.
    data.PLL2_P1 = postdiv1 - 1
    data.PLL2_P2 = postdiv2 - 1

    # PLL2 setup...
    data.PLL2_PDN  = 0
    data.LOL_PLL2_MASK = 0
    data.MUTE_APLL2_LOCK = 1
    pll2_den = plan.multiplier.denominator
    pll2_int = plan.multiplier.numerator // pll2_den
    pll2_num = plan.multiplier.numerator % pll2_den
    if plan.fixed_denom():
        data.APLL2_DEN_MODE = 0
        assert (1<<24) % pll2_den == 0
        pll2_num = pll2_num * (1<<24) // pll2_den
    else:
        data.APLL2_DEN_MODE = 1
        data.PLL2_DEN  = pll2_den
    data.PLL2_NDIV = pll2_int
    data.PLL2_NUM  = pll2_num
    # Canned values... (Should we rely on these being preprogrammed?)
    data.PLL2_RCLK_SEL = 0
    data.PLL2_RDIV_PRE = 0
    data.PLL2_RDIV_SEC = 5
    data.PLL2_DISABLE_3RD4TH = 15
    data.PLL2_CP = 1
    data.PLL2_LF_R2 = 2
    data.PLL2_LF_C1 = 0
    data.PLL2_LF_R3 = 1
    data.PLL2_LF_R4 = 1
    data.PLL2_LF_C4 = 7
    data.PLL2_LF_C3 = 7
    return data

def reverse_plan(d: MaskedBytes, reference: Fraction) -> Tuple[Target, PLLPlan]:
    '''Scrape a plan out of a configuration read-back.'''
    pll2_rdiv = (d.PLL2_RDIV_PRE + 3) * (d.PLL2_RDIV_SEC + 1)
    assert pll2_rdiv == 18              # Only supported value.

    dpll = DPLLPlan()
    dpll.ref_div = d.DPLL_PRIREF_RDIV
    dpll.fb_prediv = d.DPLL_REF_FB_PRE_DIV + 2
    if d.DPLL_REF_DEN:
        dpll.fb_div = d.DPLL_REF_FB_DIV \
            + Fraction(d.DPLL_REF_NUM, d.DPLL_REF_DEN)
    else:
        dpll.fb_div = Fraction(0)

    if dpll.ref_div:
        dpll.baw = reference / dpll.ref_div * 2 * dpll.fb_prediv * dpll.fb_div
    else:
        dpll.baw = Fraction(0)

    dpll.baw_target = dpll.baw

    plan = PLLPlan(dpll = dpll)
    if d.PLL2_DEN == 0:
        plan.multiplier = Fraction(0)
    else:
        plan.multiplier = d.PLL2_NDIV + Fraction(d.PLL2_NUM, d.PLL2_DEN)

    if not d.PLL2_PDN:
        plan.pll2 = dpll.baw / pll2_rdiv * plan.multiplier
        plan.pll2_target = plan.pll2

    for _, _, tag in CHANNELS_RAW:
        mux = d.extract(f'CH{tag}_MUX')
        prediv = 1
        if mux == 2:
            prediv = d.PLL2_P1 + 1
        elif mux == 3:
            prediv = d.PLL2_P2 + 1
        s1div = d.extract(f'OUT{tag}_DIV') + 1
        s2div = 1
        if tag == '7':
            s2div = d.OUT7_STG2_DIV + 1
        plan.dividers.append((prediv, s1div, s2div))

    pll1_pfd = dpll.baw / (d.PLL1_NDIV + Fraction(d.PLL1_NUM_STAT, 1 << 40))

    target = Target(freqs = [plan.freq(i) for i in range(len(plan.dividers))],
                    pll1_pfd = pll1_pfd, reference = reference)

    return target, plan

def report_plan(target: Target, plan: PLLPlan, raw: bool,
                power_down: int = 0, verbose: bool = False) -> None:
    channels = CHANNELS_RAW if raw else CHANNELS_COOKED
    for index, name, _ in channels:
        if index >= len(target.freqs):
            continue
        t = target.freqs[index]
        if not t:
            continue

        f = plan.freq(index)
        divs = plan.dividers[index]
        pll = 'BAW' if divs[0] < 2 else 'PLL2'
        pd = 'Power down, ' if power_down & 1 << index else ''
        print(f'{name} {pd}{freq_to_str(f)}', end='')
        if f != t:
            print(f' error {freq_to_str(f - t, 4)}', end='')
        print(f' {pll} dividers', ' '.join(str(d) for d in divs if d > 1))

    print()
    dpll = plan.dpll
    print(f'BAW: {freq_to_str(dpll.baw)} = {freq_to_str(target.reference)} '
          f'* 2 * {dpll.fb_prediv} * {fraction_to_str(dpll.fb_div)}')
    if dpll.baw != dpll.baw_target:
        error = freq_to_str(dpll.baw - dpll.baw_target, 4)
        print(f'    target {freq_to_str(dpll.baw_target)}, error {error}')
    if plan.pll2_target != 0:
        print(f'PLL2: {freq_to_str(plan.pll2)} = '
              f'BAW / 18 * {fraction_to_str(plan.multiplier)}')
        if plan.pll2 != plan.pll2_target:
            print(f'    target {freq_to_str(plan.pll2_target)}, '
                  f'error {freq_to_str(plan.error(), 4)}')

    if verbose:
        print()
        data = make_freq_data(plan)
        for r in REGISTERS.values():
            if r.extract(data.mask) != 0:
                value = data.extract(r)
                print(f'{r} = {value} ({value:#x})')

def add_pll1(target: Target, plan: PLLPlan, freqs: list[Fraction]) -> None:
    for i, f in enumerate(freqs):
        if len(plan.dividers) <= i:
            plan.dividers.append((0, 0, 0))
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

def rejig_pll1(base: PLLPlan) -> PLLPlan:
    '''Attempt to make PLL2 more accurate, by tweaking the DPLL frequency.

    Simplifying the PLL2 multiplier ratio may well be helpful, so scan through
    the continued fraction expansion and pick the best possibility.'''
    if base.pll2 == base.pll2_target:
        return base                     # Nothing to improve.

    reference = base.dpll.reference
    best = base

    # Back calculate the BAW frequency from the target.  Ratios with smaller
    # numerator & denomoninator may be easier to achieve, so work through the
    # continued fraction expansion of the multiplier.
    assert base.multiplier ==  base.pll2 / base.dpll.baw * FPD_DIVIDE
    target_multiplier = base.pll2_target / base.dpll.baw * FPD_DIVIDE
    for m in cont_frac_approx(target_multiplier):
        multiplier = m.limit_denominator(1 << 24)
        baw_target = base.pll2_target / multiplier * FPD_DIVIDE
        if not BAW_LOW <= baw_target <= BAW_HIGH:
            continue
        for pre_div in range(2, 17 + 1):
            fb_div = baw_target / reference / 2 / pre_div
            fb_div = fb_div.limit_denominator((1 << 40) - 1)
            baw = reference * 2 * pre_div * fb_div
            dpll = DPLLPlan(baw=baw, baw_target=baw, reference=reference,
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

def plan(target: Target) -> PLLPlan:
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
        plan = pll2_plan(target, dpll, pll2, pll2_lcm)

    elif target.freqs[BIG_DIVIDE]:
        assert all(not f for i, f in enumerate(pll2) if i != BIG_DIVIDE)
        plan = pll2_plan_low(target, dpll, target.freqs[BIG_DIVIDE])

    else:
        plan = PLLPlan(dpll = dpll)

    if any(pll1):
        add_pll1(target, plan, pll1)
    else:
        plan.validate()
        plan = rejig_pll1(plan)

    plan.validate()
    return plan

def test_32k() -> None:
    target = Target(
        freqs = [ZERO] * 5 + [str_to_freq('32768.298Hz')])
    assert float(target.freqs[5] / Hz) == 32768.298
    p = plan(target)
    # We should get an exact result using PLL1.
    assert p.freq(5) == target.freqs[5]
    assert p.pll2 == 0
    assert p.dpll.baw == p.dpll.baw_target
    assert BAW_LOW <= p.dpll.baw <= BAW_HIGH
    # Work without assuming our units...
    assert p.dpll.reference == 8844582 * Hz
    assert 8844582 * 2 * p.dpll.fb_prediv * p.dpll.fb_div / (
        p.dividers[5][1] * p.dividers[5][2]) == Fraction('32768.298')

def test_32k_11M() -> None:
    target = Target(
        [11 * MHz] + [ZERO] * 4 + [3276829 * Hz / 100])
    p = plan(target)
    # One should be exact....
    assert p.freq(0) == target.freqs[0] or p.freq(5) == target.freqs[5]
    # Errors should be less than a nano hertz.
    nHz = Hz / 1000_000_000
    assert abs(p.freq(0) - target.freqs[0]) < nHz
    assert abs(p.freq(5) - target.freqs[5]) < nHz

def test_11M_33M() -> None:
    target = Target([11 * MHz, 33333 * kHz])
    p = plan(target)
    assert p.freq(0) == target.freqs[0]
    assert p.freq(1) == target.freqs[1]

def test_round():
    f = Fraction('46.60376888888889')
    target = Target(freqs = [f])
    p = plan(target)
    report_plan(target, p, True)
    assert p.multiplier.denominator <= 1 << 24

def test_cont_frac() -> None:
    assert list(cont_frac_approx(Fraction(5,3))) == [1, 2, Fraction(5,3)]
    import math
    expect: list[Fraction] = []
    n, d = 1, 1
    for _ in range(21):
        expect.append(Fraction(n, d))
        n, d = n + d * 2, n + d
    approx = list(cont_frac_approx(Fraction(math.sqrt(2))))
    assert expect == approx[:len(expect)], f'{expect}\n\n{approx}'
