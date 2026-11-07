pub const MASK: &str = "[가림]";

fn in_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn sensitive(groups: &[&str]) -> bool {
    let total: usize = groups.iter().map(|g| g.len()).sum();
    match groups.len() {
        1 => (10..=14).contains(&total),
        2..=4 => groups.iter().all(|g| (2..=12).contains(&g.len())) && (9..=16).contains(&total),
        _ => false,
    }
}

fn push_word(out: &mut String, word: &str) {
    let groups: Vec<&str> = word.split('-').collect();
    if groups.iter().all(|g| is_digits(g)) && sensitive(&groups) {
        out.push_str(MASK);
    } else {
        out.push_str(word);
    }
}

pub fn mask(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if in_word(c) {
            start.get_or_insert(i);
        } else {
            if let Some(s) = start.take() {
                push_word(&mut out, &text[s..i]);
            }
            out.push(c);
        }
    }
    if let Some(s) = start {
        push_word(&mut out, &text[s..]);
    }
    out
}

#[cfg(test)]
#[path = "mask_tests.rs"]
mod tests;
