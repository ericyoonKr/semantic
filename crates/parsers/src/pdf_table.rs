// PDF 표 격자 재구성 — 신규 알고리즘 덩어리 1/2 (PROJECT.md §8)
// PDF에는 표 구조가 없다. 선 몇 개와 글자 좌표뿐 → 기하적으로 복원한다.
// Camelot의 lattice 방식과 동일한 알고리즘을 Rust로.
//
// 입력: 페이지의 선분들(Vec<Segment>), 글자들(Vec<(char/word, Rect)>)
// 출력: Result<docmodel::Table, TableError>
//
// 이 파일에서 정의할 타입:
// - struct Segment { x0: f32, y0: f32, x1: f32, y1: f32 }   // pdf.rs가 path 객체에서 뽑아 넘김
// - enum TableError { NoGrid, TooManyUnassigned { ratio: f32 }, Ambiguous(설명) }
//     실패 사유는 pdf.rs가 ExtractWarning::TableDegraded로 사용자에게 알린다.
//
// 로직 (괘선 있는 표 — 법률문서 표의 대부분):
// 1) 선분을 수평/수직으로 분류, 기울기 허용오차 내 스냅.
// 2) 가까운 좌표(허용오차 ε px)를 병합해 격자선 후보 목록 생성.
// 3) 수평선×수직선 교차점으로 격자 구성 → 셀 사각형 목록.
// 4) 이웃 셀 사이에 경계선이 없으면 병합 셀로 판정 (rowspan/colspan 계산).
// 5) 각 글자/단어를 bbox 중심점으로 셀에 배정, 셀 내 y→x 순 정렬로 텍스트 조립.
// 6) 검증: 배정 안 된 글자 비율이 임계 초과면 실패로 처리.
//
// 실패 시 (§11): Table을 만들지 않는다. 호출자(pdf.rs)가 평문 강등 +
//   degraded=true 처리 → 산술 검증·Z3 입력에서 제외.
//   셀 배정이 틀린 숫자가 Z3에 들어가는 것이 최악이므로, 애매하면 실패가 정답.
//
// 괘선 없는 표: 여기서 다루지 않는다. server::upload에서 페이지 렌더 →
//   멀티모달 LLM 추출(신뢰 등급 하락 표시 + 페이지·좌표 인용)로 우회.
//
// 시험: 다운로드 폴더의 실제 서증·계약서 PDF 표본으로 셀 배정 정확도를
//   실측한 뒤에야 저장 경로에 붙인다 (PROJECT.md §8 말미).
