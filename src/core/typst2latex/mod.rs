//! Typst to LaTeX converter
//!
//! This module implements the Typst to LaTeX converter.
//! It uses `typst-syntax` to parse Typst code, and then converts the AST to LaTeX.

pub mod context;
pub mod engine;
mod markup;
mod math;
mod preprocess;
mod table;
mod utils;

pub use context::{ConvertContext, EnvironmentContext, T2LOptions, TokenType};
use typst_syntax::{parse, parse_math};

// Re-export specific items that were previously exposed by `eval` from core
pub use engine::{expand_macros, EvalError, EvalResult, MiniEval, SourceSpan, Value};

// Re-export preprocessing functions for backwards compatibility
pub use preprocess::{extract_let_definitions, preprocess_typst, TypstDefDb};

// =============================================================================
// Warning System (mirroring latex2typst's design)
// =============================================================================

/// Kind of warning generated during Typst to LaTeX conversion.
///
/// This enum provides type-safe classification of warnings, allowing callers
/// to programmatically handle different warning types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    /// An undefined variable was referenced
    UndefinedVariable,
    /// A type mismatch occurred during evaluation
    TypeMismatch,
    /// Division by zero was attempted
    DivisionByZero,
    /// An invalid operation was attempted
    InvalidOperation,
    /// Too many loop iterations (infinite loop protection triggered)
    TooManyIterations,
    /// A function argument error occurred
    ArgumentError,
    /// An index was out of bounds
    IndexOutOfBounds,
    /// A dictionary key was not found
    KeyNotFound,
    /// A syntax error in the source code
    SyntaxError,
    /// A file was not found during import
    FileNotFound,
    /// An import error occurred
    ImportError,
    /// A regex error occurred
    RegexError,
    /// Recursion limit exceeded (infinite recursion protection)
    RecursionLimitExceeded,
    /// General evaluation warning (from MiniEval)
    EvalWarning,
    /// Other/generic warning
    Other,
}

impl std::fmt::Display for WarningKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningKind::UndefinedVariable => write!(f, "undefined variable"),
            WarningKind::TypeMismatch => write!(f, "type mismatch"),
            WarningKind::DivisionByZero => write!(f, "division by zero"),
            WarningKind::InvalidOperation => write!(f, "invalid operation"),
            WarningKind::TooManyIterations => write!(f, "too many iterations"),
            WarningKind::ArgumentError => write!(f, "argument error"),
            WarningKind::IndexOutOfBounds => write!(f, "index out of bounds"),
            WarningKind::KeyNotFound => write!(f, "key not found"),
            WarningKind::SyntaxError => write!(f, "syntax error"),
            WarningKind::FileNotFound => write!(f, "file not found"),
            WarningKind::ImportError => write!(f, "import error"),
            WarningKind::RegexError => write!(f, "regex error"),
            WarningKind::RecursionLimitExceeded => write!(f, "recursion limit exceeded"),
            WarningKind::EvalWarning => write!(f, "eval warning"),
            WarningKind::Other => write!(f, "other"),
        }
    }
}

impl From<&engine::EvalErrorKind> for WarningKind {
    fn from(kind: &engine::EvalErrorKind) -> Self {
        use engine::EvalErrorKind;
        match kind {
            EvalErrorKind::UndefinedVariable(_) => WarningKind::UndefinedVariable,
            EvalErrorKind::TypeMismatch { .. } => WarningKind::TypeMismatch,
            EvalErrorKind::DivisionByZero => WarningKind::DivisionByZero,
            EvalErrorKind::InvalidOperation(_) => WarningKind::InvalidOperation,
            EvalErrorKind::TooManyIterations => WarningKind::TooManyIterations,
            EvalErrorKind::ArgumentError(_) => WarningKind::ArgumentError,
            EvalErrorKind::IndexOutOfBounds { .. } => WarningKind::IndexOutOfBounds,
            EvalErrorKind::KeyNotFound(_) => WarningKind::KeyNotFound,
            EvalErrorKind::SyntaxError(_) => WarningKind::SyntaxError,
            EvalErrorKind::FileNotFound(_) => WarningKind::FileNotFound,
            EvalErrorKind::ImportError(_) => WarningKind::ImportError,
            EvalErrorKind::RegexError(_) => WarningKind::RegexError,
            EvalErrorKind::RecursionLimitExceeded { .. } => WarningKind::RecursionLimitExceeded,
            EvalErrorKind::Other(_) => WarningKind::Other,
        }
    }
}

