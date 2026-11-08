#[path = "fake/bundle.rs"]
mod bundle;

use bundle::{Spec, TestResult, cfg, edge, expand, lines, nodes, write_bundle};
use kg::{
    Direction, Evidence, ExpandQuery, FindBy, FindQuery, KgError, KgSet, Period, Reason, Status,
    sync_all,
};
use serde_json::json;

fn loaded_with(spec: &Spec<'_>) -> Result<(tempfile::TempDir, KgSet), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    write_bundle(tmp.path(), "b1", "g", &lines(spec), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    Ok((tmp, set))
}

fn loaded() -> Result<(tempfile::TempDir, KgSet), Box<dyn std::error::Error>> {
    loaded_with(&Spec::new("g"))
}

fn ids(hits: &[kg::NodeHit]) -> Vec<&str> {
    hits.iter().map(|h| h.id.as_str()).collect()
}

#[test]
fn find_uses_trigram_for_long_and_like_for_short_queries() -> TestResult {
    let (_tmp, set) = loaded()?;
    let f = |text: &str, kind: Option<&str>| nodes(&set, "g", FindBy::Text(text), kind, 10);
    assert_eq!(ids(&f("alpha", None)?), vec!["n1"]);
    assert_eq!(ids(&f("lph", None)?), vec!["n1"]);
    assert_eq!(ids(&f("xy", None)?), vec!["n4"]);
    assert_eq!(ids(&f("p", None)?), vec!["n4", "n3", "n1"]);
    assert_eq!(ids(&f("%", None)?), Vec::<&str>::new());
    assert_eq!(ids(&f("group", Some("kind_a"))?), Vec::<&str>::new());
    assert_eq!(ids(&f("group", Some("kind_b"))?), vec!["n3"]);
    assert_eq!(ids(&f("a \"q", None)?), Vec::<&str>::new());
    let beta = nodes(&set, "g", FindBy::Text("beta"), None, 1)?;
    assert_eq!(
        beta[0].state,
        Some(json!({"judgment": "unknown", "reason": "reason text"}))
    );
    assert_eq!(beta[0].gloss, None);
    assert!(matches!(f("  ", None), Err(KgError::Argument(_))));
    assert!(matches!(
        nodes(&set, "g", FindBy::Text("alpha"), None, 21),
        Err(KgError::Argument(_))
    ));
    Ok(())
}

#[test]
fn find_reports_one_of_five_statuses() -> TestResult {
    let (_tmp, set) = loaded()?;
    let q = |by| FindQuery {
        by,
        kind: None,
        limit: 5,
        as_of: None,
    };
    let found = set.find("g", &q(FindBy::Text("gamma")))?;
    assert_eq!((found.status, found.reason), (Status::Found, None));
    assert_eq!(found.nodes[0].gloss.as_deref(), Some("gamma gloss"));
    let none = set.find("g", &q(FindBy::Text("nothing here")))?;
    assert_eq!(
        (none.status, none.reason),
        (Status::Unknown, Some(Reason::NoMatch))
    );
    let graph = set.find("zz", &q(FindBy::Text("alpha")))?;
    assert_eq!(graph.status, Status::OutOfScope);
    let law = set.find("g", &q(FindBy::LawKey("k")))?;
    assert_eq!(
        (law.status, law.reason),
        (Status::OutOfScope, Some(Reason::NoArticleIndex))
    );
    let article = set.find(
        "g",
        &q(FindBy::Article {
            law: "법",
            unit: "제1조",
        }),
    )?;
    assert_eq!(article.reason, Some(Reason::NoArticleIndex));
    Ok(())
}

