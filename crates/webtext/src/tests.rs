use super::{Layered, Marked, Piece, find_all, mark_layers, mark_spans, parse_box_table, plain_math, split_body};

const BOX: [&str; 13] = [
    "┏━━━━━━━━┯━━━━━━┯━━━━━━━━━━━━━━━━━┓",
    "┃위반사항        │근거 법령   │행정처분기준                      ┃",
    "┃                │            ├─────┬─────┬─────┨",
    "┃                │            │1차 위반  │2차 위반  │3차 위반  ┃",
    "┠────────┼──────┼─────┼─────┼─────┨",
    "┃                │            │          │          │          ┃",
    "┃1. 법 제4조를   │법 제72조   │영업정지  │영업정지  │영업허가  ┃",
    "┃  가. 썩거나    │및 법 제75  │1개월과   │3개월과   │·등록취  ┃",
    "┃                │조          │          │          │소        ┃",
    "┃                │            │          │          │          ┃",
    "┃  나. 설익어서  │            │영업정지  │영업정지  │영업정지  ┃",
    "┃                │            │15일과    │1개월과   │3개월과   ┃",
    "┗━━━━━━━━┷━━━━━━┷━━━━━━━━━━━━━━━━━┛",
];

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn box_tables_split_into_header_rows_and_cells() {
    let t = parse_box_table(&BOX);
    assert_eq!(t.head, vec![strings(&["위반사항", "근거 법령", "행정처분기준"]), strings(&["", "", "1차 위반", "2차 위반", "3차 위반"])]);
    assert_eq!(t.rows.len(), 2);
    assert_eq!(
        t.rows[0],
        strings(&["1. 법 제4조를\n가. 썩거나", "법 제72조 및 법 제75조", "영업정지 1개월과", "영업정지 3개월과", "영업허가·등록취소"])
    );
    assert_eq!(t.rows[1], strings(&["나. 설익어서", "", "영업정지 15일과", "영업정지 1개월과", "영업정지 3개월과"]));
}

#[test]
fn wrapped_header_cells_are_joined_as_one_word_and_rule_marks_do_not_leak() {
    let lines = [
        "┏━━━━┯━━━━┯━━━━━━━━━━━━━━━━━━━┓",
        "┃위반사항│근거  │행정처분기준                 ┃",
        "┃        │법령  ├────┬────┬────┬────┨",
        "┃        │      │1차위│2차위│3차위│4차위┃",
        "┃        │      │반   │반   │반   │반   ┃",
        "┠────┼────┼────┼────┼────┼────┨",
        "┃마약 방치│제75조│15일│1개월│3개월│폐쇄 ┃",
        "┗━━━━┷━━━━┷━━━━━━━━━━━━━━━━━━━┛",
    ];
    let t = parse_box_table(&lines);
    assert_eq!(t.head, vec![strings(&["위반사항", "근거법령", "행정처분기준"]), strings(&["", "", "1차위반", "2차위반", "3차위반", "4차위반"])]);
    assert_eq!(t.rows, vec![strings(&["마약 방치", "제75조", "15일", "1개월", "3개월", "폐쇄"])]);
    assert!(!format!("{t:?}").contains('┬'));
}

