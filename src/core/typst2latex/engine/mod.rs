//! MiniEval: A lightweight Typst macro evaluator.
//!
//! This module implements a partial evaluator for Typst that can expand macros
//! (`#let`, `#for`, `#if`, function calls) without the overhead of the full
//! Typst compiler. It is designed for source-to-source transformation.
//!
//! # Architecture
//!
//! ```text
//! Source Code (with macros)
//!        │
//!        ▼
//!    ┌───────────────┐
//!    │ typst-syntax  │  (AST parsing)
//!    │    Parser     │
//!    └───────────────┘
//!        │
//!        ▼
//!    ┌───────────────┐
//!    │   MiniEval    │  (This module)
//!    │   Evaluator   │
//!    └───────────────┘
//!        │
//!        ▼
//!    Expanded Source Code
//!        │
//!        ▼
//!    ┌───────────────┐
//!    │    Tylax      │  (Existing converter)
//!    │   Converter   │
//!    └───────────────┘
//!        │
//!        ▼
//!       LaTeX
//! ```
//!
//! # Example
//!
//! ```ignore
//! use tylax::expand_macros;
//!
//! let input = r#"
//! #let n = 3
//! #for i in range(n) {
//!   Item #(i + 1)
//! }
//! "#;
//!
//! let expanded = expand_macros(input)?;
//! // Result: "Item 1\nItem 2\nItem 3\n"
//! ```

mod data;
mod eval;
mod library;
mod ops;
mod scope;
mod value;
mod vfs;

pub use data::{parse_csv, parse_json, parse_toml, parse_yaml};
pub use eval::{expand_macros, expand_macros_with_warnings, EvalWarning, ExpandResult, MiniEval};
pub use scope::{Scope, Scopes};
pub(crate) use value::render_math_segments_to_typst_source;
pub use value::{
    Alignment, Arg, Arguments, Closure, Color, ContentNode, Counter, DateTime, Direction,
    EvalError, EvalErrorKind, EvalResult, HorizAlign, Length, LengthUnit, MathSegment, Selector,
    SourceSpan, Symbol, ValType, Value, VertAlign, WrappedRegex,
};
#[cfg(not(target_arch = "wasm32"))]
pub use vfs::RealVfs;
pub use vfs::{MemoryVfs, NoopVfs, VfsError, VfsResult, VirtualFileSystem};
