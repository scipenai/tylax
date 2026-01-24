//! TeX Macro Expansion Engine
//!
//! This module provides a token-stream-based TeX macro expansion engine
//! that correctly handles nested braces, recursive macro expansion, and
//! standard LaTeX macro definitions.
//!
//! ## Architecture
//!
//! ```text
//! Input String → Lexer → Token Stream → Expander → Expanded Tokens → Detokenizer → Output String
//! ```
//!
//! ## Components
//!
//! - `token`: Token type definitions
//! - `lexer`: Tokenizer that converts strings to tokens
//! - `engine`: The macro expansion VM
//! - `primitives`: Parsing for `\newcommand`, `\def`, etc.

#[allow(clippy::module_inception)]
pub mod engine;
pub mod lexer;
pub mod primitives;
pub mod token;
pub mod utils;

pub use engine::{Engine, MacroDb, MacroDef};
pub use lexer::{detokenize, tokenize, Lexer};
pub use primitives::{parse_definitions, DefinitionKind};
pub use token::{TexToken, TokenList};

// =============================================================================
// Structured Engine Warnings
// =============================================================================

/// Structured warning type from the macro expansion engine.
///
/// This enum provides type-safe warning information that can be
/// converted to user-facing messages without string parsing.
#[derive(Debug, Clone)]
pub enum EngineWarning {
    /// Macro expansion depth exceeded the safety limit
    DepthExceeded { max_depth: usize },
    /// Total token count exceeded the safety limit
    TokenLimitExceeded { max_tokens: usize },
    /// Argument parsing failed for a macro or environment
    ArgumentParsingFailed {
        macro_name: String,
        error_kind: ArgumentErrorType,
    },
    /// LaTeX3/expl3 syntax block was skipped
    LaTeX3Skipped { token_count: usize },
    /// An unsupported TeX primitive was encountered
    UnsupportedPrimitive { name: String },
    /// \let target not found in macro database
    LetTargetNotFound { name: String, target: String },
}

/// Type of argument parsing error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentErrorType {
    RunawayArgument,
    PatternMismatch,
    Other(String),
}

impl std::fmt::Display for ArgumentErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RunawayArgument => write!(f, "Runaway argument"),
            Self::PatternMismatch => write!(f, "Pattern mismatch"),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl EngineWarning {
    /// Convert to a human-readable message
    pub fn message(&self) -> String {
        match self {
            EngineWarning::DepthExceeded { max_depth } => {
                format!(
                    "Macro expansion depth exceeded maximum ({}). Possible infinite recursion.",
                    max_depth
                )
            }
            EngineWarning::TokenLimitExceeded { max_tokens } => {
                format!(
                    "Macro expansion produced too many tokens (exceeded {}). Possible infinite loop or exponential expansion.",
                    max_tokens
                )
            }
            EngineWarning::ArgumentParsingFailed {
                macro_name,
                error_kind,
            } => {
                format!(
                    "Macro '\\{}' argument parsing failed: {}",
                    macro_name, error_kind
                )
            }
            EngineWarning::LaTeX3Skipped { token_count } => {
                format!(
                    "LaTeX3 block (\\ExplSyntaxOn ... \\ExplSyntaxOff) skipped ({} tokens). \
                    LaTeX3/expl3 syntax is not supported.",
                    token_count
                )
            }
            EngineWarning::UnsupportedPrimitive { name } => {
                format!(
                    "Unsupported TeX primitive '\\{}' encountered. \
                    This may produce incorrect output.",
                    name
                )
            }
            EngineWarning::LetTargetNotFound { name, target } => {
                format!(
                    "\\let\\{}\\{}: target '\\{}' not found in macro database. \
                    Built-in LaTeX commands cannot be copied with \\let.",
                    name, target, target
                )
            }
        }
    }
}

impl std::fmt::Display for EngineWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

/// Result of macro expansion with diagnostics
#[derive(Debug, Clone)]
pub struct ExpandResult {
    /// The expanded LaTeX string
    pub output: String,
    /// Structured warnings collected during expansion
    pub warnings: Vec<EngineWarning>,
}

