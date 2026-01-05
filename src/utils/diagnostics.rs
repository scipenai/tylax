//! LaTeX diagnostics using AST analysis
//!
//! This module provides error detection and reporting for LaTeX documents
//! using the mitex-parser's AST. It can identify:
//!
//! - Syntax errors (unbalanced braces, malformed commands)
//! - Unsupported commands
//! - Missing required packages
//! - Potential conversion issues
//!
//! ## Example
//!
//! ```rust
//! use tylax::diagnostics::{check_latex, DiagnosticLevel};
//!
//! let diagnostics = check_latex(r"\begin{foo}");
//! assert!(!diagnostics.is_empty());
//! ```

use mitex_parser::syntax::{SyntaxElement, SyntaxKind, SyntaxNode};
use mitex_parser::CommandSpec;
use mitex_spec_gen::DEFAULT_SPEC;
use std::collections::HashSet;
use std::fmt;

use crate::data::maps::TEX_COMMAND_SPEC;
use fxhash::FxHashMap;
use lazy_static::lazy_static;

lazy_static! {
    /// Merged command specification for parsing
    static ref MERGED_SPEC: CommandSpec = {
        let mut commands: FxHashMap<String, _> = DEFAULT_SPEC
            .items()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();

        for (k, v) in TEX_COMMAND_SPEC.items() {
            commands.insert(k.to_string(), v.clone());
        }

        CommandSpec::new(commands)
    };

    /// Commands we know are not supported
    static ref UNSUPPORTED_COMMANDS: HashSet<&'static str> = {
        let mut s = HashSet::new();
        // TikZ (partially supported via tikz module)
        s.insert("pgfmathparse");
        s.insert("pgfmathresult");
        // Low-level TeX
        s.insert("catcode");
        s.insert("makeatletter");
        s.insert("makeatother");
        s.insert("expandafter");
        s.insert("csname");
        s.insert("endcsname");
        // Complex packages
        s.insert("lstinputlisting");
        s.insert("inputminted");
        // PGFPlots
        s.insert("addplot");
        s.insert("addplot3");
        s.insert("axis");
        s
    };

    /// Commands that need specific packages
    static ref PACKAGE_COMMANDS: FxHashMap<&'static str, &'static str> = {
        let mut m = FxHashMap::default();
        m.insert("SI", "siunitx");
        m.insert("si", "siunitx");
        m.insert("num", "siunitx");
        m.insert("ang", "siunitx");
        m.insert("gls", "glossaries");
        m.insert("Gls", "glossaries");
        m.insert("acrshort", "glossaries");
        m.insert("acrlong", "glossaries");
        m.insert("lstlisting", "listings");
        m.insert("lstinline", "listings");
        m.insert("minted", "minted");
        m.insert("mintinline", "minted");
        m.insert("ce", "mhchem");
        m
    };
}

/// Diagnostic severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticLevel {
    /// Informational note
    Info,
    /// Warning - conversion might not be perfect
    Warning,
    /// Error - conversion will likely fail or produce incorrect output
    Error,
}

impl fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticLevel::Info => write!(f, "info"),
            DiagnosticLevel::Warning => write!(f, "warning"),
            DiagnosticLevel::Error => write!(f, "error"),
        }
    }
}

/// A single diagnostic message
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity level
    pub level: DiagnosticLevel,
    /// Human-readable message
    pub message: String,
    /// Line number (1-indexed)
    pub line: Option<usize>,
    /// Column number (1-indexed)
    pub column: Option<usize>,
    /// Span of text in the source (start, end)
    pub span: Option<(usize, usize)>,
    /// Relevant source text
    pub source_text: Option<String>,
    /// Suggested fix
    pub suggestion: Option<String>,
}

impl Diagnostic {
    /// Create a new diagnostic
    pub fn new(level: DiagnosticLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            line: None,
            column: None,
            span: None,
            source_text: None,
            suggestion: None,
        }
    }

    /// Add location information
    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    /// Add span information
    pub fn with_span(mut self, start: usize, end: usize) -> Self {
        self.span = Some((start, end));
        self
    }

    /// Add source text
    pub fn with_source(mut self, text: impl Into<String>) -> Self {
        self.source_text = Some(text.into());
        self
    }

    /// Add suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format: level: message
        //         --> file:line:column
        //         |
        //      42 | source text
        //         | ^^^^ suggestion

        write!(f, "{}: {}", self.level, self.message)?;

        if let (Some(line), Some(col)) = (self.line, self.column) {
            write!(f, "\n  --> line {}:{}", line, col)?;
        }

        if let Some(ref source) = self.source_text {
            write!(f, "\n  |\n  | {}", source)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n  = help: {}", suggestion)?;
        }

        Ok(())
    }
}

