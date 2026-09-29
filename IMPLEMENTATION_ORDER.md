# 구현 순서

작성일: 2026-09-29. `PROJECT.md`의 설계를 어떤 순서로 코드로 옮길지 정리한 문서.
단계 번호는 이 문서 안에서만 쓰는 순번이다.

## 순서를 정한 기준

1. **의존 방향을 따른다.** 아래에 있는 crate부터 짠다.
   `docmodel` ← `parsers`·`checks` / `llm` ← `retrieval`·`engine` / 전부 ← `server` ← `agent`(④).
2. **이미 있는 데이터로 먼저 끝까지 잇는다.** 법령·판례 DB는 수집되어 있으므로 검색 경로가 가장 먼저
   질문부터 답변까지 이어질 수 있다. 요건 트리는 아직 없다(§12 결정 1 대기).
3. **단계가 끝날 때마다 채팅에서 직접 써 볼 수 있는 기능이 하나씩 늘어나게 한다.**
4. **저장 경로에 붙이기 전에 정확도를 잰다** (§8, §12 미논의 2). 파서와 리프 판정기가 해당한다.
   맞는 것을 통과시키는지(오탐)와 틀린 것을 막는지(누락)를 둘 다 표본으로 센다.
5. **단계를 시작할 때 무엇을 재서 무엇이 나오면 성공인지 한 줄로 먼저 적는다.**
   아래 「성공 기준」이 그 자리다. 표본 수는 착수할 때 정해 함께 적는다.

## 한눈에 보기

| 단계 | 끝나면 되는 것 | 먼저 끝나야 하는 것 |
|---|---|---|
| 0 | (코드 없음) 기존 DB 확인 결과와 결정 사항이 정리됨 | — |
| 1 | 문서 모델 타입, Rust 쪽 LLM 호출 | — |
| 2 | 법령·판례 검색 API | 0 (실측 1·2·3) |
| 3 | 채팅에서 법령·판례 질문에 답하고 세션을 기억함 | 2, 결정 2·4 |
| 4 | DOCX·HWPX를 올리고 그 문서에 대해 질문함 | 3, 결정 3 |
| 5 | 요건 판단 — 국가배상 1종 | 4, 결정 1, 평가 표본 |
| 6 | PDF를 올림 | 4 |
| 7 | 나머지 요건 트리 3종, 부분·가정 질문, 수정판 재검증 | 5 |
| 8 | 웹 프론트 | 3 이후 언제든 |

---

## 0단계 — 착수 전 확인 (코드 거의 없음)

기존 DB에 SQL 몇 줄을 던져 확인하는 일이다. 이 결과가 2단계의 설계를 바꾼다.

| 항목 | 할 일 | 기록할 곳 |
|---|---|---|
| 실측 1 | 저장된 임베딩(조문 30만·판례 412만 행)을 만든 모델과 차원을 확인한다 | `infer/README.md` |
| 실측 2 | 한국어 FTS 인덱스 방식을 확인한다 (인덱스 정의 조회) | `crates/retrieval/src/sql.rs` 주석 |
| 실측 3 | 판례에서 행정 사건만 거를 수 있는 칼럼이 있는지 확인한다 | `crates/retrieval/src/sql.rs` 주석 |
| 결정 2 | LLM을 외부 API로 쓸지 로컬로 쓸지 정한다 | `PROJECT.md` §12 |
| 결정 4 | 체크포인트를 둘 PostgreSQL 데이터베이스를 정한다 | `PROJECT.md` §12 |

**성공 기준**
- 실측 1: 확인한 모델로 DB에 저장된 문장 하나를 다시 임베딩했을 때, 저장된 벡터와의 cosine이 1에 가깝다.
  멀면 그 모델이 아니다.
- 실측 2: 그 방식으로 쓴 질의가 실제로 인덱스를 탄다(EXPLAIN으로 확인).

결정 1(입증책임)은 5단계 전에, 결정 3(입력 범위)은 4단계 전에 정하면 된다.

## 1단계 — 바닥 타입과 LLM 호출

| 순서 | 파일 | 내용 |
|---|---|---|
| 1 | `crates/docmodel/src/anchor.rs`, `block.rs`, `clause.rs` | Block·Anchor·Clause 타입 (§8 통합 문서 모델) |
| 2 | `crates/llm/src/client.rs`, `limits.rs` | `complete_json`, 재시도, 동시 호출 상한, `LlmError` |

