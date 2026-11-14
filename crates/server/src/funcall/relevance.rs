use std::time::Duration;

use llm::{CallOptions, LlmClient};
use serde::{Deserialize, Serialize};

pub const SYSTEM: &str = "법률 쟁점과 근거 원문 목록을 받아, 각 원문이 그 쟁점에 답하는지(쟁점의 요건·효과·절차·대상을 직접 정하거나 판단하는지) 판정한다. 쟁점과 무관한 일반 조문, 다른 사안의 판례, 절차만 적은 규정은 아님이다. JSON {\"keep\": [맞는 원문의 번호]} 하나만 낸다.";
const DEADLINE: Duration = Duration::from_secs(20);
const MAX_TOKENS: u32 = 64;
pub const MAX_ITEMS: usize = 12;
pub const ITEM_CHARS: usize = 800;

#[derive(Debug, Deserialize)]
pub struct JudgeItem {
    pub r#ref: u32,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct JudgeRequest {
    pub issue: String,
    pub items: Vec<JudgeItem>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct JudgeReply {
    pub keep: Vec<u32>,
    pub judged: bool,
}

#[derive(Debug, Deserialize)]
struct Kept {
    keep: Vec<u32>,
}

fn clip(text: &str, max: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

pub fn prompt(req: &JudgeRequest) -> String {
    let mut out = format!("쟁점: {}\n근거 원문:", clip(&req.issue, ITEM_CHARS));
    for item in req.items.iter().take(MAX_ITEMS) {
        out.push_str(&format!("\n[{}] {}: {}", item.r#ref, clip(&item.label, 80), clip(&item.text, ITEM_CHARS)));
    }
    out
}

pub fn unjudged(req: &JudgeRequest) -> JudgeReply {
    JudgeReply { keep: req.items.iter().map(|i| i.r#ref).collect(), judged: false }
}

pub async fn judge(llm: &LlmClient, req: &JudgeRequest) -> JudgeReply {
    if req.items.is_empty() {
        return JudgeReply { keep: Vec::new(), judged: true };
    }
    let opts = CallOptions { max_tokens: MAX_TOKENS, deadline: DEADLINE, json_mode: true, ..CallOptions::default() };
    match llm.complete_json::<Kept>(SYSTEM, &prompt(req), &opts).await {
        Ok(kept) => JudgeReply { keep: req.items.iter().map(|i| i.r#ref).filter(|r| kept.keep.contains(r)).collect(), judged: true },
        Err(error) => {
            tracing::warn!(%error, "relevance.judge_failed");
            unjudged(req)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JudgeItem, JudgeRequest, prompt, unjudged};

    #[test]
    fn the_prompt_numbers_each_source_after_the_issue() {
        let req = JudgeRequest {
            issue: "단란주점이 마약 투약을 방치했을 때의 제재 근거".into(),
            items: vec![
                JudgeItem { r#ref: 1, label: "식품위생법 제75조".into(), text: "① 영업정지를\n명할 수 있다.".into() },
                JudgeItem { r#ref: 2, label: "지방자치법 제28조".into(), text: "조례를 제정할 수 있다.".into() },
            ],
        };
        assert_eq!(
            prompt(&req),
            "쟁점: 단란주점이 마약 투약을 방치했을 때의 제재 근거\n근거 원문:\n[1] 식품위생법 제75조: ① 영업정지를 명할 수 있다.\n[2] 지방자치법 제28조: 조례를 제정할 수 있다."
        );
        assert_eq!(unjudged(&req).keep, vec![1, 2]);
        assert!(!unjudged(&req).judged);
    }
}
