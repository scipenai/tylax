//! Conversion context and options for Typst to LaTeX conversion
//!
//! This module handles state management, output buffering, and conversion options.

use std::collections::HashMap;

/// Options for Typst to LaTeX conversion
#[derive(Debug, Clone)]
pub struct T2LOptions {
    /// Whether to wrap output in a complete LaTeX document
    pub full_document: bool,
    /// Document class to use (default: "article")
    pub document_class: String,
    /// Document title (optional)
    pub title: Option<String>,
    /// Document author (optional)
    pub author: Option<String>,
    /// Whether this is math-only input
    pub math_only: bool,
    /// Whether we're in block math mode (affects display/inline conversion)
    /// true = block math mode (default), false = inline math mode
    pub block_math_mode: bool,
}

impl Default for T2LOptions {
    fn default() -> Self {
        Self {
            full_document: false,
            document_class: "article".to_string(),
            title: None,
            author: None,
            math_only: false,
            block_math_mode: true,
        }
    }
}

/// Environment context for Typst to LaTeX conversion
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EnvironmentContext {
    #[default]
    None,
    Document,
    Figure,
    Table,
    Tabular,
    Itemize,
    Enumerate,
    Description,
    Equation,
    Align,
    Matrix(String), // Matrix type: matrix, pmatrix, bmatrix, etc.
    Cases,
    Theorem(String), // Theorem-like environment with name
    Center,
    Quote,
    Verbatim,
}

impl T2LOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn math_only() -> Self {
        Self {
            math_only: true,
            ..Default::default()
        }
    }

    pub fn full_document() -> Self {
        Self {
            full_document: true,
            ..Default::default()
        }
    }

    /// Create options for inline math mode
    pub fn inline_math() -> Self {
        Self {
            math_only: true,
            block_math_mode: false,
            ..Default::default()
        }
    }

    /// Create options for block math mode
    pub fn block_math() -> Self {
        Self {
            math_only: true,
            block_math_mode: true,
            ..Default::default()
        }
    }
}

/// Token type for smart spacing decisions
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TokenType {
    None,
    Operator,
    Letter,
    Number,
    OpenParen,
    CloseParen,
    Command,
    Text,
    Newline,
}

/// Conversion context for tracking state during conversion
pub struct ConvertContext {
    /// Output buffer
    pub output: String,
    /// Whether we're inside a matrix/aligned environment
    pub in_environment: bool,
    /// Last token type for spacing decisions
    pub last_token: TokenType,
    /// Current indentation level
    pub indent_level: usize,
    /// Whether we're in math mode
    pub in_math: bool,
    /// Conversion options
    pub options: T2LOptions,
    /// Stack of environment contexts (for nested environments)
    pub env_stack: Vec<EnvironmentContext>,
    /// List nesting level (for itemize/enumerate)
    pub list_depth: usize,
    /// Collected labels for cross-references
    pub labels: Vec<String>,
    /// Collected warnings during conversion
    pub warnings: Vec<String>,
    /// User-defined variables (from #let)
    pub variables: HashMap<String, String>,
    /// Pending label to be attached to the next figure/table environment
    pub pending_label: Option<String>,
}

/// Initial capacity for output buffer (reduces reallocations)
const INITIAL_BUFFER_CAPACITY: usize = 1024;

impl ConvertContext {
    /// Create a new context with pre-allocated buffer
    pub fn new() -> Self {
        Self {
            output: String::with_capacity(INITIAL_BUFFER_CAPACITY),
            in_environment: false,
            last_token: TokenType::None,
            indent_level: 0,
            in_math: false,
            options: T2LOptions::default(),
            env_stack: Vec::new(),
            list_depth: 0,
            labels: Vec::new(),
            warnings: Vec::new(),
            variables: HashMap::new(),
            pending_label: None,
        }
    }

