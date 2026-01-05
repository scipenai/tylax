//! Core state and structures for LaTeX to Typst conversion
//!
//! This module contains the main converter struct and conversion state.

use mitex_parser::syntax::{CmdItem, SyntaxElement, SyntaxKind, SyntaxNode};
use mitex_parser::CommandSpec;
use mitex_spec_gen::DEFAULT_SPEC;
use rowan::ast::AstNode;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::data::constants::{AcronymDef, GlossaryDef};
use crate::data::maps::TEX_COMMAND_SPEC;
use fxhash::FxHashMap;
use lazy_static::lazy_static;

use super::utils::{
    clean_whitespace, convert_caption_text, extract_arg_content, extract_arg_content_with_braces,
    extract_curly_inner_content, protect_zero_arg_commands, restore_protected_commands,
};

// =============================================================================
// LaTeX → Typst Conversion Options
// =============================================================================

/// Options for LaTeX to Typst conversion
#[derive(Debug, Clone)]
pub struct L2TOptions {
    /// Use shorthand symbols (e.g., `->` instead of `arrow.r`)
    /// Default: true
    pub prefer_shorthands: bool,

    /// Convert simple fractions to slash notation (e.g., `a/b` instead of `frac(a, b)`)
    /// Only applies to simple single-character numerator/denominator
    /// Default: true
    pub frac_to_slash: bool,

    /// Use `oo` instead of `infinity` for `\infty`
    /// Default: false
    pub infty_to_oo: bool,

    /// Preserve original spacing in the output
    /// Default: false
    pub keep_spaces: bool,

    /// Non-strict mode: allow unknown commands to pass through
    /// Default: true
    pub non_strict: bool,

    /// Apply output optimizations (e.g., `floor.l x floor.r` → `floor(x)`)
    /// Default: true
    pub optimize: bool,
}

impl Default for L2TOptions {
    fn default() -> Self {
        Self {
            prefer_shorthands: true,
            frac_to_slash: true,
            infty_to_oo: false,
            keep_spaces: false,
            non_strict: true,
            optimize: true,
        }
    }
}

impl L2TOptions {
    /// Create new options with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Create options optimized for human readability
    pub fn readable() -> Self {
        Self {
            prefer_shorthands: true,
            frac_to_slash: true,
            infty_to_oo: true,
            keep_spaces: false,
            non_strict: true,
            optimize: true,
        }
    }

    /// Create options for maximum compatibility (verbose output)
    pub fn verbose() -> Self {
        Self {
            prefer_shorthands: false,
            frac_to_slash: false,
            infty_to_oo: false,
            keep_spaces: false,
            non_strict: true,
            optimize: false,
        }
    }

    /// Create strict mode options (errors on unknown commands)
    pub fn strict() -> Self {
        Self {
            non_strict: false,
            ..Self::default()
        }
    }
}

lazy_static! {
    /// Merged command specification for parsing
    pub static ref MERGED_SPEC: CommandSpec = {
        let mut commands: FxHashMap<String, _> = DEFAULT_SPEC
            .items()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();

        for (k, v) in TEX_COMMAND_SPEC.items() {
            commands.insert(k.to_string(), v.clone());
        }

        CommandSpec::new(commands)
    };
}

/// Conversion mode (text vs math)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConversionMode {
    #[default]
    Text,
    Math,
}

/// Current environment context
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EnvironmentContext {
    #[default]
    None,
    Document,
    Bibliography,
    Figure,
    Table,
    Tabular,
    Itemize,
    Enumerate,
    Description,
    Equation,
    Align,
    Matrix,
    Cases,
    TikZ,
    Verbatim,
    Theorem(String), // Theorem-like environment with name
}

/// Macro definition
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: String,
    pub num_args: usize,
    pub default_arg: Option<String>,
    pub replacement: String,
}

/// Pending operator state (for operatorname*)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOperator {
    pub is_limits: bool,
}

