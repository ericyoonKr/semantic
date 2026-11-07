use checks::periods::{Start, StartKnown};
use chrono::NaiveDate;
use engine::Trit;
use engine::arith::extract::{DateAnswer, date_in_quote, start_from_quote};

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
}

fn genuine() -> Vec<(&'static str, NaiveDate)> {
    vec![
        ("원고는 2026. 3. 2. 처분서를 받았다.", d(2026, 3, 2)),
        ("처분일: 2026.3.2", d(2026, 3, 2)),
        ("2026년 3월 2일 통지서를 수령함", d(2026, 3, 2)),
        ("송달일 2026-03-02", d(2026, 3, 2)),
        ("2026 . 03 . 02 . 접수", d(2026, 3, 2)),
        ("소장 접수일은 2026/06/01이다.", d(2026, 6, 1)),
        ("재결서는 2025. 12. 31. 송달되었다.", d(2025, 12, 31)),
        ("2024년 2월 29일 처분이 있었다", d(2024, 2, 29)),
        ("처분(2026. 3. 2.)과 통지(2026. 3. 5.)", d(2026, 3, 5)),
        ("갑은2026.3.2.받았다", d(2026, 3, 2)),
        ("2027년 1월 4일에 소를 냈습니다", d(2027, 1, 4)),
        ("처분 2026. 3. 2.자", d(2026, 3, 2)),
    ]
}

fn wrong() -> Vec<(&'static str, NaiveDate)> {
    vec![
        ("원고는 2026. 3. 2. 처분서를 받았다.", d(2026, 3, 3)),
        ("원고는 3월 2일 처분서를 받았다.", d(2026, 3, 2)),
        ("원고는 26. 3. 2. 처분서를 받았다.", d(2026, 3, 2)),
        ("원고는 어제 처분서를 받았다.", d(2026, 3, 2)),
        ("원고는 2026. 3. 2. 처분서를 받았다.", d(2025, 3, 2)),
        ("2026년 3월 처분서를 받았다", d(2026, 3, 1)),
        ("처분서는 12026. 3. 2. 받았다", d(2026, 3, 2)),
        ("처분서는 2026. 3. 21. 받았다", d(2026, 3, 2)),
        ("처분서는 2026. 2. 30. 받았다", d(2026, 3, 2)),
        ("처분서는 송달되었다", d(2026, 3, 2)),
        ("소는 2026. 6. 1.에서 사흘 뒤 냈다", d(2026, 6, 4)),
        ("2026. 3. 2.부터 90일", d(2026, 5, 31)),
    ]
}

#[test]
fn date_in_quote_check_passes_genuine_and_blocks_wrong_samples() {
    let passed = genuine().iter().filter(|(q, date)| date_in_quote(*date, q)).count();
    let blocked = wrong().iter().filter(|(q, date)| !date_in_quote(*date, q)).count();
    println!("date_in_quote: genuine {passed}/12 passed, wrong {blocked}/12 blocked");
    assert_eq!((passed, blocked), (12, 12));
}

#[test]
fn start_time_check_needs_a_stated_time_that_fits_the_answer() {
    let from = d(2026, 3, 2);
    let good: [(Trit, &str, StartKnown); 8] = [
        (Trit::True, "2026. 3. 2. 오전 0시부터 효력이 생긴다", StartKnown::Known(Start::AtMidnight)),
        (Trit::True, "처분은 2026. 3. 2. 영시부터 시행한다", StartKnown::Known(Start::AtMidnight)),
        (Trit::True, "2026-03-02 00:00 발효", StartKnown::Known(Start::AtMidnight)),
        (Trit::False, "2026. 3. 2. 오후 2시 처분서를 받았다", StartKnown::Known(Start::AfterEvent)),
        (Trit::False, "2026. 3. 2. 10시 30분 송달", StartKnown::Known(Start::AfterEvent)),
        (Trit::False, "14:20에 통지서를 받았다", StartKnown::Known(Start::AfterEvent)),
        (Trit::False, "2026. 3. 2. 오전 9시 통지", StartKnown::Known(Start::AfterEvent)),
        (Trit::True, "0시부터 시작한다", StartKnown::Known(Start::AtMidnight)),
    ];
    let bad: [(Trit, &str); 8] = [
        (Trit::True, "2026. 3. 2. 처분서를 받았다"),
        (Trit::True, "2026. 3. 2. 오후 2시 처분서를 받았다"),
        (Trit::False, "2026. 3. 2. 오전 0시부터 효력이 생긴다"),
        (Trit::False, "2026. 3. 2. 처분서를 받았다"),
        (Trit::True, "2026. 3. 5. 오전 0시부터 효력이 생긴다"),
        (Trit::False, "3시간 뒤에 받았다"),
        (Trit::Unknown, "2026. 3. 2. 오후 2시 처분서를 받았다"),
        (Trit::True, "0시와 15시에 두 번 통지했다"),
    ];
    let passed = good.iter().filter(|(v, q, want)| start_from_quote(*v, q, from) == *want).count();
    let blocked = bad.iter().filter(|(v, q)| start_from_quote(*v, q, from) == StartKnown::Undetermined).count();
    println!("start_from_quote: genuine {passed}/8 passed, wrong {blocked}/8 blocked");
    assert_eq!((passed, blocked), (8, 8));
}

#[test]
fn answer_json_is_read_and_names_match_without_underscores() -> Result<(), serde_json::Error> {
    let a: DateAnswer = serde_json::from_str(
        r#"{"dates": [{"name": "처분이 있음을 안 날", "date": "2026-03-02", "quote": "q", "source": "doc"},
                      {"name": "제소일", "date": ""}]}"#,
    )?;
    assert_eq!(a.item("처분이_있음을_안_날").map(|i| i.date.as_str()), Some("2026-03-02"));
    assert_eq!(a.item("제소일").map(|i| i.quote.as_str()), Some(""));
    assert!(a.item("재결서_정본_송달일").is_none());
    assert!(a.start.is_none());
    let bad: Result<DateAnswer, _> = serde_json::from_str(r#"{"dates": "2026-03-02"}"#);
    assert!(bad.is_err());
    Ok(())
}