**성공 기준**: 가짜 응답을 주는 테스트 서버에 대고 `cargo test`를 돌린다.
- JSON 파싱에 실패하면 1회 다시 요청하고, 그래도 실패하면 `ParseFailed`를 돌려준다(기본값을 채우지 않는다).
- 429·5xx 응답에는 정해진 횟수만큼 다시 시도한다.

## 2단계 — 법령·판례 검색 (③)

| 순서 | 파일 | 내용 |
|---|---|---|
| 1 | `infer/run_llamacpp.sh` | 실측 1에서 확인한 임베딩 모델로 ② 추론 서버를 띄운다 |
| 2 | `crates/retrieval/src/embed_client.rs` | ② 호출. 차원이 다르면 곧바로 오류를 낸다 |
| 4 | `crates/retrieval/src/rewriter.rs` | 쿼리 재작성 + moka 캐시 |
| 5 | `crates/retrieval/src/lib.rs` | `search()`, 강등 규칙 (§11) |
| 6 | `crates/server/src/main.rs`, `routes.rs` | 부팅과 AppState, `/search/laws`·`/search/cases`만 |

**성공 기준**
- 행정법 질문 표본에서 기대한 조문·판례가 top-k 안에 드는 비율과 지연을 잰다.
- ②를 끈 채 검색하면 FTS만으로 결과가 나오고 `degraded: true`가 붙는다.

## 3단계 — 채팅 에이전트 최소판 (④)

처음으로 채팅에서 질문하고 답을 받는 단계다.

| 순서 | 파일 | 내용 |
|---|---|---|
| 1 | `agent/semantic_agent/config.py` | 설정 |
| 2 | `agent/semantic_agent/memory.py` | 체크포인트 연결 (결정 4의 데이터베이스) |
| 3 | `agent/semantic_agent/tools.py` | `search_laws`·`search_cases`만 |
| 4 | `agent/semantic_agent/prompts.py` | 규칙 2·3·4·5 (규칙 1은 5단계에서) |
| 5 | `agent/semantic_agent/graph.py` | `create_deep_agent` 조립 |
| 6 | `agent/semantic_agent/sessions.py` | 세션별 락 (업로드 우선 취소는 4단계에서) |
| 7 | `agent/semantic_agent/stream.py` | SSE 이벤트: Status·Token·Warning·Error·Done (VerdictReady는 5단계에서) |
| 8 | `agent/semantic_agent/app.py` | `POST /session`, `ask`(SSE), `history`, `DELETE` |

**성공 기준**
- 같은 세션의 두 번째 질문이 첫 답변과 읽은 조문을 이어받는다.
- 에이전트를 다시 띄운 뒤 `history`로 대화가 그대로 다시 그려진다.
- Rust 서버를 끈 채 질문하면 답변에 「검색 실패」를 밝힌다.
- 답변 문장마다 출처(법령명·조문번호, 사건번호)가 붙는다.

2026-09-29에 가짜 모델로 시험한 흐름(`PROJECT.md` §9.1)을 실제 모델로 다시 확인하는 셈이다.

## 4단계 — DOCX·HWPX 업로드 (⑥·⑦·⑧)

| 순서 | 파일 | 내용 |
|---|---|---|
| 1 | `crates/parsers/src/lib.rs` → `docx.rs` → `hwpx.rs` | 형식 판별, zip + quick-xml 파싱, 표 병합(`gridSpan`·`vMerge`), 제목 수준 |
| 2 | `crates/docmodel/src/block.rs` | `Table::to_markdown` — 표를 글자로 바꾸는 유일한 함수 |
| 3 | `crates/docmodel/src/clause/marker.rs` → `clause.rs` | 조·항·호 표시 판별 → `assign_clauses` (clause_id 부여) |
| 4 | `crates/checks/src/dates.rs`, `sums.rs`, `refs.rs` | 날짜 글자 해석, 합계·참조를 결정론으로 검증 |
| 5 | `crates/retrieval/src/upload_index.rs` | 업로드 문서의 청크를 메모리 안에서 cosine top-k로 찾는다 |
| 6 | `crates/sessdb/src/lib.rs`, `cf.rs`, `batch.rs` | `state`·`leaf_cache` CF, 업로드 한 건을 WriteBatch로 저장 |
| 7 | `crates/server/src/upload.rs`, `session_actor.rs`(Upload만), `routes.rs` | `/files`, `/upload_search`, `/clauses` |
| 8 | `agent/semantic_agent/tools.py`, `sessions.py`, `app.py` | `search_upload`·`read_clauses` 도구, 업로드 우선 취소, 업로드 요약을 대화에 넣기 |