/// Main entry point: expand all macros in a LaTeX string
///
/// This function:
/// 1. Tokenizes the input
/// 2. Extracts macro definitions (`\newcommand`, `\def`, etc.)
/// 3. Expands all macro invocations
/// 4. Detokenizes back to a string
///
/// # Example
///
/// ```
/// use tylax::core::latex2typst::engine::expand_latex;
///
/// let input = r"\newcommand{\pair}[2]{\langle #1, #2\rangle} \pair{a}{b}";
/// let output = expand_latex(input);
/// assert!(output.contains(r"\langle a, b\rangle"));
/// ```
pub fn expand_latex(input: &str) -> String {
    let mut engine = Engine::new();

    // Tokenize
    let tokens = tokenize(input);

    // Parse definitions and expand
    let expanded = engine.process(tokens);

    // Detokenize
    detokenize(&expanded)
}

/// Expand macros with full diagnostics
///
/// This function provides the same functionality as `expand_latex` but also
/// returns any warnings that were generated during expansion.
///
/// # Arguments
/// * `input` - The LaTeX source code
/// * `math_mode` - Whether to enable math mode (affects `\ifmmode` conditionals)
///
/// # Example
///
/// ```
/// use tylax::core::latex2typst::engine::expand_latex_with_warnings;
///
/// let result = expand_latex_with_warnings(r"\newcommand{\x}{y} \x", false);
/// assert!(result.output.contains("y"));
/// assert!(result.warnings.is_empty()); // No warnings for valid expansion
/// ```
pub fn expand_latex_with_warnings(input: &str, math_mode: bool) -> ExpandResult {
    let mut engine = if math_mode {
        Engine::new_math_mode()
    } else {
        Engine::new()
    };

    // Tokenize
    let tokens = tokenize(input);

    // Parse definitions and expand
    let expanded = engine.process(tokens);

    // Collect structured warnings
    let warnings = engine.take_structured_warnings();

    // Always use the detokenized output from the engine.
    // DO NOT return original input on error - it contains \newcommand/\renewcommand
    // which causes MiTeX parser to hang!
    let output = detokenize(&expanded);

    ExpandResult { output, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_latex_simple() {
        let input = r"\newcommand{\hello}{world} \hello";
        let output = expand_latex(input);
        assert!(output.contains("world"));
        assert!(!output.contains(r"\hello"));
    }

    #[test]
    fn test_expand_latex_with_args() {
        let input = r"\newcommand{\pair}[2]{\langle #1, #2\rangle} \pair{a}{b}";
        let output = expand_latex(input);
        assert!(output.contains(r"\langle a, b\rangle"));
    }

    #[test]
    fn test_expand_latex_nested_braces() {
        let input = r"\newcommand{\pair}[2]{\langle #1, #2\rangle} \pair{a^2}{\frac{\pi}{2}}";
        let output = expand_latex(input);
        // The key test: nested braces should be preserved
        assert!(output.contains(r"\langle a^2, \frac{\pi}{2}\rangle"));
    }

    #[test]
    fn test_expand_latex_recursive() {
        let input = r"\newcommand{\double}[1]{#1#1} \double{x}";
        let output = expand_latex(input);
        assert!(output.contains("xx"));
    }

    #[test]
    fn test_expand_latex_with_warnings_success() {
        let input = r"\newcommand{\x}{y} \x";
        let result = expand_latex_with_warnings(input, false);
        assert!(result.output.contains("y"));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_expand_latex_with_warnings_math_mode() {
        // Test \ifmmode conditional
        let input = r"\newcommand{\x}{\ifmmode MATH\else TEXT\fi} \x";

        // Text mode
        let result_text = expand_latex_with_warnings(input, false);
        assert!(
            result_text.output.contains("TEXT"),
            "Expected TEXT in: {}",
            result_text.output
        );

        // Math mode
        let result_math = expand_latex_with_warnings(input, true);
        assert!(
            result_math.output.contains("MATH"),
            "Expected MATH in: {}",
            result_math.output
        );
    }

    #[test]
    fn test_expand_latex_blackboard_bold() {
        // Test common math macro pattern
        let input = r"\newcommand{\R}{\mathbb{R}} \R";
        let output = expand_latex(input);
        assert!(
            output.contains(r"\mathbb{R}"),
            "Expected \\mathbb{{R}} in: {}",
            output
        );
    }
}
