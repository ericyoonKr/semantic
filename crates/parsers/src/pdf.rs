// PDF 파서 — pdfium-render (PROJECT.md §8, §10)
//
// 로직:
// 1) 문서 열기 (암호화면 ParseError::Encrypted).
// 2) 페이지별로:
//    - 글자 + bbox 추출 → 줄/문단으로 클러스터링 → Para(anchor: page+bbox)
//    - 페이지의 path 객체에서 선분(수평/수직) 수집 → pdf_table::reconstruct로 전달
// 3) 텍스트가 전혀 없는 페이지가 전부면 ParseError::EmptyText(스캔 추정)
//    일부면 Document.warnings에 "p.N~M 추출 불가" 기록.
//
// ★ 스레드 안전성 (§10): pdfium은 스레드 세이프하지 않다.
//   이 모듈은 동기 함수만 제공하고, server 쪽에서 pdfium 전용 스레드 1개 +
//   mpsc 채널로 모든 PDF 작업을 직렬화한다. 여기서 전역 상태를 만들지 않는다.
//   (업로드 시점 1회 작업이라 직렬화해도 체감 지연 없음)
//
// 함수: pub(crate) fn parse_pdf(bytes: &[u8]) -> Result<Document, ParseError>
//       (내부에서 pdf_table 호출, 실패한 표는 degraded=true로 평문 강등)
