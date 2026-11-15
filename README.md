# Legal AI Agent using Semantic RAG

의미론적 RAG를 이용한 법률 인공지능 에이전트 — 2026 캡스톤 페스티벌 및 졸업작품전시회 출품작
팀 「특이점이 온다」 · 단국대학교 소프트웨어학과 32227546 윤치호

**법령·판례·조례·학칙을 실제로 읽은 것만 인용해 답하는 법률 챗봇**이다. 큰 언어 모델이 그럴듯한 법령명·사건번호를 지어내는 문제를, 모델을 믿지 않는 구조로 막는다. 모델은 근거를 찾아 읽는 일과 문장을 쓰는 일만 하고, 어떤 근거를 인용할 수 있는지는 코드가 정한다.

제출물: [결과보고서(PDF)](docs/제출/결과보고서.pdf) · [포스터(PDF)](docs/제출/포스터.pdf) · [포스터(PNG)](docs/제출/포스터.png) · [시연 시나리오](docs/제출/시연_시나리오.md) · 실행 파일은 이 저장소의 Release(리눅스 x86-64 `server`·`gateway`) · 시연 영상 링크와 심사용 접속 주소는 제출 메일에 적는다.

## 무엇이 의미론적이고 새로운가

1. **의미·글자 하이브리드 검색** — tantivy 글자 색인(2글자 토큰·제목 가중·시행일 범위)과 bge-m3 벡터(pgvector halfvec·HNSW)를 순위 역수 합(RRF)으로 합치고, 조문을 고른 뒤 그 안에서 항·호·목을 다시 골라 「조문 전체」가 아니라 「맞는 단위」를 돌려준다. 질문 날짜에 효력 있던 판본만 쓴다.
2. **grep RAG — 에이전트 주도 조사** — 조사 하위 에이전트가 글자 검색과 뜻 검색을 번갈아 부르며 검색→읽기→다시 검색을 반복한다. 읽은 조문·판례만 근거 번호를 받는다.
3. **여러 층의 임베딩** — 조문 판본·항호목 단위·별표 표·행 노드·긴 잎 조각·판례 판단 조각을 따로 임베딩한다.
4. **뉴로심볼릭 요건 판단** — 요건 나무의 잎(원자 명제) 하나씩만 모델이 판정하고, 조합·기간 계산·상태 전이는 Z3와 Rust가 한다. 인용 원문이 문서에 없으면 그 잎은 「알 수 없음」이다.
5. **지식그래프 소비** — 개념 연결망(관계 20종, 순환 허용 다중 그래프)과 요건 나무(AND·OR 비순환 DAG)를 읽기 전용 SQLite로 두고 찾기·펼치기·상태 확인·나무 판정 도구로 쓴다.

그 위에 **문장 단위 인용 관문**(Rust가 문장마다 인용을 읽은 근거와 대조, 안 맞으면 문장째 뺌), **규범 종류 명세**(법령·행정규칙·자치법규·기관 규정을 한 모듈에), **함수가 정하는 다음 단계**(도구 선택은 모델이 아니라 서버 함수가), **결정론 문서 모델**(PDF·DOCX·HWPX → 쪽 단위 인용)이 얹힌다. 생성·임베딩 모델은 공개 가중치(Gemma 4 26B-A4B, bge-m3)라 외부 API 없이 온프레미스로 돌아간다. 자세한 설명과 잰 숫자는 결과보고서에 있다.

## 구조

```
[웹 화면 React] ──HTTPS──▶ ⓪ 창구 gateway (Rust) ──HTTP──▶ ④ 채팅 에이전트 (Python, Deep Agents/LangGraph)
                                                                    │ HTTP(내부 키)
                                               ① 내부 도구 API server (Rust): 검색·읽기·인용 관문·규범 명세·다음 단계 함수·지식그래프·요건 엔진
                        [PostgreSQL + pgvector] [tantivy 색인] [지식그래프 SQLite] [RocksDB] [vLLM: Gemma 4 · bge-m3]
```

밖에 열리는 것은 창구 하나다. 판단 논리(검색 결합, 인용 관문, 규범 명세, 요건 엔진, 지식그래프 탐색)는 모두 Rust에 있고, Python은 에이전트 루프 조립과 도구 연결만 한다.

## 저장소 구성

| 경로 | 내용 |
|---|---|
| `crates/gateway` | 창구. 로그인·세션·CSRF, 대화 소유자 확인, 속도 제한, 대답 대기열, SSE 걸러 보내기 |
| `crates/server` | 내부 도구 API. 검색·읽기 경로, 다음 단계 함수, 되돌림 검토, 근거 맞음 판정, 인용문 대조, 지식그래프 도구, 요건 판단 경로, 올린 문서 색인 |
| `crates/retrieval` | 법령·판례 검색(tantivy 2글자 토크나이저, 벡터 후보, RRF, 주관 법령 범위, 직접 인용 해석, 판본·시행일) |
| `crates/citegate` | 문장 단위 인용 관문(인용 뽑기, 읽은 근거와 대조, 맨이름 검사, 근거 없는 주장 검출, 인용문 대조) |
| `crates/norms` | 규범 종류 명세(열거형·트레잇·공장 함수) |
| `crates/engine` | 요건 판단 엔진(잎 판정 호출, Z3 조합, 기간 계산, 상태 전이, 판정 문장 조립) |
| `crates/kg`, `crates/ruleloader` | 지식그래프 읽기(SQLite)·카탈로그·탐색, 요건 나무 검증·변환 |
| `crates/parsers`, `crates/docmodel`, `crates/webtext` | PDF(pdfium·표 격자·글자 인식)·DOCX·HWPX → 통합 문서 모델, 화면용 글 |
| `crates/lawtime`, `crates/lawnames`, `crates/llm`, `crates/checks`, `crates/sessdb`, `crates/searcheval` | 법령 날짜 계산, 법령 이름, 모델 호출, 검사 공통, 세션 저장, 검색 평가 |
| `agent/` | Python 에이전트(Deep Agents 조립, 도구 연결, 미들웨어: 첫 단계 강제·예산·되돌림·요약·도구 줄이기·문맥 넘침)와 시험 |
| `web/` | React 화면(대화, 근거 칩, 원문 보기, 올린 문서 쪽 보기, 문맥 띠) |
| `docs/` | 설계서, 구현계획서, 구현지시서, 재설계안, 인수인계 문서(날짜별), `docs/제출/` 제출물 |
| `infer/` | 모델 서버(vLLM) 기동 노트 |

