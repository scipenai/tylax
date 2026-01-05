//! Environment handling for LaTeX to Typst conversion
//!
//! This module handles LaTeX environments like figure, table, itemize, equation, etc.

use mitex_parser::syntax::{CmdItem, EnvItem, SyntaxElement, SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;
use std::fmt::Write;

use super::context::{ConversionMode, EnvironmentContext, LatexConverter};
use super::table::{parse_with_grid_parser, CellAlign};
use super::utils::sanitize_label;
use crate::data::constants::{CodeBlockOptions, TheoremStyle, LANGUAGE_MAP, THEOREM_TYPES};

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
        "tabular" | "tabular*" | "tabularx" | "longtable" | "longtabu" | "array" => {
            convert_tabular(conv, &node, output);
        }

        // List environments
        "itemize" => {
            conv.state.push_env(EnvironmentContext::Itemize);
            output.push('\n');
            conv.visit_env_content(&node, output);
            conv.state.pop_env();
            output.push('\n');
        }
        "enumerate" => {
            conv.state.push_env(EnvironmentContext::Enumerate);
            output.push('\n');
            conv.visit_env_content(&node, output);
            conv.state.pop_env();
            output.push('\n');
        }
        "description" => {
            conv.state.push_env(EnvironmentContext::Description);
            output.push('\n');
            conv.visit_env_content(&node, output);
            conv.state.pop_env();
            output.push('\n');
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
fn convert_figure(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
    conv.state.push_env(EnvironmentContext::Figure);

    output.push_str("\n#figure(\n");

    // Find image and caption using AST
    let mut has_image = false;
    let mut caption_cmd: Option<CmdItem> = None;
    let mut label_text = String::new();

    for child in node.children_with_tokens() {
        if let SyntaxElement::Node(n) = &child {
            if let Some(cmd) = CmdItem::cast(n.clone()) {
                if let Some(name_tok) = cmd.name_tok() {
                    let name = name_tok.text();
                    if name == "\\includegraphics" {
                        has_image = true;
                        output.push_str("  image(\"");
                        if let Some(path) = conv.get_required_arg(&cmd, 0) {
                            output.push_str(&path);
                        }
                        output.push_str("\"),\n");
                    } else if name == "\\caption" {
                        // Store the command for later conversion
                        caption_cmd = Some(cmd.clone());
                    } else if name == "\\label" {
                        if let Some(lbl) = conv.get_required_arg(&cmd, 0) {
                            label_text = lbl;
                        }
                    }
                }
            }
        }
    }

    if !has_image {
        output.push_str("  [],\n"); // Placeholder
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
    let mut table_content = String::new();

    // First pass: extract caption, label, and tabular content using AST
    for child in node.children_with_tokens() {
        if let SyntaxElement::Node(n) = &child {
            if let Some(cmd) = CmdItem::cast(n.clone()) {
                if let Some(name_tok) = cmd.name_tok() {
                    let name = name_tok.text();
                    if name == "\\caption" {
                        caption_cmd = Some(cmd.clone());
                    } else if name == "\\label" {
                        if let Some(lbl) = conv.get_required_arg(&cmd, 0) {
                            label_text = lbl;
                        }
                    }
                }
            }
            // Check for tabular environment
            if let Some(env) = EnvItem::cast(n.clone()) {
                if env
                    .name_tok()
                    .map(|t| t.text().to_string())
                    .unwrap_or_default()
                    .starts_with("tabular")
                {
                    // convert_tabular handles its own push/pop of Tabular context
                    convert_tabular(conv, n, &mut table_content);
                }
            }
        }
    }

    // Build properly formatted figure
    output.push_str("\n#figure(");

    // Convert caption content (may contain math)
    if let Some(ref cmd) = caption_cmd {
        if let Some(cap) = conv.get_converted_required_arg(cmd, 0) {
            let _ = writeln!(output, "\n  caption: [{}],", cap);
        }
    }

    output.push_str(")[\n");
    output.push_str(&table_content);
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
    let columns = parse_column_spec(&col_spec);

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

    // Use the new grid parser
    let typst_output = parse_with_grid_parser(&content, alignments);
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

    // Apply math cleanup
    let cleaned = conv.cleanup_math_spacing(&math_content);

    // For starred equations (equation*), disable numbering
    if is_starred {
        output.push_str("#math.equation(block: true, numbering: none)[\n$ ");
        output.push_str(&cleaned);
        output.push_str(" $\n]");
    } else {
        output.push_str("$ ");
        output.push_str(&cleaned);
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

    // Extract label first using AST (for numbered align environments)
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

    // Apply math cleanup
    let cleaned = conv.cleanup_math_spacing(&math_content);

    if !is_inner {
        // For starred environments (align*, eqnarray*, etc.), disable numbering
        if is_starred {
            output.push_str("#math.equation(block: true, numbering: none)[\n$ ");
            output.push_str(&cleaned);
            output.push_str(" $\n]");
        } else {
            output.push_str("$ ");
            output.push_str(&cleaned);
            output.push_str(" $");

            if !label.is_empty() {
                let _ = write!(output, " <{}>", sanitize_label(&label));
            }
        }
        output.push('\n');
    } else {
        output.push_str(&cleaned);
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
    let delim = match env_name {
        "pmatrix" => Some("("),
        "bmatrix" => Some("["),
        "Bmatrix" => Some("{"),
        "vmatrix" => Some("|"),
        "Vmatrix" => Some("‖"), // Use double bar Unicode character for Typst
        "smallmatrix" | "matrix" => None,
        _ => None,
    };

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

/// Convert a cases environment
fn convert_cases(conv: &mut LatexConverter, node: &SyntaxNode, output: &mut String) {
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
    conv.state.push_env(EnvironmentContext::Bibliography);

    output.push_str("\n= References\n\n");
    output.push_str("#show figure.where(kind: \"bib\"): it => block[#it.caption #it.body]\n");

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
    // Look for ItemBegin, then find the column specification argument
    for child in node.children() {
        if child.kind() == SyntaxKind::ItemBegin {
            // In ItemBegin, look for ClauseArgument with curly braces
            for begin_child in child.children() {
                if begin_child.kind() == SyntaxKind::ClauseArgument {
                    // Check if it's a curly (required) argument
                    let has_curly = begin_child
                        .children()
                        .any(|c| c.kind() == SyntaxKind::ItemCurly);
                    if has_curly {
                        // Extract the content
                        let mut content = String::new();
                        for arg_child in begin_child.children_with_tokens() {
                            match arg_child.kind() {
                                SyntaxKind::TokenLBrace
                                | SyntaxKind::TokenRBrace
                                | SyntaxKind::TokenLBracket
                                | SyntaxKind::TokenRBracket => continue,
                                SyntaxKind::ItemCurly => {
                                    // Extract inner content
                                    if let SyntaxElement::Node(n) = arg_child {
                                        for inner in n.children_with_tokens() {
                                            match inner.kind() {
                                                SyntaxKind::TokenLBrace
                                                | SyntaxKind::TokenRBrace => continue,
                                                _ => {
                                                    if let SyntaxElement::Token(t) = inner {
                                                        content.push_str(t.text());
                                                    } else if let SyntaxElement::Node(n) = inner {
                                                        content.push_str(&n.text().to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    if let SyntaxElement::Token(t) = arg_child {
                                        content.push_str(t.text());
                                    }
                                }
                            }
                        }
                        let trimmed = content.trim().to_string();
                        if !trimmed.is_empty() {
                            return Some(trimmed);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse column specification from LaTeX format (e.g., "l|ccc" -> ["l", "c", "c", "c"])
fn parse_column_spec(spec: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut chars = spec.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            'l' | 'c' | 'r' => columns.push(c.to_string()),
            'p' | 'm' | 'b' | 'X' => {
                // Skip width specification
                if chars.peek() == Some(&'{') {
                    let mut depth = 0;
                    for ch in chars.by_ref() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                }
                columns.push("l".to_string()); // Default to left
            }
            '*' => {
                // Repeat specification *{n}{spec}
                if chars.peek() == Some(&'{') {
                    chars.next();
                    let mut count_str = String::new();
                    for ch in chars.by_ref() {
                        if ch == '}' {
                            break;
                        }
                        count_str.push(ch);
                    }
                    let count: usize = count_str.parse().unwrap_or(1);

                    if chars.peek() == Some(&'{') {
                        chars.next();
                        let mut spec_str = String::new();
                        let mut depth = 1;
                        for ch in chars.by_ref() {
                            if ch == '{' {
                                depth += 1;
                            } else if ch == '}' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            spec_str.push(ch);
                        }

                        let inner_cols = parse_column_spec(&spec_str);
                        for _ in 0..count {
                            columns.extend(inner_cols.clone());
                        }
                    }
                }
            }
            '|' | '@' | '!' => {
                // Skip separators and @{} expressions
                if (c == '@' || c == '!') && chars.peek() == Some(&'{') {
                    let mut depth = 0;
                    for ch in chars.by_ref() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if columns.is_empty() {
        columns.push("l".to_string());
    }

    columns
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
