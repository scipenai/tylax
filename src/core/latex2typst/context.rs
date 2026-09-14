//! Core state and structures for LaTeX to Typst conversion
//!
//! This module contains the main converter struct and conversion state.

use mitex_parser::syntax::{CmdItem, SyntaxElement, SyntaxKind, SyntaxNode};
use mitex_parser::CommandSpec;
use mitex_spec::CommandSpecItem;
use mitex_spec_gen::DEFAULT_SPEC;
use rowan::ast::AstNode;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::data::constants::{AcronymDef, GlossaryDef};
use crate::data::extended_symbols::EXTENDED_SYMBOLS;
use crate::data::maps::TEX_COMMAND_SPEC;
use crate::features::refs::{CitationMode, ReferenceType};
use fxhash::FxHashMap;
use lazy_static::lazy_static;

use super::engine::{ArgumentErrorType, EngineWarning};
use super::{ConversionResult, ConversionWarning, WarningKind};

use super::utils::{
    clean_whitespace, convert_caption_text, extract_arg_content, extract_arg_content_with_braces,
    extract_curly_inner_content, protect_zero_arg_commands, restore_protected_commands,
};

// =============================================================================
// LaTeX → Typst Conversion Options
// =============================================================================

/// Controls how the *style* preamble (`#set page` / `#set heading` /
/// `#set math.equation` / `#import polylux`) is emitted at the top of a
/// converted document.
///
/// This does **not** affect document metadata (`#set document(title:..)`)
/// or the rendered title block — those are derived from the LaTeX source
/// (`\title`, `\author`) and stay regardless of this setting.
#[derive(Debug, Clone, Default)]
pub enum PreambleMode {
    /// Emit the default style preamble for the detected document class.
    #[default]
    Default,
    /// Emit no style preamble at all. Body content only.
    None,
    /// Emit the given string verbatim in place of the default preamble,
    /// followed by one blank line. The caller is responsible for the
    /// content (e.g. providing alternative `#set` rules).
    Custom(String),
}

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

    /// Expand LaTeX macros before parsing
    /// When true, macros defined with \newcommand, \def, etc. are expanded
    /// Default: true
    pub expand_macros: bool,

    /// Controls emission of the style preamble in document mode.
    /// Default: [`PreambleMode::Default`]
    pub preamble: PreambleMode,
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
            expand_macros: true,
            preamble: PreambleMode::Default,
        }
    }
}

/// Build the default style preamble for the given LaTeX document class.
fn default_style_preamble(document_class: Option<&str>) -> String {
    match document_class.unwrap_or("article") {
        "report" | "book" => "#set page(paper: \"a4\")\n\
             #set heading(numbering: \"1.1\")\n\
             #set math.equation(numbering: \"(1)\")\n\n"
            .to_string(),
        "beamer" => "#import \"@preview/polylux:0.3.1\": *\n\
             #set page(paper: \"presentation-16-9\")\n\n"
            .to_string(),
        _ => "#set page(paper: \"a4\")\n\
             #set heading(numbering: \"1.\")\n\
             #set math.equation(numbering: \"(1)\")\n\n"
            .to_string(),
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
            infty_to_oo: true,
            ..Self::default()
        }
    }

    /// Create options for maximum compatibility (verbose output)
    pub fn verbose() -> Self {
        Self {
            prefer_shorthands: false,
            frac_to_slash: false,
            optimize: false,
            ..Self::default()
        }
    }

    /// Create strict mode options (errors on unknown commands)
    pub fn strict() -> Self {
        Self {
            non_strict: false,
            ..Self::default()
        }
    }

    /// Create options with macro expansion disabled
    pub fn no_expand() -> Self {
        Self {
            expand_macros: false,
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

/// Pending citation state for commands whose arguments are emitted as following siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCitation {
    pub mode: CitationMode,
    pub optional_args: Vec<String>,
    pub current_optional_raw: String,
    pub collecting_optional: bool,
}

/// Pending reference state for commands whose label argument is emitted as a following curly group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingReference {
    pub ref_type: ReferenceType,
}

/// Shape of a siunitx command, used to format its collected arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiunitxKind {
    /// `\SI{value}{unit}` / `\qty{value}{unit}` → `value "unit"`.
    NumberUnit,
    /// `\si{unit}` / `\unit{unit}` → just the unit.
    UnitOnly,
    /// `\num{value}` → just the number.
    NumberOnly,
    /// `\ang{degrees}` → `value°`.
    Angle,
}

/// Pending siunitx state; mitex has no argument pattern, so groups arrive as siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSiunitx {
    pub kind: SiunitxKind,
    pub needed: usize,
    pub args: Vec<String>,
    /// Nesting depth while skipping a leading optional siunitx configuration.
    pub optional_bracket_depth: usize,
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
    /// Pending citation state
    pub pending_citation: Option<PendingCitation>,
    /// Pending reference state
    pub pending_reference: Option<PendingReference>,
    /// Suppress the next raw whitespace token after emitting a spacing-aware token.
    pub suppress_next_space: bool,
    /// User-defined macros
    pub macros: HashMap<String, MacroDef>,
    /// Whether we're in preamble
    pub in_preamble: bool,
    /// Document metadata
    pub title: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
    pub document_class: Option<String>,
    /// Collected structured warnings
    pub structured_warnings: Vec<ConversionWarning>,
    /// Legacy string warnings (for compatibility)
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

/// One argument of an environment's `\begin{..}` header.
pub(crate) struct EnvHeaderArg {
    /// `true` for a bracketed `[..]` slot, `false` for a braced `{..}` one.
    pub(crate) optional: bool,
    pub(crate) content: String,
}

/// Parse the argument slots of an environment header, in source order.
///
/// Slots come from the environment's signature in the command spec
/// (`EnvShape`), so mixed, repeated and out-of-order optional/required slots
/// all parse; converters read the result instead of guessing at a leading
/// bracket. mitex attaches the bound slots to the `\begin` marker, so they are
/// collected from there rather than from the environment's children.
pub(crate) fn env_header_args(node: &SyntaxNode) -> Vec<EnvHeaderArg> {
    let mut args = Vec::new();
    for child in node.children() {
        if child.kind() != SyntaxKind::ItemBegin {
            continue;
        }
        for slot in child.children() {
            if slot.kind() != SyntaxKind::ClauseArgument {
                continue;
            }
            let optional = slot
                .children()
                .any(|item| item.kind() == SyntaxKind::ItemBracket);
            let braced = slot
                .children()
                .any(|item| item.kind() == SyntaxKind::ItemCurly);
            if optional || braced {
                args.push(EnvHeaderArg {
                    optional,
                    content: extract_arg_content(&slot),
                });
            }
        }
    }
    args
}

/// A starred sectioning command whose title group has not been reached yet,
/// and which form is waiting.
///
/// mitex's argument patterns know only term/bracket/paren kinds, so the `*` is
/// bound as the title, and a greedy `RangeLenTerm` is worse still. The star is
/// treated as a modifier and the title taken from the next group (issue #45).
#[derive(Debug, Clone, Copy)]
pub(crate) enum PendingSection {
    /// `\section*`..`\paragraph*`, `\chapter*`: unnumbered heading at this 0-based depth.
    Heading { level: u8 },
    /// `\part*`: the centred part block, without its "Part N" line or a part number.
    Part,
    /// `\subparagraph*`: run-in italics, which carry no number either way.
    Subparagraph,
}
impl ConversionState {
    /// Add a structured warning
    pub fn add_warning(&mut self, warning: ConversionWarning) {
        self.structured_warnings.push(warning);
    }

    /// Take all structured warnings
    pub fn take_structured_warnings(&mut self) -> Vec<ConversionWarning> {
        std::mem::take(&mut self.structured_warnings)
    }
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
    /// Half-collected siunitx arguments, carried across sibling nodes.
    pub(crate) siunitx_arg_collector: Option<PendingSiunitx>,
    /// A starred sectioning command whose title has not been reached yet.
    pub(crate) pending_section: Option<PendingSection>,
}

