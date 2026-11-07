use std::io::BufRead;

use docmodel::Block;

use super::para::{SKIP, read_para};
use super::state::Ctx;
use super::table::read_table;
use crate::error::ParseError;
use crate::xml::{Guarded, Node, XmlReader};

pub(super) fn read_body<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    ctx: &mut Ctx<'_>,
) -> Result<Vec<Block>, ParseError> {
    let mut blocks = Vec::new();
    let mut paras: usize = 0;
    let mut tables: usize = 0;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                name if SKIP.contains(&name) => xml.skip()?,
                "w:sectPr" | "w:sdtPr" | "w:sdtEndPr" => xml.skip()?,
                "w:p" => {
                    let index = paras;
                    paras += 1;
                    let para = read_para(xml, ctx)?;
                    if para.text.trim().is_empty() {
                        continue;
                    }
                    let anchor = ctx.anchor(Some(para.page), format!("body/p[{index}]"));
                    blocks.push(match para.outline {
                        Some(n) => Block::Heading { level: n + 1, text: para.text, anchor },
                        None => Block::Para { text: para.text, anchor },
                    });
                }
                "w:tbl" => {
                    let index = tables;
                    tables += 1;
                    blocks.push(Block::Table(read_table(xml, ctx, index)?));
                }
                _ => {}
            },
            Node::Eof => return Ok(blocks),
            _ => {}
        }
    }
}
