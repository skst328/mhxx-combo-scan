"""動画から ROI を切り出す。期待値は Rust 側 (examples/record.rs) が作る。

出力は動画 1 本につき 1 ディレクトリ:
    tests/cases/<名前>/input.json     ROI の位置と、コマごとの時刻
    tests/cases/<名前>/.raw/NNNN.png  全コマの ROI

`.raw` は作業用。`cargo run --release --example record` が、テストに要るコマだけを
`frames/` に残して消す。

PNG は 3 チャンネルの最大値を取った 1 チャンネル。read_frame が見ているのが
その値だけなので、色を持っても結果は変わらず容量が 3 倍になるだけ。

使い方:
    python tools/dump_cases.py
    python tools/dump_cases.py --movies path/to/movies
    cargo run --release --example record

必要なもの: pip install opencv-python
"""
import argparse
import json
import os
import shutil
import sys

import cv2

CRATE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# 動画を探す場所。置き場所が分かれていてもよいよう、全部を順に見る
MOVIE_DIRS = [
    os.path.join(CRATE, "movies"),
    os.path.join(CRATE, "reference", "mhxx-chougou", "movies"),
]

# src/read.rs の ROI と同じ値。ずれると Rust 側のテストが落ちる
ROI = (513, 49, 321, 221)

# (出力先のディレクトリ名, ファイル名)。名前はその録画が覆う場合分けを表す
VIDEOS = [
    # 通常。検索が一意に当たる
    ("clean-0", "20260919015837-01M2TQ71GD0DYYDF1G3APJDD29.mp4"),
    ("clean-1", "20260920023547-01M2WRAFYJS6BT4GVFA530R49R.mp4"),
    # 途中で乱数が 1 つ余分に進む。通常の検索では当たらず、診断が要る
    ("drift-0", "20260920023614-01M2WRB82FNSYXQ8XTM4YF57DC.mp4"),
    ("drift-1", "rng-unknown-0.mp4"),
    ("drift-2", "rng-unknown-1.mp4"),
    ("drift-3", "rng-unknown-3.mp4"),
    ("drift-4", "rng-misdetect-0.mp4"),
    # 調合開始直後に捨てる 3 回では足りない
    ("lead-skip-0", "rng-unknown-2.mp4"),
    # 前の調合の結果 (完成品が上限) が映ったあとに、別の調合が始まる
    ("restart-0", "start_misjudge.mp4"),
]


def find_movies(explicit):
    """動画の置き場所のうち実在するものを全部返す"""
    dirs = [d for d in ([explicit] if explicit else MOVIE_DIRS) if os.path.isdir(d)]
    if not dirs:
        sys.exit("動画が見つからない。--movies で置き場所を指定する。探した場所:\n  "
                 + "\n  ".join([explicit] if explicit else MOVIE_DIRS))
    return dirs


def locate(dirs, name):
    """動画 1 本の在処。どこにも無ければ None"""
    return next((p for d in dirs for p in [os.path.join(d, name)] if os.path.exists(p)), None)


def dump(path, out_dir):
    """全コマの ROI を .raw に書き出し、コマごとの時刻を返す"""
    rx, ry, rw, rh = ROI
    raw_dir = os.path.join(out_dir, ".raw")
    os.makedirs(raw_dir)

    cap = cv2.VideoCapture(path)
    fps = cap.get(cv2.CAP_PROP_FPS) or 30
    frames, i = [], 0
    while True:
        ok, frame = cap.read()
        if not ok:
            break
        png = f"{i:04d}.png"
        roi = frame[ry:ry + rh, rx:rx + rw]
        cv2.imwrite(os.path.join(raw_dir, png), roi.max(axis=2))
        frames.append({"index": i, "t": i / fps, "png": png})
        i += 1
    cap.release()
    return frames


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--movies", help="動画の置き場所")
    args = ap.parse_args()

    movies = find_movies(args.movies)
    cases_dir = os.path.join(CRATE, "tests", "cases")
    rx, ry, rw, rh = ROI

    for stem, name in VIDEOS:
        path = locate(movies, name)
        if path is None:
            print(f"skip (見つからない): {name}")
            continue
        # 古い PNG が残ると、消したはずのコマを照合し続けてしまう
        out_dir = os.path.join(cases_dir, stem)
        shutil.rmtree(out_dir, ignore_errors=True)
        os.makedirs(out_dir)

        frames = dump(path, out_dir)
        with open(os.path.join(out_dir, "input.json"), "w", encoding="utf-8") as f:
            json.dump({
                "video": name,
                "roi": {"x": rx, "y": ry, "width": rw, "height": rh},
                "frames": frames,
            }, f, ensure_ascii=False, indent=1)
        print(f"{stem}: {len(frames)}コマ")

    print("次に: cargo run --release --example record")


if __name__ == "__main__":
    main()