/// A warning generated during Typst to LaTeX conversion.
///
/// This structure provides type-safe warning classification with precise source
/// location information, allowing callers to handle different warning types
/// programmatically.
#[derive(Debug, Clone)]
pub struct ConversionWarning {
    /// The kind of warning (for programmatic handling)
    pub kind: WarningKind,
    /// Human-readable warning message
    pub message: String,
    /// Optional source span where the warning occurred (byte range in original source)
    pub span: Option<SourceSpan>,
}

impl ConversionWarning {
    /// Create a new warning with a kind and message.
    pub fn new(kind: WarningKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            span: None,
        }
    }

    /// Create a new warning with a kind, message, and source span.
    pub fn with_span(kind: WarningKind, message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            kind,
            message: message.into(),
            span: Some(span),
        }
    }

    /// Create a warning from a MiniEval error (for graceful degradation).
    pub fn from_eval_error(error: &EvalError) -> Self {
        let kind = WarningKind::from(&error.kind);
        let message = format!("{}", error);
        Self {
            kind,
            message,
            span: error.span,
        }
    }
}

impl From<engine::EvalWarning> for ConversionWarning {
    fn from(warning: engine::EvalWarning) -> Self {
        Self {
            kind: WarningKind::EvalWarning,
            message: warning.message,
            span: warning.span,
        }
    }
}

impl std::fmt::Display for ConversionWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(span) = &self.span {
            write!(
                f,
                "[{}] {}..{}: {}",
                self.kind, span.start, span.end, self.message
            )
        } else {
            write!(f, "[{}] {}", self.kind, self.message)
        }
    }
}

impl From<ConversionWarning> for crate::utils::error::CliDiagnostic {
    fn from(warning: ConversionWarning) -> Self {
        use crate::utils::error::{CliDiagnostic, DiagnosticSeverity};

        let severity = match warning.kind {
            WarningKind::UndefinedVariable
            | WarningKind::DivisionByZero
            | WarningKind::RecursionLimitExceeded => DiagnosticSeverity::Error,
            WarningKind::TypeMismatch
            | WarningKind::InvalidOperation
            | WarningKind::TooManyIterations
            | WarningKind::ArgumentError
            | WarningKind::SyntaxError => DiagnosticSeverity::Warning,
            _ => DiagnosticSeverity::Info,
        };

        let location = warning.span.map(|s| format!("{}..{}", s.start, s.end));

        let mut diag = CliDiagnostic::new(severity, warning.kind.to_string(), warning.message);
        if let Some(loc) = location {
            diag = diag.with_location(loc);
        }
        diag
    }
}

/// Result of a Typst to LaTeX conversion with diagnostics.
///
/// This structure provides the converted output along with any warnings
/// generated during conversion, allowing callers to handle diagnostics
/// programmatically rather than relying on stderr output.
#[derive(Debug, Clone)]
pub struct ConversionResult {
    /// The converted LaTeX output
    pub output: String,
    /// Warnings generated during conversion
    pub warnings: Vec<ConversionWarning>,
}

impl ConversionResult {
    /// Create a successful result with no warnings.
    pub fn ok(output: String) -> Self {
        Self {
            output,
            warnings: Vec::new(),
        }
    }

