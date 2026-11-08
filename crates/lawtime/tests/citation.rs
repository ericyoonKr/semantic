mod common;

use common::{R, animal_store, animal_unit, d, road_store, road_unit, tax_store, tax_unit};
use lawtime::citation::{Bound, CiteStatus, CiteType, MistakeReason, parse_citations, parse_one, resolve_citation};
use lawtime::versions::PromulNo;

const PHRASES: [(&str, CiteType); 8] = [
    ("구 도로교통법(2020. 6. 9. 법률 제17371호로 개정되어 2020. 12. 10. 시행되기 전의 것)", CiteType::UpperWithEffective),
    ("구 국토의계획 및 이용에 관한 법률(2002. 12. 30. 법률 제6841호로 개정되기 전의 것, 이하 '국토계획법'이라 한다)", CiteType::UpperOnly),
    ("구 국세기본법(1996. 12. 30. 법률 제5189호로 개정되고, 2006. 12. 30. 법률 제8139호로 개정되기 전의 것)", CiteType::BothSides),
    ("구 조세감면규제법(1993. 12. 31. 법률 제4666호로 전부 개정되기 전의 것)", CiteType::FullRevisionUpper),
    ("구 조세감면규제법(1990. 12. 31. 법률 제4285호) 부칙 제23조", CiteType::AsPromulgated),
    ("2018. 12. 24. 법률 제16075호로 일부 개정되어 2019. 3. 25.부터 시행된 동물보호법 제33조 제4항 제5호 (나)목", CiteType::Prose),
    ("구 소득세법 시행령(1998. 12. 31. 대통령령 제16184호로 개정되기 전의 것)", CiteType::Subordinate),
    ("도로교통법 제44조 제1항", CiteType::Unmarked),
];

#[test]
fn eight_phrases_parse_to_their_types() -> R {
    let mut read = 0;
    for (text, want) in PHRASES {
        let Some(c) = parse_one(text)? else { continue };
        assert_eq!(c.cite_type(), want, "{text}");
        read += 1;
    }
    assert_eq!(read, 8);
    Ok(())
}

#[test]
fn phrase_fields_are_read() -> R {
    let names: Vec<String> = PHRASES
        .iter()
        .filter_map(|(t, _)| parse_one(t).ok().flatten())
        .map(|c| c.law_name)
        .collect();
    assert_eq!(
        names,
        vec!["도로교통법", "국토의계획 및 이용에 관한 법률", "국세기본법", "조세감면규제법", "조세감면규제법", "동물보호법", "소득세법 시행령", "도로교통법"]
    );
    let Some(c) = parse_one(PHRASES[4].0)? else { return Err("공포 단위 지정을 못 읽음".into()) };
    assert_eq!(c.article.as_deref(), Some("부칙 제23조"));
    let Some(c) = parse_one(PHRASES[5].0)? else { return Err("산문형을 못 읽음".into()) };
    assert_eq!(c.article.as_deref(), Some("제33조 제4항 제5호 (나)목"));
    let Some(c) = parse_one(PHRASES[2].0)? else { return Err("양측 경계를 못 읽음".into()) };
    match c.bound {
        Bound::Both { lower, upper, .. } => {
            assert_eq!(lower.no, PromulNo::statute(5189));
            assert_eq!(upper.no, PromulNo::statute(8139));
        }
        other => return Err(format!("양측 경계가 아님: {other:?}").into()),
    }
    Ok(())
}

#[test]
fn same_below_carries_bound_to_unmarked_mention() -> R {
    let text = "구 도로교통법(2020. 6. 9. 법률 제17371호로 개정되기 전의 것, 이하 같다) 제148조의2 제1항, 도로교통법 제44조 제1항";
    let cites = parse_citations(text)?;
    assert_eq!(cites.len(), 2);
    assert!(cites[1].inherited);
    assert_eq!(cites[1].cite_type(), CiteType::UpperOnly);
    assert_eq!(cites[1].article.as_deref(), Some("제44조 제1항"));
    Ok(())
}

#[test]
fn resolves_case_citations_to_versions() -> R {
    let road = road_store()?;
    let Some(c) = parse_one(&format!("{} 제148조의2 제1항", PHRASES[0].0))? else { return Err("못 읽음".into()) };
    let r = resolve_citation(&road, &c, &road_unit(), d(2022, 12, 22)?)?;
    assert_eq!(r.status, CiteStatus::Resolved);
    assert_eq!(r.base_date, Some(d(2020, 12, 9)?));
    assert_eq!(r.version.map(|v| v.promul_no), Some(PromulNo::statute(0)));

    let animal = animal_store()?;
    let Some(c) = parse_one(PHRASES[5].0)? else { return Err("못 읽음".into()) };
    let r = resolve_citation(&animal, &c, &animal_unit(), d(2023, 2, 2)?)?;
    assert_eq!(r.base_date, Some(d(2019, 3, 25)?));
    assert_eq!(r.version.map(|v| v.promul_no), Some(PromulNo::statute(16075)));

    let tax = tax_store()?;
    for text in [PHRASES[3].0, PHRASES[4].0] {
        let Some(c) = parse_one(text)? else { return Err("못 읽음".into()) };
        let r = resolve_citation(&tax, &c, &tax_unit(), d(2012, 7, 26)?)?;
        assert_eq!(r.status, CiteStatus::Resolved, "{text}");
        assert_eq!(r.version.map(|v| v.promul_no), Some(PromulNo::statute(4285)));
    }
    Ok(())
}

#[test]
fn wrong_citations_become_mistake_candidates() -> R {
    let road = road_store()?;
    let cases = [
        "구 도로교통법(2020. 6. 8. 법률 제17371호로 개정되기 전의 것)",
        "구 도로교통법(2020. 6. 9. 법률 제17372호로 개정되기 전의 것)",
        "구 도로교통법(2020. 6. 9. 법률 제17371호로 개정되고, 1900. 1. 1. 법률 제0호로 개정되기 전의 것)",
    ];
    let mut reasons = Vec::new();
    for text in cases {
        let Some(c) = parse_one(text)? else { return Err(format!("못 읽음: {text}").into()) };
        let r = resolve_citation(&road, &c, &road_unit(), d(2022, 12, 22)?)?;
        match r.status {
            CiteStatus::MistakeCandidate(m) => reasons.push(m),
            CiteStatus::Resolved => return Err(format!("오기 후보가 아님: {text}").into()),
        }
    }
    assert!(matches!(reasons[0], MistakeReason::PromulDateMismatch { .. }));
    assert!(matches!(reasons[1], MistakeReason::UnknownPromulgation(_)));
    assert!(matches!(reasons[2], MistakeReason::InvertedBounds { .. }));
    Ok(())
}
