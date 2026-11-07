use std::collections::{BTreeMap, HashSet};
use std::io::BufRead;

use docmodel::Block;

use super::para::read_para;
use super::state::Ctx;
use crate::error::ParseError;
use crate::xml::{Guarded, Node, XmlReader};

pub(super) fn read_footnotes<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    ctx: &mut Ctx<'_>,
) -> Result<BTreeMap<u32, String>, ParseError> {
    let mut notes = BTreeMap::new();
    loop {
        match xml.next()? {
            Node::Open(tag) if tag.name == "w:footnote" => {
                let normal = matches!(tag.attr("w:type"), None | Some("normal"));
                match tag.attr_u32("w:id").filter(|_| normal) {
                    Some(id) => {
                        let text = read_note(xml, ctx)?;
                        notes.insert(id, text);
                    }
                    None => xml.skip()?,
                }
            }
            Node::Eof => return Ok(notes),
            _ => {}
        }
    }
}

fn read_note<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    ctx: &mut Ctx<'_>,
) -> Result<String, ParseError> {
    let mut lines: Vec<String> = Vec::new();
    loop {
        match xml.next()? {
            Node::Open(tag) if tag.name == "w:p" => {
                let para = read_para(xml, ctx)?;
                if !para.text.trim().is_empty() {
                    lines.push(para.text);
                }
            }
            Node::Close(name) if name == "w:footnote" => return Ok(lines.join("\n")),
            Node::Eof => return Err(ParseError::Corrupted),
            _ => {}
        }
    }
}

pub(super) fn footnote_blocks(
    mut notes: BTreeMap<u32, String>,
    ctx: &Ctx<'_>,
    has_pages: bool,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut seen = HashSet::new();
    let referenced = ctx.footnote_refs.iter().map(|(id, page)| (*id, Some(*page)));
    let rest: Vec<(u32, Option<u32>)> = notes.keys().map(|id| (*id, None)).collect();
    for (id, page) in referenced.chain(rest) {
        if !seen.insert(id) {
            continue;
        }
        let Some(text) = notes.remove(&id) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let page = page.filter(|_| has_pages);
        blocks.push(Block::Para { text, anchor: ctx.anchor(page, format!("footnote[{id}]")) });
    }
    blocks
}