/// Conversion state maintained during AST traversal
#[derive(Debug, Default)]
pub struct ConversionState {
    /// Current conversion mode
    pub mode: ConversionMode,
    /// Stack of environment contexts
    pub env_stack: Vec<EnvironmentContext>,
    /// Indentation level (for lists)
    pub indent: usize,
    /// Collected labels for the current element
    pub pending_label: Option<String>,
    /// Pending operator state
    pub pending_op: Option<PendingOperator>,
    /// User-defined macros
    pub macros: HashMap<String, MacroDef>,
    /// Whether we're in preamble
    pub in_preamble: bool,
    /// Document metadata
    pub title: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
    pub document_class: Option<String>,
    /// Collected errors/warnings
    pub warnings: Vec<String>,
    /// Counter for theorems, equations, etc.
    pub counters: HashMap<String, u32>,
    /// Acronym definitions (key -> AcronymDef)
    pub acronyms: HashMap<String, AcronymDef>,
    /// Glossary definitions (key -> GlossaryDef)
    pub glossary: HashMap<String, GlossaryDef>,
    /// Set of acronyms that have been used (for first-use tracking)
    pub used_acronyms: HashSet<String>,
    /// Conversion options
    pub options: L2TOptions,
}

impl ConversionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new environment onto the stack
    pub fn push_env(&mut self, env: EnvironmentContext) {
        if matches!(
            env,
            EnvironmentContext::Itemize | EnvironmentContext::Enumerate
        ) {
            self.indent += 2;
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
                self.indent = self.indent.saturating_sub(2);
            }
        }
        env
    }

    /// Get current environment
    pub fn current_env(&self) -> &EnvironmentContext {
        self.env_stack.last().unwrap_or(&EnvironmentContext::None)
    }

    /// Check if we're in a specific environment type anywhere in the stack
    pub fn is_inside(&self, env: &EnvironmentContext) -> bool {
        self.env_stack
            .iter()
            .any(|e| std::mem::discriminant(e) == std::mem::discriminant(env))
    }

    /// Get next counter value
    pub fn next_counter(&mut self, name: &str) -> u32 {
        let counter = self.counters.entry(name.to_string()).or_insert(0);
        *counter += 1;
        *counter
    }

    /// Register an acronym definition
    pub fn register_acronym(&mut self, key: &str, short: &str, long: &str) {
        self.acronyms
            .insert(key.to_string(), AcronymDef::new(short, long));
    }

    /// Register a glossary entry
    pub fn register_glossary(&mut self, key: &str, name: &str, description: &str) {
        self.glossary
            .insert(key.to_string(), GlossaryDef::new(name, description));
    }

    /// Get acronym and mark as used, returns (text, is_first_use)
    pub fn use_acronym(&mut self, key: &str) -> Option<(String, bool)> {
        if let Some(acr) = self.acronyms.get(key) {
            let is_first = !self.used_acronyms.contains(key);
            self.used_acronyms.insert(key.to_string());
            let text = if is_first {
                acr.full() // First use: "Long Form (SF)"
            } else {
                acr.short.clone() // Subsequent use: "SF"
            };
            Some((text, is_first))
        } else {
            None
        }
    }

    /// Get acronym short form only
    pub fn get_acronym_short(&self, key: &str) -> Option<String> {
        self.acronyms.get(key).map(|a| a.short.clone())
    }

    /// Get acronym long form only
    pub fn get_acronym_long(&self, key: &str) -> Option<String> {
        self.acronyms.get(key).map(|a| a.long.clone())
    }

    /// Get acronym full form
    pub fn get_acronym_full(&self, key: &str) -> Option<String> {
        self.acronyms.get(key).map(|a| a.full())
    }

    /// Get glossary entry name
    pub fn get_glossary_name(&self, key: &str) -> Option<String> {
        self.glossary.get(key).map(|g| g.name.clone())
    }
}

