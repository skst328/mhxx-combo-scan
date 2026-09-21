//! 累計個数の並びから、調合開始時の乱数位置 (フレーム) を探す。
//!
//! [`Searcher::step`] は KMP の途中状態を保持したまま中断・再開できるので、
//! 区切って呼んでも境界をまたぐ一致を取りこぼさない。
//!
//! ライセンスは [`crate::rng`] の記載を参照 (apmnnn/mhxx-rng, MIT)。

use crate::rng::{ascend, jump};
use std::collections::BTreeMap;

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

/// 位置を絞るのに使う窓の長さ。ずれをまたがない区間がこれだけ取れれば位置を拾える
const WINDOW: usize = 12;
/// 候補として採るのに必要な、同じ位置を指した窓の数
const MIN_VOTES: usize = 2;
/// 拾う一致の総数の上限
const MAX_ANCHORS: usize = 1 << 18;
/// 照合にかける候補の上限。票の多い順に採る
const MAX_CANDIDATES: usize = 256;
/// 1 箇所で許すずれ幅。観測できたものはすべて ±1 だった
const MAX_STEP_DRIFT: i64 = 2;
/// ずれの合計の上限
const MAX_TOTAL_DRIFT: i64 = 5;
/// ずれ幅を確定するときに先読みする個数。偶然合っただけの位置を弾く
const LOOKAHEAD: usize = 3;

/// 生産数は 2〜4 の 3 通りなので、直近 `WINDOW` 個を 3 進数 1 つに畳める
const SYMBOLS: u32 = 3;
const CODES: u32 = SYMBOLS.pow(WINDOW as u32);
/// 最も古い 1 個を落とすための桁
const DROP: u32 = CODES / SYMBOLS;
/// 引く表が大きくなりすぎないこと。`WINDOW` を伸ばすとここに引っかかる
const _: () = assert!(CODES <= 1 << 21);

/// 窓を引くための表。同じ内容の窓が複数あってもよいよう、連結して持つ
struct WindowTable {
    /// 3 進コード -> その内容を持つ窓のうち最初のもの。`NONE` なら無し
    head: Vec<u16>,
    /// 同じ内容を持つ次の窓
    next: Vec<u16>,
}

const NONE: u16 = u16::MAX;

impl WindowTable {
    fn new(pattern: &[u8]) -> Self {
        let count = pattern.len() - WINDOW + 1;
        let mut t = Self { head: vec![NONE; CODES as usize], next: vec![NONE; count] };
        for k in 0..count {
            let code = pattern[k..k + WINDOW]
                .iter()
                .fold(0u32, |c, &x| c * SYMBOLS + (x - 2) as u32);
            t.next[k] = t.head[code as usize];
            t.head[code as usize] = k as u16;
        }
        t
    }
}

/// 乱数列を 1 回だけ舐めて、窓ごとの一致を集める。
/// 戻り値は (差分列の先頭が来る乱数位置, 一致した窓)
fn find_anchors(pattern: &[u8], start_frame: u64, total: u64) -> Vec<(u64, usize)> {
    let table = WindowTable::new(pattern);
    let lead = (STRIDE * (WINDOW - 1)) as u64;

    let mut state = jump(start_frame);
    // STRIDE 本ぶんの直近 WINDOW 個。それぞれ WINDOW 個溜まるまでは引かない
    let mut code = [0u32; STRIDE];
    let mut filled = [0usize; STRIDE];
    let mut r = 0;
    let mut out = Vec::new();

    for i in 0..total {
        let x = yield_of(state[3]);
        state = ascend(state);
        code[r] = (code[r] % DROP) * SYMBOLS + (x - 2) as u32;
        if filled[r] < WINDOW {
            filled[r] += 1;
        }
        if filled[r] == WINDOW {
            let mut k = table.head[code[r] as usize];
            while k != NONE {
                // 窓の先頭は STRIDE * (WINDOW - 1) だけ手前。そこが差分列の k 番目
                let end = start_frame + i;
                if let Some(base) = end.checked_sub(lead + (STRIDE * k as usize) as u64) {
                    out.push((base, k as usize));
                }
                k = table.next[k as usize];
            }
            if out.len() >= MAX_ANCHORS {
                break;
            }
        }
        r = (r + 1) % STRIDE;
    }
    out
}

