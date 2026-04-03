//! Utility functions for Typst to LaTeX conversion
//!
//! Helper functions for text escaping, content extraction, etc.

use crate::data::colors::TYPST_TO_LATEX_COLORS;
use lazy_static::lazy_static;
use std::collections::HashMap;
use typst_syntax::{SyntaxKind, SyntaxNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpacingSpec {
    Fixed(String),
    Flex(String),
}

lazy_static! {
    /// Unicode math characters to LaTeX command mapping
    pub static ref UNICODE_TO_LATEX: HashMap<char, &'static str> = {
        let mut m = HashMap::new();

        // Greek lowercase
        m.insert('α', "\\alpha");
        m.insert('β', "\\beta");
        m.insert('γ', "\\gamma");
        m.insert('δ', "\\delta");
        m.insert('ε', "\\varepsilon");
        m.insert('ϵ', "\\epsilon");
        m.insert('ζ', "\\zeta");
        m.insert('η', "\\eta");
        m.insert('θ', "\\theta");
        m.insert('ϑ', "\\vartheta");
        m.insert('ι', "\\iota");
        m.insert('κ', "\\kappa");
        m.insert('λ', "\\lambda");
        m.insert('μ', "\\mu");
        m.insert('ν', "\\nu");
        m.insert('ξ', "\\xi");
        m.insert('π', "\\pi");
        m.insert('ρ', "\\rho");
        m.insert('ϱ', "\\varrho");
        m.insert('σ', "\\sigma");
        m.insert('ς', "\\varsigma");
        m.insert('τ', "\\tau");
        m.insert('υ', "\\upsilon");
        m.insert('φ', "\\varphi");
        m.insert('ϕ', "\\phi");
        m.insert('χ', "\\chi");
        m.insert('ψ', "\\psi");
        m.insert('ω', "\\omega");

        // Greek uppercase
        m.insert('Α', "A");
        m.insert('Β', "B");
        m.insert('Γ', "\\Gamma");
        m.insert('Δ', "\\Delta");
        m.insert('Ε', "E");
        m.insert('Ζ', "Z");
        m.insert('Η', "H");
        m.insert('Θ', "\\Theta");
        m.insert('Ι', "I");
        m.insert('Κ', "K");
        m.insert('Λ', "\\Lambda");
        m.insert('Μ', "M");
        m.insert('Ν', "N");
        m.insert('Ξ', "\\Xi");
        m.insert('Ο', "O");
        m.insert('Π', "\\Pi");
        m.insert('Ρ', "P");
        m.insert('Σ', "\\Sigma");
        m.insert('Τ', "T");
        m.insert('Υ', "\\Upsilon");
        m.insert('Φ', "\\Phi");
        m.insert('Χ', "X");
        m.insert('Ψ', "\\Psi");
        m.insert('Ω', "\\Omega");

        // Common math symbols
        m.insert('∞', "\\infty");
        m.insert('∂', "\\partial");
        m.insert('∇', "\\nabla");
        m.insert('∈', "\\in");
        m.insert('∉', "\\notin");
        m.insert('∋', "\\ni");
        m.insert('∅', "\\emptyset");
        m.insert('∀', "\\forall");
        m.insert('∃', "\\exists");
        m.insert('¬', "\\neg");
        m.insert('∧', "\\land");
        m.insert('∨', "\\lor");
        m.insert('∩', "\\cap");
        m.insert('∪', "\\cup");
        m.insert('⊂', "\\subset");
        m.insert('⊃', "\\supset");
        m.insert('⊆', "\\subseteq");
        m.insert('⊇', "\\supseteq");
        m.insert('×', "\\times");
        m.insert('÷', "\\div");
        m.insert('±', "\\pm");
        m.insert('∓', "\\mp");
        m.insert('·', "\\cdot");
        m.insert('∘', "\\circ");
        m.insert('⊕', "\\oplus");
        m.insert('⊗', "\\otimes");
        m.insert('†', "\\dagger");
        m.insert('‡', "\\ddagger");
        m.insert('★', "\\star");

        // Relations
        m.insert('≠', "\\neq");
        m.insert('≈', "\\approx");
        m.insert('≡', "\\equiv");
        m.insert('≤', "\\leq");
        m.insert('≥', "\\geq");
        m.insert('≪', "\\ll");
        m.insert('≫', "\\gg");
        m.insert('≺', "\\prec");
        m.insert('≻', "\\succ");
        m.insert('∼', "\\sim");
        m.insert('≃', "\\simeq");
        m.insert('≅', "\\cong");
        m.insert('∝', "\\propto");
        m.insert('⊥', "\\perp");
        m.insert('∥', "\\parallel");
        m.insert('⊢', "\\vdash");
        m.insert('⊣', "\\dashv");
        m.insert('⊨', "\\models");

        // Arrows
        m.insert('→', "\\rightarrow");
        m.insert('←', "\\leftarrow");
        m.insert('↔', "\\leftrightarrow");
        m.insert('⇒', "\\Rightarrow");
        m.insert('⇐', "\\Leftarrow");
        m.insert('⇔', "\\Leftrightarrow");
        m.insert('↦', "\\mapsto");
        m.insert('↑', "\\uparrow");
        m.insert('↓', "\\downarrow");
        m.insert('↗', "\\nearrow");
        m.insert('↘', "\\searrow");
        m.insert('↙', "\\swarrow");
        m.insert('↖', "\\nwarrow");
        m.insert('⟶', "\\longrightarrow");
        m.insert('⟵', "\\longleftarrow");
        m.insert('⟹', "\\Longrightarrow");
        m.insert('⟸', "\\Longleftarrow");

        // Big operators
        m.insert('∑', "\\sum");
        m.insert('∏', "\\prod");
        m.insert('∫', "\\int");
        m.insert('∬', "\\iint");
        m.insert('∭', "\\iiint");
        m.insert('∮', "\\oint");
        m.insert('⋂', "\\bigcap");
        m.insert('⋃', "\\bigcup");
        m.insert('⋀', "\\bigwedge");
        m.insert('⋁', "\\bigvee");

        // Delimiters
        m.insert('⟨', "\\langle");
        m.insert('⟩', "\\rangle");
        m.insert('⌈', "\\lceil");
        m.insert('⌉', "\\rceil");
        m.insert('⌊', "\\lfloor");
        m.insert('⌋', "\\rfloor");
        m.insert('‖', "\\|");

        // Dots
        m.insert('…', "\\ldots");
        m.insert('⋯', "\\cdots");
        m.insert('⋮', "\\vdots");
        m.insert('⋱', "\\ddots");

        // Misc
        m.insert('ℕ', "\\mathbb{N}");
        m.insert('ℤ', "\\mathbb{Z}");
        m.insert('ℚ', "\\mathbb{Q}");
        m.insert('ℝ', "\\mathbb{R}");
        m.insert('ℂ', "\\mathbb{C}");
        m.insert('ℓ', "\\ell");
        m.insert('ℏ', "\\hbar");
        m.insert('℘', "\\wp");
        m.insert('ℑ', "\\Im");
        m.insert('ℜ', "\\Re");
        m.insert('ℵ', "\\aleph");
        m.insert('□', "\\square");
        m.insert('◇', "\\diamond");
        m.insert('△', "\\triangle");
        m.insert('▽', "\\triangledown");
        m.insert('♠', "\\spadesuit");
        m.insert('♥', "\\heartsuit");
        m.insert('♦', "\\diamondsuit");
        m.insert('♣', "\\clubsuit");
        m.insert('′', "'");
        m.insert('″', "''");
        m.insert('°', "^\\circ");

        m
    };
}

