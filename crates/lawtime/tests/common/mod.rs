#![allow(dead_code)]

use chrono::NaiveDate;
use lawtime::addendum::{AddendumRule, Comparator, FullRevision, Outcome, Pivot, RuleSet, RuleSource, RuleType, Trigger, TriggerEvent};
use lawtime::facts::EventKind;
use lawtime::period::{Period, ymd};
use lawtime::principles::Graph;
use lawtime::versions::{EndReason, MemoryStore, PromulNo, UnitKey, Version};
use serde_json::{Value, json};

pub type R<T = ()> = Result<T, Box<dyn std::error::Error>>;

const RULES: [(&str, &str, &[&str]); 25] = [
    ("P:행위시법", "형사", &["T:행위시"]),
    ("P:포괄일죄실행종료", "형사", &["T:실행종료"]),
    ("P:형사절차신법", "형사", &["T:진행중"]),
    ("X:경한신법", "형사", &["T:행위시", "T:재판시"]),
    ("X:동기설폐기", "형사", &[]),
    ("X:면소", "형사", &["T:재판시"]),
    ("X:확정후집행면제", "형사", &["T:확정후"]),
    ("P:제재행위시법", "행정", &["T:행위시"]),
    ("X:제재경한신법", "행정", &["T:행위시", "T:처분시"]),
    ("P:과태료행위시법", "행정", &["T:행위시"]),
    ("X:과태료유리한신법", "행정", &[]),
    ("X:과태료확정후면제", "행정", &["T:확정후"]),
    ("P:처분시법", "행정", &["T:처분시"]),
    ("P:행정소급금지", "행정", &["T:완성종결"]),
    ("P:위법판단처분시", "행정", &["T:처분시"]),
    ("P:민법소급효", "민사", &["T:시행일"]),
    ("P:민사소송신법", "민사", &["T:진행중"]),
    ("P:납세의무성립시기", "조세", &["T:납세의무성립"]),
    ("P:소급과세금지", "조세", &["T:납세의무성립"]),
    ("P:상속준거법사망시", "국제사법", &["T:사망시"]),
    ("P:경과규정우선", "공통", &[]),
    ("P:부진정소급허용", "공통", &["T:진행중"]),
    ("P:위헌결정장래효", "헌법재판", &["T:위헌결정일"]),
    ("X:형벌조항소급실효", "헌법재판", &["T:시행일", "T:종전합헌결정다음날"]),
    ("X:위헌소급효제한", "헌법재판", &[]),
];

const TIMES: [&str; 13] = [
    "T:행위시", "T:실행종료", "T:진행중", "T:재판시", "T:확정후", "T:처분시", "T:완성종결",
    "T:시행일", "T:납세의무성립", "T:사망시", "T:위헌결정일", "T:종전합헌결정다음날", "T:공포일",
];

const FIELDS: [&str; 7] = ["형사", "행정", "민사", "조세", "국제사법", "공통", "헌법재판"];

pub fn graph_json(extra_nodes: Vec<Value>, extra_edges: Vec<Value>) -> Value {
    let mut nodes: Vec<Value> = TIMES.iter().map(|t| json!({"id": t, "type": "기준시점", "label": t})).collect();
    nodes.extend(FIELDS.iter().map(|f| json!({"id": format!("F:{f}"), "type": "분야", "label": f, "desc": "시험"})));
    nodes.push(json!({"id": "S:시험근거", "type": "조문", "label": "시험 근거", "meta": {"법령ID": "000000"}}));
    let mut edges = Vec::new();
    for (id, field, times) in RULES {
        let kind = if id.starts_with("X:") { "예외" } else { "원칙" };
        nodes.push(json!({
            "id": id, "type": kind, "label": format!("{id} 시험 문구"), "field": field,
            "times": times, "sources": [{"source": "S:시험근거", "quote": "시험 인용", "ok": true}]
        }));
        edges.push(json!({"from": id, "to": "S:시험근거", "rel": "근거", "quote": "시험 인용"}));
        edges.push(json!({"from": format!("F:{field}"), "to": id, "rel": "분야"}));
        for t in times {
            edges.push(json!({"from": id, "to": t, "rel": "기준 시점"}));
        }
    }
    nodes.extend(extra_nodes);
    edges.extend(extra_edges);
    let mut by_kind = serde_json::Map::new();
    for n in &nodes {
        let k = n["type"].as_str().unwrap_or_default().to_string();
        let c = by_kind.get(&k).and_then(Value::as_u64).unwrap_or(0) + 1;
        by_kind.insert(k, json!(c));
    }
    json!({
        "기준일": "2026-11-08",
        "검사": {"노드": nodes.len(), "간선": edges.len(), "종류별 노드": by_kind,
                 "인용 원문 대조": "시험", "근거 없는 원칙": 0, "순서 관계 순환": 0, "오류 수": 0},
        "오류": [],
        "인용검사": [{"원칙": "P:행위시법", "근거": "시험 근거", "원문에 있음": true}],
        "nodes": nodes,
        "edges": edges,
        "판본": {},
        "_source": {"file": "시험", "sha256": "0"}
    })
}