/// The main AST-based converter
pub struct LatexConverter {
    pub(crate) state: ConversionState,
    pub(crate) spec: CommandSpec,
}

impl LatexConverter {
    /// Create a new converter with default options
    pub fn new() -> Self {
        Self {
            state: ConversionState::new(),
            spec: MERGED_SPEC.clone(),
        }
    }

    /// Create a new converter with custom options
    pub fn with_options(options: L2TOptions) -> Self {
        let mut state = ConversionState::new();
        state.options = options;
        Self {
            state,
            spec: MERGED_SPEC.clone(),
        }
    }

    /// Get a reference to the current options
    pub fn options(&self) -> &L2TOptions {
        &self.state.options
    }

    /// Get a mutable reference to the current options
    pub fn options_mut(&mut self) -> &mut L2TOptions {
        &mut self.state.options
    }

    /// Convert a complete LaTeX document to Typst
    pub fn convert_document(&mut self, input: &str) -> String {
        self.state.in_preamble = true;

        // Preprocess: protect zero-argument commands that MiTeX would otherwise lose
        let protected_input = protect_zero_arg_commands(input);

        // Preprocess: extract and expand macro definitions
        let (mut macro_db, processed_input) =
            crate::features::macros::extract_and_remove_definitions(&protected_input);

        // Remove macros that we handle natively to avoid expansion issues
        for op in crate::data::constants::NATIVE_MATH_OPERATORS.iter() {
            macro_db.undefine(op);
        }

        let expanded_input = crate::features::macros::expand_macros(&processed_input, &macro_db);

        // Parse with mitex-parser
        let tree = mitex_parser::parse(&expanded_input, self.spec.clone());

        // Convert AST to Typst with pre-allocated buffer
        let estimated_size = (expanded_input.len() as f64 * 1.5) as usize;
        let mut output = String::with_capacity(estimated_size.max(1024));

        // Walk the tree
        self.visit_node(&tree, &mut output);

        // Build final document with preamble
        let result = self.build_document(output);

        // Restore protected commands
        restore_protected_commands(&result)
    }

    /// Convert math-only LaTeX to Typst
    pub fn convert_math(&mut self, input: &str) -> String {
        self.state.mode = ConversionMode::Math;
        self.state.in_preamble = false;

        // Parse
        let tree = mitex_parser::parse(input, self.spec.clone());

        // Convert with pre-allocated buffer
        let mut output = String::with_capacity(input.len().max(256));
        self.visit_node(&tree, &mut output);

        // Post-process
        self.postprocess_math(output)
    }

    /// Visit a syntax node and convert it
    pub fn visit_node(&mut self, node: &SyntaxNode, output: &mut String) {
        for child in node.children_with_tokens() {
            self.visit_element(child, output);
        }
    }

