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
    pub degraded : bool,
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
    pub warnings : Vec<ExtractWarning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExtractWarning {
    PagesUnextracted {from : u32, to : u32},
    TableDegraded {anchor : Anchor},
    LowConfidenceTable {anchor: Anchor},
}

impl Table {
    pub fn to_markdown(&self) -> String {
        let rows = self.cells.iter().map(|c| c.row + c.rowspan.max(1)).max().unwrap_or(0) as usize;
        let cols = self.cells.iter().map(|c| c.col + c.colspan.max(1)).max().unwrap_or(0) as usize;
        if rows == 0 || cols == 0 {
            return String::new();
        }
        let mut grid = vec![vec![String::new(); cols]; rows];
        for c in &self.cells {
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
        out.pop();
        out
    }
}
