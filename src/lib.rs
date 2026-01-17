//! # tylax
//!
//! High-performance bidirectional LaTeX ↔ Typst converter written in Rust.
//!
//! ## Features
//!
//! - **High Performance**: AST-based parsing engine built on Rust
//! - **Bidirectional**: Supports both LaTeX → Typst and Typst → LaTeX
//! - **Full Document**: Converts complete documents including headings, lists, tables
//! - **Rich Symbol Set**: 700+ symbol mappings
//! - **WASM Support**: Compiles to WebAssembly for browser usage
//! - **Table Support**: Full table conversion with multicolumn/multirow
//! - **Reference System**: Complete citation and cross-reference support
//! - **Macro Expansion**: Basic LaTeX macro definition and expansion
//!
//! ## Usage Examples
//!
//! ### Math Formula Conversion
//!
//! ```rust
//! use tylax::{latex_to_typst, typst_to_latex};
//!
//! // LaTeX → Typst
//! let typst = latex_to_typst(r"\frac{1}{2}");
//! assert!(typst.contains("frac") || typst.contains("/"));
//!
//! // Typst → LaTeX  
//! let latex = typst_to_latex("$frac(1, 2)$");
//! assert!(latex.contains(r"\frac"));
//! ```
//!
//! ### Full Document Conversion
//!
//! ```rust
//! use tylax::{latex_document_to_typst, typst_document_to_latex};
//!
//! let typst = latex_document_to_typst(r#"
//!     \documentclass{article}
//!     \title{My Paper}
//!     \begin{document}
//!     \section{Introduction}
//!     Hello, world!
//!     \end{document}
//! "#);
//!
//! let latex = typst_document_to_latex(r#"
//!     = Introduction
//!     
//!     Hello, *world*!
//! "#);
//! ```

/// Core conversion modules
pub mod core;

/// Data layer - static mappings and constants
pub mod data;

/// Feature modules - advanced conversion features
pub mod features;

/// Utility modules
pub mod utils;

/// WASM bindings (feature-gated)
#[cfg(feature = "wasm")]
pub mod wasm;

// Re-export core conversion functions
pub use core::typst2latex;
pub use core::typst2latex::T2LOptions;
pub use core::typst2latex::{
    typst_document_to_latex, typst_to_latex, typst_to_latex_with_diagnostics,
    typst_to_latex_with_eval, typst_to_latex_with_options, ConversionResult as T2LConversionResult,
};

pub use core::latex2typst::{
    convert_document_with_ast, convert_document_with_ast_options, convert_math_with_ast,
    convert_math_with_ast_options, convert_with_ast, convert_with_ast_options,
    latex_math_to_typst_with_diagnostics, latex_math_to_typst_with_eval,
    latex_to_typst_with_diagnostics, latex_to_typst_with_eval, ConversionMode,
    ConversionResult as L2TConversionResult, ConversionState, EnvironmentContext, L2TOptions,
    LatexConverter, WarningKind,
};

// Re-export data modules
pub use data::constants;
pub use data::maps;

// Re-export feature modules
pub use features::bibtex;
pub use features::images;
pub use features::refs;
pub use features::tables;
pub use features::templates;
pub use features::tikz;

// Re-export symbol data
pub use data::colors;
pub use data::extended_symbols;
pub use data::siunitx;
pub use data::symbols;

// Re-export utilities
pub use utils::diagnostics;
pub use utils::error::{
    CliDiagnostic, ConversionError, ConversionOutput, ConversionResult, ConversionWarning,
    DiagnosticSeverity,
};
pub use utils::files;

// Re-export main types and functions from eval (MiniEval) - now located in typst2latex
pub use core::typst2latex::engine::{
    self, expand_macros, ContentNode, EvalError, EvalResult, MiniEval, Value,
};

/// Convert LaTeX math code to Typst math code
///
/// # Arguments
/// * `input` - LaTeX math code
///
/// # Returns
/// Typst math code
pub fn latex_to_typst(input: &str) -> String {
    convert_math_with_ast(input)
}

/// Convert LaTeX math code to Typst math code with custom options
///
/// # Arguments
/// * `input` - LaTeX math code
/// * `options` - Conversion options
///
/// # Returns
/// Typst math code
pub fn latex_to_typst_with_options(input: &str, options: &L2TOptions) -> String {
    convert_math_with_ast_options(input, options.clone())
}

