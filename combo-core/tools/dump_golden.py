"""基準実装の出力を Rust のテスト用データとして書き出す。

出力:
    tests/golden/<名前>.json   コマごとの読み取り結果・累計・見つかったフレーム
    tests/fixtures/            代表的なコマの ROI と期待値
    tests/golden/frames/*.png  ROI の全コマ (--all-frames のとき)

使い方:
    python tools/dump_golden.py
    python tools/dump_golden.py --all-frames
    python tools/dump_golden.py --movies path/to/movies

必要なもの: pip install opencv-python numpy
"""
import argparse
import json
import os
import shutil
import sys

import cv2

import ref_reader as ref
import ref_rng

CRATE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 動画を探す場所。先に見つかったほうを使う
MOVIE_DIRS = [
    os.path.join(CRATE, "movies"),
    os.path.join(CRATE, "reference", "mhxx-chougou", "movies"),
]

# src/read.rs の ROI と同じ値。ずれると Rust 側のテストが落ちる
ROI = (513, 49, 321, 221)

# 動画ごとの解析区間 (ファイル名, 開始秒, 終了秒)
VIDEOS = [
    ("20260919015837-01M2TQ71GD0DYYDF1G3APJDD29.mp4", 23, 28.5),
    ("20260920023547-01M2WRAFYJS6BT4GVFA530R49R.mp4", 19.5, 28.5),
    ("20260920023614-01M2WRB82FNSYXQ8XTM4YF57DC.mp4", 23, 28.5),
]

# 代表コマの上限
MAX_FIXTURES = 24

ROW_KEYS = ("t", "crafting", "material1", "material2", "product")


def read_rows(path, start, end, layout, digits, marks):
    """コマごとに読み取り、ROI を切り出した画像を添えて返す。
    調合画面でないコマも残す。完成品が上限に達したら終了"""
    box = ref.DEFAULT_BOX
    x, y = box[:2]
    prod_slots = [(x + sx, y + sy, w, h) for sx, sy, w, h in layout["slots"]]
    mat_slots = layout["mat_slots"]
    rx, ry, rw, rh = ROI

    cap = cv2.VideoCapture(path)
    fps = cap.get(cv2.CAP_PROP_FPS) or 30
    i, last = int(start * fps), int(end * fps)
    cap.set(cv2.CAP_PROP_POS_FRAMES, i)

    rows = []
    while i <= last:
        ok, frame = cap.read()
        if not ok:
            break
        t = i / fps
        i += 1
        row = {"t": t, "crafting": False, "material1": None, "material2": None, "product": None}
        if ref.is_crafting(frame, box, marks):
            row["crafting"] = True
            row["material1"] = ref.read_number(frame, mat_slots[0], digits)
            row["material2"] = ref.read_number(frame, mat_slots[1], digits)
            row["product"] = ref.read_number(frame, prod_slots, digits)
        # 全コマぶんの元画像を持つとメモリが足りないので ROI だけ残す
        row["crop"] = frame[ry:ry + rh, rx:rx + rw].copy()
        rows.append(row)
        if row["product"] == ref.CAP:
            break
    cap.release()
    return rows


def analyze(rows):
    """累計の並びとエラーを返す。決まらない箇所は None"""
    craft_rows = [(r["t"], r["material1"], r["material2"], r["product"])
                  for r in rows if r["crafting"]]
    _, crafts, errors = ref.cross_check(craft_rows)
    if not crafts:
        return [], errors
    cum = [f"{crafts[0]['before']:02d}"] + [v for c in crafts for v in ref.cumulative(c)]
    return [None if v == "??" else int(v) for v in cum], errors


