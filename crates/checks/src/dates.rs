// 날짜 글자 해석 — 문서의 날짜 표기 → NaiveDate → ordinal (PROJECT.md §7.2)
// 기간 계산(마감일)은 periods.rs가 한다. 여기는 「글자를 날짜로」까지만.
//
// 담을 것:
// - fn to_ordinal(d: NaiveDate) -> i64            // Z3 Int로 넣을 값
// - fn parse_korean_date(s: &str) -> Option<NaiveDate>
//     ("2024. 3. 22.", "24. 3. 22."(2자리 연도 후보 해석), "2024년 3월 22일" 등)
//
// 규칙: Z3에 달력을 구현하지 않는다. LLM에 날짜 계산을 시키지 않는다.
//       LLM의 역할은 "처분을 안 날이 문서 어디에 어떤 값으로 적혀 있는가" 추출까지.
