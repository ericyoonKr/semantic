// DOCX 파서 — zip + quick-xml 직접 파싱 (PROJECT.md §8)
// 기존 crate(docx-rs 등)는 쓰기 위주라 읽기가 부실 → 직접 스트리밍 파싱.
//
// 로직:
// 1) zip에서 word/document.xml 추출.
// 2) quick-xml 스트리밍으로 순회:
//    - <w:p>  → Block::Para. 개요 수준이 있으면 Block::Heading { level }:
//      문단의 <w:pPr><w:outlineLvl w:val="N"/> 또는 그 문단 스타일(styles.xml)의 outlineLvl을 읽어
//      level = N + 1 (N은 0~8, 9는 본문). 스타일 이름("Heading 1", "제목 1")은 언어판마다
//      달라서 기준으로 쓰지 않는다.
//    - <w:tbl> → Table: <w:tr>/<w:tc> 순회로 Cell{row, col} 채움
// 3) 병합 셀:
//    - <w:gridSpan w:val="N"> → colspan = N
//    - <w:vMerge w:val="restart"> → rowspan 시작, 이후 <w:vMerge/>(continue)를
//      만나는 동안 rowspan += 1, continue 셀은 생성하지 않음.
// 4) Anchor.path에 "body/p[i]" / "tbl[i]/tr[r]/tc[c]" 기록.
//
// 신뢰 등급: 최상 (구조가 XML에 명시). degraded = false 고정.
//
// 함수: pub(crate) fn parse_docx(bytes: &[u8]) -> Result<Document, ParseError>
