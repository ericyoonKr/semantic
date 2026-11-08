use serde_json::Value;

pub(crate) struct Fields {
    pub gloss: &'static [&'static str],
    pub evidence: &'static [&'static str],
    pub edge_checks: &'static [&'static str],
    pub period: &'static str,
    pub search_records: &'static str,
}

const BASE: Fields = Fields {
    gloss: &["gloss", "definition", "description", "summary"],
    evidence: &["evidence"],
    edge_checks: &[],
    period: "period",
    search_records: "item_state.search_records",
};

const TABLE: &[(&str, Fields)] = &[(
    "연결망",
    Fields {
        gloss: &["gloss", "record.gloss"],
        evidence: &["evidence", "book_evidence"],
        edge_checks: &["target_check", "evidence_check"],
        period: "period",
        search_records: "item_state.search_records",
    },
)];

pub(crate) fn for_shape(shape: &str) -> &'static Fields {
    TABLE
        .iter()
        .find(|(name, _)| *name == shape)
        .map(|(_, f)| f)
        .unwrap_or(&BASE)
}

pub(crate) fn at<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(data, |v, key| v.get(key))
        .filter(|v| !v.is_null())
}

#[cfg(test)]
mod tests {
    use super::{at, for_shape};
    use serde_json::json;

    #[test]
    fn picks_row_by_shape_and_reads_dotted_paths() {
        assert!(for_shape("연결망").evidence.contains(&"book_evidence"));
        assert_eq!(for_shape("나무").evidence, &["evidence"]);
        let v = json!({"record": {"gloss": "g"}, "x": null});
        assert_eq!(at(&v, "record.gloss"), Some(&json!("g")));
        assert_eq!(at(&v, "x"), None);
        assert_eq!(at(&v, "record.none"), None);
    }
}
