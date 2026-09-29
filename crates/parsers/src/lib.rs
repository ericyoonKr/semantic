// parsers — ⑥ 문서 파싱층 (PROJECT.md §8)
//
// 형식별 난이도: DOCX·HWPX 쉬움(표 공짜) → PDF 어려움 → PDF 표가 최난.
//
// 담을 것:
// - enum Format { Docx, Hwpx, Pdf }  + fn detect(bytes) -> Option<Format>
//   (매직 바이트: zip은 PK, PDF는 %PDF. zip이면 내부 경로로 docx/hwpx 구분)
// - fn parse(bytes: &[u8]) -> Result<docmodel::Document, ParseError>
// - enum ParseError { Unsupported(설명), Corrupted, Encrypted, EmptyText(스캔 추정), ... }
//   → server::upload에서 「추출 불가 · 판정 불가」로 사용자에게 명시 (§11)
//
// 규칙:
// - 범위 제외 (§12 결정 3 확정 전까지의 기본값):
//   HWP 구형(CFB 바이너리) → Unsupported(변환 안내 문구)
//   스캔 PDF(글자가 전혀 없는 PDF) → EmptyText(스캔 추정) — pdf.rs가 판별
// - 부분 성공 허용: 일부 페이지만 추출되면 Document.warnings에 성공 범위 기록.
// - 이 crate는 전부 동기 함수. 호출자가 spawn_blocking으로 감싼다 (§10).

pub mod docx;
pub mod hwpx;
pub mod pdf;
pub mod pdf_table;