/// A `ClauseArgument` is a *required* argument iff it does not carry an
/// optional-bracket payload. This covers both braced (`{...}`) and unbraced
/// single-token forms (`\frac 12`, `\frac\alpha\beta`, `\hat x`).
///
/// Two signals indicate a *non*-required (optional `[...]`) clause:
///   - the clause has an `ItemBracket` child (well-formed `[...]`)
///   - or the clause's first token is `[`, which happens when mitex parses a
///     command from the merged spec where the user wrote `[` without a matching
///     `]` consumable within the argument boundary (e.g. `\citep[see]{a}` - the
///     `[see]` is not part of the required-arg slot in mitex's spec, so the
///     parser emits a `ClauseArgument` containing only the bare `[` token).
///     We must reject this case, or downstream code mistakes `"["` for the
///     real required argument and the citation handler in `markup.rs` is
///     bypassed.
fn is_required_clause(child: &SyntaxNode) -> bool {
    child.kind() == SyntaxKind::ClauseArgument
        && !child
            .children()
            .any(|c| c.kind() == SyntaxKind::ItemBracket)
        && !matches!(
            child.first_token().map(|t| t.kind()),
            Some(SyntaxKind::TokenLBracket)
        )
}

fn slash_is_between_mathy(elem: SyntaxElement) -> bool {
    let Some(prev) = nearest_non_trivia_sibling(&elem, false) else {
        return false;
    };
    let Some(next) = nearest_non_trivia_sibling(&elem, true) else {
        return false;
    };

    elements_form_math_slash(&prev, &next)
}

fn nearest_non_trivia_sibling(elem: &SyntaxElement, next: bool) -> Option<SyntaxElement> {
    let mut sibling = if next {
        elem.next_sibling_or_token()
    } else {
        elem.prev_sibling_or_token()
    };

    while let Some(current) = sibling {
        if !matches!(
            current.kind(),
            SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak | SyntaxKind::TokenComment
        ) {
            return Some(current);
        }
        sibling = if next {
            current.next_sibling_or_token()
        } else {
            current.prev_sibling_or_token()
        };
    }

    None
}

fn element_is_mathy(elem: &SyntaxElement) -> bool {
    match elem.kind() {
        SyntaxKind::ItemLR | SyntaxKind::ItemAttachComponent => true,
        SyntaxKind::ItemCmd => {
            let Some(node) = elem.as_node() else {
                return false;
            };
            let Some(cmd) = CmdItem::cast(node.clone()) else {
                return false;
            };
            let Some(name) = cmd.name_tok() else {
                return false;
            };
            command_is_math_like(name.text().trim_start_matches('\\'))
        }
        _ => false,
    }
}

/// Glue a lone identifier/number to a following `(`/`[` (`f (x)` -> `f(x)`),
/// but keep the space after a multi-letter token: `tilde (b)` (relation `~` on
/// a group, from `\sim (b)`) must not collapse to the accent call `tilde(b)`
/// (issue #34).
fn glue_lone_identifier_calls(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // <ident-char> <whitespace-run> `(`/`[`
        if c.is_whitespace() && i > 0 && chars[i - 1].is_ascii_alphanumeric() {
            let mut after_space = i;
            while chars
                .get(after_space)
                .is_some_and(|next| next.is_whitespace())
            {
                after_space += 1;
            }
            // Lone token: the char before it is absent or non-alphanumeric.
            let is_lone = i < 2 || !chars[i - 2].is_ascii_alphanumeric();
            if is_lone && matches!(chars.get(after_space), Some('(') | Some('[')) {
                i = after_space; // drop the space
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Split `s` on top-level occurrences of `sep`, respecting nesting of `()`,
/// `[]`, and `{}` so a separator inside e.g. `frac(a, b)` is left alone.
fn split_top_level(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            _ if c == sep && depth == 0 => {
                parts.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    parts.push(cur.trim().to_string());
    parts
}

/// Escape commas at the top level of a `cases` row. Inside a row that already
/// has explicit `&` columns, a comma is literal content, not another column or
/// row separator. Nested function/group commas remain untouched.
fn escape_top_level_case_commas(s: &str) -> String {
    let mut depth = 0i32;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                out.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                out.push(c);
            }
            ',' if depth == 0 => out.push_str("\\,"),
            _ => out.push(c),
        }
    }
    out
}

/// If `s` is exactly a single call `name(...)` — the paren matching the opening
/// one is the final character — return the inner argument text; else `None`.
fn strip_call<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let inner = s.strip_prefix(name)?.strip_prefix('(')?.strip_suffix(')')?;
    // Reject `name(a)(b)`: the first '(' must stay open until the end.
    let mut depth = 1i32;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return None;
        }
    }
    Some(inner)
}

/// Protect complete TikZ environments before macro expansion and AST parsing.
///
/// MiTeX represents an environment semantically: its `SyntaxNode::text()`
/// omits delimiters such as the braces in `\begin{tikzpicture}`, node option
/// brackets, and coordinate parentheses. Passing that reconstructed text to
/// the TikZ parser loses the first command in the picture. TikZ is its own
/// language, so preserve each complete source block verbatim, convert it with
/// the dedicated parser, and restore the rendered CeTZ after document output
/// has been built.
fn shield_tikz_blocks(
    input: &str,
    protected: &[(usize, usize)],
) -> (String, Vec<(String, String)>) {
    const BEGIN: &str = r"\begin{tikzpicture}";
    const END: &str = r"\end{tikzpicture}";

    let mut out = String::with_capacity(input.len());
    let mut rendered_blocks = Vec::new();
    let mut cursor = 0;

    // `protected` holds lstlisting/minted bodies; verbatim and `\verb` are shielded upstream.
    while let Some(begin) = find_uncommented_latex_command(input, BEGIN, cursor, protected) {
        let content_start = begin + BEGIN.len();
        let Some(end) = find_uncommented_latex_command(input, END, content_start, protected) else {
            // Leave an unterminated environment to the normal diagnostics.
            break;
        };
        let block_end = end + END.len();
        // Private-use delimiters cannot collide with prose yet survive MiTeX as text.
        let marker = format!("\u{E010}TylaxTikzBlock{}X\u{E011}", rendered_blocks.len());

        out.push_str(&input[cursor..begin]);
        out.push_str(&marker);
        rendered_blocks.push((
            marker,
            format!(
                "\n// TikZ converted to CeTZ\n{}\n",
                crate::tikz::convert_tikz_to_cetz(&input[begin..block_end])
            ),
        ));
        cursor = block_end;
    }

    out.push_str(&input[cursor..]);
    (out, rendered_blocks)
}

/// Find `needle` after `from`, skipping comments and `protected` ranges.
fn find_uncommented_latex_command(
    input: &str,
    needle: &str,
    from: usize,
    protected: &[(usize, usize)],
) -> Option<usize> {
    let mut search_from = from;
    while let Some(relative) = input[search_from..].find(needle) {
        let position = search_from + relative;
        if !is_latex_comment_position(input, position)
            && !is_position_protected(position, protected)
        {
            return Some(position);
        }
        search_from = position + needle.len();
    }
    None
}

/// Whether `position` falls within any `[start, end)` protected range.
fn is_position_protected(position: usize, protected: &[(usize, usize)]) -> bool {
    protected
        .iter()
        .any(|&(start, end)| position >= start && position < end)
}

/// Environments whose body is literal source, markered before MiTeX. `alltt` is excluded.
const TRUE_VERBATIM_ENVS: [&str; 4] = ["verbatim", "verbatim*", "Verbatim", "Verbatim*"];

/// fancyvrb environments taking a leading `[key=val]`; plain `verbatim` takes none.
const FANCYVRB_ENVS: [&str; 2] = ["Verbatim", "Verbatim*"];

/// Environments MiTeX converts semantically but whose body is literal, so the scan skips it.
const SKIP_SCAN_ENVS: [&str; 2] = ["lstlisting", "minted"];

/// Result of [`shield_verbatim_regions`].
struct VerbatimShield {
    /// Source with true verbatim and inline `\verb` replaced by opaque markers.
    source: String,
    /// Marker → Typst-raw restorations to splice back after the document builds.
    restorations: Vec<(String, String)>,
    /// Byte ranges of `lstlisting`/`minted` bodies: kept for MiTeX, skipped by the TikZ scan.
    tikz_skip_ranges: Vec<(usize, usize)>,
}

