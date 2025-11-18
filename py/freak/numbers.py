from math import gcd

__all__ = 'factorize',

SMALL_PRIMES = [
    2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71,
    73, 79, 83, 89, 97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151,
    157, 163, 167, 173, 179, 181, 191, 193, 197, 199, 211, 223, 227, 229, 233,
    239, 241, 251, 257, 263, 269, 271, 277, 281, 283, 293, 307, 311, 313, 317,
    331, 337, 347, 349, 353, 359, 367, 373, 379, 383, 389, 397, 401, 409, 419,
    421, 431, 433, 439, 443, 449, 457, 461, 463, 467, 479, 487, 491, 499, 503,
    509, 521, 523, 541, 547, 557, 563, 569, 571, 577, 587, 593, 599, 601, 607,
    613, 617, 619, 631, 641, 643, 647, 653, 659, 661, 673, 677, 683, 691, 701,
    709, 719, 727, 733, 739, 743, 751, 757, 761, 769, 773, 787, 797, 809, 811,
    821, 823, 827, 829, 839, 853, 857, 859, 863, 877, 881, 883, 887, 907, 911,
    919, 929, 937, 941, 947, 953, 967, 971, 977, 983, 991, 997]

SMALL_FACTOR_LIMIT = 1009 * 1009

def factorize(n: int) -> list[int]:
    assert n > 0
    factors: list[int] = []
    for p in SMALL_PRIMES:
        if n % p == 0:
            factors.append(p)
            n //= p
            while n % p == 0:
                n //= p
        if n < p * p:
            break
    if n >= SMALL_FACTOR_LIMIT:
        factor_set = set(factors)
        large_factors(factor_set, n)
        factors = list(factor_set)
        factors.sort()
    elif n > 1:
        factors.append(n)
    return factors

def large_factors(factors: set[int], n: int) -> None:
    while n >= SMALL_FACTOR_LIMIT:
        if miller_rabin_pseudo_prime(n):
            factors.add(n)
            return
        factor = pollard_ρ(n)
        if factor >= SMALL_FACTOR_LIMIT:
            large_factors(factors, factor)
        else:
            factors.add(factor)
        n //= factor
    if n > 1:
        factors.add(n)

def pollard_ρ(n: int) -> int:
    '''Pollard rho factorisation.'''
    # No particular reason to use primes here.  Just an arbitrary choice to
    # avoid thinking.
    for a in reversed(SMALL_PRIMES):
        slow = a
        fast = (slow * slow + a) % n
        count = 2
        while True:
            g = gcd(n, slow - fast)
            if g != 1:
                if g < n:
                    return g
                break
            if count & (count - 1) == 0:
                slow = fast
            fast = (fast * fast + a) % n
            count += 1
    assert False

def miller_rabin_pseudo_prime(n: int) -> bool:
    n = abs(n)
    if n < 3:
        return n == 2
    num_twos = ((n - 2) & -n).bit_length()
    untwo = (n - 1) >> num_twos
    assert untwo & 1 != 0
    assert untwo << num_twos == n - 1
    # We don't need primes here but it's a convenient selection.
    for p in SMALL_PRIMES[-6:]:
        if n % p == 0:
            return n == p
        l = pow(p, untwo, n)
        if l == 1 or l == n - 1:
            continue                    # Useless
        for _ in range(1, num_twos):
            l = l * l % n
            if l == 1:
                return False
            if l == n - 1:
                break                   # Useless
        else:
            return False
    return True

def test_small_primes() -> None:
    assert len(SMALL_PRIMES) == 168
    sqrt = 33
    sieve = bytearray(1000)
    assert len(sieve) <= sqrt * sqrt
    sieve[0] = 1
    sieve[1] = 1
    for i in range(2, sqrt):
        if sieve[i] == 0:
            for j in range(i * i, len(sieve), i):
                sieve[j] = 1
    primes = [i for i, f in enumerate(sieve) if f == 0]
    print(primes)
    assert SMALL_PRIMES == primes
    next_small, = factorize(SMALL_FACTOR_LIMIT)
    assert SMALL_FACTOR_LIMIT == next_small * next_small
    assert not any(next_small % p == 0 for p in SMALL_PRIMES)
    for n in range(SMALL_PRIMES[-1] + 1, next_small):
        assert any(n % p == 0 for p in SMALL_PRIMES)

def test_pollard_ρ() -> None:
    for n in (1 << 32) + 1, 1301119843216015234441:
        f = pollard_ρ(n)
        print(n, f, n % f, n // f)
        assert 1 < f < n
        assert n % f == 0

def test_miller_rabin() -> None:
    assert not miller_rabin_pseudo_prime(SMALL_FACTOR_LIMIT)
    assert miller_rabin_pseudo_prime(65537)
    assert not miller_rabin_pseudo_prime(1301119843216015234441)
    assert not miller_rabin_pseudo_prime((1 << 32) - 1)
    assert miller_rabin_pseudo_prime(2)
    assert not miller_rabin_pseudo_prime(1)
    # Check the asserts on some range of numbers...
    for i in range(2000000, 2001000):
        miller_rabin_pseudo_prime(i)

def test_factor() -> None:
    for n in 8, 65537, (1 << 32) - 1, 1301119843216015234441, \
            170141183460469232735155936644549397091, \
            2148696083 * 18446744556051857693:
        factors = factorize(n)
        print(n, factors)
        left = n
        for f in factors:
            assert miller_rabin_pseudo_prime(f)
            assert left % f == 0
            while left % f == 0:
                left //= f
        assert left == 1

if __name__ == '__main__':
    from math import sqrt
    gold = (1 + sqrt(5)) / 2
    f32 = int(round(next(filter(lambda x: x < 1e7, # pyright: ignore
                                (32e6 / (gold + i) for i in range(10))))))
    f64 = int(round(next(filter(lambda x: x < 1e7, # pyright: ignore
                                (64e6 / (gold + i) for i in range(10))))))
    for f in f32, f64:
        print(f, factorize(f))
        for i in range(1, 40):
            print(f - i, factorize(f - i))
            print(f + i, factorize(f + i))

