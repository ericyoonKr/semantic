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
    let want = format!("쟁점: 낙찰률은 무엇인가?\n목적: 조회\n{EXCERPT_HEAD}\n감사 결과 「지방계약법」 제16조에 따라 검사한다.\n먼저 읽을 조문: 지방계약법 제16조");
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
    assert_eq!(log.after_answer("t", &q, &unread, &[], 0, &no_ctx()), Step::Task { description: want.into(), subagent_type: "researcher".into() });
    assert_eq!(log.after_answer("t", &q, &unread, &[], 0, &no_ctx()), Step::Model);
    assert_eq!(log.after_answer("t", &ask("r2", "또?"), &[], &[], 0, &no_ctx()), Step::Model);
    assert!(matches!(log.after_answer("t", &ask("r3", "또?"), &unread, &[], 0, &no_ctx()), Step::Task { .. }));
    let uncited = vec!["총 12건을 목적 외로 집행하여 손해를 발생시켰습니다.".to_string()];
    let plain = log.after_answer("t", &ask("r4", "개요?"), &[], &uncited, 0, &no_ctx());
    assert!(matches!(&plain, Step::Task { description, .. } if description.contains(super::UNCITED_HEAD) && !description.contains(super::DOC_CITE_LINE)));
    let with_docs = StepLog::default();
    let docs = [doc("small", 3_000, 2, "감사 결과\n「지방계약법」 제16조에 따라 검사한다.")];
    assert!(matches!(with_docs.next("d", &ask("r5", "이 사건의 개요 뭐야"), true, &docs, &[], &no_ctx()), Step::ReadFile { .. }));
    let paged = with_docs.after_answer("d", &ask("r5", "이 사건의 개요 뭐야"), &[], &uncited, 0, &no_ctx());
    assert!(matches!(&paged, Step::Task { description, .. } if description.ends_with(super::DOC_CITE_LINE)));
    assert_eq!(super::doc_units(&docs), vec!["지방계약법 제16조".to_string()]);
}

