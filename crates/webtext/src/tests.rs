use super::{Marked, Piece, mark_spans, parse_box_table, plain_math, split_body};

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
