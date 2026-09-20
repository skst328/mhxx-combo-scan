//! 累計個数の並びから、調合開始時の乱数位置 (フレーム) を探す。
//!
//! [`Searcher::step`] は KMP の途中状態を保持したまま中断・再開できるので、
//! 区切って呼んでも境界をまたぐ一致を取りこぼさない。
//!
//! ライセンスは [`crate::rng`] の記載を参照 (apmnnn/mhxx-rng, MIT)。

use crate::rng::{ascend, jump};
use crate::types::{Diagnosis, Drift, SearchError};

/// 調合 1 回で乱数が進む数。この間隔で拾った乱数が生産数を決める
const STRIDE: usize = 5;

/// 乱数 (0〜99) から生産数へ: 0〜24 は 2 個、25〜74 は 3 個、75〜99 は 4 個
fn yield_of(w: u32) -> u8 {
    match (w & 0xFFFF) % 100 {
        0..=24 => 2,
        25..=74 => 3,
        _ => 4,
    }
}

/// 累計の並びを、検索に使う差分列に直す。
/// 戻り値は (差分列, 元の並びの長さ)
fn pattern_from(cumulative: &[Option<u8>]) -> Result<(Vec<u8>, usize), SearchError> {
    // 先頭 3 つは調合開始前の乱数消費と噛み合わないので使わない
    let tail = cumulative.get(3..).unwrap_or(&[]);

    let unknown: Vec<usize> = tail
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_none())
        .map(|(i, _)| i + 3)
        .collect();
    if !unknown.is_empty() {
        return Err(SearchError::UnknownValue { positions: unknown });
    }

    let mut pos: Vec<u8> = tail.iter().map(|v| v.expect("上で None を弾いた")).collect();
    // 上限に張り付いた最後の 1 回は生産数が削られているので使わない
    if pos.last() == Some(&99) {
        pos.pop();
    }
    if pos.len() <= 1 {
        return Err(SearchError::TooShort);
    }

    let mut diffs = Vec::with_capacity(pos.len() - 1);
    let mut bad = Vec::new();
    for (i, pair) in pos.windows(2).enumerate() {
        let d = pair[1] as i32 - pair[0] as i32;
        if !(2..=4).contains(&d) {
            // cumulative 上での位置に直す (pos[i+1] は cumulative[i+4])
            bad.push(i + 4);
        }
        diffs.push(d.clamp(0, u8::MAX as i32) as u8);
    }
    if !bad.is_empty() {
        return Err(SearchError::InvalidDifference { positions: bad });
    }
    Ok((diffs, cumulative.len()))
}

fn kmp_prefix(a: &[u8]) -> Vec<usize> {
    let mut pi = vec![0; a.len()];
    let mut j = 0;
    for i in 1..a.len() {
        while j > 0 && a[i] != a[j] {
            j = pi[j - 1];
        }
        if a[i] == a[j] {
            j += 1;
        }
        pi[i] = j;
    }
    pi
}

/// 乱数列を前から舐めて、生産数の並びに一致する位置を探す
pub struct Searcher {
    state: [u32; 4],
    pattern: Vec<u8>,
    pi: Vec<usize>,
    /// STRIDE 本ぶんの KMP の途中状態
    kmp: [usize; STRIDE],
    r: usize,
    /// 開始位置から進めた数
    consumed: u64,
    /// ヒット位置をフレーム番号に直すための下駄
    offset: i64,
}

impl Searcher {
    /// `start_frame` から探し始める。並びが検索に使えなければ [`SearchError`]
    pub fn new(cumulative: &[Option<u8>], start_frame: u64) -> Result<Self, SearchError> {
        let (pattern, raw_len) = pattern_from(cumulative)?;
        Ok(Self {
            state: jump(start_frame),
            pi: kmp_prefix(&pattern),
            pattern,
            kmp: [0; STRIDE],
            r: 0,
            consumed: 0,
            // 使わなかった先頭 3 回ぶん、初回調合の遅延、各調合による進行 2 の補正
            offset: start_frame as i64 - (STRIDE * 3) as i64 - 15 + 2 * (raw_len as i64 - 1),
        })
    }

