//! Math formula handling for LaTeX to Typst conversion
//!
//! This module handles math formulas, delimiters, and math-specific constructs.

use mitex_parser::syntax::{CmdItem, EnvItem, FormulaItem, SyntaxElement, SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;
use std::fmt::Write;

use super::context::{ConversionMode, LatexConverter};
use super::environment::{convert_array_with_delim, convert_matrix_with_delim};
use super::utils::{brackets_balanced, protect_top_level_comma, scan_brackets};
use crate::data::symbols;

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

    if let Some(pending) = conv.state.pending_citation.take() {
        super::markup::emit_pending_citation_from_curly(
            &node,
            pending,
            &mut conv.citations.pending,
            output,
        );
        return;
    }

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
        let raw = node.text().to_string();
        let raw = raw.trim();
        let raw = raw.strip_prefix('{').unwrap_or(raw);
        let raw = raw.strip_suffix('}').unwrap_or(raw);
        let op_call = super::markup::operator_name_call(raw, content.trim());
        if op.is_limits {
            let _ = write!(output, "limits({})", op_call);
        } else {
            output.push_str(&op_call);
        }
        return;
    }

    // Check if it's empty, then visit the group as a sequence so TeX style
    // declarations remain scoped to the group rather than escaping it.
    let children: Vec<_> = node.children_with_tokens().collect();
    let mut has_content = false;
    let mut content = Vec::new();
    for child in children {
        match child.kind() {
            SyntaxKind::TokenWhiteSpace
            | SyntaxKind::TokenLineBreak
            | SyntaxKind::TokenLBrace
            | SyntaxKind::TokenRBrace => {}
            _ => has_content = true,
        }
        if !matches!(
            child.kind(),
            SyntaxKind::TokenLBrace | SyntaxKind::TokenRBrace
        ) {
            content.push(child);
        }
    }
    // A group that opens with a script has an empty base, as `{}_i` does. Typst
    // would attach the script to whatever precedes the group (`a + {_i}` would
    // put `i` under the `+`), so give it an explicit empty base. A group that is
    // itself a script (`k_{_{AB}}`) is left to `fix_baseless_attachment`. The
    // base is `zws`, as for `{}_i`, so both spellings read back the same.
    if matches!(conv.state.mode, ConversionMode::Math)
        && node.parent().map(|p| p.kind()) != Some(SyntaxKind::ItemAttachComponent)
        && starts_with_baseless_script(&content)
    {
        conv.attachment_repairs.note_empty_base();
        output.push_str("zws");
    }
    conv.visit_elements(&content, output);
    // Add zero-width space for empty groups in math mode
    if !has_content && matches!(conv.state.mode, ConversionMode::Math) {
        output.push_str("zws ");
    }
}

