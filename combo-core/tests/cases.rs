//! 録画から作ったテストケースで、解析の一式が前と同じ結果になることを確かめる。
//!
//! テストケースは `tests/cases/<名前>/` に 1 本ずつ。名前はその録画が覆う場合分けを表す。
//!
//! | 置き場所 | 中身 |
//! |---|---|
//! | `tests/cases/<名前>/expected.json` | コマごとの読み取り結果・累計・フレーム位置・診断 |
//! | `tests/cases/<名前>/frames/*.png` | そのコマの ROI (1 チャンネル) |
//!
//! **これは回帰のテストで、正しさの証明ではない。** 期待値はこの実装自身が出したもので、
//! 照合できるのは「前と同じか」だけ。値を変えたときは、変えた意図と合うかを自分で見る。
//!
//! 作り直しは 2 段階。動画のデコードに OpenCV が要るので、切り出しだけ Python が担う。
//!
//! ```text
//! python tools/dump_cases.py                     # 動画 -> 全コマの ROI
//! cargo test --release --test cases -- --ignored record_cases --nocapture
//! ```

use std::path::{Path, PathBuf};

use combo_core::types::{Analysis, Diagnosis, FrameReading};
use combo_core::{ROI, Searcher, cross_check, diagnose, read_frame, reached_cap};
use serde::{Deserialize, Serialize};

/// 探す範囲。期待値に載っているフレーム位置はすべてこの中に入る
const SEARCH_STEPS: u64 = 1_000_000;

#[derive(Deserialize, Serialize, Clone, Copy)]
struct RoiDoc {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Deserialize, Serialize)]
struct Row {
    t: f64,
    crafting: bool,
    material1: Option<u8>,
    material2: Option<u8>,
    product: Option<u8>,
}

#[derive(Deserialize, Serialize)]
struct Saved {
    index: usize,
    png: String,
}

/// `tools/dump_cases.py` が書くもの
#[derive(Deserialize)]
struct Input {
    video: String,
    roi: RoiDoc,
    frames: Vec<InputFrame>,
}

#[derive(Deserialize)]
struct InputFrame {
    index: usize,
    t: f64,
    png: String,
}

/// `record_cases` が書くもの
#[derive(Deserialize, Serialize)]
struct Expected {
    video: String,
    roi: RoiDoc,
    rows: Vec<Row>,
    cumulative: Vec<Option<u8>>,
    frames: Vec<i64>,
    diagnosis: Option<Diagnosis>,
    saved_frames: Vec<Saved>,
}

struct Case {
    dir: PathBuf,
    expected: Expected,
}

impl Row {
    fn as_reading(&self) -> FrameReading {
        FrameReading {
            t: self.t,
            crafting: self.crafting,
            material1: self.material1,
            material2: self.material2,
            product: self.product,
            // 上限到達は 1 コマでは決まらないので、読み取りの比較には含めない
            done: false,
        }
    }

    fn of(r: &FrameReading) -> Self {
        Self {
            t: r.t,
            crafting: r.crafting,
            material1: r.material1,
            material2: r.material2,
            product: r.product,
        }
    }

    /// 読み取り結果が同じなら、ROI を足しても通る場合分けは増えない
    fn same_reading(&self, other: &Row) -> bool {
        (self.crafting, self.material1, self.material2, self.product)
            == (other.crafting, other.material1, other.material2, other.product)
    }
}

impl Case {
    fn name(&self) -> String {
        self.dir.file_name().unwrap().to_string_lossy().into_owned()
    }

    fn png(&self, saved: &Saved) -> PathBuf {
        self.dir.join("frames").join(&saved.png)
    }
}

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases")
}

/// `tests/cases/` 直下で、名前の付いたファイルを持つディレクトリを順に返す
fn case_dirs(file: &str) -> Vec<PathBuf> {
    let root = cases_dir();
    let mut out: Vec<PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join(file).is_file())
        .collect();
    out.sort();
    out
}

