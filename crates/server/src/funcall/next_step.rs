use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use citegate::Mention;
use serde::{Deserialize, Serialize};

use super::known::names_a_law;

pub const WHOLE_READ_CHARS: u64 = 20_000;
pub const WHOLE_READ_LINES: u32 = 2_000;
pub const EXCERPT_CHARS: usize = 1_500;
pub const ISSUE_HEAD: &str = "쟁점:";
pub const PURPOSE_HEAD: &str = "목적:";
pub const FIRST_PURPOSE: &str = "조회";
pub const EXCERPT_HEAD: &str = "올린 문서에서 법령이 적힌 줄:";
pub const QUESTION_LAWS_HEAD: &str = "질문에 나온 법령(먼저 쟁점 조문을 찾아 읽는다):";
pub const HISTORY_HEAD: &str = "앞선 질문:";
pub const LAST_ANSWER_HEAD: &str = "직전 답 요지:";
pub const READ_HEAD: &str = "이 대화에서 이미 읽은 근거:";
pub const HISTORY_MAX: usize = 5;
pub const HISTORY_CHARS: usize = 200;
pub const LAST_ANSWER_CHARS: usize = 1500;
pub const READ_MAX: usize = 12;
const GENERIC_LAW_WORDS: [&str; 4] = ["판례", "조문", "조항", "근거"];
const LAW_NAME_MIN: usize = 3;
const ALIAS_WORDS: usize = 3;
pub const RESEARCHER: &str = "researcher";
const KEYWORD_MIN: usize = 2;
const KEEP: Duration = Duration::from_secs(24 * 3600);
const MAX_THREADS: usize = 10_000;
const STRIP: &[char] = &['?', '!', '.', ',', '·', '「', '」', '『', '』', '(', ')', '[', ']', '"', '\''];
const PARTICLES: [&str; 17] = ["이란", "인가", "인지", "에서", "으로", "란", "은", "는", "이", "가", "을", "를", "에", "의", "로", "도", "만"];

fn names_law(text: &str) -> bool {
    let quoted = text.find('「').is_some_and(|open| text[open..].contains('」'));
    quoted || citegate::mentions(text).iter().any(|m| matches!(m, Mention::Unit { law: Some(_), .. }))
}

#[derive(Debug, Clone)]
pub struct Doc {
    pub file_id: String,
    pub filename: String,
    pub chars: u64,
    pub uploaded_at: u64,
    pub processing: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "tool", content = "args", rename_all = "snake_case")]
pub enum Step {
    Model,
    ReadFile { file_path: String, limit: u32 },
    SearchDocument { query: String },
    Task { description: String, subagent_type: String },
}

