use docmodel::{Anchor, Block, FactSource};

use super::counter::Counters;
use super::numbering::Numbering;
use super::styles::Styles;

#[derive(Debug)]
pub(super) struct Pages {
    page: u32,
    breaks: u32,
    fresh: bool,
}

impl Default for Pages {
    fn default() -> Self {
        Self { page: 1, breaks: 0, fresh: true }
    }
}

impl Pages {
    pub(super) fn brk(&mut self) {
        if !self.fresh {
            self.page = self.page.saturating_add(1);
            self.breaks = self.breaks.saturating_add(1);
            self.fresh = true;
        }
    }

    pub(super) fn text(&mut self) {
        self.fresh = false;
    }

    pub(super) fn current(&self) -> u32 {
        self.page
    }

    pub(super) fn any(&self) -> bool {
        self.breaks > 0
    }
}

pub(super) struct Ctx<'d> {
    pub(super) styles: &'d Styles,
    pub(super) numbering: &'d Numbering,
    pub(super) counters: Counters,
    pub(super) pages: Pages,
    pub(super) footnote_refs: Vec<(u32, u32)>,
    pub(super) source: FactSource,
}

impl<'d> Ctx<'d> {
    pub(super) fn new(styles: &'d Styles, numbering: &'d Numbering, source: FactSource) -> Self {
        Self {
            styles,
            numbering,
            counters: Counters::default(),
            pages: Pages::default(),
            footnote_refs: Vec::new(),
            source,
        }
    }

    pub(super) fn anchor(&self, page: Option<u32>, path: String) -> Anchor {
        Anchor { source: self.source.clone(), page, bbox: None, path: Some(path), span: None }
    }
}

pub(super) fn clear_pages(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Heading { anchor, .. } | Block::Para { anchor, .. } => anchor.page = None,
            Block::Table(t) => {
                t.anchor.page = None;
                for cell in &mut t.cells {
                    cell.anchor.page = None;
                }
            }
        }
    }
}
