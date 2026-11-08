#[path = "fake/bundle.rs"]
mod bundle;

use std::fs;

use bundle::{Spec, TestResult, cfg, lines, nodes, sqlite_files, write_bundle};
use kg::{FindBy, sync_all};
use serde_json::Value;

#[test]
fn same_graph_name_in_two_branches_loads_both() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let body = lines(&Spec::new("concept_frame"));
    write_bundle(
        tmp.path(),
        "civil-2026-11-08-2",
        "concept_frame",
        &body,
        None,
    )?;
    write_bundle(
        tmp.path(),
        "constitutional-2026-11-07",
        "concept_frame",
        &body,
        None,
    )?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.disabled().is_empty(), "{:?}", set.disabled());
    let names: Vec<String> = set.catalog().into_iter().map(|e| e.graph).collect();
    assert_eq!(
        names,
        vec!["civil/concept_frame", "constitutional/concept_frame"]
    );
    let bundles: Vec<String> = set.catalog().into_iter().map(|e| e.bundle).collect();
    assert_eq!(
        bundles,
        vec!["civil-2026-11-08-2", "constitutional-2026-11-07"]
    );
    for name in &names {
        assert_eq!(nodes(&set, name, FindBy::Text("alpha"), None, 5)?.len(), 1);
    }
    assert_eq!(
        sqlite_files(tmp.path(), "civil-2026-11-08-2"),
        vec!["concept_frame.sqlite".to_string()]
    );
    Ok(())
}

#[test]
fn older_edition_of_same_branch_is_disabled() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let body = lines(&Spec::new("g"));
    for bundle in [
        "a-2026-11-07",
        "a-2026-11-08",
        "a-2026-11-08-2",
        "a-2026-11-08-10",
    ] {
        write_bundle(tmp.path(), bundle, "g", &body, None)?;
    }
    let set = sync_all(tmp.path(), &cfg())?;
    let catalog = set.catalog();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].graph, "a/g");
    assert_eq!(catalog[0].bundle, "a-2026-11-08-10");
    let off: Vec<(&str, &str)> = set
        .disabled()
        .iter()
        .map(|d| (d.bundle.as_str(), d.reason.as_str()))
        .collect();
    assert_eq!(
        off,
        vec![
            ("a-2026-11-07", "같은 갈래의 새 판이 있음"),
            ("a-2026-11-08", "같은 갈래의 새 판이 있음"),
            ("a-2026-11-08-2", "같은 갈래의 새 판이 있음"),
        ]
    );
    assert!(sqlite_files(tmp.path(), "a-2026-11-07").is_empty());
    Ok(())
}

#[test]
fn bundle_without_date_tail_keeps_whole_name_as_branch() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(
        tmp.path(),
        "semantic-rules",
        "g",
        &lines(&Spec::new("g")),
        None,
    )?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.contains("semantic-rules/g"));
    assert!(!set.contains("g"));
    assert_eq!(kg::graph_name("semantic-rules", "g"), "semantic-rules/g");
    Ok(())
}

#[test]
fn source_bundle_must_match_folder_name() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let dir = write_bundle(
        tmp.path(),
        "a-2026-11-08",
        "g",
        &lines(&Spec::new("g")),
        None,
    )?;
    let path = dir.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path)?)?;
    manifest["source_bundle"] = Value::String("a-2026-11-07".into());
    fs::write(&path, manifest.to_string())?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.catalog().is_empty());
    assert!(set.disabled()[0].reason.contains("source_bundle"));
    Ok(())
}

#[test]
fn duplicate_graph_inside_one_bundle_still_disables_it() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let dir = write_bundle(
        tmp.path(),
        "a-2026-11-08",
        "g",
        &lines(&Spec::new("g")),
        None,
    )?;
    let path = dir.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let first = manifest["graphs"][0].clone();
    if let Some(list) = manifest["graphs"].as_array_mut() {
        list.push(first);
    }
    fs::write(&path, manifest.to_string())?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.catalog().is_empty());
    assert!(set.disabled()[0].reason.contains("more than one bundle"));
    Ok(())
}