/// Escape special LaTeX characters in text
pub fn escape_latex_text(text: &str) -> String {
    text.replace('\\', "\\textbackslash{}")
        .replace('&', "\\&")
        .replace('%', "\\%")
        .replace('$', "\\$")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('~', "\\textasciitilde{}")
        .replace('^', "\\textasciicircum{}")
}

/// Check if a string is a known color name
pub fn is_color_name(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "black"
            | "white"
            | "red"
            | "green"
            | "blue"
            | "yellow"
            | "cyan"
            | "magenta"
            | "orange"
            | "purple"
            | "pink"
            | "brown"
            | "gray"
            | "grey"
            | "lime"
            | "olive"
            | "navy"
            | "teal"
            | "aqua"
            | "maroon"
            | "silver"
            | "fuchsia"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatexColorSpec {
    pub model: Option<&'static str>,
    pub value: String,
}

impl LatexColorSpec {
    pub fn new(model: Option<&'static str>, value: impl Into<String>) -> Self {
        Self {
            model,
            value: value.into(),
        }
    }

    pub fn format_command(&self, command: &str) -> String {
        if let Some(model) = self.model {
            format!("\\{}[{}]{{{}}}", command, model, self.value)
        } else {
            format!("\\{}{{{}}}", command, self.value)
        }
    }
}

pub fn normalize_typst_color_expr(color: &str) -> Option<String> {
    let color = color.trim();

    if is_color_name(color)
        || is_typst_color_method_chain(color)
        || parse_typst_rgb_spec(color).is_some()
        || parse_typst_cmyk_spec(color).is_some()
        || parse_typst_luma_spec(color).is_some()
    {
        Some(color.to_string())
    } else {
        None
    }
}

pub fn format_latex_color_command(command: &str, color: &str) -> String {
    typst_color_to_latex_spec(color).format_command(command)
}

pub fn typst_color_to_latex_spec(color: &str) -> LatexColorSpec {
    let color = color.trim();

    if let Some((model, value)) = parse_typst_rgb_spec(color) {
        return LatexColorSpec::new(Some(model), value);
    }

    if let Some((model, value)) = parse_typst_cmyk_spec(color) {
        return LatexColorSpec::new(Some(model), value);
    }

    if let Some((model, value)) = parse_typst_luma_spec(color) {
        return LatexColorSpec::new(Some(model), value);
    }

    // Handle color expressions like "purple.lighten(80%)" or "red.darken(20%)"
    if let Some(color) = color_method_chain_to_latex(color) {
        return LatexColorSpec::new(None, color);
    }

    LatexColorSpec::new(None, simple_color_to_latex(color))
}

/// Extract percentage from method call like "lighten(80%)" -> 80
fn extract_percentage(method: &str) -> Option<i32> {
    // Find content between ( and )
    let start = method.find('(')?;
    let end = method.find(')')?;
    let content = &method[start + 1..end];
    // Remove % sign and parse
    let num_str = content.trim().trim_end_matches('%');
    num_str.parse::<i32>().ok()
}

/// Simple color name to LaTeX mapping (returns &'static str)
fn simple_color_to_latex(color: &str) -> &'static str {
    // Data-driven mapping from centralized configuration
    if let Some(tex) = TYPST_TO_LATEX_COLORS.get(color.to_lowercase().as_str()) {
        return tex;
    }
    "black" // Default fallback
}

