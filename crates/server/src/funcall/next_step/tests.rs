use super::{
    Context, Doc, EXCERPT_HEAD, QUESTION_LAWS_HEAD, Step, StepLog, StepRequest, WHOLE_READ_LINES, asks_for_law, brief, keyword, law_lines,
    question_laws,
};

fn doc(id: &str, chars: u64, at: u64, text: &str) -> Doc {
    Doc { file_id: id.into(), filename: format!("{id}.pdf"), chars, uploaded_at: at, processing: false, text: text.into() }
}

fn ask(id: &str, q: &str) -> StepRequest {
    StepRequest { request_id: id.into(), question: q.into(), answer: None, history: Vec::new(), last_answer: None }
}

fn no_ctx() -> Context {
    Context::default()
}

#[test]
fn small_documents_are_read_large_ones_searched_then_research_once() {
    let log = StepLog::default();
    let docs = [doc("big", 50_000, 1, "본문"), doc("small", 3_000, 2, "감사 결과\n「지방계약법」 제16조에 따라 검사한다.")];
    let q = ask("r1", "낙찰률은 무엇인가?");
    assert_eq!(log.next("t", &q, true, &docs, &[], &no_ctx()), Step::ReadFile { file_path: "/uploads/small.md".into(), limit: WHOLE_READ_LINES });
    assert_eq!(log.next("t", &q, true, &docs, &[], &no_ctx()), Step::SearchDocument { query: "낙찰률은 무엇인가?".into() });
    let want = format!("쟁점: 낙찰률은 무엇인가?\n목적: 조회\n{EXCERPT_HEAD}\n감사 결과 「지방계약법」 제16조에 따라 검사한다.");
    assert_eq!(log.next("t", &q, true, &docs, &[], &no_ctx()), Step::Task { description: want, subagent_type: "researcher".into() });
    assert_eq!(log.next("t", &q, true, &docs, &[], &no_ctx()), Step::Model);
    assert!(matches!(log.next("t", &ask("r2", "과징금은?"), true, &docs, &[], &no_ctx()), Step::Task { .. }));
}

#[test]
fn an_answer_citing_unread_articles_is_sent_back_to_read_them_once() {
    let log = StepLog::default();
    let q = ask("r1", "지방공무원법 해당 됨?");
    let unread = vec!["지방공무원법 제48조".to_string(), "지방공무원법 제55조".to_string()];
    let want = "쟁점: 지방공무원법 해당 됨?\n목적: 조회\n먼저 읽을 조문: 지방공무원법 제48조, 지방공무원법 제55조";
    assert_eq!(log.after_answer("t", &q, &unread, &no_ctx()), Step::Task { description: want.into(), subagent_type: "researcher".into() });
    assert_eq!(log.after_answer("t", &q, &unread, &no_ctx()), Step::Model);
    assert_eq!(log.after_answer("t", &ask("r2", "또?"), &[], &no_ctx()), Step::Model);
    assert!(matches!(log.after_answer("t", &ask("r3", "또?"), &unread, &no_ctx()), Step::Task { .. }));
}

#[test]
fn laws_named_in_the_question_are_found_and_put_in_the_brief() {
    assert_eq!(question_laws("지방공무원법 해당 됨?"), vec!["지방공무원법".to_string()]);
    assert_eq!(question_laws("「공직자의 이해충돌 방지법」 제5조와 식품위생법상 처분은?"), vec!["공직자의이해충돌방지법".to_string(), "식품위생법".to_string()]);
    assert!(question_laws("처리 방법 알려줘").is_empty());
    let log = StepLog::default();
    let laws = vec!["지방공무원법".to_string()];
    let want = format!("쟁점: 지방공무원법 해당 됨?\n목적: 조회\n{QUESTION_LAWS_HEAD} 지방공무원법");
    assert_eq!(log.next("t", &ask("r1", "지방공무원법 해당 됨?"), true, &[], &laws, &no_ctx()), Step::Task { description: want, subagent_type: "researcher".into() });
}

#[test]
fn chat_and_non_legal_questions_go_to_the_model() {
    let log = StepLog::default();
    assert_eq!(log.next("t", &ask("r1", "야"), true, &[], &[], &no_ctx()), Step::Model);
    assert_eq!(log.next("t", &ask("r2", "고마워요 정말"), false, &[], &[], &no_ctx()), Step::Model);
    assert_eq!(keyword("영업정지 처분은 무엇인가?").as_deref(), Some("영업정지"));
    assert_eq!(keyword("응?"), None);
    assert!(log.new_turn("t", "r3") && !log.new_turn("t", "r2"));
}

#[test]
fn earlier_questions_the_last_answer_and_read_items_go_into_the_brief() {
    let mut reads = citegate::ReadSet::default();
    reads.add_unit("식품위생법", "제75조");
    reads.add_case("2024두35989");
    let req = StepRequest {
        history: vec!["클럽에서 마약 투약을 방치한 업소 제재 근거".into(), "무슨 법?".into()],
        last_answer: Some("식품위생법 제75조에 따라\n영업정지 처분을 할 수 있습니다.".into()),
        ..ask("r1", "무슨 조례?")
    };
    let ctx = Context::new(&req, &reads);
    assert_eq!(ctx.read, vec!["식품위생법 제75조".to_string(), "2024두35989".to_string()]);
    let want = "쟁점: 무슨 조례?\n목적: 조회\n앞선 질문:\n- 클럽에서 마약 투약을 방치한 업소 제재 근거\n- 무슨 법?\n직전 답 요지: 식품위생법 제75조에 따라 영업정지 처분을 할 수 있습니다.\n이 대화에서 이미 읽은 근거: 식품위생법 제75조, 2024두35989";
    assert_eq!(brief("무슨 조례?", &[], "", &ctx), want);
    let long = StepRequest { history: (0..8).map(|i| format!("질문{i}")).collect(), last_answer: Some("가".repeat(2000)), ..ask("r2", "또?") };
    let ctx = Context::new(&long, &citegate::ReadSet::default());
    assert_eq!(ctx.history, (3..8).map(|i| format!("질문{i}")).collect::<Vec<_>>());
    assert_eq!(ctx.last_answer.as_ref().map(|a| a.chars().count()), Some(1500));
    assert_eq!(Context::new(&ask("r3", "야"), &citegate::ReadSet::default()), Context::default());
}

#[test]
fn questions_naming_law_words_are_legal_without_asking_the_model() {
    for q in ["무슨 법?", "무슨 조례?", "아니 시발 무슨 조례", "판례는?", "근거가 뭐야", "지방공무원법 해당 됨?"] {
        assert!(asks_for_law(q), "{q}");
    }
    for q in ["고마워요 정말", "더 짧게 써 줘", "방법 알려줘", "강남구 단란주점", "표로 정리해"] {
        assert!(!asks_for_law(q), "{q}");
    }
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