**성공 기준** (저장 경로에 붙이기 전에)
- 실제 문서 표본으로 표 셀 배정 정확도(맞게 배정된 셀 수 / 전체 셀 수)를 잰다.
- `clause/marker.rs`의 표시 판별을 실제 계약서·처분서 문장 표본으로 센다: 표시가 아닌 문장을 표시로 잡는지(오탐),
  표시를 놓치는지(누락).
- `checks`는 두 가지 표본으로 센다.
  - 일부러 합계를 틀리게 만든 표본: 놓치지 않는지(누락)
  - 맞는 표본: 막지 않는지(오탐)
- 채팅에서 「제5조가 무슨 뜻이야?」에 업로드 문서의 clause_id·페이지를 달아 답한다.

## 5단계 — 요건 판단: 국가배상 1종 (⑤·⑩)

국가배상을 먼저 하는 이유가 두 가지 있다.
- 총정리 PDF 33~39쪽이 사실상 완성된 트리다.
- 리프 8개의 AND에 OR 하나뿐이고, 날짜 산술 리프가 없어 가장 단순하다.

**시작 전에 필요한 것**
- 결정 1: 입증책임·직권조사의 의미를 정한다 (§7.5).
- 평가 표본: 리프마다 정답(True/False/Unknown)을 단 사례를 만든다 (§12 미논의 2).

| 순서 | 파일 | 내용 |
|---|---|---|
| 1 | `crates/engine/src/schema.rs` → `graph.rs` | 노드·엣지·LeafResult 타입, `ClaimGraph::from_rows`, DAG 가드(순환 검사, 기대 노드 수) |
| 2 | `crates/engine/src/compile.rs` → `verdict.rs` | Z3 컴파일·판정, sat/unsat/unknown, unsat_core |
| 3 | `migrations/001_claim_graph.sql`, `rules/claim_국가배상.yaml`, `crates/ruleloader/src/main.rs` | 트리 적재 (검증에 실패하면 전체 중단) |
| 4 | `crates/engine/src/deps.rs` → `slices.rs` → `leaf.rs` → `lib.rs` | 바깥 기능 트레이트, 리프별 근거 조립, LLM 리프 판정, `judge()` |
| 5 | `crates/server/src/engine_deps.rs` | 트레이트 구현 (업로드 조항 검색, 법령·판례 검색, moka + sessdb 캐시) |
| 6 | `crates/server/src/answer.rs`, `session_actor.rs`(Judge), `routes.rs` | 판정 문장 조립, `/catalog`, `/judge` |
| 7 | `agent/semantic_agent/tools.py`, `prompts.py`, `stream.py` | `judge` 도구, 프롬프트 규칙 1, `VerdictReady` 이벤트 |

순서 1~3은 LLM 없이 진행한다. 리프 값을 손으로 넣어 Z3 판정만 먼저 검증한다.
순서 4의 트레이트 덕분에 engine은 server 없이, 가짜 구현만 붙여 따로 돌려 볼 수 있다.

**성공 기준**
- 손으로 넣은 리프 조합마다 Z3 결론(성립·불성립·유보)과 unsat_core 노드가 기대값과 모두 맞는다.
- 평가 표본으로 리프 판정기의 오탐·누락을 센다. 이 수치 없이는 judge 결과를 해석할 수 없다.
- 채팅에서 요건 판단 질문을 하면 판정 문장이 원문 그대로 화면에 나오고, Unknown 리프가 빠짐없이 나열된다.

## 6단계 — PDF 업로드

| 순서 | 파일 | 내용 |
|---|---|---|
| 1 | `crates/parsers/src/pdf.rs` | pdfium 전용 스레드, 글자와 좌표 추출 |
| 2 | `crates/parsers/src/pdf_table.rs` | 괘선을 격자로 맞춰 표 재구성 |
| 3 | `crates/server/src/upload.rs` | 괘선 없는 표 → 멀티모달 LLM 추출 (신뢰 등급을 낮춰 표시) |

**성공 기준**
- PDF 표 표본으로 셀 배정 정확도를 잰 뒤 저장 경로에 붙인다.
- 격자 재구성에 실패한 표는 산술 검증과 Z3 입력에서 빠진다.