#[test]
fn expand_respects_direction_depth_and_contract() -> TestResult {
    let (_tmp, set) = loaded()?;
    let out1 = expand(&set, "n1", None, Direction::Out, 1, 20)?;
    let mut pairs: Vec<(&str, &str)> = out1
        .edges
        .iter()
        .map(|e| (e.src.as_str(), e.dst.as_str()))
        .collect();
    pairs.sort();
    assert_eq!(pairs, vec![("n1", "n2"), ("n1", "n4")]);
    assert_eq!(ids(&out1.nodes).len(), 2);
    assert_eq!(out1.status, Status::Found);

    let out2 = expand(&set, "n1", None, Direction::Out, 2, 20)?;
    assert_eq!(out2.edges.len(), 3);
    assert!(
        out2.edges
            .iter()
            .any(|e| e.src == "n2" && e.dst == "n3" && e.depth == 2)
    );
    assert_eq!(out2.refused_paths, 0);
    assert_eq!(out2.composition_mode.as_deref(), Some("chain_only"));

    let inbound = expand(&set, "n2", None, Direction::In, 2, 20)?;
    assert_eq!(inbound.edges.len(), 1);
    assert_eq!(
        (inbound.edges[0].src.as_str(), inbound.edges[0].dst.as_str()),
        ("n1", "n2")
    );

    let only = expand(&set, "n1", Some("rel_y"), Direction::Out, 2, 20)?;
    assert_eq!(ids(&only.nodes), vec!["n4"]);

    let capped = expand(&set, "n1", None, Direction::Out, 2, 1)?;
    assert_eq!(capped.edges.len(), 1);

    let outside = expand(&set, "n1", Some("unlisted"), Direction::Out, 1, 5)?;
    assert_eq!(
        (outside.status, outside.reason),
        (Status::OutOfScope, Some(Reason::RelationNotInContract))
    );
    for depth in [0, 3] {
        assert!(matches!(
            expand(&set, "n1", None, Direction::Out, depth, 5),
            Err(KgError::Argument(_))
        ));
    }
    let missing = expand(&set, "nx", None, Direction::Out, 1, 5)?;
    assert_eq!(missing.reason, Some(Reason::NodeNotInGraph));
    Ok(())
}

#[test]
fn second_step_follows_only_declared_compositions() -> TestResult {
    let mut spec = Spec::new("g");
    spec.compositions = json!([["rel_x", "kind_a", "rel_x"], ["rel_y", "kind_a", "rel_x"]]);
    spec.extra_edges
        .push(edge("e8", "n2", "n4", "rel_y", json!([])));
    spec.extra_edges
        .push(edge("e9", "n4", "n3", "rel_x", json!([])));
    let (_tmp, set) = loaded_with(&spec)?;
    let out = expand(&set, "n1", None, Direction::Out, 2, 20)?;
    let second: Vec<(&str, &str)> = out
        .edges
        .iter()
        .filter(|e| e.depth == 2)
        .map(|e| (e.src.as_str(), e.dst.as_str()))
        .collect();
    assert_eq!(second, vec![("n2", "n3"), ("n4", "n3")]);
    assert_eq!(out.refused_paths, 1);
    let back = expand(&set, "n3", None, Direction::In, 2, 20)?;
    assert!(
        back.edges
            .iter()
            .any(|e| e.src == "n1" && e.dst == "n2" && e.depth == 2)
    );
    assert!(
        back.edges
            .iter()
            .any(|e| e.src == "n2" && e.dst == "n4" && e.depth == 2)
    );
    assert_eq!(back.refused_paths, 0);

    let mut bare = Spec::new("g");
    bare.compositions = json!([]);
    let (_tmp2, plain) = loaded_with(&bare)?;
    let two = expand(&plain, "n1", None, Direction::Out, 2, 20)?;
    assert_eq!(
        (two.status, two.reason, two.edges.len()),
        (Status::OutOfScope, Some(Reason::NoCompositions), 0)
    );
    assert_eq!(
        expand(&plain, "n1", None, Direction::Out, 1, 20)?
            .edges
            .len(),
        2
    );
    Ok(())
}