/// Whether the first item of a group is a `_` or `^` script with no base.
fn starts_with_baseless_script(content: &[SyntaxElement]) -> bool {
    let is_trivia = |e: &SyntaxElement| {
        matches!(
            e.kind(),
            SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak
        )
    };
    let Some(SyntaxElement::Node(first)) = content.iter().find(|e| !is_trivia(e)) else {
        return false;
    };
    first.kind() == SyntaxKind::ItemAttachComponent
        && first
            .children_with_tokens()
            .find(|e| !is_trivia(e))
            .is_some_and(|e| {
                matches!(
                    e.kind(),
                    SyntaxKind::TokenUnderscore | SyntaxKind::TokenCaret
                )
            })
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
                    // The delimiter can be a Token (like "(") or a Node (like \lVert command)
                    // First try to find it in the full text after \left
                    if let Some(delim_text) = text.strip_prefix("\\left") {
                        let delim_text = delim_text.trim();
                        if !delim_text.is_empty() {
                            // Extract just the delimiter part (first command or symbol)
                            let delim = extract_delimiter_from_text(delim_text);
                            left_delim = Some(convert_delimiter(delim));
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
                    // First try to find it in the full text after \right
                    if let Some(delim_text) = text.strip_prefix("\\right") {
                        let delim_text = delim_text.trim();
                        if !delim_text.is_empty() {
                            // Extract just the delimiter part (first command or symbol)
                            let delim = extract_delimiter_from_text(delim_text);
                            right_delim = Some(convert_delimiter(delim));
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
    let body_children = children.get(body_start..body_end).unwrap_or_default();

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

    // `\left\{ ... \right.` is LaTeX's piecewise-definition idiom. Convert a
    // contained matrix-like environment while the AST still distinguishes
    // columns (`&`), row breaks (`\\`), and literal commas. Reconstructing a
    // `cases(...)` call from the rendered `mat(...)` text loses that distinction.
    if left_delim.as_deref() == Some("{") && right_delim.as_deref() == Some(".") {
        if let Some(cases_env) =
            classify_lr_cases_environment(children.as_slice(), body_start, body_end)
        {
            super::environment::convert_cases(conv, &cases_env, output);
            return;
        }
    }

    // Check for matrix-like bodies first so determinant-style expressions with
    // vertical bars do not get collapsed into abs()/norm().
    if let Some(matrix_like) = classify_lr_matrix_like(children.as_slice(), body_start, body_end) {
        match (
            &matrix_like.kind,
            left_delim.as_deref(),
            right_delim.as_deref(),
        ) {
            (LrMatrixKind::NoIntrinsicDelim, Some("("), Some(")")) => {
                emit_lr_matrix_like(conv, &matrix_like.node, &matrix_like.env_name, "(", output);
                return;
            }
            (LrMatrixKind::NoIntrinsicDelim, Some("["), Some("]")) => {
                emit_lr_matrix_like(conv, &matrix_like.node, &matrix_like.env_name, "[", output);
                return;
            }
            (LrMatrixKind::NoIntrinsicDelim, Some("{"), Some("}")) => {
                emit_lr_matrix_like(conv, &matrix_like.node, &matrix_like.env_name, "{", output);
                return;
            }
            (LrMatrixKind::NoIntrinsicDelim, Some("bar.v"), Some("bar.v")) => {
                emit_lr_matrix_like(conv, &matrix_like.node, &matrix_like.env_name, "|", output);
                return;
            }
            (LrMatrixKind::NoIntrinsicDelim, Some("bar.v.double"), Some("bar.v.double")) => {
                emit_lr_matrix_like(conv, &matrix_like.node, &matrix_like.env_name, "‖", output);
                return;
            }
            (LrMatrixKind::WithIntrinsicDelim, Some("bar.v"), Some("bar.v")) => {
                emit_nested_lr_matrix_like(
                    conv,
                    &matrix_like.node,
                    &matrix_like.env_name,
                    "bar.v",
                    output,
                );
                return;
            }
            (LrMatrixKind::WithIntrinsicDelim, Some("bar.v.double"), Some("bar.v.double")) => {
                emit_nested_lr_matrix_like(
                    conv,
                    &matrix_like.node,
                    &matrix_like.env_name,
                    "bar.v.double",
                    output,
                );
                return;
            }
            _ => {}
        }
    }

    // \left| ... \right| -> abs(...) and \left\| ... \right\| -> norm(...)
    let bar_function = match (left_delim.as_deref(), right_delim.as_deref()) {
        (Some("bar.v"), Some("bar.v")) => Some(("bar.v", "abs")),
        (Some("bar.v.double"), Some("bar.v.double")) => Some(("bar.v.double", "norm")),
        _ => None,
    };
    if let Some((bar, function)) = bar_function {
        let mut content = String::new();
        visit_lr_body(conv, body_children, &mut content);
        if is_delimited_matrix(&content) {
            let _ = write!(output, "lr({bar} {} {bar}) ", content.trim());
        } else {
            // A top-level comma would split the argument.
            let _ = write!(output, "{function}({}) ", protect_top_level_comma(&content));
        }
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
        visit_lr_body(conv, body_children, output);

        // Output right delimiter if present
        if let Some(ref delim) = right_delim {
            if delim != "." && !delim.is_empty() {
                output.push_str(delim);
                output.push(' ');
            }
        }
        return;
    }

    // Output body content
    let mut body = String::new();
    visit_lr_body(conv, body_children, &mut body);

    let delimiters = |name_brackets: bool| {
        [&left_delim, &right_delim].map(|delim| {
            let delim = delim.as_deref().filter(|d| *d != "." && !d.is_empty());
            match delim {
                Some(d) if name_brackets => bracket_delimiter_name(d).unwrap_or(d),
                _ => delim.unwrap_or_default(),
            }
        })
    };
    let join = |[left, right]: [&str; 2]| {
        let mut inner = String::new();
        if !left.is_empty() {
            inner.push_str(left);
            inner.push(' ');
        }
        inner.push_str(&body);
        if !right.is_empty() {
            inner.push(' ');
            inner.push_str(right);
        }
        inner
    };
    let mut inner = join(delimiters(false));
    // `lr({ ...)` is left for `fix_left_brace_cases`, which makes it `cases`.
    let brace_cases = left_delim.as_deref() == Some("{") && right_delim.as_deref() == Some(".");
    if use_lr && !brace_cases {
        // Typst closes an opening bracket with any closing one, so a bracket
        // delimiter that does not balance opens a group or ends the call:
        // `\left( a \right.` gave `lr(( a)`. Spell such delimiters by name.
        if !brackets_balanced(&inner, &[]) {
            inner = join(delimiters(true));
        }
        // A comma outside every group would split the argument of `lr`.
        inner = protect_top_level_comma(&inner);
    }

    // `lr` takes a bracket at either edge of its content as its own delimiter,
    // so `\left. \left[ n \right] \right.` would scale `[` and `]` as the outer
    // pair. With no delimiter on either side, the content alone renders the same,
    // unless it carries a script.
    let null_pair = left_delim.as_deref() == Some(".") && right_delim.as_deref() == Some(".");
    if null_pair && edge_is_bracket(&body) && !super::markup::is_script_base(&node) {
        output.push_str(body.trim());
        output.push(' ');
    } else if use_lr {
        let _ = write!(output, "lr({}) ", inner);
    } else {
        output.push_str(&inner);
        output.push(' ');
    }
}

/// Whether converted math starts or ends with something `lr` would take as a delimiter.
fn edge_is_bracket(body: &str) -> bool {
    let body = body.trim();
    body.starts_with(['(', '[', '{', '|', '‖'])
        || body.ends_with([')', ']', '}', '|', '‖'])
        || ["bar.v", "chevron.l", "floor.l", "ceil.l"]
            .iter()
            .any(|open| body.starts_with(open))
        || ["bar.v", "bar.v.double", "chevron.r", "floor.r", "ceil.r"]
            .iter()
            .any(|close| body.ends_with(close))
}

enum LrMatrixKind {
    NoIntrinsicDelim,
    WithIntrinsicDelim,
}

struct LrMatrixBody {
    env_name: String,
    node: SyntaxNode,
    kind: LrMatrixKind,
}

/// Return a standalone environment that represents the body of a piecewise
/// expression. These environments use the same row/cell grammar as `cases`,
/// so they can be converted under `EnvironmentContext::Cases` directly.
fn classify_lr_cases_environment(
    children: &[SyntaxElement],
    body_start: usize,
    body_end: usize,
) -> Option<SyntaxNode> {
    let mut significant = Vec::new();

    for child in children.iter().take(body_end).skip(body_start) {
        match child {
            SyntaxElement::Token(t)
                if matches!(
                    t.kind(),
                    SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak
                ) => {}
            SyntaxElement::Token(t) if t.text() == "." => {}
            SyntaxElement::Node(n) if n.kind() == SyntaxKind::ClauseLR => {}
            _ => significant.push(child),
        }
    }

    if significant.len() != 1 {
        return None;
    }

    let node = unwrap_trivial_matrix_wrapper(significant[0].clone())?;
    let env = EnvItem::cast(node.clone())?;
    matches!(
        env.name_tok()?.text(),
        "aligned" | "array" | "matrix" | "smallmatrix"
    )
    .then_some(node)
}

fn classify_lr_matrix_like(
    children: &[SyntaxElement],
    body_start: usize,
    body_end: usize,
) -> Option<LrMatrixBody> {
    let mut significant = Vec::new();

    for child in children.iter().take(body_end).skip(body_start) {
        match child {
            SyntaxElement::Token(t)
                if matches!(
                    t.kind(),
                    SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak
                ) => {}
            SyntaxElement::Token(t) if t.text() == "." => {}
            SyntaxElement::Node(n) if n.kind() == SyntaxKind::ClauseLR => {}
            _ => significant.push(child),
        }
    }

    if significant.len() != 1 {
        return None;
    }

    let node = unwrap_trivial_matrix_wrapper(significant[0].clone())?;
    let env = mitex_parser::syntax::EnvItem::cast(node.clone())?;
    let env_name = env.name_tok()?.text().to_string();

    let kind = match env_name.as_str() {
        "array" | "matrix" | "smallmatrix" => LrMatrixKind::NoIntrinsicDelim,
        "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" => {
            LrMatrixKind::WithIntrinsicDelim
        }
        _ => return None,
    };

    Some(LrMatrixBody {
        env_name,
        node,
        kind,
    })
}

fn unwrap_trivial_matrix_wrapper(elem: SyntaxElement) -> Option<SyntaxNode> {
    match elem {
        SyntaxElement::Node(node) => match node.kind() {
            SyntaxKind::ItemCurly | SyntaxKind::ItemText | SyntaxKind::ClauseArgument => {
                let mut significant = Vec::new();
                for child in node.children_with_tokens() {
                    match child.kind() {
                        SyntaxKind::TokenWhiteSpace
                        | SyntaxKind::TokenLineBreak
                        | SyntaxKind::TokenLBrace
                        | SyntaxKind::TokenRBrace => {}
                        _ => significant.push(child),
                    }
                }
                if significant.len() == 1 {
                    unwrap_trivial_matrix_wrapper(significant[0].clone())
                } else {
                    Some(node)
                }
            }
            _ => Some(node),
        },
        SyntaxElement::Token(_) => None,
    }
}

fn emit_lr_matrix_like(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    delim: &str,
    output: &mut String,
) {
    match env_name {
        "array" => convert_array_with_delim(conv, node, Some(delim), output),
        _ => convert_matrix_with_delim(conv, node, env_name, Some(delim), output),
    }
}

fn emit_nested_lr_matrix_like(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    outer_delim: &str,
    output: &mut String,
) {
    output.push_str("lr(");
    output.push_str(outer_delim);
    output.push(' ');
    convert_matrix_with_delim(conv, node, env_name, None, output);
    output.push(' ');
    output.push_str(outer_delim);
    output.push_str(") ");
}

/// Whether converted math is a single delimited matrix, `mat(delim: "(", …)`.
/// Bars round one keep the `lr` form that `\left| \begin{pmatrix}` gets, so
/// `\left| \left( \begin{matrix} … \right) \right|` reads back the same.
fn is_delimited_matrix(content: &str) -> bool {
    let content = content.trim();
    if !content.starts_with("mat(delim: \"") {
        return false;
    }
    // The call must close at the very end, so `mat(…) x` does not count.
    let call = &content["mat".len()..];
    let mut close = None;
    scan_brackets(call, |offset, ch, depth| {
        let closes_call = depth == 1 && matches!(ch, ')' | ']' | '}');
        if closes_call {
            close = Some(offset);
        }
        !closes_call
    });
    close == Some(call.len() - 1)
}

/// Convert subscript/superscript attachment
pub fn convert_attachment(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    let node = match elem {
        SyntaxElement::Node(n) => n,
        _ => return,
    };

    // `\underbrace{body}_{label}` / `\overbrace{body}^{label}` parse as an
    // attachment whose nucleus is the brace command and whose script is the
    // label. Typst takes the label as a second positional argument, so fold it
    // in (`underbrace(body, label)`) instead of emitting a literal script.
    if let Some(folded) = fold_brace_annotation(conv, &node) {
        output.push_str(&folded);
        return;
    }

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

/// Fold a `\underbrace`/`\overbrace` brace annotation into Typst's second
/// positional argument.
///
/// The attachment parses as `ItemAttachComponent[ ClauseArgument[ ItemCmd ],
/// marker, script ]`, where the nucleus is the brace command and the script is
/// the label. Emitting it verbatim yields `underbrace(body)_(label)`, which
/// renders the label as a real subscript instead of the brace label; Typst
/// expects `underbrace(body, label)`.
///
/// Only the canonical pairings fold: `\underbrace` with `_`, `\overbrace` with
/// `^`. Any other shape (non-brace nucleus, the opposite/extra script) returns
/// `None` so the generic attachment handling applies unchanged.
fn fold_brace_annotation(conv: &mut LatexConverter, node: &SyntaxNode) -> Option<String> {
    let mut nucleus: Option<SyntaxNode> = None;
    let mut marker: Option<SyntaxKind> = None;
    let mut script: Option<SyntaxElement> = None;

    for child in node.children_with_tokens() {
        match child.kind() {
            SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak => continue,
            kind @ (SyntaxKind::TokenUnderscore | SyntaxKind::TokenCaret) => {
                if marker.is_some() {
                    return None; // a second script (e.g. `_a^b`): not a plain label
                }
                marker = Some(kind);
            }
            _ if marker.is_none() => {
                if nucleus.is_some() {
                    return None; // unexpected extra content before the script
                }
                nucleus = Some(child.into_node()?);
            }
            _ => {
                if script.is_some() {
                    return None; // unexpected extra content after the script
                }
                script = Some(child);
            }
        }
    }

    let marker = marker?;
    // The nucleus command is wrapped in a `ClauseArgument`; descend to the
    // `ItemCmd` (also accept a bare `ItemCmd` defensively).
    let nucleus = nucleus?;
    let cmd_node = if nucleus.kind() == SyntaxKind::ItemCmd {
        nucleus
    } else {
        nucleus
            .children()
            .find(|n| n.kind() == SyntaxKind::ItemCmd)?
    };
    let cmd = CmdItem::cast(cmd_node)?;
    let name = cmd.name_tok()?.text().trim_start_matches('\\').to_string();

    // Non-canonical pairings (e.g. `\underbrace{..}^{..}`) are returned to the
    // generic attachment handler untouched. We bail out *before* converting so
    // that path sees an unconsumed nucleus/script.
    let folds = matches!(
        (name.as_str(), marker),
        ("underbrace", SyntaxKind::TokenUnderscore) | ("overbrace", SyntaxKind::TokenCaret)
    );
    if !folds {
        return None;
    }

    let script = script?;
    // The folded body is math content (e.g. `AB` in `\underbrace{AB}_{C}`); force
    // math rendering so it splits into atoms `A B`, matching the standalone
    // `\underbrace` path and the annotation below. Otherwise the document/CLI
    // path emits `underbrace(AB, C)`, which Typst rejects as `unknown variable:
    // AB` (issue #35).
    let body = conv.convert_required_math_arg(&cmd, 0)?;
    let previous_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;
    let mut label = String::new();
    conv.visit_element(script, &mut label);
    conv.state.mode = previous_mode;

    // A top-level comma in either operand would be read as an extra positional
    // argument to the brace function; wrap such operands in `{}` so each stays a
    // single argument (same protection used for `sqrt`, `root`, ...).
    let body = protect_top_level_comma(&body);
    let label = protect_top_level_comma(&label);

    Some(format!("{}({}, {}) ", name, body, label))
}

// =============================================================================
// Helper functions
// =============================================================================

/// Extract delimiter from text after \left or \right.
///
/// This function handles all LaTeX delimiter forms:
/// - Single character: `(`, `)`, `[`, `]`, `|`, `.`
/// - Letter-based commands: `\langle`, `\rangle`, `\lVert`, `\lfloor`, etc.
/// - Non-letter commands: `\|`, `\{`, `\}`
fn extract_delimiter_from_text(text: &str) -> &str {
    if text.is_empty() {
        return ".";
    }

    if let Some(after_backslash) = text.strip_prefix('\\') {
        // Check if the character after \ is a letter
        if after_backslash.is_empty() {
            return "\\";
        }

        let first_char = after_backslash.chars().next().unwrap();
        if first_char.is_ascii_alphabetic() {
            // Letter-based command: \langle, \lVert, etc.
            // Find where the command name ends
            let end = after_backslash
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(after_backslash.len());
            &text[..end + 1] // +1 for the backslash
        } else {
            // Non-letter command: \|, \{, \}
            // The command is exactly \ + one character
            let char_len = first_char.len_utf8();
            &text[..1 + char_len]
        }
    } else {
        // Single character delimiter: (, ), [, ], |, .
        let first_char = text.chars().next().unwrap();
        &text[..first_char.len_utf8()]
    }
}

/// The Typst symbol name of a bracket delimiter.
fn bracket_delimiter_name(delim: &str) -> Option<&'static str> {
    match delim {
        "(" => Some("paren.l"),
        ")" => Some("paren.r"),
        "[" => Some("bracket.l"),
        "]" => Some("bracket.r"),
        "{" => Some("brace.l"),
        "}" => Some("brace.r"),
        _ => None,
    }
}

/// Visit the content between `\left` and `\right`.
///
/// It goes through `visit_elements`, so a `\displaystyle` inside is scoped
/// as in any other group; one element at a time it fell through to the bare
/// identifier `display`.
fn visit_lr_body(conv: &mut LatexConverter, body: &[SyntaxElement], output: &mut String) {
    let body: Vec<SyntaxElement> = body
        .iter()
        .filter(|child| match child {
            SyntaxElement::Token(t) => t.text() != "." && !t.text().starts_with("\\right"),
            SyntaxElement::Node(_) => true,
        })
        .cloned()
        .collect();
    conv.visit_elements(&body, output);
}

/// Convert a LaTeX delimiter to Typst equivalent
fn convert_delimiter(delim: &str) -> String {
    let delim = delim.trim();
    match delim {
        "." => ".", // Empty delimiter
        "\\uparrow" => "arrow.t",
        "\\downarrow" => "arrow.b",
        "\\updownarrow" => "arrow.t.b",
        "\\Uparrow" => "arrow.t.double",
        "\\Downarrow" => "arrow.b.double",
        "\\Updownarrow" => "arrow.t.b.double",
        "\\lgroup" => "paren.l.flat",
        "\\rgroup" => "paren.r.flat",
        _ => symbols::convert_delimiter(delim).unwrap_or(delim),
    }
    .to_string()
}