/// Convert a complete LaTeX document to Typst
pub fn latex_document_to_typst(input: &str) -> String {
    convert_document_with_ast(input)
}

/// Convert a complete LaTeX document to Typst with custom options
pub fn latex_document_to_typst_with_options(input: &str, options: &L2TOptions) -> String {
    convert_document_with_ast_options(input, options.clone())
}

/// Convert with automatic direction detection
///
/// Detects whether the input is LaTeX or Typst and converts accordingly.
/// Uses heuristics based on command patterns to determine the format.
pub fn convert_auto(input: &str) -> (String, &'static str) {
    // Heuristic: if input contains backslash commands, it's likely LaTeX
    let is_latex = input.contains('\\')
        && (input.contains("\\frac")
            || input.contains("\\alpha")
            || input.contains("\\sum")
            || input.contains("\\int")
            || input.contains("\\begin")
            || input.contains("\\section")
            || input.contains("\\documentclass"));

    if is_latex {
        (latex_to_typst(input), "typst")
    } else {
        (typst_to_latex(input), "latex")
    }
}

/// Convert with automatic direction detection for full documents
pub fn convert_auto_document(input: &str) -> (String, &'static str) {
    let is_latex = input.contains("\\documentclass")
        || input.contains("\\begin{document}")
        || (input.contains('\\') && (input.contains("\\section") || input.contains("\\chapter")));

    let is_typst = input.contains("#set")
        || input.contains("#show")
        || input.starts_with('=')
        || input.contains("\n=");

    if is_latex && !is_typst {
        (latex_document_to_typst(input), "typst")
    } else if is_typst && !is_latex {
        (typst_document_to_latex(input), "latex")
    } else if is_latex {
        (latex_document_to_typst(input), "typst")
    } else {
        (typst_document_to_latex(input), "latex")
    }
}

