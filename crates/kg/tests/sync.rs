#[path = "fake/bundle.rs"]
mod bundle;

use bundle::{Spec, TestResult, cfg, lines, nodes, sqlite_files, write_bundle};
use kg::{FindBy, sync_all};
use serde_json::json;

#[test]
fn good_bundle_loads_with_matching_counts() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(tmp.path(), "b1", "g", &lines(&Spec::new("g")), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.disabled().is_empty(), "{:?}", set.disabled());
    let catalog = set.catalog();
    assert_eq!(catalog.len(), 1);
    let entry = &catalog[0];
    assert_eq!((entry.nodes, entry.edges, entry.states), (4, 3, 3));
    assert_eq!(entry.graph, "b1/g");
    assert_eq!(entry.shape, "연결망");
    assert_eq!(
        (entry.compositions, entry.dated, entry.article_index),
        (1, 1, 0)
    );
    assert_eq!(entry.composition_mode.as_deref(), Some("chain_only"));
    assert_eq!(entry.relations, vec!["rel_x", "rel_y", "rel_z"]);
    assert_eq!(entry.state.as_ref().map(|s| s.values.len()), Some(2));
    assert_eq!(entry.signature, "unchecked");
    assert_eq!(entry.acyclic, json!({"checked": ["rel_x"]}));
    assert_eq!(sqlite_files(tmp.path(), "b1"), vec!["g.sqlite".to_string()]);
    Ok(())
}

#[test]
fn removed_line_disables_only_that_bundle() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let mut bad = lines(&Spec::new("g"));
    bad.remove(3);
    write_bundle(tmp.path(), "a_bad", "g", &bad, None)?;
    write_bundle(tmp.path(), "b_good", "h", &lines(&Spec::new("h")), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert_eq!(set.disabled().len(), 1);
    assert_eq!(set.disabled()[0].bundle, "a_bad");
    assert!(
        set.disabled()[0].reason.contains("count"),
        "{}",
        set.disabled()[0].reason
    );
    assert!(sqlite_files(tmp.path(), "a_bad").is_empty());
    assert!(!set.contains("a_bad/g"));
    assert!(set.contains("b_good/h"));
    assert_eq!(
        nodes(&set, "b_good/h", FindBy::Text("alpha"), None, 5)?.len(),
        1
    );
    Ok(())
}

#[test]
fn wrong_sha256_disables_bundle() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let wrong = "0".repeat(64);
    write_bundle(tmp.path(), "b1", "g", &lines(&Spec::new("g")), Some(&wrong))?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.catalog().is_empty());
    assert_eq!(set.disabled().len(), 1);
    assert!(set.disabled()[0].reason.contains("sha256"));
    assert!(sqlite_files(tmp.path(), "b1").is_empty());
    Ok(())
}

#[test]
fn second_sync_skips_unchanged_graph() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(tmp.path(), "b1", "g", &lines(&Spec::new("g")), None)?;
    drop(sync_all(tmp.path(), &cfg())?);
    let db = tmp.path().join("sqlite").join("b1").join("g.sqlite");
    {
        let conn = rusqlite::Connection::open(&db)?;
        conn.execute("INSERT INTO meta (key, value) VALUES ('probe', 'kept')", [])?;
    }
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.disabled().is_empty());
    assert_eq!(set.catalog().len(), 1);
    drop(set);
    let conn = rusqlite::Connection::open(&db)?;
    let probe: String = conn.query_row("SELECT value FROM meta WHERE key = 'probe'", [], |r| {
        r.get(0)
    })?;
    assert_eq!(probe, "kept");
    Ok(())
}

#[test]
fn changed_file_is_reloaded() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(tmp.path(), "b1", "g", &lines(&Spec::new("g")), None)?;
    drop(sync_all(tmp.path(), &cfg())?);
    let mut spec = Spec::new("g");
    spec.extra_edges.push(json!({"edge": {"id": "e9", "source": {"id": "n4"}, "target": {"id": "n3"}, "relation": "rel_y"}}));
    write_bundle(tmp.path(), "b1", "g", &lines(&spec), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert_eq!(set.catalog()[0].edges, 4);
    Ok(())
}

#[test]
fn cache_from_older_loader_is_rebuilt() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(tmp.path(), "b1", "g", &lines(&Spec::new("g")), None)?;
    drop(sync_all(tmp.path(), &cfg())?);
    let db = tmp.path().join("sqlite").join("b1").join("g.sqlite");
    {
        let conn = rusqlite::Connection::open(&db)?;
        conn.execute("UPDATE meta SET value = '1' WHERE key = 'loader'", [])?;
        conn.execute("INSERT INTO meta (key, value) VALUES ('probe', 'old')", [])?;
    }
    drop(sync_all(tmp.path(), &cfg())?);
    let conn = rusqlite::Connection::open(&db)?;
    let probes: i64 = conn.query_row("SELECT count(*) FROM meta WHERE key = 'probe'", [], |r| {
        r.get(0)
    })?;
    assert_eq!(probes, 0);
    Ok(())
}

#[test]
fn unregistered_shape_disables_bundle() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let mut spec = Spec::new("g");
    spec.shape = "unregistered";
    write_bundle(tmp.path(), "b1", "g", &lines(&spec), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.catalog().is_empty());
    assert!(set.disabled()[0].reason.contains("shape"));
    Ok(())
}

#[test]
fn schema_mismatch_disables_bundle() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let mut spec = Spec::new("g");
    spec.schema = "other-consumer/9";
    write_bundle(tmp.path(), "b1", "g", &lines(&spec), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.catalog().is_empty());
    assert!(set.disabled()[0].reason.contains("format name"));
    assert!(sqlite_files(tmp.path(), "b1").is_empty());
    Ok(())
}

#[test]
fn relation_outside_contract_disables_bundle() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let mut spec = Spec::new("g");
    spec.extra_edges.push(json!({"edge": {"id": "e9", "source": {"id": "n1"}, "target": {"id": "n3"}, "relation": "unlisted"}}));
    write_bundle(tmp.path(), "b1", "g", &lines(&spec), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.catalog().is_empty());
    assert!(set.disabled()[0].reason.contains("outside the contract"));
    assert!(sqlite_files(tmp.path(), "b1").is_empty());
    Ok(())
}

#[test]
fn cycle_in_acyclic_layer_disables_but_other_layer_may_cycle() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let mut bad = Spec::new("g");
    bad.extra_edges.push(json!({"edge": {"id": "e9", "source": {"id": "n3"}, "target": {"id": "n1"}, "relation": "rel_x"}}));
    write_bundle(tmp.path(), "a_bad", "g", &lines(&bad), None)?;
    let mut ok = Spec::new("h");
    ok.extra_edges.push(json!({"edge": {"id": "e9", "source": {"id": "n4"}, "target": {"id": "n1"}, "relation": "rel_y"}}));
    write_bundle(tmp.path(), "b_ok", "h", &lines(&ok), None)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert_eq!(set.disabled().len(), 1);
    assert!(set.disabled()[0].reason.contains("cycle"));
    assert!(sqlite_files(tmp.path(), "a_bad").is_empty());
    assert!(set.contains("b_ok/h"));
    Ok(())
}

#[test]
fn missing_consumer_dir_gives_empty_set() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.catalog().is_empty() && set.disabled().is_empty());
    Ok(())
}
