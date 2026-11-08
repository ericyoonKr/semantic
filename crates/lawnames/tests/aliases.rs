use lawnames::{Aliases, Entry, Error, key};

fn entry(alias: &str, law_id: &str) -> Entry {
    Entry { alias: alias.into(), law_id: law_id.into(), name: "정식".into(), source: "시험".into() }
}

#[test]
fn spacing_and_middle_dot_variants_match() -> Result<(), Error> {
    let a = Aliases::from_entries(vec![entry("10ㆍ27법난법", "010719"), entry("헌법", "001444")])?;
    assert_eq!(a.get("10·27 법난법").map(|e| e.law_id.as_str()), Some("010719"));
    assert_eq!(a.get(" 헌 법 ").map(|e| e.law_id.as_str()), Some("001444"));
    assert!(a.get("구 헌법").is_none());
    assert_eq!(key("가ㆍ나 다"), "가·나다");
    Ok(())
}

#[test]
fn duplicate_alias_refuses_to_load() {
    let r = Aliases::from_entries(vec![entry("민소법", "001700"), entry("민소 법", "009999")]);
    assert!(matches!(r, Err(Error::Duplicate(_))));
}

#[test]
fn real_file_loads_when_given() -> Result<(), Error> {
    let Ok(path) = std::env::var("LAW_ALIASES") else {
        eprintln!("LAW_ALIASES가 없어 건너뜀");
        return Ok(());
    };
    let a = Aliases::load(std::path::Path::new(&path))?;
    println!("약칭 {}개", a.len());
    assert_eq!(a.get("헌법").map(|e| e.name.as_str()), Some("대한민국헌법"));
    Ok(())
}