    /// Visit a syntax element (node or token)
    pub fn visit_element(&mut self, elem: SyntaxElement, output: &mut String) {
        use SyntaxKind::*;

        match elem.kind() {
            // Handle errors gracefully
            TokenError => {
                let text = match &elem {
                    SyntaxElement::Node(n) => n.text().to_string(),
                    SyntaxElement::Token(t) => t.text().to_string(),
                };
                self.state.warnings.push(format!("Parse error: {}", text));
                let _ = write!(output, "/* LaTeX Error: {} */", text.replace("*/", "* /"));
            }

            // Root - always recurse
            ScopeRoot => {
                if let SyntaxElement::Node(n) = elem {
                    self.visit_node(&n, output);
                }
            }

            // Containers - only output content after preamble
            ItemText | ItemParen | ClauseArgument => {
                if self.state.in_preamble {
                    if let SyntaxElement::Node(n) = elem {
                        let mut dummy = String::new();
                        self.visit_node(&n, &mut dummy);
                    }
                } else if let SyntaxElement::Node(n) = elem {
                    self.visit_node(&n, output);
                }
            }

            // Math formula
            ItemFormula => {
                super::math::convert_formula(self, elem, output);
            }

            // Curly group
            ItemCurly => {
                if self.state.in_preamble {
                    return;
                }
                super::math::convert_curly(self, elem, output);
            }

            // Left/Right delimiters
            ItemLR | ClauseLR => {
                super::math::convert_lr(self, elem, output);
            }

            // Attachment (subscript/superscript)
            ItemAttachComponent => {
                super::math::convert_attachment(self, elem, output);
            }

            // Command
            ItemCmd => {
                super::markup::convert_command(self, elem, output);
            }

            // Environment
            ItemEnv => {
                super::environment::convert_environment(self, elem, output);
            }

            // Plain word
            TokenWord => {
                if self.state.in_preamble {
                    return;
                }
                if let SyntaxElement::Token(t) = elem {
                    let text = t.text();
                    if matches!(self.state.mode, ConversionMode::Math) {
                        for c in text.chars() {
                            output.push(c);
                            output.push(' ');
                        }
                    } else {
                        output.push_str(text);
                    }
                }
            }

            // Whitespace
            TokenWhiteSpace => {
                if let SyntaxElement::Token(t) = elem {
                    output.push_str(t.text());
                }
            }

            // Line break
            TokenLineBreak => {
                if let SyntaxElement::Token(t) = elem {
                    output.push_str(t.text());
                    for _ in 0..self.state.indent {
                        output.push(' ');
                    }
                } else {
                    output.push('\n');
                }
            }

            // Newline command \\
            ItemNewLine => match self.state.current_env() {
                EnvironmentContext::Matrix => output.push_str("zws ;"),
                EnvironmentContext::Cases => output.push(','),
                EnvironmentContext::Align | EnvironmentContext::Equation => {
                    output.push_str(" \\ ");
                }
                EnvironmentContext::Tabular => output.push_str("|||ROW|||"),
                _ => output.push_str("\\ "),
            },

            // Ampersand (column separator)
            TokenAmpersand => match self.state.current_env() {
                EnvironmentContext::Matrix => output.push_str("zws, "),
                EnvironmentContext::Cases => output.push_str("& "),
                EnvironmentContext::Align => output.push_str("& "),
                EnvironmentContext::Tabular | EnvironmentContext::Table => {
                    output.push_str("|||CELL|||")
                }
                _ => output.push('&'),
            },

            // Special characters
            TokenTilde => {
                if matches!(self.state.mode, ConversionMode::Math) {
                    output.push_str("space.nobreak ");
                } else {
                    output.push(' ');
                }
            }
            TokenHash => output.push_str("\\#"),
            TokenUnderscore => {
                if matches!(self.state.mode, ConversionMode::Math) {
                    output.push('_');
                } else {
                    output.push_str("\\_");
                }
            }
            TokenCaret => {
                if matches!(self.state.mode, ConversionMode::Math) {
                    output.push('^');
                } else {
                    output.push_str("\\^");
                }
            }
            TokenApostrophe => output.push('\''),
            TokenComma => output.push(','),
            TokenSlash => output.push('/'),
            TokenAsterisk => {
                if let Some(ref mut op) = self.state.pending_op {
                    op.is_limits = true;
                    return;
                }
                if matches!(self.state.mode, ConversionMode::Math) {
                    output.push('*');
                } else {
                    output.push_str("\\*");
                }
            }
            TokenAtSign => output.push('@'),
            TokenSemicolon => output.push(';'),
            TokenDitto => output.push('"'),
            TokenLParen => output.push('('),
            TokenRParen => output.push(')'),
            TokenLBracket => {
                if matches!(self.state.mode, ConversionMode::Math) {
                    output.push('[');
                }
            }
            TokenRBracket => {
                if matches!(self.state.mode, ConversionMode::Math) {
                    output.push(']');
                }
            }

            // Ignore these
            TokenLBrace | TokenRBrace | TokenDollar | TokenBeginMath | TokenEndMath
            | TokenComment | ItemBlockComment | ClauseCommandName | ItemBegin | ItemEnd
            | ItemBracket => {}

            // Command symbol
            TokenCommandSym => {
                super::markup::convert_command_sym(self, elem, output);
            }

            // Typst code passthrough
            ItemTypstCode => {
                if let SyntaxElement::Node(n) = elem {
                    output.push_str(&n.text().to_string());
                }
            }
        }
    }

