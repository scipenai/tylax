//! LaTeX to Typst converter
//!
//! This module implements the AST-based LaTeX to Typst converter.
//! It uses `mitex-parser` to parse LaTeX into an AST, and then traverses
//! the AST to generate Typst code.

pub mod context;
pub mod engine;
mod environment;
mod markup;
mod math;
mod table;
mod utils;

pub use context::{
    ConversionMode, ConversionState, EnvironmentContext, L2TOptions, LatexConverter, PreambleMode,
    MERGED_SPEC,
};

// =============================================================================
// Warning System
// =============================================================================

/// Kind of warning generated during conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind {
    /// An unknown macro was encountered and passed through unchanged
    UnsupportedMacro,
    /// A macro was only partially expanded (e.g., missing arguments)
    PartialExpansion,
    /// An unsupported TeX primitive was encountered (\catcode, \scantokens, etc.)
    UnsupportedPrimitive,
    /// A LaTeX3/expl3 block was skipped
    LaTeX3Skipped,
    /// Delimited argument pattern did not match input
    PatternMismatch,
    /// Argument parsing ran away (no delimiter found)
    RunawayArgument,
    /// Infinite recursion detected
    MacroLoop,
    /// General parsing or conversion issue
    ParseError,
}

impl std::fmt::Display for WarningKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningKind::UnsupportedMacro => write!(f, "unsupported macro"),
            WarningKind::PartialExpansion => write!(f, "partial expansion"),
            WarningKind::UnsupportedPrimitive => write!(f, "unsupported primitive"),
            WarningKind::LaTeX3Skipped => write!(f, "LaTeX3 skipped"),
            WarningKind::PatternMismatch => write!(f, "pattern mismatch"),
            WarningKind::RunawayArgument => write!(f, "runaway argument"),
            WarningKind::MacroLoop => write!(f, "macro loop"),
            WarningKind::ParseError => write!(f, "parse error"),
        }
    }
}

/// A warning generated during LaTeX to Typst conversion
#[derive(Debug, Clone)]
pub struct ConversionWarning {
    /// The kind of warning
    pub kind: WarningKind,
    /// Human-readable message
    pub message: String,
    /// Location context (e.g., "\\foo" or "line 42")
    pub location: Option<String>,
}

impl ConversionWarning {
    /// Create a new warning
    pub fn new(kind: WarningKind, message: impl Into<String>) -> Self {
        ConversionWarning {
            kind,
            message: message.into(),
            location: None,
        }
    }

    /// Add location context to the warning
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Create an unsupported macro warning
    pub fn unsupported_macro(name: &str) -> Self {
        ConversionWarning::new(
            WarningKind::UnsupportedMacro,
            format!("Unknown macro '{}' passed through unchanged", name),
        )
        .with_location(name.to_string())
    }

    /// Create an unsupported primitive warning
    pub fn unsupported_primitive(name: &str) -> Self {
        ConversionWarning::new(
            WarningKind::UnsupportedPrimitive,
            format!(
                "Primitive '{}' is not supported and may produce incorrect output",
                name
            ),
        )
        .with_location(name.to_string())
    }

    /// Create a LaTeX3 skipped warning
    pub fn latex3_skipped(start: &str, end: &str) -> Self {
        ConversionWarning::new(
            WarningKind::LaTeX3Skipped,
            format!("LaTeX3 block ({} ... {}) was skipped", start, end),
        )
    }

    /// Create a pattern mismatch warning
    pub fn pattern_mismatch(macro_name: &str) -> Self {
        ConversionWarning::new(
            WarningKind::PatternMismatch,
            format!("Argument pattern for '{}' did not match input", macro_name),
        )
        .with_location(macro_name.to_string())
    }

    /// Create a runaway argument warning
    pub fn runaway_argument(macro_name: &str) -> Self {
        ConversionWarning::new(
            WarningKind::RunawayArgument,
            format!(
                "Runaway argument while parsing '{}' (missing delimiter?)",
                macro_name
            ),
        )
        .with_location(macro_name.to_string())
    }

    /// Create a macro loop warning
    pub fn macro_loop(macro_name: &str) -> Self {
        ConversionWarning::new(
            WarningKind::MacroLoop,
            format!("Infinite recursion detected in macro '{}'", macro_name),
        )
        .with_location(macro_name.to_string())
    }

    /// Create a parse error warning
    pub fn parse_error(msg: impl Into<String>) -> Self {
        ConversionWarning::new(WarningKind::ParseError, msg)
    }
}

impl std::fmt::Display for ConversionWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref loc) = self.location {
            write!(f, "[{}] {}: {}", self.kind, loc, self.message)
        } else {
            write!(f, "[{}] {}", self.kind, self.message)
        }
    }
}

impl From<ConversionWarning> for crate::utils::error::CliDiagnostic {
    fn from(warning: ConversionWarning) -> Self {
        use crate::utils::error::{CliDiagnostic, DiagnosticSeverity};

        let severity = match warning.kind {
            WarningKind::MacroLoop | WarningKind::RunawayArgument => DiagnosticSeverity::Error,
            WarningKind::UnsupportedMacro
            | WarningKind::PartialExpansion
            | WarningKind::PatternMismatch
            | WarningKind::ParseError => DiagnosticSeverity::Warning,
            WarningKind::UnsupportedPrimitive | WarningKind::LaTeX3Skipped => {
                DiagnosticSeverity::Info
            }
        };

        let mut diag = CliDiagnostic::new(severity, warning.kind.to_string(), warning.message);
        if let Some(loc) = warning.location {
            diag = diag.with_location(loc);
        }
        diag
    }
}

