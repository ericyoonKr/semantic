mod body;
mod counter;
mod footnotes;
mod numbering;
mod numfmt;
mod para;
mod props;
mod state;
mod styles;
mod table;

use docmodel::{Document, FactSource, FileId};

use crate::error::ParseError;
use crate::guard::{Deadline, GuardLimits, Package};
use crate::xml::XmlReader;
use numbering::Numbering;
use state::{Ctx, clear_pages};
use styles::Styles;

const MAIN: &str = "word/document.xml";
const STYLES: &str = "word/styles.xml";
const NUMBERING: &str = "word/numbering.xml";
const FOOTNOTES: &str = "word/footnotes.xml";

pub(crate) fn parse_docx(
    bytes: &[u8],
    limits: &GuardLimits,
    source: FileId,
) -> Result<Document, ParseError> {
    let deadline = Deadline::start(limits);
    let mut package = Package::open(bytes, limits)?;
    let styles = match package.member(STYLES)? {
        Some(m) => Styles::parse(&mut XmlReader::new(m, deadline))?,
        None => Styles::default(),
    };
    let numbering = match package.member(NUMBERING)? {
        Some(m) => Numbering::parse(&mut XmlReader::new(m, deadline))?,
        None => Numbering::default(),
    };
    let mut ctx = Ctx::new(&styles, &numbering, FactSource::File(source.clone()));
    let main = package.member(MAIN)?.ok_or(ParseError::Corrupted)?;
    let mut blocks = body::read_body(&mut XmlReader::new(main, deadline), &mut ctx)?;
    let has_pages = ctx.pages.any();
    if let Some(m) = package.member(FOOTNOTES)? {
        let notes = footnotes::read_footnotes(&mut XmlReader::new(m, deadline), &mut ctx)?;
        blocks.extend(footnotes::footnote_blocks(notes, &ctx, has_pages));
    }
    if !has_pages {
        clear_pages(&mut blocks);
    }
    Ok(Document { blocks, source, warnings: Vec::new() })
}

#[cfg(test)]
mod tests;
