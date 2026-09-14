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

    // Blank the literal regions ONCE and run every layer against the result.
    // This also shields the parse: mitex has no `\verb`/verbatim handling and
    // would report syntax errors for valid literal text. The mask keeps the
    // exact byte length, so offsets stay valid.
    let masked = mask_literal_regions(input);
    let input = masked.as_str();

    // Parse the input
    let tree = mitex_parser::parse(input, MERGED_SPEC.clone());

    // Calculate line offsets for position reporting
    let line_offsets = compute_line_offsets(input);

    // Packages the document actually loads, so a "requires the 'x' package"
    // hint is only emitted when that package is missing (issue #40).
    let declared = collect_declared_packages(input);

    // Walk the AST looking for issues
    check_node(&tree, input, &line_offsets, &declared, &mut result);

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
                    check_command(cmd_node, source, line_offsets, declared, result);
                }
            }

            SyntaxKind::ItemEnv => {
                // Check environment
                if let SyntaxElement::Node(env_node) = &child {
                    check_environment(env_node, source, line_offsets, declared, result);
                }
            }

            _ => {
                // Recurse into child nodes
                if let SyntaxElement::Node(n) = child {
                    check_node(&n, source, line_offsets, declared, result);
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
    declared: &HashSet<String>,
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

    // Recurse into children
    check_node(node, source, line_offsets, declared, result);
}

/// Check an environment node for issues
fn check_environment(
    node: &SyntaxNode,
    source: &str,
    line_offsets: &[usize],
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
            let (line, col) = offset_to_location(offset, line_offsets);

            result.add(
                Diagnostic::new(DiagnosticLevel::Warning, message)
                    .with_location(line, col)
                    .with_source(format!("\\begin{{{}}}", env_name)),
            );
        }
    }

    // Recurse into children
    check_node(node, source, line_offsets, declared, result);
}

/// Check for unbalanced braces
fn check_brace_balance(input: &str, result: &mut CheckResult) {
    let mut depth = 0i32;
    let mut last_open_line = 0;
    // `check_latex` has already blanked the literal regions, so a brace inside a
    // comment, a `\verb` span or a verbatim body never reaches here.
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

/// Environments whose BODY is literal source for diagnostic purposes.
///
/// `alltt` is deliberately absent: it typesets verbatim-like, but commands
/// inside it are still expanded, so its body is real LaTeX whose structure must
/// keep being checked.
const DIAGNOSTIC_LITERAL_ENVS: &[&str] = &[
    "verbatim",
    "verbatim*",
    "Verbatim",
    "Verbatim*",
    "lstlisting",
    "minted",
];

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
                blank(&mut out, i..end);
                i = end;
            }
            b'\\' => {
                if let Some(end) = inline_verb_end(input, i) {
                    blank(&mut out, i..end);
                    i = end;
                } else if let Some(body) = literal_env_body(input, i) {
                    blank(&mut out, body);
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

/// End offset (exclusive) of an inline `\verb`/`\verb*` span starting at
/// `start`, or `None` if this is not one (e.g. `\verbatim`, or an unterminated
/// span, which is left for the normal checks to report).
fn inline_verb_end(input: &str, start: usize) -> Option<usize> {
    let after = input.get(start..)?.strip_prefix("\\verb")?;
    let mut chars = after.char_indices();
    let (_, first) = chars.next()?;
    // A real inline `\verb` is never followed by a letter.
    if first.is_ascii_alphabetic() {
        return None;
    }
    let (delim_offset, delim) = if first == '*' {
        chars.next()?
    } else {
        (0, first)
    };
    let content_start = start + "\\verb".len() + delim_offset + delim.len_utf8();
    let close = input.get(content_start..)?.find(delim)?;
    Some(content_start + close + delim.len_utf8())
}

/// A parsed `\begin`/`\end` tag: the environment name and the offset just past
/// the closing `}` of its name group.
struct EnvTag<'a> {
    name: &'a str,
    end: usize,
}

/// Advance past the whitespace and `%` comments TeX allows between a control
/// word and its argument, so `\begin {verbatim}` and `\begin% note\n{verbatim}`
/// parse exactly like `\begin{verbatim}`.
fn skip_spaces_and_comments(input: &str, start: usize) -> usize {
    let bytes = input.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                index = input[index..]
                    .find('\n')
                    .map_or(bytes.len(), |p| index + p + 1);
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            _ => break,
        }
    }
    index
}

