//! Table Grid Parser System
//!
//! A state-aware grid parser for handling complex LaTeX table structures.
//!
//! This module provides robust parsing of LaTeX tables including:
//! - `\multirow` and `\multicolumn` support
//! - Sparse data tables (empty cells in the middle)
//! - Partial horizontal lines (`\cline`, `\cmidrule`)
//! - Nested table structures
//!
//! # Architecture
//!
//! The parser maintains a virtual grid state to correctly handle cell spanning:
//!
//! ```text
//! Raw LaTeX -> Pre-processing -> Grid State Machine -> Typst Generation
//! ```
//!
//! # Example
//!
//! ```ignore
//! use table::{CellAlign, parse_with_grid_parser};
//!
//! let alignments = vec![CellAlign::Left, CellAlign::Center, CellAlign::Right];
//! let typst_code = parse_with_grid_parser(content, alignments);
//! ```

mod cell;
mod hline;
mod parser;

#[cfg(test)]
mod tests;

// Re-export public API
pub use cell::{CellAlign, GridCell};
pub use hline::{clean_cell_content, clean_hline_args, extract_hline_range, HLine};
pub use parser::{parse_with_grid_parser, GridRow, TableGridParser};
