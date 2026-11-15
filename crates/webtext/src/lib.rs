mod boxtable;
mod math;
mod spans;

pub use boxtable::{BoxTable, Piece, parse_box_table, split_body};
pub use math::plain_math;
pub use spans::{Layered, Marked, find_all, mark_layers, mark_spans};

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(test)]
mod tests;
