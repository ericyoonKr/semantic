use std::io::BufRead;

use crate::error::ParseError;
use crate::xml::{Guarded, Node, XmlReader};

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct ParaProps {
    pub(super) outline: Option<u8>,
    pub(super) num_id: Option<u32>,
    pub(super) ilvl: Option<u8>,
    pub(super) break_before: Option<bool>,
}

impl ParaProps {
    pub(super) fn or(self, base: &ParaProps) -> ParaProps {
        ParaProps {
            outline: self.outline.or(base.outline),
            num_id: self.num_id.or(base.num_id),
            ilvl: self.ilvl.or(base.ilvl),
            break_before: self.break_before.or(base.break_before),
        }
    }
}

const SKIP_IN_PPR: [&str; 3] = ["w:pPrChange", "w:rPr", "w:sectPr"];

pub(super) fn read_ppr<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    props: &mut ParaProps,
    style: &mut Option<String>,
) -> Result<(), ParseError> {
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                name if SKIP_IN_PPR.contains(&name) => xml.skip()?,
                "w:pStyle" => *style = tag.attr("w:val").map(str::to_string),
                "w:outlineLvl" => props.outline = tag.attr_u32("w:val").map(clamp_level),
                "w:numId" => props.num_id = tag.attr_u32("w:val"),
                "w:ilvl" => props.ilvl = tag.attr_u32("w:val").map(clamp_level),
                "w:pageBreakBefore" => props.break_before = Some(tag.on("w:val")),
                _ => {}
            },
            Node::Close(name) if name == "w:pPr" => return Ok(()),
            Node::Close(_) | Node::Text(_) => {}
            Node::Eof => return Err(ParseError::Corrupted),
        }
    }
}

pub(super) fn clamp_level(v: u32) -> u8 {
    v.min(9) as u8
}