    // ============================================================
    // Argument extraction helpers
    // ============================================================

    /// Get a required argument from a command (raw text, strips braces)
    pub fn get_required_arg(&self, cmd: &CmdItem, index: usize) -> Option<String> {
        let mut required_count = 0;
        for child in cmd.syntax().children() {
            if child.kind() == SyntaxKind::ClauseArgument {
                let is_curly = child.children().any(|c| c.kind() == SyntaxKind::ItemCurly);
                if is_curly {
                    if required_count == index {
                        return Some(extract_arg_content(&child));
                    }
                    required_count += 1;
                }
            }
        }
        None
    }

    /// Get a required argument preserving inner braces
    pub fn get_required_arg_with_braces(&self, cmd: &CmdItem, index: usize) -> Option<String> {
        let mut required_count = 0;
        for child in cmd.syntax().children() {
            if child.kind() == SyntaxKind::ClauseArgument {
                let is_curly = child.children().any(|c| c.kind() == SyntaxKind::ItemCurly);
                if is_curly {
                    if required_count == index {
                        return Some(extract_arg_content_with_braces(&child));
                    }
                    required_count += 1;
                }
            }
        }
        None
    }

    /// Get an optional argument from a command
    pub fn get_optional_arg(&self, cmd: &CmdItem, index: usize) -> Option<String> {
        let mut optional_count = 0;
        for child in cmd.syntax().children() {
            if child.kind() == SyntaxKind::ClauseArgument {
                let is_bracket = child
                    .children()
                    .any(|c| c.kind() == SyntaxKind::ItemBracket);
                if is_bracket {
                    if optional_count == index {
                        return Some(extract_arg_content(&child));
                    }
                    optional_count += 1;
                }
            }
        }
        None
    }

    /// Convert a required argument - recursively processes the content
    pub fn convert_required_arg(&mut self, cmd: &CmdItem, index: usize) -> Option<String> {
        let mut required_count = 0;
        for child in cmd.syntax().children() {
            if child.kind() == SyntaxKind::ClauseArgument {
                let is_curly = child.children().any(|c| c.kind() == SyntaxKind::ItemCurly);
                if is_curly {
                    if required_count == index {
                        let mut output = String::new();
                        for arg_child in child.children() {
                            if arg_child.kind() == SyntaxKind::ItemCurly {
                                for content in arg_child.children_with_tokens() {
                                    match content.kind() {
                                        SyntaxKind::TokenLBrace | SyntaxKind::TokenRBrace => {
                                            continue
                                        }
                                        _ => {
                                            self.visit_element(content, &mut output);
                                        }
                                    }
                                }
                            }
                        }
                        return Some(output.trim().to_string());
                    }
                    required_count += 1;
                }
            }
        }
        None
    }

    /// Get a required argument from a command and convert it to Typst
    pub fn get_converted_required_arg(&mut self, cmd: &CmdItem, index: usize) -> Option<String> {
        let raw_text = self.get_required_arg_with_braces(cmd, index)?;
        if raw_text.contains('$') || raw_text.contains('\\') {
            Some(convert_caption_text(&raw_text))
        } else {
            Some(raw_text)
        }
    }

