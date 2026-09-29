// clause_id 부여 (PROJECT.md §6.1, §8)
// 쓰임 — 같은 좌표를 셋이 공유한다:
// - RAG 청크 단위 = clause (retrieval::upload_index)
// - fact slice의 출처 = clause_id (engine::leaf)
// - 답변 인용 = clause_id + anchor (server::answer)
// 그래서 RAG가 인용한 대목과 엔진의 fact slice 출처가 같은 좌표를 가리킨다.
//
// 이 파일은 「블록들을 조항 단위로 묶는다」 한 가지만 한다.
// 조·항·호 표시 판별은 clause/marker.rs, 표를 글자로 바꾸기는 Table::to_markdown(block.rs).

mod marker; // 조·항·호 표시 판별 — clause/marker.rs

use crate::{
    anchor::Anchor,
    block::{Block, Document},
};
use marker::{Marker, parse_marker};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClauseId(pub String);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clause {
    pub id: ClauseId,
    pub text: String,
    pub anchor: Anchor,
    pub block_range: (usize, usize),
}

#[derive(Default)]
struct Builder {
    out: Vec<Clause>,
    open: Option<Clause>,
    article: Option<String>,
    para: Option<u32>,
    seq: u32,
    used: HashSet<String>,
}

// other crate server use this
pub fn assign_clauses(doc: &Document) -> Vec<Clause> {
    let mut b = Builder::default();
    for (i, block) in doc.blocks.iter().enumerate() {
        match block {
            Block::Heading { text, anchor, .. } => match parse_marker(text) {
                Some(m) => b.start_marked(m, i, text, anchor),
                None => b.push_single(i, text.clone(), anchor),
            },
            Block::Para { text, anchor } => match parse_marker(text) {
                Some(m) => b.start_marked(m, i, text, anchor),
                None => b.continue_or_single(i, text, anchor),
            },
            Block::Table(t) => b.push_single(i, t.to_markdown(), &t.anchor),
        }
    }
    b.finish()
}

impl Builder {
    fn start_marked(&mut self, m: Marker, i: usize, text: &str, anchor: &Anchor) {
        self.close();
        let mut item = None;
        match m {
            Marker::Article { name, para } => {
                self.article = Some(name);
                self.para = para;
            }
            Marker::Paragraph(n) => self.para = Some(n),
            Marker::Item(n) => item = Some(n),
        }
        let mut parts = Vec::new();
        if let Some(a) = &self.article {
            parts.push(a.clone());
        }
        if let Some(p) = self.para {
            parts.push(format!("{p}항"));
        }
        if let Some(n) = item {
            parts.push(format!("{n}호"));
        }
        let id = self.unique(parts.join("-"));
        self.open = Some(Clause {
            id: ClauseId(id),
            text: text.to_string(),
            anchor: anchor.clone(),
            block_range: (i, i + 1),
        });
    }

    fn continue_or_single(&mut self, i: usize, text: &str, anchor: &Anchor) {
        match self.open.as_mut() {
            Some(c) => {
                c.text.push('\n');
                c.text.push_str(text);
                c.block_range.1 = i + 1;
            }
            None => self.push_single(i, text.to_string(), anchor),
        }
    }

    fn push_single(&mut self, i: usize, text: String, anchor: &Anchor) {
        self.close();
        self.seq += 1;
        let id = self.unique(format!("c-{:04}", self.seq));
        self.out.push(Clause {
            id: ClauseId(id),
            text,
            anchor: anchor.clone(),
            block_range: (i, i + 1),
        });
    }
    /// 같은 번호가 또 나오면 "#2", "#3"을 붙여 문서 안에서 겹치지 않게
    fn unique(&mut self, base: String) -> String {
        let mut id = base.clone();
        let mut n = 2;
        while !self.used.insert(id.clone()) {
            id = format!("{base}#{n}");
            n += 1;
        }
        id
    }

    fn finish(mut self) -> Vec<Clause> {
        self.close();
        self.out
    }

    fn close(&mut self) {
        if let Some(c) = self.open.take() {
            self.out.push(c);
        }
    }
}
