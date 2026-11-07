use docmodel::{Block, Cell, Document};

use crate::error::ParseError;
use crate::guard::GuardLimits;
use crate::parse;
use crate::testzip::{W_NS, build_zip, document_xml, para};

const MAIN: &str = "word/document.xml";

fn styles_xml() -> String {
    format!(
        "<w:styles {W_NS}><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
         <w:style w:type=\"paragraph\" w:styleId=\"T1\"><w:name w:val=\"x\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr></w:style></w:styles>"
    )
}

fn tc(props: &str, text: &str) -> String {
    format!("<w:tc><w:tcPr><w:tcW w:w=\"100\" w:type=\"dxa\"/>{props}</w:tcPr>{}</w:tc>", para(text))
}

fn parse_parts(parts: &[(&str, &str)]) -> Result<Document, ParseError> {
    let entries: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (*n, d.as_bytes())).collect();
    parse(&build_zip(&entries), &GuardLimits::default())
}

fn texts(doc: &Document) -> Vec<String> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { text, .. } | Block::Para { text, .. } => Some(text.clone()),
            Block::Table(_) => None,
        })
        .collect()
}

fn cell_shape(c: &Cell) -> (u32, u32, u32, u32, &str, Option<&str>) {
    (c.row, c.col, c.rowspan, c.colspan, c.text.as_str(), c.anchor.path.as_deref())
}

#[test]
fn heading_para_and_merged_table() {
    let table = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid>\
         <w:tr>{}{}</w:tr><w:tr>{}{}{}</w:tr></w:tbl>",
        tc("<w:gridSpan w:val=\"2\"/>", "가"),
        tc("<w:vMerge w:val=\"restart\"/>", "나"),
        tc("", "다"),
        tc("", "라"),
        tc("<w:vMerge/>", ""),
    );
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"T1\"/></w:pPr><w:r><w:t>개요</w:t></w:r></w:p>{}{table}\
         <w:p><w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr><w:r><w:t>세목</w:t></w:r></w:p>",
        para("본문")
    );
    let doc = parse_parts(&[(MAIN, &document_xml(&body)), ("word/styles.xml", &styles_xml())])
        .expect("parse");
    assert_eq!(doc.blocks.len(), 4);
    match &doc.blocks[0] {
        Block::Heading { level, text, anchor } => {
            assert_eq!((*level, text.as_str()), (1, "개요"));
            assert_eq!(anchor.path.as_deref(), Some("body/p[0]"));
            assert_eq!(anchor.page, None);
        }
        other => panic!("{other:?}"),
    }
    match &doc.blocks[1] {
        Block::Para { text, anchor } => {
            assert_eq!(text, "본문");
            assert_eq!(anchor.path.as_deref(), Some("body/p[1]"));
        }
        other => panic!("{other:?}"),
    }
    let Block::Table(t) = &doc.blocks[2] else { panic!("{:?}", doc.blocks[2]) };
    assert_eq!(t.anchor.path.as_deref(), Some("tbl[0]"));
    assert!(!t.degraded);
    let shapes: Vec<_> = t.cells.iter().map(cell_shape).collect();
    assert_eq!(
        shapes,
        vec![
            (0, 0, 1, 2, "가", Some("tbl[0]/tr[0]/tc[0]")),
            (0, 2, 2, 1, "나", Some("tbl[0]/tr[0]/tc[1]")),
            (1, 0, 1, 1, "다", Some("tbl[0]/tr[1]/tc[0]")),
            (1, 1, 1, 1, "라", Some("tbl[0]/tr[1]/tc[1]")),
        ]
    );
    match &doc.blocks[3] {
        Block::Heading { level, text, anchor } => {
            assert_eq!((*level, text.as_str()), (2, "세목"));
            assert_eq!(anchor.path.as_deref(), Some("body/p[2]"));
        }
        other => panic!("{other:?}"),
    }
}

fn numbered(num: u32, ilvl: u32, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{ilvl}\"/><w:numId w:val=\"{num}\"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"
    )
}