    /// Get optional argument from an environment
    pub fn get_env_optional_arg(&self, node: &SyntaxNode) -> Option<String> {
        for child in node.children() {
            if child.kind() == SyntaxKind::ItemBegin {
                for begin_child in child.children() {
                    if begin_child.kind() == SyntaxKind::ClauseArgument {
                        let has_bracket = begin_child
                            .children()
                            .any(|c| c.kind() == SyntaxKind::ItemBracket);
                        if has_bracket {
                            return Some(extract_arg_content(&begin_child));
                        }
                    }
                }
            }
        }
        None
    }

    /// Get a required argument from an environment
    pub fn get_env_required_arg(&self, node: &SyntaxNode, index: usize) -> Option<String> {
        let mut required_count = 0;
        for child in node.children() {
            if child.kind() == SyntaxKind::ClauseArgument {
                let is_curly = child.children().any(|c| c.kind() == SyntaxKind::ItemCurly);
                if is_curly {
                    if required_count == index {
                        return Some(extract_arg_content(&child));
                    }
                    required_count += 1;
                }
            }
        }
        None
    }

    /// Extract and convert argument for metadata (title, author, date)
    pub fn extract_metadata_arg(&mut self, cmd: &CmdItem) -> Option<String> {
        self.get_required_arg_with_braces(cmd, 0)
            .map(|raw| convert_caption_text(&raw).trim().to_string())
    }

    /// Extract inner content of a curly/bracket node, skipping its braces
    pub fn extract_curly_inner_content(&self, node: &SyntaxNode) -> String {
        extract_curly_inner_content(node)
    }

    // ============================================================
    // Math post-processing
    // ============================================================

    /// Post-process math output
    pub fn postprocess_math(&self, input: String) -> String {
        let mut result = input;

        result = self.fix_operatorname(&result);
        result = self.fix_blackboard_bold(&result);
        result = self.fix_empty_accent_args(&result);

        while result.contains("  ") {
            result = result.replace("  ", " ");
        }

        result = result.replace(" ,", ",");
        result = result.replace("( ", "(");
        result = result.replace(" )", ")");
        result = result.replace(" ^", "^");
        result = result.replace(" _", "_");

        result.trim().to_string()
    }

    /// Clean up math spacing
    pub fn cleanup_math_spacing(&self, input: &str) -> String {
        let mut result = input.to_string();

        while result.contains("  ") {
            result = result.replace("  ", " ");
        }

        result = result.replace(" ,", ",");
        result = result.replace("( ", "(");
        result = result.replace(" )", ")");
        result = result.replace(" (", "(");
        result = result.replace(" [", "[");
        result = result.replace(" ^", "^");
        result = result.replace(" _", "_");

        result
    }

    /// Fix operatorname() patterns
    pub fn fix_operatorname(&self, input: &str) -> String {
        let mut result = input.to_string();

        while let Some(start) = result.find("operatorname(") {
            let after = &result[start + 13..];
            if let Some(end) = self.find_matching_paren(after) {
                let content = &after[..end];
                let clean_content: String =
                    content.chars().filter(|c| !c.is_whitespace()).collect();
                let replacement = format!("op(\"{}\")", clean_content);
                let total_end = start + 13 + end + 1;
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    replacement,
                    &result[total_end..]
                );
            } else {
                break;
            }
        }

