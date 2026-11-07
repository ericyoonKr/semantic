use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::anchor::Anchor;
use crate::block::{Block, Document};
use crate::clause::marker::article_title;

static PAGE_NUMBER_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:-[ \t]*[0-9]{1,4}[ \t]*-|[0-9]{1,4})$").expect("PAGE_NUMBER_LINE 정규식")
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanMap {
    pub start: u32,
    pub end: u32,
    pub block_index: usize,
    pub anchor: Anchor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineEntry {
    pub level: u8,
    pub title: String,
    pub page: Option<u32>,
}

#[derive(Default)]
struct Paged {
    text: String,
    chars: u32,
    maps: Vec<SpanMap>,
}

impl Paged {
    fn push(&mut self, s: &str) {
        self.text.push_str(s);
        self.chars = self.chars.saturating_add(char_len(s));
    }
}

impl Document {
    pub fn render_paged(&self) -> (String, Vec<SpanMap>) {
        let mut out = Paged::default();
        let mut current: Option<u32> = None;
        for (i, block) in self.blocks.iter().enumerate() {
            let anchor = block_anchor(block);
            if let Some(p) = anchor.page
                && current != Some(p)
            {
                out.push(&format!("--- p{p} ---\n"));
                current = Some(p);
            }
            let start = out.chars;
            out.push(&block_text(block));
            let end = out.chars;
            let mut mapped = anchor.clone();
            mapped.page = mapped.page.or(current);
            mapped.span = Some((start, end));
            out.maps.push(SpanMap {
                start,
                end,
                block_index: i,
                anchor: mapped,
            });
            out.push("\n");
        }
        (out.text, out.maps)
    }

    pub fn outline(&self, max_level: u8, max_lines: usize) -> Vec<OutlineEntry> {
        let mut out = Vec::new();
        let mut current: Option<u32> = None;
        let mut heading_level: u8 = 0;
        for block in &self.blocks {
            if out.len() >= max_lines {
                break;
            }
            let anchor = block_anchor(block);
            if anchor.page.is_some() {
                current = anchor.page;
            }
            let entry = match block {
                Block::Heading { level, text, .. } => {
                    heading_level = *level;
                    Some((*level, one_line(text)))
                }
                Block::Para { text, .. } => {
                    article_title(text).map(|t| (heading_level.saturating_add(1), t))
                }
                Block::Table(_) => None,
            };
            if let Some((level, title)) = entry
                && level <= max_level
                && !title.is_empty()
            {
                out.push(OutlineEntry {
                    level,
                    title,
                    page: current,
                });
            }
        }
        out
    }
}

fn block_anchor(block: &Block) -> &Anchor {
    match block {
        Block::Heading { anchor, .. } | Block::Para { anchor, .. } => anchor,
        Block::Table(t) => &t.anchor,
    }
}

fn block_text(block: &Block) -> String {
    match block {
        Block::Heading { text, .. } => text.clone(),
        Block::Para { text, .. } if is_page_number_line(text) => " ".repeat(text.chars().count()),
        Block::Para { text, .. } => text.clone(),
        Block::Table(t) => t.to_markdown(),
    }
}

fn is_page_number_line(text: &str) -> bool {
    !text.contains('\n') && PAGE_NUMBER_LINE.is_match(text.trim())
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn char_len(s: &str) -> u32 {
    u32::try_from(s.chars().count()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::{FactSource, FileId};

    fn anchor(page: Option<u32>) -> Anchor {
        Anchor {
            source: FactSource::File(FileId("f".to_string())),
            page,
            bbox: None,
            path: None,
            span: None,
        }
    }

    fn para(text: &str, page: Option<u32>) -> Block {
        Block::Para {
            text: text.to_string(),
            anchor: anchor(page),
        }
    }

    fn doc(blocks: Vec<Block>) -> Document {
        Document {
            blocks,
            source: FileId("f".to_string()),
            warnings: Vec::new(),
        }
    }

    fn slice(s: &str, start: u32, end: u32) -> String {
        s.chars()
            .skip(start as usize)
            .take(end.saturating_sub(start) as usize)
            .collect()
    }

    #[test]
    fn page_number_lines_are_blanked_and_offsets_kept() {
        let d = doc(vec![
            para("첫 문단", Some(1)),
            para("- 1 -", Some(1)),
            para("둘째 문단", Some(2)),
            para("12", None),
            para("-3-", None),
            para("12개월", None),
            para("1.5%", None),
        ]);
        let (paged, maps) = d.render_paged();
        let expected = "--- p1 ---\n첫 문단\n     \n--- p2 ---\n둘째 문단\n  \n   \n12개월\n1.5%\n";
        assert_eq!(paged, expected);
        assert_eq!(maps.len(), 7);
        assert_eq!(maps[1].end - maps[1].start, 5);
        assert_eq!(slice(&paged, maps[1].start, maps[1].end), "     ");
        assert_eq!(slice(&paged, maps[5].start, maps[5].end), "12개월");
        assert_eq!(slice(&paged, maps[6].start, maps[6].end), "1.5%");
    }

    #[test]
    fn span_map_points_to_right_block() {
        let d = doc(vec![
            para("첫 문단", Some(1)),
            para("- 1 -", Some(1)),
            para("둘째 문단", Some(2)),
            para("셋째 문단", None),
        ]);
        let (paged, maps) = d.render_paged();
        let Some(pos) = paged.find("셋째") else {
            assert!(paged.contains("셋째"));
            return;
        };
        let offset = char_len(&paged[..pos]);
        let hit = maps.iter().find(|m| m.start <= offset && offset < m.end);
        assert_eq!(hit.map(|m| m.block_index), Some(3));
        assert_eq!(hit.and_then(|m| m.anchor.page), Some(2));
        assert_eq!(hit.and_then(|m| m.anchor.span), hit.map(|m| (m.start, m.end)));
        assert_eq!(
            hit.map(|m| slice(&paged, m.start, m.end)),
            Some("셋째 문단".to_string())
        );
    }
}