저장소에는 법령·판례 데이터, 지식그래프 묶음, 접속 주소·키, 수집·적재 프로그램(생산 계층)을 두지 않는다. 데이터는 `$DATA_DIR`·`$KG_DIR`·PostgreSQL에 있고, 모델은 vLLM이 OpenAI 호환 API로 제공한다.

## 실행

### 필요한 것

- Rust(stable, edition 2024), Python 3.12와 `uv`, Node 20
- PostgreSQL 16 이상 + pgvector(코퍼스 DB, 체크포인트 DB, 창구 DB)
- OpenAI 호환 생성 모델 서버와 임베딩 서버(이 작품은 vLLM으로 `google/gemma-4-26b-a4b-it`(서빙 이름 `gemma4`)와 `BAAI/bge-m3`를 띄웠다. 기동 인자는 `infer/README.md`)
- pdfium 공유 라이브러리(`PDFIUM_DIR`), Z3(크레이트가 함께 빌드)
- 코퍼스 DB·글자 색인·지식그래프 묶음은 생산 계층이 만든다(이 저장소 범위 밖)

### 빌드

```
cargo build --release -p server -p gateway
cd agent && uv sync
cd web && npm ci && npm run build        # gateway가 web/dist를 서빙한다
```

### 환경 변수(이름만 적는다. 값은 저장소에 두지 않는다)

| 프로그램 | 변수 |
|---|---|
| server | `BIND_ADDR`, `DATA_DIR`, `INTERNAL_KEY`, `CORPUS_PG_URL`, `CORPUS_KEYWORD_INDEX`, `CORPUS_CASE_KEYWORD_INDEX`, `CORPUS_VECTOR_SEARCH`, `CORPUS_LAW_WEIGHTS`, `CORPUS_LAW_WEIGHTS_ANNEX`, `CORPUS_POOL_SIZE`, `LLM_BASE_URL`, `LLM_MODEL`, `LLM_API_KEY`, `EMBED_BASE_URL`, `EMBED_MODEL`, `EMBED_API_KEY`, `KG_DIR`, `KG_CONSUMER_SCHEMA`, `KG_MANIFEST_SCHEMA`, `KG_SOURCE_SCHEMA`, `LAWTIME_PRINCIPLES`, `LAW_ALIASES`, `HOLIDAYS_FILE`, `PDFIUM_DIR`, `OCR_DIR`, `OCR_THREADS`, `LEAF_FACT_BUDGET_TOKENS`, `LEAF_CANDIDATE_K`, `LEAF_AUTHORITY_K`, `SOLVER_TIMEOUT_MS`, `ROUTE_LEGAL`, `RESEARCH_JUDGE`, `RUST_LOG` |
| agent | `INTERNAL_KEY`, `RUST_API_URL`, `LLM_BASE_URL`, `LLM_MODEL`, `LLM_API_KEY`, `CHECKPOINT_PG_URL`, `DATA_DIR` |
| gateway | `GATEWAY_BIND`, `APP_DB_URL`, `AUTH_SCHEMA`, `AUDIT_SCHEMA`, `AGENT_URL`, `INTERNAL_KEY`, `WEB_DIR`, `GATEWAY_HIDDEN_SPANS` |

### 띄우는 순서

1. `server` (내부 도구 API, 기본 127.0.0.1:8080)
2. `cd agent && uv run python -m semantic_agent.api.app` (127.0.0.1:8000)
3. `gateway` (밖에 여는 창구, 예 0.0.0.0:8090) — 계정은 `gateway user add <아이디>`로 만든다(비밀번호는 입력으로 받는다)
4. 브라우저로 창구에 접속해 로그인한다.

### 시험

```
cargo test                      # Rust 단위 시험
cd agent && uv run pytest       # 에이전트 시험(가짜 모델·가짜 도구)
cd web && npm test              # 화면 시험
```

## 문서

- `docs/설계서.md` — 설계 원칙, 구조, 검색·지식그래프·요건 엔진 설계
- `docs/구현계획서.md`, `docs/구현지시서.md`, `docs/재설계안_검색_그래프소비.md`
- `docs/인수인계/` — 날짜별 작업 기록(바꾼 것, 잰 숫자, 남은 것)
- `docs/제출/` — 결과보고서·포스터·시연 시나리오

## 사용 제한

이 저장소는 졸업작품 평가 목적으로 공개한다. 소스와 부속 자료에 대한 이용 허락은 부여하지 않는다(`LICENSE`). 법률 상담을 대신하지 않으며, 평가 목적 외 사용에 따른 책임은 사용자에게 있다.
