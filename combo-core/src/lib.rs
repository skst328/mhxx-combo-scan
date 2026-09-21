//! MHXX の調合動画から、ゲーム内の乱数位置 (フレーム) を特定する。
//!
//! このファイルは wasm との境界だけを持ち、ロジックは各モジュールにある:
//!
//! | モジュール | 役割 | 参照範囲 |
//! |---|---|---|
//! | [`read`] | 1 コマの読み取り | そのコマだけ |
//! | [`cross`] | 素材の減りと完成品の増えの突き合わせ | 並び全体 |
//! | [`rng`] / [`search`] | 生産数の並びから乱数位置を探す | — |
//!
//! 前提は「Switch 2 録画の 1280x720・30fps・時刻順に全コマが渡される」こと。
//! 解像度の確認は TS 側の責任で、ここでは長さだけ検査する。

mod cross;
mod read;
mod rng;
mod search;
mod templates;
pub mod types;

use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

pub use cross::cross_check;
pub use read::{ROI, read_frame, reached_cap};
pub use search::{Searcher, diagnose};
use types::*;

/// コマを順に受け取って溜め、まとめて解析する
#[wasm_bindgen]
pub struct Session {
    rows: Vec<FrameReading>,
    /// 上限未満の完成品を見たか。[`read::reached_cap`] に持ち回る
    seen_below_cap: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { rows: Vec::new(), seen_below_cap: false }
    }

    /// 受け付けるコマの大きさ。TS はこれと違う動画を弾く
    #[wasm_bindgen(js_name = sourceSize)]
    pub fn source_size() -> Result<Ts<Size>, JsError> {
        Ok(read::SOURCE.into_ts()?)
    }

    /// TS が切り出すべき矩形。`sourceSize()` の座標系
    pub fn roi() -> Result<Ts<Rect>, JsError> {
        Ok(read::ROI.into_ts()?)
    }

    /// `rgba` は `roi()` の幅 x 高さ x 4 バイト。長さが違えば throw する。
    /// 戻り値はそのコマだけを見た生の値で、前後のコマによる補正は入っていない
    #[wasm_bindgen(js_name = pushFrame)]
    pub fn push_frame(&mut self, t: f64, rgba: &[u8]) -> Result<Ts<FrameReading>, JsError> {
        let mut reading = read::read_frame(t, rgba).map_err(|e| JsError::new(&e))?;
        reading.done = read::reached_cap(reading.product, &mut self.seen_below_cap);
        self.rows.push(reading);
        Ok(reading.into_ts()?)
    }

    /// 溜まったコマにクロスチェックをかける。途中で呼んでもよい
    pub fn analyze(&self) -> Result<Ts<Analysis>, JsError> {
        Ok(cross::cross_check(&self.rows).into_ts()?)
    }
}

/// 通常の検索で見つからなかったとき、前半だけで位置を定めて、途中で乱数が
/// どれだけ余分に進んだかを調べる。手がかりが足りなければ undefined を返す
#[wasm_bindgen(js_name = diagnoseSearch)]
pub fn diagnose_search(
    cumulative: Ts<Cumulative>,
    start_frame: f64,
    total: f64,
) -> Result<Ts<MaybeDiagnosis>, JsValue> {
    let cumulative: Cumulative = cumulative
        .to_rust()
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let found = search::diagnose(
        &cumulative.0,
        start_frame.max(0.0) as u64,
        total.max(0.0) as u64,
    )
    .map_err(|e| {
        serde_wasm_bindgen::to_value(&e).unwrap_or_else(|_| JsValue::from_str("search error"))
    })?;
    MaybeDiagnosis(found)
        .into_ts()
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// 乱数位置の検索。重いので Worker 内で [`FrameSearcher::step`] を回す想定
#[wasm_bindgen]
pub struct FrameSearcher {
    inner: Searcher,
}

#[wasm_bindgen]
impl FrameSearcher {
    /// `cumulative` は [`Analysis::cumulative`]。検索に使えない並びなら
    /// [`SearchError`] をそのまま throw する (Error ではなく `{ type: ... }` の値)
    pub fn create(cumulative: Ts<Cumulative>, start_frame: f64) -> Result<FrameSearcher, JsValue> {
        let cumulative: Cumulative = cumulative.to_rust().map_err(|e| JsValue::from_str(&e.to_string()))?;
        Searcher::new(&cumulative.0, start_frame.max(0.0) as u64)
            .map(|inner| Self { inner })
            .map_err(|e| {
                serde_wasm_bindgen::to_value(&e).unwrap_or_else(|_| JsValue::from_str("search error"))
            })
    }

    /// `count` 回ぶん乱数を進めて、その間に確定したフレーム位置を返す
    pub fn step(&mut self, count: u32) -> Vec<f64> {
        self.inner.step(count as u64).into_iter().map(|f| f as f64).collect()
    }

    /// 開始位置から進めた数
    #[wasm_bindgen(getter)]
    pub fn consumed(&self) -> f64 {
        self.inner.consumed() as f64
    }

    /// 照合に使う差分の個数。範囲 N を探すときの偽陽性の期待値は
    /// N / 2^(1.5 * これ) なので、探してよい範囲の目安になる
    #[wasm_bindgen(getter, js_name = patternLength)]
    pub fn pattern_length(&self) -> usize {
        self.inner.pattern_len()
    }
}