/// Check if a node kind represents string or content
pub fn is_string_or_content(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Str | SyntaxKind::ContentBlock | SyntaxKind::Text | SyntaxKind::Markup
    )
}

/// Get string content from a node (strips quotes)
pub fn get_string_content(node: &SyntaxNode) -> String {
    let text = node.text().to_string();
    text.trim_matches('"').to_string()
}

/// Count heading level from markers (number of = signs)
pub fn count_heading_markers(node: &SyntaxNode) -> usize {
    for child in node.children() {
        if child.kind() == SyntaxKind::HeadingMarker {
            return child
                .text()
                .to_string()
                .chars()
                .filter(|&c| c == '=')
                .count();
        }
    }
    1
}

/// Check if an equation is display math (block math)
/// Display math in Typst starts with `$ ` (space after $) or `$\n` (newline after $)
pub fn is_display_math(node: &SyntaxNode) -> bool {
    // Get full text recursively for newline detection
    let text = get_simple_text(node);

    // Check if there are newlines in the content (multi-line = display math)
    if text.contains('\n') {
        return true;
    }

    // Check the structure of the Equation node
    // Display math: $ followed by Space, then Math, then Space (optional), then $
    // Inline math: $ followed directly by Math content, then $
    let children: Vec<_> = node.children().collect();

    // Look for: [Dollar] [Space] [Math] ...
    // or: [Dollar] [Math] (no space = inline)
    if children.len() >= 2 && children[0].kind() == SyntaxKind::Dollar {
        // Check if the second child is a Space (display math)
        if children[1].kind() == SyntaxKind::Space {
            return true;
        }
    }

    // Fallback: check raw text
    let raw_text = node.text().to_string();
    if raw_text.len() >= 2 {
        let after_dollar = raw_text.chars().nth(1);
        if matches!(after_dollar, Some(' ') | Some('\n') | Some('\r')) {
            return true;
        }
    }

    false
}

/// Get raw text content and language from a raw node
pub fn get_raw_text_with_lang(node: &SyntaxNode) -> (String, Option<String>) {
    // Try to get text from node directly
    let text = node.text().to_string();

    // If node has children, collect text from them (for RawLang, RawTrimmed, etc.)
    let has_children = node.children().count() > 0;
    let collected_text = if has_children {
        let mut result = String::new();
        collect_text(node, &mut result);
        result
    } else {
        text.clone()
    };

    // Use the longer of the two (handles both cases)
    let full_text = if collected_text.len() > text.len() {
        &collected_text
    } else {
        &text
    };

    // Handle fenced code blocks: ```lang\ncode\n```
    if full_text.starts_with("```") {
        // Strip the opening ```
        let after_open = full_text.strip_prefix("```").unwrap_or(full_text);

        // Find the first newline to separate lang from content
        if let Some(newline_pos) = after_open.find('\n') {
            let lang_line = after_open[..newline_pos].trim();
            let lang = if !lang_line.is_empty() {
                Some(lang_line.to_string())
            } else {
                None
            };

            // Get content after language line
            let content = &after_open[newline_pos + 1..];
            // Strip trailing ``` if present
            let content = content.trim_end_matches('`').trim_end();

            return (content.to_string(), lang);
        } else {
            // No newline, single line code block
            let content = after_open.trim_end_matches('`').trim();
            // Check if first word is a language
            if let Some(space_pos) = content.find(char::is_whitespace) {
                let lang = &content[..space_pos];
                let code = content[space_pos..].trim();
                return (code.to_string(), Some(lang.to_string()));
            }
            return (content.to_string(), None);
        }
    }

    // Handle inline code: `code`
    (full_text.trim_matches('`').to_string(), None)
}

/// Check if a node contains actual content (not punctuation/structure)
pub fn is_content_node(node: &SyntaxNode) -> bool {
    !matches!(
        node.kind(),
        SyntaxKind::Comma
            | SyntaxKind::LeftParen
            | SyntaxKind::RightParen
            | SyntaxKind::Space
            | SyntaxKind::Semicolon
    )
}

