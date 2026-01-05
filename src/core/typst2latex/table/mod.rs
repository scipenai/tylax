//! Typst to LaTeX Table Conversion Engine
//!
//! A state-aware generator for converting Typst tables to LaTeX,
//! handling complex features like rowspan, colspan, partial hlines, and nested tables.
//!
//! # Architecture
//!
//! ```text
//! Typst AST -> Cell/HLine Parsing -> State Machine -> LaTeX Generation
//! ```
//!
//! # Example
//!
//! ```ignore
//! use table::{LatexTableGenerator, LatexCell};
//!
//! let mut gen = LatexTableGenerator::new(3, vec![LatexCellAlign::Left, LatexCellAlign::Center, LatexCellAlign::Right]);
//! gen.process_row(cells);
//! let latex = gen.generate_latex();
//! ```

mod cell;
mod generator;
mod hline;

#[cfg(test)]
mod tests;

// Re-export public API
pub use cell::{LatexCell, LatexCellAlign};
pub use generator::{LatexRow, LatexTableGenerator};
pub use hline::{HLineStyle, LatexHLine};
