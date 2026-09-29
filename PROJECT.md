# PROJECT.md — 행정법 Legal AI Semantic RAG (졸업작품)

작성일: 2026-09-27. 착수 전 설계 종합.

## 1. 프로젝트 정의

행정법+헌법 중심의 legal AI semantic RAG. 사용자가 챗에서 문서를 업로드하면
**그 문서를 사실(fact)의 유일한 출처로, 기수집된 법령·판례 DB를 근거(authority)의
출처로** 삼아,

- 일반 질문은 RAG로,
- 요건 판단형 질문은 Leaf 구조 + Z3 뉴로심볼릭 엔진으로

답한다. 채팅 세션은 LangGraph 체크포인트(PostgreSQL)에 저장해, 세션 안에서 답한 것과
에이전트가 읽은 것을 모두 기억한다 (§9.1).

- 채팅 에이전트(④)는 **Python** — Deep Agents(LangGraph·LangChain). 모티브 제품
- 검색·판정·파싱과 핵심 로직은 **Rust**. C++는 라이브러리(libz3, pdfium, llama.cpp/onnxruntime)를
  링크하는 형태로 들어간다. 순수 C++로 새로 짜는 모듈은 없다.

## 2. 설계를 관통하는 두 원칙

1. **LLM은 원자 명제 판정기다.** 문서 구조·산술·논리·규칙 조합·상태전이는
   Rust 코드와 Z3가 통제한다. LLM에게 전체 결론("이 처분은 위법한가?")을 묻지 않는다.
   채팅 에이전트도 요건 판단은 judge 도구(Rust 엔진)에 맡기고, 돌려받은 판정 문장을 고치지 않는다.
2. **모든 실패는 Unknown/「판정 불가」 방향으로만 무너진다.** 오류가 확정 판정으로
   승격되는 경로는 코드에 존재하지 않는다. 판정 도구에서 최악의 실패는 500 에러가
   아니라, 오류를 삼키고 "성립"이라고 말하는 것이다.

## 3. 데이터 자산

### 3.1 이미 수집된 것 (로컬 docker, pgvector — 그대로 사용)

| 컨테이너 (포트) | DB | 실측 내용 (2026-09-26) |
|---|---|---|

### 3.2 새로 만들 데이터 — 행정법 요건 트리 (코드가 아니라 데이터 작업)

취소소송 / 무효확인 / 국가배상 / 손실보상 4종. YAML로 작성 → 로더가 검증 →
`claim_nodes`/`claim_edges` 적재. 상세는 §7.3~7.5.

## 4. 전체 아키텍처

```
[웹 프론트 — 미논의]
        │ HTTP / SSE
┌─ ④ 채팅 에이전트 (Python, FastAPI + Deep Agents/LangGraph) ─────┐
│ ⑨ 질문 라우팅 = 도구 선택: judge(요건 판단) ↔ 검색 도구(일반 질문)  │
│ 서술 답변 · judge 판정 문장은 원문 그대로 화면에 따로 전달          │
│ 세션 기억: LangGraph 체크포인트 (thread_id = 세션 ID)             │──SQL──▶ [PostgreSQL — 체크포인트]
└──────┬───────────────────────────────────────────────────────┘
       │ HTTP (127.0.0.1, 도구 호출)
┌─ ① 내부 도구 API (Rust, axum) ──────────────────────────────┐
│ ⑥ 파싱층: DOCX·HWPX(zip+quick-xml) / PDF(pdfium)+표 격자 재구성 │
│    → 통합 문서 모델(Block+Anchor) → clause_id → 결정론 검증     │
│ ⑦ 업로드 RAG: 청크=구조화 산출물, 메모리 내 cosine top-k         │
│ ③ 검색 코어: 쿼리 재작성(LLM+moka 캐시) → FTS+벡터 RRF(SQL 이식) │
│ ⑤ 법률 엔진: DAG 로더(petgraph) → 리프 오케스트레이터(LLM 병렬)   │
│    → Z3(z3 crate): sat/unsat/unknown + unsat_core             │
│ ⑩ 판정 문장 조립: 결론 템플릿 + anchor 인용 + Unknown 목록        │
└──────┬──────────────────┬──────────────────┬─────────────────┘
       │ SQL(sqlx 풀)      │ HTTP             │ 프로세스 내
[PostgreSQL+pgvector]  [② 추론 서버(C++)]    [⑧ RocksDB]
 법령·조문·판례·임베딩    llama.cpp/onnxruntime   CF: state/leaf_cache
 요건 DAG(claim_*)      임베딩(리랭커 후순위)
       ▲
[외부 LLM API] ← Rust: 리프 판정 · 쿼리 재작성 (temperature 0, strict JSON)
               ← ④: 도구 선택 · 서술 답변 · 긴 대화 요약
```