/// Get simple text representation of a node (recursive)
pub fn get_simple_text(node: &SyntaxNode) -> String {
    let mut result = String::new();
    collect_text(node, &mut result);
    result.trim().to_string()
}

/// Recursively collect text from a node
pub fn collect_text(node: &SyntaxNode, output: &mut String) {
    if node.children().count() > 0 {
        for child in node.children() {
            collect_text(child, output);
        }
    } else {
        output.push_str(node.text().as_str());
    }
}

/// Extract a length value from Typst spacing (e.g., "1em", "10pt", "2cm")
/// Returns the value in LaTeX-compatible format, or None if not recognized
pub fn extract_length_value(text: &str) -> Option<String> {
    let text = text
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    // Common LaTeX-compatible units
    let units = ["em", "ex", "pt", "pc", "in", "cm", "mm", "bp", "sp"];

    for unit in units {
        if text.ends_with(unit) {
            let num_part = text.trim_end_matches(unit).trim();
            // Verify it's a valid number
            if num_part.parse::<f64>().is_ok() {
                return Some(format!("{}{}", num_part, unit));
            }
        }
    }

    // Handle percentage -> \textwidth
    if text.ends_with('%') {
        let num_part = text.trim_end_matches('%').trim();
        if let Ok(percent) = num_part.parse::<f64>() {
            return Some(format!("{:.2}\\textwidth", percent / 100.0));
        }
    }

    None
}

pub fn parse_spacing_spec(text: &str) -> Option<SpacingSpec> {
    let trimmed = text
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    if let Some(value) = extract_length_value(trimmed) {
        return Some(SpacingSpec::Fixed(value));
    }

    if let Some(number) = trimmed.strip_suffix("fr").map(str::trim) {
        if number.parse::<f64>().is_ok() {
            return Some(SpacingSpec::Flex(format!("{}fr", number)));
        }
    }

    None
}

fn is_typst_color_method_chain(color: &str) -> bool {
    let Some((base, rest)) = color.split_once('.') else {
        return false;
    };

    is_color_name(base.trim())
        && rest.split('.').all(|method| {
            method.starts_with("lighten(")
                || method.starts_with("darken(")
                || method.starts_with("transparentize(")
                || method.starts_with("opacify(")
        })
}

fn color_method_chain_to_latex(color: &str) -> Option<String> {
    let (base, rest) = color.split_once('.')?;
    if !is_color_name(base.trim()) {
        return None;
    }

    let mut current = simple_color_to_latex(base.trim()).to_string();
    for method in rest.split('.') {
        if method.starts_with("lighten(") {
            let pct = extract_percentage(method)?;
            let remaining = 100 - pct;
            current = format!("{}!{}!white", current, remaining);
        } else if method.starts_with("darken(") {
            let pct = extract_percentage(method)?;
            let remaining = 100 - pct;
            current = format!("{}!{}!black", current, remaining);
        } else if method.starts_with("transparentize(") || method.starts_with("opacify(") {
            continue;
        } else {
            return None;
        }
    }

    Some(current)
}

fn parse_typst_rgb_spec(color: &str) -> Option<(&'static str, String)> {
    let content = parse_typst_color_func_args(color, "rgb")?;

    if content.len() == 1 {
        let hex = content[0].trim().trim_matches('"').trim_matches('\'');
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(("HTML", hex.to_uppercase()));
        }
        if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let expanded: String = hex
                .chars()
                .flat_map(|c| [c.to_ascii_uppercase(), c.to_ascii_uppercase()])
                .collect();
            return Some(("HTML", expanded));
        }
        return None;
    }

    if content.len() != 3 {
        return None;
    }

    let components: Option<Vec<_>> = content
        .iter()
        .map(|part| parse_color_component(part))
        .collect();
    let components = components?;

    if components
        .iter()
        .all(|component| component.is_percent || component.value <= 1.0)
    {
        let values = components
            .iter()
            .map(|component| {
                let value = if component.is_percent {
                    component.value / 100.0
                } else {
                    component.value
                };
                format_decimal(value)
            })
            .collect::<Vec<_>>()
            .join(",");
        Some(("rgb", values))
    } else if components
        .iter()
        .all(|component| !component.is_percent && component.value <= 255.0)
    {
        let values = components
            .iter()
            .map(|component| format!("{}", component.value.round() as i32))
            .collect::<Vec<_>>()
            .join(",");
        Some(("RGB", values))
    } else {
        None
    }
}

fn parse_typst_cmyk_spec(color: &str) -> Option<(&'static str, String)> {
    let content = parse_typst_color_func_args(color, "cmyk")?;
    if content.len() != 4 {
        return None;
    }

    let components: Option<Vec<_>> = content
        .iter()
        .map(|part| parse_color_component(part))
        .collect();
    let components = components?;

    let values = components
        .iter()
        .map(|component| {
            let value = if component.is_percent {
                component.value / 100.0
            } else {
                component.value
            };
            format_decimal(value)
        })
        .collect::<Vec<_>>()
        .join(",");
    Some(("cmyk", values))
}