/// Detect input format
///
/// Returns "latex", "typst", or "unknown" based on content analysis.
pub fn detect_format(input: &str) -> &'static str {
    // Strong LaTeX indicators
    let latex_score: i32 = if input.contains("\\documentclass") {
        10
    } else {
        0
    } + if input.contains("\\begin{document}") {
        10
    } else {
        0
    } + if input.contains("\\section") { 5 } else { 0 }
        + if input.contains("\\frac") { 3 } else { 0 }
        + if input.contains("\\alpha") { 2 } else { 0 }
        + if input.contains("\\\\") { 2 } else { 0 }
        + (input.matches('\\').count() as i32);

    // Strong Typst indicators
    let typst_score: i32 = if input.contains("#set") { 10 } else { 0 }
        + if input.contains("#show") { 10 } else { 0 }
        + if input.contains("#import") { 8 } else { 0 }
        + if input.starts_with('=') { 5 } else { 0 }
        + if input.contains("\n= ") { 5 } else { 0 }
        + if input.contains("frac(") { 3 } else { 0 }
        + if input.contains("sqrt(") { 3 } else { 0 };

    if latex_score > typst_score + 3 {
        "latex"
    } else if typst_score > latex_score + 3 {
        "typst"
    } else if latex_score > 0 {
        "latex"
    } else if typst_score > 0 {
        "typst"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latex_to_typst_basic() {
        let result = latex_to_typst(r"\alpha + \beta");
        // AST converter outputs Unicode greek letters by default
        assert!(result.contains("alpha") || result.contains("α"));
        assert!(result.contains("beta") || result.contains("β"));
    }

    #[test]
    fn test_latex_to_typst_frac() {
        let result = latex_to_typst(r"\frac{1}{2}");
        // With frac_to_slash enabled by default, simple fractions use slash notation
        assert!(result.contains("/") || result.contains("frac"));
    }

    #[test]
    fn test_typst_to_latex_basic() {
        let result = typst_to_latex("alpha + beta");
        assert!(result.contains("alpha"));
        assert!(result.contains("beta"));
    }

    #[test]
    fn test_typst_to_latex_frac() {
        let result = typst_to_latex("frac(1, 2)");
        assert!(result.contains("frac"));
    }

    #[test]
    fn test_convert_auto_latex() {
        let (result, format) = convert_auto(r"\frac{1}{2}");
        assert_eq!(format, "typst");
        // With frac_to_slash enabled by default, simple fractions use slash notation
        assert!(result.contains("/") || result.contains("frac"));
    }

    #[test]
    fn test_convert_auto_typst() {
        let (result, format) = convert_auto("alpha + beta");
        assert_eq!(format, "latex");
        assert!(result.contains("alpha"));
    }

    #[test]
    fn test_detect_format_latex() {
        assert_eq!(detect_format(r"\documentclass{article}"), "latex");
        assert_eq!(detect_format(r"\frac{1}{2}"), "latex");
        assert_eq!(detect_format(r"\begin{document}"), "latex");
    }

    #[test]
    fn test_detect_format_typst() {
        assert_eq!(detect_format("#set page(paper: \"a4\")"), "typst");
        assert_eq!(detect_format("= Heading"), "typst");
        assert_eq!(detect_format("#import \"test.typ\""), "typst");
    }

    #[test]
    fn test_document_conversion_typst() {
        let input = "= Hello\n\nWorld!";
        let result = typst_document_to_latex(input);
        assert!(result.contains("section"));
    }

    #[test]
    fn test_l2t_options_prefer_shorthands() {
        // With shorthands enabled (default)
        let opts_short = L2TOptions {
            prefer_shorthands: true,
            ..Default::default()
        };
        let result_short = latex_to_typst_with_options(r"\rightarrow", &opts_short);
        assert!(result_short.contains("->") || result_short.contains("arrow.r"));

        // With shorthands disabled
        let opts_long = L2TOptions {
            prefer_shorthands: false,
            ..Default::default()
        };
        let result_long = latex_to_typst_with_options(r"\rightarrow", &opts_long);
        assert!(result_long.contains("arrow.r"));
    }

    #[test]
    fn test_l2t_options_infty_to_oo() {
        // With infty_to_oo disabled (default)
        let opts_default = L2TOptions {
            infty_to_oo: false,
            ..Default::default()
        };
        let result_default = latex_to_typst_with_options(r"\infty", &opts_default);
        assert!(result_default.contains("infinity"));

        // With infty_to_oo enabled
        let opts_oo = L2TOptions {
            infty_to_oo: true,
            ..Default::default()
        };
        let result_oo = latex_to_typst_with_options(r"\infty", &opts_oo);
        assert!(result_oo.contains("oo"));
    }

    #[test]
    fn test_l2t_options_frac_to_slash() {
        // With frac_to_slash enabled (default) - simple fraction
        let opts_slash = L2TOptions {
            frac_to_slash: true,
            ..Default::default()
        };
        let result_slash = latex_to_typst_with_options(r"\frac{a}{b}", &opts_slash);
        assert!(result_slash.contains("/") || result_slash.contains("frac"));

        // With frac_to_slash disabled
        let opts_frac = L2TOptions {
            frac_to_slash: false,
            ..Default::default()
        };
        let result_frac = latex_to_typst_with_options(r"\frac{a}{b}", &opts_frac);
        assert!(result_frac.contains("frac("));
    }

    #[test]
    fn test_l2t_options_preset_readable() {
        let opts = L2TOptions::readable();
        assert!(opts.prefer_shorthands);
        assert!(opts.frac_to_slash);
        assert!(opts.infty_to_oo);
    }

    #[test]
    fn test_l2t_options_preset_verbose() {
        let opts = L2TOptions::verbose();
        assert!(!opts.prefer_shorthands);
        assert!(!opts.frac_to_slash);
        assert!(!opts.infty_to_oo);
    }

    #[test]
    fn test_t2l_options_block_math_mode() {
        // Default: block math mode
        let opts_block = T2LOptions {
            block_math_mode: true,
            math_only: true,
            ..Default::default()
        };
        let result_block = typst_to_latex_with_options("display(sum)", &opts_block);
        // In block mode, display() just outputs \displaystyle without restore
        assert!(result_block.contains("displaystyle"));

        // Inline math mode
        let opts_inline = T2LOptions {
            block_math_mode: false,
            math_only: true,
            ..Default::default()
        };
        let result_inline = typst_to_latex_with_options("display(sum)", &opts_inline);
        // In inline mode, display() outputs \displaystyle and restores to \textstyle
        assert!(result_inline.contains("displaystyle"));
    }

    #[test]
    fn test_ifmmode_nested_full_conversion() {
        // Test the full L2T conversion with nested \ifmmode macros
        // This is the EXACT pattern from the user's test document
        let input = r#"\documentclass{article}
\usepackage{amsmath}

\newcommand{\RR}{\mathbb{R}}
\newcommand{\norm}[1]{\left\lVert #1 \right\rVert}
\newcommand{\inner}[2]{\langle #1, #2 \rangle}
\newcommand{\strong}[1]{\ifmmode \mathbf{#1} \else \textbf{#1} \fi}
\newcommand{\xvec}{\strong{x}}

\begin{document}
\section{Test}
Text: \xvec.

Math: $\norm{\xvec} = \sqrt{\inner{\xvec}{\xvec}}$
\end{document}
"#;
        let result = latex_document_to_typst(input);
        eprintln!("Full conversion result:\n{}", result);

        // In math mode ($...$), \xvec should expand to \mathbf{x}
        // which should become bold(...) or upright(bold(...)), NOT *x*
        // *x* in math mode would be multiplication!

        // Check that the math section contains bold(x) not *x*
        let math_section = result.split("Math:").nth(1).unwrap_or("");
        eprintln!("Math section: {}", math_section);

        assert!(
            !math_section.contains("*x*"),
            "Math section should not have *x* (which is multiplication in Typst math), got: {}",
            math_section
        );
        assert!(
            math_section.contains("bold(x)") || math_section.contains("bold(x"),
            "Math section should have bold(x), got: {}",
            math_section
        );
    }

    #[test]
    fn test_ifmmode_bracket_display_math_full() {
        // Test the full L2T conversion with \[...\] display math
        // This is the EXACT pattern that was failing in the user's document
        let input = r#"\documentclass{article}
\usepackage{amsmath}

\newcommand{\norm}[1]{\left\lVert #1 \right\rVert}
\newcommand{\inner}[2]{\langle #1, #2 \rangle}
\newcommand{\strong}[1]{\ifmmode \mathbf{#1} \else \textbf{#1} \fi}
\newcommand{\xvec}{\strong{x}}

\begin{document}
\section{Test}

\[
    \norm{\xvec} = \sqrt{\inner{\xvec}{\xvec}}
\]
\end{document}
"#;
        let result = latex_document_to_typst(input);
        eprintln!("Bracket display math result:\n{}", result);

        // In \[...\] display math, \xvec should expand to \mathbf{x}
        // which should become bold(...), NOT *x*
        assert!(
            !result.contains("*x*"),
            "Should not have *x* in result (would be multiplication in Typst math), got: {}",
            result
        );
        assert!(
            result.contains("bold(x)"),
            "Should have bold(x) in display math, got: {}",
            result
        );
    }

    #[test]
    fn test_langle_rangle_in_sqrt() {
        // Test that \langle x, y \rangle inside \sqrt doesn't break
        // The comma should not be parsed as a function argument separator
        let input = r#"\sqrt{\langle x, y \rangle}"#;
        let result = latex_to_typst(input);
        eprintln!("langle in sqrt result: {}", result);

        // Should have {} wrapper around the content to protect the comma
        // sqrt({angle.l x, y angle.r}) instead of sqrt(angle.l x, y angle.r)
        assert!(
            result.contains("sqrt({"),
            "Should have sqrt({{...}}) wrapper to protect comma, got: {}",
            result
        );
        assert!(
            result.contains("chevron.l"),
            "Should have chevron.l, got: {}",
            result
        );
        assert!(
            result.contains("chevron.r"),
            "Should have chevron.r, got: {}",
            result
        );
    }

    #[test]
    fn test_sqrt_without_comma_no_braces() {
        // When no comma, should not have extra braces
        let input = r#"\sqrt{x + y}"#;
        let result = latex_to_typst(input);
        eprintln!("sqrt without comma result: {}", result);

        // Should NOT have {} wrapper since no comma
        assert!(
            !result.contains("sqrt({"),
            "Should not have extra braces when no comma, got: {}",
            result
        );
        assert!(
            result.contains("sqrt("),
            "Should have sqrt(...), got: {}",
            result
        );
    }
}
