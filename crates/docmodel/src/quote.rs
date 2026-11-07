use unicode_normalization::char::canonical_combining_class;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

pub fn normalize_for_match(s: &str) -> (String, Vec<usize>) {
    let (text, map, _) = normalize_with_ends(s);
    (text, map)
}

fn normalize_with_ends(s: &str) -> (String, Vec<usize>, Vec<usize>) {
    let mut out = Normalized {
        text: String::with_capacity(s.len()),
        map: Vec::with_capacity(s.len()),
        ends: Vec::with_capacity(s.len()),
    };
    let mut segment = String::new();
    let mut segment_start = 0usize;
    for (i, c) in s.chars().enumerate() {
        if !segment.is_empty() && has_boundary_before(c) {
            out.push_segment(&segment, segment_start);
            segment.clear();
        }
        if segment.is_empty() {
            segment_start = i;
        }
        segment.push(c);
    }
    if !segment.is_empty() {
        out.push_segment(&segment, segment_start);
    }
    (out.text, out.map, out.ends)
}

pub fn locate_quote(paged: &str, quote: &str) -> Option<(u32, u32)> {
    let (q, _) = normalize_for_match(quote);
    if q.is_empty() {
        return None;
    }
    let (n, map, ends) = normalize_with_ends(paged);
    let byte = n.find(&q)?;
    let start = n.get(..byte)?.chars().count();
    let end = start + q.chars().count();
    let orig_start = *map.get(start)?;
    let orig_end = *ends.get(end.checked_sub(1)?)?;
    Some((u32::try_from(orig_start).ok()?, u32::try_from(orig_end).ok()?))
}

struct Normalized {
    text: String,
    map: Vec<usize>,
    ends: Vec<usize>,
}

impl Normalized {
    fn push_segment(&mut self, segment: &str, start: usize) {
        let end = start + segment.chars().count();
        for c in segment.nfc() {
            if !c.is_whitespace() {
                self.text.push(c);
                self.map.push(start);
                self.ends.push(end);
            }
        }
    }
}

fn has_boundary_before(c: char) -> bool {
    canonical_combining_class(c) == 0
        && matches!(is_nfc_quick(std::iter::once(c)), IsNormalized::Yes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(s: &str, span: (u32, u32)) -> String {
        s.chars()
            .skip(span.0 as usize)
            .take(span.1.saturating_sub(span.0) as usize)
            .collect()
    }

    const PAGED: &str = "--- p1 ---\n제5조(계약기간) ① 계약기간은\n   \n--- p2 ---\n2026년 1월 1일부터   1년으로 한다.\n";

    #[test]
    fn whitespace_only_difference_is_found() {
        let span = locate_quote(PAGED, "계약기간은 2026년 1월\t1일부터 1년으로 한다.");
        assert_eq!(span, None);
        let span = locate_quote(PAGED, " 2026년 1월\n1일부터\n1년으로   한다. ");
        assert_eq!(
            span.map(|s| slice(PAGED, s)),
            Some("2026년 1월 1일부터   1년으로 한다.".to_string())
        );
        let paged = "계약기간은\n   \n2026년 1월 1일부터";
        let span = locate_quote(paged, "계약기간은 2026년 1월 1일부터");
        assert_eq!(span, Some((0, 23)));
        assert_eq!(span.map(|s| slice(paged, s)), Some(paged.to_string()));
    }

    #[test]
    fn one_character_difference_is_not_found() {
        assert_eq!(locate_quote(PAGED, "2026년 1월 2일부터 1년으로 한다."), None);
        assert_eq!(
            locate_quote(PAGED, "2026년 1월 1일부터 1년으로 한다").map(|s| slice(PAGED, s)),
            Some("2026년 1월 1일부터   1년으로 한다".to_string())
        );
        assert_eq!(
            locate_quote(PAGED, "2026년 1월 1일 부터 1년으로 한다.").map(|s| slice(PAGED, s)),
            Some("2026년 1월 1일부터   1년으로 한다.".to_string())
        );        assert_eq!(locate_quote(PAGED, "   "), None);
    }

    #[test]
    fn nfd_input_is_found() {
        let nfc_quote = "계약기간은";
        let nfd_quote: String = nfc_quote.nfd().collect();
        assert_ne!(nfd_quote.chars().count(), nfc_quote.chars().count());
        let expected = locate_quote(PAGED, nfc_quote);
        assert!(expected.is_some());
        assert_eq!(locate_quote(PAGED, &nfd_quote), expected);

        let nfd_paged: String = PAGED.nfd().collect();
        let span = locate_quote(&nfd_paged, nfc_quote);
        assert_eq!(span.map(|s| slice(&nfd_paged, s)), Some(nfd_quote.clone()));
        let nfd_changed: String = "계약기관은".nfd().collect();
        assert_eq!(locate_quote(&nfd_paged, &nfd_changed), None);
    }
}
