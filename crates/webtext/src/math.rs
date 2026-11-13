const SYMBOLS: [(&str, &str); 19] = [
    ("rightarrow", "→"),
    ("to", "→"),
    ("rarr", "→"),
    ("leftarrow", "←"),
    ("gets", "←"),
    ("Rightarrow", "⇒"),
    ("Leftarrow", "⇐"),
    ("leftrightarrow", "↔"),
    ("Leftrightarrow", "⇔"),
    ("times", "×"),
    ("cdot", "·"),
    ("le", "≤"),
    ("leq", "≤"),
    ("ge", "≥"),
    ("geq", "≥"),
    ("ne", "≠"),
    ("neq", "≠"),
    ("approx", "≈"),
    ("pm", "±"),
];

fn symbol(name: &str) -> Option<&'static str> {
    SYMBOLS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

fn command_at(chars: &[char], at: usize) -> Option<(String, usize)> {
    if chars.get(at) != Some(&'\\') {
        return None;
    }
    let end = (at + 1..=chars.len()).find(|&i| !chars.get(i).is_some_and(char::is_ascii_alphabetic)).unwrap_or(chars.len());
    (end > at + 1).then(|| (chars[at + 1..end].iter().collect(), end))
}

fn skip_spaces(chars: &[char], mut at: usize) -> usize {
    while chars.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    at
}

pub fn plain_math(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' {
            let inner = skip_spaces(&chars, i + 1);
            if let Some((name, after)) = command_at(&chars, inner) {
                let close = skip_spaces(&chars, after);
                if chars.get(close) == Some(&'$')
                    && let Some(sym) = symbol(&name)
                {
                    out.push_str(sym);
                    i = close + 1;
                    continue;
                }
            }
        }
        if let Some((name, after)) = command_at(&chars, i)
            && let Some(sym) = symbol(&name)
        {
            out.push_str(sym);
            i = after;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}
