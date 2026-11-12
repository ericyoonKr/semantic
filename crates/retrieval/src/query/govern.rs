use sqlx::PgPool;

use crate::error::RetrievalError;
use crate::query::direct;
use crate::query::laws::resolve_law;
use crate::query::terms::compact;

const NAME_ENDINGS: [&str; 4] = ["법", "법률", "령", "규칙"];
const SUBORDINATE: [&str; 2] = ["시행령", "시행규칙"];
const PARTICLES: [&str; 16] = [
    "에서는", "에서", "에는", "상의", "상", "으로", "로", "의", "에", "은", "는", "이", "가", "을", "를", "과",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    Only(Vec<String>),
    Prefer(Vec<String>),
}

impl Scope {
    pub fn only(&self) -> Option<&[String]> {
        match self {
            Scope::Only(ids) => Some(ids),
            _ => None,
        }
    }

    pub fn ids(&self) -> &[String] {
        match self {
            Scope::All => &[],
            Scope::Only(ids) | Scope::Prefer(ids) => ids,
        }
    }

    pub fn preferred(&self) -> Option<&[String]> {
        match self {
            Scope::Prefer(ids) => Some(ids),
            _ => None,
        }
    }
}

fn strip_particle(word: &str) -> &str {
    for p in PARTICLES {
        if let Some(stem) = word.strip_suffix(p)
            && NAME_ENDINGS.iter().any(|e| stem.ends_with(e))
        {
            return stem;
        }
    }
    word
}

fn bracketed(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('「') {
        let after = &rest[open + '「'.len_utf8()..];
        let Some(close) = after.find('」') else { break };
        let name = after[..close].trim();
        if !name.is_empty() {
            out.push(name.to_string());
        }
        rest = &after[close + '」'.len_utf8()..];
    }
    out
}

pub fn mentioned_names(text: &str) -> Vec<String> {
    let mut out: Vec<String> = bracketed(text);
    if let Some(d) = direct::parse(text) {
        out.push(d.law);
    }
    let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric())).collect();
    for (i, word) in words.iter().enumerate() {
        let stem = strip_particle(word);
        if SUBORDINATE.contains(&stem) || !NAME_ENDINGS.iter().any(|e| stem.ends_with(e)) {
            continue;
        }
        if let Some(next) = words.get(i + 1).map(|n| strip_particle(n))
            && SUBORDINATE.contains(&next)
        {
            out.push(format!("{stem} {next}"));
        }
        out.push(stem.to_string());
    }
    let mut seen: Vec<String> = Vec::new();
    out.retain(|n| {
        let key = compact(n);
        if key.is_empty() || seen.contains(&key) {
            return false;
        }
        seen.push(key);
        true
    });
    out
}

async fn with_subordinates(pool: &PgPool, name: &str) -> Result<Vec<String>, RetrievalError> {
    let base = compact(name);
    let names: Vec<String> = std::iter::once(base.clone()).chain(SUBORDINATE.iter().map(|s| format!("{base}{s}"))).collect();
    Ok(sqlx::query_scalar("SELECT law_id FROM law WHERE regexp_replace(name, '\\s', '', 'g') = ANY($1) ORDER BY law_id")
        .bind(names)
        .fetch_all(pool)
        .await?)
}

pub async fn governing_laws(pool: &PgPool, text: &str) -> Result<Vec<String>, RetrievalError> {
    named_laws(pool, &mentioned_names(text)).await
}

pub async fn named_laws(pool: &PgPool, names: &[String]) -> Result<Vec<String>, RetrievalError> {
    let mut ids: Vec<String> = Vec::new();
    for name in names {
        let Some((law_id, resolved, _)) = resolve_law(pool, name).await? else { continue };
        let mut found = with_subordinates(pool, &resolved).await?;
        if !found.contains(&law_id) {
            found.push(law_id);
        }
        for id in found {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::mentioned_names;

    #[test]
    fn law_names_are_found_with_particles_removed_and_subordinate_rules_joined() {
        assert_eq!(mentioned_names("행정소송법상 처분의 개념은?"), ["행정소송법"]);
        assert_eq!(mentioned_names("「국가배상법」에 따른 배상 기준"), ["국가배상법"]);
        assert_eq!(mentioned_names("건축법 시행령에서 정한 용도"), ["건축법 시행령", "건축법"]);
        assert_eq!(mentioned_names("지방공무원법 제25조의2"), ["지방공무원법"]);
    }

    #[test]
    fn ordinary_words_are_kept_as_candidates_only_when_they_end_like_a_law_name() {
        assert_eq!(mentioned_names("영업정지 처분을 다투는 방법"), ["방법"]);
        assert!(mentioned_names("허가 취소 처분의 요건").is_empty());
    }
}