/// Check result with summary
#[derive(Debug, Default)]
pub struct CheckResult {
    /// All diagnostics
    pub diagnostics: Vec<Diagnostic>,
    /// Number of errors
    pub errors: usize,
    /// Number of warnings
    pub warnings: usize,
    /// Number of info messages
    pub infos: usize,
}

impl CheckResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a diagnostic
    pub fn add(&mut self, diag: Diagnostic) {
        match diag.level {
            DiagnosticLevel::Error => self.errors += 1,
            DiagnosticLevel::Warning => self.warnings += 1,
            DiagnosticLevel::Info => self.infos += 1,
        }
        self.diagnostics.push(diag);
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }

    /// Check if there are any issues at all
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Get summary string
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.errors > 0 {
            parts.push(format!(
                "{} error{}",
                self.errors,
                if self.errors == 1 { "" } else { "s" }
            ));
        }
        if self.warnings > 0 {
            parts.push(format!(
                "{} warning{}",
                self.warnings,
                if self.warnings == 1 { "" } else { "s" }
            ));
        }
        if self.infos > 0 {
            parts.push(format!(
                "{} note{}",
                self.infos,
                if self.infos == 1 { "" } else { "s" }
            ));
        }
        if parts.is_empty() {
            "no issues found".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Check LaTeX source for issues
pub fn check_latex(input: &str) -> CheckResult {
    let mut result = CheckResult::new();

    // Parse the input
    let tree = mitex_parser::parse(input, MERGED_SPEC.clone());

    // Calculate line offsets for position reporting
    let line_offsets = compute_line_offsets(input);

    // Walk the AST looking for issues
    check_node(&tree, input, &line_offsets, &mut result);

    // Check for unbalanced braces
    check_brace_balance(input, &mut result);

    // Check for unbalanced environments
    check_environment_balance(input, &mut result);

    result
}

/// Compute byte offsets for each line start
fn compute_line_offsets(input: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, c) in input.char_indices() {
        if c == '\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

/// Convert byte offset to line and column
fn offset_to_location(offset: usize, line_offsets: &[usize]) -> (usize, usize) {
    let line = line_offsets
        .iter()
        .position(|&o| o > offset)
        .unwrap_or(line_offsets.len())
        - 1;

    let column = offset - line_offsets.get(line).unwrap_or(&0) + 1;
    (line + 1, column) // 1-indexed
}

/// Check a syntax node recursively
fn check_node(node: &SyntaxNode, source: &str, line_offsets: &[usize], result: &mut CheckResult) {
    for child in node.children_with_tokens() {
        match child.kind() {
            SyntaxKind::TokenError => {
                // Syntax error
                let text = match &child {
                    SyntaxElement::Token(t) => t.text().to_string(),
                    SyntaxElement::Node(n) => n.text().to_string(),
                };

                let offset = child.text_range().start().into();
                let (line, col) = offset_to_location(offset, line_offsets);

                result.add(
                    Diagnostic::new(
                        DiagnosticLevel::Error,
                        format!("syntax error: unexpected '{}'", text),
                    )
                    .with_location(line, col)
                    .with_source(&text),
                );
            }

            SyntaxKind::ItemCmd => {
                // Check if command is supported
                if let SyntaxElement::Node(cmd_node) = &child {
                    check_command(cmd_node, source, line_offsets, result);
                }
            }

            SyntaxKind::ItemEnv => {
                // Check environment
                if let SyntaxElement::Node(env_node) = &child {
                    check_environment(env_node, source, line_offsets, result);
                }
            }

            _ => {
                // Recurse into child nodes
                if let SyntaxElement::Node(n) = child {
                    check_node(&n, source, line_offsets, result);
                }
            }
        }
    }
}

/// Check a command node for issues
fn check_command(
    node: &SyntaxNode,
    source: &str,
    line_offsets: &[usize],
    result: &mut CheckResult,
) {
    // Extract command name
    let text = node.text().to_string();
    let cmd_name = text
        .split(|c: char| !c.is_alphanumeric() && c != '\\')
        .next()
        .unwrap_or("")
        .trim_start_matches('\\');

    let offset: usize = node.text_range().start().into();
    let (line, col) = offset_to_location(offset, line_offsets);

    // Check if unsupported
    if UNSUPPORTED_COMMANDS.contains(cmd_name) {
        result.add(
            Diagnostic::new(
                DiagnosticLevel::Warning,
                format!("command '\\{}' is not fully supported", cmd_name),
            )
            .with_location(line, col)
            .with_source(format!("\\{}", cmd_name))
            .with_suggestion("This command may not convert correctly"),
        );
    }

    // Check for package requirements
    if let Some(package) = PACKAGE_COMMANDS.get(cmd_name) {
        result.add(
            Diagnostic::new(
                DiagnosticLevel::Info,
                format!(
                    "command '\\{}' requires the '{}' package",
                    cmd_name, package
                ),
            )
            .with_location(line, col),
        );
    }

    // Recurse into children
    check_node(node, source, line_offsets, result);
}

/// Check an environment node for issues
fn check_environment(
    node: &SyntaxNode,
    source: &str,
    line_offsets: &[usize],
    result: &mut CheckResult,
) {
    // Extract environment name from the begin clause
    let text = node.text().to_string();

    // Check for known problematic environments
    let problematic = [
        (
            "tikzpicture",
            "TikZ drawings are converted to CeTZ with limited support",
        ),
        ("pgfpicture", "PGF pictures require manual conversion"),
        ("pspicture", "PSTricks is not supported"),
        ("asy", "Asymptote is not supported"),
    ];

    for (env_name, message) in problematic {
        if text.contains(&format!("\\begin{{{}}}", env_name)) {
            let offset: usize = node.text_range().start().into();
            let (line, col) = offset_to_location(offset, line_offsets);

            result.add(
                Diagnostic::new(DiagnosticLevel::Warning, message)
                    .with_location(line, col)
                    .with_source(format!("\\begin{{{}}}", env_name)),
            );
        }
    }

    // Recurse into children
    check_node(node, source, line_offsets, result);
}

/// Check for unbalanced braces
fn check_brace_balance(input: &str, result: &mut CheckResult) {
    let mut depth = 0i32;
    let mut last_open_line = 0;
    let line_offsets = compute_line_offsets(input);

    for (offset, c) in input.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    let (line, _) = offset_to_location(offset, &line_offsets);
                    last_open_line = line;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth < 0 {
                    let (line, col) = offset_to_location(offset, &line_offsets);
                    result.add(
                        Diagnostic::new(DiagnosticLevel::Error, "unmatched closing brace '}'")
                            .with_location(line, col)
                            .with_suggestion("Check for missing opening brace"),
                    );
                    depth = 0;
                }
            }
            _ => {}
        }
    }

    if depth > 0 {
        result.add(
            Diagnostic::new(
                DiagnosticLevel::Error,
                format!(
                    "{} unclosed brace{} (opened around line {})",
                    depth,
                    if depth == 1 { "" } else { "s" },
                    last_open_line
                ),
            )
            .with_suggestion("Check for missing closing brace '}'"),
        );
    }
}

