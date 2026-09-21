//! クロスチェック。
//!
//! 考え方: 調合の「回数」は素材の減り (1 回につき必ず 1 個) で数え、
//! 1 回あたりの「個数」は完成品の増えで読む。素材の表示が変わってから次に
//! 変わるまでの間の完成品の増加量を、その間の調合回数で割り振る。

use crate::read::CAP;
use crate::types::{Analysis, Craft, FrameReading, Issue, Resolution};

/// 1 回の生産数の範囲
const YIELD_MIN: i32 = 2;
const YIELD_MAX: i32 = 4;

/// 候補の打ち切り。これを超えるほど曖昧なら結局どのみち確定しないので、
/// 組み合わせ爆発を避けるために途中で止める
const MAX_CANDIDATES: usize = 256;

/// 素材の値が同じ間のコマのまとまり (調合 1 回ぶんに相当)
struct Seg {
    material: u8,
    /// 区間の最初のコマの時刻
    t: f64,
    /// その区間で最後に読めた完成品の値。完成品の表示は素材より 1 コマ遅れるため、
    /// 最後の値を取れば必ずその区間の調合結果になる
    last_product: Option<u8>,
}

/// 素材 1・素材 2 から素材の値を決める。食い違ったら前の値と辻褄が合う方
fn material_value(m1: Option<u8>, m2: Option<u8>, prev: Option<u8>) -> Option<u8> {
    match (m1, m2) {
        (Some(a), Some(b)) if a != b => {
            let mut ok = [a, b].into_iter().filter(|&m| prev.is_none_or(|p| m <= p));
            match (ok.next(), ok.next()) {
                (Some(only), None) => Some(only),
                _ => None, // 0 個または 2 個なら決められない
            }
        }
        (Some(a), _) => Some(a),
        (None, other) => other,
    }
}

/// 合計 total を k 回の調合 (各 2〜4 個) に割り振る全パターンを昇順で
fn splits(total: i32, k: u8) -> Vec<Vec<u8>> {
    fn rec(rem: i32, left: u8, cur: &mut Vec<u8>, out: &mut Vec<Vec<u8>>) {
        if left == 0 {
            if rem == 0 {
                out.push(cur.clone());
            }
            return;
        }
        for v in YIELD_MIN..=YIELD_MAX {
            if out.len() >= MAX_CANDIDATES {
                return;
            }
            // 残りを 2〜4 の範囲で埋められない枝は捨てる
            let (rem, left) = (rem - v, left - 1);
            if rem < YIELD_MIN * left as i32 || rem > YIELD_MAX * left as i32 {
                continue;
            }
            cur.push(v as u8);
            rec(rem, left, cur, out);
            cur.pop();
        }
    }
    let mut out = Vec::new();
    if k > 0 {
        rec(total, k, &mut Vec::with_capacity(k as usize), &mut out);
    }
    out
}

/// 素材の値が変わるごとに区間を区切る。
/// 素材が増えていたら別の調合が始まったとみなし、そこから数え直す
fn segments(rows: &[FrameReading]) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut prev_m = None;
    for r in rows.iter().filter(|r| r.crafting) {
        let Some(m) = material_value(r.material1, r.material2, prev_m) else {
            continue;
        };
        // 1 回の調合で素材が増えることはない。2 つの表示が揃って増えているなら
        // 別の調合が始まったとみなして数え直す。揃っていなければ読み間違い扱い
        if prev_m.is_some_and(|p| m > p) {
            if r.material1 != r.material2 {
                continue;
            }
            segs.clear();
        }
        if segs.last().is_none_or(|s| s.material != m) {
            let carry = segs.last().and_then(|s| s.last_product);
            segs.push(Seg { material: m, t: r.t, last_product: carry });
        }
        if let Some(p) = r.product {
            segs.last_mut().expect("直前に push 済み").last_product = Some(p);
        }
        prev_m = Some(m);
    }
    segs
}

/// その区間の調合 1 回ごとの累計個数。途中が決まらない回は None
fn craft_cumulative(c: &Craft) -> Vec<Option<u8>> {
    let yields = match &c.resolution {
        Resolution::Exact { yields } | Resolution::Inferred { yields } => Some(yields),
        Resolution::Capped { yields: Some(y) } => Some(y),
        Resolution::Capped { yields: None } | Resolution::Ambiguous { .. } | Resolution::Failed => None,
    };
    match yields {
        Some(y) => {
            let mut v = c.before;
            y.iter()
                .map(|&n| {
                    v = v.saturating_add(n);
                    Some(v)
                })
                .collect()
        }
        None => {
            let mut out = vec![None; c.count as usize - 1];
            out.push(Some(c.after));
            out
        }
    }
}

