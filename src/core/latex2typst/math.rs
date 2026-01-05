//! Math formula handling for LaTeX to Typst conversion
//!
//! This module handles math formulas, delimiters, and math-specific constructs.

use mitex_parser::syntax::{FormulaItem, SyntaxElement, SyntaxKind};
use rowan::ast::AstNode;
use std::fmt::Write;

use crate::data::extended_symbols::EXTENDED_SYMBOLS;
use crate::data::maps::TEX_COMMAND_SPEC;
use crate::data::symbols::GREEK_LETTERS;
use mitex_spec::CommandSpecItem;

use super::context::{ConversionMode, EnvironmentContext, LatexConverter};

/// Convert a math formula ($..$ or $$..$$)
pub fn convert_formula(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    if let SyntaxElement::Node(n) = elem {
        if let Some(formula) = FormulaItem::cast(n.clone()) {
            let is_inline = formula.is_inline();
            let prev_mode = conv.state.mode;
            conv.state.mode = ConversionMode::Math;

            // Collect math content into a buffer for post-processing
            let mut math_content = String::new();
            conv.visit_node(&n, &mut math_content);

            // Apply math cleanup
            let cleaned = conv.cleanup_math_spacing(&math_content);

            if is_inline {
                output.push('$');
                output.push_str(&cleaned);
                output.push('$');
            } else {
                output.push_str("$ ");
                output.push_str(&cleaned);
                output.push_str(" $");
            }

            conv.state.mode = prev_mode;
        }
    }
}

/// Convert a curly group in math mode
pub fn convert_curly(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    if conv.state.in_preamble {
        return;
    }

    let node = match elem {
        SyntaxElement::Node(n) => n,
        _ => return,
    };

    // Check if this is an argument for a pending operator (operatorname*)
    if let Some(op) = conv.state.pending_op.take() {
        // This group is the argument for a pending operator
        let mut content = String::new();
        // Extract content without braces
        for child in node.children_with_tokens() {
            match child.kind() {
                SyntaxKind::TokenWhiteSpace
                | SyntaxKind::TokenLineBreak
                | SyntaxKind::TokenLBrace
                | SyntaxKind::TokenRBrace => {}
                _ => conv.visit_element(child, &mut content),
            }
        }
        let text = content.trim();

        // Handle common operator patterns that might include spacing commands
        // e.g. "arg thin min" -> "argmin"
        let normalized = text.replace("thin", "").replace(" ", "");
        let final_text = if normalized == "argmin" {
            "argmin"
        } else if normalized == "argmax" {
            "argmax"
        } else {
            text
        };

        // Try to keep it as simple text if possible for cleaner output
        let op_content = if final_text
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace())
        {
            format!("\"{}\"", final_text)
        } else {
            // Wrap in content block if complex
            format!("[{}]", final_text)
        };

        if op.is_limits {
            let _ = write!(output, "limits(op({}))", op_content);
        } else {
            let _ = write!(output, "op({})", op_content);
        }
        return;
    }

    // Check if it's empty
    let mut has_content = false;
    for child in node.children_with_tokens() {
        match child.kind() {
            SyntaxKind::TokenWhiteSpace
            | SyntaxKind::TokenLineBreak
            | SyntaxKind::TokenLBrace
            | SyntaxKind::TokenRBrace => {}
            _ => has_content = true,
        }
        conv.visit_element(child, output);
    }
    // Add zero-width space for empty groups in math mode
    if !has_content && matches!(conv.state.mode, ConversionMode::Math) {
        output.push_str("zws ");
    }
}