#[test]
fn bodies_split_into_text_and_tables_keeping_character_positions() {
    let body = format!("앞 글\n{}\n뒤 글", BOX.join("\n"));
    let pieces = split_body(&body);
    let total = body.chars().count();
    let kinds: Vec<&str> = pieces.iter().map(|p| if matches!(p, Piece::Text { .. }) { "text" } else { "table" }).collect();
    assert_eq!(kinds, vec!["text", "table", "text"]);
    let bounds: Vec<(usize, usize)> = pieces.iter().map(|p| match p { Piece::Text { start, end, .. } | Piece::Table { start, end, .. } => (*start, *end) }).collect();
    assert_eq!(bounds[0].0, 0);
    assert_eq!(bounds[0].1, bounds[1].0);
    assert_eq!(bounds[1].1, bounds[2].0);
    assert_eq!(bounds[2].1, total);
    assert_eq!(pieces[2], Piece::Text { text: "뒤 글".into(), start: bounds[2].0, end: total });
    let json = serde_json::to_string(&pieces[0]).unwrap_or_default();
    assert!(json.starts_with(r#"{"kind":"text","text":"앞 글\n""#));
}

#[test]
fn marked_spans_follow_characters_not_bytes() {
    let got = mark_spans("가나다라마", &[(3, 5), (1, 2), (4, 9)]);
    let want = vec![
        Marked { text: "가".into(), marked: false },
        Marked { text: "나".into(), marked: true },
        Marked { text: "다".into(), marked: false },
        Marked { text: "라마".into(), marked: true },
    ];
    assert_eq!(got, want);
}

#[test]
fn finding_ignores_spaces_and_line_breaks_on_both_sides() {
    let body = "영업 정지 1개월\n어느 하\n  나에 해당";
    assert_eq!(find_all(body, "영업정지"), vec![(0, 5)]);
    assert_eq!(find_all(body, "하나에"), vec![(13, 19)]);
    assert_eq!(body.chars().skip(13).take(6).collect::<String>(), "하\n  나에");
    assert_eq!(find_all("영업정지", "영업 정지"), vec![(0, 4)]);
    assert_eq!(find_all("영업정지", "영업\t정지\n"), vec![(0, 4)]);
}

#[test]
fn finding_ignores_letter_case() {
    assert_eq!(find_all("Abc ABC abc", "aBC"), vec![(0, 3), (4, 7), (8, 11)]);
    assert_eq!(find_all("제Ⅰ장 제ⅰ장", "제ⅰ장"), vec![(0, 3), (4, 7)]);
}

#[test]
fn empty_or_blank_queries_find_nothing() {
    assert_eq!(find_all("아무 글", ""), Vec::<(usize, usize)>::new());
    assert_eq!(find_all("아무 글", " \n\t"), Vec::<(usize, usize)>::new());
    assert_eq!(find_all("", "가"), Vec::<(usize, usize)>::new());
}

#[test]
fn matches_do_not_overlap_and_run_from_the_front() {
    assert_eq!(find_all("aaaa", "aa"), vec![(0, 2), (2, 4)]);
    assert_eq!(find_all("aaa", "aa"), vec![(0, 2)]);
    assert_eq!(find_all("가가 가가가", "가가"), vec![(0, 2), (3, 5)]);
}

fn layer(text: &str, cite: bool, find: bool, current: bool) -> Layered {
    Layered { text: text.into(), cite, find, current }
}

#[test]
fn overlapping_cite_and_find_pieces_carry_both_layers() {
    let got = mark_layers("가나다라마바", &[(1, 4)], &[(3, 5)], None);
    let want = vec![
        layer("가", false, false, false),
        layer("나다", true, false, false),
        layer("라", true, true, false),
        layer("마", false, true, false),
        layer("바", false, false, false),
    ];
    assert_eq!(got, want);
}

#[test]
fn the_current_find_is_flagged_only_at_its_index() {
    let finds = [(0, 2), (3, 5), (6, 8)];
    let got = mark_layers("ab ab ab", &[], &finds, Some(1));
    let want = vec![
        layer("ab", false, true, false),
        layer(" ", false, false, false),
        layer("ab", false, true, true),
        layer(" ", false, false, false),
        layer("ab", false, true, false),
    ];
    assert_eq!(got, want);
    assert!(mark_layers("ab ab ab", &[], &finds, Some(5)).iter().all(|l| !l.current));
    assert!(mark_layers("ab ab ab", &[], &finds, None).iter().all(|l| !l.current));
}

#[test]
fn reversed_and_out_of_range_layers_are_ignored() {
    let got = mark_layers("가나다", &[(2, 1), (0, 9), (1, 1)], &[(1, 2)], Some(0));
    let want = vec![layer("가", false, false, false), layer("나", false, true, true), layer("다", false, false, false)];
    assert_eq!(got, want);
    assert_eq!(got.iter().map(|l| l.text.as_str()).collect::<String>(), "가나다");
    assert_eq!(mark_layers("", &[(0, 1)], &[], None), Vec::<Layered>::new());
    assert_eq!(mark_layers("가나", &[], &[], None), vec![layer("가나", false, false, false)]);
}

#[test]
fn math_commands_become_symbols_and_unknown_ones_stay() {
    assert_eq!(plain_math("영업정지 $\\rightarrow$ 시·군·구청"), "영업정지 → 시·군·구청");
    assert_eq!(plain_math("A \\Rightarrow B, 3 $ \\times $ 2"), "A ⇒ B, 3 × 2");
    assert_eq!(plain_math("$\\alpha$ 그대로"), "$\\alpha$ 그대로");
    assert_eq!(plain_math("\\toc 는 바꾸지 않음"), "\\toc 는 바꾸지 않음");
}

#[test]
fn light_box_tables_take_the_first_block_as_the_head_and_keep_merged_cells() {
    let lines = [
        "┌────────┬──────┬──────┐",
        "│비위의 유형     │고의        │경과실      │",
        "├────────┼──────┼──────┤",
        "│1. 성실의무 위반│            │            │",
        "│                │            │            │",
        "│  가. 회계질서  │파면        │감봉∼견책  │",
        "│문란            │            │            │",
        "│                │                         │",
        "│  나. 수당 부정 │별표 1의2와 같음         │",
        "│수급            │                         │",
        "├────────┼──────┴──────┤",
        "│2. 청렴의무 위반│별표 2와 같음            │",
        "└────────┴─────────────┘",
    ];
    let t = parse_box_table(&lines);
    assert_eq!(t.head, vec![vec!["비위의 유형", "고의", "경과실"]]);
    assert_eq!(
        t.rows,
        vec![
            vec!["1. 성실의무 위반", "", ""],
            vec!["가. 회계질서문란", "파면", "감봉∼견책"],
            vec!["나. 수당 부정수급", "별표 1의2와 같음"],
            vec!["2. 청렴의무 위반", "별표 2와 같음"],
        ]
    );
    let body = format!("앞 글\n{}\n뒤 글", lines.join("\n"));
    let kinds: Vec<&str> = split_body(&body).iter().map(|p| if matches!(p, Piece::Table { .. }) { "table" } else { "text" }).collect();
    assert_eq!(kinds, vec!["text", "table", "text"]);
}
