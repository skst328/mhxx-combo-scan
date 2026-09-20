# 開発

使い方は [README](../README.md)、アルゴリズムの解説は [how-it-works.md](how-it-works.md) を参照。
ここではビルドとテストの手順を扱う。

## 構成

| | |
|---|---|
| デコード・切り出し・画面 | TypeScript (React + Vite + Tailwind + shadcn/ui) |
| 画像認識・クロスチェック・乱数検索 | Rust → WebAssembly (`combo-core/`) |

重い処理は 2 つの Worker に分けてある。メインスレッドは進捗を受け取って描くだけ。

```
src/workers/analyze.worker.ts   mp4 のデコード → ROI 切り出し → wasm で読み取り
src/workers/search.worker.ts    乱数列の検索
```

座標もテンプレートも Rust 側だけが持つ。TS は `Session.roi()` が指す矩形
（1280x720 のうち 321x221）を切り出して渡すだけで、画面のレイアウトを知らない。

```
combo-core/src/
  lib.rs        wasm との境界だけ (Session, FrameSearcher)
  types.rs      TS とやり取りする型。tsify が .d.ts を生成する
  read.rs       1 コマの読み取り。前後のコマを参照しない
  templates.rs  二値化済みの見本 (実体は build.rs が生成)
  cross.rs      素材の減りと完成品の増えの突き合わせ
  rng.rs        xorshift128 と jump
  search.rs     KMP による逐次検索
```

## ビルド

```bash
pnpm install

cd combo-core
wasm-pack build --target web    # combo-core/pkg/ を作る。先に必要
cd ..

pnpm dev
pnpm build
```

`combo-core/pkg/` は git 管理外なので、**クローン直後は必ず `wasm-pack build` を先に実行する。**
`src/App.tsx` がここを直接 import している。

Rust を変更したら `wasm-pack build` を再実行する。

## テスト

Rust の実装は、Python で書かれた基準実装と同じ結果を出さなければならない。

| | |
|---|---|
| `combo-core/tools/ref_reader.py` | 読み取りとクロスチェックの基準 |
| `combo-core/tools/ref_rng.py` | 乱数位置の検索の基準 |

その出力を固定したものが `tests/golden/` と `tests/fixtures/` にあり、`cargo test` は
これと突き合わせる。Python も動画も要らない。

```bash
cd combo-core
cargo test
```

| テスト | 内容 | 使うデータ |
|---|---|---|
| 単体 26 件 | `src/*.rs` の `#[cfg(test)]` | なし |
| `cross_check_matches_reference` | 累計の並びが基準実装と一致するか | `tests/golden/*.json` |
| `search_matches_reference` | 同じフレーム位置を出すか | 同上 |
| `read_frame_matches_fixtures` | 代表コマの読み取りが一致するか | `tests/fixtures/` (15 コマ) |
| `read_frame_matches_all_frames` | 同上を全コマに広げた版 | `tests/golden/frames/` (539 コマ) |

`tests/golden/frames/` は 36MB あるので git 管理外。無い場合 `read_frame_matches_all_frames`
は何も検査せずに通る。手元で全コマを確かめたいときは `--all-frames` で作り直す。

速度の計測は `#[ignore]` を付けてあるので、明示しないと走らない。

```bash
cargo test --release -- --ignored --nocapture   # bench
```

## 照合データを作り直す

ROI を変えたときや動画を足したときだけ必要。`combo-core/movies/` に動画を置き、
cv2 の入った Python で実行する。

```bash
cd combo-core
python tools/dump_golden.py               # golden と fixtures
python tools/dump_golden.py --all-frames  # 全コマの ROI も (36MB)
python tools/bench.py                     # 基準実装との速度比較
```

動画を足すときは `tools/dump_golden.py` の `VIDEOS` に
`(ファイル名, 開始秒, 終了秒)` を追記する。`cargo test` は `tests/golden/*.json` を
すべて読むので、Rust 側の変更は要らない。

`tools/ref_*.py` は基準そのものなので、**Rust に合わせて書き換えない。**
両方を同時に変えるとテストが通ってしまう。

## アイコン

`public/favicon.svg` が元データ。ホーム画面用の PNG は、角丸を外して実寸を指定してから
書き出す（SVG のまま拡大するとぼやける）。

```bash
for s in 180 192 512; do
  sed "s|<svg |<svg width=\"$s\" height=\"$s\" |; s| rx=\"7\"||" public/favicon.svg > /tmp/sq.svg
  convert -background none /tmp/sq.svg -strip -colors 32 -define png:compression-level=9 out-$s.png
done
```

## 前提と割り切り

| 前提 | 外れたらどうなるか |
|---|---|
| 1280x720・30fps の録画 | 枠の位置が合わず読めない。TS 側で弾いている |
| 調合パネルの位置は常に同じ | 座標は決め打ち。±1px のズレは吸収する |
| 成功率 100% の調合 | 失敗は「解析エラー」になる |
| 素材は 1 回に 1 個ずつ減る | 調合回数の数え方の根拠 |
| 生産数は 2〜4 個 | 変える場合は `cross.rs` と `ref_reader.py` の両方を直す |
