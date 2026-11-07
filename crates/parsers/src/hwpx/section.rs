use std::io::BufRead;

use docmodel::{Anchor, Block, Cell, FactSource, Table};

use crate::error::ParseError;
use crate::xml::{Guarded, Node, XmlReader};

const SKIP: [&str; 1] = ["hp:ctrl"];

struct Paragraph {
    text: String,
    tables: Vec<Table>,
}

pub(super) fn read_section<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    source: &FactSource,
    section: usize,
) -> Result<Vec<Block>, ParseError> {
    let mut blocks = Vec::new();
    let mut index: usize = 0;
    loop {
        match xml.next()? {
            Node::Open(tag) if tag.name == "hp:p" => {
                let path = format!("section[{section}]/p[{index}]");
                index += 1;
                let para = read_paragraph(xml, source, Some(&path))?;
                if !para.text.trim().is_empty() {
                    blocks.push(Block::Para { text: para.text, anchor: anchor(source, path) });
                }
                blocks.extend(para.tables.into_iter().map(Block::Table));
            }
            Node::Eof => return Ok(blocks),
            _ => {}
        }
    }
}

fn read_paragraph<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    source: &FactSource,
    path: Option<&str>,
) -> Result<Paragraph, ParseError> {
    let mut out = Paragraph { text: String::new(), tables: Vec::new() };
    let mut in_text = false;
    let mut nested: usize = 0;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                name if SKIP.contains(&name) => xml.skip()?,
                "hp:tbl" => {
                    if let Some(p) = path {
                        let table_path = format!("{p}/tbl[{}]", out.tables.len());
                        out.tables.push(read_table(xml, source, table_path)?);
                    }
                }
                "hp:p" => {
                    nested += 1;
                    if !out.text.is_empty() {
                        out.text.push('\n');
                    }
                }
                "hp:t" => in_text = true,
                "hp:tab" if in_text => out.text.push('\t'),
                "hp:lineBreak" if in_text => out.text.push('\n'),
                _ => {}
            },
            Node::Close(name) => match name.as_str() {
                "hp:t" => in_text = false,
                "hp:p" if nested == 0 => return Ok(out),
                "hp:p" => nested -= 1,
                _ => {}
            },
            Node::Text(t) if in_text => out.text.push_str(&t),
            Node::Text(_) => {}
            Node::Eof => return Err(ParseError::Corrupted),
        }
    }
}

fn read_table<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    source: &FactSource,
    path: String,
) -> Result<Table, ParseError> {
    let mut cells = Vec::new();
    let mut row: u32 = 0;
    let mut tr: i64 = -1;
    let mut tc: usize = 0;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                "hp:tr" => {
                    tr += 1;
                    row = tr.max(0) as u32;
                    tc = 0;
                }
                "hp:tc" => {
                    let cell_path = format!("{path}/tr[{row}]/tc[{tc}]");
                    let (mut cell, row_addr) = read_cell(xml, source)?;
                    cell.row = row_addr.unwrap_or(row);
                    cell.anchor = anchor(source, cell_path);
                    cells.push(cell);
                    tc += 1;
                }
                _ => {}
            },
            Node::Close(name) if name == "hp:tbl" => break,
            Node::Eof => return Err(ParseError::Corrupted),
            _ => {}
        }
    }
    Ok(Table { cells, anchor: anchor(source, path), degraded: false })
}

fn read_cell<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    source: &FactSource,
) -> Result<(Cell, Option<u32>), ParseError> {
    let mut cell = Cell {
        row: 0,
        col: 0,
        rowspan: 1,
        colspan: 1,
        text: String::new(),
        anchor: anchor(source, String::new()),
    };
    let mut row_addr = None;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                name if SKIP.contains(&name) => xml.skip()?,
                "hp:p" => {
                    let para = read_paragraph(xml, source, None)?;
                    if !para.text.trim().is_empty() {
                        push_line(&mut cell.text, &para.text);
                    }
                }
                "hp:cellAddr" => {
                    cell.col = tag.attr_u32("colAddr").unwrap_or(cell.col);
                    row_addr = tag.attr_u32("rowAddr");
                }
                "hp:cellSpan" => {
                    cell.colspan = tag.attr_u32("colSpan").unwrap_or(1).max(1);
                    cell.rowspan = tag.attr_u32("rowSpan").unwrap_or(1).max(1);
                }
                _ => {}
            },
            Node::Close(name) if name == "hp:tc" => return Ok((cell, row_addr)),
            Node::Eof => return Err(ParseError::Corrupted),
            _ => {}
        }
    }
}

fn push_line(text: &mut String, line: &str) {
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(line);
}

fn anchor(source: &FactSource, path: String) -> Anchor {
    Anchor { source: source.clone(), page: None, bbox: None, path: Some(path), span: None }
}
