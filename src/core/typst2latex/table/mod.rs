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
mod columns;
mod generator;
mod hline;

use typst_syntax::{ast, ast::AstNode, SyntaxKind, SyntaxNode};

#[cfg(test)]
mod tests;

// Re-export public API
pub use cell::{LatexCell, LatexCellAlign};
pub use generator::LatexTableGenerator;
pub use hline::LatexHLine;

pub(crate) use columns::{exceeds_column_limit, infer_table_columns, MAX_TABLE_COLUMNS};

pub(crate) fn table_rule(node: &SyntaxNode) -> Option<(&'static str, &SyntaxNode)> {
    // Keep the unwrapped call so hline parsing retains its arguments.
    let node = unparenthesized(node);
    let call = node.cast::<ast::FuncCall>()?;
    let access = unparenthesized(call.callee().to_untyped()).cast::<ast::FieldAccess>()?;
    let namespace = unparenthesized(access.target().to_untyped()).cast::<ast::Ident>()?;
    if !matches!(namespace.get().as_str(), "table" | "grid") {
        return None;
    }
    match access.field().get().as_str() {
        "hline" => Some(("hline", node)),
        "vline" => Some(("vline", node)),
        _ => None,
    }
}

fn unparenthesized(mut node: &SyntaxNode) -> &SyntaxNode {
    while let Some(group) = node.cast::<ast::Parenthesized>() {
        node = group.expr().to_untyped();
    }
    node
}

pub(crate) fn is_none_value(node: &SyntaxNode) -> bool {
    unparenthesized(node).kind() == SyntaxKind::None
}

/// An empty spread adds no cells, unlike a positional `none`.
pub(crate) fn is_empty_spread(node: &SyntaxNode) -> bool {
    node.cast::<ast::Spread>()
        .is_some_and(|spread| is_none_value(spread.expr().to_untyped()))
}
