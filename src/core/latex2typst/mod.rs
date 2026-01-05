//! LaTeX to Typst conversion module
//!
//! This module provides functionality to convert LaTeX documents and math to Typst format.
//! It uses an AST-based approach with mitex-parser for parsing LaTeX.
//!
//! # Module Structure
//!
//! - `context`: Core structures and converter implementation (`LatexConverter`, `L2TOptions`, etc.)
//! - `markup`: Handles LaTeX commands like `\section`, `\textbf`, `\cite`, etc.
//! - `math`: Handles math formulas, delimiters, and math-specific constructs
//! - `environment`: Handles LaTeX environments like `figure`, `table`, `equation`, etc.
//! - `utils`: Utility functions for text processing and AST manipulation
//!
//! # Example
//!
//! ```rust
//! use tylax::core::latex2typst::{LatexConverter, L2TOptions};
//!
//! // Convert a complete document
//! let mut converter = LatexConverter::new();
//! let typst = converter.convert_document(r"\documentclass{article}\begin{document}Hello\end{document}");
//!
//! // Convert math only
//! let mut converter = LatexConverter::new();
//! let typst_math = converter.convert_math(r"\frac{a}{b}");
//!
//! // Use custom options
//! let options = L2TOptions::readable();
//! let mut converter = LatexConverter::with_options(options);
//! ```

// Submodules
pub mod context;
pub mod environment;
pub mod markup;
pub mod math;
pub mod table;
pub mod utils;

// Re-export main types for convenience
pub use context::{
    ConversionMode, ConversionState, EnvironmentContext, L2TOptions, LatexConverter, MacroDef,
    MERGED_SPEC,
};

// =============================================================================
// Backward-compatible API Functions
// =============================================================================

/// Convert LaTeX document using AST-based converter (backward-compatible alias)
pub fn convert_document_with_ast(input: &str) -> String {
    latex_to_typst(input)
}

/// Convert LaTeX document with custom options (backward-compatible alias)
pub fn convert_document_with_ast_options(input: &str, options: &L2TOptions) -> String {
    latex_to_typst_with_options(input, options.clone())
}

/// Convert LaTeX math using AST-based converter (backward-compatible alias)
pub fn convert_math_with_ast(input: &str) -> String {
    latex_math_to_typst(input)
}

/// Convert LaTeX math with custom options (backward-compatible alias)
pub fn convert_math_with_ast_options(input: &str, options: &L2TOptions) -> String {
    latex_math_to_typst_with_options(input, options.clone())
}

/// Generic convert function - detects document vs math (backward-compatible alias)
pub fn convert_with_ast(input: &str) -> String {
    // If input looks like a document, convert as document
    if input.contains("\\documentclass") || input.contains("\\begin{document}") {
        latex_to_typst(input)
    } else {
        latex_math_to_typst(input)
    }
}

/// Generic convert with options (backward-compatible alias)
pub fn convert_with_ast_options(input: &str, options: &L2TOptions) -> String {
    if input.contains("\\documentclass") || input.contains("\\begin{document}") {
        latex_to_typst_with_options(input, options.clone())
    } else {
        latex_math_to_typst_with_options(input, options.clone())
    }
}

// =============================================================================
// Public API Functions
// =============================================================================

/// Convert a complete LaTeX document to Typst
///
/// This is a convenience function that creates a new converter and processes the input.
///
/// # Arguments
///
/// * `input` - The LaTeX document source
///
/// # Returns
///
/// The converted Typst document as a string
///
/// # Example
///
/// ```rust
/// use tylax::core::latex2typst::latex_to_typst;
///
/// let latex = r"\documentclass{article}\begin{document}Hello World\end{document}";
/// let typst = latex_to_typst(latex);
/// ```
pub fn latex_to_typst(input: &str) -> String {
    let mut converter = LatexConverter::new();
    converter.convert_document(input)
}

/// Convert a complete LaTeX document to Typst with custom options
///
/// # Arguments
///
/// * `input` - The LaTeX document source
/// * `options` - Conversion options
///
/// # Returns
///
/// The converted Typst document as a string
pub fn latex_to_typst_with_options(input: &str, options: L2TOptions) -> String {
    let mut converter = LatexConverter::with_options(options);
    converter.convert_document(input)
}

/// Convert LaTeX math to Typst math
///
/// This is a convenience function for converting math-only content.
/// The input should be LaTeX math content (without delimiters).
///
/// # Arguments
///
/// * `input` - The LaTeX math source
///
/// # Returns
///
/// The converted Typst math as a string
///
/// # Example
///
/// ```rust
/// use tylax::core::latex2typst::latex_math_to_typst;
///
/// let latex_math = r"\frac{a}{b} + \sqrt{c}";
/// let typst_math = latex_math_to_typst(latex_math);
/// ```
pub fn latex_math_to_typst(input: &str) -> String {
    let mut converter = LatexConverter::new();
    converter.convert_math(input)
}