## 5. 컴포넌트

| # | 이름 | 기술 | 핵심 |
|---|---|---|---|
| ① | 내부 도구 API | Rust, axum, tokio | 업로드·검색·판정을 HTTP로 제공. 부르는 쪽은 ④ 하나(127.0.0.1에만 바인드). 세션 안의 업로드·판정은 actor로 직렬화 |
| ② | 추론 서버 | C++ — llama.cpp server 또는 onnxruntime | 임베딩(코퍼스와 동일 모델 필수). 리랭커는 후순위(기존 실측에서 250ms 개선 항목이지 필수 경로 아님). FFI 대신 별도 프로세스+HTTP |
| ④ | 채팅 에이전트 | Python — FastAPI, Deep Agents(LangGraph·LangChain), langgraph-checkpoint-postgres, langchain-openai | 사용자가 만나는 유일한 창구(HTTP·SSE). 도구 선택·서술 답변. 세션 기억 = 체크포인트 (§9.1). judge 판정 문장은 원문 그대로 전달. 코드 위치 `agent/` |
| ⑤ | 법률 엔진 | Rust, z3 crate, petgraph | §7 상세. 업로드 조항 검색·법령 검색·리프 캐시는 `engine/deps.rs`의 트레이트로 받고 ①이 구현한다(`server/engine_deps.rs`) — engine은 DB·sessdb·retrieval을 모른다 (의존성 역전) |
| ⑥ | 문서 파싱층 | Rust + pdfium 링크 | §8 상세 |
| ⑦ | 업로드 RAG | Rust | 청크=구조화 산출물(clause_id 재사용). 문서 1~2개 분량이라 인덱스 없이 메모리 내 brute-force cosine top-k. 재접속 대비 직렬화 사본만 RocksDB |
| ⑧ | 세션층 | LangGraph 체크포인트(④) + rust-rocksdb(①) | §9 상세 |
| ⑨ | 질문 라우팅 | ④의 도구 선택 | 요건 판단형이면 judge(`claim_type`, **`node_id`**, 가정), 일반이면 검색 도구. `node_id`로 부분 질문 지원 (§6.3). 노드 카탈로그는 ①의 GET /catalog에서 받아 judge 도구 설명에 넣는다 |
| ⑩ | 판정 문장 조립 | Rust 템플릿 | Z3 결론 + unsat_core 이유 + anchor 인용 + Unknown 목록 + **판정 범위 명시** → judge 응답. ④는 고치지 않고 화면에 따로 보낸다 |

알고리즘다운 신규 작업은 ⑤의 이식과 ⑥의 표 격자 재구성 둘. 나머지는 배선.

## 6. 처리 흐름

### 6.1 업로드

파싱(⑥) → Block+Anchor 통합 모델 → clause_id 부여 → 결정론 검증(날짜·합계·참조,
순수 Rust) → 청크 임베딩(②) → RocksDB `state`에 저장(WriteBatch) → 업로드 요약을
④에 돌려준다. ④는 그 요약을 대화에 넣어(에이전트가 올라온 파일을 기억) 사용자에게도 보낸다.
추출 못 한 부분은 「추출 불가」로 사용자에게 명시.

