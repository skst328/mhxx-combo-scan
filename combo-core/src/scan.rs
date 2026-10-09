//! 調合が始まる位置を粗く探す。
//!
//! 全コマを描画すると遅いので、探している間は間引いて観測する。調合中は素材が
//! 1 回につき 1 個ずつ減り、完成品が 2〜4 個ずつ増えるので、隣り合う観測の差を見れば
//! 進行しているかが分かる。調合画面かどうかだけでは、前の調合の結果が映っている
//! 画面と区別が付かない。
//!
//! 見つけたら手前に戻って、そこから全コマ読み直す。

use crate::types::{FrameReading, Next};

/// 探索中に何コマおきに観測するか
pub const STRIDE: u32 = 5;

/// 調合 1 回ぶんのコマ数。ボタンを押しっぱなしにした前提
const FRAMES_PER_CRAFT: u32 = 3;
/// 1 回の生産数の範囲
const YIELD_MIN: i32 = 2;
const YIELD_MAX: i32 = 4;

/// 観測の間に進みうる調合の回数
const MIN_CRAFTS: i32 = (STRIDE / FRAMES_PER_CRAFT) as i32;
const MAX_CRAFTS: i32 = (STRIDE / FRAMES_PER_CRAFT + 1) as i32;

/// 戻り先として覚えておく観測の数。比較の前側から 2 つ前まで遡れるだけ持つ
const KEEP: usize = 3;

/// 判定に使う 3 つのうち、いくつ満たせば調合中とみなすか。
/// 完成品は調合の演出で 1 コマだけ読めなくなることがあるので、全部は求めない
const NEEDED: usize = 2;

/// 素材が調合 1 回ぶん以上減ったか
fn dropped(prev: Option<u8>, cur: Option<u8>) -> bool {
    match (prev, cur) {
        (Some(a), Some(b)) => (MIN_CRAFTS..=MAX_CRAFTS).contains(&(a as i32 - b as i32)),
        _ => false,
    }
}

/// 完成品が調合 1 回ぶん以上増えたか
fn gained(prev: Option<u8>, cur: Option<u8>) -> bool {
    match (prev, cur) {
        (Some(a), Some(b)) => {
            (YIELD_MIN * MIN_CRAFTS..=YIELD_MAX * MAX_CRAFTS).contains(&(b as i32 - a as i32))
        }
        _ => false,
    }
}

/// 2 つの観測の間で調合が進んだか。読めなかった枠は条件を満たさないので数に入らない
fn advanced(prev: &FrameReading, cur: &FrameReading) -> bool {
    let hits = [
        dropped(prev.material1, cur.material1),
        dropped(prev.material2, cur.material2),
        gained(prev.product, cur.product),
    ];
    hits.iter().filter(|x| **x).count() >= NEEDED
}

/// 間引いた観測から調合の開始を探す
pub struct Scanner {
    /// 直近の観測。古い順。先頭が戻り先になる
    recent: Vec<FrameReading>,
}

impl Scanner {
    pub fn new() -> Self {
        Self { recent: Vec::with_capacity(KEEP) }
    }

    /// 1 回ぶんの観測を受け取り、次に何をすべきかを返す
    pub fn observe(&mut self, reading: FrameReading) -> Next {
        if let Some(prev) = self.recent.last() {
            if advanced(prev, &reading) {
                // 比較の前側から 2 つ前へ。それより手前は「動いていない」と確認済み
                let target = self.recent.first().expect("last があるので空ではない");
                return Next::RewindTo { t: target.t };
            }
        }
        self.recent.push(reading);
        if self.recent.len() > KEEP {
            self.recent.remove(0);
        }
        Next::Skip { frames: STRIDE - 1 }
    }
}