/// Shield verbatim-like source regions before any LaTeX interpretation.
///
/// A single left-to-right lexical scan recognizes real command tokens, `%` line
/// comments, and inline `\verb`, so a `\begin{verbatim}` shown inside a comment
/// (or a `\verb` span) is never mistaken for a real environment, and a `%` that
/// is itself inside verbatim stays literal.
fn shield_verbatim_regions(input: &str) -> VerbatimShield {
    let mut out = String::with_capacity(input.len());
    let mut restorations: Vec<(String, String)> = Vec::new();
    let mut skip_ranges = Vec::new();
    let mut i = 0;
    let n = input.len();

    while i < n {
        let rest = &input[i..];
        let b = rest.as_bytes()[0];

        // `%` line comment: copy through end of line; `\%` is consumed by the backslash branch.
        if b == b'%' {
            let line_end = rest.find('\n').map(|r| i + r + 1).unwrap_or(n);
            out.push_str(&input[i..line_end]);
            i = line_end;
            continue;
        }

        if b == b'\\' {
            if let Some((consumed, content)) = parse_inline_verb(rest) {
                let marker = format!("\u{E010}TylaxVerbatim{}X\u{E011}", restorations.len());
                out.push_str(&marker);
                restorations.push((marker, typst_raw_inline(content)));
                i += consumed;
                continue;
            }
            if let Some((consumed, env, body)) = parse_env_block(rest) {
                if TRUE_VERBATIM_ENVS.contains(&env) {
                    // fancyvrb reads a `%` prefix and `[key=val]` header, neither a body.
                    let body = if FANCYVRB_ENVS.contains(&env) {
                        strip_fancyvrb_header(body)
                    } else {
                        body
                    };
                    let marker = format!("\u{E010}TylaxVerbatim{}X\u{E011}", restorations.len());
                    out.push_str(&marker);
                    restorations.push((marker, typst_raw_block(body)));
                    i += consumed;
                    continue;
                }
                if SKIP_SCAN_ENVS.contains(&env) {
                    // Keep the environment for MiTeX, recording its span so the TikZ scan skips it.
                    let start = out.len();
                    out.push_str(&input[i..i + consumed]);
                    skip_ranges.push((start, out.len()));
                    i += consumed;
                    continue;
                }
            }
            // Copy the backslash with its next char, so a following `%` or `{` is not reread.
            out.push('\\');
            i += 1;
            if i < n {
                let l = input[i..].chars().next().unwrap().len_utf8();
                out.push_str(&input[i..i + l]);
                i += l;
            }
            continue;
        }

        let l = rest.chars().next().unwrap().len_utf8();
        out.push_str(&input[i..i + l]);
        i += l;
    }

    VerbatimShield {
        source: out,
        restorations,
        tikz_skip_ranges: skip_ranges,
    }
}

/// Parse a leading inline `\verb`/`\verb*`: bytes consumed and literal content.
fn parse_inline_verb(rest: &str) -> Option<(usize, &str)> {
    let after = rest.strip_prefix(r"\verb")?;
    let mut chars = after.char_indices();
    let (_, first) = chars.next()?;
    // A real inline `\verb` is followed by `*` or a non-letter delimiter, never a letter.
    if first.is_ascii_alphabetic() {
        return None;
    }
    let (delim, delim_start) = if first == '*' {
        let (off, d) = chars.next()?;
        (d, off)
    } else {
        (first, 0usize)
    };
    let content_start = delim_start + delim.len_utf8();
    let region = &after[content_start..];
    let close = region.find(delim)?;
    let content = &region[..close];
    let consumed = r"\verb".len() + content_start + close + delim.len_utf8();
    Some((consumed, content))
}

/// Byte length of the leading run of TeX-ignorable separators in `s`: ASCII
/// whitespace and `%` line comments (through their terminating newline).
/// Used ONLY between the `\begin`/`\end` control word and its `{env}`
/// argument, which TeX reads the same either way, never for body content.
fn ignorable_separator(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'%' => match s[i..].find('\n') {
                Some(rel) => i += rel + 1,
                None => return s.len(),
            },
            _ => break,
        }
    }
    i
}

/// Match `\begin`/`\end` followed by TeX-ignorable separators and `{env}`,
/// returning the byte length of the whole tag if `env` matches. TeX ignores
/// spaces and comments after a control word, so `\begin {verbatim}` is valid.
fn match_env_tag(s: &str, keyword: &str, env: &str) -> Option<usize> {
    let after_kw = s.strip_prefix(keyword)?;
    let sep = ignorable_separator(after_kw);
    let braced = after_kw[sep..].strip_prefix('{')?;
    let close = braced.find('}')?;
    if &braced[..close] != env {
        return None;
    }
    Some(keyword.len() + sep + 1 + close + 1)
}

/// Find the first `\end{ENV}` (separator-tolerant) in `hay`: start offset and tag length.
fn find_env_end(hay: &str, env: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(rel) = hay[from..].find(r"\end") {
        let pos = from + rel;
        if let Some(len) = match_env_tag(&hay[pos..], r"\end", env) {
            return Some((pos, len));
        }
        from = pos + r"\end".len();
    }
    None
}

/// Parse a leading `\begin{ENV}`..`\end{ENV}` block into the bytes consumed,
/// the environment name and the raw body. The first matching `\end` ends it,
/// since verbatim-like environments cannot nest.
fn parse_env_block(rest: &str) -> Option<(usize, &str, &str)> {
    let after_kw = rest.strip_prefix(r"\begin")?;
    let sep = ignorable_separator(after_kw);
    let braced = after_kw[sep..].strip_prefix('{')?;
    let name_end = braced.find('}')?;
    let env = &braced[..name_end];
    if env.is_empty() {
        return None;
    }
    let header_len = r"\begin".len() + sep + 1 + name_end + 1;
    let body_region = &rest[header_len..];
    let (end_rel, end_len) = find_env_end(body_region, env)?;
    let body = &body_region[..end_rel];
    let consumed = header_len + end_rel + end_len;
    Some((consumed, env, body))
}

/// Strip a `[..]` argument only when it follows the tag IMMEDIATELY, as fancyvrb requires.
fn strip_leading_optional_arg(body: &str) -> &str {
    let Some(inner) = body.strip_prefix('[') else {
        return body;
    };
    let mut depth = 1usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return &inner[idx + ch.len_utf8()..];
                }
            }
            _ => {}
        }
    }
    // Unbalanced bracket: not a well-formed optional argument, leave as-is.
    body
}

/// Strip a fancyvrb `Verbatim` header, returning the raw content.
///
/// fancyvrb reads the `[key=val]` argument in non-verbatim mode, so a `%`
/// comment ending the `\begin{Verbatim}` line is honored and the argument may
/// follow on the next line (checked against TeX Live `pdflatex`). A bare newline
/// ends the header instead, making the next line literal body.
fn strip_fancyvrb_header(body: &str) -> &str {
    let mut rest = body;
    // Consume comment-only header lines: optional spaces, `%`, through the newline.
    loop {
        let after_hspace = rest.trim_start_matches([' ', '\t']);
        let Some(after_pct) = after_hspace.strip_prefix('%') else {
            break;
        };
        rest = match after_pct.find('\n') {
            Some(nl) => &after_pct[nl + 1..],
            None => &after_pct[after_pct.len()..],
        };
    }
    // With no comment consumed `rest == body`, enforcing the exact-start rule.
    strip_leading_optional_arg(rest)
}

/// Longest run of consecutive backticks in `s`, used to size a raw fence.
fn max_backtick_run(s: &str) -> usize {
    let mut max = 0usize;
    let mut cur = 0usize;
    for c in s.chars() {
        if c == '`' {
            cur += 1;
            max = max.max(cur);
        } else {
            cur = 0;
        }
    }
    max
}

/// Render literal text as a fenced Typst raw block; the fence grows past any backtick run.
fn typst_raw_block(body: &str) -> String {
    // Drop one newline adjacent to each tag so the block is tight; keep interior bytes.
    let body = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body);
    let body = body
        .strip_suffix("\r\n")
        .or_else(|| body.strip_suffix('\n'))
        .unwrap_or(body);
    let fence = "`".repeat(max_backtick_run(body).max(2) + 1);
    format!("\n{fence}\n{body}\n{fence}\n")
}

/// Render literal text as inline Typst raw, growing the fence past any backtick run.
fn typst_raw_inline(content: &str) -> String {
    let ticks = max_backtick_run(content) + 1;
    let fence = "`".repeat(ticks);
    if ticks == 1 {
        format!("{fence}{content}{fence}")
    } else {
        // A multi-backtick raw span trims one edge space, so the guards keep backticks apart.
        format!("{fence} {content} {fence}")
    }
}

/// Whether `position` sits after a real LaTeX comment marker on its line.
fn is_latex_comment_position(input: &str, position: usize) -> bool {
    let line_start = input[..position].rfind('\n').map_or(0, |index| index + 1);
    let mut preceding_backslashes = 0usize;

    for character in input[line_start..position].chars() {
        match character {
            '\\' => preceding_backslashes += 1,
            '%' if preceding_backslashes & 1 == 0 => return true,
            _ => preceding_backslashes = 0,
        }
    }
    false
}

