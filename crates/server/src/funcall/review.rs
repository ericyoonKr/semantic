use serde::{Deserialize, Serialize};

use super::check::{UnreadRequest, unread_in_corpus};
use super::coverage::{CASE, LAW, NAMED_LAW, missing};
use crate::state::AppState;

pub const NO_SEARCH: &str = "no_search";
pub const MUST_READ: &str = "must_read";
pub const COVERAGE: &str = "coverage";
pub const ROUNDS_PER_KIND: u32 = 1;

pub const NO_SEARCH_NOTE: &str = "앞서 검색도 읽기도 하지 않고 결과를 돌려줬다. 쟁점의 낱말로 search_laws와 search_cases를 지금 실제로 부르고, 맞는 조문은 read_law로, 판례는 read_case로 읽은 뒤 정해진 JSON 형식으로 돌려준다.";
pub const MUST_READ_NOTE: &str = "올린 문서에 적힌 다음 조문·판례를 아직 읽지 않았다. 각각 read_law 또는 read_case로 지금 읽고, 읽은 근거를 넣어 정해진 JSON 형식으로 다시 돌려준다:";
pub const COVERAGE_NOTE: &str = "이번 조사에서 아직 읽지 않은 근거 종류가 있다. 다음을 지금 하고, 읽은 근거를 넣어 정해진 JSON 형식으로 다시 돌려준다. 찾아도 맞는 것이 없으면 not_found에 적는다:";
pub const COVERAGE_LAW: &str = "search_laws로 쟁점의 의무·요건·효과 조문을 찾아 맞는 조문을 3개 이상 read_law로 읽는다.";
pub const COVERAGE_CASE: &str =
    "search_cases로 쟁점의 판례를 찾아 맞는 판례 1~2건을 read_case(decision_id, sections=[\"holding_issue\", \"holding_summary\"])로 읽는다.";

pub fn named_law_step(law: &str) -> String {
    format!("질문에 나온 {law}에서 쟁점의 의무·요건·효과 조문을 search_laws로 찾아 read_law로 읽는다.")
}

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub request_id: String,
    pub thread_id: String,
    #[serde(default)]
    pub searched: bool,
    #[serde(default)]
    pub doc_lines: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReviewReply {
    pub note: Option<String>,
}

pub fn coverage_note(lack: &[String]) -> String {
    let steps = lack.iter().map(|kind| match kind.as_str() {
        LAW => COVERAGE_LAW.to_string(),
        CASE => COVERAGE_CASE.to_string(),
        other => named_law_step(other.strip_prefix(NAMED_LAW).unwrap_or(other)),
    });
    std::iter::once(COVERAGE_NOTE.to_string()).chain(steps).collect::<Vec<_>>().join("\n")
}

pub async fn review(state: &AppState, req: &ReviewRequest) -> ReviewReply {
    let request = req.request_id.trim();
    if !req.searched && state.coverage.round(request, NO_SEARCH) <= ROUNDS_PER_KIND {
        return ReviewReply { note: Some(NO_SEARCH_NOTE.to_string()) };
    }
    if !req.doc_lines.is_empty() {
        let ask = UnreadRequest { thread_id: req.thread_id.clone(), text: req.doc_lines.join("\n") };
        let unread = unread_in_corpus(state, &ask).await.unread;
        if !unread.is_empty() && state.coverage.round(request, MUST_READ) <= ROUNDS_PER_KIND {
            return ReviewReply { note: Some(format!("{MUST_READ_NOTE} {}", unread.join(", "))) };
        }
    }
    let lack = missing(&state.coverage.tally(request));
    if !lack.is_empty() && state.coverage.round(request, COVERAGE) <= ROUNDS_PER_KIND {
        return ReviewReply { note: Some(coverage_note(&lack)) };
    }
    ReviewReply { note: None }
}

#[cfg(test)]
mod tests {
    use super::{COVERAGE_CASE, COVERAGE_LAW, COVERAGE_NOTE, coverage_note, named_law_step};
    use crate::funcall::coverage::Coverage;

    #[test]
    fn each_kind_of_send_back_happens_once_per_question() {
        let c = Coverage::default();
        assert_eq!(c.round("r1", "no_search"), 1);
        assert_eq!(c.round("r1", "no_search"), 2);
        assert_eq!(c.round("r1", "coverage"), 1);
        assert_eq!(c.round("r2", "no_search"), 1);
        assert_eq!(c.tally("r1").rounds, vec![("no_search".to_string(), 2), ("coverage".to_string(), 1)]);
    }

    #[test]
    fn the_coverage_note_lists_a_step_for_each_missing_kind() {
        let note = coverage_note(&["law".to_string(), "case".to_string(), "law:지방공무원법".to_string()]);
        assert_eq!(note, format!("{COVERAGE_NOTE}\n{COVERAGE_LAW}\n{COVERAGE_CASE}\n{}", named_law_step("지방공무원법")));
    }
}