/// `anchor` から差分列を順に照合し、想定とずれたぶんを記録する。
///
/// 先頭でいきなりずれる場合は、基準そのものがずれているのと区別が付かないので、
/// ずれとして数えずに基準を動かす。戻り値の最初の要素はその動かしたぶん。
///
/// どのずれ幅でも合わないときは、先頭の 1 個だけ照合から外す。調合開始直後に
/// 捨てる回数 (3 回) が足りないことがあり、そのぶんは乱数のずれではない。
/// 最後まで合わせられなければ None
fn align(pattern: &[u8], anchor: u64) -> Option<Fit> {
    // ずれは負にもなるので、少し手前から乱数を作っておく
    let back = (MAX_TOTAL_DRIFT + STRIDE as i64) as usize;
    if anchor < back as u64 {
        return None;
    }
    let span = back + STRIDE * (pattern.len() + LOOKAHEAD) + back;
    let mut ys = Vec::with_capacity(span);
    let mut s = jump(anchor - back as u64);
    for _ in 0..span {
        ys.push(yield_of(s[3]));
        s = ascend(s);
    }
    // i 番目をずれ d で見たときの生産数。ys は anchor - back から始まる
    let at = |i: usize, d: i64| -> Option<u8> {
        let idx = back as i64 + (STRIDE * i) as i64 + d;
        usize::try_from(idx).ok().and_then(|u| ys.get(u).copied())
    };
    let fits = |i: usize, d: i64| {
        (0..=LOOKAHEAD)
            .map(|t| i + t)
            .take_while(|&j| j < pattern.len())
            .all(|j| at(j, d) == Some(pattern[j]))
    };

    let mut drift = 0;
    let mut shift = 0;
    let mut drifts = Vec::new();
    let mut leading_skipped = false;
    for i in 0..pattern.len() {
        if at(i, drift) == Some(pattern[i]) {
            continue;
        }
        // 合わないので、ずれ幅を先読みで確かめながら小さいほうから探す。
        // 先読みはここでしか使わない。毎回使うと、次のずれの手前で
        // どの幅でも合わなくなってしまう
        let step = (1..=MAX_STEP_DRIFT)
            .flat_map(|a| [a, -a])
            .find(|&d| (drift + d).abs() <= MAX_TOTAL_DRIFT && fits(i, drift + d));
        let Some(step) = step else {
            // 先頭だけは、外して残り全部が合うなら外す
            if i == 0 && (1..pattern.len()).all(|j| at(j, 0) == Some(pattern[j])) {
                leading_skipped = true;
                break;
            }
            return None;
        };
        drift += step;
        if i == 0 {
            shift = drift;
        } else {
            // 差分列の i 番目は (i + 4) 回目の調合の生産数
            drifts.push(Drift { craft: i + 4, steps: step as i32 });
        }
    }
    Some(Fit { shift, drifts, total_drift: drift - shift, leading_skipped })
}

/// `align` が合わせきった結果
struct Fit {
    /// 基準を動かしたぶん
    shift: i64,
    drifts: Vec<Drift>,
    total_drift: i64,
    leading_skipped: bool,
}

/// 票を集めた位置。`base` は差分列の先頭が来る乱数位置
struct Candidate {
    base: u64,
    votes: usize,
    /// `base` を指した窓のうち最も先頭寄りのもの
    first_k: usize,
}

/// 窓ごとに探し、「差分の先頭が来るはずの位置」に換算して数える。
/// ずれの範囲で隣り合う位置は同じ候補とみなし、ずれの無い窓が指したほうを代表にする
fn candidates(pattern: &[u8], start_frame: u64, total: u64) -> Vec<Candidate> {
    // 位置 -> (票数, それを指した窓のうち最も先頭寄りのもの)
    let mut votes: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
    for (base, k) in find_anchors(pattern, start_frame, total) {
        let e = votes.entry(base).or_insert((0, usize::MAX));
        e.0 += 1;
        e.1 = e.1.min(k);
    }

    let mut out: Vec<Candidate> = Vec::new();
    for (base, (n, first_k)) in votes {
        // 直前の候補とずれの範囲で重なるなら、ずれた見え方として同じ位置にまとめる
        match out.last_mut() {
            Some(prev) if base - prev.base <= MAX_TOTAL_DRIFT as u64 => {
                prev.votes += n;
                // 先頭に近い窓が指した位置ほど、ずれの入らない基準に近い
                if first_k < prev.first_k {
                    prev.base = base;
                    prev.first_k = first_k;
                }
            }
            _ => out.push(Candidate { base, votes: n, first_k }),
        }
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.votes));
    out.retain(|c| c.votes >= MIN_VOTES);
    out.truncate(MAX_CANDIDATES);
    out
}