#[derive(Debug, Serialize)]
pub struct StepReply {
    #[serde(flatten)]
    pub step: Step,
    pub notes: Vec<String>,
    pub guidance: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub researcher_first: Option<ToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolCall {
    pub tool: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct StepRequest {
    pub request_id: String,
    pub question: String,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub history: Vec<String>,
    #[serde(default)]
    pub last_answer: Option<String>,
}

pub const MUST_READ_HEAD: &str = "먼저 읽을 조문:";
pub const KG_HEAD: &str = "지식그래프(kg_find로 개념·청구·범죄·절차를 찾고 kg_expand로 요건·항변·근거 조문으로 펼친 뒤 read_law로 읽는다. 없으면 검색):";
pub const KG_MAX: usize = 16;

pub fn kg_line(graphs: &[(String, String)]) -> Option<String> {
    if graphs.is_empty() {
        return None;
    }
    let items: Vec<String> = graphs.iter().take(KG_MAX).map(|(name, shape)| format!("{name}({shape})")).collect();
    Some(format!("{KG_HEAD} {}", items.join(", ")))
}

pub const KG_FOUND_HEAD: &str = "지식그래프에서 찾은 노드(그 그래프에서 kg_find로 번호를 받아 kg_expand로 펼친다):";
pub const KG_FIND: &str = "kg_find";
pub const KG_FIND_LIMIT: usize = 3;
pub const KG_TERM_MIN: usize = 3;
pub const KG_TERMS_MAX: usize = 6;
pub const KG_HITS_MAX: usize = 8;
const QUESTION_TAILS: [&str; 8] = ["인가요", "입니까", "나요", "까요", "세요", "어요", "는지", "을까"];

pub fn kg_terms(question: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in question.split_whitespace() {
        let word = strip_particle(word.trim_matches(STRIP));
        if word.chars().count() < KG_TERM_MIN || QUESTION_TAILS.iter().any(|t| word.ends_with(t)) || out.iter().any(|w| w == word) {
            continue;
        }
        out.push(word.to_string());
    }
    out.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    out.truncate(KG_TERMS_MAX);
    out
}

const SHAPE_ORDER: [&str; 6] = ["시점별", "나무", "흐름", "항목 목록", "찾아보기", "연결망"];

fn shape_rank(shape: &str) -> usize {
    SHAPE_ORDER.iter().position(|s| *s == shape).unwrap_or(SHAPE_ORDER.len())
}

pub fn kg_hits(
    graphs: &[(String, String)],
    terms: &[String],
    mut find: impl FnMut(&str, &str) -> Vec<String>,
) -> (Vec<(String, String)>, Option<ToolCall>) {
    let mut hits: Vec<(String, String)> = Vec::new();
    let mut first: Option<(usize, ToolCall)> = None;
    'terms: for term in terms {
        for (graph, shape) in graphs {
            for name in find(graph, term) {
                if hits.len() >= KG_HITS_MAX {
                    break 'terms;
                }
                let rank = shape_rank(shape);
                if first.as_ref().is_none_or(|(best, _)| rank < *best) {
                    first = Some((rank, ToolCall { tool: KG_FIND.to_string(), args: serde_json::json!({ "graph": graph, "text": term }) }));
                }
                hits.push((graph.clone(), name));
            }
        }
    }
    (hits, first.map(|(_, call)| call))
}

pub const KG_ISSUES_HEAD: &str = "그래프에서 뽑은 쟁점(이 이름을 쟁점으로 삼아 하나씩 조사하고, 질문이 묻는 것과 맞지 않는 이름은 뺀다):";
pub const KG_ISSUES_MAX: usize = 5;

pub fn kg_issues_line(hits: &[(String, String)]) -> Option<String> {
    let mut names: Vec<&str> = Vec::new();
    for (_, name) in hits {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
        if names.len() >= KG_ISSUES_MAX {
            break;
        }
    }
    (!names.is_empty()).then(|| format!("{KG_ISSUES_HEAD} {}", names.join(" / ")))
}

pub const BRIEF_KINDS: [&str; 23] = [
    "가압류·가처분 신청서",
    "강제경매신청서",
    "고소장",
    "답변서",
    "배당요구서",
    "보전처분 이의신청서",
    "사실조회신청서",
    "상고이유서",
    "상고장",
    "소송비용액확정신청서",
    "소장",
    "유체동산 강제집행신청서",
    "의견서",
    "준비서면",
    "집행정지 신청서",
    "채권압류 및 전부명령 신청서",
    "채권압류 및 추심명령 신청서",
    "추완항소장",
    "항고이유서",
    "항고장",
    "항소답변서",
    "항소이유서",
    "항소장",
];
const WRITE_WORDS: [&str; 7] = ["써", "작성", "초안", "만들어", "적어", "작성해", "작성하"];
pub const BRIEF_HEAD: &str = "서면:";
pub const BRIEF_WRITING_NOTE: &str = "서면 초안은 글 답변 안에 쓴다: 서면 규칙 그래프에서 읽은 필수 기재사항 순서로 항목을 두고 항목마다 근거 번호를 단다. 사실은 사용자 진술과 올린 문서에서만 가져오고 모르는 사실은 「[확인 필요: 무엇]」으로 비워 둔다. 법령 조문은 read_law로 읽은 것만 인용하고, 파일로 만들지 않는다.";

fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

pub fn asks_for_brief(question: &str) -> Option<&'static str> {
    let flat = squeeze(question);
    let kind = BRIEF_KINDS.iter().filter(|k| flat.contains(&squeeze(k))).max_by_key(|k| k.chars().count())?;
    WRITE_WORDS.iter().any(|w| flat.contains(w)).then_some(*kind)
}