    /// `count` 回ぶん乱数を進めて、その間に確定したフレーム位置を返す
    pub fn step(&mut self, count: u64) -> Vec<i64> {
        let n = self.pattern.len();
        let mut hits = Vec::new();
        for _ in 0..count {
            let x = yield_of(self.state[3]);
            self.state = ascend(self.state);

            let mut k = self.kmp[self.r];
            while k > 0 && self.pattern[k] != x {
                k = self.pi[k - 1];
            }
            if self.pattern[k] == x {
                k += 1;
            }
            if k == n {
                hits.push(self.offset + self.consumed as i64 - (STRIDE * (n - 1)) as i64);
                k = self.pi[n - 1];
            }
            self.kmp[self.r] = k;

            self.r = (self.r + 1) % STRIDE;
            self.consumed += 1;
        }
        hits
    }

    /// 開始位置から進めた数
    pub fn consumed(&self) -> u64 {
        self.consumed
    }

    /// 照合に使う差分の個数。生産数 1 個あたり 1.5 bit なので、範囲 N を探すときの
    /// 偽陽性の期待値は N / 2^(1.5 * これ) になる
    pub fn pattern_len(&self) -> usize {
        self.pattern.len()
    }
}

// ---------------------------------------------------------------- 診断

/// 位置を定めるのに使う差分の個数。これを下回ると偶然の一致と区別できない
const PROBE_LEN: usize = 18;
/// 手がかりとして拾う候補の上限
const MAX_ANCHORS: usize = 64;
/// 1 箇所で許すずれ幅
const MAX_STEP_DRIFT: usize = 20;
/// 全体で許すずれ幅
const MAX_TOTAL_DRIFT: usize = 200;
/// ずれ幅を確定するときに先読みする個数。偶然合っただけの位置を弾く
const LOOKAHEAD: usize = 3;

/// 差分列の先頭 `PROBE_LEN` 個が現れる乱数位置を集める
fn find_anchors(probe: &[u8], start_frame: u64, total: u64) -> Vec<u64> {
    let pi = kmp_prefix(probe);
    let mut state = jump(start_frame);
    let mut kmp = [0usize; STRIDE];
    let mut r = 0;
    let mut anchors = Vec::new();
    for i in 0..total {
        let x = yield_of(state[3]);
        state = ascend(state);
        let mut k = kmp[r];
        while k > 0 && probe[k] != x {
            k = pi[k - 1];
        }
        if probe[k] == x {
            k += 1;
        }
        if k == probe.len() {
            anchors.push(start_frame + i - (STRIDE * (probe.len() - 1)) as u64);
            k = pi[probe.len() - 1];
            if anchors.len() >= MAX_ANCHORS {
                break;
            }
        }
        kmp[r] = k;
        r = (r + 1) % STRIDE;
    }
    anchors
}

/// `anchor` から差分列を順に照合し、途中で余分に進んだぶんを記録する。
/// 最後まで合わせられなければ None
fn align(pattern: &[u8], anchor: u64) -> Option<(Vec<Drift>, usize)> {
    let span = STRIDE * (pattern.len() + LOOKAHEAD) + MAX_TOTAL_DRIFT + STRIDE;
    let mut ys = Vec::with_capacity(span);
    let mut s = jump(anchor);
    for _ in 0..span {
        ys.push(yield_of(s[3]));
        s = ascend(s);
    }
    // ずれ幅 d を仮定したとき、i 番目から先読みぶんまで合うか
    let fits = |i: usize, d: usize| {
        (0..=LOOKAHEAD)
            .map(|t| i + t)
            .take_while(|&j| j < pattern.len())
            .all(|j| ys.get(STRIDE * j + d) == Some(&pattern[j]))
    };

    let mut drift = 0;
    let mut drifts = Vec::new();
    for i in 0..pattern.len() {
        if ys.get(STRIDE * i + drift) == Some(&pattern[i]) {
            continue; // そのまま合うので進む
        }
        // 合わないので、ずれ幅を先読みで確かめながら探す。
        // 先読みはここでしか使わない。毎回使うと、ずれの境界をまたいだときに
        // どの幅でも合わなくなってしまう
        let step = (1..=MAX_STEP_DRIFT).find(|&d| fits(i, drift + d))?;
        // 差分列の i 番目は (i + 4) 回目の調合の生産数
        drifts.push(Drift { craft: i + 4, steps: step as u32 });
        drift += step;
        if drift > MAX_TOTAL_DRIFT {
            return None;
        }
    }
    Some((drifts, drift))
}