fn parse_typst_luma_spec(color: &str) -> Option<(&'static str, String)> {
    let content = parse_typst_color_func_args(color, "luma")?;
    if content.len() != 1 {
        return None;
    }

    let component = parse_color_component(content[0])?;
    let value = if component.is_percent {
        component.value / 100.0
    } else {
        component.value
    };
    Some(("gray", format_decimal(value)))
}

fn parse_typst_color_func_args<'a>(color: &'a str, func: &str) -> Option<Vec<&'a str>> {
    let color = color.trim();
    let prefix = format!("{}(", func);
    if !color.starts_with(&prefix) || !color.ends_with(')') {
        return None;
    }

    let inner = &color[prefix.len()..color.len() - 1];
    Some(
        inner
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect(),
    )
}

#[derive(Debug, Clone, Copy)]
struct ParsedColorComponent {
    value: f64,
    is_percent: bool,
}

fn parse_color_component(value: &str) -> Option<ParsedColorComponent> {
    let value = value.trim();
    if let Some(num) = value.strip_suffix('%') {
        return Some(ParsedColorComponent {
            value: num.trim().parse::<f64>().ok()?,
            is_percent: true,
        });
    }

    Some(ParsedColorComponent {
        value: value.parse::<f64>().ok()?,
        is_percent: false,
    })
}

fn format_decimal(value: f64) -> String {
    let mut formatted = format!("{:.4}", value);
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

/// Parse angle string (e.g., "45deg", "-90deg", "1.57rad") to degrees
pub fn parse_angle_value(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.ends_with("deg") {
        text.trim_end_matches("deg").trim().parse::<f64>().ok()
    } else if text.ends_with("rad") {
        text.trim_end_matches("rad")
            .trim()
            .parse::<f64>()
            .ok()
            .map(|r| r.to_degrees())
    } else {
        text.parse::<f64>().ok()
    }
}

// ============================================================================
// Generic Function Arguments Parser
// ============================================================================

/// Parsed function argument with extracted values
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ParsedArg<'a> {
    /// The argument value as text
    pub value: String,
    /// The argument name (for named arguments)
    pub name: Option<String>,
    /// Whether this is a positional argument
    pub is_positional: bool,
    /// Original AST node for this argument
    pub node: &'a SyntaxNode,
    /// Value AST node for this argument (same as `node` for positional args)
    pub value_node: Option<&'a SyntaxNode>,
    /// All AST nodes contributing to the value, in source order.
    pub value_nodes: Vec<&'a SyntaxNode>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownNamedArgPolicy {
    Ignore,
    Preserve,
    Fallback,
    Warn,
}

/// Generic function arguments parser for Typst AST
/// Provides a unified way to extract named and positional arguments from FuncCall nodes
#[allow(dead_code)]
pub struct FuncArgs<'a> {
    /// Named arguments: key -> first occurrence index in `all`
    named: std::collections::HashMap<String, usize>,
    /// Positional arguments in order (indices into `all`)
    positional: Vec<usize>,
    /// All arguments in order (for iteration)
    all: Vec<ParsedArg<'a>>,
}

