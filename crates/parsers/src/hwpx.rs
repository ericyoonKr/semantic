mod section;

use docmodel::{Document, FactSource, FileId};

use crate::error::ParseError;
use crate::guard::{Deadline, GuardLimits, Package};
use crate::xml::XmlReader;

const SECTION_PREFIX: &str = "Contents/section";
const SECTION_SUFFIX: &str = ".xml";

pub(crate) fn parse_hwpx(
    bytes: &[u8],
    limits: &GuardLimits,
    source: FileId,
) -> Result<Document, ParseError> {
    let deadline = Deadline::start(limits);
    let mut package = Package::open(bytes, limits)?;
    let mut sections: Vec<(u32, String)> = package
        .names()
        .into_iter()
        .filter_map(|name| {
            let n = name.strip_prefix(SECTION_PREFIX)?.strip_suffix(SECTION_SUFFIX)?.parse().ok()?;
            Some((n, name))
        })
        .collect();
    sections.sort();
    if sections.is_empty() {
        return Err(ParseError::Corrupted);
    }
    let fact = FactSource::File(source.clone());
    let mut blocks = Vec::new();
    for (index, (_, name)) in sections.iter().enumerate() {
        let member = package.member(name)?.ok_or(ParseError::Corrupted)?;
        let mut xml = XmlReader::new(member, deadline);
        blocks.extend(section::read_section(&mut xml, &fact, index)?);
    }
    Ok(Document { blocks, source, warnings: Vec::new() })
}

#[cfg(test)]
mod tests {
    use docmodel::Block;

    use crate::guard::GuardLimits;
    use crate::parse;
    use crate::testzip::build_zip;

    const NS: &str = "xmlns:hs=\"urn:s\" xmlns:hp=\"urn:p\"";

    fn p(text: &str) -> String {
        format!("<hp:p><hp:run><hp:t>{text}</hp:t></hp:run></hp:p>")
    }

    fn tc(row: u32, col: u32, rs: u32, cs: u32, text: &str) -> String {
        format!(
            "<hp:tc><hp:subList>{}</hp:subList><hp:cellAddr colAddr=\"{col}\" rowAddr=\"{row}\"/>\
             <hp:cellSpan colSpan=\"{cs}\" rowSpan=\"{rs}\"/></hp:tc>",
            p(text)
        )
    }

    #[test]
    fn paragraphs_and_cell_spans() {
        let table = format!(
            "<hp:p><hp:run><hp:tbl rowCnt=\"2\" colCnt=\"2\"><hp:tr>{}{}</hp:tr><hp:tr>{}</hp:tr></hp:tbl></hp:run></hp:p>",
            tc(0, 0, 2, 1, "가"),
            tc(0, 1, 1, 1, "나"),
            tc(1, 1, 1, 1, "다"),
        );
        let section = format!("<hs:sec {NS}>{}{table}</hs:sec>", p("제1조(목적) 글"));
        let bytes = build_zip(&[
            ("mimetype", b"application/hwp+zip".as_slice()),
            ("Contents/section0.xml", section.as_bytes()),
        ]);
        let doc = parse(&bytes, &GuardLimits::default()).expect("parse");
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Para { text, anchor } => {
                assert_eq!(text, "제1조(목적) 글");
                assert_eq!(anchor.path.as_deref(), Some("section[0]/p[0]"));
            }
            other => panic!("{other:?}"),
        }
        let Block::Table(t) = &doc.blocks[1] else { panic!("{:?}", doc.blocks[1]) };
        let shape: Vec<_> = t
            .cells
            .iter()
            .map(|c| (c.row, c.col, c.rowspan, c.colspan, c.text.as_str()))
            .collect();
        assert_eq!(shape, vec![(0, 0, 2, 1, "가"), (0, 1, 1, 1, "나"), (1, 1, 1, 1, "다")]);
        assert_eq!(t.anchor.path.as_deref(), Some("section[0]/p[1]/tbl[0]"));
    }
}