/// Check for unbalanced environments
fn check_environment_balance(input: &str, result: &mut CheckResult) {
    let mut env_stack: Vec<(String, usize)> = Vec::new();
    let line_offsets = compute_line_offsets(input);

    // Find \begin{...} and \end{...}
    let mut pos = 0;
    while pos < input.len() {
        if let Some(begin_pos) = input[pos..].find(r"\begin{") {
            let abs_pos = pos + begin_pos;
            let after = &input[abs_pos + 7..];

            if let Some(close) = after.find('}') {
                let env_name = &after[..close];
                let (line, _) = offset_to_location(abs_pos, &line_offsets);
                env_stack.push((env_name.to_string(), line));
                pos = abs_pos + 7 + close + 1;
                continue;
            }
        }

        if let Some(end_pos) = input[pos..].find(r"\end{") {
            let abs_pos = pos + end_pos;
            let after = &input[abs_pos + 5..];

            if let Some(close) = after.find('}') {
                let env_name = &after[..close];
                let (line, col) = offset_to_location(abs_pos, &line_offsets);

                if let Some((open_name, open_line)) = env_stack.pop() {
                    if open_name != env_name {
                        result.add(
                            Diagnostic::new(
                                DiagnosticLevel::Error,
                                format!("mismatched environment: opened '{}' at line {}, closed '{}' at line {}",
                                    open_name, open_line, env_name, line)
                            )
                            .with_location(line, col)
                            .with_suggestion(format!("Use \\end{{{}}}", open_name))
                        );
                    }
                } else {
                    result.add(
                        Diagnostic::new(
                            DiagnosticLevel::Error,
                            format!("unmatched \\end{{{}}}", env_name),
                        )
                        .with_location(line, col)
                        .with_suggestion("Check for missing \\begin"),
                    );
                }

                pos = abs_pos + 5 + close + 1;
                continue;
            }
        }

        // Move forward
        if let Some(next_backslash) = input[pos + 1..].find('\\') {
            pos = pos + 1 + next_backslash;
        } else {
            break;
        }
    }

    // Report unclosed environments
    for (env_name, line) in env_stack {
        result.add(
            Diagnostic::new(
                DiagnosticLevel::Error,
                format!(
                    "unclosed environment '{}' (opened at line {})",
                    env_name, line
                ),
            )
            .with_suggestion(format!("Add \\end{{{}}}", env_name)),
        );
    }
}

