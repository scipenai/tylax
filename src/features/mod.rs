//! Feature modules - Advanced conversion features
//!
//! This module contains specialized handlers for complex LaTeX/Typst features:
//! - Tables (tabular, multicolumn, multirow)
//! - Images and figures
//! - Citations and cross-references
//! - BibTeX parsing
//! - TikZ to CeTZ conversion
//! - Macro expansion
//! - Document templates

pub mod bibtex;
pub mod images;
pub mod macros;
pub mod refs;
pub mod tables;
pub mod templates;
pub mod tikz;

// Re-export commonly used types
pub use images::Dimension;
pub use refs::{Citation, CitationMode, Label};
pub use tables::{Alignment, ColSpec, Table};
