use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

static NUMBERS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"\d{4}[가-힣]{1,2}\d+|\d{4}\.\d{1,2}\.\d{1,2}\.|\d+\.\d+원|[\d,]*\d원|제\d+조(?:의\d+)?(?:제\d+항)?(?:제\d+호)?|\(\d\)|\d{1,2}\)|「[^」\s]{1,60}」").ok()
});

#[derive(Debug, Default, Clone, Copy)]
pub struct Score {
    pub pages: usize,
    pub chars: usize,
    pub edits: usize,
    pub hits: usize,
    pub got: usize,
    pub numbers: usize,
    pub numbers_ok: usize,
    pub secs: f64,
}

fn squash(text: &str) -> Vec<char> {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn overlap(a: &[char], b: &[char]) -> usize {
    let mut pool = HashMap::<char, usize>::new();
    for c in b {
        *pool.entry(*c).or_default() += 1;
    }
    a.iter()
        .filter(|c| pool.get_mut(c).is_some_and(|n| *n > 0 && { *n -= 1; true }))
        .count()
}

pub fn compare(truth: &str, got: &str, secs: f64) -> Score {
    let (t, g) = (squash(truth), squash(got));
    let (t_text, g_text): (String, String) = (t.iter().collect(), g.iter().collect());
    let tokens: Vec<&str> = NUMBERS.as_ref().map(|re| re.find_iter(&t_text).map(|m| m.as_str()).collect()).unwrap_or_default();
    Score {
        pages: 1,
        chars: t.len(),
        edits: edit_distance(&t, &g),
        hits: overlap(&t, &g),
        got: g.len(),
        numbers: tokens.len(),
        numbers_ok: tokens.iter().filter(|tok| g_text.contains(*tok)).count(),
        secs,
    }
}

impl std::ops::AddAssign for Score {
    fn add_assign(&mut self, o: Self) {
        self.pages += o.pages;
        self.chars += o.chars;
        self.edits += o.edits;
        self.hits += o.hits;
        self.got += o.got;
        self.numbers += o.numbers;
        self.numbers_ok += o.numbers_ok;
        self.secs += o.secs;
    }
}

impl fmt::Display for Score {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ratio = |a: usize, b: usize| a as f64 / b.max(1) as f64;
        write!(
            f,
            "pages {}\tchars {}\tCER {:.3}\trecall {:.3}\tprecision {:.3}\tnumbers {}/{}\t{:.1}s/page",
            self.pages,
            self.chars,
            ratio(self.edits, self.chars),
            ratio(self.hits, self.chars),
            ratio(self.hits, self.got),
            self.numbers_ok,
            self.numbers,
            self.secs / self.pages.max(1) as f64
        )
    }
}

#[cfg(test)]
mod tests {
    use super::compare;

    #[test]
    fn counts_edits_overlap_and_number_tokens_ignoring_spaces() {
        let s = compare("선고 2019다213344 판결", "선고 2019다2334 판결", 1.0);
        assert_eq!((s.chars, s.got, s.edits, s.hits), (15, 13, 2, 13));
        assert_eq!((s.numbers, s.numbers_ok), (1, 0));
    }
}
