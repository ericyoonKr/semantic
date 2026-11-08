use chrono::NaiveDate;
use lawtime::addendum::{Comparator, Outcome, Pivot, TriggerEvent};
use lawtime::facts::EventKind;
use lawtime::versions::pg_rules::{Gap, RawRule, read_rule};
use lawtime::versions::{PromulNo, UnitKey};

type R<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn raw<'a>(no: &'a PromulNo, trigger: Option<&'a str>, outcome: Option<&'a str>, targets: Option<&'a [String]>) -> RawRule<'a> {
    RawRule {
        rule_id: "r1",
        law_id: "001638",
        promul_no: no,
        promul_date: NaiveDate::from_ymd_opt(2023, 1, 3),
        addendum_article: "제2조",
        rule_type: "APPLICATION",
        trigger,
        outcome,
        targets,
        scope_note: None,
        reviewed_by: None,
        original_text: "제2조(적용례)",
        kept: false,
    }
}

#[test]
fn stored_rule_json_becomes_engine_rule() -> R {
    let no = PromulNo::statute(19158);
    let targets = vec!["제148조의2".to_string()];
    let t = r#"{"event_type": "APPLICATION_FILED", "comparator": ">=", "pivot": "EFFECTIVE_DATE", "first_only": true}"#;
    let rule = read_rule(&raw(&no, Some(t), Some(r#"{"apply_version": "NEW"}"#), Some(&targets))).map_err(|g| format!("{g:?}"))?;
    let trig = rule.trigger.ok_or("trigger")?;
    assert_eq!(trig.event, TriggerEvent::Event(EventKind::ApplicationFiled));
    assert_eq!(trig.comparator, Comparator::AtOrAfter);
    assert_eq!(trig.pivot, Pivot::EffectiveDate);
    assert!(trig.first_only);
    assert_eq!(rule.outcome, Outcome::New);
    assert_eq!(rule.targets, vec![UnitKey::article("001638", "제148조의2")]);
    let pending = r#"{"event_type": "PROCEEDING_PENDING", "comparator": "PENDING_AT", "pivot": "2024-01-01"}"#;
    let rule = read_rule(&raw(&no, Some(pending), Some(r#"{"apply_version": "OLD"}"#), Some(&[]))).map_err(|g| format!("{g:?}"))?;
    assert_eq!(rule.trigger.ok_or("trigger")?.pivot, Pivot::Date(NaiveDate::from_ymd_opt(2024, 1, 1).ok_or("날짜")?));
    assert!(rule.targets.is_empty());
    Ok(())
}

#[test]
fn missing_or_unknown_parts_are_gaps_not_guesses() {
    let no = PromulNo::statute(1);
    let t = r#"{"event_type": "ACT", "comparator": "<", "pivot": "EFFECTIVE_DATE"}"#;
    let new = Some(r#"{"apply_version": "NEW"}"#);
    let some: Vec<String> = vec!["제1조".into()];
    assert_eq!(read_rule(&raw(&no, Some(t), new, None)).err(), Some(Gap::NoTargets));
    assert_eq!(read_rule(&raw(&no, None, new, Some(&some))).err(), Some(Gap::NoTrigger));
    let odd = r#"{"event_type": "REPORT_FILED", "comparator": "<", "pivot": "EFFECTIVE_DATE"}"#;
    assert!(matches!(read_rule(&raw(&no, Some(odd), new, Some(&some))), Err(Gap::Unreadable(_))));
    assert!(matches!(read_rule(&raw(&no, Some(t), Some(r#"{"apply_version": "MAYBE"}"#), Some(&some))), Err(Gap::Unreadable(_))));
    assert!(matches!(read_rule(&raw(&no, Some("{"), new, Some(&some))), Err(Gap::Unreadable(_))));
}