    /// Create a result with warnings.
    pub fn with_warnings(output: String, warnings: Vec<ConversionWarning>) -> Self {
        Self { output, warnings }
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Get warnings as formatted strings.
    pub fn format_warnings(&self) -> Vec<String> {
        self.warnings.iter().map(|w| w.to_string()).collect()
    }
}

/// Convert Typst code to LaTeX
pub fn typst_to_latex(input: &str) -> String {
    typst_to_latex_with_options(input, &T2LOptions::default())
}

/// Convert Typst code to LaTeX with options
pub fn typst_to_latex_with_options(input: &str, options: &T2LOptions) -> String {
    let mut ctx = ConvertContext::new();
    ctx.options = options.clone();

    // Preprocess: handle imports, etc.
    let processed_input = preprocess::preprocess_typst(input);

    if options.math_only {
        let root = parse_math(&processed_input);
        math::convert_math_node(&root, &mut ctx);
    } else {
        let root = parse(&processed_input);
        markup::convert_markup_node(&root, &mut ctx);
    }

    let mut result = ctx.finalize();

    if options.full_document {
        result = wrap_in_document(&result, options);
    }

    result
}

/// Convert Typst document to LaTeX document
pub fn typst_document_to_latex(input: &str) -> String {
    typst_to_latex_with_options(input, &T2LOptions::full_document())
}

/// Convert Typst code to LaTeX with MiniEval preprocessing, returning full diagnostics.
///
/// This is the **recommended** function for library/integration use. It returns a
/// structured `ConversionResult` containing both the output and any warnings,
/// allowing callers to handle diagnostics programmatically.
///
/// The function:
/// 1. Expands dynamic Typst features (loops, functions, variables) using MiniEval
/// 2. Falls back gracefully to simple preprocessing if MiniEval fails
/// 3. Converts the expanded markup to LaTeX
/// 4. Returns all warnings without printing to stderr
///
/// # Example
/// ```ignore
/// use tylax::core::typst2latex::{typst_to_latex_with_diagnostics, T2LOptions};
///
/// let input = r#"
/// #let fib(n) = if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
/// #for i in range(5) [F#i = #fib(i)]
/// "#;
/// let result = typst_to_latex_with_diagnostics(input, &T2LOptions::default());
/// println!("Output: {}", result.output);
/// for warning in result.warnings {
///     eprintln!("Warning: {}", warning);
/// }
/// ```
pub fn typst_to_latex_with_diagnostics(input: &str, options: &T2LOptions) -> ConversionResult {
    let mut warnings = Vec::new();

    // Step 1: Expand macros using MiniEval (with show rules applied)
    let expanded_input = match engine::expand_macros_with_warnings(input) {
        Ok(result) => {
            // Convert engine warnings to conversion warnings
            warnings.extend(result.warnings.into_iter().map(ConversionWarning::from));
            result.output
        }
        Err(e) => {
            // Graceful degradation: fall back to simple preprocessing
            warnings.push(ConversionWarning::from_eval_error(&e));
            preprocess::preprocess_typst(input)
        }
    };

    // Step 2: Convert the (possibly expanded) Typst to LaTeX
    let mut ctx = ConvertContext::new();
    ctx.options = options.clone();

    if options.math_only {
        let root = parse_math(&expanded_input);
        math::convert_math_node(&root, &mut ctx);
    } else {
        let root = parse(&expanded_input);
        markup::convert_markup_node(&root, &mut ctx);
    }

    let mut output = ctx.finalize();

    if options.full_document {
        output = wrap_in_document(&output, options);
    }

    ConversionResult::with_warnings(output, warnings)
}

/// Convert Typst code to LaTeX with MiniEval preprocessing.
///
/// This is a convenience wrapper around [`typst_to_latex_with_diagnostics`] that
/// prints warnings to stderr and returns only the output string. Suitable for
/// CLI tools and simple scripts.
///
/// **For library/integration use**, prefer [`typst_to_latex_with_diagnostics`]
/// which returns structured diagnostics without side effects.
///
/// # Features
/// - Expands `#let` with function definitions
/// - Expands `#for` loops and `#if` conditionals
/// - Evaluates array methods like `.map()`, `.filter()`
/// - Gracefully falls back on MiniEval errors
///
/// # Example
/// ```ignore
/// let input = r#"
/// #let fib(n) = if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
/// #for i in range(5) [F#i = #fib(i)]
/// "#;
/// let latex = typst_to_latex_with_eval(input, &T2LOptions::full_document());
/// ```
pub fn typst_to_latex_with_eval(input: &str, options: &T2LOptions) -> String {
    // Delegate to the diagnostics API
    let result = typst_to_latex_with_diagnostics(input, options);

    // Print warnings to stderr (preserving legacy behavior for CLI/script use)
    for warning in &result.warnings {
        eprintln!("[tylax] Warning: {}", warning);
    }

    result.output
}

fn wrap_in_document(content: &str, options: &T2LOptions) -> String {
    let mut doc = String::new();

    // Document class
    let doc_class = if options.document_class.is_empty() {
        "article"
    } else {
        &options.document_class
    };
    doc.push_str(&format!("\\documentclass{{{}}}\n", doc_class));

    // Standard packages
    doc.push_str("\\usepackage[utf8]{inputenc}\n");
    doc.push_str("\\usepackage{amsmath}\n");
    doc.push_str("\\usepackage{amssymb}\n");
    doc.push_str("\\usepackage{graphicx}\n");
    doc.push_str("\\usepackage{hyperref}\n");
    doc.push_str("\\usepackage{xcolor}\n");
    doc.push_str("\\usepackage{longtable}\n"); // For tables
    doc.push_str("\\usepackage{booktabs}\n"); // For better tables
    doc.push_str("\\usepackage{geometry}\n");
    doc.push_str("\\geometry{a4paper, margin=2cm}\n");

    // Title and author
    if let Some(ref title) = options.title {
        doc.push_str(&format!("\\title{{{}}}\n", title));
    }
    if let Some(ref author) = options.author {
        doc.push_str(&format!("\\author{{{}}}\n", author));
    }

    doc.push('\n');
    doc.push_str("\\begin{document}\n\n");

    // Include \maketitle if title is set
    if options.title.is_some() {
        doc.push_str("\\maketitle\n\n");
    }

    doc.push_str(content);

    doc.push_str("\n\n\\end{document}");

    doc
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_conversion() {
        let typst = "Hello *world*!";
        let latex = typst_to_latex(typst);
        assert_eq!(latex.trim(), "Hello \\textbf{world}!");
    }

    // ========================================================================
    // Diagnostics API Tests
    // ========================================================================

    #[test]
    fn test_diagnostics_basic_success() {
        // Basic conversion should succeed with no warnings
        let result = typst_to_latex_with_diagnostics("Hello *world*!", &T2LOptions::default());
        assert!(
            result.output.contains("\\textbf{world}"),
            "Output should contain bold: {}",
            result.output
        );
        // Simple content should have no warnings
        assert!(
            !result.has_warnings(),
            "Should have no warnings for simple content"
        );
    }

    #[test]
    fn test_diagnostics_with_eval() {
        // Test that MiniEval expansion works through diagnostics API
        let input = r#"
#let x = 5
#x
"#;
        let result = typst_to_latex_with_diagnostics(input, &T2LOptions::default());
        assert!(
            result.output.contains("5"),
            "Output should contain expanded value: {}",
            result.output
        );
    }

    #[test]
    fn test_diagnostics_graceful_degradation() {
        // Test that invalid code triggers graceful degradation with warning
        // Note: This tests the error path - an undefined variable should produce a warning
        let input = r#"#undefined_variable"#;
        let result = typst_to_latex_with_diagnostics(input, &T2LOptions::default());

        // Should have at least one warning about the error
        assert!(
            result.has_warnings(),
            "Should have warnings for undefined variable"
        );
        // Should still produce some output (graceful degradation)
        assert!(
            !result.output.is_empty(),
            "Should still produce output on error"
        );
    }

    #[test]
    fn test_diagnostics_wrapper_equivalence() {
        // Test that the wrapper produces the same output as the diagnostics API
        let input = r#"
#let double(x) = x * 2
#double(5)
"#;
        let options = T2LOptions::default();
        let diagnostics_result = typst_to_latex_with_diagnostics(input, &options);
        let wrapper_result = typst_to_latex_with_eval(input, &options);

        assert_eq!(
            diagnostics_result.output, wrapper_result,
            "Wrapper should produce same output as diagnostics API"
        );
    }

    #[test]
    fn test_conversion_warning_display() {
        // Test ConversionWarning Display implementation
        let warning_no_span = ConversionWarning::new(WarningKind::Other, "Test warning");
        assert_eq!(warning_no_span.to_string(), "[other] Test warning");

        let warning_with_span = ConversionWarning::with_span(
            WarningKind::SyntaxError,
            "Test warning",
            SourceSpan::new(10, 20),
        );
        assert_eq!(
            warning_with_span.to_string(),
            "[syntax error] 10..20: Test warning"
        );
    }

    #[test]
    fn test_warning_kind_from_eval_error() {
        // Test that WarningKind correctly maps from EvalErrorKind
        use engine::EvalErrorKind;

        assert_eq!(
            WarningKind::from(&EvalErrorKind::UndefinedVariable("x".to_string())),
            WarningKind::UndefinedVariable
        );
        assert_eq!(
            WarningKind::from(&EvalErrorKind::DivisionByZero),
            WarningKind::DivisionByZero
        );
        assert_eq!(
            WarningKind::from(&EvalErrorKind::SyntaxError("test".to_string())),
            WarningKind::SyntaxError
        );
    }

    #[test]
    fn test_graceful_degradation_warning_kind() {
        // Test that undefined variable produces the correct WarningKind
        let input = r#"#undefined_variable"#;
        let result = typst_to_latex_with_diagnostics(input, &T2LOptions::default());

        assert!(result.has_warnings(), "Should have warnings");
        let warning = &result.warnings[0];
        assert_eq!(
            warning.kind,
            WarningKind::UndefinedVariable,
            "Warning should be UndefinedVariable, got: {:?}",
            warning.kind
        );
    }

    #[test]
    fn test_conversion_result_helpers() {
        // Test ConversionResult helper methods
        let ok_result = ConversionResult::ok("output".to_string());
        assert!(!ok_result.has_warnings());
        assert!(ok_result.format_warnings().is_empty());

        let warning = ConversionWarning::new(WarningKind::Other, "test");
        let warned_result = ConversionResult::with_warnings("output".to_string(), vec![warning]);
        assert!(warned_result.has_warnings());
        assert_eq!(warned_result.format_warnings().len(), 1);
    }

    #[test]
    fn test_diagnostics_for_loop() {
        // Test that for loops are expanded correctly
        // Use explicit newlines in content block for clearer output
        let input = r#"#for i in range(3) [
#i
]"#;
        let result = typst_to_latex_with_diagnostics(input, &T2LOptions::default());

        // Should contain 0, 1, 2
        assert!(
            result.output.contains("0"),
            "Should have 0: {}",
            result.output
        );
        assert!(
            result.output.contains("1"),
            "Should have 1: {}",
            result.output
        );
        assert!(
            result.output.contains("2"),
            "Should have 2: {}",
            result.output
        );
    }