#[test]
fn empty_expansion_uses_search_records_and_never_says_none_without_them() -> TestResult {
    let (_tmp, set) = loaded()?;
    let none = expand(&set, "n4", Some("rel_x"), Direction::Out, 1, 5)?;
    assert_eq!(
        (none.status, none.reason),
        (Status::NoneConfirmed, Some(Reason::RecordedNone))
    );
    assert_eq!(none.basis, vec!["reason text".to_string()]);
    let unknown = expand(&set, "n4", Some("rel_z"), Direction::In, 1, 5)?;
    assert_eq!(unknown.status, Status::Unknown);
    let unseen = expand(&set, "n4", Some("rel_y"), Direction::Out, 1, 5)?;
    assert_eq!(
        (unseen.status, unseen.reason),
        (Status::NotExamined, Some(Reason::NoRecord))
    );
    let all = expand(&set, "n4", None, Direction::Out, 1, 5)?;
    assert_eq!(all.status, Status::Unknown);
    let leaf = expand(&set, "n3", None, Direction::Out, 1, 5)?;
    assert_eq!(leaf.status, Status::NotExamined);
    Ok(())
}

#[test]
fn as_of_judges_known_periods_and_counts_exclusions() -> TestResult {
    let (_tmp, set) = loaded()?;
    let q = |as_of| ExpandQuery {
        relation: None,
        direction: Direction::Out,
        depth: 1,
        limit: 20,
        as_of,
    };
    let late = set.expand("g", "n1", &q(Some("2024-01-01")))?;
    assert_eq!(late.edges.len(), 1);
    assert_eq!(late.edges[0].period, Some(Period::NotChecked));
    let summary = late.as_of.ok_or("summary")?;
    assert_eq!(
        (summary.excluded, summary.not_checked, summary.in_force),
        (1, 1, 0)
    );
    let early = set.expand("g", "n1", &q(Some("2020-06-01")))?;
    assert!(
        early
            .edges
            .iter()
            .any(|e| e.period == Some(Period::InForce))
    );
    let only = ExpandQuery {
        relation: Some("rel_y"),
        ..q(Some("2024-01-01"))
    };
    let gone = set.expand("g", "n1", &only)?;
    assert_eq!(
        (gone.status, gone.reason),
        (Status::Unknown, Some(Reason::ExcludedByAsOf))
    );
    assert!(matches!(
        set.expand("g", "n1", &q(Some("2024-13-01"))),
        Err(KgError::Argument(_))
    ));
    assert_eq!(set.expand("g", "n1", &q(None))?.as_of, None);
    Ok(())
}

#[test]
fn expand_reads_evidence_and_checks_from_the_shape_table() -> TestResult {
    let (_tmp, set) = loaded()?;
    let out = expand(&set, "n1", Some("rel_x"), Direction::Out, 2, 20)?;
    let e1 = out
        .edges
        .iter()
        .find(|e| e.dst == "n2")
        .ok_or("missing e1")?;
    let Evidence::Present(items) = &e1.evidence else {
        return Err("e1 evidence".into());
    };
    assert_eq!(items[0].field, "evidence");
    assert_eq!(
        items[0].item,
        json!({"source": "s", "quote": "quoted line"})
    );
    assert_eq!(e1.certainty, Some(json!("uncertain")));
    assert_eq!(e1.checks["evidence_check"]["state"], json!("unchecked"));
    let e2 = out
        .edges
        .iter()
        .find(|e| e.dst == "n3")
        .ok_or("missing e2")?;
    let Evidence::Present(book) = &e2.evidence else {
        return Err("e2 evidence".into());
    };
    assert_eq!(book[0].field, "book_evidence");
    let n2 = out
        .nodes
        .iter()
        .find(|n| n.id == "n2")
        .ok_or("missing n2")?;
    assert_eq!(
        n2.state,
        Some(json!({"judgment": "unknown", "reason": "reason text"}))
    );
    assert_eq!(n2.evidence, Evidence::NoEvidence);

    let mut other = Spec::new("g");
    other.shape = "나무";
    let (_tmp2, tree) = loaded_with(&other)?;
    let out = expand(&tree, "n2", Some("rel_x"), Direction::Out, 1, 20)?;
    assert_eq!(out.edges[0].evidence, Evidence::NoEvidence);
    assert!(out.edges[0].checks.is_empty());
    assert_eq!(out.nodes[0].gloss, None);
    Ok(())
}
