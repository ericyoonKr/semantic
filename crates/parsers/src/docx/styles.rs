use std::collections::HashMap;
use std::io::BufRead;

use super::props::{ParaProps, read_ppr};
use crate::error::ParseError;
use crate::xml::{Guarded, Node, XmlReader};

const MAX_CHAIN: usize = 32;

#[derive(Debug, Default)]
pub(super) struct Styles {
    by_id: HashMap<String, StyleDef>,
    default_para: Option<String>,
}

#[derive(Debug, Default)]
struct StyleDef {
    based_on: Option<String>,
    props: ParaProps,
}

impl Styles {
    pub(super) fn parse<R: BufRead + Guarded>(xml: &mut XmlReader<R>) -> Result<Self, ParseError> {
        let mut styles = Styles::default();
        loop {
            match xml.next()? {
                Node::Open(tag) if tag.name == "w:style" => {
                    let paragraph = matches!(tag.attr("w:type"), None | Some("paragraph"));
                    let Some(id) = tag.attr("w:styleId").filter(|_| paragraph) else {
                        xml.skip()?;
                        continue;
                    };
                    let id = id.to_string();
                    if matches!(tag.attr("w:default"), Some("1" | "true" | "on")) {
                        styles.default_para = Some(id.clone());
                    }
                    let def = read_style(xml)?;
                    styles.by_id.insert(id, def);
                }
                Node::Eof => return Ok(styles),
                _ => {}
            }
        }
    }

    pub(super) fn resolve(&self, style: Option<&str>) -> (ParaProps, Option<String>) {
        let effective = style.map(str::to_string).or_else(|| self.default_para.clone());
        let mut props = ParaProps::default();
        let mut current = effective.clone();
        for _ in 0..MAX_CHAIN {
            let Some(def) = current.as_deref().and_then(|id| self.by_id.get(id)) else {
                break;
            };
            props = props.or(&def.props);
            current = def.based_on.clone();
        }
        (props, effective)
    }
}

fn read_style<R: BufRead + Guarded>(xml: &mut XmlReader<R>) -> Result<StyleDef, ParseError> {
    let mut def = StyleDef::default();
    let mut ignored = None;
    loop {
        match xml.next()? {
            Node::Open(tag) => match tag.name.as_str() {
                "w:basedOn" => def.based_on = tag.attr("w:val").map(str::to_string),
                "w:pPr" => read_ppr(xml, &mut def.props, &mut ignored)?,
                _ => xml.skip()?,
            },
            Node::Close(name) if name == "w:style" => return Ok(def),
            Node::Close(_) | Node::Text(_) => {}
            Node::Eof => return Err(ParseError::Corrupted),
        }
    }
}
