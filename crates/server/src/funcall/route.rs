use std::time::Duration;

use llm::{CallOptions, LlmClient};
use serde::Deserialize;

pub const SYSTEM: &str = "질문이 법률 문제를 다루는지 판정한다. 법률 문제: 법령·판례·행정처분·소송·심판·계약·권리와 의무·책임·처벌·과태료와 과징금·허가와 신고, 올린 공문서·처분서·계약서의 내용 검토와 그에 관한 문서 작성. 이번 질문이 짧거나 혼자서는 뜻이 없으면 앞선 질문과 직전 답에 이어지는 물음으로 읽는다. 앞선 법률 질문의 근거·법령·조례·판례·절차·대상을 되묻거나 지역·업종·날짜 같은 사실을 보태는 말은 법률 문제다. 법률 문제가 아님: 인사, 부름말, 잡담, 화면이나 사용법 문의, 앞 답의 길이·말투·형식만 바꾸라는 요청. JSON {\"legal\": true} 또는 {\"legal\": false} 하나만 낸다.";
const DEADLINE: Duration = Duration::from_secs(20);
const MAX_TOKENS: u32 = 16;

#[derive(Debug, Deserialize)]
struct Judged {
    legal: bool,
}

pub fn prompt(question: &str, has_docs: bool, history: &[String], last_answer: Option<&str>) -> String {
    let docs = if has_docs { "이 대화에는 올린 문서가 있다." } else { "이 대화에는 올린 문서가 없다." };
    let mut out = docs.to_string();
    if !history.is_empty() {
        out.push_str("\n앞선 질문:");
        for q in history {
            out.push_str(&format!("\n- {q}"));
        }
    }
    if let Some(answer) = last_answer {
        out.push_str(&format!("\n직전 답 요지: {answer}"));
    }
    out.push_str(&format!("\n이번 질문: {question}"));
    out
}

pub async fn is_legal(llm: &LlmClient, question: &str, has_docs: bool, history: &[String], last_answer: Option<&str>) -> Option<bool> {
    let opts = CallOptions { max_tokens: MAX_TOKENS, deadline: DEADLINE, json_mode: true, ..CallOptions::default() };
    match llm.complete_json::<Judged>(SYSTEM, &prompt(question, has_docs, history, last_answer), &opts).await {
        Ok(j) => Some(j.legal),
        Err(e) => {
            tracing::warn!(error = %e, "route.legal_failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::prompt;

    #[test]
    fn the_judge_sees_earlier_questions_and_the_last_answer_before_this_question() {
        let history = vec!["클럽 마약 방치 제재 근거".to_string(), "무슨 법?".to_string()];
        let want = "이 대화에는 올린 문서가 없다.\n앞선 질문:\n- 클럽 마약 방치 제재 근거\n- 무슨 법?\n직전 답 요지: 식품위생법 제75조\n이번 질문: 무슨 조례?";
        assert_eq!(prompt("무슨 조례?", false, &history, Some("식품위생법 제75조")), want);
        assert_eq!(prompt("야", true, &[], None), "이 대화에는 올린 문서가 있다.\n이번 질문: 야");
    }
}