### 6.2 질문 — 두 경로

- **일반 경로**: ④가 search_upload·search_laws·search_cases를 불러 읽은 업로드 문서 top-k와
  법령·판례 top-k(③)로 서술한다. 문장마다 출처를 단다.
- **엔진 경로**: ④가 judge(노드 선택)를 부른다 → ①에서 서브트리의 리프마다 fact slice(업로드 문서,
  anchor 필수) + authority slice(DB 검색) 조립 → LLM 리프 판정 fan-out → Z3 컴파일·판정 →
  성립 / 불성립(unsat_core 이유) / 유보 → ⑩ 조립 → judge 응답 → ④가 판정 문장을 원문 그대로
  화면에 보내고(SSE `VerdictReady`) 서술을 덧붙인다.
- **수정판 재검증(두 속도)**: 결정론 검증(빠른 경로)은 문서 버전마다 전체 재실행,
  LLM 리프(느린 경로)는 cache_key가 바뀐 것만 재판정.

### 6.3 부분 질문

트리가 노드 단위 DAG라서 **어떤 노드든 루트로 삼아 서브트리만 판정**할 수 있다.
"이게 처분에 해당하나?" → `S1_처분성` 서브트리만 컴파일. "제소기간 지났나?" →
`S3_제소기간` 산술 리프 하나(LLM은 날짜 추출만).

- 가정을 얹은 질문("처분성은 인정된다 치고 …")은 Z3 모델 읽기로: 일부 리프 고정 +
  나머지 자유변수 → sat 모델 = 성립에 필요한 사실 조합. 자유변수로 채운 값은
  「문서 근거 없음」 표시를 단다 — 가정이지 판정이 아니므로.
- 부분 판정 결과는 leaf_cache에 쌓여, 이후 전체 질문 때 이미 판정된 리프를 재사용한다.
- **판정 범위 명시 필수**: "처분성 인정 ≠ 취소 인용". ⑩ 템플릿이
  「이 판정은 ○○ 서브트리에 한정, 나머지는 판정하지 않았음」을 자동으로 붙인다.

## 7. 법률 엔진 상세 (Leaf 구조 + Z3)

기준 문서: 「뉴로심볼릭_법리판단_구현_가이드북_확장판」 및 그 재구성 CLAUDE.md
(`~/Downloads/백업본-뉴로심볼릭-20260913/`). 파이썬 설계를 Rust로 이식한다.

### 7.1 리프 판정기

- LLM 호출 형태: 명제 1개 + fact slice + authority slice → True / False / Unknown.
  temperature 0, 짧은 max_tokens, timeout, 지수 backoff, strict JSON(실패 시 1회 재요청).
- `build_leaf_requests` → semaphore 제한 병렬 `judge_leaf` → `LeafResult`.
- 단계 사이에 CoT나 대화 이력을 넘기지 않는다. typed 객체만 넘긴다:
  `DocumentState → Issue → LegalIssue → RuleSet → AuthoritySnapshot → LeafResult → ReviewResult`.
- 모순 검출: 긍정/부정 쌍 리프를 `assert_and_track`으로 넣어 unsat이면 재판정,
  여전히 충돌이면 둘 다 Unknown.

```rust
enum LeafResult {
    Judged  { value: bool, anchor: Anchor, quote: String }, // 판정 근거 좌표 필수
    Unknown { failure: FailureReason },                     // Unknown의 이유 — 보고서까지 올라간다
}
```

판정된 값과 Unknown을 변형으로 가른 이유: True/False인데 anchor가 없는 값을 타입상 만들 수
없게 한다. anchor 없는 값은 Z3에 들어가지 못한다 (§7.2 마지막 항목).