/// String fallback that turns an already-converted `\left\{ ... \right.` body
/// into `cases(...)` arguments, for shapes the AST path (see math.rs
/// `classify_lr_cases_environment`) doesn't catch: `atop(a, b)`, a bare
/// expression, or a leftover `mat(...)`. A row without an explicit `&` treats a
/// top-level comma as a column hint; a row with `&` keeps commas as content.
fn body_to_cases(body: &str) -> String {
    let body = body.trim();

    let rows: Vec<String> = if let Some(inner) = strip_call(body, "mat") {
        let inner = inner.trim();
        // Drop a leading `delim: #none` keyword argument, if present.
        let inner = inner
            .strip_prefix("delim: #none,")
            .or_else(|| inner.strip_prefix("delim: #none ,"))
            .map(str::trim)
            .unwrap_or(inner);
        split_top_level(inner, ';')
    } else if let Some(inner) = strip_call(body, "atop") {
        split_top_level(inner, ',')
    } else {
        // aligned / plain body: rows are separated by Typst line breaks `\`.
        body.split('\\').map(|r| r.trim().to_string()).collect()
    };

    let cells: Vec<String> = rows
        .iter()
        .map(|r| r.trim_start_matches('&').trim())
        .filter(|r| !r.is_empty())
        .map(|r| {
            if r.contains('&') {
                escape_top_level_case_commas(r)
            } else {
                split_top_level(r, ',')
                    .into_iter()
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>()
                    .join(" & ")
            }
        })
        .collect();

    format!("cases({})", cells.join(", "))
}

/// The verbatim source text of a syntax element (node or token).
fn element_source_text(el: &SyntaxElement) -> String {
    match el {
        SyntaxElement::Node(n) => n.text().to_string(),
        SyntaxElement::Token(t) => t.text().to_string(),
    }
}

/// Whether `el` is a command with exactly `name` (without its leading slash).
fn is_command_named(el: &SyntaxElement, name: &str) -> bool {
    let Some(node) = el.as_node() else {
        return false;
    };
    CmdItem::cast(node.clone())
        .and_then(|cmd| cmd.name_tok())
        .is_some_and(|token| token.text().trim_start_matches('\\') == name)
}

/// Whether `s` is a TeX dimension (`6pt`, `-1.5em`, `0.5 ex`) or a length
/// command (`\baselineskip`). Used to drop the optional row-spacing arg of `\\`
/// (`\\[6pt]`) instead of leaking it into a matrix/aligned body (issue #41).
/// Conservative: needs a numeric factor + known unit, or a control sequence;
/// any other bracket group is left untouched.
fn is_tex_dimension(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    // A control sequence may be a user length (\baselineskip); can't resolve
    // without macro expansion, so accept it.
    if s.starts_with('\\') {
        return true;
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
    }
    let num_start = i;
    let mut saw_digit = false;
    let mut saw_decimal_point = false;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            saw_digit = true;
            i += 1;
        } else if bytes[i] == b'.' && !saw_decimal_point {
            saw_decimal_point = true;
            i += 1;
        } else {
            break;
        }
    }
    if i == num_start || !saw_digit {
        return false; // no numeric factor -> not a dimension
    }
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    // TeX accepts an optional `true` modifier (e.g. `1truept`).
    let unit = s[i..]
        .strip_prefix("true")
        .map(str::trim_start)
        .unwrap_or(&s[i..]);
    // Elastic glue units are valid too; long forms precede `fil` for correct
    // suffix matching.
    const UNITS: [&str; 16] = [
        "filll", "fill", "fil", "pt", "pc", "mm", "cm", "in", "ex", "em", "bp", "dd", "cc", "sp",
        "mu", "nd",
    ];
    UNITS.iter().any(|u| {
        let Some(after_unit) = unit.strip_prefix(u) else {
            return false;
        };
        after_unit.is_empty()
            || after_unit.starts_with('\\')
            || after_unit.chars().next().is_some_and(char::is_whitespace)
    })
}

fn command_is_math_like(name: &str) -> bool {
    let base_name = name.strip_suffix('*').unwrap_or(name);

    if let Some(CommandSpecItem::Cmd(shape)) = TEX_COMMAND_SPEC.get(name) {
        if shape.alias.is_some() {
            return true;
        }
    }

    if EXTENDED_SYMBOLS.contains_key(name) || EXTENDED_SYMBOLS.contains_key(base_name) {
        return true;
    }

    matches!(
        base_name,
        "abs"
            | "absolutevalue"
            | "bar"
            | "binom"
            | "bm"
            | "boldsymbol"
            | "bra"
            | "braket"
            | "cfrac"
            | "dfrac"
            | "dyad"
            | "ev"
            | "expval"
            | "expectationvalue"
            | "flatfrac"
            | "frac"
            | "hat"
            | "innerproduct"
            | "ip"
            | "ket"
            | "ketbra"
            | "matrixel"
            | "matrixelement"
            | "matrixquantity"
            | "mathbb"
            | "mathbf"
            | "mathcal"
            | "mathfrak"
            | "mathit"
            | "mathop"
            | "mathrm"
            | "mathsf"
            | "mathtt"
            | "mel"
            | "norm"
            | "op"
            | "outerproduct"
            | "overbrace"
            | "overleftarrow"
            | "overleftrightarrow"
            | "overline"
            | "overrightarrow"
            | "overset"
            | "qty"
            | "sqrt"
            | "stackrel"
            | "tilde"
            | "tfrac"
            | "underbrace"
            | "underset"
            | "vec"
            | "widehat"
            | "widetilde"
    )
}

fn elements_form_math_slash(prev: &SyntaxElement, next: &SyntaxElement) -> bool {
    let prev_mathy = element_is_mathy(prev);
    let next_mathy = element_is_mathy(next);
    let prev_word = prev.kind() == SyntaxKind::TokenWord;
    let next_word = next.kind() == SyntaxKind::TokenWord;

    (prev_mathy && (next_mathy || next_word)) || (prev_word && next_mathy)
}

impl LatexConverter {
    /// Create a new converter with default options
    pub fn new() -> Self {
        Self {
            state: ConversionState::new(),
            spec: MERGED_SPEC.clone(),
            siunitx_arg_collector: None,
            pending_section: None,
        }
    }

