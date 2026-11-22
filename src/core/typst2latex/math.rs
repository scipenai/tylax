//! Math mode conversion for Typst to LaTeX
//!
//! Handles mathematical expressions, formulas, and math-specific constructs.

use super::context::{ConvertContext, T2LOptions};
use super::math_emit::emit_math_ir;
use super::math_ir::{
    build_math_ir_with_env, collect_math_warnings, current_math_build_env, normalize_math_ir,
    MathBuildEnv, MathIr,
};
use super::utils::{is_content_node, FuncArgs};
use crate::data::maps::DELIMITER_MAP;
use typst_syntax::{SyntaxKind, SyntaxNode};

/// Convert a math node to LaTeX.
pub fn convert_math_node(node: &SyntaxNode, ctx: &mut ConvertContext) {
    let env = current_math_build_env();
    let ir = normalize_math_ir(build_math_ir_with_env(node, &ctx.options, &env));
    collect_math_warnings(&ir, &mut ctx.warnings);
    emit_math_ir(&ir, ctx);
}

/// Convert a math-mode Typst function call to LaTeX.
pub fn convert_func_call(node: &SyntaxNode, ctx: &mut ConvertContext) {
    convert_math_node(node, ctx);
}

// =============================================================================
// Delimiter Helper Functions (using DELIMITER_MAP as single source of truth)
// =============================================================================

/// Check if a text string represents a recognized delimiter for lr()
fn is_delimiter(text: &str) -> bool {
    text == "." || DELIMITER_MAP.contains_key(text)
}

/// Get LaTeX delimiter string from text, with a default delimiter fallback
fn get_latex_delimiter(text: &str, is_left: bool) -> &'static str {
    if text == "." {
        return ".";
    }

    DELIMITER_MAP
        .get(text)
        .copied()
        .unwrap_or(if is_left { "(" } else { ")" })
}

// =============================================================================
// lr() Conversion Functions
// =============================================================================

/// Keep lr contents in the IR so nested diagnostics survive.
pub(crate) fn build_lr_ir(
    args_node: &SyntaxNode,
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> MathIr {
    let raw_args: Vec<&SyntaxNode> = args_node.children().collect();
    let args = FuncArgs::from_args_node(args_node);
    let parsed = parse_lr_args(&raw_args, &args);

    if parsed.has_unknown_named_args {
        return build_lr_content(&parsed.alternate_content_nodes, None, options, env);
    }

    build_lr_content(&parsed.content_nodes, parsed.size, options, env)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LrDelimiterSize {
    Plain,
    Big,
    BigLarge,
    BigGl,
    BigGLarge,
}

struct ParsedLrArgs<'a> {
    content_nodes: Vec<&'a SyntaxNode>,
    alternate_content_nodes: Vec<&'a SyntaxNode>,
    size: Option<LrDelimiterSize>,
    has_unknown_named_args: bool,
}

fn parse_lr_args<'a>(raw_args: &'a [&'a SyntaxNode], args: &FuncArgs<'a>) -> ParsedLrArgs<'a> {
    let mut content_nodes = Vec::new();
    let mut alternate_content_nodes = Vec::new();
    let size = args.named("size").and_then(parse_lr_size_percent);
    let has_unknown_named_args = args.has_unknown_named(&["size"]);

    for (idx, child) in raw_args.iter().enumerate() {
        match child.kind() {
            SyntaxKind::LeftParen | SyntaxKind::RightParen | SyntaxKind::Space => {}
            SyntaxKind::Named => {
                if !is_lr_size_named(child, args) {
                    alternate_content_nodes.push(*child);
                }
            }
            SyntaxKind::Comma | SyntaxKind::Semicolon => {
                if lr_separator_is_adjacent_to_named(raw_args, idx) {
                } else {
                    content_nodes.push(*child);
                }

                if lr_separator_is_adjacent_to_size_named(raw_args, idx, args) {
                    continue;
                }

                alternate_content_nodes.push(*child);
            }
            SyntaxKind::Math | SyntaxKind::MathDelimited => {
                push_lr_inner_nodes(child, &mut content_nodes);
                push_lr_inner_nodes(child, &mut alternate_content_nodes);
            }
            _ => {
                if is_content_node(child) {
                    content_nodes.push(*child);
                    alternate_content_nodes.push(*child);
                }
            }
        }
    }

    ParsedLrArgs {
        content_nodes,
        alternate_content_nodes,
        size,
        has_unknown_named_args,
    }
}

