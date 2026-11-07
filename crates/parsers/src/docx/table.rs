use std::collections::HashMap;
use std::io::BufRead;

use docmodel::{Cell, Table};

use super::para::{SKIP, read_para};
use super::state::Ctx;
use crate::error::ParseError;
use crate::xml::{Guarded, Node, XmlReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VMerge {
    None,
    Restart,
    Continue,
}

struct CellOut {
    span: u32,
    merge: VMerge,
    text: String,
    page: u32,
}

pub(super) fn read_table<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    ctx: &mut Ctx<'_>,
    index: usize,
) -> Result<Table, ParseError> {
    let mut cells: Vec<Cell> = Vec::new();
    let mut open: HashMap<u32, usize> = HashMap::new();
    let mut row: i64 = -1;
    let mut col: u32 = 0;
    let mut tc: usize = 0;
    let mut first_page: Option<u32> = None;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                name if SKIP.contains(&name) => xml.skip()?,
                "w:tblPr" | "w:tblGrid" | "w:tblPrEx" => xml.skip()?,
                "w:tr" => {
                    row += 1;
                    col = 0;
                    tc = 0;
                }
                "w:gridBefore" => col = col.saturating_add(tag.attr_u32("w:val").unwrap_or(0)),
                "w:tc" => {
                    let out = read_cell(xml, ctx)?;
                    first_page.get_or_insert(out.page);
                    let r = row.max(0) as u32;
                    let covered = col..col.saturating_add(out.span);
                    match (out.merge, open.get(&col).copied()) {
                        (VMerge::Continue, Some(ci)) => {
                            if let Some(cell) = cells.get_mut(ci) {
                                cell.rowspan += 1;
                            }
                        }
                        (merge, _) => {
                            for c in covered.clone() {
                                open.remove(&c);
                            }
                            if merge == VMerge::Restart {
                                open.insert(col, cells.len());
                            }
                            let path = format!("tbl[{index}]/tr[{r}]/tc[{tc}]");
                            cells.push(Cell {
                                row: r,
                                col,
                                rowspan: 1,
                                colspan: out.span,
                                text: out.text,
                                anchor: ctx.anchor(Some(out.page), path),
                            });
                        }
                    }
                    col = covered.end;
                    tc += 1;
                }
                _ => {}
            },
            Node::Close(name) if name == "w:tbl" => break,
            Node::Eof => return Err(ParseError::Corrupted),
            _ => {}
        }
    }
    let page = first_page.unwrap_or_else(|| ctx.pages.current());
    Ok(Table { cells, anchor: ctx.anchor(Some(page), format!("tbl[{index}]")), degraded: false })
}

fn read_cell<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    ctx: &mut Ctx<'_>,
) -> Result<CellOut, ParseError> {
    let mut out = CellOut { span: 1, merge: VMerge::None, text: String::new(), page: 0 };
    let mut page: Option<u32> = None;
    let mut nested: usize = 0;
    let mut in_tcpr = false;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                name if SKIP.contains(&name) => xml.skip()?,
                "w:tcPr" if nested == 0 => in_tcpr = true,
                "w:tcPrChange" => xml.skip()?,
                "w:gridSpan" if in_tcpr => out.span = tag.attr_u32("w:val").unwrap_or(1).max(1),
                "w:vMerge" if in_tcpr => {
                    out.merge = match tag.attr("w:val") {
                        Some("restart") => VMerge::Restart,
                        _ => VMerge::Continue,
                    }
                }
                "w:tbl" => nested += 1,
                "w:p" => {
                    let para = read_para(xml, ctx)?;
                    if !para.text.trim().is_empty() {
                        page.get_or_insert(para.page);
                        if !out.text.is_empty() {
                            out.text.push('\n');
                        }
                        out.text.push_str(&para.text);
                    }
                }
                _ => {}
            },
            Node::Close(name) => match name.as_str() {
                "w:tcPr" => in_tcpr = false,
                "w:tbl" => nested = nested.saturating_sub(1),
                "w:tc" if nested == 0 => break,
                _ => {}
            },
            Node::Text(_) => {}
            Node::Eof => return Err(ParseError::Corrupted),
        }
    }
    out.page = page.unwrap_or_else(|| ctx.pages.current());
    Ok(out)
}
