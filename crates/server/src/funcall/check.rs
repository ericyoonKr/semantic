use citegate::{LAW_NOT_READ, NAMED_NOT_READ, UNIT_NOT_READ, Verdict};
use retrieval::Corpus;
use serde::{Deserialize, Serialize};

use super::docs::session_docs;
use super::known::{KnownLaws, law_of_label};
use crate::state::AppState;

pub const MAX_SENTENCES: usize = 200;

#[derive(Debug, Deserialize)]
pub struct CheckRequest {
    pub thread_id: String,
    pub sentences: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckReply {
    pub results: Vec<Verdict>,
}

#[derive(Debug, Deserialize)]
pub struct UnreadRequest {
    pub thread_id: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct UnreadReply {
    pub unread: Vec<String>,
}

pub fn split(text: &str) -> Vec<String> {
    let mut rest = text.to_string();
    let mut out = Vec::new();
    while let Some(at) = citegate::cut(&rest) {
        out.push(rest[..at].to_string());
        rest.drain(..at);
    }
    if !rest.trim().is_empty() {
        out.push(rest);
    }
    out
}

async fn doc_text(state: &AppState, thread: &str) -> String {
    let db = state.db.clone();
    let session = thread.to_string();
    let docs = tokio::task::spawn_blocking(move || session_docs(&db, &session)).await.unwrap_or_default();
    citegate::compact(&citegate::clean(&docs.iter().map(|d| d.text.as_str()).collect::<Vec<_>>().join("\n")))
}

async fn outside_corpus(state: &AppState, label: &str) -> bool {
    match law_of_label(label) {
        Some(law) => !state.known.in_corpus(state.corpus.as_ref(), law).await,
        None => false,
    }
}

async fn real_laws(known: &KnownLaws, corpus: Option<&Corpus>, names: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        for candidate in citegate::suffixes(&name) {
            if known.surely_in_corpus(corpus, &candidate).await {
                let law = citegate::canonical(&candidate);
                if !out.contains(&law) {
                    out.push(law);
                }
                break;
            }
        }
    }
    out
}

fn settle_with(mut verdict: Verdict, named: Vec<String>, kept: Vec<String>) -> Verdict {
    let unit_like = |r: &&str| *r == UNIT_NOT_READ || *r == LAW_NOT_READ;
    if kept.iter().all(|l| law_of_label(l).is_none()) {
        verdict.reasons.retain(|r| !unit_like(r));
    }
    if named.is_empty() {
        verdict.reasons.retain(|r| *r != NAMED_NOT_READ);
    }
    verdict.named = named;
    verdict.unread = kept;
    verdict.ok = verdict.reasons.is_empty();
    verdict
}

async fn settle(state: &AppState, docs: &str, verdict: Verdict) -> Verdict {
    if verdict.ok {
        return verdict;
    }
    let named = real_laws(&state.known, state.corpus.as_ref(), verdict.named.clone()).await;
    let mut kept = Vec::new();
    for label in &verdict.unread {
        let from_docs = law_of_label(label).is_some_and(|law| docs.contains(law));
        if !(from_docs && outside_corpus(state, label).await) {
            kept.push(label.clone());
        }
    }
    settle_with(verdict, named, kept)
}

pub async fn check_sentences(state: &AppState, req: &CheckRequest) -> CheckReply {
    let thread = req.thread_id.trim();
    let set = state.reads.snapshot(thread);
    let verdicts: Vec<Verdict> = req.sentences.iter().map(|s| citegate::check(s, &set)).collect();
    if verdicts.iter().all(|v| v.ok) {
        return CheckReply { results: verdicts };
    }
    let docs = doc_text(state, thread).await;
    let mut results = Vec::with_capacity(verdicts.len());
    for v in verdicts {
        results.push(settle(state, &docs, v).await);
    }
    CheckReply { results }
}

pub async fn unread_in_corpus(state: &AppState, req: &UnreadRequest) -> UnreadReply {
    let thread = req.thread_id.trim();
    let set = state.reads.snapshot(thread);
    let mut unread: Vec<String> = Vec::new();
    for sentence in split(&req.text) {
        let verdict = citegate::check(&sentence, &set);
        let named = real_laws(&state.known, state.corpus.as_ref(), verdict.named).await;
        for label in verdict.unread.into_iter().chain(named) {
            if unread.contains(&label) || outside_corpus(state, &label).await {
                continue;
            }
            unread.push(label);
        }
    }
    UnreadReply { unread }
}

#[cfg(test)]
mod tests {
    use citegate::{ReadSet, Verdict};

    use super::{real_laws, settle_with};
    use crate::funcall::known::KnownLaws;

    const LAWS: [&str; 29] = [
        "식품위생법", "경범죄처벌법", "민법", "형법", "헌법", "상법", "도로교통법", "행정절차법", "행정소송법", "국가배상법", "지방자치법",
        "개인정보보호법", "근로기준법", "국가공무원법", "지방공무원법", "마약류관리에관한법률", "소득세법", "법인세법", "국세기본법", "건축법",
        "주택법", "공직선거법", "환경정책기본법", "하천법", "도로법", "청소년보호법", "약사법", "의료법", "공무원행동강령",
    ];

