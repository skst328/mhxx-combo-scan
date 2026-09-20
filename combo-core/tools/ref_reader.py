"""読み取りとクロスチェックの基準実装。

src/read.rs と src/cross.rs はこれと同じ結果を出さなければならない。
tests/golden.rs がそれを検証する。

必要なもの: pip install opencv-python numpy
"""
import itertools
import json
import os

import cv2
import numpy as np

YIELD_MIN, YIELD_MAX = 2, 4       # 生産数の範囲
CAP = 99                          # 所持上限
DEFAULT_BOX = (760, 242, 85, 32)  # Switch 2録画(1280x720)での「31/99」の位置
MIN_CONTRAST = 50                 # 枠内の明暗差がこれ未満なら「空白」とみなす
MAX_DIST = 0.20                   # 数字: 二値化後の不一致率がこれを超えたら「読めない」
MAX_DIST_MARK = 0.15              # 「/」「調合素材」: これを超えたら調合画面ではない


# ---------------------------------------------------------------- 読み取り部分

def binarize(g):
    """枠内の一番暗い所と明るい所の中間をしきい値にして 0/1 にする。
    明暗差が小さければ空白とみなして None"""
    lo, hi = float(g.min()), float(g.max())
    if hi - lo < MIN_CONTRAST:
        return None
    return (g > (lo + hi) / 2).astype(np.float32)


def load_templates(folder):
    layout = json.load(open(os.path.join(folder, "layout.json"), encoding="utf-8"))

    def load(n):
        t = cv2.imread(os.path.join(folder, f"{n}.png"), cv2.IMREAD_GRAYSCALE)
        return binarize(t.astype(np.float32))

    digits = {n: load(n) for n in "0123456789"}
    marks = {n: (layout[n], load(n)) for n in ("slash", "header")}
    return layout, digits, marks


def patch(frame, rect, pad=1):
    """rect の周り pad px を含めて切り出し、明るさ(色チャンネルの最大値)にする"""
    x, y, w, h = rect
    return frame[y - pad:y + h + pad, x - pad:x + w + pad].max(axis=2).astype(np.float32)


def best_match(g, w, h, tpls):
    """g(周囲1px込み)の中で±1pxずらしながら見本と比べ、(名前, 不一致率)"""
    best_n, best_d = None, 1.0
    for dy in (0, 1, 2):
        for dx in (0, 1, 2):
            b = binarize(g[dy:dy + h, dx:dx + w])
            if b is None:
                continue
            for n, t in tpls.items():
                d = float(np.abs(b - t).mean())
                if d < best_d:
                    best_n, best_d = n, d
    return best_n, best_d


def read_number(frame, slots, digits):
    """[10の位, 1の位] のスロットから数値を読む。読めなければ None"""
    chars = []
    for rect in slots:
        g = patch(frame, rect)
        _, _, w, h = rect
        if binarize(g[1:1 + h, 1:1 + w]) is None:
            chars.append(None)          # 空白
            continue
        n, d = best_match(g, w, h, digits)
        if d > MAX_DIST:
            return None
        chars.append(int(n))
    tens, ones = chars
    if ones is None:
        return None
    return ones + (tens or 0) * 10


def is_crafting(frame, box, marks):
    """「調合素材」の見出しと「/」が両方あるときだけ調合画面とみなす"""
    for name, base in (("header", (0, 0)), ("slash", box[:2])):
        (x, y, w, h), tpl = marks[name]
        rect = (base[0] + x, base[1] + y, w, h)
        if best_match(patch(frame, rect), w, h, {name: tpl})[1] > MAX_DIST_MARK:
            return False
    return True


# ---------------------------------------------------------------- クロスチェック部分

def material_value(m1, m2, prev):
    """素材1・素材2から素材の値を決める。食い違ったら前の値と辻褄が合う方"""
    cands = [m for m in (m1, m2) if m is not None]
    if not cands:
        return None
    if len(cands) == 2 and cands[0] != cands[1]:
        ok = [m for m in cands if prev is None or m <= prev]
        if len(ok) != 1:
            return None
        return ok[0]
    return cands[0]


def splits(total, k):
    """合計 total を k 回の調合(各2〜4個)に割り振る全パターン"""
    return [c for c in itertools.product(range(YIELD_MIN, YIELD_MAX + 1), repeat=k)
            if sum(c) == total]


def cross_check(rows):
    """素材の値が変わるごとに区切り、各区間での完成品の増加を調合回数に割り振る。
    戻り値: 区間のリスト, 調合1回ごとの記録のリスト, エラーのリスト"""
    # 素材の値ごとの区間にまとめる: [素材の値, 開始時刻, その区間の最後の完成品の値]
    segs, prev_m = [], None
    for t, m1, m2, p in rows:
        m = material_value(m1, m2, prev_m)
        if m is None:
            continue
        if prev_m is not None and m > prev_m:
            continue                     # 素材が増えることはないので読み間違い扱い
        if not segs or m != segs[-1][0]:
            segs.append([m, t, segs[-1][2] if segs else None])
        if p is not None:
            segs[-1][2] = p
        prev_m = m

    crafts, errors = [], []
    a = segs[0] if segs else None
    for b in segs[1:]:
        if a[2] is None:
            a = b
            continue
        if b[2] is None:
            continue                     # 完成品が読めていない区間は次とまとめる
        k = a[0] - b[0]                  # 素材の減り = 調合回数
        total = b[2] - a[2]
        if total == 0:
            # 素材は減ったのに完成品が変わっていない = 完成品の表示を見落とした。
            # (成功率100%前提なので失敗は考えない) 次の区間とまとめて割り振る
            continue
        if b[2] == CAP:
            # 上限到達: 99 - 直前の値 をその回の生産数とみなす
            cands = [(total,)] if k == 1 else None
            crafts.append({"t": b[1], "k": k, "total": total, "cands": cands, "note": "上限到達",
                           "before": a[2], "after": b[2]})
            a = b
            continue
        cands = splits(total, k)
        if not cands:
            errors.append(f"{b[1]:.2f}s 素材{k}個減・完成品+{total} は説明できません")
            crafts.append({"t": b[1], "k": k, "total": total, "cands": [], "note": "解析失敗",
                           "before": a[2], "after": b[2]})
        else:
            note = "" if k == 1 else ("見落とし補完" if len(cands) == 1 else "見落とし・候補複数")
            crafts.append({"t": b[1], "k": k, "total": total, "cands": cands, "note": note,
                           "before": a[2], "after": b[2]})
        a = b
    if a is not None and segs and a is not segs[-1]:
        errors.append(f"{segs[-1][1]:.2f}s 最後の調合の完成品の増加が読めていません")
    return segs, crafts, errors


def cumulative(c):
    """その区間の調合1回ごとの累計個数。途中が決まらない回は "??" """
    if c["cands"] and len(c["cands"]) == 1:
        out, v = [], c["before"]
        for y in c["cands"][0]:
            v += y
            out.append(f"{v:02d}")
        return out
    return ["??"] * (c["k"] - 1) + [f"{c['after']:02d}"]
