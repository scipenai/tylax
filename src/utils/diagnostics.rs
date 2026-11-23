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
use mitex_spec::{ArgPattern, ArgShape};
use mitex_spec_gen::DEFAULT_SPEC;
use std::collections::HashSet;
use std::fmt;

use crate::core::latex2typst::lexical::{find_env_tag, inline_verb, literal_env};
use crate::core::latex2typst::{parse_latex, SourceMap};
use crate::data::maps::TEX_COMMAND_SPEC;
use crate::data::symbols::convert_delimiter;
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

    // Blank the literal regions ONCE and run every layer against the result.
    // This also shields the parse: mitex has no `\verb`/verbatim handling and
    // would report syntax errors for valid literal text. The mask keeps the
    // exact byte length, so offsets stay valid.
    let masked = mask_literal_regions(input);
    let input = masked.as_str();

    // Recovery may insert braces; diagnostics always map back to the original source.
    let parsed = parse_latex(input, MERGED_SPEC.clone());

    // Calculate line offsets for position reporting
    let line_offsets = compute_line_offsets(input);

    // Packages the document actually loads, so a "requires the 'x' package"
    // hint is only emitted when that package is missing (issue #40).
    let declared = collect_declared_packages(input);

    // Walk the AST looking for issues
    check_node(
        &parsed.tree,
        input,
        &line_offsets,
        &parsed.source_map,
        &declared,
        &mut result,
    );

    // Check for unbalanced braces
    check_brace_balance(input, &mut result);

    // Check for unbalanced environments
    check_environment_balance(input, &mut result);

    result
}

/// Package names loaded via `\usepackage[..]{a,b}` (and `\RequirePackage`).
/// mitex has no argument pattern for these, so we scan the raw source.
fn collect_declared_packages(input: &str) -> HashSet<String> {
    let mut packages = HashSet::new();
    let uncommented = strip_latex_comments(input);
    for keyword in ["\\usepackage", "\\RequirePackage"] {
        let mut search = uncommented.as_str();
        while let Some(pos) = search.find(keyword) {
            let mut rest = &search[pos + keyword.len()..];
            // Skip an optional `[..]` option list.
            let rest_trimmed = rest.trim_start();
            if let Some(after_bracket) = rest_trimmed.strip_prefix('[') {
                if let Some(end) = after_bracket.find(']') {
                    rest = &after_bracket[end + 1..];
                } else {
                    rest = after_bracket;
                }
            } else {
                rest = rest_trimmed;
            }
            // Read the `{a,b,c}` name list.
            if let Some(after_brace) = rest.trim_start().strip_prefix('{') {
                if let Some(end) = after_brace.find('}') {
                    for name in after_brace[..end].split(',') {
                        let name = name.trim();
                        if !name.is_empty() {
                            packages.insert(name.to_string());
                        }
                    }
                }
            }
            search = &search[pos + keyword.len()..];
        }
    }
    packages
}

