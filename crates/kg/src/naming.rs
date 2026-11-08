use std::collections::BTreeMap;

const DATE_LEN: usize = 11;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Edition<'a> {
    pub date: &'a str,
    pub run: u64,
}

fn date_tail(name: &str) -> Option<(&str, &str)> {
    let bytes = name.as_bytes();
    let start = bytes.len().checked_sub(DATE_LEN)?;
    let tail = bytes.get(start..)?;
    let shape_ok = tail.iter().enumerate().all(|(i, b)| match i {
        0 | 5 | 8 => *b == b'-',
        _ => b.is_ascii_digit(),
    });
    if !shape_ok || start == 0 {
        return None;
    }
    Some((name.get(..start)?, name.get(start + 1..)?))
}

pub(crate) fn split(bundle: &str) -> (&str, Option<Edition<'_>>) {
    if let Some((branch, date)) = date_tail(bundle) {
        return (branch, Some(Edition { date, run: 0 }));
    }
    let Some((head, run)) = bundle.rsplit_once('-') else {
        return (bundle, None);
    };
    if run.is_empty() || !run.bytes().all(|b| b.is_ascii_digit()) {
        return (bundle, None);
    }
    match (date_tail(head), run.parse::<u64>()) {
        (Some((branch, date)), Ok(run)) => (branch, Some(Edition { date, run })),
        _ => (bundle, None),
    }
}

pub fn bundle_branch(bundle: &str) -> &str {
    split(bundle).0
}

pub fn graph_name(bundle: &str, graph: &str) -> String {
    format!("{}/{graph}", bundle_branch(bundle))
}

pub(crate) fn newest<'a>(bundles: impl IntoIterator<Item = &'a str>) -> BTreeMap<&'a str, &'a str> {
    let mut best: BTreeMap<&str, (Edition<'_>, &str)> = BTreeMap::new();
    for bundle in bundles {
        let (branch, Some(edition)) = split(bundle) else {
            continue;
        };
        let newer = best
            .get(branch)
            .is_none_or(|(e, name)| (&edition, bundle) > (e, *name));
        if newer {
            best.insert(branch, (edition, bundle));
        }
    }
    best.into_iter().map(|(b, (_, name))| (b, name)).collect()
}

pub(crate) fn superseded<'a>(bundles: impl Iterator<Item = &'a str> + Clone) -> Vec<bool> {
    let kept = newest(bundles.clone());
    bundles
        .map(|name| {
            kept.get(bundle_branch(name))
                .is_some_and(|keep| *keep != name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{bundle_branch, graph_name, newest, split};

    #[test]
    fn strips_date_and_run_tail() {
        assert_eq!(
            bundle_branch("legal-concept-civil-2026-11-08-2"),
            "legal-concept-civil"
        );
        assert_eq!(
            bundle_branch("legal-concept-constitutional-2026-11-07"),
            "legal-concept-constitutional"
        );
        assert_eq!(bundle_branch("semantic-rules"), "semantic-rules");
        assert_eq!(bundle_branch("b1"), "b1");
        assert_eq!(bundle_branch("2026-11-08"), "2026-11-08");
        assert_eq!(bundle_branch("x-2026-1-08"), "x-2026-1-08");
        assert_eq!(bundle_branch("x-2026-11-08-"), "x-2026-11-08-");
        assert_eq!(bundle_branch("x-2026-11-08-2a"), "x-2026-11-08-2a");
        assert_eq!(bundle_branch("법-2026-11-08"), "법");
        assert_eq!(
            graph_name("legal-concept-civil-2026-11-08-2", "concept_frame"),
            "legal-concept-civil/concept_frame"
        );
        assert_eq!(split("a-2026-11-08-3").1.map(|e| e.run), Some(3));
    }

    #[test]
    fn keeps_latest_date_then_run_per_branch() {
        let names = [
            "a-2026-11-07",
            "a-2026-11-08",
            "a-2026-11-08-2",
            "b-2026-01-01",
            "c",
        ];
        let kept = newest(names);
        assert_eq!(kept.get("a"), Some(&"a-2026-11-08-2"));
        assert_eq!(kept.get("b"), Some(&"b-2026-01-01"));
        assert_eq!(kept.get("c"), None);
        assert_eq!(
            super::superseded(["x", "x-2026-11-08"].into_iter()),
            vec![true, false]
        );
        assert_eq!(
            super::superseded(names.into_iter()),
            vec![true, true, false, false, false]
        );
        assert_eq!(
            newest(["a-2026-12-01", "a-2026-11-30-9"]).get("a"),
            Some(&"a-2026-12-01")
        );
    }
}
