use unicode_normalization::UnicodeNormalization;

pub const MAX_CHARS: usize = 180;
const MAX_EXT_CHARS: usize = 16;

pub fn clean(raw: &str, max_chars: usize) -> Option<String> {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let nfc: String = base.nfc().filter(|c| !c.is_control()).collect();
    let name = nfc.trim();
    if name.is_empty() || name.chars().all(|c| c == '.') {
        return None;
    }
    if name.chars().count() <= max_chars {
        return Some(name.to_string());
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e)
        .filter(|e| !e.is_empty() && e.chars().count() <= MAX_EXT_CHARS);
    Some(match ext {
        Some(ext) => {
            let keep = max_chars.saturating_sub(ext.chars().count() + 1);
            let stem: String = name.chars().take(keep).collect();
            format!("{stem}.{ext}")
        }
        None => name.chars().take(max_chars).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_nfc_without_folders_and_bounded() {
        let decomposed = "\u{1100}\u{1161}.docx";
        assert_eq!(clean(decomposed, MAX_CHARS).as_deref(), Some("가.docx"));
        assert_eq!(
            clean("C:\\a\\b\\x.hwpx", MAX_CHARS).as_deref(),
            Some("x.hwpx")
        );
        assert_eq!(
            clean("../../etc/y.docx", MAX_CHARS).as_deref(),
            Some("y.docx")
        );
        assert_eq!(clean("a\u{0}b.docx", MAX_CHARS).as_deref(), Some("ab.docx"));
        assert_eq!(clean("  ", MAX_CHARS), None);
        assert_eq!(clean("..", MAX_CHARS), None);
        let long = format!("{}.docx", "가".repeat(300));
        let out = clean(&long, MAX_CHARS).unwrap_or_default();
        assert_eq!(out.chars().count(), MAX_CHARS);
        assert!(out.ends_with(".docx"));
    }
}