    #[test]
    fn test_diagnostics_conditional() {
        // Test that conditionals are expanded correctly
        let input = r#"#if true [yes] else [no]"#;
        let result = typst_to_latex_with_diagnostics(input, &T2LOptions::default());

        assert!(
            result.output.contains("yes"),
            "Should have yes: {}",
            result.output
        );
        assert!(
            !result.output.contains("no"),
            "Should NOT have no: {}",
            result.output
        );
    }

    #[test]
    fn test_dot_operator_conversion() {
        // Test that `dot` in math mode converts to \cdot, not .
        let opts = T2LOptions {
            math_only: true,
            ..Default::default()
        };
        let result = typst_to_latex_with_options("x dot y", &opts);
        eprintln!("dot conversion result: {}", result);

        assert!(
            result.contains("\\cdot") || result.contains("cdot"),
            "Expected \\cdot for dot operator, got: {}",
            result
        );
        // Should NOT be a literal period
        assert!(
            !result.contains("x . y") && !result.contains("x.y"),
            "Should not have literal period, got: {}",
            result
        );
    }

    #[test]
    fn test_for_loop_list_not_nested() {
        // Test that for loop with list items produces flat list, not nested
        let input = r#"#for x in ("A", "B", "C") [
- #x
]"#;
        let result = typst_to_latex_with_diagnostics(input, &T2LOptions::default());
        eprintln!("For loop list result:\n{}", result.output);

        // Should have exactly one itemize, not nested
        let itemize_count = result.output.matches("\\begin{itemize}").count();
        assert_eq!(
            itemize_count, 1,
            "Expected 1 itemize, got {}: {}",
            itemize_count, result.output
        );

        // Should have 3 items
        let item_count = result.output.matches("\\item").count();
        assert_eq!(
            item_count, 3,
            "Expected 3 items, got {}: {}",
            item_count, result.output
        );
    }
}