fn lr_separator_is_adjacent_to_named(args: &[&SyntaxNode], idx: usize) -> bool {
    lr_prev_significant_arg(args, idx)
        .map(|node| node.kind() == SyntaxKind::Named)
        .unwrap_or(false)
        || lr_next_significant_arg(args, idx)
            .map(|node| node.kind() == SyntaxKind::Named)
            .unwrap_or(false)
}

fn lr_separator_is_adjacent_to_size_named(
    args: &[&SyntaxNode],
    idx: usize,
    parsed_args: &FuncArgs<'_>,
) -> bool {
    lr_prev_significant_arg(args, idx)
        .map(|node| is_lr_size_named(node, parsed_args))
        .unwrap_or(false)
        || lr_next_significant_arg(args, idx)
            .map(|node| is_lr_size_named(node, parsed_args))
            .unwrap_or(false)
}

fn is_lr_size_named(node: &SyntaxNode, parsed_args: &FuncArgs<'_>) -> bool {
    parsed_args
        .arg_for_node(node)
        .and_then(|arg| arg.name.as_deref())
        == Some("size")
}

fn lr_prev_significant_arg<'a>(args: &'a [&'a SyntaxNode], idx: usize) -> Option<&'a SyntaxNode> {
    args[..idx].iter().rev().find_map(|node| match node.kind() {
        SyntaxKind::LeftParen | SyntaxKind::RightParen | SyntaxKind::Space => None,
        _ => Some(*node),
    })
}

fn lr_next_significant_arg<'a>(args: &'a [&'a SyntaxNode], idx: usize) -> Option<&'a SyntaxNode> {
    args[idx + 1..].iter().find_map(|node| match node.kind() {
        SyntaxKind::LeftParen | SyntaxKind::RightParen | SyntaxKind::Space => None,
        _ => Some(*node),
    })
}

fn parse_lr_size_percent(text: &str) -> Option<LrDelimiterSize> {
    let normalized = text
        .trim()
        .trim_start_matches('#')
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    let percent = normalized.strip_suffix('%')?.trim().parse::<f64>().ok()?;

    Some(if percent <= 100.0 {
        LrDelimiterSize::Plain
    } else if percent < 140.0 {
        LrDelimiterSize::Big
    } else if percent < 180.0 {
        LrDelimiterSize::BigLarge
    } else if percent < 240.0 {
        LrDelimiterSize::BigGl
    } else {
        LrDelimiterSize::BigGLarge
    })
}

/// Push the children of an `lr` argument, opening a delimited group at either
/// edge so its brackets can be the `\left` and `\right` delimiters. A group
/// inside stays whole: split up, its `{` and `}` became a LaTeX group.
fn push_lr_inner_nodes<'a>(node: &'a SyntaxNode, content: &mut Vec<&'a SyntaxNode>) {
    let inner: Vec<_> = node
        .children()
        .filter(|n| n.kind() != SyntaxKind::Space)
        .collect();
    let last = inner.len().saturating_sub(1);
    for (idx, inner) in inner.into_iter().enumerate() {
        if inner.kind() == SyntaxKind::MathDelimited && (idx == 0 || idx == last) {
            for nested in inner.children().filter(|n| n.kind() != SyntaxKind::Space) {
                content.push(nested);
            }
        } else {
            content.push(inner);
        }
    }
}

/// Convert lr() content nodes with delimiter detection, as `\left`…`\right`
/// or, given a size, as fixed-size delimiters.
/// Only looks at edge nodes for delimiters - does NOT scan interior to avoid
/// mistakenly treating inner symbols as delimiters and losing content.
fn build_lr_content(
    content: &[&SyntaxNode],
    size: Option<LrDelimiterSize>,
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> MathIr {
    let left_delim = find_lr_edge_delimiter(content, true);
    let right_delim = find_lr_edge_delimiter(content, false);

    // A missing edge delimiter is `.`; Typst draws none (`lr(a)` is `a`).
    let default_left = if content.is_empty() { "(" } else { "." };
    let default_right = if content.is_empty() { ")" } else { "." };
    let left_text = left_delim
        .as_ref()
        .map_or(default_left, |delim| delim.text.as_str());
    let right_text = right_delim
        .as_ref()
        .map_or(default_right, |delim| delim.text.as_str());
    MathIr::Delimited {
        open: lr_delimiter(left_text, true, size),
        content: Box::new(MathIr::Seq(
            content
                .iter()
                .enumerate()
                .filter(|(idx, _)| {
                    !should_skip_lr_index(*idx, left_delim.as_ref(), right_delim.as_ref())
                })
                .map(|(_, child)| build_math_ir_with_env(child, options, env))
                .collect(),
        )),
        close: lr_delimiter(right_text, false, size),
    }
}

fn lr_delimiter(delim_text: &str, is_left: bool, size: Option<LrDelimiterSize>) -> String {
    let latex = get_latex_delimiter(delim_text, is_left);

    let prefix = match size {
        None if is_left => "\\left",
        None => "\\right",
        Some(LrDelimiterSize::Plain) if latex == "." => return String::new(),
        Some(LrDelimiterSize::Plain) => "",
        Some(LrDelimiterSize::Big) => "\\big",
        Some(LrDelimiterSize::BigLarge) => "\\Big",
        Some(LrDelimiterSize::BigGl) => "\\bigg",
        Some(LrDelimiterSize::BigGLarge) => "\\Bigg",
    };
    format!("{prefix}{latex}")
}

/// Represents a detected delimiter in lr() content.
/// Used to track which nodes should be skipped during content output.
struct LrDelimiter {
    /// The delimiter text (e.g., "angle.l", "(", "||")
    text: String,
    /// Start index in content array (inclusive)
    start: usize,
    /// End index in content array (inclusive)
    /// For single-token delimiters: start == end
    /// For multi-token delimiters like "angle.l": start < end
    end: usize,
}

/// Scan direction for delimiter collection
#[derive(Clone, Copy, PartialEq)]
enum ScanDirection {
    Forward,
    Backward,
}

/// Find delimiter at edge of lr() content (first non-separator from start or end).
/// Only inspects the edge - does NOT scan interior to avoid data loss.
fn find_lr_edge_delimiter(content: &[&SyntaxNode], from_start: bool) -> Option<LrDelimiter> {
    if content.is_empty() {
        return None;
    }

    let direction = if from_start {
        ScanDirection::Forward
    } else {
        ScanDirection::Backward
    };

    // Find first non-separator index from the appropriate edge
    let edge_idx = find_first_content_index(content, direction)?;

    // Try to collect delimiter tokens starting from that index
    let (text, other_idx) = try_collect_delimiter(content, edge_idx, direction)?;

    if is_delimiter(&text) {
        let (start, end) = if direction == ScanDirection::Forward {
            (edge_idx, other_idx)
        } else {
            (other_idx, edge_idx)
        };
        Some(LrDelimiter { text, start, end })
    } else {
        None
    }
}

/// Find the first content index (skipping separators) from the given direction.
fn find_first_content_index(content: &[&SyntaxNode], direction: ScanDirection) -> Option<usize> {
    let is_separator = |node: &SyntaxNode| {
        matches!(
            node.kind(),
            SyntaxKind::Comma | SyntaxKind::Semicolon | SyntaxKind::Space
        )
    };

    match direction {
        ScanDirection::Forward => {
            for (idx, node) in content.iter().enumerate() {
                if !is_separator(node) {
                    return Some(idx);
                }
            }
        }
        ScanDirection::Backward => {
            for (idx, node) in content.iter().enumerate().rev() {
                if !is_separator(node) {
                    return Some(idx);
                }
            }
        }
    }
    None
}

/// Try to collect a delimiter starting from `idx` in the given direction.
/// Returns (delimiter_text, other_boundary_index) where:
/// - Forward: other_boundary_index is the end index
/// - Backward: other_boundary_index is the start index
fn try_collect_delimiter(
    content: &[&SyntaxNode],
    idx: usize,
    direction: ScanDirection,
) -> Option<(String, usize)> {
    let node = content[idx];

    // FieldAccess nodes (e.g., angle.l as a single node)
    if node.kind() == SyntaxKind::FieldAccess {
        return Some((collect_field_access_text(node), idx));
    }

    // Bracket-type delimiters
    if matches!(
        node.kind(),
        SyntaxKind::LeftParen
            | SyntaxKind::RightParen
            | SyntaxKind::LeftBracket
            | SyntaxKind::RightBracket
            | SyntaxKind::LeftBrace
            | SyntaxKind::RightBrace
    ) {
        return Some((get_node_delimiter_text(node), idx));
    }

    // Identifier-based delimiters (may span multiple tokens like angle + l)
    if matches!(node.kind(), SyntaxKind::MathIdent | SyntaxKind::Ident) {
        return collect_ident_delimiter(content, idx, direction);
    }

    // Fallback: use node text directly
    Some((get_node_delimiter_text(node), idx))
}

/// Collect identifier-based delimiter tokens (handles patterns like "angle l" or "angle.l").
fn collect_ident_delimiter(
    content: &[&SyntaxNode],
    idx: usize,
    direction: ScanDirection,
) -> Option<(String, usize)> {
    let node = content[idx];
    let node_text = node.text().to_string();

    match direction {
        ScanDirection::Forward => {
            // Check for "ident l/r" pattern (no dot, adjacent tokens)
            if idx + 1 < content.len()
                && matches!(
                    content[idx + 1].kind(),
                    SyntaxKind::MathIdent | SyntaxKind::Ident
                )
                && matches!(content[idx + 1].text().as_str(), "l" | "r")
            {
                let candidate = format!("{}.{}", node_text, content[idx + 1].text());
                if is_delimiter(&candidate) {
                    return Some((candidate, idx + 1));
                }
            }
            // Check for "ident.ident..." pattern
            let mut parts = vec![node_text];
            let mut end = idx;
            let mut i = idx;
            while i + 2 < content.len()
                && content[i + 1].kind() == SyntaxKind::Dot
                && matches!(
                    content[i + 2].kind(),
                    SyntaxKind::MathIdent | SyntaxKind::Ident
                )
            {
                parts.push(content[i + 2].text().to_string());
                i += 2;
                end = i;
            }
            Some((parts.join("."), end))
        }
        ScanDirection::Backward => {
            // Check for "ident l/r" pattern (current node is l/r)
            if idx >= 1
                && matches!(
                    content[idx - 1].kind(),
                    SyntaxKind::MathIdent | SyntaxKind::Ident
                )
                && matches!(node.text().as_str(), "l" | "r")
            {
                let candidate = format!("{}.{}", content[idx - 1].text(), node_text);
                if is_delimiter(&candidate) {
                    return Some((candidate, idx - 1));
                }
            }
            // Check for "...ident.ident" pattern
            let mut parts = vec![node_text];
            let mut start = idx;
            let mut i = idx;
            while i >= 2
                && content[i - 1].kind() == SyntaxKind::Dot
                && matches!(
                    content[i - 2].kind(),
                    SyntaxKind::MathIdent | SyntaxKind::Ident
                )
            {
                parts.insert(0, content[i - 2].text().to_string());
                i -= 2;
                start = i;
            }
            Some((parts.join("."), start))
        }
    }
}

fn should_skip_lr_index(
    idx: usize,
    left: Option<&LrDelimiter>,
    right: Option<&LrDelimiter>,
) -> bool {
    if let Some(delim) = left {
        if idx >= delim.start && idx <= delim.end {
            return true;
        }
    }
    if let Some(delim) = right {
        if idx >= delim.start && idx <= delim.end {
            return true;
        }
    }
    false
}

/// Get delimiter text from a node, handling FieldAccess and SyntaxKind specially
fn get_node_delimiter_text(node: &SyntaxNode) -> String {
    match node.kind() {
        SyntaxKind::FieldAccess => collect_field_access_text(node),
        // Handle delimiter tokens by their SyntaxKind
        SyntaxKind::LeftParen => "(".to_string(),
        SyntaxKind::RightParen => ")".to_string(),
        SyntaxKind::LeftBracket => "[".to_string(),
        SyntaxKind::RightBracket => "]".to_string(),
        SyntaxKind::LeftBrace => "{".to_string(),
        SyntaxKind::RightBrace => "}".to_string(),
        // Default: use text content
        _ => node.text().to_string(),
    }
}

/// Recursively collect the full text of a FieldAccess node
/// This handles nested FieldAccess like bar.v.double -> "bar.v.double"
fn collect_field_access_text(node: &SyntaxNode) -> String {
    let mut parts = Vec::new();

    for child in node.children() {
        match child.kind() {
            SyntaxKind::FieldAccess => {
                // Recursively collect nested FieldAccess
                parts.push(collect_field_access_text(child));
            }
            SyntaxKind::MathIdent | SyntaxKind::Ident => {
                parts.push(child.text().to_string());
            }
            SyntaxKind::Dot => {
                // Dot is the separator, we'll join with dots anyway
            }
            _ => {
                // For other node types, try to get their text
                let text = child.text().to_string();
                if !text.is_empty() && text != "." {
                    parts.push(text);
                }
            }
        }
    }

    // Join parts with dots, filtering out empty ones
    let result: Vec<&str> = parts
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();

    result.join(".")
}