/// Convert \left...\right with enhanced delimiter handling
/// Based on tex2typst's comprehensive approach
pub fn convert_lr(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    let node = match elem {
        SyntaxElement::Node(n) => n,
        _ => return,
    };

    let children: Vec<_> = node.children_with_tokens().collect();

    // Extract left and right delimiters
    let mut left_delim: Option<String> = None;
    let mut right_delim: Option<String> = None;
    let mut body_start = 0;
    let mut body_end = children.len();

    // Parse the \left delimiter - it can be a ClauseLR node or a Token
    // First pass: find left delimiter
    for (i, child) in children.iter().enumerate() {
        match child {
            // ClauseLR node contains the delimiter
            SyntaxElement::Node(cn) if cn.kind() == SyntaxKind::ClauseLR => {
                let text = cn.text().to_string();
                if text.starts_with("\\left") && left_delim.is_none() {
                    // Extract delimiter from inside the ClauseLR
                    for sub in cn.children_with_tokens() {
                        if let SyntaxElement::Token(t) = sub {
                            let tok_text = t.text();
                            // Skip the command name, get the delimiter token
                            if t.kind() != SyntaxKind::ClauseCommandName {
                                left_delim = Some(convert_delimiter(tok_text));
                                break;
                            }
                        }
                    }
                    body_start = i + 1;
                }
            }
            // Legacy: Token-based parsing
            SyntaxElement::Token(t) => {
                let name = t.text();
                if let Some(stripped) = name.strip_prefix("\\left") {
                    left_delim = Some(convert_delimiter(stripped));
                    body_start = i + 1;
                }
            }
            _ => {}
        }
    }

    // Second pass: find right delimiter (from the end)
    for (i, child) in children.iter().enumerate().rev() {
        match child {
            SyntaxElement::Node(cn) if cn.kind() == SyntaxKind::ClauseLR => {
                let text = cn.text().to_string();
                if text.starts_with("\\right") && right_delim.is_none() {
                    // Extract delimiter from inside the ClauseLR
                    for sub in cn.children_with_tokens() {
                        if let SyntaxElement::Token(t) = sub {
                            // Skip the command name, get the delimiter token
                            if t.kind() != SyntaxKind::ClauseCommandName {
                                right_delim = Some(convert_delimiter(t.text()));
                                break;
                            }
                        }
                    }
                    body_end = i;
                    break;
                }
            }
            SyntaxElement::Token(t) => {
                let name = t.text();
                if let Some(stripped) = name.strip_prefix("\\right") {
                    right_delim = Some(convert_delimiter(stripped));
                    body_end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    // Check for common optimizations (matching pairs that don't need lr())
    // Also handle mismatched or missing delimiters gracefully
    let (use_lr, is_valid_pair) = match (left_delim.as_deref(), right_delim.as_deref()) {
        // These pairs work naturally in Typst without lr()
        (Some("("), Some(")")) | (Some("["), Some("]")) | (Some("{"), Some("}")) => (false, true),
        // Matching pairs that need lr()
        (Some(l), Some(r)) if l == r => (true, true),
        // Valid mixed pairs that lr() can handle
        (Some("("), Some("]"))
        | (Some("["), Some(")"))
        | (Some("chevron.l"), Some("chevron.r"))
        | (Some("floor.l"), Some("floor.r"))
        | (Some("ceil.l"), Some("ceil.r")) => (true, true),
        // Empty delimiter on one side - valid for lr()
        (Some("."), Some(_)) | (Some(_), Some(".")) => (true, true),
        // Missing delimiter - don't use lr(), just output content
        (None, _) | (_, None) => (false, false),
        // Other cases - try lr() but mark as potentially invalid
        _ => (true, true),
    };

    // Check for norm: \left\| ... \right\| -> norm(...)
    if left_delim.as_deref() == Some("bar.v.double")
        && right_delim.as_deref() == Some("bar.v.double")
    {
        output.push_str("norm(");
        for child in children.iter().take(body_end).skip(body_start) {
            match child {
                SyntaxElement::Token(t) if t.text() == "." => {}
                SyntaxElement::Token(t) if t.text().starts_with("\\right") => {}
                _ => conv.visit_element(child.clone(), output),
            }
        }
        output.push_str(") ");
        return;
    }

    // Check for abs: \left| ... \right| -> abs(...)
    if left_delim.as_deref() == Some("bar.v") && right_delim.as_deref() == Some("bar.v") {
        output.push_str("abs(");
        for child in children.iter().take(body_end).skip(body_start) {
            match child {
                SyntaxElement::Token(t) if t.text() == "." => {}
                SyntaxElement::Token(t) if t.text().starts_with("\\right") => {}
                _ => conv.visit_element(child.clone(), output),
            }
        }
        output.push_str(") ");
        return;
    }

    // For invalid pairs (missing delimiters), just output the content without lr()
    if !is_valid_pair {
        // Output left delimiter if present
        if let Some(ref delim) = left_delim {
            if delim != "." && !delim.is_empty() {
                output.push_str(delim);
                output.push(' ');
            }
        }

        // Output body content
        for child in children.iter().take(body_end).skip(body_start) {
            match child {
                SyntaxElement::Token(t) if t.text() == "." => {}
                SyntaxElement::Token(t) if t.text().starts_with("\\right") => {}
                _ => conv.visit_element(child.clone(), output),
            }
        }

        // Output right delimiter if present
        if let Some(ref delim) = right_delim {
            if delim != "." && !delim.is_empty() {
                output.push_str(delim);
                output.push(' ');
            }
        }
        return;
    }

    // Output with or without lr()
    if use_lr {
        output.push_str("lr(");
    }

    // Output left delimiter
    if let Some(ref delim) = left_delim {
        if delim != "." && !delim.is_empty() {
            output.push_str(delim);
            output.push(' ');
        }
    }

    // Output body content
    for child in children.iter().take(body_end).skip(body_start) {
        match child {
            SyntaxElement::Token(t) if t.text() == "." => {}
            SyntaxElement::Token(t) if t.text().starts_with("\\right") => {}
            _ => conv.visit_element(child.clone(), output),
        }
    }

    // Output right delimiter with space before for clarity
    if let Some(ref delim) = right_delim {
        if delim != "." && !delim.is_empty() {
            output.push(' ');
            output.push_str(delim);
        }
    }

    if use_lr {
        output.push_str(") ");
    } else {
        output.push(' ');
    }
}

/// Convert subscript/superscript attachment
pub fn convert_attachment(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    let node = match elem {
        SyntaxElement::Node(n) => n,
        _ => return,
    };

    let mut is_script = false;

    for child in node.children_with_tokens() {
        let kind = child.kind();

        if kind == SyntaxKind::TokenUnderscore {
            output.push('_');
            is_script = true;
            continue;
        }

        if kind == SyntaxKind::TokenCaret {
            output.push('^');
            is_script = true;
            continue;
        }

        // Skip whitespace
        if kind == SyntaxKind::TokenWhiteSpace || kind == SyntaxKind::TokenLineBreak {
            // Check if previous char is _ or ^, if so, don't output space yet
            // Wait until after the script content
            continue;
        }

        if is_script {
            // Always wrap attachment content in parentheses to ensure correct binding
            // e.g. sum_i=1 -> sum_(i=1) instead of sum_i = 1
            output.push('(');
            conv.visit_element(child, output);
            output.push(')');
            // No space after script to ensure tight binding of multiple scripts
            is_script = false;
        } else {
            // This handles the base or other parts if any (though usually base is previous sibling)
            conv.visit_element(child, output);
        }
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Convert a LaTeX delimiter to Typst equivalent
fn convert_delimiter(delim: &str) -> String {
    match delim.trim() {
        "." => ".".to_string(), // Empty delimiter
        "(" => "(".to_string(),
        ")" => ")".to_string(),
        "[" => "[".to_string(),
        "]" => "]".to_string(),
        "\\{" | "\\lbrace" => "{".to_string(),
        "\\}" | "\\rbrace" => "}".to_string(),
        "|" | "\\vert" => "bar.v".to_string(),
        "\\|" | "\\Vert" => "bar.v.double".to_string(),
        "\\langle" => "chevron.l".to_string(),
        "\\rangle" => "chevron.r".to_string(),
        "\\lfloor" => "floor.l".to_string(),
        "\\rfloor" => "floor.r".to_string(),
        "\\lceil" => "ceil.l".to_string(),
        "\\rceil" => "ceil.r".to_string(),
        "\\lgroup" => "paren.l.flat".to_string(),
        "\\rgroup" => "paren.r.flat".to_string(),
        other => other.to_string(),
    }
}

/// Convert a math symbol command
pub fn convert_math_symbol(name: &str) -> Option<&'static str> {
    // First check TEX_COMMAND_SPEC for aliases - these give proper Typst symbol names
    if let Some(CommandSpecItem::Cmd(shape)) = TEX_COMMAND_SPEC.get(name) {
        if let Some(ref alias) = shape.alias {
            return Some(Box::leak(alias.clone().into_boxed_str()));
        }
    }

    // Check extended symbols
    if let Some(typst) = EXTENDED_SYMBOLS.get(name) {
        return Some(*typst);
    }

    let key = format!("\\{}", name);

    // Check Greek letters (fallback - returns Unicode)
    if let Some(typst) = GREEK_LETTERS.get(key.as_str()) {
        return Some(*typst);
    }

    None
}

/// Check if we're in a matrix-like environment
pub fn is_matrix_env(env: &EnvironmentContext) -> bool {
    matches!(env, EnvironmentContext::Matrix | EnvironmentContext::Cases)
}
