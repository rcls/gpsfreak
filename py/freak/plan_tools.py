from fractions import Fraction

from math import gcd
from typing import Any, Generator, NoReturn, Tuple

class PlanningFailed(RuntimeError):
    pass

def fail(*args: Any, **kwargs: Any) -> NoReturn:
    import sys
    print(*args, **kwargs)
    raise PlanningFailed(' '.join(str(s) for s in args))
    #sys.exit(1)

def is_multiple_of(a: Fraction, b: Fraction | None) -> bool:
    if not b:
        return False
    return a.numerator % b.numerator == 0 and \
        b.denominator % a.denominator == 0

def do_factor_splitting(left: int, right: int, maxL: int, maxR: int, \
                        primes: list[int], index: int) \
        -> Generator[Tuple[int, int]]:
    '''Worker function for factor_splitting below'''
    if index >= len(primes):
        if left <= maxL and right <= maxR:
            yield left, right
        return
    prime = primes[index]
    while True:
        yield from do_factor_splitting(
            left, right, maxL, maxR, primes, index + 1)
        if right % prime != 0:
            return
        left *= prime
        if left > maxL:
            return
        right //= prime

def factor_splitting(number: int, primes: list[int], maxL: int, maxR: int) \
        -> Generator[Tuple[int, int]]:
    '''Return all possible factorisations of number into two factors, with the
    constraint that both are less than maxL or maxR.  The list primes should
    contain at least all prime factors of number.'''
    # It's more efficient to put the smaller maximum first.
    if maxL <= maxR:
        yield from do_factor_splitting(1, number, maxL, maxR, primes, 0)
    for a, b in do_factor_splitting(1, number, maxR, maxL, primes, 0):
        yield b, a

def fract_lcm(a: Fraction|None, b: Fraction|None) -> Fraction|None:
    if a is None:
        return b
    if b is None:
        return a

    u = a.denominator * b.numerator
    v = a.numerator * b.denominator
    g = gcd(u, v)
    u = u // g
    v = v // g
    au = a * u
    assert au == b * v, f'{a} {b} {u} {v}'
    return au

def test_fract_lcm():
    L2 = list(map(Fraction, '1/8 1/4 1/2 1 2 4 8'.split()))
    L3 = list(map(Fraction, '1/27 1/9 1/3 1 3 9 27'.split()))
    L5 = list(map(Fraction, '1/25 1/5 1 5 25'.split()))
    L7 = list(map(Fraction, '1/49 1/7 1 7 49'.split()))

    # 7 * 7 * 5 * 5 = 1225
    fracts = []
    for a2 in L2:
        for a3 in L3:
            for a5 in L5:
                for a7 in L7:
                    fracts.append(a2 * a3 * a5 * a7)
    # ≈1.4 million checks.
    for a in fracts:
        for b in fracts:
            # We rely on the asserts in fract_lcm to actually test!
            fract_lcm(a, b)

def qd_factor(n: int, hint: list[int] | None = None) -> list[int]:
    '''Quick and dirty prime factorisation.  If you know a large likely
    factor of n, then supply it in the hint list.'''
    assert n > 0
    factors = []
    if hint is not None:
        for f in hint:
            if n % f == 0:
                factors.append(f)
                n //= f
                while n % f == 0:
                    n //= f
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
    factors.sort()
    return factors