### 7.2 Z3 사용 규칙 (z3 crate — libz3 링크, 별도 프로세스 불필요)

- 필요한 API: `Solver`, `Bool/Int/Real`, `And/Or/Not/Implies`,
  `assert_and_track` → `get_unsat_core`, `model`.
- 결과는 sat / unsat / **unknown** 삼값을 타입으로 강제. unknown = 결론 유보.
- 실패 이유는 LLM 사후 설명이 아니라 **unsat_core**에서 뽑는다.
- 날짜는 ordinal 정수로만 넣는다. 초일 산입·영업일·말일 보정·윤년 등 달력 규칙은
  Rust 함수(chrono)가 담당한다. Z3에 달력을 구현하지 않는다.
- LLM에게 Z3 식을 생성·실행시키지 않는다. 규칙은 선언 데이터에서 결정론적으로 컴파일.
- 잘못 추출된 값을 Z3에 넣으면 솔버가 "정확하게 틀린 답"을 낸다 — 사실 추출은
  반드시 anchor(출처 clause_id)를 동반하고, 근거 없는 값은 Z3에 들어가지 못한다.

### 7.3 요건 트리 스키마와 모델링 절차

```
claim_nodes: node_id, claim_type, description, gate_type(LEAF/AND/OR),
             burden, is_leaf, proposition(리프 판정 문장)
claim_edges: parent_id, child_id, relation(ELEMENT/DEFENSE/REBUTTAL)
```

- 트리라 부르지만 DAG다. **공유 리프 허용** — 처분성 리프 묶음은 취소소송·무효확인
  트리가 같이 쓴다.
- 적재 가드: 순환 검사(petgraph `is_cyclic_directed`) + 청구유형별 기대 노드·엣지 수
  고정 검사. 불일치 시 **부분 적재 금지, 전체 중단**.
- 모델링 절차(청구유형 하나당): ① 루트 결정 → ② 요건 리프 나열 → ③ 항변·재항변 층
  → ④ 리프 문장 작성 → ⑤ 날짜·금액 리프에 산술 표시.