표본 문서가 대부분 PDF라면 이 단계를 5단계 앞으로 당긴다.

## 7단계 — 나머지 요건 트리와 세부 기능

- `crates/checks/src/periods.rs`: 법정 기간의 마감일 계산 — 초일 불산입, 역에 따른 연·월 계산,
  말일 공휴일 (민법 제157조·제160조·제161조). 취소소송의 제소기간이 이것을 쓴다.
- `rules/claim_취소소송.yaml`: 제소기간은 산술 리프다. 행정심판을 거쳤는지에 따라 기산점이 바뀐다
  (행정소송법 제20조).
- `rules/claim_무효확인.yaml`, `rules/claim_손실보상.yaml`
- 부분 질문과 가정 질문: Z3 모델을 읽어 「성립에 필요한 사실 조합」을 보여 준다 (§6.3).
- 수정판 재검증: 결정론 검증은 전체를 다시 돌리고, 리프는 cache_key가 바뀐 것만 다시 판정한다 (§6.2).
  - 판정 도중 수정판이 오면 Rust에서는 `CancellationToken`으로, 에이전트에서는 진행 중인 질문 실행을 취소한다 (§10).

**성공 기준**
- 트리마다 5단계와 같은 두 가지(손으로 넣은 조합의 결론 일치, 평가 표본의 오탐·누락)를 잰다.
- `periods.rs`는 윤년이 낀 1년, 말일이 토요일·공휴일인 경우, 그달에 기산일에 해당하는 날이 없는 경우
  (예: 1월 31일부터 1개월)를 넣어 마감일이 맞는지 센다.

## 8단계 — 웹 프론트 (§12 미논의 1)

④의 `POST /session`, `ask`(SSE), `history`, `upload`만 쓰므로 3단계가 끝난 뒤 언제든 붙일 수 있다.

---

## 파일 순서 — 한 파일씩

위 단계를 파일 하나 단위로 펼친 순서. 같은 파일을 여러 번 여는 경우는 「다시」로 표시하고
그때 더할 내용을 적었다. 같은 crate 안에서는 타입을 정의하는 파일이 그 타입을 쓰는 파일보다 앞에 온다.

**1단계 — 바닥 타입과 LLM 호출**
1. `crates/docmodel/src/anchor.rs`: FileId, Rect, Anchor (완료)
2. `crates/docmodel/src/block.rs`: Block, Table, Cell, Document, ExtractWarning (Anchor를 씀)   (완료)
3. `crates/docmodel/src/clause.rs`: ClauseId, Clause 타입만 (완료)
4. `crates/docmodel/src/lib.rs`: 모듈 공개
5. `crates/llm/src/lib.rs`: LlmError
6. `crates/llm/src/limits.rs`: LlmLimiter (동시 호출 상한)
7. `crates/llm/src/client.rs`: `complete_json`

**2단계 — 법령·판례 검색**
8. `infer/run_llamacpp.sh`: 임베딩 서버 기동 (확인한 모델은 `infer/README.md`에 기록)
9. `crates/retrieval/src/lib.rs`: SearchHit, RetrievalError 타입 먼저
10. `crates/retrieval/src/embed_client.rs`
11. `crates/retrieval/src/sql.rs`: FTS + pgvector RRF
12. `crates/retrieval/src/rewriter.rs`: 쿼리 재작성 + 캐시
13. `crates/retrieval/src/lib.rs` (다시): `search()`, 강등 규칙
14. `crates/server/src/main.rs`: 부팅, AppState
15. `crates/server/src/routes.rs`: `/search/laws`, `/search/cases`

**3단계 — 채팅 에이전트 최소판**
16. `agent/semantic_agent/config.py`
17. `agent/semantic_agent/memory.py`
18. `agent/semantic_agent/tools.py`: `search_laws`, `search_cases`
19. `agent/semantic_agent/prompts.py`: 규칙 2~5
20. `agent/semantic_agent/graph.py`
21. `agent/semantic_agent/sessions.py`: 세션별 락 (업로드 우선 취소는 43번에서)
22. `agent/semantic_agent/stream.py`: Status, Token, Warning, Error, Done
23. `agent/semantic_agent/app.py`: session, ask, history, delete

