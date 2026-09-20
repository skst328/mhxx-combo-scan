"""
調合数の並びから、調合開始時の乱数位置(フレーム)を特定する基準実装。

apmnnn/mhxx-rng (https://github.com/apmnnn/mhxx-rng) の mhxx-rng.ipynb から、
search_combo とその動作に必要な部分だけを切り出したもの。
アルゴリズムは元のコードのまま変えていない。

src/rng.rs と src/search.rs はこれと同じ結果を出さなければならない。
tests/golden.rs がそれを検証する。

    from ref_rng import search_combo, watch
    frames = search_combo(0, 10**7, "00 04 07 ...")

numba が入っていれば高速に動く (pip install numba)。無くても動くが遅い。

------------------------------------------------------------------------------
Original work: MIT License

Copyright (c) 2026 apmnnn

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
------------------------------------------------------------------------------
"""
import sys
import numpy as np

try:
    from numba import njit
    from numba.typed import List
    HAS_NUMBA = True
except ImportError:          # numba が無ければ普通の Python として動かす
    HAS_NUMBA = False
    List = list
    def njit(f):
        return f

# 乱数の初期値
SEED = (0x0194FD72, 0x79E6C985, 0x08DD9701, 0x41CFCE91)


# ---------------------------------------------------------------- 乱数

def ascend(s):
    """乱数を1つ進める"""
    x, y, z, w = s
    t = (x ^ (x << 15)) & 0xFFFFFFFF
    return (y, z, w, w ^ (w >> 21) ^ t ^ (t >> 4))


def poly_mul(p1, p2):
    res = 0
    while p2 > 0:
        if p2 & 1:
            res ^= p1
        p1 <<= 1
        p2 >>= 1
    return res


def poly_mod(p, m):
    m_len = m.bit_length()
    while (delta_deg := p.bit_length() - m_len) >= 0:
        p ^= m << delta_deg
    return p


def poly_pow_mod(base, exp, mod):
    res = 1
    base = poly_mod(base, mod)
    while exp > 0:
        if exp & 1:
            res = poly_mod(poly_mul(res, base), mod)
        base = poly_mod(poly_mul(base, base), mod)
        exp >>= 1
    return res


def jump(frame):
    """初期値から frame 回進めた乱数の状態を、多項式を使って一気に求める"""
    # r(x) = x^n mod f(x)
    r_poly = poly_pow_mod(0b10, frame % (2 ** 128 - 1), 0x100000201a8362f671442057eea368001)
    # v_n = A^n(v_0) = r(A)(v_0)
    s = SEED
    acc = [0, 0, 0, 0]
    while r_poly > 0:
        if r_poly & 1:
            acc = [a ^ v for a, v in zip(acc, s)]
        r_poly >>= 1
        s = ascend(s)
    return tuple(acc)


def watch(f):
    """フレーム数を 日 時 分 秒 フレーム の表記にする (30fps)"""
    return "{0}d {1}h {2}m {3}s {4}f".format(
        f // 2592000, (f % 2592000) // 108000, (f % 108000) // 1800, (f % 1800) // 30, f % 30)


# ---------------------------------------------------------------- 検索本体

@njit
def ascend_nj(js):
    jx, jy, jz, jw, jt, jf = js
    jt = (jx ^ (jx << 15)) & 0xFFFFFFFF
    jx = jy
    jy = jz
    jz = jw
    jw = jw ^ (jw >> 21) ^ jt ^ (jt >> 4)
    jf += 1
    return (jx, jy, jz, jw, jt, jf)


@njit
def kmp_prefix_nj(A):
    n = len(A)
    pi = np.zeros(n, dtype=np.int64)
    j = 0
    for i in range(1, n):
        while j > 0 and A[i] != A[j]:
            j = pi[j - 1]
        if A[i] == A[j]:
            j += 1
        pi[i] = j
    return pi


@njit
def search_stride_nj(step, js, A, stride, lut):
    # 調合の乱数列からAに一致する列をstride飛びで検索
    n_A = len(A)
    res = List()
    if n_A == 0:
        return res
    pi = kmp_prefix_nj(A)
    state = np.zeros(stride, dtype=np.int64)
    r = 0
    for i in range(step):
        x = lut[(js[3] & 0xFFFF) % 100]
        js = ascend_nj(js)
        k = state[r]
        while k > 0 and A[k] != x:
            k = pi[k - 1]
        if A[k] == x:
            k += 1
        if k == n_A:
            res.append(i - stride * (n_A - 1))
            k = pi[n_A - 1]
        state[r] = k
        r += 1
        if r == stride:
            r = 0
    return res


def combo_lookuptable():
    """乱数(0〜99)→生産数: 0〜24は2個、25〜74は3個、75〜99は4個"""
    lut = np.zeros(100, dtype=np.uint8)
    lut[:25] = 2
    lut[25:75] = 3
    lut[75:] = 4
    return lut


def invalid_differences(dif_A):
    return [i for i, d in enumerate(dif_A) if d not in (2, 3, 4)]


def search_combo(start, step, combo_str):
    """累計個数の並び(例: "00 04 07 ... 99")から、調合開始時の乱数位置を探す。
    見つかったフレームのリストを返す。並びが不正なら ValueError"""
    x, y, z, w = jump(start)

    raw_A = list(map(int, combo_str.split()))

    # 先頭の3つ、末尾の99をカット
    pos_A = raw_A[3:]
    if pos_A and pos_A[-1] == 99:
        pos_A = pos_A[:-1]
    if len(pos_A) <= 1:
        raise ValueError("too short!")

    dif_A = [pos_A[i + 1] - pos_A[i] for i in range(len(pos_A) - 1)]
    bad = invalid_differences(dif_A)
    if bad:
        raise ValueError(f"invalid differences! (位置: {[i + 4 for i in bad]})")

    A = np.array(dif_A, dtype=np.uint8)
    hits = search_stride_nj(step, (x, y, z, w, 0, 0), A, 5, combo_lookuptable())

    # カットした3回分、初回調合の遅延、各調合による進行2の補正
    return [start + i - 5 * 3 - 15 + 2 * (len(raw_A) - 1) for i in hits]
