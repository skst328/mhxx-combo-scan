//! 累計個数の並びから、調合開始時の乱数位置 (フレーム) を探す。照合と診断は mhxx-rand (apmnnn/mhxx-rng の移植) の
//! search_combo と diagnose_combo を使い、ここでは区切って探すための持ち回りと、型の受け渡しだけをする。
//!
//! 探す範囲は報告するフレーム (検索の結果と同じ基準) で数える。

use mhxx_rand as rand;

use crate::types::{Diagnosis, Drift, SearchError};

impl From<rand::ComboError> for SearchError {
    fn from(e: rand::ComboError) -> Self {
        match e {
            rand::ComboError::TooShort => Self::TooShort,
            rand::ComboError::InvalidDifference { positions } => {
                Self::InvalidDifference { positions }
            }
            rand::ComboError::UnknownValue { positions } => Self::UnknownValue { positions },
        }
    }
}

/// 生産数の並びに一致するフレームを、範囲を区切りながら探す
pub struct Searcher {
    query: rand::ComboQuery,
    /// 探し始めるフレーム
    start: u64,
    /// 探し終えたフレームの数
    consumed: u64,
}

impl Searcher {
    /// `start_frame` から探し始める。並びが検索に使えなければ [`SearchError`]
    pub fn new(cumulative: &[Option<u8>], start_frame: u64) -> Result<Self, SearchError> {
        Ok(Self {
            query: rand::ComboQuery::new(cumulative)?,
            start: start_frame,
            consumed: 0,
        })
    }

    /// 続きの `count` フレームを探して、その中で一致したフレームを返す (小さい順)。
    /// フレームごとに独立に判定しているので、区切って呼んでも、まとめて探すのと同じ結果になる
    pub fn step(&mut self, count: u64) -> Vec<i64> {
        let from = self.start + self.consumed;
        self.consumed += count;
        rand::search_combo(&self.query, from..from + count, rand::Diagnose::Never, None)
            .frames
            .into_iter()
            .map(|f| f as i64)
            .collect()
    }

    /// 探し終えたフレームの数
    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    /// 照合に使う差分の個数。生産数 1 個あたり 1.5 bit なので、範囲 N を探すときの
    /// 偽陽性の期待値は N / 2^(1.5 * これ) になる
    pub fn pattern_len(&self) -> usize {
        self.query.pattern_len()
    }
}

/// 通常の検索で見つからないとき、途中で乱数の進み方がずれた可能性を調べる。
/// 範囲は `start_frame` から `total` フレーム。候補が 1 つに決まらなければ `None`
pub fn diagnose(
    cumulative: &[Option<u8>],
    start_frame: u64,
    total: u64,
) -> Result<Option<Diagnosis>, SearchError> {
    let query = rand::ComboQuery::new(cumulative)?;
    Ok(
        rand::diagnose_combo(&query, start_frame..start_frame + total).map(|d| Diagnosis {
            frame: d.frame as i64,
            drifts: d
                .drifts
                .iter()
                .map(|x| Drift {
                    craft: x.craft,
                    steps: x.steps,
                })
                .collect(),
            total_drift: d.total_drift,
            leading_skipped: d.leading_skipped,
        }),
    )
}

/// 照合と診断の中身は mhxx-rand のテストで確かめている。ここでは受け渡しが合っていることだけを見る
#[cfg(test)]
mod tests {
    use super::*;
    use mhxx_rand::rng::{ascend, jump};

    fn cum(v: &[u8]) -> Vec<Option<u8>> {
        v.iter().map(|&x| Some(x)).collect()
    }

    /// 調合 1 回で乱数が進む数
    const STRIDE: usize = 5;

