"""基準実装の速度を測る。Rust 側の tests/golden.rs の bench と同じ条件で比べる。

読み取りは、先に全コマをメモリに載せてから計る (動画のデコードは対象外)。
検索は numba の JIT を済ませてから計る。

使い方:
    python tools/bench.py
    python tools/bench.py --movies path/to/movies

必要なもの: pip install opencv-python numpy
"""
import argparse
import json
import os
import time

import cv2

import ref_reader as ref
import ref_rng
from dump_golden import CRATE, find_movies

ROUNDS = 20


def load_frames(path, start, end, limit):
    cap = cv2.VideoCapture(path)
    fps = cap.get(cv2.CAP_PROP_FPS) or 30
    i, last = int(start * fps), int(end * fps)
    cap.set(cv2.CAP_PROP_POS_FRAMES, i)
    out = []
    while i <= last and len(out) < limit:
        ok, frame = cap.read()
        if not ok:
            break
        out.append(frame)
        i += 1
    cap.release()
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--movies", help="動画の置き場所")
    args = ap.parse_args()

    movies = find_movies(args.movies)
    layout, digits, marks = ref.load_templates(os.path.join(CRATE, "templates"))
    box = ref.DEFAULT_BOX
    x, y = box[:2]
    prod_slots = [(x + sx, y + sy, w, h) for sx, sy, w, h in layout["slots"]]
    mat_slots = layout["mat_slots"]

    golden_dir = os.path.join(CRATE, "tests", "golden")
    goldens = [json.load(open(os.path.join(golden_dir, f), encoding="utf-8"))
               for f in sorted(os.listdir(golden_dir)) if f.endswith(".json")]

    frames = []
    for g in goldens:
        path = os.path.join(movies, g["video"])
        if os.path.exists(path):
            frames += load_frames(path, g["start"], g["end"], len(g["rows"]))
    if not frames:
        print("動画が読めなかった")
        return

    t0 = time.perf_counter()
    crafting = 0
    for _ in range(ROUNDS):
        for f in frames:
            if not ref.is_crafting(f, box, marks):
                continue
            crafting += 1
            ref.read_number(f, mat_slots[0], digits)
            ref.read_number(f, mat_slots[1], digits)
            ref.read_number(f, prod_slots, digits)
    elapsed = time.perf_counter() - t0
    per = elapsed / (ROUNDS * len(frames))
    print(f"読み取り: {per * 1e6:.0f} us/コマ  "
          f"({len(frames)} コマ x {ROUNDS} 回 = {elapsed:.2f} 秒, crafting {crafting})")

    target = next((g for g in goldens if g["frames"]), None)
    if target is None:
        print("フレームが見つかっている golden が無いので検索は測らない")
        return
    cum = " ".join("??" if v is None else f"{v:02d}" for v in target["cumulative"])
    ref_rng.search_combo(0, 1000, cum)

    t0 = time.perf_counter()
    hits = ref_rng.search_combo(0, 10 ** 7, cum)
    elapsed = time.perf_counter() - t0
    print(f"検索: {elapsed:.2f} 秒 / 10^7 ステップ  "
          f"({10 / elapsed:.0f} M/秒, hits {list(hits)})")


if __name__ == "__main__":
    main()