#[test]
fn numbering_restores_decimal_and_circled_lists() {
    let numbering = format!(
        "<w:numbering {W_NS}>\
         <w:abstractNum w:abstractNumId=\"0\">\
           <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/><w:suff w:val=\"space\"/></w:lvl>\
           <w:lvl w:ilvl=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"ganada\"/><w:lvlText w:val=\"%2)\"/><w:suff w:val=\"space\"/></w:lvl>\
         </w:abstractNum>\
         <w:abstractNum w:abstractNumId=\"1\">\
           <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimalEnclosedCircle\"/><w:lvlText w:val=\"%1\"/><w:suff w:val=\"nothing\"/></w:lvl>\
         </w:abstractNum>\
         <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>\
         <w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>\
         <w:num w:numId=\"3\"><w:abstractNumId w:val=\"0\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/></w:lvlOverride></w:num>\
         </w:numbering>"
    );
    let body = [
        numbered(1, 0, "가입"),
        numbered(1, 1, "세부"),
        numbered(1, 1, "세부"),
        numbered(1, 0, "해지"),
        numbered(1, 1, "세부"),
        numbered(2, 0, "첫째"),
        numbered(2, 0, "둘째"),
        numbered(3, 0, "다시"),
    ]
    .concat();
    let doc = parse_parts(&[(MAIN, &document_xml(&body)), ("word/numbering.xml", &numbering)])
        .expect("parse");
    assert_eq!(
        texts(&doc),
        vec!["1. 가입", "가) 세부", "나) 세부", "2. 해지", "가) 세부", "①첫째", "②둘째", "1. 다시"]
    );
}

#[test]
fn explicit_breaks_set_pages_and_footnotes_follow_body() {
    let body = format!(
        "{}<w:p><w:r><w:br w:type=\"page\"/></w:r><w:r><w:lastRenderedPageBreak/><w:t>둘</w:t>\
         <w:footnoteReference w:id=\"1\"/></w:r></w:p>",
        para("하나")
    );
    let notes = format!(
        "<w:footnotes {W_NS}><w:footnote w:type=\"separator\" w:id=\"-1\">{}</w:footnote>\
         <w:footnote w:id=\"1\">{}</w:footnote></w:footnotes>",
        para("구분"),
        para("각주 글")
    );
    let doc = parse_parts(&[(MAIN, &document_xml(&body)), ("word/footnotes.xml", &notes)])
        .expect("parse");
    let shape: Vec<_> = doc
        .blocks
        .iter()
        .map(|b| match b {
            Block::Para { text, anchor } => (text.as_str(), anchor.page, anchor.path.as_deref()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            ("하나", Some(1), Some("body/p[0]")),
            ("둘", Some(2), Some("body/p[1]")),
            ("각주 글", Some(2), Some("footnote[1]")),
        ]
    );
}

#[test]
fn high_ratio_member_is_zip_bomb() {
    let zeros = vec![0u8; 4 * 1024 * 1024];
    let doc = document_xml(&para("글"));
    let bytes = build_zip(&[(MAIN, doc.as_bytes()), ("word/media/x.bin", &zeros)]);
    assert_eq!(parse(&bytes, &GuardLimits::default()), Err(ParseError::ZipBomb));
}

#[test]
fn doctype_entity_in_part_is_rejected() {
    let xml = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE d [<!ENTITY e SYSTEM \"file:///x\">]><w:document {W_NS}><w:body>{}</w:body></w:document>",
        para("글")
    );
    assert_eq!(parse_parts(&[(MAIN, &xml)]), Err(ParseError::Corrupted));
}

#[test]
fn traversal_member_name_is_rejected() {
    let doc = document_xml(&para("글"));
    let bytes = build_zip(&[(MAIN, doc.as_bytes()), ("../x.xml", b"<a/>".as_slice())]);
    assert_eq!(parse(&bytes, &GuardLimits::default()), Err(ParseError::Corrupted));
}

#[test]
fn empty_body_is_empty_text() {
    assert_eq!(parse_parts(&[(MAIN, &document_xml("<w:p/>"))]), Err(ParseError::EmptyText));
}