/// Parse the `\begin`/`\end` tag at `start`. The single definition of the tag
/// syntax, shared by the literal-region mask and the balance scanner.
fn parse_env_tag<'a>(input: &'a str, start: usize, keyword: &str) -> Option<EnvTag<'a>> {
    let after_keyword = input.get(start..)?.strip_prefix(keyword)?;
    // A control word is not a prefix of a longer one: `\beginning`, `\endinput`.
    if after_keyword
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let argument = skip_spaces_and_comments(input, start + keyword.len());
    let braced = input.get(argument..)?.strip_prefix('{')?;
    let close = braced.find('}')?;
    Some(EnvTag {
        name: &braced[..close],
        end: argument + '{'.len_utf8() + close + '}'.len_utf8(),
    })
}

/// Find the next position at or after `from` that really parses as a `keyword`
/// tag, skipping look-alikes such as `\beginning`.
fn find_env_tag<'a>(input: &'a str, from: usize, keyword: &str) -> Option<(usize, EnvTag<'a>)> {
    let mut search = from;
    while let Some(relative) = input.get(search..)?.find(keyword) {
        let at = search + relative;
        if let Some(tag) = parse_env_tag(input, at, keyword) {
            return Some((at, tag));
        }
        search = at + keyword.len();
    }
    None
}

/// Skip a literal environment's header arguments, returning `None` when the
/// header is malformed — an incomplete `\begin{minted}{python` must stay
/// unmasked so the brace and environment checks still report it.
fn literal_env_header_end(input: &str, env: &str, after_tag: usize) -> Option<usize> {
    // A bare newline ends the header: consuming it could reinterpret the first
    // literal body line as an argument. A `%` comment is different in TeX: it
    // consumes its newline while the header is still being scanned, so an
    // optional or required header group may begin on the next physical line.
    fn skip_header_trivia(input: &str, start: usize) -> usize {
        let bytes = input.as_bytes();
        let mut index = start;
        loop {
            while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
                index += 1;
            }
            if bytes.get(index) != Some(&b'%') {
                return index;
            }
            index = input[index..]
                .find('\n')
                .map_or(input.len(), |newline| index + newline + 1);
        }
    }

    // Delimited group that must be closed if it is opened at all.
    fn skip_group(
        input: &str,
        start: usize,
        open: char,
        close: char,
        required: bool,
    ) -> Option<usize> {
        let at = skip_header_trivia(input, start);
        match input.get(at..)?.strip_prefix(open) {
            Some(rest) => {
                let end = rest.find(close)?;
                Some(at + open.len_utf8() + end + close.len_utf8())
            }
            None if required => None,
            None => Some(start),
        }
    }

    let mut position = after_tag;
    // fancyvrb, listings and minted all take an optional `[...]` option list.
    if matches!(env, "Verbatim" | "Verbatim*" | "lstlisting" | "minted") {
        position = skip_group(input, position, '[', ']', false)?;
    }
    // minted additionally requires `{<language>}`.
    if env == "minted" {
        position = skip_group(input, position, '{', '}', true)?;
    }
    Some(position)
}

/// Byte range of a verbatim-family environment's literal BODY. The `\begin` and
/// `\end` tags are excluded, so an unclosed one is still reported; an
/// unterminated block treats the remainder as literal but keeps its `\begin`.
fn literal_env_body(input: &str, start: usize) -> Option<std::ops::Range<usize>> {
    let tag = parse_env_tag(input, start, "\\begin")?;
    if !DIAGNOSTIC_LITERAL_ENVS.contains(&tag.name) {
        return None;
    }
    let body_start = literal_env_header_end(input, tag.name, tag.end)?;

    // Locate the matching `\end`, tolerating the same separators as the opener.
    let mut search = body_start;
    loop {
        match find_env_tag(input, search, "\\end") {
            Some((at, end_tag)) => {
                if end_tag.name == tag.name {
                    return Some(body_start..at);
                }
                search = end_tag.end;
            }
            // Unterminated: the remainder is literal, but the `\begin` survives
            // so the environment is still reported as unclosed.
            None => return Some(body_start..input.len()),
        }
    }
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
