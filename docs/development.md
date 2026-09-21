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
  read.rs       1 コマの読み取り。上限到達の判定だけ前のコマを見る
  templates.rs  二値化済みの見本 (実体は build.rs が生成)
  cross.rs      素材の減りと完成品の増えの突き合わせ
  rng.rs        xorshift128 と jump
  search.rs     KMP による逐次検索と、ずれの診断
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
`src/App.tsx` と 2 つの Worker がここを直接 import している。

Rust を変更したら `wasm-pack build` を再実行する。

## テスト

```bash
cd combo-core
cargo test
```

Python も動画も要らない。検査は 3 層あり、**保証の強さが違う。**

| 層 | 何を保証するか |
|---|---|
| 単体テスト (`src/*.rs` の `#[cfg(test)]`) | 作った入力に対して正しい答えを出すこと。乱数の性質、仕込んだずれの復元、区間分けの場合分けなど。記録には依存しない |
| 乱数列との突き合わせ | 生産数の並びが、ゲームの乱数列とただ 1 箇所で一致すること |
| 録画に対する回帰 (`*_matches_recorded`) | 前と同じ結果を出すこと。それだけ |

2 層目がいちばん強い。生産数 1 個の情報量は 1.5 bit で、差分が 30 個ほどあれば 45 bit。
10<sup>6</sup> (20 bit) を探して偶然 1 箇所に当たる確率は 2<sup>-25</sup> 程度しかない。
**画素の読み取りが 1 個でも違えばこの一致は起きない**ので、画像認識から乱数検索まで
一式の正しさをゲームの乱数列が裏付けていることになる。

ただしこれが効くのは**通常の検索が当たるケースだけ**。乱数がずれている録画は位置が
一意に決まらないので、3 層目しかかからない。

3 層目の期待値はこの実装自身が出したものなので、**正しさは保証しない。**
挙動を変えたら期待値を作り直し、**差分を目で見て意図と合っているかを確かめる**必要がある。

### 照合データの持ち方

テストケース 1 本につき 1 ディレクトリ。名前はその録画が覆う場合分けを表す。

```
tests/cases/drift-1/
    expected.json   コマごとの読み取り結果・累計・フレーム位置・診断
    frames/0000.png そのコマの ROI
```

どのディレクトリが何を覆っているかは `tools/dump_cases.py` の `VIDEOS` に書いてある。

層によって要るものが違う。

- **`read_frame` は画素そのものを受け取る**ので、テストにも本物の画素が要る。
  それが `frames/*.png`。PNG なのは `templates/*.png` と同じ形式にするため
  (`build.rs` が PNG をデコードして数字のテンプレートを埋め込んでいる)。
- **`cross_check` と `Searcher` と `diagnose` は画素を見ない。** 読み取り済みの数値だけで
  動くので、`expected.json` の `rows` と `cumulative` があれば足りる。

PNG は **3 チャンネルの最大値を取った 1 チャンネル**で持つ。`read_frame` が見ているのが
その値だけ (`read.rs` の `rgba[i].max(rgba[i+1]).max(rgba[i+2])`) なので、
色を持っても結果は変わらず容量が 3 倍になる。

残すのは**読み取り結果が前のコマから変わったコマだけ**。読み取りが同じコマを足しても
通る場合分けは増えないので、それで必要十分になる。1 本あたり 70 枚ほど、2MB 弱。

動画は頭から、完成品が上限に達するまで読む。調合していない区間は読み取り結果が
同じなので ROI が 1 枚に畳まれ、範囲を絞る必要がない。

### 速度の計測

`#[ignore]` を付けてあるので、名前を指定しないと走らない。

```bash
cd combo-core
cargo test --release --test cases -- --ignored bench --nocapture
```

## 照合データを作り直す

ROI や解析の挙動を変えたとき、動画を足したときに必要。動画が要る。

動画のデコードに OpenCV が要るので、**切り出しだけ Python が担う。期待値は Rust が出す。**

```bash
cd combo-core
python tools/dump_cases.py    # 動画 -> 全コマの ROI (作業用の .raw)
cargo test --release --test cases -- --ignored record_cases --nocapture
```

2 つ目が、読み取り結果が変わったコマだけを `frames/` に残して `.raw` と `input.json` を
片付け、`expected.json` を書く。`.raw` は 10 倍ほど嵩むので、必ず 2 つ目まで走らせる。

**書いたあとは `git diff` で差分を必ず見る。** 期待値はこの実装自身が出したものなので、
バグを入れてもテストは通る。差分を見るのが唯一の歯止めになる。

動画を足すときは `tools/dump_cases.py` の `VIDEOS` に `(ディレクトリ名, ファイル名)` を
追記するだけでよい。`cargo test` は `tests/cases/` 直下のディレクトリをすべて読むので、
Rust 側の変更は要らない。動画の置き場所は `MOVIE_DIRS` に並べたものを順に探す。

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
| 生産数は 2〜4 個 | 変える場合は `cross.rs` の `YIELD_MIN` / `YIELD_MAX` |