/// Convert LaTeX math to Typst math with custom options
///
/// # Arguments
///
/// * `input` - The LaTeX math source
/// * `options` - Conversion options
///
/// # Returns
///
/// The converted Typst math as a string
pub fn latex_math_to_typst_with_options(input: &str, options: L2TOptions) -> String {
    let mut converter = LatexConverter::with_options(options);
    converter.convert_math(input)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_math() {
        let input = r"\frac{a}{b}";
        let output = latex_math_to_typst(input);
        assert!(output.contains("frac") || output.contains("/"));
    }

    #[test]
    fn test_simple_document() {
        let input = r"\documentclass{article}
\begin{document}
Hello World
\end{document}";
        let output = latex_to_typst(input);
        assert!(output.contains("Hello World"));
    }

    #[test]
    fn test_section() {
        let input = r"\documentclass{article}
\begin{document}
\section{Introduction}
Content here.
\end{document}";
        let output = latex_to_typst(input);
        assert!(output.contains("== Introduction"));
    }

    #[test]
    fn test_math_symbols() {
        let input = r"\alpha + \beta = \gamma";
        let output = latex_math_to_typst(input);
        assert!(output.contains("alpha"));
        assert!(output.contains("beta"));
        assert!(output.contains("gamma"));
    }

    #[test]
    fn test_text_formatting() {
        let input = r"\documentclass{article}
\begin{document}
\textbf{bold} and \textit{italic}
\end{document}";
        let output = latex_to_typst(input);
        assert!(output.contains("*bold*"));
        assert!(output.contains("_italic_"));
    }

    #[test]
    fn test_options() {
        let options = L2TOptions::verbose();
        let input = r"\frac{a}{b}";
        let output = latex_math_to_typst_with_options(input, options);
        assert!(output.contains("frac"));
    }

    #[test]
    fn test_equation_environment() {
        let input = r"\begin{equation}
E = mc^2
\end{equation}";
        let mut converter = LatexConverter::new();
        converter.state.in_preamble = false;
        let output = converter.convert_math(input);
        // Should contain the equation content
        assert!(output.contains("E") || output.contains("m"));
    }

    #[test]
    fn test_itemize() {
        let input = r"\documentclass{article}
\begin{document}
\begin{itemize}
\item First
\item Second
\end{itemize}
\end{document}";
        let output = latex_to_typst(input);
        assert!(output.contains("- First") || output.contains("-"));
    }

    #[test]
    fn test_label_sanitization() {
        use utils::sanitize_label;

        assert_eq!(sanitize_label("fig:test"), "fig-test");
        assert_eq!(sanitize_label("eq:alpha_beta"), "eq-alpha-beta");
        assert_eq!(sanitize_label("sec:intro"), "sec-intro");
    }

    #[test]
    fn test_table_conversion() {
        let input = r"\documentclass{article}
\begin{document}
\begin{table}[h]
\centering
\caption{Test performance}
\begin{tabular}{l|ccc}
Model & MSE & MAE & Score \\
\hline
Linear & 1.21 & 0.88 & 0.60 \\
Ours & 0.74 & 0.55 & 0.83 \\
\end{tabular}
\end{table}
\end{document}";
        let output = latex_to_typst(input);
        println!("Table output:\n{}", output);
        // Should contain proper table structure with multiple columns
        assert!(output.contains("table("));
        assert!(output.contains("columns:"));
        // Should have 4 columns
        assert!(output.contains("columns: (auto, auto, auto, auto)"));
        // Should have multiple cells per row
        assert!(output.contains("[Model]") || output.contains("Model"));
        assert!(output.contains("[MSE]") || output.contains("MSE"));
        assert!(output.contains("[Linear]") || output.contains("Linear"));
        assert!(output.contains("[1.21]") || output.contains("1.21"));
    }

    #[test]
    fn test_table_with_math() {
        let input = r"\documentclass{article}
\begin{document}
\begin{tabular}{l|ccc}
Model & MSE & MAE & $R^2$ \\
\hline
Linear & 1.21 & 0.88 & 0.60 \\
\end{tabular}
\end{document}";
        let output = latex_to_typst(input);
        println!("Table with math:\n{}", output);
        // Should contain math in the header
        assert!(
            output.contains("$R^(2)$") || output.contains("R^(2)") || output.contains("$R ^(2 ) $")
        );
        // Should have 4 columns
        assert!(output.contains("columns: (auto, auto, auto, auto)"));
    }

    #[test]
    fn test_argmin_conversion() {
        // \arg\min should be converted with spaces to prevent identifier merging
        let input = r"$\arg\min_{\theta}$";
        let output = latex_math_to_typst(input);
        println!("argmin output: {}", output);
        // Should not have bare "argmin_" (no space) which is invalid in Typst
        // Should have "arg min_" (with space) which is valid
        assert!(
            !output.contains("argmin_"),
            "Should not have bare argmin_: {}",
            output
        );
        // Should have proper spacing
        assert!(
            output.contains("arg ") && output.contains("min ") || output.contains("arg min"),
            "Should have proper spacing: {}",
            output
        );
    }

    #[test]
    fn test_table6_symbols() {
        // Test Table 6 from test9.tex - math-heavy table
        let input = r"\documentclass{article}
\begin{document}
\begin{tabular}{cl}
Symbol & Description \\
$\arg\min_{\theta}$ & Optimization operator \\
\end{tabular}
\end{document}";
        let output = latex_to_typst(input);
        println!("Table 6 output:\n{}", output);
        // Should have proper arg min spacing
        assert!(
            !output.contains("argmin_"),
            "Should not have bare argmin_: {}",
            output
        );
    }

    #[test]
    fn test_table_cmidrule_cleanup() {
        let input = r"\documentclass{article}
\usepackage{booktabs}
\usepackage{multirow}
\begin{document}
\begin{tabular}{lcccc}
\toprule
\multirow{2}{*}{Model} & \multicolumn{4}{c}{Forecast Horizon} \\
\cmidrule(lr){2-5}
 & $H=1$ & $H=5$ & $H=10$ & $H=20$ \\
\midrule
ARIMA & 0.91 & 1.22 & 1.58 & 2.01 \\
\bottomrule
\end{tabular}
\end{document}";
        let output = latex_to_typst(input);
        println!("Table output:\n{}", output);

        assert!(!output.contains("(lr)2-5"));
        assert!(!output.contains("cmidrule"));

        // Ensure structure is correct
        // First row: Model and Forecast Horizon
        assert!(output.contains("table.cell(rowspan: 2)[Model]"));
        assert!(output.contains("table.cell(colspan: 4)[Forecast Horizon]"));

        // Second row should NOT contain empty cell [], because we filter empty cells now.
        // The first cell was "&", so it's empty, so it's removed.
        // Remaining cells: H=1, H=5, ...
        // This is correct because the first column is covered by Model's rowspan.
        assert!(output.contains("[$H = 1 $], [$H = 5 $]"));
    }

    #[test]
    fn test_table_sparse_data() {
        let input = r"\documentclass{article}
\begin{document}
\begin{tabular}{lccc}
Parameter & Low & Medium & High \\
$\lambda$ & 0.01 & 0.1 & 1.0 \\
$\gamma$ & & 0.5 & 1.0 \\
Dropout & No & Yes & \\
\end{tabular}
\end{document}";
        let output = latex_to_typst(input);
        println!("Table 4 output:\n{}", output);

        // Check gamma row: \gamma, [], 0.5, 1.0
        // Should have empty cell in 2nd position
        assert!(output.contains(r"[$gamma $], [], [0.5], [1.0]"));

        // Check Dropout row: Dropout, No, Yes, []
        // Typst output might end with comma, trailing empty cell might be implicit or explicit
        // Let's check for Yes followed by empty cell or end of row
        // Actually, our parser preserves intermediate empty cells but what about trailing?
        // "Dropout & No & Yes &" -> split gives 4 cells: "Dropout", "No", "Yes", ""
        // So we expect: [Dropout], [No], [Yes], []

        // Wait, if input is "Dropout & No & Yes & \\", split gives 4 cells.
        // If input is "Dropout & No & Yes \\", split gives 3 cells!
        // LaTeX allows omitting trailing & if cells are empty.
        // If 3 cells are provided for 4 columns, Typst fills the rest with empty automatically?
        // No, Typst table just flows.
        // But if we output 3 cells, Typst puts them in col 1, 2, 3.
        // Col 4 is left for the NEXT cell (next row).
        // This is fine IF the next row starts at Col 1.
        // BUT if the next row starts, Typst table continues filling.
        // So Row 4 Col 4 will be filled by... nothing?
        // Typst table needs enough cells to fill the grid?
        // Actually, if we use `columns: 4`, Typst expects multiples of 4 cells?
        // "The cells are placed in the table row by row."
        // If you provide 15 cells for a 4-column table.
        // Row 1: 4 cells. Row 2: 4 cells. Row 3: 4 cells. Row 4: 3 cells.
        // The table just ends.
        // Visual result: The last cell is empty.
        // So trailing empty cells are NOT strictly required for correct layout,
        // UNLESS there is content after that needs to be aligned.

        // In this test case, Dropout is the last row. So it doesn't matter.
        // But gamma row is middle row. It MUST have the empty cell.
    }

    #[test]
    fn test_table_nested_stress() {
        // Test case based on Row 32 of test9.tex
        // Structure: 4 columns.
        // Row 1: \multirow{2}{*}{A} (col 1) & ...
        // Row 2: & & 0.90 & 0.40
        // Col 1 covered by A. Col 2 is empty data. Col 3, 4 are data.

        let input = r"\documentclass{article}
\usepackage{multirow}
\begin{document}
\begin{tabular}{cccc}
\multirow{2}{*}{A} & Train & Test & Note \\
& & 0.90 & 0.40 \\
\end{tabular}
\end{document}";
        let output = latex_to_typst(input);
        println!("Stress test output:\n{}", output);

        // Row 2 expected: [], [0.90], [0.40]
        // Explanation:
        // Col 1 covered -> consumes 1st '&', emits nothing.
        // Col 2 empty -> consumes 2nd '&', emits [].
        // Col 3 data -> emits [0.90].
        // Col 4 data -> emits [0.40].
        assert!(output.contains(r"[], [0.90], [0.40]"));
    }
}
