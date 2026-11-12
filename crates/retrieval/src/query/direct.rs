use crate::query::annex::AnnexName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Article(String),
    Annex(AnnexName),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Direct {
    pub law: String,
    pub target: Target,
}

fn digits(chars: &[char], mut i: usize) -> (String, usize) {
    let mut out = String::new();
    while i < chars.len() && chars[i].is_ascii_digit() {
        out.push(chars[i]);
        i += 1;
    }
    (out, i)
}

fn skip_space(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn branch(chars: &[char], i: usize) -> (Option<String>, usize) {
    let j = skip_space(chars, i);
    if j < chars.len() && chars[j] == '의' {
        let (b, k) = digits(chars, skip_space(chars, j + 1));
        if !b.is_empty() {
            return (Some(b), k);
        }
    }
    (None, i)
}

fn article_at(chars: &[char], start: usize) -> Option<String> {
    if chars.get(start) != Some(&'제') {
        return None;
    }
    let (num, i) = digits(chars, skip_space(chars, start + 1));
    if num.is_empty() {
        return None;
    }
    let i = skip_space(chars, i);
    if chars.get(i) != Some(&'조') {
        return None;
    }
    let (b, _) = branch(chars, i + 1);
    Some(match b {
        Some(b) => format!("제{num}조의{b}"),
        None => format!("제{num}조"),
    })
}

pub fn article_of(unit: &str) -> Option<String> {
    let chars: Vec<char> = unit.chars().collect();
    article_at(&chars, 0)
}

fn annex_at(chars: &[char], start: usize) -> Option<AnnexName> {
    let rest: String = chars[start..].iter().collect();
    if !rest.starts_with("별표") {
        return None;
    }
    let (num, i) = digits(chars, skip_space(chars, start + 2));
    if num.is_empty() {
        return None;
    }
    let (b, _) = branch(chars, i);
    AnnexName::parse(&format!("별표{num}{}", b.map(|b| format!("의{b}")).unwrap_or_default()))
}

fn is_number_word(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    article_at(&chars, 0).is_some() || annex_at(&chars, 0).is_some()
}

pub fn without_numbers(query: &str) -> String {
    let kept: Vec<&str> = query.split_whitespace().filter(|w| !is_number_word(w)).collect();
    if kept.is_empty() {
        query.to_string()
    } else {
        kept.join(" ")
    }
}

pub fn parse(query: &str) -> Option<Direct> {
    let chars: Vec<char> = query.chars().collect();
    for start in 0..chars.len() {
        let target = article_at(&chars, start)
            .map(Target::Article)
            .or_else(|| annex_at(&chars, start).map(Target::Annex));
        let Some(target) = target else {
            continue;
        };
        let law: String = chars[..start].iter().collect();
        let law = law.trim().trim_start_matches('「').trim_end_matches('」').trim().to_string();
        if law.is_empty() {
            return None;
        }
        return Some(Direct { law, target });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article(law: &str, no: &str) -> Option<Direct> {
        Some(Direct {
            law: law.into(),
            target: Target::Article(no.into()),
        })
    }

    #[test]
    fn law_name_and_article_number_are_split() {
        assert_eq!(parse("지방공무원법 제25조의2"), article("지방공무원법", "제25조의2"));
        assert_eq!(
            parse("공공기관의 정보공개에 관한 법률 시행령 제 2 조 제1항"),
            article("공공기관의 정보공개에 관한 법률 시행령", "제2조")
        );
        assert_eq!(parse("「행정절차법」 제21조의 3"), article("행정절차법", "제21조의3"));
        assert_eq!(parse("국가배상법 제2조의 요건"), article("국가배상법", "제2조"));
    }

    #[test]
    fn annex_numbers_are_read_with_branch() {
        let d = parse("도로교통법 시행령 별표 6의2 과태료");
        let expected = AnnexName::parse("별표6의2").map(|n| Direct {
            law: "도로교통법 시행령".into(),
            target: Target::Annex(n),
        });
        assert_eq!(d, expected);
    }

    #[test]
    fn number_words_are_dropped_for_the_text_side() {
        assert_eq!(without_numbers("행정소송법 제2조"), "행정소송법");
        assert_eq!(without_numbers("도로교통법 시행령 별표6 과태료 제2조의3"), "도로교통법 시행령 과태료");
        assert_eq!(without_numbers("제2조"), "제2조");
        assert_eq!(without_numbers("제2조에 따른 신고"), "따른 신고");
    }

    #[test]
    fn article_of_reads_the_article_at_the_start() {
        assert_eq!(article_of("제2조제1항").as_deref(), Some("제2조"));
        assert_eq!(article_of("제 25 조의 2 제3항").as_deref(), Some("제25조의2"));
        assert_eq!(article_of("별표 1"), None);
        assert_eq!(article_of("2조"), None);
    }

    #[test]
    fn no_law_name_or_no_number_means_no_direct_lookup() {
        assert_eq!(parse("제2조"), None);
        assert_eq!(parse("공무원 과실 손해 배상"), None);
        assert_eq!(parse("국가배상법 제조"), None);
    }
}