    const PASS: [&str; 30] = [
        "식품위생법에 따라 영업정지 처분을 할 수 있습니다.",
        "식품위생법상 영업자 준수사항입니다.",
        "클럽이 식품위생법을 위반했습니다.",
        "처리 방법은 두 가지입니다.",
        "해결방법을 알려 드립니다.",
        "불법 행위를 방치했습니다.",
        "위법 여부를 따져야 합니다.",
        "적법 절차를 거쳤는지 봅니다.",
        "합법적인 영업인지 확인합니다.",
        "조리법은 중요하지 않습니다.",
        "관련 법령에 따라 처리됩니다.",
        "관련 법률을 찾아보겠습니다.",
        "행정법 일반 원리입니다.",
        "노동법 분야의 문제입니다.",
        "구청장의 명령에 따릅니다.",
        "내부 훈령에 따른 조치입니다.",
        "회사 규칙에 따라 징계합니다.",
        "용법과 용량을 지킵니다.",
        "문법에 맞게 씁니다.",
        "기법이 다양합니다.",
        "요령껏 처리합니다.",
        "입법 취지를 살펴봅니다.",
        "사법 심사의 대상입니다.",
        "공법상 계약입니다.",
        "세법상 불이익은 없습니다.",
        "편법을 쓰면 안 됩니다.",
        "화법이 거칩니다.",
        "업주의 관리 책임이 문제됩니다.",
        "이 사건의 쟁점은 방치 여부입니다.",
        "영업정지 기간은 뒤에 설명합니다.",
    ];

    const BLOCK: [&str; 30] = [
        "경범죄 처벌법으로도 처벌될 수 있습니다.",
        "민법에 따라 손해를 배상합니다.",
        "형법상 방조죄가 성립합니다.",
        "헌법에 따른 기본권 제한입니다.",
        "상법에 따른 회사입니다.",
        "도로교통법 위반입니다.",
        "행정절차법에 따라 사전통지를 합니다.",
        "행정소송법에 따라 취소소송을 냅니다.",
        "국가배상법에 따라 배상합니다.",
        "지방자치법에 근거합니다.",
        "개인정보 보호법을 위반했습니다.",
        "근로기준법에 따른 수당입니다.",
        "국가공무원법상 징계 사유입니다.",
        "지방공무원법에 따라 징계합니다.",
        "마약류 관리에 관한 법률에 따라 처벌합니다.",
        "소득세법에 따라 과세합니다.",
        "법인세법상 손금입니다.",
        "국세기본법에 따른 불복 절차입니다.",
        "건축법 위반 건축물입니다.",
        "주택법에 따른 사업계획 승인입니다.",
        "공직선거법에 따라 처벌됩니다.",
        "환경정책기본법의 원칙입니다.",
        "하천법에 따른 점용허가입니다.",
        "도로법에 따른 도로점용입니다.",
        "청소년 보호법에 따라 출입을 막습니다.",
        "이 사건에서 경범죄 처벌법이 적용됩니다.",
        "업주는 식품위생법과 마약류 관리에 관한 법률을 모두 위반했습니다.",
        "약사법에 따른 처분입니다.",
        "의료법 위반 여부도 봅니다.",
        "공무원 행동강령에 따릅니다.",
    ];

    async fn settled(known: &KnownLaws, reads: &ReadSet, sentence: &str) -> Verdict {
        let verdict = citegate::check(sentence, reads);
        if verdict.ok {
            return verdict;
        }
        let named = real_laws(known, None, verdict.named.clone()).await;
        let kept = verdict.unread.clone();
        settle_with(verdict, named, kept)
    }

    #[tokio::test]
    async fn bare_names_block_only_real_laws_that_were_not_read() {
        let known = KnownLaws::preset(&LAWS);
        let mut reads = ReadSet::default();
        reads.add_unit_body("식품위생법", "제75조", "① 영업정지를 명할 수 있다.");
        let mut wrongly_blocked = Vec::new();
        for s in PASS {
            let v = settled(&known, &reads, s).await;
            if !v.ok {
                wrongly_blocked.push((s, v.reasons, v.named));
            }
        }
        let mut let_through = Vec::new();
        for s in BLOCK {
            if settled(&known, &reads, s).await.ok {
                let_through.push(s);
            }
        }
        println!("pass_sample={} wrongly_blocked={} block_sample={} let_through={}", PASS.len(), wrongly_blocked.len(), BLOCK.len(), let_through.len());
        assert!(wrongly_blocked.is_empty(), "{wrongly_blocked:?}");
        assert!(let_through.is_empty(), "{let_through:?}");
        let v = settled(&known, &reads, "이 사건에서 경범죄 처벌법이 적용됩니다.").await;
        assert_eq!(v.named, vec!["경범죄처벌법".to_string()]);
        let v = settled(&known, &reads, "식품위생법에 따라 영업정지 처분을 할 수 있습니다.").await;
        assert_eq!(v.cites.len(), 1);
        assert_eq!(v.cites[0].law, "식품위생법");
    }
}
