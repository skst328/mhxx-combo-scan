//! templates/*.png を二値化して Rust のソースに埋め込む。
//!
//! 見本は 8bit 単チャンネル PNG。1 バイト 1 画素として読み、最小値と最大値の
//! 中間をしきい値に 0/1 へ落としてから静的な配列として出力する。
//! 実行時に PNG を展開しないので、wasm にデコーダは入らない。

use std::{env, fs, path::Path};

/// 二値化のしきい値判定に使う最小明暗差
const MIN_CONTRAST: f32 = 50.0;

fn binarize(px: &[u8]) -> Vec<bool> {
    let lo = *px.iter().min().expect("empty template") as f32;
    let hi = *px.iter().max().expect("empty template") as f32;
    assert!(
        hi - lo >= MIN_CONTRAST,
        "テンプレートの明暗差が {MIN_CONTRAST} 未満です (lo={lo}, hi={hi})"
    );
    let thr = (lo + hi) / 2.0;
    px.iter().map(|&v| v as f32 > thr).collect()
}

fn load(dir: &Path, name: &str) -> (usize, usize, Vec<bool>) {
    let path = dir.join(format!("{name}.png"));
    let file = fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut reader = png::Decoder::new(std::io::BufReader::new(file)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Grayscale, "{name}: 単チャンネルではない");
    assert_eq!(info.bit_depth, png::BitDepth::Eight, "{name}: 8bit ではない");
    let px = &buf[..info.buffer_size()];
    (info.width as usize, info.height as usize, binarize(px))
}

fn literal(w: usize, h: usize, bits: &[bool]) -> String {
    let body: String = bits.iter().map(|&b| if b { "true," } else { "false," }).collect();
    format!("Template {{ w: {w}, h: {h}, bits: &[{body}] }}")
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut out = String::from("// build.rs が生成。手で編集しない\n");

    let digits: Vec<String> = (0..10)
        .map(|n| {
            let (w, h, bits) = load(&dir, &n.to_string());
            literal(w, h, &bits)
        })
        .collect();
    out.push_str(&format!(
        "pub static DIGITS: [Template; 10] = [{}];\n",
        digits.join(",")
    ));

    for name in ["slash", "header"] {
        let (w, h, bits) = load(&dir, name);
        out.push_str(&format!(
            "pub static {}: Template = {};\n",
            name.to_uppercase(),
            literal(w, h, &bits)
        ));
    }

    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("templates_data.rs");
    fs::write(dest, out).unwrap();
}
