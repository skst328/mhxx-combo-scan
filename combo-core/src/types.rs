//! TS とやり取りするデータ型。serde でシリアライズし、tsify が .d.ts を生成する。

use serde::{Deserialize, Serialize};
use tsify::Tsify;

/// 1280x720 座標系の矩形
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// 元画像の (dx, dy) だけずらした位置に置いた矩形
    pub const fn offset(self, dx: u32, dy: u32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}

/// コマ全体の大きさ。ROI の座標はこの座標系で表す
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// 1 コマの読み取り結果。**そのコマだけ**を見た生の値で、前後のコマによる補正は入っていない
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct FrameReading {
    /// 秒
    pub t: f64,
    /// 調合画面か。false なら以下の数値はすべて None
    pub crafting: bool,
    pub material1: Option<u8>,
    pub material2: Option<u8>,
    pub product: Option<u8>,
    /// 完成品が上限に達した回まで読めた。TS はここでデコードを打ち切る。
    /// 上限の表示を見ただけでは立たない (調合前から上限のことがある)
    pub done: bool,
}

impl FrameReading {
    pub fn not_crafting(t: f64) -> Self {
        Self { t, crafting: false, material1: None, material2: None, product: None, done: false }
    }
}

/// 調合 1 区間ぶんの結果
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Craft {
    /// この区間が始まったコマの時刻 (秒)
    pub t: f64,
    /// 素材の減り = 調合回数
    pub count: u8,
    /// 完成品の表示が実際に増えた量。読み間違いがあれば負にもなりうる
    pub gain: i32,
    pub before: u8,
    pub after: u8,
    pub resolution: Resolution,
}

/// `gain` を `count` 回の調合にどう割り振れたか
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Resolution {
    /// count == 1 で確定
    Exact { yields: Vec<u8> },
    /// count >= 2 だが割り振りが一通りしかない (見落としを補完できた)
    Inferred { yields: Vec<u8> },
    /// 割り振りが複数あるので決めない
    Ambiguous { candidates: Vec<Vec<u8>> },
    /// 上限に到達した回。count >= 2 のときは内訳を決めない
    Capped { yields: Option<Vec<u8>> },
    /// どう割り振っても説明できない
    Failed,
}

#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Issue {
    /// 素材が count 個減ったのに完成品が gain しか増えていない
    Unexplained { t: f64, count: u8, gain: i32 },
    /// 最後の調合の完成品の増加が読めていない
    LastGainUnread { t: f64 },
}

/// クロスチェックの結果
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub crafts: Vec<Craft>,
    /// 調合前の個数と、各調合後の累計。決まらない箇所は None
    pub cumulative: Vec<Option<u8>>,
    /// 素材の最初の値。区間が 1 つも無ければ None
    pub material_from: Option<u8>,
    pub material_to: Option<u8>,
    pub issues: Vec<Issue>,
}

impl Analysis {
    pub fn empty() -> Self {
        Self {
            crafts: Vec::new(),
            cumulative: Vec::new(),
            material_from: None,
            material_to: None,
            issues: Vec::new(),
        }
    }

    /// 調合の総回数
    pub fn total_crafts(&self) -> u32 {
        self.crafts.iter().map(|c| c.count as u32).sum()
    }
}

/// `FrameSearcher::create` に渡す累計の並び。tsify では `(number | undefined)[]`
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
pub struct Cumulative(pub Vec<Option<u8>>);

/// 乱数の進み方が想定とずれた箇所
#[derive(Tsify, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drift {
    /// 何回目の調合の直前でずれたか (1 始まり)
    pub craft: usize,
    /// 想定より何ステップ多く進んだか。少なければ負
    pub steps: i32,
}

/// 通常の検索で見つからなかったときの診断結果
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    /// 調合開始時のフレーム位置。通常の検索が返すものと同じ基準
    pub frame: i64,
    /// ずれが起きた箇所
    pub drifts: Vec<Drift>,
    /// ずれの合計。報告値はこのぶんだけ補正が要る。相殺されていれば 0
    pub total_drift: i32,
    /// 先頭の 1 回を照合から外した。調合開始直後に捨てる回数が足りなかったときに起きる
    pub leading_skipped: bool,
}

/// `Diagnosis | undefined` として TS に渡すための包み
#[derive(Tsify, Serialize, Deserialize, Clone, Debug)]
pub struct MaybeDiagnosis(pub Option<Diagnosis>);

/// 累計の並びが乱数検索に使えない理由
#[derive(Tsify, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SearchError {
    /// 検索に使える調合が 1 回以下
    TooShort,
    /// 差が 2〜4 になっていない位置 (cumulative の添字)
    InvalidDifference { positions: Vec<usize> },
    /// 累計が確定していない位置 (cumulative の添字)
    UnknownValue { positions: Vec<usize> },
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "too short"),
            Self::InvalidDifference { positions } => write!(f, "invalid differences at {positions:?}"),
            Self::UnknownValue { positions } => write!(f, "unknown values at {positions:?}"),
        }
    }
}
