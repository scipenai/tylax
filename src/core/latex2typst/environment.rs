//! Environment handling for LaTeX to Typst conversion
//!
//! This module handles LaTeX environments like figure, table, itemize, equation, etc.

use mitex_parser::syntax::{CmdItem, EnvItem, SyntaxElement, SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;
use std::fmt::Write;

use super::context::{env_header_args, ConversionMode, EnvironmentContext, LatexConverter};
use super::table::{parse_with_grid_parser, CellAlign};
use super::utils::sanitize_label;
use super::{ConversionWarning, WarningKind};
use crate::data::constants::{CodeBlockOptions, TheoremStyle, LANGUAGE_MAP, THEOREM_TYPES};

/// Split a cleaned Typst math body into row strings.
///
/// Rows are separated either by a literal newline (how `align`'s `\\` currently
/// surfaces after cleanup) or by a lone Typst line break (` \ `). Empty rows are
/// dropped.
fn split_math_rows(cleaned: &str) -> Vec<String> {
    cleaned
        .replace(" \\ ", "\n")
        .split('\n')
        .map(|row| row.trim().to_string())
        .filter(|row| !row.is_empty())
        .collect()
}

/// Split a single inline `#<label>` marker out of a math row.
///
/// Returns the row text with the marker removed and, if present, the label name.
fn take_row_label(row: &str) -> (String, Option<String>) {
    if let Some(start) = row.find("#<") {
        if let Some(rel_end) = row[start + 2..].find('>') {
            let end = start + 2 + rel_end;
            let label = row[start + 2..end].to_string();
            let mut body = String::with_capacity(row.len());
            body.push_str(&row[..start]);
            body.push_str(&row[end + 1..]);
            return (body, Some(label));
        }
    }
    (row.to_string(), None)
}

/// Remove every inline `#<label>` marker from a math body.
fn strip_inline_labels(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("#<") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find('>') {
            Some(rel_end) => rest = &rest[start + 2 + rel_end + 1..],
            None => {
                // Unterminated marker: keep the remainder verbatim.
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Render labelled multi-row math without leaving invalid labels inside `$...$`.
///
/// One label attaches to the whole block, preserving its alignment. Typst cannot
/// give rows of one block independent labels, so that form degrades to separate
/// equations and warns rather than silently dropping the alignment.
fn emit_labelled_math_rows(conv: &mut LatexConverter, output: &mut String, cleaned: &str) -> bool {
    let rows = split_math_rows(cleaned);
    let labels: Vec<String> = rows
        .iter()
        .filter_map(|row| take_row_label(row).1)
        .collect();
    if labels.is_empty() {
        return false;
    }

    if labels.len() == 1 {
        output.push_str("$ ");
        output.push_str(strip_inline_labels(cleaned).trim());
        output.push_str(" $");
        let _ = writeln!(output, " <{}>", sanitize_label(&labels[0]));
        return true;
    }

    let message = "Multiple labelled rows in a LaTeX alignment are emitted as separate equations because Typst cannot attach independent labels to rows of one aligned math block.";
    conv.state.warnings.push(message.to_string());
    conv.state
        .add_warning(ConversionWarning::new(WarningKind::ParseError, message));

    for row in rows {
        let (body, label) = take_row_label(&row);
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        output.push_str("$ ");
        output.push_str(body);
        output.push_str(" $");
        if let Some(label) = label {
            let _ = write!(output, " <{}>", sanitize_label(&label));
        }
        output.push('\n');
    }

    true
}

/// Convert a LaTeX environment
pub fn convert_environment(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    let node = match &elem {
        SyntaxElement::Node(n) => n.clone(),
        _ => return,
    };

    let env = match EnvItem::cast(node.clone()) {
        Some(e) => e,
        None => return,
    };

    let env_name = env.name_tok().map(|t| t.text().to_string());
    let env_str = env_name.as_deref().unwrap_or("");

    match env_str {
        // Document environment - marks end of preamble
        "document" => {
            conv.state.in_preamble = false;
            conv.visit_env_content(&node, output);
        }

        // Figure environment
        "figure" | "figure*" => {
            convert_figure(conv, &node, output);
        }

        // Table environment
        "table" | "table*" => {
            convert_table(conv, &node, output);
        }

        // Tabular environment
        "tabular" | "tabular*" | "tabularx" | "longtable" | "longtabu" => {
            convert_tabular(conv, &node, output);
        }

        // Array environment (math-mode matrix with column alignment spec)
        "array" => {
            convert_array(conv, &node, output);
        }

        // Lists rely on each `\item` starting its own line, so no leading
        // `\n`: a nested list must stay flush under its parent (issue #43).
        "itemize" => {
            convert_list(conv, &node, EnvironmentContext::Itemize, output);
        }
        "enumerate" => {
            convert_list(conv, &node, EnvironmentContext::Enumerate, output);
        }
        "description" => {
            convert_list(conv, &node, EnvironmentContext::Description, output);
        }

        // Math environments
        "equation" | "equation*" => {
            convert_equation(conv, &node, env_str, output);
        }
        "align" | "align*" | "aligned" | "alignat" | "alignat*" | "flalign" | "flalign*"
        | "eqnarray" | "eqnarray*" => {
            convert_align(conv, &node, env_str, output);
        }
        "gather" | "gather*" => {
            convert_gather(conv, &node, env_str, output);
        }
        "multline" | "multline*" => {
            convert_multline(conv, &node, env_str, output);
        }
        "split" => {
            // split is usually inside equation, just process content
            conv.state.push_env(EnvironmentContext::Align);
            let mut content = String::new();
            conv.visit_env_content(&node, &mut content);
            conv.state.pop_env();
            output.push_str(&content);
        }

        // Matrix environments
        "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" | "smallmatrix" => {
            convert_matrix(conv, &node, env_str, output);
        }

        // Cases
        "cases" | "dcases" | "rcases" => {
            convert_cases(conv, &node, output);
        }

        // Code/verbatim environments
        "verbatim" | "verbatim*" | "Verbatim" => {
            convert_verbatim(conv, &node, output);
        }
        "lstlisting" => {
            convert_lstlisting(conv, &node, output);
        }
        "minted" => {
            convert_minted(conv, &node, output);
        }

        // TikZ
        "tikzpicture" => {
            convert_tikz(conv, &node, output);
        }

        // Theorem-like environments
        "theorem" | "lemma" | "proposition" | "corollary" | "definition" | "example" | "remark"
        | "proof" | "conjecture" | "claim" | "fact" | "observation" | "property" | "question"
        | "problem" | "solution" | "answer" | "exercise" | "assumption" | "hypothesis"
        | "notation" | "conclusion" => {
            convert_theorem(conv, &node, env_str, output);
        }

        // Quote environments
        "quote" | "quotation" => {
            output.push_str("\n#quote(block: true)[\n");
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }
        "verse" => {
            output.push_str("#block(inset: (left: 2em))[\n");
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }

        // Abstract
        "abstract" => {
            output.push_str("\n#block(width: 100%, inset: 1em)[\n");
            output.push_str("  #align(center)[#text(weight: \"bold\")[Abstract]]\n  ");
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }

        // Center, flushleft, flushright
        "center" => {
            output.push_str("#align(center)[\n");
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }
        "flushleft" | "raggedright" => {
            output.push_str("#align(left)[\n");
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }
        "flushright" | "raggedleft" => {
            output.push_str("#align(right)[\n");
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }

        // Multi-column layout: `\begin{multicols}{2}[Header]`. The optional
        // argument is a header spanning all columns, so it precedes them.
        "multicols" | "multicols*" => {
            let columns = conv
                .get_env_required_arg(&node, 0)
                .unwrap_or_else(|| "2".to_string());
            if let Some(header) = conv.get_env_optional_arg(&node) {
                let header = header.trim();
                if !header.is_empty() {
                    let _ = writeln!(output, "\n{}", header);
                }
            }
            let _ = writeln!(output, "#columns({})[", columns.trim());
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }

        // Minipage
        "minipage" => {
            let width = conv
                .get_env_required_arg(&node, 0)
                .unwrap_or("100%".to_string());
            let _ = writeln!(output, "#block(width: {})[", convert_dimension(&width));
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }

        // Bibliography
        "thebibliography" => {
            convert_bibliography(conv, &node, output);
        }

        // Appendix
        "appendix" | "appendices" => {
            output.push_str("\n// Appendix\n");
            conv.visit_env_content(&node, output);
        }

        // Frame (beamer)
        "frame" => {
            convert_frame(conv, &node, output);
        }

        // Columns (beamer)
        "columns" => {
            output.push_str("#grid(columns: 2)[\n");
            conv.visit_env_content(&node, output);
            output.push_str("\n]\n");
        }
        "column" => {
            // Individual column in columns environment
            conv.visit_env_content(&node, output);
        }

        // Subfigure
        "subfigure" => {
            convert_subfigure(conv, &node, output);
        }

        // Algorithm
        "algorithm" | "algorithmic" | "algorithm2e" => {
            convert_algorithm(conv, &node, output);
        }

        // Unknown environments - pass through content
        _ => {
            // Check if it's a theorem-like environment defined by user
            if conv.state.counters.contains_key(env_str) {
                convert_theorem(conv, &node, env_str, output);
            } else {
                // Just process content
                let _ = writeln!(output, "/* Begin {} */", env_str);
                conv.visit_env_content(&node, output);
                let _ = write!(output, "\n/* End {} */\n", env_str);
            }
        }
    }
}

// =============================================================================
// Environment conversion functions
// =============================================================================

/// Convert a figure environment
/// Number of list environments already open around this point.
fn open_list_depth(conv: &LatexConverter) -> usize {
    conv.state
        .env_stack
        .iter()
        .filter(|env| {
            matches!(
                env,
                EnvironmentContext::Itemize
                    | EnvironmentContext::Enumerate
                    | EnvironmentContext::Description
            )
        })
        .count()
}

/// Convert a list environment. Typst nests by INDENTATION, already maintained
/// by `push_env`/`pop_env`; what matters here is the paragraph break, since a
/// blank line after a NESTED list would end its parent too (issue #43).
fn convert_list(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    context: EnvironmentContext,
    output: &mut String,
) {
    let nested = open_list_depth(conv) > 0;
    conv.state.push_env(context);
    conv.visit_env_content(node, output);
    conv.state.pop_env();
    if !nested {
        output.push('\n');
    }
}

fn convert_figure(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    conv.state.push_env(EnvironmentContext::Figure);

    // `\caption`/`\label` become `#figure(..)` arguments; EVERYTHING else is
    // real content. Recognising only `\includegraphics` dropped tikzpictures,
    // tabulars and prose (issue #39).
    let mut caption_cmd: Option<CmdItem> = None;
    let mut label_text = String::new();
    let mut body_elements: Vec<SyntaxElement> = Vec::new();

    for child in node.children_with_tokens() {
        if matches!(child.kind(), SyntaxKind::ItemBegin | SyntaxKind::ItemEnd) {
            continue;
        }
        if let SyntaxElement::Node(n) = &child {
            if let Some(cmd) = CmdItem::cast(n.clone()) {
                match cmd.name_tok().as_ref().map(|t| t.text()) {
                    Some("\\caption") => {
                        caption_cmd = Some(cmd.clone());
                        continue;
                    }
                    Some("\\label") => {
                        if let Some(lbl) = conv.get_required_arg(&cmd, 0) {
                            label_text = lbl;
                        }
                        continue;
                    }
                    _ => {}
                }
            }
        }
        body_elements.push(child);
    }

    let mut body = String::new();
    conv.visit_elements(&body_elements, &mut body);
    let body = body.trim();

    output.push_str("\n#figure(\n");
    if body.is_empty() {
        output.push_str("  [],\n");
    } else {
        let _ = writeln!(output, "  [{}],", body);
    }

    // Convert caption content (may contain math like $\downarrow$)
    if let Some(ref cmd) = caption_cmd {
        if let Some(cap) = conv.get_converted_required_arg(cmd, 0) {
            let _ = writeln!(output, "  caption: [{}],", cap);
        }
    }

    output.push(')');

    if !label_text.is_empty() {
        let _ = write!(output, " <{}>", sanitize_label(&label_text));
    }

    output.push('\n');

    conv.state.pop_env();
}

/// Convert a table environment
fn convert_table(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    conv.state.push_env(EnvironmentContext::Table);

    let mut caption_cmd: Option<CmdItem> = None;
    let mut label_text = String::new();
    let mut body_elements: Vec<SyntaxElement> = Vec::new();

    // `\caption`/`\label` become `#figure(..)` arguments; EVERYTHING else is
    // real content. Recognising only the `tabular` child dropped the prose,
    // lists and second tables a `table` float is allowed to carry.
    for child in node.children_with_tokens() {
        if matches!(child.kind(), SyntaxKind::ItemBegin | SyntaxKind::ItemEnd) {
            continue;
        }
        if let SyntaxElement::Node(n) = &child {
            if let Some(cmd) = CmdItem::cast(n.clone()) {
                match cmd.name_tok().as_ref().map(|t| t.text()) {
                    Some("\\caption") => {
                        caption_cmd = Some(cmd.clone());
                        continue;
                    }
                    Some("\\label") => {
                        if let Some(lbl) = conv.get_required_arg(&cmd, 0) {
                            label_text = lbl;
                        }
                        continue;
                    }
                    _ => {}
                }
            }
        }
        body_elements.push(child);
    }

    let mut table_content = String::new();
    conv.visit_elements(&body_elements, &mut table_content);
    let table_content = table_content.trim();

    // Build properly formatted figure
    output.push_str("\n#figure(");

    // Convert caption content (may contain math)
    if let Some(ref cmd) = caption_cmd {
        if let Some(cap) = conv.get_converted_required_arg(cmd, 0) {
            let _ = writeln!(output, "\n  caption: [{}],", cap);
        }
    }

    output.push_str(")[\n");
    output.push_str(table_content);
    output.push_str("\n] ");

    if !label_text.is_empty() {
        let _ = write!(output, "<{}>", sanitize_label(&label_text));
    }

    output.push('\n');

    conv.state.pop_env();
}

/// Convert a tabular environment using the state-aware grid parser
fn convert_tabular(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    conv.state.push_env(EnvironmentContext::Tabular);

    // Save current mode and force Text mode for tabular content
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Text;

    // Get column specification from the environment's first required argument
    let col_spec = get_tabular_col_spec(node).unwrap_or_default();
    let spec = parse_column_spec_full(&col_spec);
    let columns = &spec.columns;

    // Convert column specs to CellAlign
    let alignments: Vec<CellAlign> = columns
        .iter()
        .map(|c| match c.as_str() {
            "l" => CellAlign::Left,
            "r" => CellAlign::Right,
            "c" => CellAlign::Center,
            _ => CellAlign::Auto,
        })
        .collect();

    // Collect table content
    let mut content = String::new();
    conv.visit_env_content(node, &mut content);

    // Restore previous mode
    conv.state.mode = prev_mode;

    // `table.vline` has no double-rule stroke, so `||` renders as one line —
    // a reported downgrade, not a silent one (issue #43).
    let doubled = spec.doubled_vlines();
    if !doubled.is_empty() {
        let message = format!(
            "Double vertical rule (`||`) at column boundary {} drawn as a single rule: \
             Typst's `table.vline` has no double-line stroke.",
            doubled
                .iter()
                .map(|at| at.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        conv.state.warnings.push(message.clone());
        conv.state
            .add_warning(ConversionWarning::new(WarningKind::ParseError, message));
    }

    let typst_output = parse_with_grid_parser(&content, alignments, &spec.distinct_vlines());
    output.push_str(&typst_output);

    conv.state.pop_env();
}

/// Convert an equation environment
fn convert_equation(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    output: &mut String,
) {
    conv.state.push_env(EnvironmentContext::Equation);
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;

    // Check if this is a starred (unnumbered) equation
    let is_starred = env_name.ends_with('*');

    // Extract label first using AST
    let mut label = String::new();
    for child in node.children_with_tokens() {
        if let SyntaxElement::Node(n) = &child {
            if let Some(cmd) = CmdItem::cast(n.clone()) {
                if let Some(name_tok) = cmd.name_tok() {
                    if name_tok.text() == "\\label" {
                        if let Some(lbl) = conv.get_required_arg(&cmd, 0) {
                            label = lbl;
                        }
                    }
                }
            }
        }
    }

    // Collect math content into a buffer for post-processing
    let mut math_content = String::new();
    conv.visit_env_content(node, &mut math_content);

    // The label is emitted once after the equation as a real Typst label, so
    // strip any inline `#<..>` marker the `\label` handler left in the body
    // (issue #43).
    let cleaned = conv.cleanup_math_spacing(&math_content);
    // A label nested in `aligned`/`split` arrives as an inline marker but names
    // this enclosing equation, so retain the first before removing markers.
    if label.is_empty() {
        label = take_row_label(&cleaned).1.unwrap_or_default();
    }
    let cleaned = strip_inline_labels(&cleaned);
    let cleaned = cleaned.trim();

    // For starred equations (equation*), disable numbering
    if is_starred {
        output.push_str("#math.equation(block: true, numbering: none)[\n$ ");
        output.push_str(cleaned);
        output.push_str(" $\n]");
    } else {
        output.push_str("$ ");
        output.push_str(cleaned);
        output.push_str(" $");

        if !label.is_empty() {
            let _ = write!(output, " <{}>", sanitize_label(&label));
        }
    }

    output.push('\n');

    conv.state.mode = prev_mode;
    conv.state.pop_env();
}

/// Convert an align environment
fn convert_align(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    output: &mut String,
) {
    conv.state.push_env(EnvironmentContext::Align);
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;

    // Only add $ for non-aligned (aligned is usually inside math mode already)
    let is_inner = env_name == "aligned";

    // Check if this is a starred (unnumbered) environment
    let is_starred = env_name.ends_with('*');

    // Collect math content into a buffer for post-processing. Per-row `\label`s
    // are converted inline (to `#<..>`) by the command visitor (issue #43).
    let mut math_content = String::new();
    conv.visit_env_content(node, &mut math_content);

    // Apply math cleanup
    let cleaned = conv.cleanup_math_spacing(&math_content);

    // A `#<..>` inside math creates no resolvable label, so an `align` with
    // per-row labels splits into one equation per row (issue #43).
    if is_inner {
        // Inside an outer equation: emit the body unchanged (labels stay inline,
        // which the enclosing block handles).
        output.push_str(&cleaned);
    } else if !is_starred && emit_labelled_math_rows(conv, output, &cleaned) {
        // Labels are emitted by the shared row renderer above.
    } else if is_starred {
        // For starred environments (align*, eqnarray*, etc.), disable numbering
        output.push_str("#math.equation(block: true, numbering: none)[\n$ ");
        output.push_str(&strip_inline_labels(&cleaned));
        output.push_str(" $\n]");
        output.push('\n');
    } else {
        output.push_str("$ ");
        output.push_str(&strip_inline_labels(&cleaned));
        output.push_str(" $");
        output.push('\n');
    }

    conv.state.mode = prev_mode;
    conv.state.pop_env();
}

/// Convert a gather environment
fn convert_gather(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    output: &mut String,
) {
    conv.state.push_env(EnvironmentContext::Equation);
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;

    let is_starred = env_name.ends_with('*');

    let mut content = String::new();
    conv.visit_env_content(node, &mut content);

    conv.state.mode = prev_mode;
    conv.state.pop_env();

    let processed = conv.postprocess_math(content);

    if !is_starred && emit_labelled_math_rows(conv, output, &processed) {
        return;
    }

    let processed = strip_inline_labels(&processed);
    if is_starred {
        let _ = write!(
            output,
            "#math.equation(block: true, numbering: none)[\n$ {} $\n]\n",
            processed.trim()
        );
    } else {
        let _ = writeln!(output, "$ {} $", processed.trim());
    }
}

/// Convert a multline environment
fn convert_multline(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    output: &mut String,
) {
    conv.state.push_env(EnvironmentContext::Equation);
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;

    let is_starred = env_name.ends_with('*');

    let mut content = String::new();
    conv.visit_env_content(node, &mut content);

    conv.state.mode = prev_mode;
    conv.state.pop_env();

    let processed = conv.postprocess_math(content);

    if !is_starred && emit_labelled_math_rows(conv, output, &processed) {
        return;
    }

    let processed = strip_inline_labels(&processed);
    if is_starred {
        let _ = write!(
            output,
            "#math.equation(block: true, numbering: none)[\n$ {} $\n]\n",
            processed.trim()
        );
    } else {
        let _ = writeln!(output, "$ {} $", processed.trim());
    }
}

/// Convert a matrix environment
fn convert_matrix(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    output: &mut String,
) {
    convert_matrix_with_delim(conv, node, env_name, None, output);
}

pub(crate) fn convert_matrix_with_delim(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    delim_override: Option<&str>,
    output: &mut String,
) {
    conv.state.push_env(EnvironmentContext::Matrix);
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;

    let mut content = String::new();
    conv.visit_env_content(node, &mut content);

    conv.state.mode = prev_mode;
    conv.state.pop_env();

    // Determine delimiter type
    // For plain "matrix" environment, use delim: #none
    // For others, use the appropriate delimiter string
    let delim = delim_override.or(match env_name {
        "pmatrix" => Some("("),
        "bmatrix" => Some("["),
        "Bmatrix" => Some("{"),
        "vmatrix" => Some("|"),
        "Vmatrix" => Some("‖"), // Use double bar Unicode character for Typst
        "smallmatrix" | "matrix" => None,
        _ => None,
    });

    // Clean up content - remove zws markers and format
    let content = content
        .replace("zws ;", ";")
        .replace("zws, ", ", ")
        .trim()
        .to_string();

    match delim {
        Some(d) => {
            let _ = write!(output, "mat(delim: \"{}\", {}) ", d, content);
        }
        None => {
            let _ = write!(output, "mat(delim: #none, {}) ", content);
        }
    }
}

/// Convert a \begin{array}{colspec}...\end{array} environment to Typst mat()
///
/// `array` is a math-mode environment that behaves like `matrix` but with an
/// explicit column alignment specification (e.g. `{ccc}`, `{l}`).
/// The delimiter is always `#none` because `array` is typically wrapped by
/// `\left...\right` which handles delimiters separately.
fn convert_array(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    convert_array_with_delim(conv, node, None, output);
}

pub(crate) fn convert_array_with_delim(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    delim_override: Option<&str>,
    output: &mut String,
) {
    conv.state.push_env(EnvironmentContext::Matrix);
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;

    let mut content = String::new();
    conv.visit_env_content(node, &mut content);

    conv.state.mode = prev_mode;
    conv.state.pop_env();

    let content = content
        .replace("zws ;", ";")
        .replace("zws, ", ", ")
        .trim()
        .to_string();

    match delim_override {
        Some(delim) => {
            let _ = write!(output, "mat(delim: \"{}\", {}) ", delim, content);
        }
        None => {
            let _ = write!(output, "mat(delim: #none, {}) ", content);
        }
    }
}

/// Convert a cases environment
pub(crate) fn convert_cases(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    conv.state.push_env(EnvironmentContext::Cases);
    let prev_mode = conv.state.mode;
    conv.state.mode = ConversionMode::Math;

    let mut content = String::new();
    conv.visit_env_content(node, &mut content);

    conv.state.mode = prev_mode;
    conv.state.pop_env();

    // Format as cases
    let content = content.trim();
    let _ = write!(output, "cases({}) ", content);
}

/// Convert a verbatim environment
fn convert_verbatim(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    let content = conv.extract_env_raw_content(node);
    output.push_str("```\n");
    output.push_str(content.trim());
    output.push_str("\n```\n");
}

/// Convert an lstlisting environment
fn convert_lstlisting(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    // Parse options using CodeBlockOptions
    let options_str = conv.get_env_optional_arg(node).unwrap_or_default();
    let options = CodeBlockOptions::parse(&options_str);

    // Get Typst language identifier
    let lang = options.get_typst_language();

    let content = conv.extract_env_raw_content(node);

    // If there's a caption, wrap in figure
    if let Some(ref caption) = options.caption {
        output.push_str("\n#figure(\n");
        output.push_str("```");
        output.push_str(lang);
        output.push('\n');
        output.push_str(content.trim());
        output.push_str("\n```,\n");
        let _ = writeln!(output, "  caption: [{}]", caption);
        output.push(')');
        if let Some(ref label) = options.label {
            let _ = write!(output, " <{}>", sanitize_label(label));
        }
        output.push('\n');
    } else {
        output.push_str("\n```");
        output.push_str(lang);
        output.push('\n');
        output.push_str(content.trim());
        output.push_str("\n```\n");
    }
}

/// Convert a minted environment
fn convert_minted(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    // Minted: \begin{minted}[options]{language} ... \end{minted}
    let options_str = conv.get_env_optional_arg(node).unwrap_or_default();
    let options = CodeBlockOptions::parse(&options_str);

    // Get language from required argument
    let lang_raw = conv.get_env_required_arg(node, 0).unwrap_or_default();
    let lang = LANGUAGE_MAP
        .get(lang_raw.as_str())
        .copied()
        .unwrap_or_else(|| lang_raw.to_lowercase().leak());

    let content = conv.extract_env_raw_content(node);

    // If there's a caption, wrap in figure
    if let Some(ref caption) = options.caption {
        output.push_str("\n#figure(\n");
        output.push_str("```");
        output.push_str(lang);
        output.push('\n');
        output.push_str(content.trim());
        output.push_str("\n```,\n");
        let _ = writeln!(output, "  caption: [{}]", caption);
        output.push(')');
        if let Some(ref label) = options.label {
            let _ = write!(output, " <{}>", sanitize_label(label));
        }
        output.push('\n');
    } else {
        output.push_str("\n```");
        output.push_str(lang);
        output.push('\n');
        output.push_str(content.trim());
        output.push_str("\n```\n");
    }
}

/// Convert a tikzpicture environment
fn convert_tikz(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    conv.state.push_env(EnvironmentContext::TikZ);

    // Use the TikZ to CeTZ transpiler
    let tikz_source = node.text().to_string();
    let cetz_code = crate::tikz::convert_tikz_to_cetz(&tikz_source);

    output.push_str("\n// TikZ converted to CeTZ\n");
    output.push_str(&cetz_code);
    output.push('\n');

    conv.state.pop_env();
}

/// Convert a theorem-like environment
fn convert_theorem(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    env_name: &str,
    output: &mut String,
) {
    let env_ctx = EnvironmentContext::Theorem(env_name.to_string());
    conv.state.push_env(env_ctx);

    // Get theorem info from mapping table, or use defaults
    let (display_name, style) = if let Some(info) = THEOREM_TYPES.get(env_name) {
        (info.display_name.to_string(), info.style)
    } else {
        // Fallback: capitalize first letter
        let name = env_name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
            + &env_name[1..];
        (name, TheoremStyle::Plain)
    };

    // Proof doesn't get numbered
    let is_proof = env_name == "proof";
    let counter_str = if is_proof {
        String::new()
    } else {
        let counter = conv.state.next_counter(env_name);
        format!(" {}", counter)
    };

    // Check for optional argument (theorem name/attribution)
    let custom_name = conv.get_env_optional_arg(node);

    // Standard LaTeX-like theorem format:
    // **Theorem 1.** _Body text in italics._
    // **Definition 1.** Body text in normal font.
    // _Remark 1._ Body text in normal font.
    // _Proof._ Body text. □

    output.push('\n');

    // Format header based on style
    match style {
        TheoremStyle::Plain => {
            // Bold title, will have italic body
            let _ = write!(output, "*{}{}.*", display_name, counter_str);
        }
        TheoremStyle::Definition => {
            // Bold title, normal body
            let _ = write!(output, "*{}{}.*", display_name, counter_str);
        }
        TheoremStyle::Remark => {
            // Italic title, normal body
            let _ = write!(output, "_{}{}._", display_name, counter_str);
        }
    }

    // Add custom name if present
    if let Some(name) = custom_name {
        let _ = write!(output, " _({}.)_", name);
    }
    output.push(' ');

    // Apply body formatting based on style
    let use_italic_body = matches!(style, TheoremStyle::Plain) && !is_proof;

    if use_italic_body {
        output.push('_');
    }

    conv.visit_env_content(node, output);

    if use_italic_body {
        output.push('_');
    }

    // Proof gets QED symbol
    if is_proof {
        output.push_str(" #h(1fr) $square.stroked$");
    }

    output.push_str("\n\n");

    conv.state.pop_env();
}

/// Convert a bibliography environment
fn convert_bibliography(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    // Record the manual-bibliography event so `resolve_citations` rewrites
    // `\cite{k}` to `@k`, matching the `<key>` anchors emitted below, instead
    // of an uncompilable `#cite(<k>)` with no `#bibliography()`.
    conv.citations.saw_manual_bib = true;
    conv.state.push_env(EnvironmentContext::Bibliography);

    output.push_str("\n= References\n\n");
    // Entries stay `#figure`s so `@key` has something numbered to resolve
    // against, but a figure centres its caption on its own line. Lay each out as
    // a left-aligned hanging-indent row; `it.caption.body` is the bare number,
    // without the caption's own styling.
    output.push_str(
        "#show figure.where(kind: \"bib\"): it => block(width: 100%, above: 0.65em, below: 0.65em)[\n  \
         #grid(columns: (auto, 1fr), column-gutter: 0.65em, align: (right + top, left + top),\n    \
         it.caption.body, it.body)\n]\n",
    );

    // Process bibitem commands using the dedicated function
    convert_thebibliography_content(conv, node, output);

    conv.state.pop_env();
}

/// Special converter for thebibliography environment content
fn convert_thebibliography_content(
    conv: &mut LatexConverter,
    node: &SyntaxNode,
    output: &mut String,
) {
    let mut bib_counter = 1;
    let mut current_label = String::new();
    let mut in_item = false;

    for child in node.children_with_tokens() {
        // Check if current child is a \bibitem command
        let is_bibitem = if let SyntaxElement::Node(n) = &child {
            if let Some(cmd) = CmdItem::cast(n.clone()) {
                if let Some(name) = cmd.name_tok() {
                    name.text() == "\\bibitem"
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if is_bibitem {
            // Close previous item
            if in_item {
                output.push_str("] ");
                if !current_label.is_empty() {
                    let _ = write!(output, "<{}>", sanitize_label(&current_label));
                }
                output.push('\n');
            }

            // Start new item
            if let SyntaxElement::Node(n) = &child {
                if let Some(cmd) = CmdItem::cast(n.clone()) {
                    // Get label from arg - use get_required_arg for simple labels
                    if let Some(arg) = conv.get_required_arg(&cmd, 0) {
                        current_label = arg;
                    } else {
                        current_label = String::new();
                    }

                    let _ = write!(
                        output,
                        "#figure(kind: \"bib\", supplement: none, caption: [{}])[",
                        bib_counter
                    );
                    bib_counter += 1;
                    in_item = true;
                }
            }
        } else {
            // If in item, output content
            if in_item {
                // Skip begin/end tokens
                match child.kind() {
                    SyntaxKind::ItemBegin | SyntaxKind::ItemEnd => continue,
                    _ => conv.visit_element(child, output),
                }
            }
        }
    }

    // Close last item
    if in_item {
        output.push_str("] ");
        if !current_label.is_empty() {
            let _ = write!(output, "<{}>", sanitize_label(&current_label));
        }
        output.push('\n');
    }
}

/// Convert a beamer frame
fn convert_frame(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    let title = conv
        .get_env_optional_arg(node)
        .or_else(|| conv.get_env_required_arg(node, 0));

    output.push_str("#slide[\n");

    if let Some(t) = title {
        let _ = write!(output, "  == {}\n\n", t);
    }

    conv.visit_env_content(node, output);

    output.push_str("\n]\n");
}

/// Convert a subfigure
fn convert_subfigure(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    let width = conv
        .get_env_optional_arg(node)
        .unwrap_or("0.5\\linewidth".to_string());
    let width_typst = convert_dimension(&width);

    let _ = writeln!(output, "#box(width: {})[", width_typst);
    conv.visit_env_content(node, output);
    output.push_str("\n]\n");
}

/// Convert an algorithm environment
fn convert_algorithm(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    output.push_str("#block(width: 100%, stroke: 1pt, inset: 10pt)[\n");
    output.push_str("  #text(weight: \"bold\")[Algorithm]\n\n");

    // Process as code-like content
    let content = conv.extract_env_raw_content(node);
    output.push_str("```\n");
    output.push_str(&content);
    output.push_str("\n```\n");

    output.push_str("]\n");
}

// =============================================================================
// Helper functions
// =============================================================================

/// Get the column specification from a tabular environment
/// The col spec is in the first curly arg after the env name: \begin{tabular}{lccc}
fn get_tabular_col_spec(node: &SyntaxNode) -> Option<String> {
    // The column spec is the LAST required slot for every shape in the family:
    // `tabular[pos]{cols}` as well as `tabular*{width}[pos]{cols}`.
    env_header_args(node)
        .into_iter()
        .rfind(|arg| !arg.optional)
        .map(|arg| arg.content)
}

/// Skip over a braced group {...} if present.
/// Handles nested braces correctly.
fn skip_braced_group(chars: &mut std::iter::Peekable<std::str::Chars>) {
    if chars.peek() == Some(&'{') {
        let mut depth = 0;
        for ch in chars.by_ref() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Extract content from a braced group {...} if present.
/// Returns the content without the braces.
fn extract_braced_group(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    if chars.peek() != Some(&'{') {
        return None;
    }
    chars.next(); // consume '{'

    let mut content = String::new();
    let mut depth = 1;
    for ch in chars.by_ref() {
        match ch {
            '{' => {
                depth += 1;
                content.push(ch);
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                content.push(ch);
            }
            _ => content.push(ch),
        }
    }
    Some(content)
}

/// A parsed `\begin{tabular}{...}` column specification.
struct ColumnSpec {
    /// One alignment letter per column.
    columns: Vec<String>,
    /// One entry per `|`, sorted: `0` is left of the first column. Repeats are
    /// KEPT, since `||` is a legal double rule; the renderer decides what it
    /// can draw.
    vlines: Vec<usize>,
}

impl ColumnSpec {
    /// The distinct boundaries carrying at least one rule.
    fn distinct_vlines(&self) -> Vec<usize> {
        let mut distinct = self.vlines.clone();
        distinct.dedup();
        distinct
    }

    /// Boundaries the source drew more than once (`||`, `|||`, …).
    fn doubled_vlines(&self) -> Vec<usize> {
        let mut doubled = Vec::new();
        for at in self.distinct_vlines() {
            if self.vlines.iter().filter(|other| **other == at).count() > 1
                && !doubled.contains(&at)
            {
                doubled.push(at);
            }
        }
        doubled
    }
}

/// Parse a column specification, keeping the vertical rules.
///
/// The `|` separators are real table borders, not decoration, so they have to
/// survive as structured information: once a table switches Typst's default
/// grid off, nothing else would draw them (issue #43).
fn parse_column_spec_full(spec: &str) -> ColumnSpec {
    let mut columns = Vec::new();
    let mut vlines = Vec::new();
    let mut chars = spec.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            'l' | 'c' | 'r' => columns.push(c.to_string()),
            'p' | 'm' | 'b' | 'X' => {
                skip_braced_group(&mut chars); // Skip width specification
                columns.push("l".to_string()); // Default to left
            }
            '*' => {
                // Repeat specification *{n}{spec}
                if let Some(count_str) = extract_braced_group(&mut chars) {
                    let count: usize = count_str.parse().unwrap_or(1);
                    if let Some(spec_str) = extract_braced_group(&mut chars) {
                        let inner = parse_column_spec_full(&spec_str);
                        for _ in 0..count {
                            let base = columns.len();
                            vlines.extend(inner.vlines.iter().map(|at| base + at));
                            columns.extend(inner.columns.iter().cloned());
                        }
                    }
                }
            }
            // A vertical rule sits at the boundary before the next column.
            '|' => vlines.push(columns.len()),
            '@' | '!' => skip_braced_group(&mut chars), // Skip @{} and !{} expressions
            '>' | '<' => skip_braced_group(&mut chars), // Skip column modifiers
            _ => {}
        }
    }

    if columns.is_empty() {
        columns.push("l".to_string());
    }

    // Sorted but NOT deduplicated: `||` must stay visible as two rules.
    vlines.retain(|at| *at <= columns.len());
    vlines.sort_unstable();

    ColumnSpec { columns, vlines }
}

/// Convert a LaTeX dimension to Typst
fn convert_dimension(dim: &str) -> String {
    let dim = dim.trim();

    if dim.contains("\\linewidth") || dim.contains("\\textwidth") {
        if let Some(mult) = dim
            .strip_suffix("\\linewidth")
            .or(dim.strip_suffix("\\textwidth"))
        {
            let mult = mult.trim();
            if mult.is_empty() || mult == "1" {
                return "100%".to_string();
            }
            if let Ok(f) = mult.parse::<f32>() {
                return format!("{}%", (f * 100.0) as i32);
            }
        }
        return "100%".to_string();
    }

    dim.to_string()
}
