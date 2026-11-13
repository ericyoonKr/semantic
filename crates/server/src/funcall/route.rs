use std::time::Duration;

use llm::{CallOptions, LlmClient};
use serde::Deserialize;

pub const SYSTEM: &str = "질문이 법률 문제를 다루는지 판정한다. 법률 문제: 법령·판례·행정처분·소송·심판·계약·권리와 의무·책임·처벌·과태료와 과징금·허가와 신고, 올린 공문서·처분서·계약서의 내용 검토와 그에 관한 문서 작성. 법률 문제가 아님: 인사, 부름말, 잡담, 화면이나 사용법 문의, 앞 답의 길이·말투·형식만 바꾸라는 요청. JSON {\"legal\": true} 또는 {\"legal\": false} 하나만 낸다.";
const DEADLINE: Duration = Duration::from_secs(20);
const MAX_TOKENS: u32 = 16;

#[derive(Debug, Deserialize)]
struct Judged {
    legal: bool,
}

pub fn prompt(question: &str, has_docs: bool) -> String {
    let docs = if has_docs { "이 대화에는 올린 문서가 있다." } else { "이 대화에는 올린 문서가 없다." };
    format!("{docs}\n질문: {question}")
}

pub async fn is_legal(llm: &LlmClient, question: &str, has_docs: bool) -> Option<bool> {
    let opts = CallOptions { max_tokens: MAX_TOKENS, deadline: DEADLINE, json_mode: true, ..CallOptions::default() };
    match llm.complete_json::<Judged>(SYSTEM, &prompt(question, has_docs), &opts).await {
        Ok(j) => Some(j.legal),
        Err(e) => {
            tracing::warn!(error = %e, "route.legal_failed");
            None
        }
    }
}
