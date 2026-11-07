use std::io::BufRead;

use super::props::{ParaProps, read_ppr};
use super::state::Ctx;
use crate::error::ParseError;
use crate::xml::{Guarded, Node, XmlReader};

pub(super) const SKIP: [&str; 6] = [
    "w:del",
    "w:moveFrom",
    "w:rPrChange",
    "w:pPrChange",
    "mc:Fallback",
    "w:delInstrText",
];

const MAX_OUTLINE: u8 = 8;

pub(super) struct ParaOut {
    pub(super) text: String,
    pub(super) outline: Option<u8>,
    pub(super) page: u32,
}

struct Collector {
    body: String,
    page: Option<u32>,
    in_run: usize,
    in_text: bool,
    nested: usize,
}

impl Collector {
    fn push_text(&mut self, ctx: &mut Ctx<'_>, text: &str) {
        if text.chars().any(|c| !c.is_whitespace()) {
            if self.page.is_none() {
                self.page = Some(ctx.pages.current());
            }
            ctx.pages.text();
        }
        self.body.push_str(text);
    }
}

pub(super) fn read_para<R: BufRead + Guarded>(
    xml: &mut XmlReader<R>,
    ctx: &mut Ctx<'_>,
) -> Result<ParaOut, ParseError> {
    let mut direct = ParaProps::default();
    let mut style: Option<String> = None;
    let mut resolved: Option<(ParaProps, Option<String>)> = None;
    let mut c = Collector { body: String::new(), page: None, in_run: 0, in_text: false, nested: 0 };
    loop {
        let node = xml.next()?;
        let is_ppr = matches!(&node, Node::Open(t) if t.name == "w:pPr");
        if resolved.is_none() && !(is_ppr && c.nested == 0) {
            let r = resolve(ctx, &direct, style.as_deref());
            if r.0.break_before == Some(true) {
                ctx.pages.brk();
            }
            resolved = Some(r);
        }
        match node {
            Node::Open(tag) => {
                let in_run = c.in_run > 0;
                match tag.name.as_str() {
                    name if SKIP.contains(&name) => xml.skip()?,
                    "w:pPr" if c.nested == 0 && resolved.is_none() => {
                        read_ppr(xml, &mut direct, &mut style)?
                    }
                    "w:pPr" => xml.skip()?,
                    "w:p" => {
                        c.nested += 1;
                        if !c.body.is_empty() {
                            c.body.push('\n');
                        }
                    }
                    "w:r" => c.in_run += 1,
                    "w:t" if in_run => c.in_text = true,
                    "w:tab" | "w:ptab" if in_run => c.body.push('\t'),
                    "w:cr" if in_run => c.body.push('\n'),
                    "w:noBreakHyphen" if in_run => c.body.push('-'),
                    "w:br" if in_run => match tag.attr("w:type") {
                        Some("page") => ctx.pages.brk(),
                        Some("column") => {}
                        _ => c.body.push('\n'),
                    },
                    "w:lastRenderedPageBreak" => ctx.pages.brk(),
                    "w:footnoteReference" => {
                        if let Some(id) = tag.attr_u32("w:id") {
                            let page = c.page.unwrap_or_else(|| ctx.pages.current());
                            ctx.footnote_refs.push((id, page));
                        }
                    }
                    _ => {}
                }
            }
            Node::Close(name) => match name.as_str() {
                "w:t" => c.in_text = false,
                "w:r" => c.in_run = c.in_run.saturating_sub(1),
                "w:p" if c.nested == 0 => break,
                "w:p" => c.nested -= 1,
                _ => {}
            },
            Node::Text(text) if c.in_text => c.push_text(ctx, &text),
            Node::Text(_) => {}
            Node::Eof => return Err(ParseError::Corrupted),
        }
    }
    let (props, effective) = resolved.unwrap_or_default();
    let ilvl = props
        .ilvl
        .or_else(|| {
            let id = props.num_id?;
            ctx.numbering.level_for_style(id, effective.as_deref()?)
        })
        .unwrap_or(0);
    let label = props.num_id.and_then(|id| ctx.counters.next(ctx.numbering, id, ilvl));
    let text = if c.body.trim().is_empty() {
        String::new()
    } else {
        label.unwrap_or_default() + &c.body
    };
    Ok(ParaOut {
        text,
        outline: props.outline.filter(|l| *l <= MAX_OUTLINE),
        page: c.page.unwrap_or_else(|| ctx.pages.current()),
    })
}

fn resolve(ctx: &Ctx<'_>, direct: &ParaProps, style: Option<&str>) -> (ParaProps, Option<String>) {
    let (base, effective) = ctx.styles.resolve(style);
    (direct.clone().or(&base), effective)
}