#[allow(dead_code)]
impl<'a> FuncArgs<'a> {
    /// Create a new FuncArgs parser from a FuncCall node's children
    /// The children should be the children of the FuncCall node (first is function name)
    pub fn from_func_call(children: &'a [&'a SyntaxNode]) -> Self {
        children
            .iter()
            .skip(1)
            .find(|child| child.kind() == SyntaxKind::Args)
            .map_or_else(Self::empty, |args_node| Self::from_args_node(args_node))
    }

    /// Create a parser directly from an Args node.
    pub fn from_args_node(args_node: &'a SyntaxNode) -> Self {
        let mut named = std::collections::HashMap::new();
        let mut positional = Vec::new();
        let mut all = Vec::new();
        let children: Vec<_> = args_node.children().collect();
        let mut idx = 0;

        while idx < children.len() {
            let arg = children[idx];
            match arg.kind() {
                SyntaxKind::Named => {
                    if let Some(mut parsed) = Self::parse_named_arg(arg) {
                        if parsed.value.trim().is_empty() {
                            if let Some((value, value_node, value_nodes, cursor)) =
                                Self::recover_empty_named_value(&children, idx + 1)
                            {
                                parsed.value = value;
                                parsed.value_node = value_node;
                                parsed.value_nodes = value_nodes;
                                idx = cursor.saturating_sub(1);
                            }
                        }

                        let idx = all.len();
                        if let Some(ref name) = parsed.name {
                            named.entry(name.clone()).or_insert(idx);
                        }
                        all.push(parsed);
                    }
                }
                SyntaxKind::Comma
                | SyntaxKind::LeftParen
                | SyntaxKind::RightParen
                | SyntaxKind::Space => {}
                _ if is_content_node(arg) => {
                    let value = get_simple_text(arg);
                    if !value.is_empty() {
                        let idx = all.len();
                        positional.push(idx);
                        all.push(ParsedArg {
                            value,
                            name: None,
                            is_positional: true,
                            node: arg,
                            value_node: Some(arg),
                            value_nodes: vec![arg],
                        });
                    }
                }
                _ => {}
            }

            idx += 1;
        }

        Self {
            named,
            positional,
            all,
        }
    }

    fn empty() -> Self {
        Self {
            named: std::collections::HashMap::new(),
            positional: Vec::new(),
            all: Vec::new(),
        }
    }

    fn recover_empty_named_value(
        children: &[&'a SyntaxNode],
        mut cursor: usize,
    ) -> Option<(String, Option<&'a SyntaxNode>, Vec<&'a SyntaxNode>, usize)> {
        while cursor < children.len() && children[cursor].kind() == SyntaxKind::Space {
            cursor += 1;
        }

        if cursor >= children.len() {
            return None;
        }

        let first = children[cursor];
        let mut value_nodes = Vec::new();
        let mut value_node = Some(first);
        let mut value = String::new();

        let is_self_contained = matches!(
            first.kind(),
            SyntaxKind::Parenthesized
                | SyntaxKind::Array
                | SyntaxKind::Dict
                | SyntaxKind::ContentBlock
                | SyntaxKind::Markup
                | SyntaxKind::Str
                | SyntaxKind::Equation
                | SyntaxKind::FuncCall
        );

        if is_self_contained {
            value_nodes.push(first);
            value.push_str(first.text().as_ref());
            return Some((
                value.trim().to_string(),
                value_node,
                value_nodes,
                cursor + 1,
            ));
        }

        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;

        while cursor < children.len() {
            let node = children[cursor];
            match node.kind() {
                SyntaxKind::Comma | SyntaxKind::Semicolon
                    if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
                {
                    break;
                }
                SyntaxKind::RightParen
                    if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
                {
                    break;
                }
                SyntaxKind::Named if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                    break;
                }
                SyntaxKind::LeftParen => paren_depth += 1,
                SyntaxKind::RightParen if paren_depth > 0 => paren_depth -= 1,
                SyntaxKind::LeftBracket => bracket_depth += 1,
                SyntaxKind::RightBracket if bracket_depth > 0 => bracket_depth -= 1,
                SyntaxKind::LeftBrace => brace_depth += 1,
                SyntaxKind::RightBrace if brace_depth > 0 => brace_depth -= 1,
                _ => {}
            }

            if value_node.is_none() && node.kind() != SyntaxKind::Space {
                value_node = Some(node);
            }

            if node.kind() != SyntaxKind::Space {
                value_nodes.push(node);
            }
            value.push_str(node.text().as_ref());
            cursor += 1;
        }

        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some((trimmed, value_node, value_nodes, cursor))
        }
    }

    /// Parse a Named argument node to extract key and value while preserving AST nodes.
    fn parse_named_arg(node: &'a SyntaxNode) -> Option<ParsedArg<'a>> {
        let mut key: Option<String> = None;
        let mut value = String::new();
        let mut value_node: Option<&'a SyntaxNode> = None;
        let mut value_nodes: Vec<&'a SyntaxNode> = Vec::new();
        let mut seen_colon = false;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::Ident | SyntaxKind::MathIdent if key.is_none() => {
                    key = Some(child.text().to_string());
                }
                SyntaxKind::Colon if key.is_some() => {
                    seen_colon = true;
                }
                SyntaxKind::Space if !seen_colon => {}
                _ if key.is_some() && seen_colon => {
                    let text = get_simple_text(child);
                    if !text.trim().is_empty() {
                        if value_node.is_none() {
                            value_node = Some(child);
                        }
                        value_nodes.push(child);
                        value.push_str(&text);
                    }
                }
                _ => {}
            }
        }

        Some(ParsedArg {
            value: value.trim().to_string(),
            name: Some(key?),
            is_positional: false,
            node,
            value_node,
            value_nodes,
        })
    }

    /// Get a named argument descriptor by key.
    pub fn named_arg(&self, key: &str) -> Option<&ParsedArg<'a>> {
        self.named.get(key).and_then(|idx| self.all.get(*idx))
    }

    /// Get a named argument value by key
    pub fn named(&self, key: &str) -> Option<&str> {
        self.named_arg(key).map(|arg| arg.value.as_str())
    }

    /// Get a named argument value node by key.
    pub fn named_node(&self, key: &str) -> Option<&'a SyntaxNode> {
        self.named_arg(key).and_then(|arg| arg.value_node)
    }

    /// Get all named argument value nodes by key.
    pub fn named_nodes(&self, key: &str) -> Option<&[&'a SyntaxNode]> {
        self.named_arg(key).map(|arg| arg.value_nodes.as_slice())
    }

    /// Get a named argument as text.
    pub fn named_text(&self, key: &str) -> Option<&str> {
        self.named(key)
    }

    /// Parse a named argument as bool.
    pub fn named_bool(&self, key: &str) -> Option<bool> {
        let value = self.named(key)?.trim().trim_matches('"');
        match value {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Parse a named argument as usize.
    pub fn named_usize(&self, key: &str) -> Option<usize> {
        self.named(key)?.trim().parse::<usize>().ok()
    }

    /// Parse a named argument as LaTeX-compatible length.
    pub fn named_length(&self, key: &str) -> Option<String> {
        extract_length_value(self.named(key)?)
    }

    /// Parse a named argument as an angle in degrees.
    pub fn named_angle(&self, key: &str) -> Option<f64> {
        parse_angle_value(self.named(key)?)
    }

    /// Get a named color expression as text.
    pub fn named_color(&self, key: &str) -> Option<&str> {
        self.named(key)
    }

    /// Get a positional argument value by index
    pub fn positional(&self, index: usize) -> Option<&str> {
        self.positional_arg(index).map(|arg| arg.value.as_str())
    }

    /// Get a positional argument descriptor by index.
    pub fn positional_arg(&self, index: usize) -> Option<&ParsedArg<'a>> {
        self.positional
            .get(index)
            .and_then(|all_idx| self.all.get(*all_idx))
    }

    /// Get a positional argument value node by index.
    pub fn positional_node(&self, index: usize) -> Option<&'a SyntaxNode> {
        self.positional_arg(index).and_then(|arg| arg.value_node)
    }

    /// Get all positional argument values
    pub fn positional_values(&self) -> Vec<&str> {
        self.positional
            .iter()
            .filter_map(|idx| self.all.get(*idx).map(|arg| arg.value.as_str()))
            .collect()
    }

    /// Get all named argument keys
    pub fn named_keys(&self) -> Vec<&str> {
        self.all
            .iter()
            .filter_map(|arg| arg.name.as_deref())
            .collect()
    }

    /// Get unknown named argument keys given an allow-list.
    pub fn unknown_named_keys<'b>(&'b self, known: &[&str]) -> Vec<&'b str> {
        self.all
            .iter()
            .filter_map(|arg| arg.name.as_deref())
            .filter(|name| !known.contains(name))
            .collect()
    }

    /// Check if there are any unknown named arguments given an allow-list.
    pub fn has_unknown_named(&self, known: &[&str]) -> bool {
        self.all
            .iter()
            .filter_map(|arg| arg.name.as_deref())
            .any(|name| !known.contains(&name))
    }

    /// Get count of positional arguments
    pub fn positional_count(&self) -> usize {
        self.positional.len()
    }

    /// Get count of named arguments
    pub fn named_count(&self) -> usize {
        self.named.len()
    }

    /// Check if a named argument exists
    pub fn has_named(&self, key: &str) -> bool {
        self.named.contains_key(key)
    }

    /// Iterate over all arguments in order
    pub fn iter(&self) -> impl Iterator<Item = &ParsedArg<'a>> {
        self.all.iter()
    }

    /// Find the parsed argument descriptor for an original AST node.
    pub fn arg_for_node(&self, node: &SyntaxNode) -> Option<&ParsedArg<'a>> {
        self.all.iter().find(|arg| std::ptr::eq(arg.node, node))
    }

    /// Get the first positional argument (common case)
    pub fn first(&self) -> Option<&str> {
        self.positional_arg(0).map(|arg| arg.value.as_str())
    }

    /// Get the first positional argument node.
    pub fn first_node(&self) -> Option<&'a SyntaxNode> {
        self.positional_node(0)
    }

    /// Check if there are any arguments
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst_syntax::{parse_code, SyntaxKind, SyntaxNode};

    fn find_first_func_call(node: &SyntaxNode) -> Option<SyntaxNode> {
        if node.kind() == SyntaxKind::FuncCall {
            return Some(node.clone());
        }

        for child in node.children() {
            if let Some(found) = find_first_func_call(&child) {
                return Some(found);
            }
        }

        None
    }

    #[test]
    fn test_escape_latex() {
        assert_eq!(escape_latex_text("a & b"), "a \\& b");
        assert_eq!(escape_latex_text("50%"), "50\\%");
        assert_eq!(escape_latex_text("$100"), "\\$100");
        assert_eq!(escape_latex_text("a_b"), "a\\_b");
    }

    #[test]
    fn test_string_content() {
        // Mock test - in real use this would use actual SyntaxNode
        let text = "\"hello world\"";
        let result = text.trim_matches('"').to_string();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_extract_length() {
        assert_eq!(extract_length_value("1em"), Some("1em".to_string()));
        assert_eq!(extract_length_value("10pt"), Some("10pt".to_string()));
        assert_eq!(extract_length_value("2.5cm"), Some("2.5cm".to_string()));
        assert_eq!(
            extract_length_value("50%"),
            Some("0.50\\textwidth".to_string())
        );
        assert_eq!(extract_length_value("1fr"), None); // fr units handled separately
    }

    #[test]
    fn test_func_args_preserve_order_and_nodes() {
        let root = parse_code("demo(1, size: #200%, block: true)");
        let func = find_first_func_call(&root).expect("func call");
        let children: Vec<_> = func.children().collect();
        let args = FuncArgs::from_func_call(&children);
        assert_eq!(args.first(), Some("1"));
        assert_eq!(args.named("size"), Some("#200%"));
        assert_eq!(args.named_bool("block"), Some(true));
        assert_eq!(
            args.named_nodes("size").map(|nodes| {
                nodes
                    .iter()
                    .map(|node| node.text().to_string())
                    .collect::<String>()
            }),
            Some("#200%".to_string())
        );

        let ordered: Vec<_> = args
            .iter()
            .map(|arg| (arg.name.clone(), arg.value.clone(), arg.is_positional))
            .collect();
        assert_eq!(
            ordered,
            vec![
                (None, "1".to_string(), true),
                (Some("size".to_string()), "#200%".to_string(), false),
                (Some("block".to_string()), "true".to_string(), false),
            ]
        );
    }

    #[test]
    fn test_func_args_unknown_keys_and_typed_helpers() {
        let root = parse_code("demo(width: 10pt, angle: 1.57079632679rad, mode: fancy)");
        let func = find_first_func_call(&root).expect("func call");
        let children: Vec<_> = func.children().collect();
        let args = FuncArgs::from_func_call(&children);

        assert_eq!(args.named_length("width"), Some("10pt".to_string()));
        assert!(args.named_angle("angle").unwrap() > 89.9);
        assert_eq!(args.unknown_named_keys(&["width", "angle"]), vec!["mode"]);
        assert!(args.has_unknown_named(&["width", "angle"]));
    }

    #[test]
    fn test_func_args_content_block_node() {
        let root = parse_code("demo(width: 10pt, [hello])");
        let func = find_first_func_call(&root).expect("func call");
        let children: Vec<_> = func.children().collect();
        let args = FuncArgs::from_func_call(&children);

        assert_eq!(args.positional_count(), 1);
        assert_eq!(
            args.positional_node(0).map(|node| node.kind()),
            Some(SyntaxKind::ContentBlock)
        );
    }

    #[test]
    fn test_func_args_angle_with_content_block() {
        let root = parse_code("rotate(angle: 90deg)[Hi]");
        let func = find_first_func_call(&root).expect("func call");
        let children: Vec<_> = func.children().collect();
        let args = FuncArgs::from_func_call(&children);

        assert_eq!(args.named("angle"), Some("90deg"));
        assert_eq!(args.named_angle("angle"), Some(90.0));
        assert_eq!(
            args.positional_node(0).map(|node| node.kind()),
            Some(SyntaxKind::ContentBlock)
        );
    }

    #[test]
    fn test_func_args_tuple_named_value_preserved() {
        let root = parse_code("grid(columns: (auto, auto, auto))[A]");
        let func = find_first_func_call(&root).expect("func call");
        let children: Vec<_> = func.children().collect();
        let args = FuncArgs::from_func_call(&children);
        assert_eq!(args.named("columns"), Some("(auto, auto, auto)"));
    }

    #[test]
    fn test_func_args_rgb_named_value_preserved() {
        let root = parse_code("text(fill: rgb(255, 0, 0))[Hello]");
        let func = find_first_func_call(&root).expect("func call");
        let children: Vec<_> = func.children().collect();
        let args = FuncArgs::from_func_call(&children);
        assert_eq!(args.named("fill"), Some("rgb(255, 0, 0)"));
    }

    #[test]
    fn test_typst_color_to_latex_spec_rgb_models() {
        assert_eq!(
            typst_color_to_latex_spec("rgb(255, 0, 0)"),
            LatexColorSpec::new(Some("RGB"), "255,0,0")
        );
        assert_eq!(
            typst_color_to_latex_spec("rgb(1, 0, 0)"),
            LatexColorSpec::new(Some("rgb"), "1,0,0")
        );
        assert_eq!(
            typst_color_to_latex_spec("rgb(\"#ff0000\")"),
            LatexColorSpec::new(Some("HTML"), "FF0000")
        );
    }

    #[test]
    fn test_typst_color_to_latex_spec_cmyk_and_luma() {
        assert_eq!(
            typst_color_to_latex_spec("cmyk(0, 1, 1, 0)"),
            LatexColorSpec::new(Some("cmyk"), "0,1,1,0")
        );
        assert_eq!(
            typst_color_to_latex_spec("luma(0.5)"),
            LatexColorSpec::new(Some("gray"), "0.5")
        );
    }

    #[test]
    fn test_normalize_typst_color_expr() {
        assert_eq!(normalize_typst_color_expr("red"), Some("red".to_string()));
        assert_eq!(
            normalize_typst_color_expr("blue.lighten(80%)"),
            Some("blue.lighten(80%)".to_string())
        );
        assert_eq!(
            normalize_typst_color_expr("rgb(255, 0, 0)"),
            Some("rgb(255, 0, 0)".to_string())
        );
        assert_eq!(normalize_typst_color_expr("not-a-color"), None);
    }
}