/// Drop each line's comment suffix before a lightweight source scan; an odd
/// run of preceding backslashes escapes the `%`. Verbatim is not parsed here:
/// package declarations live in the preamble, where plain comment rules apply.
fn strip_latex_comments(input: &str) -> String {
    input
        .lines()
        .map(|line| {
            let mut backslashes = 0usize;
            for (index, character) in line.char_indices() {
                match character {
                    '\\' => backslashes += 1,
                    '%' if backslashes & 1 == 0 => return &line[..index],
                    _ => backslashes = 0,
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
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
fn check_node(
    node: &SyntaxNode,
    source: &str,
    line_offsets: &[usize],
    source_map: &SourceMap,
    declared: &HashSet<String>,
    result: &mut CheckResult,
) {
    for child in node.children_with_tokens() {
        match child.kind() {
            SyntaxKind::TokenError => {
                // Syntax error
                let text = match &child {
                    SyntaxElement::Token(t) => t.text().to_string(),
                    SyntaxElement::Node(n) => n.text().to_string(),
                };

                let offset = child.text_range().start().into();
                let (line, col) =
                    offset_to_location(source_map.original_offset(offset), line_offsets);

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
                    check_command(cmd_node, source, line_offsets, source_map, declared, result);
                }
            }

            SyntaxKind::ItemEnv => {
                // Check environment
                if let SyntaxElement::Node(env_node) = &child {
                    check_environment(env_node, source, line_offsets, source_map, declared, result);
                }
            }

            kind @ (SyntaxKind::ItemAttachComponent | SyntaxKind::ItemLR) => {
                if let SyntaxElement::Node(n) = &child {
                    if kind == SyntaxKind::ItemLR {
                        check_left_right(n, line_offsets, source_map, result);
                    } else {
                        check_attachment(n, line_offsets, source_map, result);
                    }
                    check_node(n, source, line_offsets, source_map, declared, result);
                }
            }

            _ => {
                // Recurse into child nodes
                if let SyntaxElement::Node(n) = child {
                    check_node(&n, source, line_offsets, source_map, declared, result);
                }
            }
        }
    }
}

/// Count required term (`t`) tokens outside optional groups in a MiTeX glob pattern.
fn glob_required_terms(pattern: &str) -> usize {
    let mut depth = 0usize;
    let mut required = 0usize;
    for ch in pattern.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            't' if depth == 0 => required += 1,
            _ => {}
        }
    }
    required
}

/// Minimum required argument count for a command, and whether leading optional
/// `[...]` bracket groups should be excluded when counting bound arguments.
fn required_term_count(cmd_name: &str, pattern: &ArgPattern) -> Option<(usize, bool, bool)> {
    // Variadic physics commands declare their maximum arity in `TEX_COMMAND_SPEC`.
    match cmd_name {
        "dd" | "differential" => return None,
        "dv"
        | "dv*"
        | "derivative"
        | "fdv"
        | "fdv*"
        | "fderivative"
        | "functionalderivative"
        | "pdv"
        | "pdv*"
        | "partialderivative"
        | "pderivative" => return Some((1, true, true)),
        "braket" | "braket*" | "ev" | "ev*" | "expval" | "expval*" | "expectationvalue" => {
            return Some((1, false, true));
        }
        _ => {}
    }

    match pattern {
        ArgPattern::FixedLenTerm { len } if *len > 0 => Some((usize::from(*len), false, false)),
        ArgPattern::RangeLenTerm { min, .. } if *min > 0 => Some((usize::from(*min), false, true)),
        ArgPattern::Glob { pattern } => {
            let required = glob_required_terms(&pattern.0);
            (required > 0).then_some((required, true, false))
        }
        _ => None,
    }
}

/// Whether all arguments of `cmd_name` are opaque identifiers/keys/URLs/paths
/// (where underscores or caret-like text are literal rather than math scripts).
fn is_opaque_arg_command(cmd_name: &str) -> bool {
    matches!(
        cmd_name,
        "label"
            | "ref"
            | "eqref"
            | "pageref"
            | "cref"
            | "Cref"
            | "autoref"
            | "cite"
            | "citep"
            | "citet"
            | "autocite"
            | "parencite"
            | "textcite"
            | "footcite"
            | "bibitem"
            | "url"
            | "includegraphics"
            | "input"
            | "include"
            | "bibliography"
            | "bibliographystyle"
            | "documentclass"
            | "usepackage"
            | "RequirePackage"
            | "gls"
            | "Gls"
            | "acrshort"
            | "acrlong"
            | "acrfull"
            | "ac"
            | "Acs"
            | "Acl"
            | "Acf"
    )
}

/// Extract `(command, delimiter)` tokens from a `ClauseLR` node.
fn lr_clause_parts(clause: &SyntaxNode) -> Option<(String, Option<String>)> {
    let mut tokens = clause
        .children_with_tokens()
        .filter_map(|elem| elem.into_token())
        .filter(|tok| {
            !matches!(
                tok.kind(),
                SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak | SyntaxKind::TokenComment
            )
        });
    let cmd = tokens.next()?.text().to_string();
    let delim = tokens.next().map(|tok| tok.text().to_string());
    Some((cmd, delim))
}

/// Whether `delim` is a valid delimiter token after `\left` or `\right`.
fn is_valid_lr_delimiter(delim: &str) -> bool {
    convert_delimiter(delim).is_some()
        || matches!(
            delim,
            "\\uparrow"
                | "\\downarrow"
                | "\\updownarrow"
                | "\\Uparrow"
                | "\\Downarrow"
                | "\\Updownarrow"
                | "\\lgroup"
                | "\\rgroup"
                | "\\lmoustache"
                | "\\rmoustache"
                | "\\arrowvert"
                | "\\Arrowvert"
                | "\\bracevert"
                | "\\ulcorner"
                | "\\urcorner"
                | "\\llcorner"
                | "\\lrcorner"
                | "\\llbracket"
                | "\\rrbracket"
                | "\\lAngle"
                | "\\rAngle"
                | "\\llangle"
                | "\\rrangle"
                | "\\lBrack"
                | "\\rBrack"
                | "\\lParen"
                | "\\rParen"
                | "\\Vvert"
                | "\\lbag"
                | "\\rbag"
                | "\\lang"
                | "\\rang"
        )
}

/// Check a command node for issues
fn check_command(
    node: &SyntaxNode,
    source: &str,
    line_offsets: &[usize],
    source_map: &SourceMap,
    declared: &HashSet<String>,
    result: &mut CheckResult,
) {
    // Extract the command's own token rather than the whole node text: Left1/Infix
    // commands such as `\limits` or `\over` wrap their left operand as the first
    // child of `ItemCmd`.
    let cmd_token = node
        .children_with_tokens()
        .filter_map(|elem| elem.into_token())
        .find(|tok| tok.kind() == SyntaxKind::ClauseCommandName);
    let cmd_name_owned = cmd_token
        .as_ref()
        .map(|tok| tok.text().trim_start_matches('\\').to_string())
        .unwrap_or_else(|| {
            let text = node.text().to_string();
            text.split(|c: char| !c.is_alphanumeric() && c != '\\')
                .next()
                .unwrap_or("")
                .trim_start_matches('\\')
                .to_string()
        });
    let cmd_name = cmd_name_owned.as_str();

    let offset: usize = cmd_token
        .as_ref()
        .map(|tok| tok.text_range().start().into())
        .unwrap_or_else(|| node.text_range().start().into());
    let (line, col) = offset_to_location(source_map.original_offset(offset), line_offsets);

    // Every paired `\right` lives inside `ItemLR`/`ClauseLR`, so an `ItemCmd`
    // named `right` has no matching `\left`.
    if cmd_name == "right" {
        result.add(
            Diagnostic::new(DiagnosticLevel::Error, "\\right has no matching \\left")
                .with_location(line, col)
                .with_source("\\right"),
        );
    }

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

    // Check for package requirements, but stay quiet when the document already
    // loads the package (issue #40).
    if let Some(package) = PACKAGE_COMMANDS.get(cmd_name) {
        if !declared.contains(*package) {
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
    }

    // Check for missing required arguments (`\frac{a}`, `\text_a`, `\sqrt`, `\sqrt[3]`, `\rule{1cm}`).
    if let Some(ArgShape::Right { pattern }) =
        MERGED_SPEC.get_cmd(cmd_name).map(|shape| &shape.args)
    {
        if let Some((required, skip_optional, at_least)) = required_term_count(cmd_name, pattern) {
            let bound = node
                .children()
                .filter(|child| child.kind() == SyntaxKind::ClauseArgument)
                .filter(|child| {
                    !skip_optional
                        || child
                            .first_child()
                            .is_none_or(|inner| inner.kind() != SyntaxKind::ItemBracket)
                })
                .count();
            if bound < required {
                let requirement = if at_least { "at least " } else { "" };
                result.add(
                    Diagnostic::new(
                        DiagnosticLevel::Error,
                        format!(
                            "command '\\{}' takes {}{} argument(s) but has {}",
                            cmd_name, requirement, required, bound
                        ),
                    )
                    .with_location(line, col)
                    .with_source(format!("\\{}", cmd_name)),
                );
            }
        }
    }

    // Identifiers, keys, paths and URLs contain literal `_` characters rather than subscripts.
    if is_opaque_arg_command(cmd_name) {
        return;
    }

    if cmd_name == "href" || cmd_name == "hyperref" {
        let mut skipped = false;
        for child in node.children() {
            if !skipped && child.kind() == SyntaxKind::ClauseArgument {
                let is_target = cmd_name == "href"
                    || child
                        .first_child()
                        .is_some_and(|inner| inner.kind() == SyntaxKind::ItemBracket);
                if is_target {
                    skipped = true;
                    continue;
                }
            }
            check_node(&child, source, line_offsets, source_map, declared, result);
        }
        return;
    }

    // Recurse into children
    check_node(node, source, line_offsets, source_map, declared, result);
}

/// Check a `_` or `^` script for a missing argument, and for a second script of
/// the same kind on one base (`x_a_b`, `x^a^b`, `x_a^b_c`), which LaTeX rejects.
fn check_attachment(
    node: &SyntaxNode,
    line_offsets: &[usize],
    source_map: &SourceMap,
    result: &mut CheckResult,
) {
    let is_trivia = |kind: SyntaxKind| {
        matches!(
            kind,
            SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak
        )
    };
    let marker_token_of = |node: &SyntaxNode| {
        node.children_with_tokens()
            .filter_map(|child| child.into_token())
            .find(|tok| {
                matches!(
                    tok.kind(),
                    SyntaxKind::TokenUnderscore | SyntaxKind::TokenCaret
                )
            })
    };
    let Some(marker_tok) = marker_token_of(node) else {
        return;
    };
    let marker = marker_tok.kind();
    let offset: usize = marker_tok.text_range().start().into();
    let (line, col) = offset_to_location(source_map.original_offset(offset), line_offsets);
    let (name, suggestion) = if marker == SyntaxKind::TokenUnderscore {
        (
            "subscript",
            "Brace the first script's base and script: {x_a}_b",
        )
    } else {
        (
            "superscript",
            "Brace the first script's base and script: {x^a}^b",
        )
    };

    let script_missing = node
        .children_with_tokens()
        .map(|child| child.kind())
        .filter(|kind| !is_trivia(*kind))
        .last()
        == Some(marker);
    if script_missing {
        result.add(
            Diagnostic::new(DiagnosticLevel::Error, format!("{name} has no argument"))
                .with_location(line, col),
        );
    }

    // The base of `x_a_b` is the attachment `x_a`; follow such bases down.
    let mut base = node.first_child();
    while let Some(argument) = base.filter(|n| n.kind() == SyntaxKind::ClauseArgument) {
        let Some(inner) = argument
            .first_child()
            .filter(|n| n.kind() == SyntaxKind::ItemAttachComponent)
        else {
            break;
        };
        if marker_token_of(&inner).map(|tok| tok.kind()) == Some(marker) {
            result.add(
                Diagnostic::new(DiagnosticLevel::Error, format!("double {name}"))
                    .with_location(line, col)
                    .with_suggestion(suggestion),
            );
            break;
        }
        base = inner.first_child();
    }
}

/// Check that a `\left` has its `\right`, and that both carry valid delimiters.
fn check_left_right(
    node: &SyntaxNode,
    line_offsets: &[usize],
    source_map: &SourceMap,
    result: &mut CheckResult,
) {
    let mut has_right = false;
    for clause in node
        .children()
        .filter(|child| child.kind() == SyntaxKind::ClauseLR)
    {
        let Some((cmd, delim)) = lr_clause_parts(&clause) else {
            continue;
        };
        if cmd == "\\right" {
            has_right = true;
        }
        let clause_offset: usize = clause.text_range().start().into();
        let (line, col) =
            offset_to_location(source_map.original_offset(clause_offset), line_offsets);
        match delim {
            None => result.add(
                Diagnostic::new(
                    DiagnosticLevel::Error,
                    format!("{cmd} is missing a delimiter"),
                )
                .with_location(line, col),
            ),
            Some(d) if !is_valid_lr_delimiter(&d) => result.add(
                Diagnostic::new(
                    DiagnosticLevel::Error,
                    format!("invalid delimiter '{}' after {}", d, cmd),
                )
                .with_location(line, col),
            ),
            _ => {}
        }
    }
    if !has_right {
        let offset: usize = node.text_range().start().into();
        let (line, col) = offset_to_location(source_map.original_offset(offset), line_offsets);
        result.add(
            Diagnostic::new(DiagnosticLevel::Error, "\\left has no matching \\right")
                .with_location(line, col),
        );
    }
}

/// Check an environment node for issues
fn check_environment(
    node: &SyntaxNode,
    source: &str,
    line_offsets: &[usize],
    source_map: &SourceMap,
    declared: &HashSet<String>,
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
            let (line, col) = offset_to_location(source_map.original_offset(offset), line_offsets);

            result.add(
                Diagnostic::new(DiagnosticLevel::Warning, message)
                    .with_location(line, col)
                    .with_source(format!("\\begin{{{}}}", env_name)),
            );
        }
    }

    // Recurse into children
    check_node(node, source, line_offsets, source_map, declared, result);
}

/// Check for unbalanced braces
fn check_brace_balance(input: &str, result: &mut CheckResult) {
    let mut depth = 0i32;
    let mut last_open_line = 0;
    // `check_latex` has already blanked the literal regions, so a brace inside a
    // comment, a `\verb` span or a verbatim body never reaches here.
    let line_offsets = compute_line_offsets(input);
    // `\{` and `\}` are literal braces; `\\{` is a line break, then a group.
    let mut escaped = false;

    for (offset, c) in input.char_indices() {
        if std::mem::take(&mut escaped) {
            continue;
        }
        match c {
            '\\' => escaped = true,
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

/// Blank what LaTeX never interprets — `%` comments, `\verb` spans, and
/// verbatim-family BODIES — so the balance scanners cannot be fooled by literal
/// text. The result keeps the exact byte length, so reported positions stay
/// valid; the L2T `shield_verbatim_regions` lexer cannot be reused because its
/// short marker tokens would shift every later offset. Only the body is
/// blanked, so a genuinely unclosed `verbatim` is still reported.
fn mask_literal_regions(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = bytes.to_vec();

    // Overwrite a byte range with spaces, keeping newlines so line structure
    // (and any later `%` handling) is unaffected. Ranges always start and end on
    // a char boundary, so writing ASCII spaces keeps the buffer valid UTF-8.
    fn blank(out: &mut [u8], range: std::ops::Range<usize>) {
        for byte in &mut out[range] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }

    let mut i = 0usize;
    let mut literal_header = 0..0;
    while i < bytes.len() {
        // Dispatch on the buffer being written, not the original bytes: an
        // already-blanked region must not be re-interpreted, or a `%` inside a
        // masked body starts a "comment" that eats the closing `\end`. Blanking
        // only writes spaces, so this is strictly more conservative.
        match out[i] {
            // A `%` reached here is a real comment: the escape branch below
            // consumes `\%` as a unit, so it never arrives.
            b'%' => {
                let end = input[i..].find('\n').map_or(bytes.len(), |p| i + p);
                // Keep header comment boundaries; MiTeX needs blank trivia in tags.
                let start = if literal_header.contains(&i) {
                    i + 1
                } else {
                    i
                };
                blank(&mut out, start..end);
                i = end;
            }
            b'\\' => {
                if let Some(verb) = inline_verb(input, i) {
                    blank(&mut out, i..verb.end);
                    i = verb.end;
                } else if let Some(env) = literal_env(input, i) {
                    literal_header = env.header;
                    blank(&mut out, env.body);
                    // Resume just past the control word: the opening tag may
                    // carry its own `%` separator, which the branch above must
                    // still blank. Re-walking the blanked body is harmless.
                    i += "\\begin".len();
                } else {
                    // Any other command or escaped char: step over the
                    // backslash together with the next byte so an escaped `%`
                    // is not mistaken for a comment. A multi-byte char's
                    // continuation bytes fall through the catch-all below.
                    i += if i + 1 < bytes.len() { 2 } else { 1 };
                }
            }
            _ => i += 1,
        }
    }

    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

/// Check for unbalanced environments
fn check_environment_balance(input: &str, result: &mut CheckResult) {
    let mut env_stack: Vec<(String, usize)> = Vec::new();
    // Literal regions are already blanked by `check_latex`; the mask is
    // byte-for-byte the same length, so offsets here refer to the real source.
    let line_offsets = compute_line_offsets(input);

    // Walk the markers in source order through the shared tag parser, so the
    // spaced and comment-separated forms are recognised exactly as the mask
    // recognises them. Always handle whichever marker comes first: seeking the
    // next `\begin` first skips an `\end` and desynchronises the stack on the
    // second same-named environment (issue #38).
    let mut pos = 0;
    while pos < input.len() {
        let begin = find_env_tag(input, pos, "\\begin");
        let end = find_env_tag(input, pos, "\\end");

        let handle_begin = match (&begin, &end) {
            (None, None) => break,
            (Some((b, _)), Some((e, _))) => b < e,
            (Some(_), None) => true,
            (None, Some(_)) => false,
        };

        if handle_begin {
            let (abs_pos, tag) = begin.unwrap();
            let (line, _) = offset_to_location(abs_pos, &line_offsets);
            env_stack.push((tag.name.to_string(), line));
            pos = tag.end;
        } else {
            let (abs_pos, tag) = end.unwrap();
            let env_name = tag.name;
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

            pos = tag.end;
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
    fn test_escaped_braces_are_not_counted() {
        assert!(!check_latex(r"\left\{ a \right.").has_errors());
        assert!(!check_latex(r"\{ a").has_errors());
        // A line break `\\` does not escape the brace after it.
        assert!(check_latex(r"a \\{ b").has_errors());
    }

    #[test]
    fn test_structural_math_errors() {
        for invalid in [
            r"x^",
            r"\text_a b",
            r"\frac{a}",
            r"x_a_b",
            r"x^a^b",
            r"x_a^b_c",
            r"\left( a",
            r"a \right)",
            r"\left( a \right) \right)",
            r"\left( x \right$",
            r"\left( x \right",
            r"\left( x \right a",
            r"\left( x \right{",
            r"\left$ x \right)",
            r"\left a x \right)",
            r"\sqrt",
            r"\sqrt[3]",
            r"\rule{1cm}",
            r"\section",
            r"\section[short]",
        ] {
            assert!(check_latex(invalid).has_errors(), "{invalid}");
        }
        for valid in [
            r"x^{}",
            r"x_a^b",
            r"{x_a}_b",
            r"\frac12",
            r"\left( a \right.",
            r"\left\uparrow x \right\Downarrow",
            r"\left\llbracket x \right\rrbracket",
            r"\sqrt{x}",
            r"\sqrt[3]{x}",
            r"\rule[0.5ex]{1cm}{1pt}",
            r"\section[short]{title}",
            r"\dd",
            r"\dv{x}",
            r"\pdv{f}{x}",
            r"\sum\limits_{i}^{n}",
            r"x'^2",
            r"\frac{1}{2}\limits",
            r"\overset{a}{b}\limits",
            r"\frac{1}{2}\over x",
        ] {
            let report = check_latex(valid);
            assert!(
                !report.has_errors(),
                "{valid}: {:?}",
                error_messages(&report)
            );
        }
    }

    #[test]
    fn test_double_script_suggestions_match_kind() {
        let sub = check_latex(r"x_a_b");
        assert!(
            sub.diagnostics.iter().any(|d| d.suggestion.as_deref()
                == Some("Brace the first script's base and script: {x_a}_b")),
            "{sub:?}"
        );

        let sup = check_latex(r"x^a^b");
        assert!(
            sup.diagnostics.iter().any(|d| d.suggestion.as_deref()
                == Some("Brace the first script's base and script: {x^a}^b")),
            "{sup:?}"
        );
    }

    #[test]
    fn test_opaque_document_command_arguments() {
        for valid in [
            r"\label{eq:my_first_eq}",
            r"\ref{fig:plot_a_b}",
            r"\cite{smith_2020_a}",
            r"\url{https://example.com/a_b_c}",
            r"\includegraphics{plot_a_b.png}",
            r"\href{https://example.com/a_b_c}{ok}",
            r"\hyperref[sec:a_b_c]{ok}",
        ] {
            let report = check_latex(valid);
            assert!(
                !report.has_errors(),
                "{valid}: {:?}",
                error_messages(&report)
            );
        }

        for invalid in [
            r"\href{https://example.com/a_b_c}{$x_a_b$}",
            r"\hyperref[sec:a_b_c]{$x_a_b$}",
        ] {
            assert!(check_latex(invalid).has_errors(), "{invalid}");
        }
    }

    #[test]
    fn test_structural_errors_preserve_recovered_source_locations() {
        for (source, needle) in [
            (r"\hskip 2pt + \left( x", r"\left"),
            (r"__\limits + x_a_b", "_b"),
            (r"\sqrt(2) + a \right)", r"\right"),
        ] {
            let report = check_latex(source);
            let expected_col = source.find(needle).unwrap() + 1;
            let err = report
                .diagnostics
                .iter()
                .find(|d| d.level == DiagnosticLevel::Error && d.column == Some(expected_col));
            assert!(
                err.is_some(),
                "expected error at col {expected_col} for {source:?}, got: {:?}",
                report
                    .diagnostics
                    .iter()
                    .map(|d| (d.level, d.line, d.column, &d.message))
                    .collect::<Vec<_>>()
            );
        }
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
    fn test_repeated_environment_is_not_a_false_positive() {
        // Issue #38: the same environment appearing twice used to desynchronise
        // the balance tracker, producing spurious mismatch/unclosed errors on a
        // perfectly valid document.
        let result = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             \\begin{equation}\nx=1\n\\end{equation}\n\
             \\begin{equation}\nz=3\n\\end{equation}\n\
             \\end{document}",
        );
        assert!(
            !result.has_errors(),
            "two equation blocks must not trigger balance errors, got: {:?}",
            result
                .diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_interleaved_mismatch_still_detected() {
        // The reordering fix must not mask a genuine crossing mismatch.
        let result = check_latex(r"\begin{a}\begin{b}\end{a}\end{b}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.message.contains("mismatch")),
            "crossed environments should still be flagged"
        );
    }

    /// Messages of all error-level diagnostics, for readable assertions.
    fn error_messages(result: &CheckResult) -> Vec<String> {
        result
            .diagnostics
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .map(|d| d.message.clone())
            .collect()
    }

    #[test]
    fn test_commented_environment_is_not_counted() {
        // A `\begin` inside a `%` comment is not real structure.
        let result = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             % \\begin{equation}\n\
             Real text.\n\
             \\end{document}\n",
        );
        assert!(
            !result.has_errors(),
            "commented-out environment must not be counted, got: {:?}",
            error_messages(&result)
        );
    }

    #[test]
    fn test_escaped_percent_does_not_start_a_comment() {
        // `\%` is a literal percent, so the rest of the line is still source
        // and its environment must still be tracked.
        let result = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             50\\% off \\begin{equation}\nx\n\\end{equation}\n\
             \\end{document}\n",
        );
        assert!(
            !result.has_errors(),
            "an escaped percent must not swallow the line, got: {:?}",
            error_messages(&result)
        );

        // And the converse: the environment after `\%` is genuinely tracked, so
        // leaving it unclosed is still reported.
        let unclosed = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             50\\% off \\begin{equation}\nx\n\
             \\end{document}\n",
        );
        assert!(
            unclosed.has_errors(),
            "an unclosed environment after `\\%` must still be reported"
        );
    }

    #[test]
    fn test_inline_verb_content_is_not_counted() {
        for source in [
            r"Text \verb|\begin{align}| here.",
            r"Text \verb*+\end{document}+ here.",
            r"Braces \verb|{{{| here.",
        ] {
            let result = check_latex(&format!(
                "\\documentclass{{article}}\n\\begin{{document}}\n{source}\n\\end{{document}}\n"
            ));
            assert!(
                !result.has_errors(),
                "`\\verb` content must be literal, got: {:?} for {source}",
                error_messages(&result)
            );
        }
    }

    #[test]
    fn test_verbatim_family_bodies_are_not_counted() {
        // `minted` is opened with its required language argument; a bare
        // `\begin{minted}` is invalid LaTeX and is covered separately below.
        for (env, opener) in [
            ("verbatim", "\\begin{verbatim}"),
            ("Verbatim", "\\begin{Verbatim}"),
            ("lstlisting", "\\begin{lstlisting}"),
            ("minted", "\\begin{minted}{python}"),
        ] {
            let result = check_latex(&format!(
                "\\documentclass{{article}}\n\\begin{{document}}\n\
                 {opener}\n\\begin{{equation}}\n}}{{\n\\end{{{env}}}\n\
                 \\end{{document}}\n"
            ));
            assert!(
                !result.has_errors(),
                "{env} body must be literal, got: {:?}",
                error_messages(&result)
            );
        }
    }

    #[test]
    fn test_separated_env_tags_are_recognised() {
        // TeX skips whitespace and `%` comments between a control word and its
        // argument, so these open a real verbatim whose body must be literal.
        for opener in [
            "\\begin {verbatim}\n\\begin{equation}\n\\end {verbatim}",
            "\\begin% note\n{verbatim}\n\\begin{equation}\n\\end% note\n{verbatim}",
        ] {
            let result = check_latex(&format!(
                "\\documentclass{{article}}\n\\begin{{document}}\n{opener}\n\\end{{document}}\n"
            ));
            assert!(
                !result.has_errors(),
                "separated tags must be recognised, got: {:?} for {opener}",
                error_messages(&result)
            );
        }
    }

    #[test]
    fn test_separated_tags_are_balanced_like_tight_ones() {
        // The shared parser means the balance scanner sees these too: a spaced
        // `\begin` with no matching `\end` is still an unclosed environment.
        let result = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             \\begin {equation}\nx\n\
             \\end{document}\n",
        );
        assert!(
            result.has_errors(),
            "a spaced `\\begin` must still be tracked, got: {:?}",
            error_messages(&result)
        );
    }

    #[test]
    fn test_incomplete_literal_env_header_is_not_masked() {
        // A malformed header must NOT be treated as "body and therefore
        // literal", or the unclosed group would be silently swallowed.
        let minted = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             \\begin{minted}{python\n\\end{minted}\n\
             \\end{document}\n",
        );
        assert!(
            minted.has_errors(),
            "an unclosed minted language group must be reported, got: {:?}",
            error_messages(&minted)
        );

        // An unterminated `[` has no dedicated checker, so the guarantee here is
        // the one that matters: the block is NOT masked, so its contents keep
        // being checked instead of being silently swallowed as "body".
        let listing = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             \\begin{lstlisting}[language=Python\n\
             \\begin{equation}\n\\end{lstlisting}\n\
             \\end{document}\n",
        );
        assert!(
            error_messages(&listing)
                .iter()
                .any(|m| m.contains("equation")),
            "a malformed header must leave the body checked, got: {:?}",
            error_messages(&listing)
        );
    }

    #[test]
    fn test_well_formed_literal_env_headers_still_mask_the_body() {
        // The converse of the check above: a valid header must still shield its
        // body, including the optional/required argument forms.
        for opener in [
            "\\begin{minted}{python}",
            "\\begin{minted}[linenos]{python}",
            "\\begin{lstlisting}[language=Python]",
            "\\begin{Verbatim}[frame=single]",
        ] {
            let env = opener
                .trim_start_matches("\\begin{")
                .split('}')
                .next()
                .unwrap();
            let result = check_latex(&format!(
                "\\documentclass{{article}}\n\\begin{{document}}\n\
                 {opener}\n\\begin{{equation}}\n}}{{\n\\end{{{env}}}\n\
                 \\end{{document}}\n"
            ));
            assert!(
                !result.has_errors(),
                "a valid header must shield the body, got: {:?} for {opener}",
                error_messages(&result)
            );
        }
    }

    #[test]
    fn test_commented_minted_header_still_masks_the_body() {
        // TeX discards a comment and its newline while scanning `minted`'s
        // required language argument. The apparent equation below is code, not
        // a real environment that the balance checker should report.
        let result = check_latex(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{minted}% header comment\n{python}\n\
             \\begin{equation}\n\\end{minted}\n\\end{document}\n",
        );
        assert!(
            !result.has_errors(),
            "a valid commented minted header must shield its body, got: {:?}",
            error_messages(&result)
        );
    }

    #[test]
    fn test_alltt_body_is_still_checked() {
        // `alltt` expands commands, so its body is real LaTeX: an unclosed
        // environment inside it must still be reported.
        let result = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             \\begin{alltt}\n\\begin{equation}\n\\end{alltt}\n\
             \\end{document}\n",
        );
        assert!(
            result.has_errors(),
            "alltt content must keep being checked, got: {:?}",
            error_messages(&result)
        );
    }

    #[test]
    fn test_comment_in_literal_body_does_not_swallow_the_closing_tag() {
        // The mask must not re-interpret what it has already blanked. Reading
        // the original bytes let a `%` inside the body start a "comment" that
        // ran past the body and blanked the `\end{verbatim}` on the same line,
        // reporting a well-formed block as unclosed.
        let source = "\\documentclass{article}\n\
                      \\begin{document}\n\
                      \\begin{verbatim}\n% \\end{verbatim}\n\
                      \\end{document}\n";
        assert!(
            mask_literal_regions(source).contains("\\end{verbatim}"),
            "the closing tag must survive masking: {:?}",
            mask_literal_regions(source)
        );
        let result = check_latex(source);
        assert!(
            !error_messages(&result)
                .iter()
                .any(|m| m.contains("verbatim")),
            "the block is closed and must not be reported, got: {:?}",
            error_messages(&result)
        );
    }

    #[test]
    fn test_unclosed_verbatim_is_still_reported() {
        // Masking covers only the BODY: the `\begin` tag survives, so a real
        // unclosed verbatim is still caught rather than silently swallowed.
        let result = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             \\begin{verbatim}\nliteral text\n\
             \\end{document}\n",
        );
        assert!(
            error_messages(&result)
                .iter()
                .any(|m| m.contains("verbatim")),
            "an unclosed verbatim must still be reported, got: {:?}",
            error_messages(&result)
        );
    }

    #[test]
    fn test_masking_preserves_byte_offsets_for_line_numbers() {
        // The mask must be byte-for-byte the same length, or every reported
        // line number after a literal region would drift.
        let source = "\\documentclass{article}\n\
                      % comment with braces {{{ and \\begin{equation}\n\
                      \\begin{verbatim}\n\\begin{align}\n\\end{verbatim}\n\
                      Text \\verb|\\begin{x}| more\n\
                      \\begin{document}\n\\end{equation}\n";
        let masked = mask_literal_regions(source);
        assert_eq!(masked.len(), source.len(), "mask must preserve byte length");
        assert_eq!(
            masked.lines().count(),
            source.lines().count(),
            "mask must preserve newlines"
        );
        // The reported line must be the real one (8), not a drifted value.
        let result = check_latex(source);
        assert!(
            result.diagnostics.iter().any(|d| d.line == Some(8)),
            "expected a diagnostic on line 8, got: {:?}",
            result
                .diagnostics
                .iter()
                .map(|d| (d.line, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_multibyte_source_is_masked_without_corruption() {
        // Blanking works on bytes, so a masked range must stay char-aligned.
        let result = check_latex(
            "\\documentclass{article}\n\
             \\begin{document}\n\
             中文 % 注释里的 \\begin{equation}\n\
             更多中文 \\verb|中文{|\n\
             \\end{document}\n",
        );
        assert!(
            !result.has_errors(),
            "multi-byte text must not corrupt masking, got: {:?}",
            error_messages(&result)
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

    #[test]
    fn test_package_hint_respects_usepackage() {
        // Issue #40: a "requires the 'siunitx' package" hint must not fire when
        // the document already loads siunitx, but should still fire when it does not.
        let siunitx_hint = |src: &str| {
            check_latex(src)
                .diagnostics
                .iter()
                .any(|d| d.message.contains("requires the 'siunitx'"))
        };
        assert!(
            !siunitx_hint(r"\usepackage{siunitx}\SI{47}{\micro\henry}"),
            "hint should be suppressed when siunitx is loaded"
        );
        assert!(
            !siunitx_hint(r"\usepackage[per-mode=symbol]{siunitx}\SI{47}{\micro\henry}"),
            "hint should be suppressed even with package options"
        );
        assert!(
            siunitx_hint(r"\SI{47}{\micro\henry}"),
            "hint should fire when siunitx is not loaded"
        );
        assert!(
            siunitx_hint("% \\usepackage{siunitx}\n\\SI{47}{\\micro\\henry}"),
            "a commented-out package declaration must not suppress the hint"
        );
    }
}
