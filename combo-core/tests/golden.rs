//! 基準実装 (tools/ref_reader.py, tools/ref_rng.py) と同じ結果になることを確かめる。
//! データは `python tools/dump_golden.py` が作る。
//!
//! | 置き場所 | 中身 |
//! |---|---|
//! | `tests/golden/*.json` | コマごとの読み取り結果・累計・フレーム位置 |
//! | `tests/fixtures/` | 代表的なコマの ROI と期待値 |
//! | `tests/golden/frames/` | ROI の全コマ。無ければそのテストは何もしない |

use std::path::{Path, PathBuf};

use combo_core::types::{Analysis, FrameReading};
use combo_core::{ROI, Searcher, cross_check, read_frame};
use serde::Deserialize;

/// 探す範囲。golden に載っているフレーム位置はすべてこの中に入る
const SEARCH_STEPS: u64 = 1_000_000;

#[derive(Deserialize)]
struct Golden {
    video: String,
    rows: Vec<Row>,
    cumulative: Vec<Option<u8>>,
    frames: Vec<i64>,
    saved_frames: Vec<Saved>,
}

#[derive(Deserialize)]
struct Row {
    t: f64,
    crafting: bool,
    material1: Option<u8>,
    material2: Option<u8>,
    product: Option<u8>,
}

#[derive(Deserialize)]
struct Saved {
    index: usize,
    png: String,
}

/// tests/fixtures/expected.json
#[derive(Deserialize)]
struct Fixtures {
    roi: RoiDoc,
    frames: Vec<Fixture>,
}

#[derive(Deserialize)]
struct RoiDoc {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct Fixture {
    png: String,
    #[serde(flatten)]
    row: Row,
}

impl Row {
    fn as_reading(&self) -> FrameReading {
        FrameReading {
            t: self.t,
            crafting: self.crafting,
            material1: self.material1,
            material2: self.material2,
            product: self.product,
            done: self.product == Some(99),
        }
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn load_all() -> Vec<Golden> {
    let dir = golden_dir();
    let mut out: Vec<Golden> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .map(|e| {
            let text = std::fs::read_to_string(e.path()).unwrap();
            serde_json::from_str(&text).unwrap_or_else(|err| panic!("{:?}: {err}", e.path()))
        })
        .collect();
    out.sort_by(|a: &Golden, b: &Golden| a.video.cmp(&b.video));
    assert!(!out.is_empty(), "golden データが無い。tools/dump_golden.py を実行する");
    out
}

/// 3 チャンネルの PNG を RGBA に広げる。
/// read_frame は 3 チャンネルの最大値しか見ないので、並び順は影響しない
fn load_roi_rgba(path: &Path) -> Vec<u8> {
    let file = std::io::BufReader::new(std::fs::File::open(path).unwrap());
    let mut reader = png::Decoder::new(file).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.width, ROI.width);
    assert_eq!(info.height, ROI.height);
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    buf[..info.buffer_size()]
        .chunks_exact(3)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect()
}

/// クロスチェックの結果が基準実装と一致する
#[test]
fn cross_check_matches_reference() {
    for g in load_all() {
        let rows: Vec<FrameReading> = g.rows.iter().map(Row::as_reading).collect();
        let analysis: Analysis = cross_check(&rows);
        assert_eq!(analysis.cumulative, g.cumulative, "{}: 累計が違う", g.video);
        assert_eq!(
            analysis.total_crafts() as usize,
            g.cumulative.len().saturating_sub(1),
            "{}: 調合回数が累計の個数と合わない",
            g.video
        );
    }
}

/// 乱数検索が基準実装と同じフレーム位置を出す
#[test]
fn search_matches_reference() {
    for g in load_all() {
        let mut searcher = match Searcher::new(&g.cumulative, 0) {
            Ok(s) => s,
            Err(e) => {
                assert!(g.frames.is_empty(), "{}: 基準実装は {:?} を見つけている ({e})", g.video, g.frames);
                continue;
            }
        };
        let hits = searcher.step(SEARCH_STEPS);

        // golden に載っている位置のうち、この範囲に入るものは全部見つかること
        for f in &g.frames {
            if (*f as u64) < SEARCH_STEPS {
                assert!(hits.contains(f), "{}: {f} が見つからない (hits={hits:?})", g.video);
            }
        }
        // 逆に、golden に無いものを拾っていないこと
        for h in &hits {
            assert!(g.frames.contains(h), "{}: {h} は golden に無い", g.video);
        }
    }
}

/// 速度の目安。`cargo test --release -- --ignored --nocapture` で走る
#[test]
#[ignore = "計測用"]
fn bench() {
    use std::time::Instant;

    // 先に全 ROI をメモリに載せてから計る (PNG のデコードは対象外)
    let frames_dir = golden_dir().join("frames");
    let mut frames = Vec::new();
    for g in load_all() {
        for saved in &g.saved_frames {
            let path = frames_dir.join(&saved.png);
            if path.exists() {
                frames.push(load_roi_rgba(&path));
            }
        }
    }
    assert!(!frames.is_empty(), "frames が無い。dump_golden.py --all-frames を実行する");

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

    // 検索: 10^7 ステップ
    let cumulative = load_all()
        .into_iter()
        .find(|g| !g.frames.is_empty())
        .expect("フレームが見つかっている golden が無い")
        .cumulative;
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

/// 代表的なコマの読み取りが基準実装と一致する
#[test]
fn read_frame_matches_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let text = std::fs::read_to_string(dir.join("expected.json"))
        .expect("tests/fixtures/expected.json が無い。tools/dump_golden.py を実行する");
    let fixtures: Fixtures = serde_json::from_str(&text).unwrap();

    // ROI を変えたらフィクスチャも作り直さないといけない
    assert_eq!(
        (fixtures.roi.x, fixtures.roi.y, fixtures.roi.width, fixtures.roi.height),
        (ROI.x, ROI.y, ROI.width, ROI.height),
        "ROI が変わっている。tools/dump_golden.py で作り直す"
    );
    assert!(fixtures.frames.len() >= 10, "フィクスチャが少なすぎる");

    for f in &fixtures.frames {
        let expected = f.row.as_reading();
        let actual = read_frame(expected.t, &load_roi_rgba(&dir.join(&f.png))).unwrap();
        assert_eq!(actual, expected, "{}", f.png);
    }
    eprintln!("{} コマ (fixtures) を照合した", fixtures.frames.len());
}

/// 全コマに広げた版。golden/frames が無ければ何もしない
#[test]
fn read_frame_matches_all_frames() {
    let frames_dir = golden_dir().join("frames");
    let mut checked = 0;
    for g in load_all() {
        for saved in &g.saved_frames {
            let path = frames_dir.join(&saved.png);
            if !path.exists() {
                continue;
            }
            let expected = g.rows[saved.index].as_reading();
            let actual = read_frame(expected.t, &load_roi_rgba(&path)).unwrap();
            assert_eq!(actual, expected, "{} の {}", g.video, saved.png);
            checked += 1;
        }
    }
    if checked == 0 {
        eprintln!("golden/frames が無いので飛ばした (tools/dump_golden.py --all-frames で作る)");
    } else {
        eprintln!("{checked} コマを照合した");
    }
}