    /// Create a new context with custom buffer capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            output: String::with_capacity(capacity),
            in_environment: false,
            last_token: TokenType::None,
            indent_level: 0,
            in_math: false,
            options: T2LOptions::default(),
            env_stack: Vec::new(),
            list_depth: 0,
            labels: Vec::new(),
            warnings: Vec::new(),
            variables: HashMap::new(),
            pending_label: None,
        }
    }

    // =========================================================================
    // Environment Management
    // =========================================================================

    /// Push a new environment onto the stack
    pub fn push_env(&mut self, env: EnvironmentContext) {
        if matches!(
            env,
            EnvironmentContext::Itemize | EnvironmentContext::Enumerate
        ) {
            self.list_depth += 1;
        }
        self.env_stack.push(env);
    }

    /// Pop the current environment from the stack
    pub fn pop_env(&mut self) -> Option<EnvironmentContext> {
        let env = self.env_stack.pop();
        if let Some(ref e) = env {
            if matches!(
                e,
                EnvironmentContext::Itemize | EnvironmentContext::Enumerate
            ) {
                self.list_depth = self.list_depth.saturating_sub(1);
            }
        }
        env
    }

    /// Get current environment
    pub fn current_env(&self) -> &EnvironmentContext {
        self.env_stack.last().unwrap_or(&EnvironmentContext::None)
    }

    /// Check if we're in any list environment
    pub fn in_list(&self) -> bool {
        self.env_stack.iter().any(|e| {
            matches!(
                e,
                EnvironmentContext::Itemize | EnvironmentContext::Enumerate
            )
        })
    }

    /// Check if we're in a specific environment
    pub fn is_in_env(&self, env: &EnvironmentContext) -> bool {
        self.env_stack.contains(env)
    }

    /// Add a warning
    pub fn add_warning(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    /// Get list indentation string
    pub fn list_indent(&self) -> String {
        "  ".repeat(self.list_depth)
    }

    /// Push a string to the output buffer
    pub fn push(&mut self, s: &str) {
        self.output.push_str(s);
    }

    /// Push a line with current indentation
    pub fn push_line(&mut self, s: &str) {
        self.push_indent();
        self.push(s);
        self.push("\n");
        self.last_token = TokenType::Newline;
    }

    /// Push current indentation
    pub fn push_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("  ");
        }
    }

    /// Add a newline if not already at one
    pub fn newline(&mut self) {
        if !self.output.ends_with('\n') {
            self.push("\n");
        }
        self.last_token = TokenType::Newline;
    }

    /// Ensure there's a paragraph break (double newline)
    pub fn ensure_paragraph_break(&mut self) {
        if !self.output.ends_with("\n\n") && !self.output.is_empty() {
            if self.output.ends_with('\n') {
                self.push("\n");
            } else {
                self.push("\n\n");
            }
        }
    }

    /// Add smart spacing based on context
    pub fn push_with_spacing(&mut self, s: &str, token_type: TokenType) {
        // Add space before if needed
        let needs_space = matches!(
            (self.last_token, token_type),
            (TokenType::Letter, TokenType::Letter)
                | (TokenType::Command, TokenType::Letter)
                | (TokenType::Command, TokenType::Number)
                | (TokenType::Number, TokenType::Letter)
        );

        if needs_space && !self.output.ends_with(' ') && !self.output.ends_with('{') {
            self.push(" ");
        }

        self.push(s);
        self.last_token = token_type;
    }

    /// Finalize and clean up the output
    pub fn finalize(self) -> String {
        // Clean up the output in-place where possible
        let trimmed = self.output.trim();

        // Estimate final capacity (usually slightly smaller than input)
        let mut result = String::with_capacity(trimmed.len());

        let mut prev_char = None;

        for ch in trimmed.chars() {
            // Handle double spaces
            if ch == ' ' && prev_char == Some(' ') {
                continue;
            }

            // Handle spacing around braces in math mode
            if self.options.math_only {
                if ch == ' ' && prev_char == Some('{') {
                    continue; // Skip space after {
                }
                if ch == '}' && prev_char == Some(' ') {
                    // Remove trailing space before }
                    if result.ends_with(' ') {
                        result.pop();
                    }
                }
            }

            result.push(ch);
            prev_char = Some(ch);
        }

        result
    }

    /// Get approximate output size (useful for pre-allocation)
    pub fn output_len(&self) -> usize {
        self.output.len()
    }
}

impl Default for ConvertContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_default() {
        let opts = T2LOptions::new();
        assert!(!opts.full_document);
        assert!(!opts.math_only);
        assert_eq!(opts.document_class, "article");
    }

    #[test]
    fn test_options_math_only() {
        let opts = T2LOptions::math_only();
        assert!(opts.math_only);
        assert!(!opts.full_document);
    }

    #[test]
    fn test_context_push() {
        let mut ctx = ConvertContext::new();
        ctx.push("hello");
        ctx.push(" world");
        assert_eq!(ctx.output, "hello world");
    }

    #[test]
    fn test_context_indent() {
        let mut ctx = ConvertContext::new();
        ctx.indent_level = 2;
        ctx.push_line("test");
        assert_eq!(ctx.output, "    test\n");
    }

    #[test]
    fn test_context_finalize() {
        let mut ctx = ConvertContext::new();
        ctx.push("  hello  world  ");
        let result = ctx.finalize();
        assert_eq!(result, "hello world");
    }
}