**4단계 — DOCX·HWPX 업로드**
24. `crates/parsers/src/lib.rs`: Format, `detect`, ParseError
25. `crates/parsers/src/docx.rs`
26. `crates/parsers/src/hwpx.rs`
27. `crates/docmodel/src/block.rs` (다시): `Table::to_markdown`
28. `crates/docmodel/src/clause/marker.rs`: Marker, 정규식 4개, `parse_marker`
29. `crates/docmodel/src/clause.rs` (다시): `assign_clauses`, Builder (28번 `parse_marker`와 27번 `to_markdown`을 씀)
30. `crates/checks/src/lib.rs`: Finding 타입
31. `crates/checks/src/dates.rs`: 날짜 글자 해석, ordinal
32. `crates/checks/src/sums.rs`
33. `crates/checks/src/refs.rs`
34. `crates/checks/src/lib.rs` (다시): `run_all`
35. `crates/retrieval/src/upload_index.rs`
36. `crates/sessdb/src/lib.rs`: `open`, CF 2개
37. `crates/sessdb/src/cf.rs`
38. `crates/sessdb/src/batch.rs`
39. `crates/server/src/upload.rs`
40. `crates/server/src/session_actor.rs`: Upload
41. `crates/server/src/routes.rs` (다시): `/files`, `/upload_search`, `/clauses`
42. `agent/semantic_agent/tools.py` (다시): `search_upload`, `read_clauses`
43. `agent/semantic_agent/sessions.py` (다시): 업로드가 오면 진행 중인 질문 취소
44. `agent/semantic_agent/app.py` (다시): upload

**5단계 — 요건 판단 (국가배상)**
45. `crates/engine/src/schema.rs`: NodeId, GateType, Trit, LeafResult(Judged·Unknown), FailureReason, ArithSpec
46. `crates/engine/src/graph.rs`: `ClaimGraph::from_rows`, DAG 가드, 서브트리
47. `crates/engine/src/compile.rs`: Z3 컴파일·판정, RawVerdict
48. `crates/engine/src/verdict.rs`: Verdict (compile 결과로 만듦)
49. `migrations/001_claim_graph.sql`
50. `rules/claim_국가배상.yaml`
51. `crates/ruleloader/src/main.rs`: YAML → engine 타입 → `from_rows`로 검증 → DB 적재
52. `crates/engine/src/deps.rs`: FactSearch, AuthoritySearch, LeafCache 트레이트
53. `crates/engine/src/slices.rs`: 리프별 근거 조립
54. `crates/engine/src/leaf.rs`: LLM 리프 판정, 모순 검출
55. `crates/engine/src/lib.rs`: `judge()` (graph → slices → leaf → compile → verdict를 잇는 진입점)
56. `crates/server/src/engine_deps.rs`: 52번 트레이트의 구현
57. `crates/server/src/answer.rs`: 판정 문장 조립
58. `crates/server/src/session_actor.rs` (다시): Judge
59. `crates/server/src/routes.rs` (다시): `/catalog`, `/judge`
60. `agent/semantic_agent/tools.py` (다시): `judge`
61. `agent/semantic_agent/prompts.py` (다시): 규칙 1
62. `agent/semantic_agent/stream.py` (다시): VerdictReady 이벤트

45~51번은 LLM 없이 진행한다. 리프 값을 손으로 넣어 Z3 판정부터 검증한 뒤 52번으로 넘어간다.

**6단계 — PDF**
63. `crates/parsers/src/pdf.rs`: pdfium 전용 스레드, 글자·좌표·괘선 추출
64. `crates/parsers/src/pdf_table.rs`: Segment, TableError, 괘선을 격자로 맞춰 표 재구성
65. `crates/server/src/upload.rs` (다시): 괘선 없는 표 → 멀티모달 LLM 우회

**7단계 — 나머지 요건 트리**
66. `crates/checks/src/periods.rs`: 법정 기간 마감일, 공휴일
67. `rules/claim_취소소송.yaml`: 제소기간 (66번 `periods.rs`를 씀, 행정심판 경유 여부는 긍정·부정 쌍 리프)
68. `rules/claim_무효확인.yaml`
69. `rules/claim_손실보상.yaml`

`crates/server/src/main.rs`는 단계마다 새로 붙는 의존성을 AppState에 더하러 다시 연다.
4단계에서 sessdb를, 5단계에서 engine 조립(`engine_deps.rs`의 구현, 요건 DAG를 읽어 `from_rows`)과
노드 카탈로그를, 6단계에서 pdfium 스레드를 더한다.