/// 通常の検索で見つからないとき、前半だけで位置を定めて、
/// 途中で乱数がどれだけ余分に進んだかを調べる。
/// 手がかりが足りない、または候補が絞れない場合は `None`
pub fn diagnose(
    cumulative: &[Option<u8>],
    start_frame: u64,
    total: u64,
) -> Result<Option<Diagnosis>, SearchError> {
    let (pattern, raw_len) = pattern_from(cumulative)?;
    if pattern.len() <= PROBE_LEN {
        return Ok(None);
    }

    let mut found: Option<Diagnosis> = None;
    for anchor in find_anchors(&pattern[..PROBE_LEN], start_frame, total) {
        let Some((drifts, total_drift)) = align(&pattern, anchor) else {
            continue;
        };
        if found.is_some() {
            return Ok(None); // 候補が複数あるなら決められない
        }
        found = Some(Diagnosis {
            frame: anchor as i64 - (STRIDE * 3) as i64 - 15 + 2 * (raw_len as i64 - 1),
            drifts,
            total_drift: total_drift as u32,
        });
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cum(v: &[u8]) -> Vec<Option<u8>> {
        v.iter().map(|&x| Some(x)).collect()
    }

    #[test]
    fn rejects_short_sequences() {
        // 先頭 3 つを落とすと何も残らない
        assert_eq!(pattern_from(&cum(&[0, 3, 5])).unwrap_err(), SearchError::TooShort);
        // 残りが 1 つだけでは差分が作れない
        assert_eq!(pattern_from(&cum(&[0, 3, 5, 7])).unwrap_err(), SearchError::TooShort);
        // 末尾の 99 を落とすと 1 つになる場合も同じ
        assert_eq!(pattern_from(&cum(&[0, 3, 5, 7, 99])).unwrap_err(), SearchError::TooShort);
        // 2 つ残れば差分 1 個で成立する
        assert_eq!(pattern_from(&cum(&[0, 3, 5, 7, 9])).unwrap().0, vec![2]);
    }

    #[test]
    fn rejects_unknown_values() {
        let mut c = cum(&[0, 3, 5, 7, 9, 12]);
        c[4] = None;
        assert_eq!(
            pattern_from(&c).unwrap_err(),
            SearchError::UnknownValue { positions: vec![4] }
        );
    }

    #[test]
    fn rejects_impossible_differences() {
        // cumulative[5] - cumulative[4] = 1 は生産数として成立しない
        assert_eq!(
            pattern_from(&cum(&[0, 3, 5, 7, 9, 10, 13])).unwrap_err(),
            SearchError::InvalidDifference { positions: vec![5] }
        );
    }

    #[test]
    fn drops_the_first_three_and_a_trailing_cap() {
        let (diffs, raw_len) = pattern_from(&cum(&[0, 3, 5, 7, 10, 13, 99])).unwrap();
        assert_eq!(diffs, vec![3, 3]); // 7 -> 10 -> 13 ("99" と先頭 3 つは落ちる)
        assert_eq!(raw_len, 7);
    }

    /// 乱数列から実際に生産数の並びを作り、それを検索して元の位置が出ること
    #[test]
    fn finds_a_sequence_generated_from_the_rng() {
        const START: u64 = 4_000;
        // START から stride 刻みで 12 回ぶんの生産数を作る
        let mut s = jump(START);
        let mut yields = Vec::new();
        for i in 0..STRIDE * 12 {
            if i % STRIDE == 0 {
                yields.push(yield_of(s[3]));
            }
            s = ascend(s);
        }
        // 累計に直す。先頭 3 つは検索に使われないので適当な差 (3) で埋める
        let mut values = vec![0u8, 3, 6, 9];
        for y in &yields {
            values.push(values.last().unwrap() + y);
        }
        let cumulative = cum(&values);

        let mut searcher = Searcher::new(&cumulative, 0).unwrap();
        let hits = searcher.step(START + 10_000);
        let expected = START as i64 - (STRIDE * 3) as i64 - 15 + 2 * (values.len() as i64 - 1);
        assert!(hits.contains(&expected), "hits = {hits:?}, expected {expected}");
    }

    /// 途中から探し始めても、返るのは同じ絶対フレーム位置
    #[test]
    fn a_later_start_finds_the_same_frame() {
        const START: u64 = 4_000;
        let mut s = jump(START);
        let mut values = vec![0u8, 3, 6, 9];
        for i in 0..STRIDE * 12 {
            if i % STRIDE == 0 {
                let v = values.last().unwrap() + yield_of(s[3]);
                values.push(v);
            }
            s = ascend(s);
        }
        let cumulative = cum(&values);
        let expected = START as i64 - (STRIDE * 3) as i64 - 15 + 2 * (values.len() as i64 - 1);

        // 0 から探した場合
        let mut from_zero = Searcher::new(&cumulative, 0).unwrap();
        assert!(from_zero.step(20_000).contains(&expected));

        // ヒットの手前から探した場合。消費数は減るが、返る値は同じ
        let mut from_mid = Searcher::new(&cumulative, 3_000).unwrap();
        let hits = from_mid.step(20_000);
        assert!(hits.contains(&expected), "hits = {hits:?}, expected {expected}");

        // ヒットを過ぎた位置から探すと見つからない
        let mut too_late = Searcher::new(&cumulative, START + 500).unwrap();
        assert!(!too_late.step(20_000).contains(&expected));
    }

    #[test]
    fn pattern_len_counts_the_diffs() {
        let s = Searcher::new(&cum(&[0, 3, 5, 7, 10, 13, 99]), 0).unwrap();
        assert_eq!(s.pattern_len(), 2);
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

        assert_eq!(whole, parts);
        assert_eq!(one.consumed(), chunked.consumed());
    }

    /// 途中で乱数が 1 つ余分に進んだ並びから、その位置とずれ量を割り出せる
    #[test]
    fn diagnose_finds_an_extra_advance() {
        const START: u64 = 60_000;
        const BREAK_AT: usize = 22; // 何個目の差分でずれを起こすか

        // START から stride 5 で生産数を拾い、BREAK_AT の手前で 1 つ余分に進める
        let mut s = jump(START);
        let mut yields = Vec::new();
        let mut extra_done = false;
        for i in 0..30 {
            yields.push(yield_of(s[3]));
            if i == BREAK_AT && !extra_done {
                s = ascend(s);
                extra_done = true;
            }
            for _ in 0..STRIDE {
                s = ascend(s);
            }
        }

        let mut values = vec![0u8, 3, 6, 9];
        for y in &yields {
            values.push(values.last().unwrap() + y);
        }
        let cumulative: Vec<Option<u8>> = values.iter().map(|&v| Some(v)).collect();

        // 通常の検索では見つからない
        let mut searcher = Searcher::new(&cumulative, 0).unwrap();
        assert!(searcher.step(START + 20_000).is_empty(), "ずれた並びが素通りしている");

        // 診断ならずれの位置と量が出る
        let d = diagnose(&cumulative, 0, START + 20_000).unwrap().expect("診断できていない");
        assert_eq!(d.total_drift, 1);
        assert_eq!(d.drifts.len(), 1);
        assert_eq!(d.drifts[0].steps, 1);
        assert_eq!(d.drifts[0].craft, BREAK_AT + 1 + 4);
    }

    /// ずれのない並びなら、診断も通常の検索と同じ位置を返す
    #[test]
    fn diagnose_agrees_with_search_when_clean() {
        const START: u64 = 30_000;
        let mut s = jump(START);
        let mut values = vec![0u8, 3, 6, 9];
        for i in 0..STRIDE * 30 {
            if i % STRIDE == 0 {
                let v = values.last().unwrap() + yield_of(s[3]);
                values.push(v);
            }
            s = ascend(s);
        }
        let cumulative: Vec<Option<u8>> = values.iter().map(|&v| Some(v)).collect();

        let hits = Searcher::new(&cumulative, 0).unwrap().step(START + 10_000);
        let d = diagnose(&cumulative, 0, START + 10_000).unwrap().expect("診断できていない");
        assert_eq!(d.total_drift, 0);
        assert!(d.drifts.is_empty());
        assert!(hits.contains(&d.frame), "hits = {hits:?}, diagnosis = {}", d.frame);
    }

    /// 差分が短すぎると位置を定められないので None
    #[test]
    fn diagnose_gives_up_when_too_short() {
        let short: Vec<Option<u8>> = [0u8, 3, 6, 9, 12, 15].iter().map(|&v| Some(v)).collect();
        assert_eq!(diagnose(&short, 0, 100_000).unwrap(), None);
    }
}