/// 読み取り済みの全コマからクロスチェックを行う。
/// 調合画面でないコマは無視するので、TS は全コマを渡してよい
pub fn cross_check(rows: &[FrameReading]) -> Analysis {
    let segs = segments(rows);
    let (Some(first), Some(last)) = (segs.first(), segs.last()) else {
        return Analysis::empty();
    };
    let (material_from, material_to) = (Some(first.material), Some(last.material));

    let mut crafts = Vec::new();
    let mut issues = Vec::new();
    let mut ai = 0;
    for bi in 1..segs.len() {
        let (a, b) = (&segs[ai], &segs[bi]);
        // 完成品が読めていない区間は次とまとめる
        let Some(before) = a.last_product else {
            ai = bi;
            continue;
        };
        let Some(after) = b.last_product else {
            continue;
        };
        let count = a.material - b.material; // 素材の減り = 調合回数
        let gain = after as i32 - before as i32;
        // 素材は減ったのに完成品が変わっていない = 完成品の表示を見落とした。
        // (成功率 100% 前提なので失敗は考えない) 次の区間とまとめて割り振る
        if gain == 0 {
            continue;
        }
        let resolution = if after == CAP {
            // 上限到達: 99 - 直前の値 をその回の生産数とみなす
            Resolution::Capped {
                yields: (count == 1).then(|| vec![gain as u8]),
            }
        } else {
            match splits(gain, count).as_slice() {
                [] => {
                    issues.push(Issue::Unexplained { t: b.t, count, gain });
                    Resolution::Failed
                }
                [only] if count == 1 => Resolution::Exact { yields: only.clone() },
                [only] => Resolution::Inferred { yields: only.clone() },
                many => Resolution::Ambiguous { candidates: many.to_vec() },
            }
        };
        crafts.push(Craft { t: b.t, count, gain, before, after, resolution });
        ai = bi;
    }
    if ai != segs.len() - 1 {
        issues.push(Issue::LastGainUnread { t: last.t });
    }

    let cumulative = match crafts.first() {
        None => Vec::new(),
        Some(f) => std::iter::once(Some(f.before))
            .chain(crafts.iter().flat_map(craft_cumulative))
            .collect(),
    };
    Analysis { crafts, cumulative, material_from, material_to, issues }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(t: f64, m: Option<u8>, p: Option<u8>) -> FrameReading {
        FrameReading { t, crafting: true, material1: m, material2: m, product: p, done: false }
    }

    /// 素材 1・素材 2 が食い違うコマ
    fn split_row(t: f64, m1: Option<u8>, m2: Option<u8>, p: Option<u8>) -> FrameReading {
        FrameReading { t, crafting: true, material1: m1, material2: m2, product: p, done: false }
    }

    /// 調合を始める前に前の調合の結果が映っていても、本番だけを数える
    #[test]
    fn a_new_run_restarts_the_count() {
        let rows = vec![
            // 前の調合の結果。素材 67・完成品は上限
            row(0.0, Some(67), Some(99)),
            FrameReading::not_crafting(0.5),
            // ここから本番。素材が増えているので別の調合とみなす
            row(1.0, Some(99), Some(0)),
            row(1.2, Some(98), Some(0)),
            row(1.3, Some(98), Some(3)),
            row(1.4, Some(97), Some(3)),
            row(1.5, Some(97), Some(7)),
        ];
        let a = cross_check(&rows);
        assert_eq!(a.material_from, Some(99));
        assert_eq!(a.material_to, Some(97));
        assert_eq!(a.cumulative, vec![Some(0), Some(3), Some(7)]);
    }

    /// 表示が揃っていない増え方は、別の調合ではなく読み間違い
    #[test]
    fn a_disagreeing_increase_is_still_a_misreading() {
        let rows = vec![
            row(0.0, Some(90), Some(10)),
            row(0.1, Some(89), Some(10)),
            row(0.2, Some(89), Some(13)),
            // 素材 1 だけが増えている。ここで数え直したら本番を捨ててしまう
            split_row(0.3, Some(98), Some(88), Some(13)),
            row(0.4, Some(88), Some(16)),
        ];
        let a = cross_check(&rows);
        assert_eq!(a.material_from, Some(90));
        assert_eq!(a.cumulative, vec![Some(10), Some(13), Some(16)]);
    }

    #[test]
    fn splits_enumerates_in_ascending_order() {
        assert_eq!(splits(3, 1), vec![vec![3]]);
        assert_eq!(splits(4, 2), vec![vec![2, 2]]);
        assert_eq!(splits(6, 2), vec![vec![2, 4], vec![3, 3], vec![4, 2]]);
        assert!(splits(5, 1).is_empty());
        assert!(splits(0, 0).is_empty());
    }

    #[test]
    fn material_value_prefers_the_one_consistent_with_prev() {
        assert_eq!(material_value(Some(98), Some(98), Some(99)), Some(98));
        // 99 は前の値 98 を超えるので 97 を採る
        assert_eq!(material_value(Some(99), Some(97), Some(98)), Some(97));
        // どちらも前の値以下なら決められない
        assert_eq!(material_value(Some(97), Some(96), Some(98)), None);
        assert_eq!(material_value(None, Some(50), None), Some(50));
        assert_eq!(material_value(None, None, None), None);
    }

    /// 完成品の表示は素材より 1 コマ遅れる。区間の最後の値を取ればその区間の結果になる
    #[test]
    fn uses_the_last_product_of_each_segment() {
        let rows = [
            row(24.31, Some(99), Some(0)),
            row(24.34, Some(98), Some(0)),
            row(24.37, Some(98), Some(3)),
            row(24.87, Some(97), Some(3)),
            row(24.91, Some(97), Some(5)),
            row(24.97, Some(96), Some(5)),
            row(25.01, Some(96), Some(7)),
        ];
        let a = cross_check(&rows);
        assert_eq!(a.material_from, Some(99));
        assert_eq!(a.material_to, Some(96));
        assert_eq!(a.total_crafts(), 3);
        let yields: Vec<&Resolution> = a.crafts.iter().map(|c| &c.resolution).collect();
        assert_eq!(
            yields,
            vec![
                &Resolution::Exact { yields: vec![3] },
                &Resolution::Exact { yields: vec![2] },
                &Resolution::Exact { yields: vec![2] },
            ]
        );
        assert_eq!(a.cumulative, vec![Some(0), Some(3), Some(5), Some(7)]);
        assert!(a.issues.is_empty());
    }

    /// 素材が 2 個減って +4 なら「2 2」しかないので補完できる
    #[test]
    fn infers_a_missed_reading() {
        let rows = [
            row(1.0, Some(99), Some(0)),
            row(2.0, Some(97), Some(4)),
            row(3.0, Some(96), Some(7)),
        ];
        let a = cross_check(&rows);
        assert_eq!(a.crafts[0].resolution, Resolution::Inferred { yields: vec![2, 2] });
        assert_eq!(a.cumulative, vec![Some(0), Some(2), Some(4), Some(7)]);
    }

    /// 素材が 2 個減って +6 は 3 通りあるので決めない
    #[test]
    fn keeps_all_candidates_when_ambiguous() {
        let rows = [row(1.0, Some(99), Some(0)), row(2.0, Some(97), Some(6))];
        let a = cross_check(&rows);
        assert_eq!(
            a.crafts[0].resolution,
            Resolution::Ambiguous { candidates: vec![vec![2, 4], vec![3, 3], vec![4, 2]] }
        );
        assert_eq!(a.cumulative, vec![Some(0), None, Some(6)]);
    }

    #[test]
    fn reports_unexplained_gain() {
        let rows = [row(1.0, Some(99), Some(0)), row(2.0, Some(98), Some(9))];
        let a = cross_check(&rows);
        assert_eq!(a.crafts[0].resolution, Resolution::Failed);
        assert_eq!(a.issues, vec![Issue::Unexplained { t: 2.0, count: 1, gain: 9 }]);
    }

    #[test]
    fn caps_at_99() {
        let rows = [row(1.0, Some(50), Some(98)), row(2.0, Some(49), Some(99))];
        let a = cross_check(&rows);
        assert_eq!(a.crafts[0].resolution, Resolution::Capped { yields: Some(vec![1]) });
        assert_eq!(a.cumulative, vec![Some(98), Some(99)]);
    }

    #[test]
    fn ignores_non_crafting_frames() {
        let rows = [
            FrameReading::not_crafting(0.5),
            row(1.0, Some(99), Some(0)),
            FrameReading::not_crafting(1.2),
            row(2.0, Some(98), Some(3)),
        ];
        let a = cross_check(&rows);
        assert_eq!(a.total_crafts(), 1);
        assert_eq!(a.cumulative, vec![Some(0), Some(3)]);
    }

    #[test]
    fn empty_input_yields_empty_analysis() {
        assert_eq!(cross_check(&[]), Analysis::empty());
    }
}