    /// `start` の位置から調合 `count` 回ぶんの生産数を作り、累計の並びに直す。`drift` が (何回目, 量) なら、
    /// その回の直後に乱数が余分に進む。先頭 3 つは検索に使われないので適当な差 (3) で埋める
    fn generate(start: u64, count: usize, drift: Option<(usize, i64)>) -> Vec<Option<u8>> {
        let mut s = jump(start);
        let mut values = vec![0u8, 3, 6, 9];
        for i in 0..count {
            let y = match (s[3] & 0xFFFF) % 100 {
                0..=24 => 2,
                25..=74 => 3,
                _ => 4,
            };
            values.push(values.last().unwrap() + y);
            let extra = drift.filter(|&(at, _)| at == i).map_or(0, |(_, d)| d);
            for _ in 0..(STRIDE as i64 + extra) {
                s = ascend(s);
            }
        }
        cum(&values)
    }

    /// 位置 `start` から作った並びの報告値 (使わない先頭 3 回ぶん、初回の調合の遅れ、調合ごとの進みの補正)
    fn reported(start: u64, cumulative: &[Option<u8>]) -> i64 {
        start as i64 - (STRIDE * 3) as i64 - 15 + 2 * (cumulative.len() as i64 - 1)
    }

    #[test]
    fn errors_are_passed_through() {
        assert_eq!(
            Searcher::new(&cum(&[0, 3, 5, 7]), 0).err(),
            Some(SearchError::TooShort)
        );
        let mut c = cum(&[0, 3, 5, 7, 9, 12]);
        c[4] = None;
        assert_eq!(
            Searcher::new(&c, 0).err(),
            Some(SearchError::UnknownValue { positions: vec![4] })
        );
        assert_eq!(
            Searcher::new(&cum(&[0, 3, 5, 7, 9, 10, 13]), 0).err(),
            Some(SearchError::InvalidDifference { positions: vec![5] })
        );
        assert_eq!(
            diagnose(&cum(&[0, 3, 5, 7]), 0, 100).err(),
            Some(SearchError::TooShort)
        );
    }

    /// 乱数列から作った並びを探すと、元の位置の報告値が見つかる。範囲は報告値で数える
    #[test]
    fn finds_a_sequence_generated_from_the_rng() {
        let cumulative = generate(4_000, 12, None);
        let want = reported(4_000, &cumulative);
        let mut s = Searcher::new(&cumulative, 0).unwrap();
        assert!(s.step(20_000).contains(&want));
        assert_eq!(s.pattern_len(), 12);
        // 報告値の直後から探すと見つからない
        let mut late = Searcher::new(&cumulative, want as u64 + 1).unwrap();
        assert!(!late.step(20_000).contains(&want));
    }

    /// 区切って呼んでも、まとめて呼んだのと同じ結果になる
    #[test]
    fn stepping_in_chunks_matches_one_shot() {
        let cumulative = cum(&[0, 3, 6, 9, 12, 14, 17, 21, 23, 26, 29, 31, 34]);
        let mut one = Searcher::new(&cumulative, 0).unwrap();
        let whole = one.step(200_000);
        let mut chunked = Searcher::new(&cumulative, 0).unwrap();
        let mut parts = Vec::new();
        for _ in 0..20 {
            parts.extend(chunked.step(10_000));
        }
        assert!(!whole.is_empty());
        assert_eq!(whole, parts);
        assert_eq!(one.consumed(), chunked.consumed());
    }

    /// 途中で乱数が 1 つ余分に進んだ並びから、ずれの位置と量を割り出し、型に移す
    #[test]
    fn diagnose_finds_an_extra_advance() {
        let cumulative = generate(50_000, 40, Some((20, 1)));
        let want = reported(50_000, &cumulative);
        let mut s = Searcher::new(&cumulative, 0).unwrap();
        assert!(
            !s.step(100_000).contains(&want),
            "ずれがあると普通の検索では見つからない"
        );
        let d = diagnose(&cumulative, 0, 100_000)
            .unwrap()
            .expect("診断で見つかる");
        // ずれを足し戻した値は、ずれの無い並びの報告値より 1 大きい (余分に進んだ 1 ステップのぶん)
        assert_eq!(d.frame + d.total_drift as i64, want + 1);
        // ずれの位置は前後に少し滑ることがあるので、量だけを見る
        assert_eq!(d.total_drift, 1);
        assert_eq!(d.drifts.iter().map(|x| x.steps).sum::<i32>(), 1);
    }
}
