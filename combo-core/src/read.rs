//! 1 コマの読み取り。
//!
//! **前後のコマは一切参照しない。** 補正はすべて [`crate::cross`] 側で行う。

use crate::templates::{DIGITS, HEADER, SLASH, Template};
use crate::types::{FrameReading, Rect, Size};

/// 枠内の明暗差がこれ未満なら「空白」とみなす
const MIN_CONTRAST: f32 = 50.0;
/// 数字: 二値化後の不一致率がこれを超えたら「読めない」
const MAX_DIST: f64 = 0.20;
/// 「/」「調合素材」: これを超えたら調合画面ではない
const MAX_DIST_MARK: f64 = 0.15;
/// 所持上限
pub const CAP: u8 = 99;

/// 枠の座標が前提とするコマの大きさ
pub const SOURCE: Size = Size { width: 1280, height: 720 };

/// 完成品「31/99」の枠
const BOX: Rect = Rect::new(760, 242, 85, 32);
/// 「調合素材」の見出し (画面全体の座標)
const HEADER_RECT: Rect = Rect::new(514, 50, 82, 27);
/// 完成品の数字スロット (BOX 内の相対座標を絶対座標に直したもの)
const PROD_SLOTS: [Rect; 2] = [
    Rect::new(12, 3, 15, 24).offset(BOX.x, BOX.y),
    Rect::new(26, 3, 15, 24).offset(BOX.x, BOX.y),
];
/// 「/」 (同上)
const SLASH_RECT: Rect = Rect::new(40, 3, 12, 24).offset(BOX.x, BOX.y);
/// 素材 1・素材 2 の数字スロット (画面全体の座標)
const MAT_SLOTS: [[Rect; 2]; 2] = [
    [Rect::new(802, 97, 15, 24), Rect::new(818, 97, 15, 24)],
    [Rect::new(802, 165, 15, 24), Rect::new(818, 165, 15, 24)],
];

/// TS が切り出す矩形。上記すべての枠を周囲 1px ごと含む外接矩形。
///
/// 各枠は ±1px ずらして照合するため周囲 1px を余分に読む。
pub const ROI: Rect = Rect::new(513, 49, 321, 221);

/// ROI の RGBA バイト数
pub const ROI_LEN: usize = (ROI.width * ROI.height * 4) as usize;

// ROI はコマの内側に収まっていなければならない
const _: () = {
    assert!(ROI.x + ROI.width <= SOURCE.width);
    assert!(ROI.y + ROI.height <= SOURCE.height);
};

/// 周囲 1px 込みで切り出した明るさ (色チャンネルの最大値)
struct Patch {
    /// 枠の幅 + 2
    w: usize,
    px: Vec<u8>,
}

impl Patch {
    /// ROI の RGBA から rect の周囲 1px を含めて切り出す
    fn new(rgba: &[u8], rect: Rect) -> Self {
        let (w, h) = (rect.width as usize + 2, rect.height as usize + 2);
        // ROI 内での左上位置。周囲 1px を含むので -1 する
        let ox = (rect.x - ROI.x) as usize - 1;
        let oy = (rect.y - ROI.y) as usize - 1;
        let stride = ROI.width as usize;
        let mut px = vec![0u8; w * h];
        for r in 0..h {
            let row = ((oy + r) * stride + ox) * 4;
            for c in 0..w {
                let i = row + c * 4;
                // アルファは無視して 3 チャンネルの最大値。白字も赤字も「明るい」と
                // 扱える。最大値なのでチャンネルの並び順には依存しない
                px[r * w + c] = rgba[i].max(rgba[i + 1]).max(rgba[i + 2]);
            }
        }
        Self { w, px }
    }