- 리프 문장 규칙: "누가·언제·무엇을"이 들어간 **사실 문장**. 법적 평가어("처분성이
  있다", "재량을 남용했다")는 결론이므로 하위 징표로 쪼갤 때까지 내려간다.
  금지: 결론을 리프로 / 한 리프에 두 사실 / 항변을 요건의 단순 부정으로 /
  위법성·인과관계를 통째로 LLM에게.
- 청구유형 하나당 노드 10~30개 수준의 수작업. 리프 문장의 재료는
  `~/Downloads/대학 행정법 총정리.pdf`(93쪽)의 개념 징표·판례 기준.

### 7.4 행정법 예시 트리

**국가배상(국가배상법 제2조)** — 총정리 PDF 33~39쪽이 사실상 완성된 트리:

```
CLAIM_국가배상                        AND
├─ R1_공무원          (기능적 의미 — 위탁받은 사인 포함)
├─ R2_직무            (입법·사법 포함, 사경제작용 제외)
├─ R3_집행하면서      (외형설)
├─ R4_고의과실        (평균적 공무원 기준)
├─ R5_법령위반        OR
│   ├─ R5a_엄격한 법규 위반
│   └─ R5b_인권존중·권력남용금지·신의성실 위반
├─ R6_타인
├─ R7_손해            (반사적 이익 침해 제외)
└─ R8_인과관계
```

**취소소송**:

```
CLAIM_처분취소                        AND
├─ 소송요건            AND
│   ├─ S1_처분성       AND   ← 개념 징표로 분해
│   │   ├─ 행정청의 행위인가
│   │   ├─ 구체적 사실에 대한 행위인가
│   │   ├─ 법집행행위로서 공권력 행사인가
│   │   └─ 외부에 직접적 법효과를 미치는가
│   ├─ S2_원고적격(법률상 이익)
│   ├─ S3_제소기간     AND   ← 산술 리프: 안 날+90일, 있은 날+1년(역에 따라 계산) → Z3 정수 비교
│   │                           행정심판을 거치면 기산점이 재결로 바뀌고, 1년은 정당한 사유가 있으면 예외
│   │                           (행정소송법 제20조 — rules/claim_취소소송.yaml)
│   └─ S4_협의의 소익
└─ 본안_위법성         OR
    ├─ 절차 하자 (사전통지·이유제시 결여 등)
    ├─ 법률유보 위반
    └─ 재량 하자      OR
        ├─ 재량권의 일탈
        ├─ 재량권의 남용 OR (사실오인 / 비례원칙 위반 / 평등원칙 위반)
        └─ 재량권의 불행사
```

학설 대립이 있는 리프는 판례 입장을 판정 기준 문장으로 적고, 그 판례를
authority slice로 리프에 연결한다.

### 7.5 트리 작성 전 결정 사항 — 입증책임

가이드북의 burden(PLAINTIFF/DEFENDANT)과 요건/항변/재항변 층은 민사 요건사실론
기준이다. 행정소송은 소송요건이 직권조사사항이고, 본안에서 처분의 적법성 입증책임은
원칙적으로 처분청, 재량 일탈·남용은 원고가 입증한다. 따라서 burden enum을 행정소송
의미로 재정의(직권조사 표시 추가)하거나 입증책임 귀속을 리프별 속성으로 옮기는 결정을
**트리 그리기 전에** 해야 한다. 이 귀속이 Unknown 리프의 불이익 방향(입증 못 하면
누구에게 불리한가)을 정하므로 Z3 결론 조립에 직접 영향을 준다.

## 8. 문서 파싱층

**형식별 난이도: DOCX·HWPX 쉬움(표 공짜) → PDF 어려움 → PDF 표가 최난.**

| 형식 | 방법 | 표 |
|---|---|---|
| DOCX | zip + quick-xml 직접 파싱(기존 crate는 쓰기 위주) | `<w:tbl>` 명시 구조. `gridSpan`(가로 병합)·`vMerge`(세로 병합)만 처리 |
| HWPX | 동일 (zip+XML, KS X 6101) | `<hp:tbl>` 명시 구조 |
| HWP(구형) | 직접 파싱 비추천(CFB+zlib 레코드). 외부 변환기 또는 범위 제외+변환 안내 | — |
| PDF(텍스트) | pdfium (`pdfium-render`) — 글자+좌표(bbox) | 구조 없음 → 기하 재구성 (아래) |
| PDF(스캔) | 범위 제외 권장, 「추출 불가·판정 불가」 명시 출력 | — |

**PDF 표 재구성**: 괘선 있는 표는 pdfium에서 선분(path)을 뽑아 수평·수직선을 격자로
스냅 → 셀 사각형 생성 → 글자를 bbox로 셀에 배정 (Camelot lattice 방식과 동일 알고리즘,
Rust로 작성). 괘선 없는 표는 페이지를 이미지로 렌더 → 멀티모달 LLM에 JSON 추출 —
단 **신뢰 등급 하락 표시 + 원문 페이지·좌표 인용** 필수.

**통합 문서 모델** — 파서가 셋이어도 출력은 하나:

```rust
enum Block { Heading(..), Para(..), Table(Table) }
struct Table { cells: Vec<Cell>, anchor: Anchor }   // row, col, rowspan, colspan, text
struct Anchor { source: FileId, page: Option<u32>, bbox: Option<Rect>, path: Option<String> }
```

clause_id 부여·결정론 검증·RAG 청킹·fact slice가 전부 이 모델 위에서 돌고, 모든 값이
anchor를 달고 다닌다. 표를 평문으로 뭉개지 않는 이유: 과징금 산정 내역, 별표 금액
합계가 "합계=부분합" 결정론 검증과 Z3 산술의 입력이다. 표를 글자로 바꾸는 일은
`Table::to_markdown` 하나만 한다 — clause 텍스트(RAG·fact slice)와 LLM에 넘길 슬라이스가 같은 글자를 쓴다.

파서는 판정의 입구이므로, 실제 문서 표본으로 추출 정확도(특히 표 셀 배정)를 실측한
뒤 저장 경로에 붙인다.

## 9. 세션층 — LangGraph 체크포인트 + RocksDB

코퍼스는 PostgreSQL(읽기 전용), 채팅 세션(대화와 에이전트가 읽은 것)은 ④의 LangGraph
체크포인트(PostgreSQL, 코퍼스와 다른 데이터베이스), Rust 쪽 업로드 문서 파싱 결과와 리프 판정
캐시는 RocksDB로 역할을 나눈다.

### 9.1 채팅 세션 기억 (④, langgraph-checkpoint-postgres)

세션 ID = LangGraph `thread_id`. 에이전트가 한 단계를 끝낼 때마다 체크포인트가 저장된다.

| 상태 키 | 담기는 것 |
|---|---|
| `messages` | 사용자 질문, 에이전트 답변, 도구 호출과 도구 결과(= 에이전트가 읽은 것, judge 판정 문장 포함), 업로드 요약 |
| `files` `/large_tool_results/` | 2만 토큰 넘는 도구 결과의 원문. 메시지에는 앞뒤 일부와 경로만 남는다 |
| `files` `/conversation_history/` | 긴 대화를 요약할 때 모델 입력에서 빠진 앞부분 대화의 원문 |

- 대화가 모델 문맥 한도의 85%(한도를 모르는 모델이면 17만 토큰)를 넘으면 deepagents가 앞부분을
  요약해 모델에 보낸다. 이때도 `messages` 원본은 지우지 않는다 — 모델 입력만 바뀐다.
- `files`는 StateBackend(create_deep_agent 기본값)라 체크포인트와 함께 저장된다.
  FilesystemBackend로 바꾸면 파일이 체크포인트에 들어가지 않아 세션 기억에서 빠진다.
- 하위 에이전트(task 도구, 기본으로 붙음)가 읽은 도구 결과는 세션에 남지 않는다. 마지막 답변
  하나와 `files`만 돌아온다. 그래서 검색·읽기는 메인 에이전트가 직접 한다.
- 세션 사이 공유 기억(LangGraph Store)은 쓰지 않는다. 기억 범위는 세션 하나.
- 확인 상태 (2026-09-29): 위 동작은 deepagents 0.7.19 소스와, 가짜 모델 + in-memory 체크포인트
  시험으로 확인했다. 시험에서 106,890자 도구 결과가 `/large_tool_results/`에 원문 그대로
  저장됐고, 새로 만든 에이전트 객체가 같은 `thread_id`의 앞 답변과 도구 결과를 입력으로 받았다.
  PostgreSQL 체크포인트로도 확인했다: 첫 질문 프로세스가 끝난 뒤 새 프로세스가 같은 `thread_id`로
  질문하자 앞 답변·도구 결과가 모델 입력에 들어 있었고, 106,890자 도구 결과의 SHA-256이 원문과
  같았다. `adelete_thread` 뒤에는 그 세션의 체크포인트 행이 세 테이블 모두 0이었다.

### 9.2 RocksDB (①, rust-rocksdb)

| CF | key | value |
|---|---|---|
| `state` | `{session_id}/doc/{file_id}` 등 | DocumentState·구조화 산출물·업로드 벡터 사본·Finding (serde 직렬화) |
| `leaf_cache` | cache_key = hash(rule_id + fact slice 해시 + authority 스냅샷 해시) | LeafResult의 serde_json 바이트 (직렬화는 `server/engine_deps.rs` — sessdb는 LeafResult 타입을 모른다) |

- `leaf_cache`가 두 속도(§6.2)와 부분→전체 점증 대화(§6.3)를 구현하는 자리.
- 세션 조회 = prefix iterator, 세션 삭제 = `state`에 `delete_range` (`leaf_cache`는 세션과
  무관한 전역 캐시라 지우지 않는다).
- 업로드 한 건의 산출물은 `WriteBatch` 원자 쓰기.

## 10. 동시성 설계

기조: **I/O 대기만 비동기(tokio), CPU 코어 로직은 동기 순수 함수 + `spawn_blocking` 격리.**

| 성격 | 해당 | 처리 |
|---|---|---|
| I/O | LLM API, ② 호출, PostgreSQL, ④의 도구 호출 수신 | tokio + 커넥션 풀(sqlx, reqwest) |
| CPU | 파싱·표 재구성, Z3, cosine, 결정론 검증 | 동기 함수 → `spawn_blocking` |
| 중간 | RocksDB | 블로킹 API지만 µs~ms. 핸들 `Arc` 공유, 긴 iterator만 `spawn_blocking` |

- 엔진·파서를 async로 물들이지 않는다 — 단위 테스트와 정확도 실측이 런타임 없이 가능해야 한다.
- 병렬 발사: 리프 판정 fan-out(`JoinSet`+`Semaphore` 8~16 +timeout+backoff),
  쿼리 재작성∥임베딩(`tokio::join!`), 업로드 검색∥DB 검색.
- **스레드 안전성 함정 둘**: pdfium은 스레드 세이프 아님 → 전용 스레드 1개+채널 직렬화.
  Z3 Context는 스레드 간 공유 불가 → 판정마다 생성·폐기(ms 단위라 비용 없음).
- **세션 안은 직렬, 세션 사이는 병렬** — 두 곳에서 지킨다:
  - ④: 세션별 `asyncio.Lock`. 한 `thread_id`에 실행은 하나만 — 같은 체크포인트에서 갈라진 두 실행
    중 한쪽 기록이 최신 상태에서 빠질 수 있다. 업로드가 오면 진행 중인 질문 실행을 취소하고
    락을 넘겨받는다 (수정판 우선). 취소로 답 없이 남은 도구 호출은 다음 실행 때
    PatchToolCallsMiddleware(기본으로 켜짐)가 메운다.
  - ①: 세션당 actor(mpsc+단일 소비 태스크)가 업로드·판정을 직렬화한다.
- 문서 수정판 도착 시 진행 중 리프 판정은 `CancellationToken`으로 취소 → 그 judge는
  「판정 불가: 문서 수정판 도착」으로 응답.
- 캐시 스탬피드 방지: moka `get_with` (동일 키 동시 미스 합류).

## 11. 오류 처리 설계

대원칙(§2-2): 오류는 Unknown/판정 불가로만 무너진다.

| 층 | 오류 | 처리 |
|---|---|---|
| LLM 리프 | timeout·429·5xx / JSON 파싱 실패 | backoff 재시도(횟수 제한) / 1회 재요청 → 최종 실패 = **Unknown + 사유**. 기본값 True/False 채움 금지 |
| LLM 리프 | 긍정·부정 쌍 모순 | 재판정 → 여전히 충돌 시 둘 다 Unknown |
| Z3 | unknown 결과 | 결론 유보로 보고 (unsat 취급 금지). 삼값을 타입으로 강제 |
| Z3 | 패닉·자원 초과 | `spawn_blocking` 경계에서 포획 → 판정 불가 |
| 파싱 | 손상·암호화·빈 추출 | 「추출 불가」 명시. 부분 성공 시 성공 범위 명시 |
| 파싱 | 표 재구성 실패 | 평문 강등 + **산술 검증·Z3 입력에서 제외** |
| 검색 | ② 다운 / 리랭커 실패 / 재작성 실패 | FTS 단독 강등(명시) / 후보 순서 사용 / 원 질의로 검색 |
| PG | 일시 오류 / 제약 위반 | 재시도 / 즉시 실패 (구분) |
| RocksDB | 쓰기 실패 | 세션에 오류 반환 (삼키지 않음) |
| ④ 도구 호출 | ① HTTP 오류·timeout | 예외로 올리지 않고 「검색 실패: 사유」·「판정 불가: 사유」 문자열을 도구 결과로 → 에이전트가 답변에 밝힌다 |
| ④ 체크포인트 | 쓰기 실패 | 그 질문을 Error로 끝낸다 — 저장 못 한 답을 기억한 것처럼 두지 않는다 |
| DAG 적재 | 검증 실패 | **부분 적재 금지, 전체 중단** — 반쪽 트리 판정이 더 위험 |

- Rust: 층별 `thiserror` enum(`ParseError`/`RetrievalError`/`LeafJudgeError`/`EngineError`/`StorageError`),
  코어 `unwrap`/`panic` 금지(clippy 강제), axum 패닉 포획 레이어.
- 멱등성으로 재시도 안전: cache_key, `WriteBatch`.
  SSE가 끊기면 ④의 GET /session/{id}/history가 체크포인트에서 대화를 다시 그린다.
- Python(④): 도구 실패는 문자열로, 체크포인트 실패는 Error 이벤트로 — 어느 쪽도 삼키지 않는다.
- 종착점은 로그가 아니라 사용자에게 보이는 문장: 리프 N개 중 M개 Unknown이면
  결론에 목록과 이유를 그대로 내보낸다
  (예: "R4_고의과실: 업로드 문서에서 근거 조항을 찾지 못함 — 판정 불가").

## 12. 착수 전 남은 항목

**실측 4건 (1~3은 아직 안 쟀음, 4는 2026-09-29 확인 — §9.1)**
1. 기존 임베딩 30만+412만 행이 어떤 모델 세트인지 — 질의 임베딩 모델과 일치 필수.
   다르면 재임베딩 규모(판례 412만 행 = 며칠 단위)부터 산정.
2. 기존 DB의 한국어 FTS 인덱스 방식(mecab 계열 확장 / pg_trgm / bigram) —
   ③이 같은 방식으로 질의해야 한다.
3. 판례 341만 건에서 행정 사건만 거르는 분류 칼럼 유무.
4. PostgreSQL 체크포인트로 ④를 다시 띄운 뒤 같은 세션이 앞 답변·읽은 것을 이어받는지 (§9.1).

**결정 4건**
1. burden/직권조사 의미 재정의 (§7.5 — 요건 트리 작성 전 필수).
2. LLM 선택 — 리프 판정·서술 생성을 외부 API로 갈지 로컬로 갈지.
3. 입력 범위 확정 — HWP 구형·스캔 PDF 제외 여부와 안내 문구.
4. 체크포인트를 둘 PostgreSQL 데이터베이스 — 기존 컨테이너에 새 데이터베이스를 만들지, 새 컨테이너를 띄울지.

**미논의 2건**
1. 웹 프론트엔드.
2. 평가 표본 — 리프 판정기의 오탐·누락 실측 없이는 엔진 결과를 해석할 수 없다.
   무엇을 재서 무엇이 나오면 성공인지 착수 전에 한 줄로 정한다.

## 13. 참고 자산 (모티브 — 코드 재사용 아님)

| 자산 | 경로 | 가져올 것 |
|---|---|---|
| 뉴로심볼릭 가이드북 | `~/Downloads/백업본-뉴로심볼릭-20260913/` (가이드북 PDF + 재구성 CLAUDE.md 3,652줄) | Leaf/Z3 설계 전체 (§7) |
| 행정법 교재 | `~/Downloads/대학 행정법 총정리.pdf` (93쪽) | 요건 트리 리프 문장의 재료 (개념 징표·판례 기준). 파일명이 NFD 정규화라 셸에서 글자 그대로 매칭 안 될 수 있음 |
