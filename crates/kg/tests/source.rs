use std::fs;
use std::path::Path;

use kg::{Direction, ExpandQuery, FindBy, FindQuery, KgConfig, sync_all};
use sha2::{Digest, Sha256};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TREE: &str = "# kgio-stream/1
schema_version: test-source/1
id: req
roots:
- R
# kgio-stream-list: nodes
nodes:
- id: R
  label: 처분의 효력
  kind: rule
  gate: AND
  period:
    state: unknown
- id: L1
  label: 취소·철회 또는 소멸 전
  kind: rule
  gate: LEAF
- id: L2
  label: 무효인 처분
  kind: rule
  gate: LEAF
# kgio-stream-list: edges
edges:
- id: e1
  source:
    graph: req
    id: R
  target:
    graph: req
    id: L1
  relation: element
- id: e2
  source:
    graph: req
    id: R
  target:
    graph: req
    id: L2
  relation: defense
notes: []
";

const NET: &str = "schema_version: test-source/1
id: cf
roots: []
nodes:
- id: c1
  label: 처분
  kind: concept
  record:
    aliases:
    - 행정처분
    gloss: 행정청의 공권력 행사
- id: c2
  label: 행정행위
  kind: concept
edges:
- id: c1#r0
  source:
    graph: cf
    id: c1
  target:
    graph: cf
    id: c2
  relation: contrasts_with
notes: []
";

const CONTRACT: &str = "graph_contracts:
  req:
    node_kinds: [rule]
    relations:
      element: {}
    optional_relations: [defense, rebuttal]
  cf:
    node_kinds: [concept]
    relations: {}
    optional_relations: [is_a, part_of, contrasts_with]
    composition_mode: chain_only
graph_locations:
  req: graphs/req.yaml
  cf: graphs/cf.yaml
  absent: graphs/absent.yaml
";

const TYPES: &str = "graph_types:
  R:
    groups:
      판정 관계: [req]
  C:
    families: [cf]
query_shapes:
  나무:
    state:
      kind: truth
      values: [충족, 불충족, 모름, 주장됨]
  연결망:
    state:
      kind: note
      values: [해당, 해당 아님, 참고함]
";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn write_source(root: &Path, bundle: &str, tree: &str) -> Result<(), std::io::Error> {
    let dir = root.join("source").join(bundle);
    fs::create_dir_all(dir.join("graphs"))?;
    let files = [
        ("legal_contract.yaml", CONTRACT),
        ("types.yaml", TYPES),
        ("graphs/req.yaml", TREE),
        ("graphs/cf.yaml", NET),
    ];
    let mut build = String::from("파일:\n");
    for (rel, text) in files {
        fs::write(dir.join(rel), text)?;
        let listed = if rel == "graphs/req.yaml" { sha(tree.as_bytes()) } else { sha(text.as_bytes()) };
        build.push_str(&format!("  {rel}: {listed}\n"));
    }
    fs::write(dir.join("legal_build.yaml"), build)
}

fn cfg() -> KgConfig {
    KgConfig::new("test-consumer/1", "test-manifest/1").with_source_schema(Some("test-source/1".into()))
}

#[test]
fn yaml_source_loads_tree_and_net_with_shapes_states_and_aliases() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_source(tmp.path(), "admin-2026-11-10", TREE)?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert!(set.disabled().is_empty(), "{:?}", set.disabled());
    let catalog = set.catalog();
    let tree = catalog.iter().find(|e| e.graph == "admin/req").ok_or("tree missing")?;
    let net = catalog.iter().find(|e| e.graph == "admin/cf").ok_or("net missing")?;
    assert_eq!((tree.shape.as_str(), tree.nodes, tree.edges), ("나무", 3, 2));
    assert_eq!((net.shape.as_str(), net.nodes, net.edges), ("연결망", 2, 1));
    assert_eq!(tree.state.as_ref().map(|s| s.kind.as_str()), Some("truth"));
    assert_eq!(net.state.as_ref().map(|s| s.values.len()), Some(3));
    assert_eq!(catalog.len(), 2);

    let by_alias = FindQuery { by: FindBy::Text("행정처분"), kind: None, limit: 5, as_of: None };
    let found = set.find("admin/cf", &by_alias)?;
    assert_eq!(found.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["c1"]);
    let short = FindQuery { by: FindBy::Text("처분"), kind: None, limit: 5, as_of: None };
    assert_eq!(set.find("admin/cf", &short)?.nodes.len(), 1);

    let q = ExpandQuery { relation: Some("defense"), direction: Direction::Out, depth: 1, limit: 10, as_of: None };
    let ex = set.expand("admin/req", "R", &q)?;
    assert_eq!(ex.edges.iter().map(|e| e.dst.as_str()).collect::<Vec<_>>(), vec!["L2"]);
    Ok(())
}

#[test]
fn yaml_source_with_changed_graph_is_disabled() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_source(tmp.path(), "admin-2026-11-10", TREE)?;
    let changed = tmp.path().join("source/admin-2026-11-10/graphs/req.yaml");
    fs::write(&changed, TREE.replace("무효인 처분", "무효 처분"))?;
    let set = sync_all(tmp.path(), &cfg())?;
    assert_eq!(set.disabled().len(), 1);
    assert!(set.disabled()[0].reason.contains("sha256"), "{}", set.disabled()[0].reason);
    assert!(set.catalog().is_empty());
    Ok(())
}

#[test]
fn tree_from_yaml_source_folds_elements_and_blocks_on_met_defense() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_source(tmp.path(), "admin-2026-11-10", TREE)?;
    let set = sync_all(tmp.path(), &cfg())?;
    let tree = set.tree("admin/req", "R", 100)?;
    assert_eq!(tree.nodes.len(), 3);
    let states = |pairs: &[(&str, kg::Truth)]| {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<std::collections::BTreeMap<_, _>>()
    };
    let held = kg::judge(&tree, &states(&[("L1", kg::Truth::Met)]));
    assert_eq!(held.value, kg::Truth::Met);
    assert_eq!(held.open_leaves, vec!["L2".to_string()]);
    let blocked = kg::judge(&tree, &states(&[("L1", kg::Truth::Met), ("L2", kg::Truth::Met)]));
    assert_eq!(blocked.value, kg::Truth::NotMet);
    Ok(())
}