pub fn brief_line(kind: &str) -> String {
    format!(
        "{BRIEF_HEAD} {kind} — 서면 규칙 그래프(brief_rules)에서 「{kind}」를 kg_find로 찾아 cites로 달린 조문을 모두 read_law로 읽고 필수 기재사항·제출 기한·형식을 모은다. 청구의 요건은 요건 나무(claims)에서 찾는다."
    )
}

pub fn kg_found_line(hits: &[(String, String)]) -> Option<String> {
    if hits.is_empty() {
        return None;
    }
    let items: Vec<String> = hits.iter().map(|(graph, name)| format!("{graph}: {name}")).collect();
    Some(format!("{KG_FOUND_HEAD} {}", items.join(" · ")))
}

fn one_line(text: &str, max: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

pub fn read_labels(set: &citegate::ReadSet) -> Vec<String> {
    let parts = set.parts();
    parts.units.iter().map(|(law, unit, _)| format!("{law} {unit}")).chain(parts.cases.iter().cloned()).take(READ_MAX).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    pub history: Vec<String>,
    pub last_answer: Option<String>,
    pub read: Vec<String>,
    pub scope: Vec<String>,
}

impl Context {
    pub fn new(req: &StepRequest, reads: &citegate::ReadSet) -> Self {
        let skip = req.history.len().saturating_sub(HISTORY_MAX);
        let history = req.history.iter().skip(skip).map(|q| one_line(q, HISTORY_CHARS)).filter(|q| !q.is_empty()).collect();
        let last_answer = req.last_answer.as_deref().map(|a| one_line(a, LAST_ANSWER_CHARS)).filter(|a| !a.is_empty());
        Context { history, last_answer, read: read_labels(reads), scope: Vec::new() }
    }

    fn lines(&self) -> String {
        let mut out = String::new();
        if !self.history.is_empty() {
            out.push_str(&format!("\n{HISTORY_HEAD}\n{}", self.history.iter().map(|q| format!("- {q}")).collect::<Vec<_>>().join("\n")));
        }
        if let Some(answer) = &self.last_answer {
            out.push_str(&format!("\n{LAST_ANSWER_HEAD} {answer}"));
        }
        if !self.read.is_empty() {
            out.push_str(&format!("\n{READ_HEAD} {}", self.read.join(", ")));
        }
        for line in &self.scope {
            out.push_str(&format!("\n{line}"));
        }
        out
    }
}

const MODIFIER_TAILS: [&str; 5] = ["는", "은", "할", "을", "던"];
const FORMAT_WORDS: [&str; 10] = ["표로", "영어로", "요약", "짧게", "길게", "다시 말해", "존댓말", "번역", "한 문장으로", "말투"];

pub fn asks_for_law(question: &str) -> bool {
    let raw: Vec<&str> = question.split_whitespace().map(|w| w.trim_matches(STRIP)).collect();
    let named = raw.iter().enumerate().any(|(i, w)| match strip_particle(w) {
        "법" => i == 0 || !MODIFIER_TAILS.iter().any(|t| raw[i - 1].ends_with(t)),
        word => GENERIC_LAW_WORDS.contains(&word) || norms::question_words().contains(&word),
    });
    named || !question_laws(question).is_empty()
}

pub fn asks_for_format(question: &str) -> bool {
    let flat = question.split_whitespace().collect::<Vec<_>>().join(" ");
    FORMAT_WORDS.iter().any(|w| flat.contains(w))
}

#[derive(Debug, Default)]
struct Progress {
    at: Option<Instant>,
    docs: HashSet<String>,
    turn: String,
    done: bool,
    reread: bool,
    legal: bool,
    cites: u32,
    flagged: Vec<String>,
}

pub const UNCITED_HEAD: &str = "근거 없이 쓴 문장(조문·판례를 찾아 붙이거나 뺀다):";
pub const DOC_CITE_LINE: &str = "올린 문서에서 가져온 사실에는 「파일명 · N쪽」을 붙인다.";

pub fn reread_brief(question: &str, unread: &[String], uncited: &[String], ctx: &Context, has_docs: bool) -> String {
    let mut out = brief(question, &[], "", ctx);
    if !unread.is_empty() {
        out.push_str(&format!("\n{MUST_READ_HEAD} {}", unread.join(", ")));
    }
    if !uncited.is_empty() {
        out.push_str(&format!("\n{UNCITED_HEAD}\n{}", uncited.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n")));
        if has_docs {
            out.push_str(&format!("\n{DOC_CITE_LINE}"));
        }
    }
    out
}

#[derive(Default)]
pub struct StepLog(Mutex<HashMap<String, Progress>>);

pub fn upload_path(file_id: &str) -> String {
    format!("/uploads/{file_id}.md")
}

fn strip_particle(word: &str) -> &str {
    PARTICLES.iter().find_map(|p| word.strip_suffix(p).filter(|rest| !rest.is_empty())).unwrap_or(word)
}

pub fn keyword(question: &str) -> Option<String> {
    question
        .split_whitespace()
        .map(|w| strip_particle(w.trim_matches(STRIP)).to_string())
        .filter(|w| w.chars().count() >= KEYWORD_MIN)
        .max_by_key(|w| w.chars().count())
}

fn sentences(text: &str) -> Vec<String> {
    let mut rest = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    while let Some(at) = citegate::cut(&rest) {
        out.push(rest[..at].trim().to_string());
        rest.drain(..at);
    }
    if !rest.trim().is_empty() {
        out.push(rest.trim().to_string());
    }
    out
}

pub fn law_lines(docs: &[Doc]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut size = 0;
    for doc in docs.iter().rev() {
        for text in sentences(&doc.text) {
            if !names_law(&text) || out.contains(&text) {
                continue;
            }
            if size + text.chars().count() > EXCERPT_CHARS {
                return out.join("\n");
            }
            size += text.chars().count();
            out.push(text);
        }
    }
    out.join("\n")
}

pub fn question_laws(question: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: String| {
        if !out.contains(&name) {
            out.push(name);
        }
    };
    for m in citegate::mentions(question) {
        if let Mention::Law(law) | Mention::Unit { law: Some(law), .. } = m {
            push(law);
        }
    }
    let mut outside = String::new();
    let mut depth = 0usize;
    for c in question.chars() {
        match c {
            '「' => depth += 1,
            '」' => depth = depth.saturating_sub(1),
            _ if depth == 0 => outside.push(c),
            _ => {}
        }
    }
    let words: Vec<&str> = outside
        .split_whitespace()
        .map(|w| {
            let w = strip_particle(w.trim_matches(STRIP));
            w.strip_suffix("상").unwrap_or(w)
        })
        .filter(|w| !w.is_empty())
        .collect();
    for word in &words {
        if word.chars().count() >= LAW_NAME_MIN && names_a_law(word) {
            push(citegate::compact(word));
        }
    }
    for n in (1..=ALIAS_WORDS).rev() {
        for start in 0..words.len().saturating_sub(n - 1) {
            let phrase = words[start..start + n].join(" ");
            let plain = citegate::compact(&phrase);
            if plain.chars().count() < 2 {
                continue;
            }
            let canon = citegate::canonical(&phrase);
            if canon != plain {
                push(canon);
            }
        }
    }
    out
}

pub fn brief(question: &str, laws: &[String], excerpt: &str, ctx: &Context) -> String {
    let issue = question.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = format!("{ISSUE_HEAD} {issue}\n{PURPOSE_HEAD} {FIRST_PURPOSE}{}", ctx.lines());
    if !laws.is_empty() {
        out.push_str(&format!("\n{QUESTION_LAWS_HEAD} {}", laws.join(", ")));
    }
    if !excerpt.is_empty() {
        out.push_str(&format!("\n{EXCERPT_HEAD}\n{excerpt}"));
    }
    out
}

pub const DOC_UNITS_MAX: usize = 8;

pub fn doc_units(docs: &[Doc]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for text in law_lines(docs).lines() {
        for m in citegate::mentions(text) {
            if let Mention::Unit { law: Some(law), unit, .. } = m {
                let label = format!("{} {unit}", citegate::canonical(&law));
                if !out.contains(&label) {
                    out.push(label);
                }
                if out.len() >= DOC_UNITS_MAX {
                    return out;
                }
            }
        }
    }
    out
}

pub fn task_brief(question: &str, laws: &[String], docs: &[Doc], ctx: &Context) -> String {
    let mut out = brief(question, laws, &law_lines(docs), ctx);
    let units = doc_units(docs);
    if !units.is_empty() {
        out.push_str(&format!("\n{MUST_READ_HEAD} {}", units.join(", ")));
    }
    out
}

fn readable_whole(doc: &Doc) -> bool {
    !doc.processing && doc.chars <= WHOLE_READ_CHARS
}

impl StepLog {
    pub fn after_answer(&self, thread: &str, req: &StepRequest, unread: &[String], uncited: &[String], cites: u32, ctx: &Context) -> Step {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let progress = map.entry(thread.to_string()).or_default();
        if progress.turn != req.request_id {
            progress.turn = req.request_id.clone();
            progress.done = true;
            progress.reread = false;
            progress.flagged.clear();
        }
        let _ = cites;
        if (unread.is_empty() && uncited.is_empty()) || progress.reread {
            return Step::Model;
        }
        progress.reread = true;
        progress.flagged = uncited.iter().map(|s| citegate::compact(s)).collect();
        let has_docs = !progress.docs.is_empty();
        Step::Task { description: reread_brief(&req.question, unread, uncited, ctx, has_docs), subagent_type: RESEARCHER.into() }
    }

    pub fn flagged(&self, thread: &str) -> Vec<String> {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).map(|p| p.flagged.clone()).unwrap_or_default()
    }

    pub fn new_turn(&self, thread: &str, request_id: &str) -> bool {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).is_none_or(|p| p.turn != request_id)
    }

    pub fn is_legal(&self, thread: &str) -> bool {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).is_none_or(|p| p.legal)
    }

    pub fn count_cites(&self, thread: &str, found: u32) -> u32 {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let progress = map.entry(thread.to_string()).or_default();
        progress.cites += found;
        progress.cites
    }

    pub fn next(&self, thread: &str, req: &StepRequest, legal: bool, docs: &[Doc], laws: &[String], ctx: &Context) -> Step {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if map.len() >= MAX_THREADS {
            map.retain(|_, p| p.at.is_some_and(|at| now.duration_since(at) < KEEP));
        }
        let progress = map.entry(thread.to_string()).or_default();
        progress.at = Some(now);
        if progress.turn != req.request_id {
            progress.turn = req.request_id.clone();
            progress.done = false;
            progress.reread = false;
            progress.legal = legal;
            progress.cites = 0;
            progress.flagged.clear();
        }
        if keyword(&req.question).is_none() || progress.done || !legal {
            progress.done = true;
            return Step::Model;
        }
        let mut newest: Vec<&Doc> = docs.iter().collect();
        newest.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at).then_with(|| a.file_id.cmp(&b.file_id)));
        if let Some(doc) = newest.iter().find(|d| readable_whole(d) && progress.docs.insert(d.file_id.clone())) {
            return Step::ReadFile { file_path: upload_path(&doc.file_id), limit: WHOLE_READ_LINES };
        }
        let large: Vec<String> = newest.iter().filter(|d| !readable_whole(d) && !progress.docs.contains(&d.file_id)).map(|d| d.file_id.clone()).collect();
        if !large.is_empty() {
            progress.docs.extend(large);
            return Step::SearchDocument { query: req.question.split_whitespace().collect::<Vec<_>>().join(" ") };
        }
        progress.done = true;
        Step::Task { description: task_brief(&req.question, laws, docs, ctx), subagent_type: RESEARCHER.into() }
    }
}

#[cfg(test)]
mod tests;
