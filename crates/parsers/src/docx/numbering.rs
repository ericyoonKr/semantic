use std::collections::HashMap;
use std::io::BufRead;

use super::props::clamp_level;
use crate::error::ParseError;
use crate::xml::{Guarded, Node, Tag, XmlReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Suffix {
    Tab,
    Space,
    Nothing,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LevelDef {
    pub(super) start: i64,
    pub(super) fmt: String,
    pub(super) text: Option<String>,
    pub(super) restart: Option<u8>,
    pub(super) suffix: Suffix,
    pub(super) legal: bool,
    pub(super) style: Option<String>,
}

impl Default for LevelDef {
    fn default() -> Self {
        Self {
            start: 0,
            fmt: "decimal".to_string(),
            text: None,
            restart: None,
            suffix: Suffix::Tab,
            legal: false,
            style: None,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Override {
    pub(super) start: Option<i64>,
    pub(super) level: Option<LevelDef>,
}

#[derive(Debug, Default)]
pub(super) struct NumDef {
    pub(super) abstract_id: u32,
    pub(super) overrides: HashMap<u8, Override>,
}

#[derive(Debug, Default)]
pub(super) struct Numbering {
    pub(super) abstracts: HashMap<u32, HashMap<u8, LevelDef>>,
    pub(super) nums: HashMap<u32, NumDef>,
}

impl Numbering {
    pub(super) fn parse<R: BufRead + Guarded>(xml: &mut XmlReader<R>) -> Result<Self, ParseError> {
        let mut numbering = Numbering::default();
        loop {
            match xml.next()? {
                Node::Open(tag) if tag.name == "w:abstractNum" => {
                    let id = tag.attr_u32("w:abstractNumId");
                    let levels = read_abstract(xml)?;
                    if let Some(id) = id {
                        numbering.abstracts.insert(id, levels);
                    }
                }
                Node::Open(tag) if tag.name == "w:num" => {
                    let id = tag.attr_u32("w:numId");
                    let def = read_num(xml)?;
                    if let Some(id) = id {
                        numbering.nums.insert(id, def);
                    }
                }
                Node::Eof => return Ok(numbering),
                _ => {}
            }
        }
    }

    pub(super) fn level_for_style(&self, num_id: u32, style: &str) -> Option<u8> {
        let num = self.nums.get(&num_id)?;
        let levels = self.abstracts.get(&num.abstract_id)?;
        levels
            .iter()
            .filter(|(_, l)| l.style.as_deref() == Some(style))
            .map(|(k, _)| *k)
            .min()
    }
}

fn read_abstract<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
) -> Result<HashMap<u8, LevelDef>, ParseError> {
    let mut levels = HashMap::new();
    loop {
        match xml.next()? {
            Node::Open(tag) if tag.name == "w:lvl" => {
                let ilvl = tag.attr_u32("w:ilvl").map(clamp_level).unwrap_or(0);
                levels.insert(ilvl, read_level(xml)?);
            }
            Node::Close(name) if name == "w:abstractNum" => return Ok(levels),
            Node::Eof => return Err(ParseError::Corrupted),
            _ => {}
        }
    }
}

fn read_num<R: BufRead + Guarded>(xml: &mut XmlReader<R>) -> Result<NumDef, ParseError> {
    let mut def = NumDef::default();
    let mut current: Option<u8> = None;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                "w:abstractNumId" => def.abstract_id = tag.attr_u32("w:val").unwrap_or(0),
                "w:lvlOverride" => {
                    let ilvl = tag.attr_u32("w:ilvl").map(clamp_level).unwrap_or(0);
                    def.overrides.entry(ilvl).or_default();
                    current = Some(ilvl);
                }
                "w:startOverride" => {
                    if let Some(o) = current.and_then(|k| def.overrides.get_mut(&k)) {
                        o.start = tag.attr("w:val").and_then(|v| v.trim().parse().ok());
                    }
                }
                "w:lvl" => {
                    let level = read_level(xml)?;
                    if let Some(o) = current.and_then(|k| def.overrides.get_mut(&k)) {
                        o.level = Some(level);
                    }
                }
                _ => {}
            },
            Node::Close(name) if name == "w:lvlOverride" => current = None,
            Node::Close(name) if name == "w:num" => return Ok(def),
            Node::Eof => return Err(ParseError::Corrupted),
            _ => {}
        }
    }
}

fn read_level<R: BufRead + Guarded>(xml: &mut XmlReader<R>) -> Result<LevelDef, ParseError> {
    let mut level = LevelDef::default();
    loop {
        match xml.next()? {
            Node::Open(tag) => apply(&mut level, &tag, xml)?,
            Node::Close(name) if name == "w:lvl" => return Ok(level),
            Node::Eof => return Err(ParseError::Corrupted),
            _ => {}
        }
    }
}

fn apply<R: BufRead + Guarded>(
    level: &mut LevelDef,
    tag: &Tag,
    xml: &mut XmlReader<R>,
) -> Result<(), ParseError> {
    let val = tag.attr("w:val");
    match tag.name.as_str() {
        "w:start" => level.start = val.and_then(|v| v.trim().parse().ok()).unwrap_or(0),
        "w:numFmt" => level.fmt = val.unwrap_or("decimal").to_string(),
        "w:lvlText" => level.text = val.map(str::to_string),
        "w:lvlRestart" => level.restart = tag.attr_u32("w:val").map(clamp_level),
        "w:pStyle" => level.style = val.map(str::to_string),
        "w:isLgl" => level.legal = tag.on("w:val"),
        "w:suff" => {
            level.suffix = match val {
                Some("space") => Suffix::Space,
                Some("nothing") => Suffix::Nothing,
                _ => Suffix::Tab,
            }
        }
        "w:pPr" | "w:rPr" => xml.skip()?,
        _ => {}
    }
    Ok(())
}
