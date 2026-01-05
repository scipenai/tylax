//! Utility functions for Typst to LaTeX conversion
//!
//! Helper functions for text escaping, content extraction, etc.

use crate::data::colors::TYPST_TO_LATEX_COLORS;
use lazy_static::lazy_static;
use std::collections::HashMap;
use typst_syntax::{SyntaxKind, SyntaxNode};

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

/// Convert Unicode math characters to LaTeX commands
pub fn unicode_to_latex(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);

    for ch in text.chars() {
        if let Some(latex) = UNICODE_TO_LATEX.get(&ch) {
            result.push_str(latex);
            // Add space after command if next char is a letter
            result.push(' ');
        } else {
            result.push(ch);
        }
    }

    // Clean up extra spaces
    result.replace("  ", " ").trim_end().to_string()
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

/// Convert Typst color to LaTeX color name (returns owned String for complex expressions)
pub fn typst_color_to_latex(color: &str) -> String {
    let color = color.trim();

    // Handle color expressions like "purple.lighten(80%)" or "red.darken(20%)"
    if color.contains('.') {
        let parts: Vec<&str> = color.split('.').collect();
        if let Some(base_color) = parts.first() {
            let base = simple_color_to_latex(base_color.trim());

            // Check for lighten/darken
            if parts.len() > 1 {
                let method = parts[1];
                if method.starts_with("lighten(") {
                    // Extract percentage: lighten(80%) -> 80
                    if let Some(pct) = extract_percentage(method) {
                        // LaTeX xcolor: color!percentage!white for lighten
                        let remaining = 100 - pct;
                        return format!("{}!{}!white", base, remaining);
                    }
                } else if method.starts_with("darken(") {
                    // Extract percentage: darken(20%) -> 20
                    if let Some(pct) = extract_percentage(method) {
                        // LaTeX xcolor: color!percentage!black for darken
                        let remaining = 100 - pct;
                        return format!("{}!{}!black", base, remaining);
                    }
                } else if method.starts_with("transparentize(") || method.starts_with("opacify(") {
                    // Transparency - just return base color (LaTeX doesn't handle this well in tables)
                    return base.to_string();
                }
            }
            return base.to_string();
        }
    }

    simple_color_to_latex(color).to_string()
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

/// Get raw text content from a raw node (strips backticks and language tag)
pub fn get_raw_text(node: &SyntaxNode) -> String {
    let (content, _lang) = get_raw_text_with_lang(node);
    content
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

/// Wrap LaTeX content in a complete document structure
pub fn wrap_in_document(content: &str, options: &super::context::T2LOptions) -> String {
    let mut doc = String::new();

    doc.push_str(&format!("\\documentclass{{{}}}\n", options.document_class));
    doc.push_str("\\usepackage[utf8]{inputenc}\n");
    doc.push_str("\\usepackage{amsmath}\n");
    doc.push_str("\\usepackage{amssymb}\n");
    doc.push_str("\\usepackage{graphicx}\n");
    doc.push_str("\\usepackage{hyperref}\n");
    doc.push_str("\\usepackage{listings}\n");
    doc.push_str("\\usepackage[table]{xcolor}\n");
    doc.push_str("\\usepackage{multirow}\n");
    doc.push_str("\\usepackage{booktabs}\n");
    doc.push_str("\\usepackage{ctex}\n");
    doc.push('\n');

    if let Some(ref title) = options.title {
        doc.push_str(&format!("\\title{{{}}}\n", escape_latex_text(title)));
    }
    if let Some(ref author) = options.author {
        doc.push_str(&format!("\\author{{{}}}\n", escape_latex_text(author)));
    }

    doc.push_str("\n\\begin{document}\n");

    if options.title.is_some() {
        doc.push_str("\\maketitle\n");
    }

    doc.push('\n');
    doc.push_str(content);
    doc.push_str("\n\\end{document}\n");

    doc
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

// ============================================================================
// Generic Function Arguments Parser
// ============================================================================

/// Parsed function argument with extracted values
#[derive(Debug, Clone)]
pub struct ParsedArg {
    /// The argument value as text
    pub value: String,
    /// The argument name (for named arguments)
    pub name: Option<String>,
    /// Whether this is a positional argument
    pub is_positional: bool,
}

/// Generic function arguments parser for Typst AST
/// Provides a unified way to extract named and positional arguments from FuncCall nodes
/// This version stores extracted values rather than node references for simplicity
pub struct FuncArgs {
    /// Named arguments: key -> value text
    named: std::collections::HashMap<String, String>,
    /// Positional arguments in order (as text)
    positional: Vec<String>,
    /// All arguments in order (for iteration)
    all: Vec<ParsedArg>,
}

impl FuncArgs {
    /// Create a new FuncArgs parser from a FuncCall node's children
    /// The children should be the children of the FuncCall node (first is function name)
    pub fn from_func_call(children: &[&SyntaxNode]) -> Self {
        let mut named = std::collections::HashMap::new();
        let mut positional = Vec::new();
        let mut all = Vec::new();

        // Find the Args node (usually the second child)
        for child in children.iter().skip(1) {
            if child.kind() == SyntaxKind::Args {
                // Parse arguments from Args node
                for arg in child.children() {
                    match arg.kind() {
                        SyntaxKind::Named => {
                            // Named argument: key: value
                            let (key, value) = Self::parse_named_arg(arg);
                            if let (Some(k), Some(v)) = (key, value) {
                                named.insert(k.clone(), v.clone());
                                all.push(ParsedArg {
                                    value: v,
                                    name: Some(k),
                                    is_positional: false,
                                });
                            }
                        }
                        // Skip punctuation and whitespace
                        SyntaxKind::Comma
                        | SyntaxKind::LeftParen
                        | SyntaxKind::RightParen
                        | SyntaxKind::Space => {
                            continue;
                        }
                        _ => {
                            // Positional argument
                            if is_content_node(arg) {
                                let value = get_simple_text(arg);
                                if !value.is_empty() {
                                    positional.push(value.clone());
                                    all.push(ParsedArg {
                                        value,
                                        name: None,
                                        is_positional: true,
                                    });
                                }
                            }
                        }
                    }
                }
                break;
            }
        }

        Self {
            named,
            positional,
            all,
        }
    }

    /// Parse a Named argument node to extract key and value
    fn parse_named_arg(node: &SyntaxNode) -> (Option<String>, Option<String>) {
        let mut key: Option<String> = None;
        let mut value: Option<String> = None;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::Ident => {
                    if key.is_none() {
                        key = Some(child.text().to_string());
                    }
                }
                // Skip punctuation
                SyntaxKind::Colon | SyntaxKind::Space => {
                    continue;
                }
                _ => {
                    // The value node
                    if key.is_some() && value.is_none() {
                        value = Some(get_simple_text(child));
                    }
                }
            }
        }

        (key, value)
    }

    /// Get a named argument value by key
    pub fn named(&self, key: &str) -> Option<&str> {
        self.named.get(key).map(|s| s.as_str())
    }

    /// Get a positional argument value by index
    pub fn positional(&self, index: usize) -> Option<&str> {
        self.positional.get(index).map(|s| s.as_str())
    }

    /// Get all positional argument values
    pub fn positional_values(&self) -> &[String] {
        &self.positional
    }

    /// Get all named argument keys
    pub fn named_keys(&self) -> Vec<&str> {
        self.named.keys().map(|s| s.as_str()).collect()
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
    pub fn iter(&self) -> impl Iterator<Item = &ParsedArg> {
        self.all.iter()
    }

    /// Get the first positional argument (common case)
    pub fn first(&self) -> Option<&str> {
        self.positional.first().map(|s| s.as_str())
    }

    /// Check if there are any arguments
    pub fn is_empty(&self) -> bool {
        self.positional.is_empty() && self.named.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
