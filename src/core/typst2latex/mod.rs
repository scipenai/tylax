//! Typst to LaTeX converter
//!
//! This module converts Typst code to LaTeX, supporting both math mode and markup mode.
//!
//! # Module Structure
//! - `context`: Conversion state and options
//! - `utils`: Helper functions for escaping and text extraction
//! - `markup`: Document structure and text formatting conversion
//! - `math`: Mathematical expression conversion
//! - `preprocess`: #let definition extraction and variable expansion

pub mod context;
pub mod markup;
pub mod math;
pub mod preprocess;
pub mod table;
pub mod utils;

// Re-export main types and functions
pub use context::{ConvertContext, T2LOptions, TokenType};
pub use preprocess::{extract_let_definitions, preprocess_typst, TypstDefDb};

use markup::convert_markup_node;
use math::convert_math_node;
use typst_syntax::{parse, parse_math};
use utils::wrap_in_document;

/// Convert Typst math code to LaTeX (legacy function for compatibility)
///
/// # Arguments
/// * `input` - Typst math code (without $ delimiters)
///
/// # Returns
/// LaTeX math code
pub fn typst_to_latex(input: &str) -> String {
    typst_to_latex_with_options(input, &T2LOptions::math_only())
}

/// Convert Typst code to LaTeX with options
pub fn typst_to_latex_with_options(input: &str, options: &T2LOptions) -> String {
    let mut ctx = ConvertContext::new();
    ctx.options = options.clone();

    if options.math_only {
        // Parse as math only
        let root = parse_math(input);
        convert_math_node(&root, &mut ctx);
    } else {
        // Preprocess: extract #let definitions and expand variables
        let processed_input = preprocess::preprocess_typst(input);

        // Parse as full document/markup
        let root = parse(&processed_input);
        convert_markup_node(&root, &mut ctx);
    }

    let mut result = ctx.finalize();

    // Wrap in document if requested
    if options.full_document {
        result = wrap_in_document(&result, options);
    }

    result
}

/// Convert Typst document to LaTeX document
pub fn typst_document_to_latex(input: &str) -> String {
    typst_to_latex_with_options(input, &T2LOptions::full_document())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Math Mode Tests ==========

    #[test]
    fn test_simple_symbols() {
        let result = typst_to_latex("alpha + beta");
        assert!(result.contains("alpha"));
        assert!(result.contains("beta"));
    }

    #[test]
    fn test_fraction() {
        let result = typst_to_latex("frac(1, 2)");
        assert!(result.contains("\\frac"));
        assert!(result.contains("{1}"));
        assert!(result.contains("{2}"));
    }

    #[test]
    fn test_sqrt() {
        let result = typst_to_latex("sqrt(x)");
        assert!(result.contains("\\sqrt"));
    }

    #[test]
    fn test_subscript_superscript() {
        let result = typst_to_latex("x^2");
        assert!(result.contains("x"));
        assert!(result.contains("^"));
        assert!(result.contains("2"));
    }

    #[test]
    fn test_operators() {
        let result = typst_to_latex("a + b - c = d");
        assert!(result.contains("+"));
        assert!(result.contains("-"));
        assert!(result.contains("="));
    }

    // ========== Markup Mode Tests ==========

    #[test]
    fn test_heading() {
        let input = "= My Heading";
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\section"));
        assert!(result.contains("My Heading"));
    }

    #[test]
    fn test_bold() {
        let input = "*bold text*";
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\textbf"));
        assert!(result.contains("bold text"));
    }

    #[test]
    fn test_italic() {
        let input = "_italic text_";
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\textit"));
        assert!(result.contains("italic text"));
    }

    #[test]
    fn test_inline_code() {
        let input = "`code`";
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\texttt"));
    }

    #[test]
    fn test_inline_math() {
        let input = "The formula $x + y$ is simple.";
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("$"));
        assert!(result.contains("formula"));
    }

    #[test]
    fn test_full_document() {
        let input = "= Title\n\nSome text.";
        let result = typst_to_latex_with_options(input, &T2LOptions::full_document());
        assert!(result.contains("\\documentclass"));
        assert!(result.contains("\\begin{document}"));
        assert!(result.contains("\\end{document}"));
        assert!(result.contains("\\section"));
    }

    #[test]
    fn test_subsection() {
        let input = "== Subsection";
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\subsection"));
    }

    #[test]
    fn test_subsubsection() {
        let input = "=== Subsubsection";
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\subsubsection"));
    }

    // ========== Table Mode Tests ==========

    #[test]
    fn test_simple_table() {
        let input = r#"#table(
            columns: 3,
            [A], [B], [C],
            [1], [2], [3],
        )"#;
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\begin{tabular}"));
        assert!(result.contains("\\end{tabular}"));
        assert!(result.contains("A"));
        assert!(result.contains("1"));
    }

    #[test]
    fn test_table_with_colspan() {
        let input = r#"#table(
            columns: 3,
            table.cell(colspan: 2)[Header], [X],
            [1], [2], [3],
        )"#;
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\multicolumn{2}"));
        assert!(result.contains("Header"));
    }

    #[test]
    fn test_table_with_rowspan() {
        let input = r#"#table(
            columns: 3,
            table.cell(rowspan: 2)[Span], [A], [B],
            [C], [D],
        )"#;
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("\\multirow{2}"));
        assert!(result.contains("Span"));
    }

    #[test]
    fn test_table_with_align() {
        let input = r#"#table(
            columns: 3,
            align: (left, center, right),
            [A], [B], [C],
        )"#;
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        assert!(result.contains("|l|c|r|"));
    }

    #[test]
    fn test_table_with_linebreak_in_cell() {
        // Test Level 5 style cells with backslash linebreaks and rowspan/colspan
        let input = r#"#table(
            columns: 4,
            [Left],
            table.cell(colspan: 2, rowspan: 2)[
                *Big Red Box*
                \
                (Should have thick red border)
            ],
            [Right],
            [Bottom Left], 
            [Bottom Right],
        )"#;
        let result = typst_to_latex_with_options(input, &T2LOptions::default());
        println!("=== CONVERTED OUTPUT ===\n{}", result);
        assert!(result.contains("\\begin{tabular}"));
        assert!(result.contains("Big Red Box"));
        // Check that the linebreak doesn't break the table
        assert!(!result.contains("\\begin{center}"));
    }
}
