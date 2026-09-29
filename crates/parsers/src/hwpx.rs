// HWPX 파서 — zip + XML (KS X 6101), docx.rs와 같은 요령 (PROJECT.md §8)
//
// 로직:
// 1) zip에서 Contents/section*.xml 들을 순서대로 추출.
// 2) quick-xml 스트리밍:
//    - <hp:p> → Para. 그 문단 모양이 개요 문단이면 Heading { level } (개요 수준 + 1).
//      개요 여부·수준을 담은 속성 이름은 실제 HWPX 파일(header.xml의 문단 모양)로 확인하고 적는다.
//      Heading을 못 가려도 조항 묶기(docmodel::clause)는 Para의 「제N조」 표시로 동작한다.
//    - <hp:tbl> → Table (<hp:tr>/<hp:tc>, 병합은 cellSpan 속성 — rowSpan/colSpan)
// 3) Anchor.path에 "section[i]/p[j]" 형태 기록.
//
// 주의: HWP 구형 바이너리(.hwp)는 여기서 다루지 않는다 — lib.rs의 detect가
//       CFB 매직(D0 CF 11 E0)을 보면 ParseError::Unsupported(변환 안내)로 반환.
//
// 함수: pub(crate) fn parse_hwpx(bytes: &[u8]) -> Result<Document, ParseError>