/// Result of conversion with diagnostics
#[derive(Debug, Clone)]
pub struct ConversionResult {
    /// The converted output
    pub output: String,
    /// Warnings generated during conversion
    pub warnings: Vec<ConversionWarning>,
}

impl ConversionResult {
    /// Create a new result with no warnings
    pub fn ok(output: String) -> Self {
        ConversionResult {
            output,
            warnings: Vec::new(),
        }
    }

    /// Create a result with warnings
    pub fn with_warnings(output: String, warnings: Vec<ConversionWarning>) -> Self {
        ConversionResult { output, warnings }
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Get warnings as formatted strings
    pub fn format_warnings(&self) -> Vec<String> {
        self.warnings.iter().map(|w| w.to_string()).collect()
    }
}

/// Convert LaTeX document to Typst
pub fn latex_to_typst(input: &str) -> String {
    let mut converter = LatexConverter::new();
    converter.convert_document(input)
}

/// Convert LaTeX math equation to Typst
pub fn latex_math_to_typst(input: &str) -> String {
    let mut converter = LatexConverter::new();
    converter.convert_math(input)
}

// Legacy wrappers for compatibility
pub fn convert_with_ast(input: &str) -> String {
    latex_to_typst(input)
}

pub fn convert_with_ast_options(input: &str, options: L2TOptions) -> String {
    let mut converter = LatexConverter::with_options(options);
    converter.convert_document(input)
}

pub fn convert_document_with_ast(input: &str) -> String {
    latex_to_typst(input)
}

pub fn convert_document_with_ast_options(input: &str, options: L2TOptions) -> String {
    let mut converter = LatexConverter::with_options(options);
    converter.convert_document(input)
}

pub fn convert_math_with_ast(input: &str) -> String {
    latex_math_to_typst(input)
}

pub fn convert_math_with_ast_options(input: &str, options: L2TOptions) -> String {
    let mut converter = LatexConverter::with_options(options);
    converter.convert_math(input)
}

/// Convert LaTeX document to Typst with macro expansion
///
/// This explicitly enables macro expansion (using the token-based engine).
/// Macros defined with `\newcommand`, `\def`, etc. are expanded before conversion.
///
/// # Example
///
/// ```
/// use tylax::core::latex2typst::latex_to_typst_with_eval;
///
/// let input = r"\newcommand{\R}{\mathbb{R}} $x \in \R$";
/// let output = latex_to_typst_with_eval(input);
/// // The macro \R is expanded to \mathbb{R} before conversion
/// ```
pub fn latex_to_typst_with_eval(input: &str) -> String {
    let options = L2TOptions {
        expand_macros: true,
        ..Default::default()
    };
    let mut converter = LatexConverter::with_options(options);
    converter.convert_document(input)
}

/// Convert LaTeX math to Typst with macro expansion
///
/// This explicitly enables macro expansion (using the token-based engine).
/// Math mode is enabled in the expansion engine, affecting `\ifmmode` conditionals.
///
/// # Example
///
/// ```
/// use tylax::core::latex2typst::latex_math_to_typst_with_eval;
///
/// let input = r"\newcommand{\R}{\mathbb{R}} x \in \R";
/// let output = latex_math_to_typst_with_eval(input);
/// // The macro \R is expanded to \mathbb{R} before conversion
/// ```
pub fn latex_math_to_typst_with_eval(input: &str) -> String {
    let options = L2TOptions {
        expand_macros: true,
        ..Default::default()
    };
    let mut converter = LatexConverter::with_options(options);
    converter.convert_math(input)
}

/// Convert LaTeX to Typst with full diagnostics
///
/// Returns both the converted output and any warnings generated during conversion.
/// This is the recommended function for applications that need to report conversion issues.
///
/// # Example
///
/// ```
/// use tylax::core::latex2typst::latex_to_typst_with_diagnostics;
///
/// let result = latex_to_typst_with_diagnostics(r"\documentclass{article}\begin{document}Hello\end{document}");
/// println!("Output: {}", result.output);
/// for warning in result.warnings {
///     eprintln!("Warning: {}", warning);
/// }
/// ```
pub fn latex_to_typst_with_diagnostics(input: &str) -> ConversionResult {
    let mut converter = LatexConverter::new();
    converter.convert_document_with_diagnostics(input)
}

/// Convert LaTeX to Typst with full diagnostics and custom options
pub fn latex_to_typst_with_diagnostics_options(
    input: &str,
    options: L2TOptions,
) -> ConversionResult {
    let mut converter = LatexConverter::with_options(options);
    converter.convert_document_with_diagnostics(input)
}

/// Convert LaTeX math to Typst with full diagnostics
pub fn latex_math_to_typst_with_diagnostics(input: &str) -> ConversionResult {
    let mut converter = LatexConverter::new();
    converter.convert_math_with_diagnostics(input)
}
