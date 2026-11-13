use super::{Doc, EXCERPT_HEAD, Step, StepLog, StepRequest, WHOLE_READ_LINES, keyword, law_lines};

fn doc(id: &str, chars: u64, at: u64, text: &str) -> Doc {
    Doc { file_id: id.into(), chars, uploaded_at: at, text: text.into() }
}

fn ask(id: &str, q: &str) -> StepRequest {
    StepRequest { request_id: id.into(), question: q.into(), answer: None }
}

#[test]
fn small_documents_are_read_large_ones_searched_then_research_once() {
    let log = StepLog::default();
    let docs = [doc("big", 50_000, 1, "본문"), doc("small", 3_000, 2, "감사 결과\n「지방계약법」 제16조에 따라 검사한다.")];
    let q = ask("r1", "낙찰률은 무엇인가?");
    assert_eq!(log.next("t", &q, true, &docs), Step::ReadFile { file_path: "/uploads/small.md".into(), limit: WHOLE_READ_LINES });
    assert_eq!(
        log.next("t", &q, true, &docs),
        Step::Grep { pattern: "낙찰률".into(), path: "/uploads/big.md".into(), output_mode: "content".into() }
    );
    let want = format!("쟁점: 낙찰률은 무엇인가?\n목적: 조회\n{EXCERPT_HEAD}\n감사 결과 「지방계약법」 제16조에 따라 검사한다.");
    assert_eq!(log.next("t", &q, true, &docs), Step::Task { description: want, subagent_type: "researcher".into() });
    assert_eq!(log.next("t", &q, true, &docs), Step::Model);
    assert!(matches!(log.next("t", &ask("r2", "과징금은?"), true, &docs), Step::Task { .. }));
}

#[test]
fn an_answer_citing_unread_articles_is_sent_back_to_read_them_once() {
    let log = StepLog::default();
    let q = ask("r1", "지방공무원법 해당 됨?");
    let unread = vec!["지방공무원법 제48조".to_string(), "지방공무원법 제55조".to_string()];
    let want = "쟁점: 지방공무원법 해당 됨?\n목적: 조회\n먼저 읽을 조문: 지방공무원법 제48조, 지방공무원법 제55조";
    assert_eq!(log.after_answer("t", &q, &unread), Step::Task { description: want.into(), subagent_type: "researcher".into() });
    assert_eq!(log.after_answer("t", &q, &unread), Step::Model);
    assert_eq!(log.after_answer("t", &ask("r2", "또?"), &[]), Step::Model);
    assert!(matches!(log.after_answer("t", &ask("r3", "또?"), &unread), Step::Task { .. }));
}

#[test]
fn chat_and_non_legal_questions_go_to_the_model() {
    let log = StepLog::default();
    assert_eq!(log.next("t", &ask("r1", "야"), true, &[]), Step::Model);
    assert_eq!(log.next("t", &ask("r2", "고마워요 정말"), false, &[]), Step::Model);
    assert_eq!(keyword("영업정지 처분은 무엇인가?").as_deref(), Some("영업정지"));
    assert_eq!(keyword("응?"), None);
    assert!(log.new_turn("t", "r3") && !log.new_turn("t", "r2"));
}

#[test]
fn law_lines_keep_only_sentences_that_name_laws_even_across_pdf_line_breaks() {
    let text = "사진을 첨부하였다. 「지방자치단체를 당사자로 하는 계약에 관한 법률」(이하 ‘지방계약법’이\n라 한다) 제16조에 따라 검사하여야 한다. 별도 의견은 없다.";
    let docs = [doc("a", 10, 1, text)];
    assert_eq!(
        law_lines(&docs),
        "「지방자치단체를 당사자로 하는 계약에 관한 법률」(이하 ‘지방계약법’이 라 한다) 제16조에 따라 검사하여야 한다."
    );
}