/// 通常の検索で見つからないとき、途中で乱数の進み方がずれた可能性を調べる。
///
/// 差分列を重なり合う窓に切って別々に探し、複数の窓が同じ位置を指したものを候補にする。
/// 早い段階でずれていても、ずれをまたがない窓が 2 つ取れれば位置を絞れる。
/// 候補が 1 つに決まらなければ `None`
pub fn diagnose(
    cumulative: &[Option<u8>],
    start_frame: u64,
    total: u64,
) -> Result<Option<Diagnosis>, SearchError> {
    let (pattern, raw_len) = pattern_from(cumulative)?;
    if pattern.len() <= WINDOW {
        return Ok(None); // 窓が 1 つしか取れないと投票にならない
    }

    // 照合が通ったものを、ずれの少ない順・票の多い順に並べる
    let mut fitted: Vec<(usize, usize, u64, Fit)> = candidates(&pattern, start_frame, total)
        .into_iter()
        .filter_map(|c| {
            let fit = align(&pattern, c.base)?;
            let base = c.base.checked_add_signed(fit.shift)?;
            Some((fit.drifts.len(), c.votes, base, fit))
        })
        .collect();
    fitted.sort_by_key(|(n, votes, ..)| (*n, std::cmp::Reverse(*votes)));

    // 2 番手と決め手が付かないなら答えない
    if let [(n, votes, ..), (n2, votes2, ..), ..] = fitted.as_slice() {
        if (n, votes) == (n2, votes2) {
            return Ok(None);
        }
    }
    let Some((_, _, base, fit)) = fitted.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(Diagnosis {
        frame: base as i64 - (STRIDE * 3) as i64 - 15 + 2 * (raw_len as i64 - 1),
        drifts: fit.drifts,
        total_drift: fit.total_drift as i32,
        leading_skipped: fit.leading_skipped,
    }))
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

    /// `start` から stride 刻みで `count` 回ぶんの生産数を作り、累計の並びに直す。
    /// `drifts` の (位置, 量) は、その回の直後に乱数が余分に進む (負なら足りない) ことを表す
    fn with_drifts(start: u64, count: usize, drifts: &[(usize, i64)]) -> Vec<Option<u8>> {
        let mut s = jump(start);
        // 先頭 3 つは検索に使われないので適当な差 (3) で埋める
        let mut values = vec![0u8, 3, 6, 9];
        for i in 0..count {
            let v = values.last().unwrap() + yield_of(s[3]);
            values.push(v);
            let extra = drifts.iter().find(|(at, _)| *at == i).map_or(0, |(_, d)| *d);
            for _ in 0..(STRIDE as i64 + extra) {
                s = ascend(s);
            }
        }
        values.into_iter().map(Some).collect()
    }

    /// `with_drifts` の (位置, 量) を、診断が返す craft 番号に直す。
    /// ずれが効き始めるのは次の回から
    fn craft_of(at: usize) -> usize {
        at + 1 + 4
    }

    /// 診断が答えるべき値。ずれを足し戻したものが、ずれの無い並びの報告値と一致する
    fn corrected(d: &Diagnosis) -> i64 {
        d.frame + d.total_drift as i64
    }

    /// `with_drifts` で作った並びの、正しい報告値
    fn want(start: u64, cumulative: &[Option<u8>]) -> i64 {
        start as i64 - (STRIDE * 3) as i64 - 15 + 2 * (cumulative.len() as i64 - 1)
    }

    /// 途中で乱数が 1 つ余分に進んだ並びから、その位置とずれ量を割り出せる
    #[test]
    fn diagnose_finds_an_extra_advance() {
        let cumulative = with_drifts(60_000, 30, &[(22, 1)]);

        // 通常の検索では見つからない
        let mut searcher = Searcher::new(&cumulative, 0).unwrap();
        assert!(searcher.step(80_000).is_empty(), "ずれた並びが素通りしている");

        let d = diagnose(&cumulative, 0, 80_000).unwrap().expect("診断できていない");
        assert_eq!(corrected(&d), want(60_000, &cumulative) + 1);
        assert_eq!(d.drifts.iter().map(|x| x.steps).sum::<i32>(), 1);
    }

    /// 乱数が 1 つ足りない方向のずれも、同じように割り出せる
    #[test]
    fn diagnose_finds_a_missing_advance() {
        let cumulative = with_drifts(60_000, 30, &[(22, -1)]);

        let mut searcher = Searcher::new(&cumulative, 0).unwrap();
        assert!(searcher.step(80_000).is_empty(), "ずれた並びが素通りしている");

        let d = diagnose(&cumulative, 0, 80_000).unwrap().expect("診断できていない");
        assert_eq!(corrected(&d), want(60_000, &cumulative) - 1);
        assert_eq!(d.drifts.iter().map(|x| x.steps).sum::<i32>(), -1);
    }

    /// ずれが打ち消し合っていれば、報告値に補正は要らない
    #[test]
    fn diagnose_reports_zero_when_drifts_cancel() {
        let cumulative = with_drifts(60_000, 36, &[(5, -1), (25, 1)]);

        let d = diagnose(&cumulative, 0, 80_000).unwrap().expect("診断できていない");
        assert_eq!(d.total_drift, 0);
        assert_eq!(corrected(&d), want(60_000, &cumulative));
        assert_eq!(d.frame, want(60_000, &cumulative));
    }

    /// ずれは「何回目で起きたか」までは一意に決まらないが、位置と合計は決まる。
    /// ずれの直後の生産数がたまたま一致すると、検出はその先にずれ込む
    #[test]
    fn diagnose_pins_the_total_even_when_the_spot_slips() {
        let cumulative = with_drifts(60_000, 30, &[(22, -1)]);
        let d = diagnose(&cumulative, 0, 80_000).unwrap().expect("診断できていない");

        assert_eq!(d.drifts.len(), 1);
        assert!(
            (craft_of(22)..=craft_of(22) + LOOKAHEAD).contains(&d.drifts[0].craft),
            "craft = {}",
            d.drifts[0].craft
        );
    }

    /// ずれが真ん中にあって、前後のどちらも窓より少し長いだけの並びでも位置を拾える。
    /// 窓を長くすると、どちらの窓も取れずに取りこぼす
    #[test]
    fn diagnose_finds_a_drift_that_splits_the_sequence() {
        // 差分は 30 個。ずれの前が 15 個、後ろが 15 個に割れる
        let cumulative = with_drifts(60_000, 31, &[(14, 1)]);

        let d = diagnose(&cumulative, 0, 80_000).unwrap().expect("診断できていない");
        assert_eq!(corrected(&d), want(60_000, &cumulative) + 1);
        assert_eq!(d.drifts.iter().map(|x| x.steps).sum::<i32>(), 1);
    }

    /// 先頭の 1 個だけが合わないときは、それを外して残りで位置を決める。
    /// 調合開始直後に捨てる 3 回では足りないことがある
    #[test]
    fn diagnose_drops_a_leading_value_that_does_not_fit() {
        let mut cumulative = with_drifts(60_000, 30, &[]);
        // cumulative[3] は差分列の起点。ここだけ動かすと先頭 1 個だけが合わなくなる
        let next = cumulative[4].unwrap();
        let first = next - cumulative[3].unwrap();
        cumulative[3] = Some(next - if first == 2 { 3 } else { 2 });

        let mut searcher = Searcher::new(&cumulative, 0).unwrap();
        assert!(searcher.step(80_000).is_empty(), "合わない並びが素通りしている");

        let d = diagnose(&cumulative, 0, 80_000).unwrap().expect("診断できていない");
        assert!(d.leading_skipped);
        assert!(d.drifts.is_empty());
        assert_eq!(d.total_drift, 0);
        assert_eq!(d.frame, want(60_000, &cumulative));
    }

    /// ずれでは説明できない値が混ざっていたら、位置が分かっていても答えない
    #[test]
    fn diagnose_gives_up_on_a_value_that_is_not_a_drift() {
        let mut cumulative = with_drifts(60_000, 30, &[]);
        // 1 箇所だけ生産数を別の値にする。以降の累計もまとめてずらす
        let wrong = cumulative[20].unwrap().wrapping_add(1);
        let delta = wrong - cumulative[20].unwrap();
        for v in &mut cumulative[20..] {
            *v = Some(v.unwrap() + delta);
        }

        assert_eq!(diagnose(&cumulative, 0, 80_000).unwrap(), None);
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