def features(row):
    """そのコマが覆っている読み取りの場合分け"""
    out = {("crafting", row["crafting"])}
    if not row["crafting"]:
        return out
    # 上限に達した数字は暗い赤で表示され、明暗差が白い数字の半分以下になる
    if row["product"] == ref.CAP:
        out.add(("cap",))
    if row["material1"] != row["material2"]:
        out.add(("materials_disagree",))
    for name in ("material1", "material2", "product"):
        v = row[name]
        if v is None:
            out.add(("unreadable", name))
            continue
        out.add(("digit", name, "tens", "blank" if v < 10 else str(v // 10)))
        out.add(("digit", name, "ones", str(v % 10)))
    return out


def pick_fixtures(candidates):
    """場合分けを一番多く新しく覆うコマから順に取る (貪欲な集合被覆)"""
    covered, picked = set(), []
    remaining = list(candidates)
    while remaining and len(picked) < MAX_FIXTURES:
        best = max(remaining, key=lambda c: len(features(c["row"]) - covered))
        gained = features(best["row"]) - covered
        if not gained:
            break
        covered |= gained
        picked.append(best)
        remaining.remove(best)
    picked.sort(key=lambda c: (c["stem"], c["index"]))
    return picked, covered


def changed_indices(rows):
    """読み取り結果が前のコマから変わった位置"""
    out, prev = [], None
    for i, r in enumerate(rows):
        key = tuple(r[k] for k in ROW_KEYS[1:])
        if key != prev:
            out.append(i)
            prev = key
    return out


def find_movies(explicit):
    for d in [explicit] if explicit else MOVIE_DIRS:
        if os.path.isdir(d):
            return d
    sys.exit("動画が見つからない。--movies で置き場所を指定する。探した場所:\n  "
             + "\n  ".join([explicit] if explicit else MOVIE_DIRS))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--all-frames", action="store_true", help="ROI を全コマ書き出す")
    ap.add_argument("--movies", help="動画の置き場所")
    args = ap.parse_args()

    movies = find_movies(args.movies)
    layout, digits, marks = ref.load_templates(os.path.join(CRATE, "templates"))
    golden_dir = os.path.join(CRATE, "tests", "golden")
    frame_dir = os.path.join(golden_dir, "frames")
    fixture_dir = os.path.join(CRATE, "tests", "fixtures")
    os.makedirs(frame_dir, exist_ok=True)
    shutil.rmtree(fixture_dir, ignore_errors=True)
    os.makedirs(fixture_dir)

    rx, ry, rw, rh = ROI
    roi_doc = {"x": rx, "y": ry, "width": rw, "height": rh}
    candidates = []

    for name, start, end in VIDEOS:
        path = os.path.join(movies, name)
        if not os.path.exists(path):
            print(f"skip (見つからない): {name}")
            continue
        stem = name.split("-")[0]
        rows = read_rows(path, start, end, layout, digits, marks)
        cumulative, errors = analyze(rows)

        frames = []
        if cumulative and None not in cumulative[3:]:
            cum_str = " ".join("??" if v is None else f"{v:02d}" for v in cumulative)
            frames = ref_rng.search_combo(0, 10 ** 7, cum_str)

        wanted = range(len(rows)) if args.all_frames else changed_indices(rows)
        saved = []
        for i in wanted:
            png = f"{stem}-{i:04d}.png"
            cv2.imwrite(os.path.join(frame_dir, png), rows[i]["crop"])
            saved.append({"index": i, "png": png})

        with open(os.path.join(golden_dir, f"{stem}.json"), "w", encoding="utf-8") as f:
            json.dump({
                "video": name,
                "start": start,
                "end": end,
                "roi": roi_doc,
                "rows": [{k: r[k] for k in ROW_KEYS} for r in rows],
                "cumulative": cumulative,
                "errors": errors,
                "frames": frames,
                "saved_frames": saved,
            }, f, ensure_ascii=False, indent=1)
        print(f"{stem}: {len(rows)}コマ 累計{len(cumulative)}個 "
              f"フレーム{frames} ROI保存{len(saved)}枚 エラー{len(errors)}件")

        candidates += [{"stem": stem, "index": i, "row": r} for i, r in enumerate(rows)]

    if not candidates:
        print("動画が 1 本も読めなかったので fixtures は作らなかった")
        return

    picked, covered = pick_fixtures(candidates)
    entries = []
    for c in picked:
        png = f"{c['stem']}-{c['index']:04d}.png"
        cv2.imwrite(os.path.join(fixture_dir, png), c["row"]["crop"])
        entries.append({"png": png, **{k: c["row"][k] for k in ROW_KEYS}})
    with open(os.path.join(fixture_dir, "expected.json"), "w", encoding="utf-8") as f:
        json.dump({"roi": roi_doc, "frames": entries}, f, ensure_ascii=False, indent=1)

    size = sum(os.path.getsize(os.path.join(fixture_dir, e["png"])) for e in entries)
    print(f"fixtures: {len(entries)}枚 {size // 1024}KB ({len(covered)}通りの場合分けを網羅)")


if __name__ == "__main__":
    main()
