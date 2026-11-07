use std::collections::HashMap;

use super::numbering::{LevelDef, NumDef, Numbering, Suffix};
use super::numfmt::format_number;

const LEVELS: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Abstract(u32),
    Num(u32),
}

#[derive(Debug, Default)]
pub(super) struct Counters {
    state: HashMap<Key, [Option<i64>; LEVELS]>,
}

impl Counters {
    pub(super) fn next(&mut self, numbering: &Numbering, num_id: u32, ilvl: u8) -> Option<String> {
        if num_id == 0 {
            return None;
        }
        let num = numbering.nums.get(&num_id)?;
        let levels = numbering.abstracts.get(&num.abstract_id)?;
        let ilvl = usize::from(ilvl).min(LEVELS - 1);
        let view = View { num, levels };
        let current = view.level(ilvl)?;
        let key = if num.overrides.values().any(|o| o.start.is_some()) {
            Key::Num(num_id)
        } else {
            Key::Abstract(num.abstract_id)
        };
        let state = self.state.entry(key).or_insert([None; LEVELS]);
        for (m, slot) in state.iter_mut().enumerate().skip(ilvl + 1) {
            let threshold = view.level(m).and_then(|l| l.restart).map_or(m, usize::from);
            if ilvl < threshold {
                *slot = None;
            }
        }
        state[ilvl] = Some(state[ilvl].map_or_else(|| view.start(ilvl), |v| v.saturating_add(1)));
        render(current, &view, state)
    }
}

struct View<'n> {
    num: &'n NumDef,
    levels: &'n HashMap<u8, LevelDef>,
}

impl View<'_> {
    fn level(&self, k: usize) -> Option<&LevelDef> {
        let key = k as u8;
        self.num
            .overrides
            .get(&key)
            .and_then(|o| o.level.as_ref())
            .or_else(|| self.levels.get(&key))
    }

    fn start(&self, k: usize) -> i64 {
        self.num
            .overrides
            .get(&(k as u8))
            .and_then(|o| o.start)
            .or_else(|| self.level(k).map(|l| l.start))
            .unwrap_or(0)
    }
}

fn render(current: &LevelDef, view: &View<'_>, state: &[Option<i64>; LEVELS]) -> Option<String> {
    if current.fmt == "bullet" {
        return None;
    }
    let template = current.text.as_deref()?;
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        let digit = chars.peek().and_then(|d| d.to_digit(10)).filter(|d| (1..=9).contains(d));
        match (c, digit) {
            ('%', Some(d)) => {
                chars.next();
                let k = (d - 1) as usize;
                let value = state[k].unwrap_or_else(|| view.start(k));
                let fmt = match view.level(k) {
                    Some(l) if current.legal && l.fmt != "none" => "decimal",
                    Some(l) => l.fmt.as_str(),
                    None => "decimal",
                };
                out.push_str(&format_number(value, fmt)?);
            }
            _ => out.push(c),
        }
    }
    if out.trim().is_empty() {
        return None;
    }
    match current.suffix {
        Suffix::Tab => out.push('\t'),
        Suffix::Space => out.push(' '),
        Suffix::Nothing => {}
    }
    Some(out)
}
