//! Core conversion modules
//!
//! This module contains the main conversion engines:
//! - `latex2typst`: LaTeX to Typst converter (AST-based)
//! - `typst2latex`: Typst to LaTeX converter

pub mod latex2typst;
pub mod typst2latex;

// Re-export main types and functions from latex2typst
pub use latex2typst::{
    convert_document_with_ast, convert_document_with_ast_options, convert_math_with_ast,
    convert_math_with_ast_options, convert_with_ast, convert_with_ast_options, ConversionMode,
    ConversionState, EnvironmentContext, L2TOptions, LatexConverter, MERGED_SPEC,
};

// Re-export main types and functions from typst2latex
pub use typst2latex::{
    typst_document_to_latex, typst_to_latex, typst_to_latex_with_options, T2LOptions,
};
