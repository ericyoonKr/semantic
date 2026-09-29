// 문서 블록 타입 (PROJECT.md §8 "통합 문서 모델")

use serde::{Deserialize, Serialize};

use crate::anchor::{Anchor, FileId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Heading {level : u8, text : String, anchor: Anchor},
    Para {text : String, anchor: Anchor},
    Table(Table),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub cells : Vec<Cell>,
    pub anchor : Anchor,
    pub degraded : bool, // if true, not use for checks::sums && Z3 input
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub row : u32,
    pub col : u32,
    pub rowspan : u32,
    pub colspan : u32,
    pub text : String,
    pub anchor : Anchor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub blocks : Vec<Block>,
    pub source : FileId,
    pub warnings : Vec<ExtractWarning>, //parser/src/pdf.rs lib.rs
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExtractWarning {
    PagesUnextracted {from : u32, to : u32},
    TableDegraded {anchor : Anchor},
    LowConfidenceTable {anchor: Anchor},
}

//     warnings = 「추출 불가」·부분 추출 범위 등, 사용자에게 그대로 표면화할 것들.
// 규칙:
// - 표를 평문으로 뭉개지 않는다. 행·열 구조를 유지해야
//   "합계 = 부분합" 결정론 검증과 Z3 산술의 입력이 된다.
// - 표를 글자로 바꾸는 일은 Table::to_markdown 하나만 한다 — clause 텍스트(clause.rs)와
//   LLM에 넘길 슬라이스가 같은 글자를 쓴다.
// - serde Serialize/Deserialize 파생 — RocksDB state CF에 직렬화 보관되므로.

impl Table {
    /// 표를 markdown 표로. 첫 행을 머리 행으로 쓴다.
    /// 병합 셀은 시작 칸에만 글자를 두고, 덮인 칸은 비운다.
    pub fn to_markdown(&self) -> String {
        let rows = self.cells.iter().map(|c| c.row + c.rowspan.max(1)).max().unwrap_or(0) as usize;
        let cols = self.cells.iter().map(|c| c.col + c.colspan.max(1)).max().unwrap_or(0) as usize;
        if rows == 0 || cols == 0 {
            return String::new();
        }
        let mut grid = vec![vec![String::new(); cols]; rows];
        for c in &self.cells {
            // 셀 안 줄바꿈과 '|'는 markdown 표를 깨므로 바꾼다
            grid[c.row as usize][c.col as usize] = c.text.replace('\n', " ").replace('|', "\\|");
        }
        let mut out = String::new();
        for (i, row) in grid.iter().enumerate() {
            out.push_str("| ");
            out.push_str(&row.join(" | "));
            out.push_str(" |\n");
            if i == 0 {
                out.push_str(&"| --- ".repeat(cols));
                out.push_str("|\n");
            }
        }
        out.pop(); // 마지막 줄바꿈
        out
    }
}
