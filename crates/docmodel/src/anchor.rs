// Anchor — 출처 좌표 (PROJECT.md §8)
//
// 쓰임:
// - LeafResult::Judged.anchor: 리프 판정의 근거 좌표 (engine::schema — 판정된 리프는 Anchor가 필수)
// - 답변 조립 시 인용 표기 (server::answer)
// - 신뢰 등급이 낮은 추출(괘선 없는 표의 LLM-vision 추출)은
//   원문 페이지·좌표 인용을 반드시 동반한다 — 사람이 확인할 수 있게.
//
// 규칙: Anchor 없는 값은 판정 근거가 될 수 없다. Option<Anchor>가 None인 채
//       Z3에 들어가는 경로를 타입으로 막는 것이 이 파일의 존재 이유.

use serde::{Deserialize, Serialize};

// upload file_id 
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub String);

//pdf bbox
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f32, 
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub source: FileId,
    pub page: Option<u32>,
    pub bbox: Option<Rect>,
    pub path: Option<String>, // passage, table path
}