    /// (dx, dy) を左上とする w*h の領域を二値化して `out` に書く。
    /// 明暗差が小さければ空白とみなして false
    fn binarize(&self, dx: usize, dy: usize, w: usize, h: usize, out: &mut Vec<bool>) -> bool {
        let (mut lo, mut hi) = (u8::MAX, u8::MIN);
        for r in 0..h {
            let row = (dy + r) * self.w + dx;
            for &v in &self.px[row..row + w] {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if (hi as f32) - (lo as f32) < MIN_CONTRAST {
            return false;
        }
        let thr = (lo as f32 + hi as f32) / 2.0;
        out.clear();
        for r in 0..h {
            let row = (dy + r) * self.w + dx;
            out.extend(self.px[row..row + w].iter().map(|&v| v as f32 > thr));
        }
        true
    }
}

/// ±1px ずらした 9 通り × 全見本で不一致率を計算し、一番小さいものを返す。
/// どの見本とも 1.0 未満にならなければ (None, 1.0)
fn best_match(p: &Patch, w: usize, h: usize, tpls: &[&Template]) -> (Option<usize>, f64) {
    let mut best: (Option<usize>, f64) = (None, 1.0);
    let mut buf = Vec::with_capacity(w * h);
    let total = (w * h) as f64;
    for dy in 0..3 {
        for dx in 0..3 {
            if !p.binarize(dx, dy, w, h, &mut buf) {
                continue;
            }
            for (i, t) in tpls.iter().enumerate() {
                debug_assert_eq!((t.w, t.h), (w, h));
                let diff = buf.iter().zip(t.bits).filter(|(a, b)| a != b).count();
                let d = diff as f64 / total;
                if d < best.1 {
                    best = (Some(i), d);
                }
            }
        }
    }
    best
}

/// [10 の位, 1 の位] のスロットから数値を読む。読めなければ None
fn read_number(rgba: &[u8], slots: &[Rect; 2]) -> Option<u8> {
    let digits: Vec<&Template> = DIGITS.iter().collect();
    let mut chars = [None; 2];
    for (i, rect) in slots.iter().enumerate() {
        let (w, h) = (rect.width as usize, rect.height as usize);
        let p = Patch::new(rgba, *rect);
        let mut buf = Vec::new();
        // ずらさない位置で明暗差を見て、小さければ空白
        if !p.binarize(1, 1, w, h, &mut buf) {
            continue;
        }
        let (n, d) = best_match(&p, w, h, &digits);
        if d > MAX_DIST {
            return None;
        }
        chars[i] = n.map(|v| v as u8);
    }
    // 1 の位が空白なら数値として成立しない
    Some(chars[1]? + chars[0].unwrap_or(0) * 10)
}

/// 「調合素材」の見出しと「/」が両方あるときだけ調合画面とみなす
fn is_crafting(rgba: &[u8]) -> bool {
    for (rect, tpl) in [(HEADER_RECT, &HEADER), (SLASH_RECT, &SLASH)] {
        let p = Patch::new(rgba, rect);
        let (_, d) = best_match(&p, rect.width as usize, rect.height as usize, &[tpl]);
        if d > MAX_DIST_MARK {
            return false;
        }
    }
    true
}

/// ROI の RGBA 1 枚から読み取る。`rgba` の長さは `ROI_LEN` でなければならない
pub fn read_frame(t: f64, rgba: &[u8]) -> Result<FrameReading, String> {
    if rgba.len() != ROI_LEN {
        return Err(format!(
            "ROI のバイト数が違います: {} (期待値 {ROI_LEN} = {}x{}x4)",
            rgba.len(),
            ROI.width,
            ROI.height
        ));
    }
    if !is_crafting(rgba) {
        return Ok(FrameReading::not_crafting(t));
    }
    let product = read_number(rgba, &PROD_SLOTS);
    Ok(FrameReading {
        t,
        crafting: true,
        material1: read_number(rgba, &MAT_SLOTS[0]),
        material2: read_number(rgba, &MAT_SLOTS[1]),
        product,
        // 上限に達したかは 1 コマでは決まらない。[`reached_cap`] で決める
        done: false,
    })
}

/// 完成品が上限に達したか。
///
/// 上限の表示を見ただけでは決まらない。調合を始める前の画面に前の調合の結果が
/// 映っていることがあるので、**上限未満を見たあとの上限**だけを到達とみなす。
/// `seen_below` は同じ動画のあいだ持ち回る
pub fn reached_cap(product: Option<u8>, seen_below: &mut bool) -> bool {
    match product {
        Some(p) if p < CAP => {
            *seen_below = true;
            false
        }
        Some(_) => *seen_below,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 調合前から上限が映っていても到達とはみなさない
    #[test]
    fn cap_needs_a_smaller_value_first() {
        let mut seen = false;
        // 前の調合の結果が映っている
        assert!(!reached_cap(Some(CAP), &mut seen));
        // 調合画面でないコマは何も変えない
        assert!(!reached_cap(None, &mut seen));
        assert!(!reached_cap(Some(CAP), &mut seen));
        // ここから本番
        assert!(!reached_cap(Some(0), &mut seen));
        assert!(!reached_cap(Some(98), &mut seen));
        assert!(reached_cap(Some(CAP), &mut seen));
    }

    /// ROI がすべての枠 (周囲 1px 込み) を含んでいること
    #[test]
    fn roi_covers_all_rects() {
        let mut rects = vec![HEADER_RECT, SLASH_RECT];
        rects.extend(PROD_SLOTS);
        rects.extend(MAT_SLOTS.iter().flatten().copied());

        let left = rects.iter().map(|r| r.x - 1).min().unwrap();
        let top = rects.iter().map(|r| r.y - 1).min().unwrap();
        let right = rects.iter().map(|r| r.x + r.width + 1).max().unwrap();
        let bottom = rects.iter().map(|r| r.y + r.height + 1).max().unwrap();

        assert_eq!(ROI, Rect::new(left, top, right - left, bottom - top));
    }

    /// 見本の大きさが枠と一致していること
    #[test]
    fn template_sizes_match_rects() {
        for t in &DIGITS {
            assert_eq!((t.w, t.h), (15, 24));
        }
        assert_eq!((SLASH.w, SLASH.h), (SLASH_RECT.width as usize, SLASH_RECT.height as usize));
        assert_eq!((HEADER.w, HEADER.h), (HEADER_RECT.width as usize, HEADER_RECT.height as usize));
    }

    /// 長さが違えばエラー
    #[test]
    fn rejects_wrong_length() {
        assert!(read_frame(0.0, &[0; 100]).is_err());
    }

    /// 全面真っ黒なら明暗差ゼロで調合画面ではない
    #[test]
    fn blank_frame_is_not_crafting() {
        let r = read_frame(1.5, &vec![0; ROI_LEN]).unwrap();
        assert!(!r.crafting);
        assert_eq!(r.t, 1.5);
        assert!(!r.done);
    }
}