    /// Create a new converter with custom options
    pub fn with_options(options: L2TOptions) -> Self {
        let mut state = ConversionState::new();
        state.options = options;
        Self {
            state,
            spec: MERGED_SPEC.clone(),
            siunitx_arg_collector: None,
            pending_section: None,
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

    /// Preprocess input with optional macro expansion
    ///
    /// If `expand_macros` is enabled in options, this will:
    /// 1. Tokenize the input
    /// 2. Expand all macro definitions and invocations
    /// 3. Collect any warnings from the expansion process
    /// 4. Return the expanded string
    ///
    /// Otherwise, returns the input unchanged.
    fn preprocess_expansion(&mut self, input: &str, math_mode: bool) -> String {
        if self.state.options.expand_macros {
            let result =
                crate::core::latex2typst::engine::expand_latex_with_warnings(input, math_mode);

            // Convert structured engine warnings to conversion warnings (type-safe!)
            for engine_warning in result.warnings {
                let warning = Self::convert_engine_warning(&engine_warning);
                // Keep legacy string warning for compatibility
                self.state.warnings.push(engine_warning.message());
                self.state.structured_warnings.push(warning);
            }

            result.output
        } else {
            input.to_string()
        }
    }

    /// Convert a structured engine warning to a conversion warning.
    ///
    /// This is a type-safe mapping - no string parsing required!
    fn convert_engine_warning(warning: &EngineWarning) -> ConversionWarning {
        match warning {
            EngineWarning::DepthExceeded { max_depth } => ConversionWarning::new(
                WarningKind::MacroLoop,
                format!(
                    "Macro expansion depth exceeded maximum ({}). Possible infinite recursion.",
                    max_depth
                ),
            ),
            EngineWarning::TokenLimitExceeded { max_tokens } => ConversionWarning::new(
                WarningKind::MacroLoop,
                format!(
                    "Macro expansion produced too many tokens (exceeded {}). Possible infinite loop or exponential expansion.",
                    max_tokens
                ),
            ),
            EngineWarning::ArgumentParsingFailed {
            macro_name,
            error_kind,
        } => {
            let kind = match error_kind {
                ArgumentErrorType::RunawayArgument => WarningKind::RunawayArgument,
                ArgumentErrorType::PatternMismatch => WarningKind::PatternMismatch,
                ArgumentErrorType::Other(_) => WarningKind::ParseError,
            };
            ConversionWarning::new(
                kind,
                format!(
                    "Macro '\\{}' argument parsing failed: {}",
                    macro_name, error_kind
                ),
            )
            .with_location(format!("\\{}", macro_name))
        }
            EngineWarning::LaTeX3Skipped { token_count } => ConversionWarning::new(
                WarningKind::LaTeX3Skipped,
                format!(
                    "LaTeX3 block (\\ExplSyntaxOn ... \\ExplSyntaxOff) skipped ({} tokens). \
                        LaTeX3/expl3 syntax is not supported.",
                    token_count
                ),
            ),
            EngineWarning::UnsupportedPrimitive { name } => ConversionWarning::new(
                WarningKind::UnsupportedPrimitive,
                format!(
                    "Unsupported TeX primitive '\\{}' encountered. \
                        This may produce incorrect output.",
                    name
                ),
            )
            .with_location(format!("\\{}", name)),
            EngineWarning::LetTargetNotFound { name, target } => ConversionWarning::new(
                WarningKind::UnsupportedMacro,
                format!(
                    "\\let\\{}\\{}: target '\\{}' not found. \
                        Built-in LaTeX commands cannot be copied with \\let.",
                    name, target, target
                ),
            )
            .with_location(format!("\\let\\{}\\{}", name, target)),
        }
    }

    /// Check if input contains a real `\begin{document}` that is not commented out.
    ///
    /// Reuses the verbatim lexer's boundary rules: [`match_env_tag`] accepts the
    /// TeX-ignorable separators (whitespace and `%` comments) that may sit between
    /// the `\begin` control word and its `{document}` argument, so `\begin {document}`
    /// and `\begin% c\n{document}` are recognized. A `\begin` that is itself after a
    /// `%` comment on its line is ignored via [`is_latex_comment_position`] (which
    /// is escaped-`\%`-aware). Verbatim examples of `\begin{document}` are already
    /// replaced by markers before this runs, so any remaining match is real.
    fn has_real_begin_document(input: &str) -> bool {
        let mut from = 0;
        while let Some(rel) = input[from..].find(r"\begin") {
            let pos = from + rel;
            if match_env_tag(&input[pos..], r"\begin", "document").is_some()
                && !is_latex_comment_position(input, pos)
            {
                return true;
            }
            from = pos + r"\begin".len();
        }
        false
    }

    /// Reset all per-conversion state before a new top-level conversion.
    ///
    /// A converter may be reused across documents, so every input-derived field
    /// must start clean or macros, counters, citations and warnings leak into
    /// the next one. Replacing the whole `ConversionState` (rather than
    /// clearing fields individually) keeps a reused converter identical to a
    /// fresh one and resets future fields automatically.
    fn reset_conversion_state(&mut self) {
        self.state = ConversionState {
            options: self.state.options.clone(),
            ..ConversionState::default()
        };
        self.siunitx_arg_collector = None;
        self.pending_section = None;
    }

    /// Convert a complete LaTeX document to Typst
    pub fn convert_document(&mut self, input: &str) -> String {
        // A reused converter must start each document identical to a fresh one.
        self.reset_conversion_state();

        // Shield verbatim and `\verb` first; lstlisting/minted stay for MiTeX.
        let verbatim = shield_verbatim_regions(input);

        // Enter preamble mode only for a real `\begin{document}`, read from shielded source.
        self.state.in_preamble = Self::has_real_begin_document(&verbatim.source);

        // Preserve raw TikZ before expansion: its punctuation is unrecoverable afterwards.
        let (tikz_protected_input, rendered_tikz_blocks) =
            shield_tikz_blocks(&verbatim.source, &verbatim.tikz_skip_ranges);

        // Preprocess: protect zero-argument commands that MiTeX would otherwise lose.
        let protected_input = protect_zero_arg_commands(&tikz_protected_input);

        // Optionally expand macros using the SOTA token-based engine
        // This correctly handles nested braces and complex macro arguments
        let expanded_input = self.preprocess_expansion(&protected_input, false);

        // Parse with mitex-parser
        let tree = mitex_parser::parse(&expanded_input, self.spec.clone());

        // Convert AST to Typst with pre-allocated buffer
        let estimated_size = (expanded_input.len() as f64 * 1.5) as usize;
        let mut output = String::with_capacity(estimated_size.max(1024));

        // Walk the tree
        self.visit_node(&tree, &mut output);

        // Build final document with preamble
        let mut result = self.build_document(output);

        // Restore TikZ only after cleanup: the markers travel as inert text.
        for (marker, rendered) in rendered_tikz_blocks {
            result = result.replace(&marker, &rendered);
        }

        // Restore shielded verbatim after TikZ, so literal bodies never re-enter a scan.
        for (marker, raw) in verbatim.restorations {
            result = result.replace(&marker, &raw);
        }

        // Restore protected commands
        restore_protected_commands(&result)
    }

    /// Convert math-only LaTeX to Typst
    pub fn convert_math(&mut self, input: &str) -> String {
        self.reset_conversion_state();
        self.state.mode = ConversionMode::Math;
        self.state.in_preamble = false;

        // Optionally expand macros with math mode enabled
        let expanded_input = self.preprocess_expansion(input, true);

        // Parse
        let tree = mitex_parser::parse(&expanded_input, self.spec.clone());

        // Convert with pre-allocated buffer
        let mut output = String::with_capacity(expanded_input.len().max(256));
        self.visit_node(&tree, &mut output);

        // Post-process
        self.postprocess_math(output)
    }

    /// Visit a syntax node and convert it.
    pub fn visit_node(&mut self, node: &SyntaxNode, output: &mut String) {
        let children: Vec<SyntaxElement> = node.children_with_tokens().collect();
        self.visit_elements(&children, output);
    }

    /// Visit a sequence of elements, keeping TeX declaration scopes intact.
    ///
    /// `\displaystyle`/`\textstyle` are declarations (mitex emits the affected
    /// expression as siblings): wrap the rendered suffix in `display(..)`/
    /// `inline(..)`. A nested group re-enters here, so it can't leak out (#42).
    pub fn visit_elements(&mut self, children: &[SyntaxElement], output: &mut String) {
        let mut index = 0;
        while index < children.len() {
            let child = &children[index];
            let style = if matches!(self.state.mode, ConversionMode::Math) {
                if is_command_named(child, "displaystyle") {
                    Some("display")
                } else if is_command_named(child, "textstyle") {
                    Some("inline")
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(style) = style {
                let mut styled_content = String::new();
                self.visit_elements(&children[index + 1..], &mut styled_content);
                let styled_content = styled_content.trim();
                if !styled_content.is_empty() {
                    let _ = write!(output, "{}({})", style, styled_content);
                }
                return;
            }

            self.visit_element(child.clone(), output);
            index += 1;
        }
    }

    fn handle_pending_citation(&mut self, elem: SyntaxElement, output: &mut String) -> bool {
        let Some(mut pending) = self.state.pending_citation.take() else {
            return false;
        };

        match elem.kind() {
            SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak
                if pending.collecting_optional =>
            {
                if !pending.current_optional_raw.ends_with(' ') {
                    pending.current_optional_raw.push(' ');
                }
                self.state.pending_citation = Some(pending);
                true
            }
            SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak => {
                self.state.pending_citation = Some(pending);
                true
            }
            SyntaxKind::TokenAsterisk => {
                self.state.pending_citation = Some(pending);
                true
            }
            SyntaxKind::TokenLBracket if !pending.collecting_optional => {
                pending.collecting_optional = true;
                pending.current_optional_raw.clear();
                self.state.pending_citation = Some(pending);
                true
            }
            SyntaxKind::TokenRBracket if pending.collecting_optional => {
                pending
                    .optional_args
                    .push(pending.current_optional_raw.trim().to_string());
                pending.current_optional_raw.clear();
                pending.collecting_optional = false;
                self.state.pending_citation = Some(pending);
                true
            }
            SyntaxKind::ItemCurly if !pending.collecting_optional => {
                if let SyntaxElement::Node(node) = elem {
                    super::markup::emit_pending_citation_from_curly(&node, pending, output);
                    return true;
                }
                self.state.pending_citation = Some(pending);
                false
            }
            _ if pending.collecting_optional => {
                match &elem {
                    SyntaxElement::Node(node) => pending
                        .current_optional_raw
                        .push_str(&node.text().to_string()),
                    SyntaxElement::Token(token) => {
                        pending.current_optional_raw.push_str(token.text())
                    }
                }
                self.state.pending_citation = Some(pending);
                true
            }
            _ => {
                self.state.pending_citation = Some(pending);
                false
            }
        }
    }

    /// Take the title group following a starred sectioning command and emit
    /// the heading. TeX skips whitespace AND `%` comments before the argument
    /// it scans for, so `\section* % note\n{Title}` is `\section*{Title}`.
    fn handle_pending_section(&mut self, elem: SyntaxElement, output: &mut String) -> bool {
        let Some(pending) = self.pending_section.take() else {
            return false;
        };

        match elem.kind() {
            SyntaxKind::TokenWhiteSpace
            | SyntaxKind::TokenLineBreak
            | SyntaxKind::TokenComment
            | SyntaxKind::ItemBlockComment
            | SyntaxKind::TokenAsterisk => {
                self.pending_section = Some(pending);
                true
            }
            SyntaxKind::ItemCurly => {
                if let SyntaxElement::Node(node) = elem {
                    // Convert the title so markup inside it is translated, not copied.
                    let body: Vec<SyntaxElement> = node
                        .children_with_tokens()
                        .filter(|child| {
                            !matches!(
                                child.kind(),
                                SyntaxKind::TokenLBrace | SyntaxKind::TokenRBrace
                            )
                        })
                        .collect();
                    let mut title = String::new();
                    self.visit_elements(&body, &mut title);
                    super::markup::emit_starred_section(pending, title.trim(), output);
                    return true;
                }
                self.pending_section = Some(pending);
                false
            }
            // No title group follows (`\section*` alone): emit nothing rather than invent it.
            _ => false,
        }
    }

    fn handle_pending_reference(&mut self, elem: SyntaxElement, output: &mut String) -> bool {
        let Some(pending) = self.state.pending_reference.take() else {
            return false;
        };

        match elem.kind() {
            SyntaxKind::TokenWhiteSpace
            | SyntaxKind::TokenLineBreak
            | SyntaxKind::TokenAsterisk => {
                self.state.pending_reference = Some(pending);
                true
            }
            SyntaxKind::ItemCurly => {
                if let SyntaxElement::Node(node) = elem {
                    super::markup::emit_pending_reference_from_curly(&node, pending, output);
                    return true;
                }
                self.state.pending_reference = Some(pending);
                false
            }
            _ => {
                self.state.pending_reference = Some(pending);
                false
            }
        }
    }

    /// Consume the curly-sibling arguments of a pending siunitx command.
    fn handle_pending_siunitx(&mut self, elem: SyntaxElement, output: &mut String) -> bool {
        let Some(mut pending) = self.siunitx_arg_collector.take() else {
            return false;
        };

        if pending.optional_bracket_depth > 0 {
            match elem.kind() {
                SyntaxKind::TokenLBracket => pending.optional_bracket_depth += 1,
                SyntaxKind::TokenRBracket => pending.optional_bracket_depth -= 1,
                _ => {}
            }
            self.siunitx_arg_collector = Some(pending);
            return true;
        }

        match elem.kind() {
            // Whitespace and TeX comments are transparent to argument collection.
            SyntaxKind::TokenWhiteSpace
            | SyntaxKind::TokenLineBreak
            | SyntaxKind::TokenComment
            | SyntaxKind::ItemBlockComment => {
                self.siunitx_arg_collector = Some(pending);
                true
            }
            SyntaxKind::TokenLBracket => {
                pending.optional_bracket_depth = 1;
                self.siunitx_arg_collector = Some(pending);
                true
            }
            // siunitx allows a key-value configuration; Typst has none, so skip it.
            SyntaxKind::ItemBracket => {
                self.siunitx_arg_collector = Some(pending);
                true
            }
            // mitex may wrap the same optional bracket in a `ClauseArgument`.
            SyntaxKind::ClauseArgument
                if elem.as_node().is_some_and(|node| {
                    node.children()
                        .any(|child| child.kind() == SyntaxKind::ItemBracket)
                }) =>
            {
                self.siunitx_arg_collector = Some(pending);
                true
            }
            SyntaxKind::ItemCurly => {
                if let SyntaxElement::Node(node) = &elem {
                    let raw = super::utils::extract_curly_inner_content(node);
                    pending.args.push(raw);
                    if pending.args.len() >= pending.needed {
                        super::markup::emit_siunitx(self, &pending, output);
                    } else {
                        self.siunitx_arg_collector = Some(pending);
                    }
                    return true;
                }
                // Not a node: flush what we have and let the element fall through.
                super::markup::emit_siunitx(self, &pending, output);
                false
            }
            // A non-group element means arguments are missing; flush and handle it.
            _ => {
                super::markup::emit_siunitx(self, &pending, output);
                false
            }
        }
    }

    /// Visit a syntax element (node or token)
    pub fn visit_element(&mut self, elem: SyntaxElement, output: &mut String) {
        use SyntaxKind::*;

        if self.handle_pending_citation(elem.clone(), output) {
            return;
        }
        if self.handle_pending_reference(elem.clone(), output) {
            return;
        }
        if self.handle_pending_section(elem.clone(), output) {
            return;
        }
        if self.handle_pending_siunitx(elem.clone(), output) {
            return;
        }

        if self.state.suppress_next_space && !matches!(elem.kind(), SyntaxKind::TokenWhiteSpace) {
            self.state.suppress_next_space = false;
        }

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
                    if self.state.suppress_next_space {
                        self.state.suppress_next_space = false;
                    } else {
                        output.push_str(t.text());
                    }
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
            TokenComma => {
                if matches!(self.state.current_env(), EnvironmentContext::Cases) {
                    while output.ends_with(char::is_whitespace) {
                        output.pop();
                    }
                    output.push_str("\\,");
                } else {
                    output.push(',');
                }
            }
            TokenSlash => {
                if matches!(self.state.mode, ConversionMode::Math) || slash_is_between_mathy(elem) {
                    while output.ends_with(char::is_whitespace) {
                        output.pop();
                    }
                    output.push_str(" slash ");
                    self.state.suppress_next_space = true;
                } else {
                    output.push('/');
                }
            }
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
            // Brackets here are literal text: a real optional argument is consumed by the grammar.
            TokenLBracket => {
                if !self.state.in_preamble {
                    output.push('[');
                }
            }
            TokenRBracket => {
                if !self.state.in_preamble {
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
            if is_required_clause(&child) {
                if required_count == index {
                    return Some(extract_arg_content(&child));
                }
                required_count += 1;
            }
        }
        None
    }

    /// Get a required argument preserving inner braces
    pub fn get_required_arg_with_braces(&self, cmd: &CmdItem, index: usize) -> Option<String> {
        let mut required_count = 0;
        for child in cmd.syntax().children() {
            if is_required_clause(&child) {
                if required_count == index {
                    return Some(extract_arg_content_with_braces(&child));
                }
                required_count += 1;
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

    /// Recursively convert an optional `[...]` argument (bracket-clause analogue
    /// of [`Self::convert_required_arg`]). Unlike [`Self::get_optional_arg`],
    /// which returns raw brace-stripped text and mangles `\textbf{X}` into
    /// `\textbfX`, this handles embedded math/commands — e.g. `\item[$O(n)$]`.
    pub fn convert_optional_arg(&mut self, cmd: &CmdItem, index: usize) -> Option<String> {
        let mut optional_count = 0;
        for child in cmd.syntax().children() {
            if child.kind() != SyntaxKind::ClauseArgument {
                continue;
            }
            let Some(bracket) = child
                .children()
                .find(|c| c.kind() == SyntaxKind::ItemBracket)
            else {
                continue;
            };
            if optional_count == index {
                let mut output = String::new();
                let content: Vec<_> = bracket
                    .children_with_tokens()
                    .filter(|element| {
                        !matches!(
                            element.kind(),
                            SyntaxKind::TokenLBracket | SyntaxKind::TokenRBracket
                        )
                    })
                    .collect();
                self.visit_elements(&content, &mut output);
                return Some(output.trim().to_string());
            }
            optional_count += 1;
        }
        None
    }

    /// Convert a required argument - recursively processes the content.
    ///
    /// Handles both braced (`{...}`) and unbraced single-token arguments. Empty
    /// groups are padded with a zero-width space by `visit_element`, keeping the
    /// surrounding Typst construct valid (e.g. `frac(zws, zws)` instead of the
    /// invalid `frac(,)`).
    pub fn convert_required_arg(&mut self, cmd: &CmdItem, index: usize) -> Option<String> {
        let mut required_count = 0;
        for child in cmd.syntax().children() {
            if is_required_clause(&child) {
                if required_count == index {
                    let mut output = String::new();
                    let content: Vec<_> = child
                        .children_with_tokens()
                        .filter(|element| {
                            !matches!(
                                element.kind(),
                                SyntaxKind::TokenLBrace
                                    | SyntaxKind::TokenRBrace
                                    | SyntaxKind::TokenLBracket
                                    | SyntaxKind::TokenRBracket
                            )
                        })
                        .collect();
                    self.visit_elements(&content, &mut output);
                    return Some(output.trim().to_string());
                }
                required_count += 1;
            }
        }
        None
    }

    /// Convert a required *term* argument such as `b` in `\frac{a}b` or `\sim`
    /// in `\overset{p}\sim`. Thin alias of [`Self::convert_required_arg`], which
    /// already handles unbraced single-token terms (and pads empty groups).
    pub fn convert_required_term_arg(&mut self, cmd: &CmdItem, index: usize) -> Option<String> {
        self.convert_required_arg(cmd, index)
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
        env_header_args(node)
            .into_iter()
            .find(|arg| arg.optional)
            .map(|arg| arg.content)
    }

    /// The n-th OPTIONAL argument of the environment header, in source order.
    ///
    /// `\begin{minipage}[pos][height][inner-pos]{width}` has three, so a single
    /// "the optional argument" accessor cannot describe it.
    pub fn get_env_optional_arg_at(&self, node: &SyntaxNode, index: usize) -> Option<String> {
        env_header_args(node)
            .into_iter()
            .filter(|arg| arg.optional)
            .nth(index)
            .map(|arg| arg.content)
    }

    /// Get a required argument from an environment
    pub fn get_env_required_arg(&self, node: &SyntaxNode, index: usize) -> Option<String> {
        env_header_args(node)
            .into_iter()
            .filter(|arg| !arg.optional)
            .nth(index)
            .map(|arg| arg.content)
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

        result = self.fix_symbol_spacing(&result);

        while result.contains("  ") {
            result = result.replace("  ", " ");
        }

        result = result.replace(" ,", ",");
        result = result.replace("( ", "(");
        result = result.replace(" )", ")");
        result = result.replace(" ^", "^");
        result = result.replace(" _", "_");

        // Rewrite `\left\{ ... \right.` (`lr({ ... )`) into Typst `cases(...)`.
        result = self.fix_left_brace_cases(&result);

        // Repair base-less `_`/`^` attachments now that spacing is canonical
        // (`(_(` and leading `^(` are literal). Typst rejects these otherwise.
        result = self.fix_baseless_attachment(&result);

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
        // Only glue a lone identifier/number to a following `(`/`[` (function
        // application like `f (x)` -> `f(x)`). We must NOT strip the space after
        // a multi-letter symbol name: in Typst math `tilde (b)` (relation `~`
        // on a group) and `tilde(b)` (the `tilde` accent call) mean different
        // things, so the space in front of `(` is significant there (issue #34).
        result = glue_lone_identifier_calls(&result);
        result = result.replace(" ^", "^");
        result = result.replace(" _", "_");

        // Rewrite `\left\{ ... \right.` and repair base-less `_`/`^` attachments
        // (see `postprocess_math`); the inline `$...$` path flows through here.
        result = self.fix_left_brace_cases(&result);
        result = self.fix_baseless_attachment(&result);

        result.trim().to_string()
    }

    /// Fix missing spaces before Typst symbol names.
    ///
    /// When a non-letter character (digit, `/`, `)`, `]`, etc.) is immediately followed
    /// by a Typst symbol name (e.g., `angle.l`, `pi`, `theta`), insert a space.
    pub fn fix_symbol_spacing(&self, input: &str) -> String {
        // Common Typst symbol prefixes that need space separation
        // These are symbols that often appear after expressions without spaces
        static SYMBOL_PREFIXES: &[&str] = &[
            "chevron.l",
            "chevron.r",
            "floor.l",
            "floor.r",
            "ceil.l",
            "ceil.r",
            "bracket.l",
            "bracket.r",
            "paren.l",
            "paren.r",
            "alpha",
            "beta",
            "gamma",
            "delta",
            "epsilon",
            "zeta",
            "eta",
            "theta",
            "iota",
            "kappa",
            "lambda",
            "mu",
            "nu",
            "xi",
            "omicron",
            "pi",
            "rho",
            "sigma",
            "tau",
            "upsilon",
            "phi",
            "chi",
            "psi",
            "omega",
            "Alpha",
            "Beta",
            "Gamma",
            "Delta",
            "Epsilon",
            "Zeta",
            "Eta",
            "Theta",
            "Iota",
            "Kappa",
            "Lambda",
            "Mu",
            "Nu",
            "Xi",
            "Omicron",
            "Pi",
            "Rho",
            "Sigma",
            "Tau",
            "Upsilon",
            "Phi",
            "Chi",
            "Psi",
            "Omega",
            "infty",
            "infinity",
            "partial",
            "nabla",
            "forall",
            "exists",
            "emptyset",
            "nothing",
            "dots",
            "cdots",
            "ldots",
            "vdots",
            "ddots",
        ];

        let mut result = input.to_string();

        for symbol in SYMBOL_PREFIXES {
            // Pattern: non-letter/non-space followed by symbol
            // We need to find cases like "2angle.r" or ")pi"
            let mut i = 0;
            while i < result.len() {
                if let Some(pos) = result[i..].find(symbol) {
                    let abs_pos = i + pos;
                    if abs_pos > 0 {
                        let prev_char = result.chars().nth(abs_pos - 1).unwrap_or(' ');
                        // Insert space if previous char is not a letter, space, or opening paren/bracket
                        if !prev_char.is_alphabetic()
                            && prev_char != ' '
                            && prev_char != '('
                            && prev_char != '['
                            && prev_char != '{'
                            && prev_char != '\n'
                            && prev_char != '\t'
                        {
                            // Check that we're not in the middle of a word
                            // e.g., don't change "tangent" when looking for "angle"
                            let after_symbol = abs_pos + symbol.len();
                            let next_char = result.chars().nth(after_symbol);
                            let is_word_boundary =
                                next_char.is_none_or(|c| !c.is_alphanumeric() && c != '.');

                            if is_word_boundary {
                                result.insert(abs_pos, ' ');
                                i = abs_pos + symbol.len() + 2; // Skip past inserted space and symbol
                                continue;
                            }
                        }
                    }
                    i = abs_pos + 1;
                } else {
                    break;
                }
            }
        }

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
        // Byte cursor into `result`. We must advance past every match we
        // process, otherwise a `bb(...)` that rewrites to itself (any letter
        // other than the special number sets, or an empty `bb()`) would be
        // re-found at the same position on the next `find`, looping forever.
        let mut search_from = 0;

        while let Some(rel) = result[search_from..].find("bb(") {
            let start = search_from + rel;
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
                // Resume scanning after the text we just wrote. Guarantees the
                // cursor strictly advances even when the replacement still
                // begins with `bb(`.
                search_from = start + replacement.len();
            } else {
                break;
            }
        }

        result
    }

    /// Repair base-less `_`/`^` attachments that Typst rejects (from OCR-style
    /// input like `V_{_{M-ABF}}` or a `^{a,b}` fragment), in two passes:
    /// collapse a pure double attachment `_(_(X))` -> `_(X)`, then insert an
    /// empty base `""` before any attachment still lacking one.
    pub fn fix_baseless_attachment(&self, input: &str) -> String {
        let collapsed = self.collapse_double_attachment(input);
        self.insert_empty_attachment_base(&collapsed)
    }

    /// Collapse `_(_(X))` -> `_(X)` and `^(^(X))` -> `^(X)` when the inner
    /// attachment is the outer group's only content (a pure wrapper).
    fn collapse_double_attachment(&self, input: &str) -> String {
        let mut result = input.to_string();
        for op in ['_', '^'] {
            // e.g. "_(_(" — an attachment whose content starts with the same
            // base-less attachment.
            let pat = format!("{op}({op}(");
            let mut from = 0;
            while let Some(rel) = result[from..].find(&pat) {
                let start = from + rel; // outer op
                let outer_open = start + 1; // outer '('
                let inner_open = start + 3; // inner '('
                let outer_end = self.find_matching_paren(&result[outer_open + 1..]);
                let inner_end = self.find_matching_paren(&result[inner_open + 1..]);
                if let (Some(o), Some(i)) = (outer_end, inner_end) {
                    let outer_close = outer_open + 1 + o;
                    let inner_close = inner_open + 1 + i;
                    // Pure wrapper: the inner group's ')' is immediately
                    // followed by the outer ')'.
                    if inner_close + 1 == outer_close {
                        let inner_content = result[inner_open + 1..inner_close].to_string();
                        let replacement = format!("{op}({inner_content})");
                        result = format!(
                            "{}{}{}",
                            &result[..start],
                            replacement,
                            &result[outer_close + 1..]
                        );
                        from = start + replacement.len();
                        continue;
                    }
                }
                from = start + 2;
            }
        }
        result
    }

    /// Insert an empty base `""` before a `_(`/`^(` attachment that has no base
    /// (at the start of the string or right after an opening `(`). Both are
    /// positions where Typst would otherwise report an unexpected `_`/`^`.
    fn insert_empty_attachment_base(&self, input: &str) -> String {
        let mut out = String::with_capacity(input.len() + 4);
        let mut last_nonspace: Option<char> = None;
        let mut chars = input.chars().peekable();
        while let Some(c) = chars.next() {
            if (c == '_' || c == '^')
                && chars.peek() == Some(&'(')
                && matches!(last_nonspace, None | Some('('))
            {
                out.push_str("\"\"");
                last_nonspace = Some('"');
            }
            out.push(c);
            if !c.is_whitespace() {
                last_nonspace = Some(c);
            }
        }
        out
    }

    /// Rewrite a `\left\{ ... \right.` piecewise idiom into `cases(...)`. The
    /// converter emits it as `lr({ ... )` with a null right delimiter, so the
    /// `{` never closes and Typst reports "unclosed delimiter". Detect exactly
    /// that shape (an `lr(` whose content opens with an unmatched `{`) and
    /// rewrite it; a balanced `lr({ ... })` (a genuine set) is left alone.
    pub fn fix_left_brace_cases(&self, input: &str) -> String {
        let mut result = input.to_string();
        let mut from = 0;
        while let Some(rel) = result[from..].find("lr(") {
            let lr_start = from + rel;
            let open_paren = lr_start + 2; // the '(' of `lr(`
            let Some(close_rel) = self.find_matching_paren(&result[open_paren + 1..]) else {
                from = lr_start + 3;
                continue;
            };
            let lr_close = open_paren + 1 + close_rel; // the matching ')'
            let inner = &result[open_paren + 1..lr_close];
            let inner_trim = inner.trim_start();
            // Null right delimiter <=> the leading '{' has no matching '}'.
            if let Some(body) = inner_trim.strip_prefix('{') {
                if inner.matches('{').count() > inner.matches('}').count() {
                    let cases = body_to_cases(body.trim());
                    result = format!(
                        "{}{}{}",
                        &result[..lr_start],
                        cases,
                        &result[lr_close + 1..]
                    );
                    from = lr_start + cases.len();
                    continue;
                }
            }
            from = lr_start + 3;
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

        // Style preamble (page / heading / math.equation, plus
        // class-specific imports). Controlled by L2TOptions.preamble.
        match &self.options().preamble {
            PreambleMode::Default => {
                doc.push_str(&default_style_preamble(
                    self.state.document_class.as_deref(),
                ));
            }
            PreambleMode::None => {}
            PreambleMode::Custom(text) => {
                doc.push_str(text.as_str());
                if !text.ends_with("\n\n") {
                    doc.push_str(if text.ends_with('\n') { "\n" } else { "\n\n" });
                }
            }
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
        // Map whole `\macro` names: `\m`/`\s` are prefixes of `\micro`/`\second` (issue #40).
        let chars: Vec<char> = input.chars().collect();
        let mut result = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' {
                let name_start = i + 1;
                let mut j = name_start;
                while j < chars.len() && chars[j].is_ascii_alphabetic() {
                    j += 1;
                }
                let name: String = chars[name_start..j].iter().collect();
                if name.is_empty() {
                    // Lone backslash (e.g. an escaped symbol): keep it verbatim.
                    result.push('\\');
                    i += 1;
                    continue;
                }
                let full: String = chars[i..j].iter().collect(); // includes '\'
                match name.as_str() {
                    "per" => result.push('/'),
                    "squared" => result.push('²'),
                    "cubed" => result.push('³'),
                    _ => {
                        if let Some(val) = crate::siunitx::SI_PREFIXES
                            .get(full.as_str())
                            .or_else(|| crate::siunitx::SI_UNITS.get(full.as_str()))
                        {
                            result.push_str(val);
                        } else {
                            // Unknown unit macro: keep the bare name rather than dropping it.
                            result.push_str(&name);
                        }
                    }
                }
                i = j;
            } else if c.is_whitespace() {
                // Unit strings carry no significant whitespace: `\metre \per \second` is `m/s`.
                i += 1;
            } else {
                result.push(c);
                i += 1;
            }
        }
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
        let children: Vec<SyntaxElement> = node.children_with_tokens().collect();
        let mut content = Vec::with_capacity(children.len());
        let mut i = 0;
        // Header argument slots are bound by the environment signature, so this is body.
        while i < children.len() {
            let child = &children[i];
            match child.kind() {
                SyntaxKind::ItemBegin | SyntaxKind::ItemEnd => {
                    i += 1;
                }
                SyntaxKind::ItemNewLine => {
                    content.push(child.clone());
                    // Drop the optional row-spacing arg of `\\` (`\\[6pt]`),
                    // else it leaks into the row as a stray cell (issue #41).
                    // mitex emits it as sibling `[`/body/`]` tokens or one
                    // `ItemBracket`; `skip_optional_row_spacing` handles both.
                    i = self.skip_optional_row_spacing(&children, i + 1);
                }
                _ => {
                    content.push(child.clone());
                    i += 1;
                }
            }
        }
        self.visit_elements(&content, output);
    }

    /// Index past an optional `[<dimension>]` group at `start`, else `start`.
    /// Only a dimension-like body ([`is_tex_dimension`]) is consumed, so real
    /// bracketed row content survives. `%` comments are trivia like whitespace,
    /// so `\\% note<newline>[6pt]` is `\\[6pt]` (issue #41).
    fn skip_optional_row_spacing(&self, children: &[SyntaxElement], start: usize) -> usize {
        let mut j = start;
        while j < children.len()
            && (matches!(
                children[j].kind(),
                SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak | SyntaxKind::TokenComment
            ) || is_command_named(&children[j], "par"))
        {
            j += 1;
        }
        if j >= children.len() {
            return start;
        }

        if children[j].kind() == SyntaxKind::ItemBracket {
            let raw = element_source_text(&children[j]);
            let is_dimension = raw
                .strip_prefix('[')
                .and_then(|body| body.strip_suffix(']'))
                .is_some_and(is_tex_dimension);
            return if is_dimension { j + 1 } else { start };
        }

        if children[j].kind() != SyntaxKind::TokenLBracket {
            return start;
        }
        let mut k = j + 1;
        let mut body = String::new();
        let mut closed = false;
        while k < children.len() {
            if children[k].kind() == SyntaxKind::TokenRBracket {
                closed = true;
                break;
            }
            body.push_str(&element_source_text(&children[k]));
            k += 1;
        }
        if closed && is_tex_dimension(body.trim()) {
            k + 1 // consume through the closing ']'
        } else {
            start
        }
    }

    // ============================================================
    // Diagnostic conversion methods
    // ============================================================

    /// Convert a complete LaTeX document to Typst with full diagnostics
    ///
    /// Returns both the converted output and any warnings generated during conversion.
    pub fn convert_document_with_diagnostics(&mut self, input: &str) -> ConversionResult {
        let output = self.convert_document(input);
        let warnings = self.state.take_structured_warnings();
        ConversionResult::with_warnings(output, warnings)
    }

    /// Convert math-only LaTeX to Typst with full diagnostics
    ///
    /// Returns both the converted output and any warnings generated during conversion.
    pub fn convert_math_with_diagnostics(&mut self, input: &str) -> ConversionResult {
        let output = self.convert_math(input);
        let warnings = self.state.take_structured_warnings();
        ConversionResult::with_warnings(output, warnings)
    }
}

impl Default for LatexConverter {
    fn default() -> Self {
        Self::new()
    }
}