#[test]
fn an_answer_with_no_citation_but_legal_claims_is_sent_back_once_with_the_claims() {
    let log = StepLog::default();
    let claims = vec!["식품위생법에 따라 영업정지 처분을 받을 수 있습니다.".to_string(), "그런 조례는 존재하지 않습니다.".to_string()];
    let want = format!("쟁점: 무슨 법?\n목적: 조회\n{}\n- 식품위생법에 따라 영업정지 처분을 받을 수 있습니다.\n- 그런 조례는 존재하지 않습니다.", super::UNCITED_HEAD);
    assert_eq!(log.after_answer("t", &ask("r1", "무슨 법?"), &[], &claims, 0, &no_ctx()), Step::Task { description: want, subagent_type: "researcher".into() });
    assert_eq!(log.flagged("t"), claims.iter().map(|c| citegate::compact(c)).collect::<Vec<_>>());
    assert_eq!(log.after_answer("t", &ask("r1", "무슨 법?"), &[], &claims, 0, &no_ctx()), Step::Model);
    assert!(matches!(log.after_answer("t", &ask("r2", "무슨 법?"), &[], &claims, 2, &no_ctx()), Step::Task { .. }));
    assert_eq!(log.after_answer("t", &ask("r3", "무슨 법?"), &[], &[], 0, &no_ctx()), Step::Model);
    assert!(log.flagged("t").is_empty());
    let both = super::reread_brief("무슨 법?", &["식품위생법 제75조".to_string()], &claims[..1], &no_ctx(), false);
    assert!(both.contains("먼저 읽을 조문: 식품위생법 제75조\n근거 없이 쓴 문장"), "{both}");
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
    assert!(log.is_legal("t") && log.is_legal("unknown"));
    assert_eq!(log.next("t", &ask("r2", "고마워요 정말"), false, &[], &[], &no_ctx()), Step::Model);
    assert!(!log.is_legal("t"));
    assert_eq!(log.count_cites("t", 2), 2);
    assert_eq!(log.count_cites("t", 1), 3);
    assert_eq!(keyword("영업정지 처분은 무엇인가?").as_deref(), Some("영업정지"));
    assert_eq!(keyword("응?"), None);
    assert!(log.new_turn("t", "r3") && !log.new_turn("t", "r2"));
    log.next("t", &ask("r3", "영업정지 처분은 무엇인가?"), true, &[], &[], &no_ctx());
    assert!(log.is_legal("t") && log.count_cites("t", 0) == 0);
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
fn the_reply_carries_the_step_and_the_notes_side_by_side() {
    let reply = super::StepReply { step: Step::Model, notes: vec!["조례는 자료에 없음".into()], guidance: vec![] };
    assert_eq!(serde_json::to_value(&reply).ok(), serde_json::json!({"tool": "model", "notes": ["조례는 자료에 없음"], "guidance": []}).into());
    let task = super::StepReply {
        step: Step::Task { description: "쟁점: x".into(), subagent_type: "researcher".into() },
        notes: vec![],
        guidance: vec![norms::RULE_WRITING_NOTE.into()],
    };
    assert_eq!(serde_json::to_value(&task).unwrap_or_default()["guidance"][0], norms::RULE_WRITING_NOTE);
    let value = serde_json::to_value(&task).unwrap_or_default();
    assert_eq!(value["tool"], "task");
    assert_eq!(value["args"]["subagent_type"], "researcher");
    assert_eq!(value["notes"], serde_json::json!([]));
    let ctx = Context { scope: vec!["자료 범위: 자치법규는 자료에 없음".into()], ..Context::default() };
    assert_eq!(brief("무슨 조례?", &[], "", &ctx), "쟁점: 무슨 조례?\n목적: 조회\n자료 범위: 자치법규는 자료에 없음");
}

#[test]
fn questions_naming_law_words_are_legal_without_asking_the_model() {
    for q in ["무슨 법?", "무슨 조례?", "아니 시발 무슨 조례", "판례는?", "근거가 뭐야", "지방공무원법 해당 됨?", "어떤 법이 적용돼?", "이 법은?", "학칙에 휴학 규정 있어?", "시행세칙은?"] {
        assert!(asks_for_law(q), "{q}");
    }
    for q in ["고마워요 정말", "더 짧게 써 줘", "방법 알려줘", "강남구 단란주점", "표로 정리해", "김밥 맛있게 싸는 법 알려줘", "파이썬으로 리스트 정렬하는 법"] {
        assert!(!asks_for_law(q), "{q}");
    }
    for q in ["표로 정리해", "영어로", "다시 말해줘", "한 문장으로 요약해 줘", "존댓말로 다시 써 줘", "더 짧게"] {
        assert!(super::asks_for_format(q), "{q}");
    }
    for q in ["무슨 법?", "강남구 단란주점", "과태료 얼마야?"] {
        assert!(!super::asks_for_format(q), "{q}");
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

#[test]
fn question_laws_resolve_registered_abbreviations_across_words() -> Result<(), lawnames::Error> {
    let entries = vec![
        lawnames::Entry { alias: "단국대 학칙".into(), law_id: "rule:dku:hakchik".into(), name: "단국대학교 학칙".into(), source: "시험".into() },
        lawnames::Entry { alias: "학칙".into(), law_id: "rule:dku:hakchik".into(), name: "단국대학교 학칙".into(), source: "시험".into() },
    ];
    let installed = lawnames::install(lawnames::Aliases::from_entries(entries)?);
    let laws = question_laws("단국대 학칙에서 휴학은 어떻게 규정돼 있어?");
    if installed || lawnames::lookup("단국대학칙").is_some() {
        assert_eq!(laws, vec!["단국대학교학칙".to_string()]);
    }
    assert_eq!(question_laws("휴학 절차가 궁금해"), Vec::<String>::new());
    assert_eq!(question_laws("지방공무원법 해당 됨?"), vec!["지방공무원법".to_string()]);
    Ok(())
}