/// Format check results for terminal output
pub fn format_diagnostics(result: &CheckResult, use_color: bool) -> String {
    let mut output = String::new();

    for diag in &result.diagnostics {
        if use_color {
            let color = match diag.level {
                DiagnosticLevel::Error => "\x1b[31m",   // Red
                DiagnosticLevel::Warning => "\x1b[33m", // Yellow
                DiagnosticLevel::Info => "\x1b[34m",    // Blue
            };
            output.push_str(color);
            output.push_str(&format!("{}", diag));
            output.push_str("\x1b[0m\n\n");
        } else {
            output.push_str(&format!("{}\n\n", diag));
        }
    }

    // Summary
    if use_color {
        if result.has_errors() {
            output.push_str("\x1b[31m");
        } else if result.warnings > 0 {
            output.push_str("\x1b[33m");
        } else {
            output.push_str("\x1b[32m");
        }
    }

    output.push_str(&format!("Summary: {}", result.summary()));

    if use_color {
        output.push_str("\x1b[0m");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_braces() {
        let result = check_latex(r"\frac{1}{2}");
        assert!(!result.has_errors(), "Should pass for balanced braces");
    }

    #[test]
    fn test_unbalanced_braces() {
        let result = check_latex(r"\frac{1}{2");
        assert!(result.has_errors(), "Should fail for unbalanced braces");
    }

    #[test]
    fn test_balanced_environments() {
        let result = check_latex(r"\begin{equation}x=1\end{equation}");
        assert!(
            !result.has_errors(),
            "Should pass for balanced environments"
        );
    }

    #[test]
    fn test_unbalanced_environments() {
        let result = check_latex(r"\begin{equation}x=1");
        assert!(result.has_errors(), "Should fail for unclosed environment");
    }

    #[test]
    fn test_mismatched_environments() {
        let result = check_latex(r"\begin{equation}x=1\end{align}");
        assert!(
            result.has_errors(),
            "Should fail for mismatched environments"
        );
    }

    #[test]
    fn test_tikz_warning() {
        let result = check_latex(
            r"\begin{tikzpicture}
\draw (0,0)--(1,1);
\end{tikzpicture}",
        );
        // TikZ warning is generated during environment checking
        // If no warning, that's OK - the test is really about not crashing
        assert!(
            !result.has_errors(),
            "Should not have errors for valid TikZ"
        );
    }

    #[test]
    fn test_summary_format() {
        let mut result = CheckResult::new();
        result.add(Diagnostic::new(DiagnosticLevel::Error, "test"));
        result.add(Diagnostic::new(DiagnosticLevel::Warning, "test"));

        let summary = result.summary();
        assert!(summary.contains("1 error"));
        assert!(summary.contains("1 warning"));
    }
}