/// 間引いた観測の列から、最初に調合中と判定した地点を返す。
/// 実データでの検証に使う
pub fn find_start(rows: &[FrameReading]) -> Option<(usize, f64)> {
    let mut scanner = Scanner::new();
    let mut i = 0;
    while i < rows.len() {
        if let Next::RewindTo { t } = scanner.observe(rows[i]) {
            return Some((i, t));
        }
        i += STRIDE as usize;
    }
    None
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(t: f64, m: Option<u8>, p: Option<u8>) -> FrameReading {
        FrameReading { t, crafting: true, material1: m, material2: m, product: p, done: false }
    }

    /// 素材の所持数が違っても、それぞれが減っていれば分かる
    fn uneven(t: f64, m1: u8, m2: u8, p: u8) -> FrameReading {
        FrameReading {
            t,
            crafting: true,
            material1: Some(m1),
            material2: Some(m2),
            product: Some(p),
            done: false,
        }
    }

    #[test]
    fn stays_while_nothing_moves() {
        let mut s = Scanner::new();
        // 前の調合の結果が映っているだけの画面。値が動かない
        for i in 0..5 {
            let next = s.observe(row(i as f64 * 0.17, Some(67), Some(99)));
            assert_eq!(next, Next::Skip { frames: STRIDE - 1 }, "{i} 回目");
        }
    }

    #[test]
    fn detects_when_values_move() {
        let mut s = Scanner::new();
        assert_eq!(s.observe(row(0.00, Some(99), Some(0))), Next::Skip { frames: 4 });
        assert_eq!(s.observe(row(0.17, Some(99), Some(0))), Next::Skip { frames: 4 });
        assert_eq!(s.observe(row(0.33, Some(99), Some(0))), Next::Skip { frames: 4 });
        // 素材が 1 減り、完成品が 3 増えた。2 つ満たすので調合中
        assert_eq!(s.observe(row(0.50, Some(98), Some(3))), Next::RewindTo { t: 0.00 });
    }

    /// 戻り先は比較の前側から 2 つ前。観測が足りなければ持っている最古のもの
    #[test]
    fn rewinds_to_the_oldest_kept_observation() {
        let mut s = Scanner::new();
        s.observe(row(0.00, Some(99), Some(0)));
        assert_eq!(s.observe(row(0.17, Some(98), Some(3))), Next::RewindTo { t: 0.00 });
    }

    /// 完成品が読めなくても、素材 2 枠で足りる
    #[test]
    fn detects_without_the_product() {
        let mut s = Scanner::new();
        s.observe(uneven(0.00, 99, 46, 0));
        s.observe(uneven(0.17, 99, 46, 0));
        s.observe(uneven(0.33, 99, 46, 0));
        let next = s.observe(FrameReading {
            t: 0.50,
            crafting: true,
            material1: Some(98),
            material2: Some(45),
            product: None,
            done: false,
        });
        assert_eq!(next, Next::RewindTo { t: 0.00 });
    }

    /// 1 つしか動いていなければ調合とみなさない
    #[test]
    fn one_signal_is_not_enough() {
        let mut s = Scanner::new();
        s.observe(row(0.00, Some(99), Some(0)));
        // 完成品だけ動いた。素材は据え置き
        assert_eq!(s.observe(row(0.17, Some(99), Some(3))), Next::Skip { frames: 4 });
    }

    /// ありえない動き方は読み間違いとして弾く
    #[test]
    fn rejects_impossible_jumps() {
        let mut s = Scanner::new();
        s.observe(row(0.00, Some(99), Some(0)));
        // 素材が 10 も減り、完成品が 30 も増えるのは 5 コマでは起こらない
        assert_eq!(s.observe(row(0.17, Some(89), Some(30))), Next::Skip { frames: 4 });
    }

    /// 調合画面でないコマは値が無いので、何も満たさない
    #[test]
    fn ignores_frames_outside_the_craft_screen() {
        let mut s = Scanner::new();
        s.observe(row(0.00, Some(99), Some(0)));
        assert_eq!(s.observe(FrameReading::not_crafting(0.17)), Next::Skip { frames: 4 });
    }
}