pub fn graph() -> R<Graph> {
    Ok(Graph::from_json(&graph_json(Vec::new(), Vec::new()).to_string())?)
}

pub fn d(y: i32, m: u32, day: u32) -> R<NaiveDate> {
    Ok(ymd(y, m, day)?)
}

pub fn version(unit: &UnitKey, no: u32, promul: NaiveDate, from: NaiveDate, until: Option<NaiveDate>, end: Option<EndReason>) -> R<Version> {
    Ok(Version {
        unit: unit.clone(),
        promul_no: PromulNo::statute(no),
        promul_date: promul,
        valid_period: Period::new(from, until)?,
        end_reason: end,
        body: format!("{} 시험 본문", unit.key),
    })
}

pub fn road_unit() -> UnitKey {
    UnitKey::article("도로교통법", "제148조의2제1항")
}

pub fn road_store() -> R<MemoryStore> {
    let mut s = MemoryStore::new();
    let unit = road_unit();
    let placeholder = d(1900, 1, 1)?;
    s.add_version(version(&unit, 0, placeholder, placeholder, Some(d(2020, 12, 10)?), Some(EndReason::Amended))?)?;
    s.add_version(version(&unit, 17371, d(2020, 6, 9)?, d(2020, 12, 10)?, None, None)?)?;
    s.add_promulgation("도로교통법", PromulNo::statute(17371), d(2020, 6, 9)?, Some(d(2020, 12, 10)?));
    Ok(s)
}

pub fn animal_unit() -> UnitKey {
    UnitKey::article("동물보호법", "제33조제4항제5호나목")
}

pub fn animal_store() -> R<MemoryStore> {
    let mut s = MemoryStore::new();
    s.add_version(version(&animal_unit(), 16075, d(2018, 12, 24)?, d(2019, 3, 25)?, None, None)?)?;
    s.add_promulgation("동물보호법", PromulNo::statute(16075), d(2018, 12, 24)?, Some(d(2019, 3, 25)?));
    Ok(s)
}

pub fn tax_unit() -> UnitKey {
    UnitKey::article("조세감면규제법", "법률제4285호부칙제23조")
}

pub fn tax_store() -> R<MemoryStore> {
    let mut s = MemoryStore::new();
    let p = d(1990, 12, 31)?;
    s.add_version(version(&tax_unit(), 4285, p, p, Some(d(1994, 1, 1)?), Some(EndReason::Amended))?)?;
    s.add_promulgation("조세감면규제법", PromulNo::statute(4285), p, None);
    s.add_promulgation("조세감면규제법", PromulNo::statute(4666), d(1993, 12, 31)?, Some(d(1994, 1, 1)?));
    Ok(s)
}

pub fn tax_rules(kept: bool) -> R<RuleSet> {
    Ok(RuleSet::new(vec![AddendumRule {
        rule_id: "L4285-ADD-23".to_string(),
        source: RuleSource {
            law_id: "조세감면규제법".to_string(),
            promul_no: PromulNo::statute(4285),
            promul_date: d(1990, 12, 31)?,
            addendum_article: "부칙 제23조".to_string(),
        },
        rule_type: RuleType::Application,
        targets: vec![tax_unit()],
        trigger: Some(Trigger {
            event: TriggerEvent::Event(EventKind::Disposition),
            comparator: Comparator::AtOrAfter,
            pivot: Pivot::EffectiveDate,
            first_only: false,
        }),
        outcome: Outcome::New,
        scope_note: "시험".to_string(),
        reviewed_by: Some("시험 고정 데이터".to_string()),
        original_text: "시험".to_string(),
        retired_by_full_revision: Some(FullRevision { promul_no: PromulNo::statute(4666), effective: d(1994, 1, 1)? }),
        kept_flag: kept,
    }]))
}

pub fn simple_rule(
    id: &str,
    unit: &UnitKey,
    promul: (u32, NaiveDate),
    rule_type: RuleType,
    trigger: Trigger,
    outcome: Outcome,
) -> AddendumRule {
    AddendumRule {
        rule_id: id.to_string(),
        source: RuleSource {
            law_id: unit.law_id.clone(),
            promul_no: PromulNo::statute(promul.0),
            promul_date: promul.1,
            addendum_article: "부칙 제2조".to_string(),
        },
        rule_type,
        targets: vec![unit.clone()],
        trigger: Some(trigger),
        outcome,
        scope_note: "시험".to_string(),
        reviewed_by: Some("시험 고정 데이터".to_string()),
        original_text: "시험".to_string(),
        retired_by_full_revision: None,
        kept_flag: false,
    }
}

pub fn trig(event: EventKind, comparator: Comparator, pivot: Pivot) -> Trigger {
    Trigger { event: TriggerEvent::Event(event), comparator, pivot, first_only: false }
}
