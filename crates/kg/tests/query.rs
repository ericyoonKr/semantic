#[path = "fake/bundle.rs"]
mod bundle;

use bundle::{Spec, TestResult, cfg, lines, write_bundle};
use kg::{Direction, Evidence, FindBy, KgError, KgSet, sync_all};
use serde_json::json;

fn loaded() -> Result<(tempfile::TempDir, KgSet), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    write_bundle(tmp.path(), "b1", "g", &lines(&Spec::new("g")), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    Ok((tmp, set))
}

fn ids(hits: &[kg::NodeHit]) -> Vec<&str> {
    hits.iter().map(|h| h.id.as_str()).collect()
}

#[test]
fn find_uses_trigram_for_long_and_like_for_short_queries() -> TestResult {
    let (_tmp, set) = loaded()?;
    assert_eq!(
        ids(&set.find("g", FindBy::Text("alpha"), None, 10)?),
        vec!["n1"]
    );
    assert_eq!(
        ids(&set.find("g", FindBy::Text("lph"), None, 10)?),
        vec!["n1"]
    );
    assert_eq!(
        ids(&set.find("g", FindBy::Text("xy"), None, 10)?),
        vec!["n4"]
    );
    assert_eq!(
        ids(&set.find("g", FindBy::Text("p"), None, 10)?),
        vec!["n4", "n3", "n1"]
    );
    assert_eq!(
        ids(&set.find("g", FindBy::Text("%"), None, 10)?),
        Vec::<&str>::new()
    );
    assert_eq!(
        ids(&set.find("g", FindBy::Text("group"), Some("kind_a"), 10)?),
        Vec::<&str>::new()
    );
    assert_eq!(
        ids(&set.find("g", FindBy::Text("group"), Some("kind_b"), 10)?),
        vec!["n3"]
    );
    assert_eq!(
        ids(&set.find("g", FindBy::Text("a \"q"), None, 10)?),
        Vec::<&str>::new()
    );
    let beta = set.find("g", FindBy::Text("beta"), None, 1)?;
    assert_eq!(
        beta[0].state,
        Some(json!({"judgment": "unknown", "reason": "reason text"}))
    );
    assert!(matches!(
        set.find("g", FindBy::Text("alpha"), None, 21),
        Err(KgError::Argument(_))
    ));
    assert!(matches!(
        set.find("g", FindBy::Text("  "), None, 5),
        Err(KgError::Argument(_))
    ));
    assert!(matches!(
        set.find("zz", FindBy::Text("alpha"), None, 5),
        Err(KgError::UnknownGraph)
    ));
    assert!(set.find("g", FindBy::LawKey("k"), None, 5)?.is_empty());
    Ok(())
}

#[test]
fn expand_respects_direction_depth_and_contract() -> TestResult {
    let (_tmp, set) = loaded()?;
    let out1 = set.expand("g", "n1", None, Direction::Out, 1, 20)?;
    let mut pairs: Vec<(&str, &str)> = out1
        .edges
        .iter()
        .map(|e| (e.src.as_str(), e.dst.as_str()))
        .collect();
    pairs.sort();
    assert_eq!(pairs, vec![("n1", "n2"), ("n1", "n4")]);
    assert_eq!(ids(&out1.nodes).len(), 2);

    let out2 = set.expand("g", "n1", None, Direction::Out, 2, 20)?;
    assert_eq!(out2.edges.len(), 3);
    assert!(
        out2.edges
            .iter()
            .any(|e| e.src == "n2" && e.dst == "n3" && e.depth == 2)
    );

    let inbound = set.expand("g", "n2", None, Direction::In, 2, 20)?;
    assert_eq!(inbound.edges.len(), 1);
    assert_eq!(
        (inbound.edges[0].src.as_str(), inbound.edges[0].dst.as_str()),
        ("n1", "n2")
    );

    let only = set.expand("g", "n1", Some("rel_y"), Direction::Out, 2, 20)?;
    assert_eq!(ids(&only.nodes), vec!["n4"]);

    let capped = set.expand("g", "n1", None, Direction::Out, 2, 1)?;
    assert_eq!(capped.edges.len(), 1);

    assert!(matches!(
        set.expand("g", "n1", Some("unlisted"), Direction::Out, 1, 5),
        Err(KgError::RelationNotAllowed)
    ));
    assert!(matches!(
        set.expand("g", "n1", None, Direction::Out, 3, 5),
        Err(KgError::Argument(_))
    ));
    assert!(matches!(
        set.expand("g", "n1", None, Direction::Out, 0, 5),
        Err(KgError::Argument(_))
    ));
    assert!(matches!(
        set.expand("g", "nx", None, Direction::Out, 1, 5),
        Err(KgError::UnknownNode)
    ));
    Ok(())
}

#[test]
fn expand_keeps_edge_evidence_and_certainty_as_given() -> TestResult {
    let (_tmp, set) = loaded()?;
    let out = set.expand("g", "n1", Some("rel_x"), Direction::Out, 2, 20)?;
    let e1 = out
        .edges
        .iter()
        .find(|e| e.dst == "n2")
        .ok_or("missing e1")?;
    assert_eq!(
        e1.evidence,
        Evidence::Present(vec![json!({"source": "s", "quote": "quoted line"})])
    );
    assert_eq!(e1.certainty, Some(json!("uncertain")));
    let e2 = out
        .edges
        .iter()
        .find(|e| e.dst == "n3")
        .ok_or("missing e2")?;
    assert_eq!(e2.evidence, Evidence::NoEvidence);
    let n3 = out
        .nodes
        .iter()
        .find(|n| n.id == "n3")
        .ok_or("missing n3")?;
    assert_eq!(n3.data["evidence"][0]["quote"], json!("node quote"));
    let n2 = out
        .nodes
        .iter()
        .find(|n| n.id == "n2")
        .ok_or("missing n2")?;
    assert_eq!(
        n2.state,
        Some(json!({"judgment": "unknown", "reason": "reason text"}))
    );
    Ok(())
}