fn load_all() -> Vec<Case> {
    let dirs = case_dirs("expected.json");
    assert!(!dirs.is_empty(), "テストケースが無い。tools/dump_cases.py と record_cases を実行する");
    dirs.into_iter()
        .map(|dir| {
            let path = dir.join("expected.json");
            let text = std::fs::read_to_string(&path).unwrap();
            let expected = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            Case { dir, expected }
        })
        .collect()
}

/// 1 チャンネルの PNG を RGBA に広げる。
/// read_frame は 3 チャンネルの最大値しか見ないので、同じ値を 3 つ並べれば元と同じ
fn load_roi_rgba(path: &Path) -> Vec<u8> {
    let file = std::io::BufReader::new(
        std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    );
    let mut reader = png::Decoder::new(file).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.width, ROI.width);
    assert_eq!(info.height, ROI.height);
    assert_eq!(info.color_type, png::ColorType::Grayscale);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    buf[..info.buffer_size()].iter().flat_map(|&v| [v, v, v, 255]).collect()
}

/// ROI を残したコマの読み取りが記録と一致する
#[test]
fn read_frame_matches_recorded() {
    let mut checked = 0;
    for c in load_all() {
        let e = &c.expected;
        // ROI を変えたら PNG も作り直さないといけない
        assert_eq!(
            (e.roi.x, e.roi.y, e.roi.width, e.roi.height),
            (ROI.x, ROI.y, ROI.width, ROI.height),
            "{}: ROI が変わっている。作り直す",
            c.name()
        );
        assert!(!e.saved_frames.is_empty(), "{}: ROI が 1 枚も無い", c.name());

        for saved in &e.saved_frames {
            let want = e.rows[saved.index].as_reading();
            let got = read_frame(want.t, &load_roi_rgba(&c.png(saved))).unwrap();
            assert_eq!(got, want, "{}: {}", c.name(), saved.png);
            checked += 1;
        }
    }
    eprintln!("{checked} コマを照合した");
}

/// クロスチェックの結果が記録と一致する
#[test]
fn cross_check_matches_recorded() {
    for c in load_all() {
        let e = &c.expected;
        let rows: Vec<FrameReading> = e.rows.iter().map(Row::as_reading).collect();
        let analysis: Analysis = cross_check(&rows);
        assert_eq!(analysis.cumulative, e.cumulative, "{}: 累計が違う", c.name());
        assert_eq!(
            analysis.total_crafts() as usize,
            e.cumulative.len().saturating_sub(1),
            "{}: 調合回数が累計の個数と合わない",
            c.name()
        );
    }
}

/// 乱数検索の結果が記録と一致する
#[test]
fn search_matches_recorded() {
    for c in load_all() {
        let e = &c.expected;
        let hits = match Searcher::new(&e.cumulative, 0) {
            Ok(mut s) => s.step(SEARCH_STEPS),
            Err(err) => {
                assert!(e.frames.is_empty(), "{}: {:?} を記録している ({err})", c.name(), e.frames);
                continue;
            }
        };
        assert_eq!(hits, e.frames, "{}: 検索の結果が違う", c.name());
    }
}

/// ずれの診断の結果が記録と一致する
#[test]
fn diagnose_matches_recorded() {
    for c in load_all() {
        let e = &c.expected;
        let got = diagnose(&e.cumulative, 0, SEARCH_STEPS).unwrap_or_else(|err| panic!("{}: {err}", c.name()));
        assert_eq!(got, e.diagnosis, "{}: 診断の結果が違う", c.name());

        // 通常の検索で見つかるなら、診断もずれ無しで同じ位置を指すこと。
        // 記録どうしの突き合わせではないので、ここは本物の検査になる
        for f in &e.frames {
            let d = got.as_ref().unwrap_or_else(|| panic!("{}: 検索は当たるのに診断が空", c.name()));
            assert_eq!(
                (d.frame, d.total_drift, d.leading_skipped),
                (*f, 0, false),
                "{}: 診断が通常の検索と食い違う",
                c.name()
            );
        }
    }
}