        result
    }

    /// Fix bb() (blackboard bold)
    pub fn fix_blackboard_bold(&self, input: &str) -> String {
        let mut result = input.to_string();

        while let Some(start) = result.find("bb(") {
            let after = &result[start + 3..];
            if let Some(end) = self.find_matching_paren(after) {
                let content = &after[..end];
                let clean_content: String =
                    content.chars().filter(|c| !c.is_whitespace()).collect();

                let replacement = match clean_content.as_str() {
                    "E" => "EE".to_string(),
                    "P" => "PP".to_string(),
                    "R" => "RR".to_string(),
                    "N" => "NN".to_string(),
                    "Z" => "ZZ".to_string(),
                    "Q" => "QQ".to_string(),
                    "C" => "CC".to_string(),
                    _ => format!("bb({})", clean_content),
                };

                let total_end = start + 3 + end + 1;
                result = format!(
                    "{}{}{}",
                    &result[..start],
                    replacement,
                    &result[total_end..]
                );
            } else {
                break;
            }
        }

        result
    }

    /// Fix empty accent/function patterns
    pub fn fix_empty_accent_args(&self, input: &str) -> String {
        let mut result = input.to_string();

        let accents = [
            "hat",
            "tilde",
            "bar",
            "vec",
            "dot",
            "ddot",
            "acute",
            "grave",
            "breve",
            "check",
            "overline",
            "underline",
            "widehat",
            "widetilde",
            "sqrt",
            "cancel",
            "bold",
            "italic",
            "cal",
            "frak",
            "bb",
            "mono",
            "sans",
        ];

        for accent in accents {
            let pattern = format!("{}()", accent);
            while let Some(pos) = result.find(&pattern) {
                let after = &result[pos + pattern.len()..];
                if let Some(first_char) = after.chars().next() {
                    if first_char.is_alphanumeric() {
                        let arg_end = self.find_simple_arg_end(after);
                        let arg = &after[..arg_end];
                        let replacement = format!("{}({})", accent, arg.trim());
                        let total = pos + pattern.len() + arg_end;
                        result = format!("{}{}{}", &result[..pos], replacement, &result[total..]);
                        continue;
                    }
                }
                break;
            }
        }

        result
    }

    /// Find matching closing parenthesis
    pub fn find_matching_paren(&self, s: &str) -> Option<usize> {
        let mut depth = 1;
        for (i, c) in s.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Find the end of a simple argument
    pub fn find_simple_arg_end(&self, s: &str) -> usize {
        let mut pos = 0;
        for c in s.chars() {
            if c.is_alphanumeric() || c == '_' {
                pos += c.len_utf8();
            } else {
                break;
            }
        }
        if pos == 0 {
            1
        } else {
            pos
        }
    }

    /// Check if a term is simple enough for slash notation
    pub fn is_simple_term(&self, s: &str) -> bool {
        let s = s.trim();
        if s.is_empty() {
            return false;
        }

        if s.len() == 1 {
            let c = s.chars().next().unwrap();
            return c.is_alphanumeric();
        }

        if s.len() <= 3 && s.chars().all(|c| c.is_alphanumeric()) {
            return true;
        }

        let simple_symbols = [
            "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
            "lambda", "mu", "nu", "xi", "pi", "rho", "sigma", "tau", "upsilon", "phi", "chi",
            "psi", "omega", "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta",
            "Iota", "Kappa", "Lambda", "Mu", "Nu", "Xi", "Pi", "Rho", "Sigma", "Tau", "Upsilon",
            "Phi", "Chi", "Psi", "Omega",
        ];

        if simple_symbols.contains(&s) {
            return true;
        }

        if s.contains('_') || s.contains('^') {
            let parts: Vec<&str> = s.split(['_', '^']).collect();
            if parts.len() == 2
                && parts[0].len() <= 2
                && parts[0].chars().all(|c| c.is_alphanumeric())
                && parts[1].len() <= 2
                && parts[1]
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '(' || c == ')')
            {
                return true;
            }
        }

        false
    }

    // ============================================================
    // Document building
    // ============================================================

    /// Build the final Typst document
    pub fn build_document(&self, content: String) -> String {
        let mut doc = String::new();

        // Document metadata
        if self.state.title.is_some() || self.state.author.is_some() {
            doc.push_str("#set document(\n");
            if let Some(ref title) = self.state.title {
                let _ = writeln!(doc, "  title: \"{}\",", title.replace('"', "\\\""));
            }
            if let Some(ref author) = self.state.author {
                let _ = writeln!(doc, "  author: \"{}\",", author.replace('"', "\\\""));
            }
            doc.push_str(")\n\n");
        }

        // Page and heading setup
        if let Some(ref class) = self.state.document_class {
            match class.as_str() {
                "article" => {
                    doc.push_str("#set page(paper: \"a4\")\n");
                    doc.push_str("#set heading(numbering: \"1.\")\n");
                    doc.push_str("#set math.equation(numbering: \"(1)\")\n\n");
                }
                "report" | "book" => {
                    doc.push_str("#set page(paper: \"a4\")\n");
                    doc.push_str("#set heading(numbering: \"1.1\")\n");
                    doc.push_str("#set math.equation(numbering: \"(1)\")\n\n");
                }
                "beamer" => {
                    doc.push_str("#import \"@preview/polylux:0.3.1\": *\n");
                    doc.push_str("#set page(paper: \"presentation-16-9\")\n\n");
                }
                _ => {
                    doc.push_str("#set page(paper: \"a4\")\n");
                    doc.push_str("#set heading(numbering: \"1.\")\n");
                    doc.push_str("#set math.equation(numbering: \"(1)\")\n\n");
                }
            }
        } else {
            doc.push_str("#set page(paper: \"a4\")\n");
            doc.push_str("#set heading(numbering: \"1.\")\n");
            doc.push_str("#set math.equation(numbering: \"(1)\")\n\n");
        }

        // Title block
        if self.state.title.is_some() || self.state.author.is_some() {
            doc.push_str("#align(center)[\n");
            if let Some(ref title) = self.state.title {
                let _ = writeln!(doc, "  #text(size: 2em, weight: \"bold\")[{}]", title);
            }
            if let Some(ref author) = self.state.author {
                let _ = write!(doc, "  \n  #text(size: 1.2em)[{}]\n", author);
            }
            if let Some(ref date) = self.state.date {
                if date == "\\today" {
                    doc.push_str("  \n  #datetime.today().display()\n");
                } else {
                    let _ = write!(doc, "  \n  {}\n", date);
                }
            }
            doc.push_str("]\n\n");
        }

        // Clean up content
        let cleaned_content = clean_whitespace(&content);
        doc.push_str(&cleaned_content);

        // Add warnings as comments
        if !self.state.warnings.is_empty() {
            doc.push_str("\n\n// Conversion warnings:\n");
            for warning in &self.state.warnings {
                let _ = writeln!(doc, "// - {}", warning);
            }
        }

        clean_whitespace(&doc)
    }

    // ============================================================
    // Helper methods for submodules
    // ============================================================

    /// Process SI unit string
    pub fn process_si_unit(&self, input: &str) -> String {
        let mut result = input.to_string();

        for (cmd, val) in crate::siunitx::SI_UNITS.iter() {
            result = result.replace(cmd, val);
        }
        for (cmd, val) in crate::siunitx::SI_PREFIXES.iter() {
            result = result.replace(cmd, val);
        }

        result = result
            .replace("\\per", "/")
            .replace("\\squared", "²")
            .replace("\\cubed", "³")
            .replace(" ", "");

        result
    }

    /// Extract raw content from a verbatim-like environment
    pub fn extract_env_raw_content(&self, node: &SyntaxNode) -> String {
        let mut content = String::new();

        for child in node.children_with_tokens() {
            match child.kind() {
                SyntaxKind::ItemBegin | SyntaxKind::ItemEnd => continue,
                _ => {
                    if let SyntaxElement::Token(t) = child {
                        content.push_str(t.text());
                    } else if let SyntaxElement::Node(n) = child {
                        content.push_str(&n.text().to_string());
                    }
                }
            }
        }

        content
    }

    /// Visit environment content (excluding begin/end)
    pub fn visit_env_content(&mut self, node: &SyntaxNode, output: &mut String) {
        for child in node.children_with_tokens() {
            match child.kind() {
                SyntaxKind::ItemBegin | SyntaxKind::ItemEnd => continue,
                _ => self.visit_element(child, output),
            }
        }
    }
}

impl Default for LatexConverter {
    fn default() -> Self {
        Self::new()
    }
}