/// 速度の目安。`cargo test --release -- --ignored --nocapture` で走る
#[test]
#[ignore = "計測用"]
fn bench() {
    use std::time::Instant;

    // 先に全 ROI をメモリに載せてから計る (PNG のデコードは対象外)
    let cases = load_all();
    let frames: Vec<Vec<u8>> = cases
        .iter()
        .flat_map(|c| c.expected.saved_frames.iter().map(|s| load_roi_rgba(&c.png(s))))
        .collect();
    assert!(!frames.is_empty(), "ROI が無い");

    const ROUNDS: u32 = 20;
    let start = Instant::now();
    let mut sink = 0usize;
    for _ in 0..ROUNDS {
        for rgba in &frames {
            sink += read_frame(0.0, rgba).unwrap().crafting as usize;
        }
    }
    let elapsed = start.elapsed();
    let per = elapsed.as_secs_f64() / (ROUNDS as usize * frames.len()) as f64;
    println!(
        "読み取り: {:.0} us/コマ  ({} コマ x {ROUNDS} 回 = {:.2} 秒, crafting {sink})",
        per * 1e6,
        frames.len(),
        elapsed.as_secs_f64()
    );

    let cumulative = cases
        .iter()
        .find(|c| !c.expected.frames.is_empty())
        .expect("フレームが見つかっているテストケースが無い")
        .expected
        .cumulative
        .clone();
    let mut searcher = Searcher::new(&cumulative, 0).unwrap();
    let start = Instant::now();
    let hits = searcher.step(10_000_000);
    let elapsed = start.elapsed();
    println!(
        "検索: {:.2} 秒 / 10^7 ステップ  ({:.0} M/秒, hits {hits:?})",
        elapsed.as_secs_f64(),
        10.0 / elapsed.as_secs_f64()
    );
}

/// `tools/dump_cases.py` が置いた ROI から期待値を作り直す。
///
/// 読み取り結果が前のコマから変わったコマだけ `frames/` に残し、残りは捨てる。
/// 完成品が上限に達したところで読むのをやめるのは、ブラウザ側と同じ
#[test]
#[ignore = "期待値の記録"]
fn record_cases() {
    for dir in case_dirs("input.json") {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(dir.join("input.json")).unwrap();
        let input: Input = serde_json::from_str(&text).unwrap();
        let raw = dir.join(".raw");

        let mut rows = Vec::new();
        let mut readings = Vec::new();
        let mut seen_below = false;
        for f in &input.frames {
            let reading = read_frame(f.t, &load_roi_rgba(&raw.join(&f.png))).unwrap();
            rows.push(Row::of(&reading));
            readings.push(reading);
            if reached_cap(reading.product, &mut seen_below) {
                break;
            }
        }

        let analysis = cross_check(&readings);
        let frames = Searcher::new(&analysis.cumulative, 0)
            .map(|mut s| s.step(SEARCH_STEPS))
            .unwrap_or_default();
        let diagnosis = diagnose(&analysis.cumulative, 0, SEARCH_STEPS).unwrap_or(None);

        let out = dir.join("frames");
        std::fs::create_dir_all(&out).unwrap();
        let mut saved = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            if i > 0 && row.same_reading(&rows[i - 1]) {
                continue;
            }
            let png = input.frames[i].png.clone();
            std::fs::rename(raw.join(&png), out.join(&png)).unwrap();
            saved.push(Saved { index: input.frames[i].index, png });
        }
        std::fs::remove_dir_all(&raw).unwrap();
        std::fs::remove_file(dir.join("input.json")).unwrap();

        let size: u64 = saved
            .iter()
            .map(|s| std::fs::metadata(out.join(&s.png)).unwrap().len())
            .sum();
        println!(
            "{name}: {}コマ 累計{}個 フレーム{frames:?} 診断{} ROI保存{}枚 {}KB",
            rows.len(),
            analysis.cumulative.len(),
            diagnosis.as_ref().map_or("なし".into(), |d| format!(
                "{}({:+})",
                d.frame, d.total_drift
            )),
            saved.len(),
            size / 1024
        );

        let expected = Expected {
            video: input.video,
            roi: input.roi,
            rows,
            cumulative: analysis.cumulative,
            frames,
            diagnosis,
            saved_frames: saved,
        };
        let json = serde_json::to_string_pretty(&expected).unwrap();
        std::fs::write(dir.join("expected.json"), json).unwrap();
    }
}
