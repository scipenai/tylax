//! Markup mode conversion for Typst to LaTeX
//!
//! Handles document structure, text formatting, and non-math content.

use super::context::{ConvertContext, EnvironmentContext, TokenType};
use super::engine::{render_math_segments_to_typst_source, ContentNode};
use super::math::convert_math_node;
use super::table::{LatexCell, LatexCellAlign, LatexHLine, LatexTableGenerator};
use super::utils::{
    count_heading_markers, escape_latex_text, extract_length_value, format_latex_color_command,
    get_raw_text_with_lang, get_simple_text, get_string_content, is_display_math,
    is_string_or_content, normalize_typst_color_expr, parse_angle_value, parse_spacing_spec,
    FuncArgs, SpacingSpec,
};
use crate::data::typst_compat::{
    get_heading_command, is_math_func_in_markup, MarkupHandler, TYPST_MARKUP_HANDLERS,
};
use crate::features::refs::{
    citation_mode_from_typst_form, citation_to_latex, label_to_latex, reference_to_latex, Citation,
    CiteGroup, Reference,
};
use crate::tikz::{convert_cetz_to_tikz, is_cetz_code};
use typst_syntax::{parse_math, SyntaxKind, SyntaxNode};

/// Languages supported by the listings package (case-insensitive check)
/// This is a subset of commonly used languages that listings supports by default
const LISTINGS_SUPPORTED_LANGUAGES: &[&str] = &[
    "abap",
    "acsl",
    "ada",
    "algol",
    "ant",
    "assembler",
    "awk",
    "bash",
    "basic",
    "c",
    "caml",
    "cil",
    "clean",
    "cobol",
    "comsol",
    "csh",
    "delphi",
    "eiffel",
    "erlang",
    "euphoria",
    "fortran",
    "gcl",
    "gnuplot",
    "haskell",
    "html",
    "idl",
    "inform",
    "java",
    "jvmis",
    "ksh",
    "lisp",
    "logo",
    "lua",
    "make",
    "mathematica",
    "matlab",
    "mercury",
    "metapost",
    "miranda",
    "mizar",
    "ml",
    "modula-2",
    "mupad",
    "nastran",
    "oberon-2",
    "ocl",
    "octave",
    "oz",
    "pascal",
    "perl",
    "php",
    "pl/i",
    "plasm",
    "postscript",
    "pov",
    "prolog",
    "promela",
    "pstricks",
    "python",
    "r",
    "reduce",
    "rexx",
    "rsl",
    "ruby",
    "s",
    "sas",
    "scala",
    "scilab",
    "sh",
    "shelxl",
    "simula",
    "sparql",
    "sql",
    "tcl",
    "tex",
    "vbscript",
    "verilog",
    "vhdl",
    "vrml",
    "xml",
    "xslt",
    // Additional common aliases
    "c++",
    "cpp",
    "objective-c",
    "objc",
    "javascript",
    "js",
    "typescript",
    "ts",
];

fn flush_typst_chunk(buffer: &mut String, ctx: &mut ConvertContext) {
    if buffer.trim().is_empty() {
        buffer.clear();
        return;
    }
    let root = typst_syntax::parse(buffer);
    convert_markup_node(&root, ctx);
    buffer.clear();
}

fn emit_rendered_math(ctx: &mut ConvertContext, math_content: &str, is_block: bool) {
    let in_table = ctx.is_in_env(&EnvironmentContext::Table);

    if math_content.trim().is_empty() {
        return;
    }

    let has_alignment = has_unescaped_alignment(math_content);

    if !in_table && has_alignment {
        ctx.push("\\begin{align}\n");
        ctx.push(math_content);
        ctx.push("\n\\end{align}");
    } else if is_block && !in_table {
        ctx.push("\\[\n");
        ctx.push(math_content);
        ctx.push("\n\\]");
    } else {
        ctx.push("$");
        ctx.push(math_content);
        ctx.push("$");
    }
    ctx.last_token = TokenType::Command;
}

pub fn convert_content_nodes_to_latex(nodes: &[ContentNode], ctx: &mut ConvertContext) {
    let mut buffer = String::new();

    for node in nodes {
        match node {
            ContentNode::Space => {
                flush_typst_chunk(&mut buffer, ctx);
                if !ctx.output.is_empty()
                    && !ctx.output.ends_with(' ')
                    && !ctx.output.ends_with('\n')
                {
                    ctx.push(" ");
                }
            }
            ContentNode::Parbreak => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.ensure_paragraph_break();
                ctx.last_token = TokenType::Newline;
            }
            ContentNode::Linebreak => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.push("\\\n");
                ctx.last_token = TokenType::Newline;
            }
            ContentNode::Citation {
                keys,
                mode,
                supplement,
            } => {
                flush_typst_chunk(&mut buffer, ctx);
                let mut group = CiteGroup::new();
                group.suffix = supplement.clone();
                for key in keys {
                    group.push(Citation::with_mode(key.clone(), *mode));
                }
                ctx.push(&citation_to_latex(&group));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::Reference { target, ref_type } => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.push(&reference_to_latex(&Reference {
                    target: target.clone(),
                    ref_type: *ref_type,
                }));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::LabelDef(label) => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.push(&label_to_latex(label));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::Bibliography { file, style } => {
                flush_typst_chunk(&mut buffer, ctx);
                let bib_name = file
                    .trim_end_matches(".yml")
                    .trim_end_matches(".yaml")
                    .trim_end_matches(".bib");
                ctx.ensure_paragraph_break();
                ctx.push_line(&format!(
                    r"\bibliographystyle{{{}}}",
                    style.clone().unwrap_or_else(|| "plain".to_string())
                ));
                ctx.push_line(&format!(r"\bibliography{{{}}}", bib_name));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::Math { segments, block } => {
                flush_typst_chunk(&mut buffer, ctx);
                let math_source = render_math_segments_to_typst_source(segments);
                let root = parse_math(&math_source);
                let mut math_ctx = ConvertContext::new();
                math_ctx.options = ctx.options.clone();
                math_ctx.in_math = true;
                convert_math_node(&root, &mut math_ctx);
                let math_content = math_ctx.finalize().trim().to_string();
                emit_rendered_math(ctx, &math_content, *block);
            }
            other => buffer.push_str(&other.to_typst()),
        }
    }

    flush_typst_chunk(&mut buffer, ctx);
}

/// Check if a language is supported by the listings package
fn is_listings_supported(lang: &str) -> bool {
    let lang_lower = lang.to_lowercase();
    LISTINGS_SUPPORTED_LANGUAGES
        .iter()
        .any(|&supported| supported == lang_lower)
}

/// Check if math content has alignment markers (&) that are not inside cases/matrix environments
/// These need to be wrapped in an align environment
fn has_unescaped_alignment(content: &str) -> bool {
    // Count nesting level of cases/matrix environments
    let mut depth = 0;
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Check for \begin or \end
            let rest: String = chars.clone().take(5).collect();
            if rest.starts_with("begin") {
                depth += 1;
            } else if rest.starts_with("end") && depth > 0 {
                depth -= 1;
            }
        } else if ch == '&' && depth == 0 {
            // Found an alignment marker outside of any environment
            return true;
        }
    }

    false
}

/// Convert a markup node to LaTeX
pub fn convert_markup_node(node: &SyntaxNode, ctx: &mut ConvertContext) {
    match node.kind() {
        SyntaxKind::Markup => {
            // Process children, but group consecutive list items into list environments
            let children: Vec<_> = node.children().collect();
            let mut i = 0;
            while i < children.len() {
                let child = children[i];
                match child.kind() {
                    SyntaxKind::ListItem => {
                        // Start an itemize environment for consecutive list items
                        ctx.ensure_paragraph_break();
                        ctx.push_line("\\begin{itemize}");
                        ctx.indent_level += 1;

                        // Collect all consecutive ListItems
                        while i < children.len()
                            && (children[i].kind() == SyntaxKind::ListItem
                                || children[i].kind() == SyntaxKind::Space
                                || children[i].kind() == SyntaxKind::Parbreak)
                        {
                            if children[i].kind() == SyntaxKind::ListItem {
                                convert_list_item(children[i], ctx);
                            }
                            i += 1;
                        }

                        ctx.indent_level -= 1;
                        ctx.push_line("\\end{itemize}");
                    }
                    SyntaxKind::EnumItem => {
                        // Start an enumerate environment for consecutive enum items
                        ctx.ensure_paragraph_break();
                        ctx.push_line("\\begin{enumerate}");
                        ctx.indent_level += 1;

                        // Collect all consecutive EnumItems
                        while i < children.len()
                            && (children[i].kind() == SyntaxKind::EnumItem
                                || children[i].kind() == SyntaxKind::Space
                                || children[i].kind() == SyntaxKind::Parbreak)
                        {
                            if children[i].kind() == SyntaxKind::EnumItem {
                                convert_enum_item(children[i], ctx);
                            }
                            i += 1;
                        }

                        ctx.indent_level -= 1;
                        ctx.push_line("\\end{enumerate}");
                    }
                    SyntaxKind::FuncCall => {
                        // Check if this is a figure/table that has a label following it
                        let func_name = child
                            .children()
                            .next()
                            .map(|n| n.text().to_string())
                            .unwrap_or_default();

                        if func_name == "figure" {
                            // Look ahead for a Label node (skip Space nodes)
                            let mut label_text: Option<String> = None;
                            let mut label_idx: Option<usize> = None;

                            for (j, sibling) in children.iter().enumerate().skip(i + 1) {
                                match sibling.kind() {
                                    SyntaxKind::Space | SyntaxKind::Linebreak => continue,
                                    SyntaxKind::Label => {
                                        let text = sibling.text().to_string();
                                        label_text = Some(
                                            text.trim_start_matches('<')
                                                .trim_end_matches('>')
                                                .to_string(),
                                        );
                                        label_idx = Some(j);
                                        break;
                                    }
                                    _ => break, // Not a label, stop looking
                                }
                            }

                            // Set the pending label in context
                            ctx.pending_label = label_text;
                            convert_markup_node(child, ctx);
                            ctx.pending_label = None;

                            // Skip the label node if we found one
                            if let Some(j) = label_idx {
                                i = j + 1;
                            } else {
                                i += 1;
                            }
                        } else {
                            convert_markup_node(child, ctx);
                            i += 1;
                        }
                    }
                    _ => {
                        convert_markup_node(child, ctx);
                        i += 1;
                    }
                }
            }
        }

        SyntaxKind::Text => {
            let text = node.text().to_string();
            // Strip Typst code block braces if they wrap the entire text
            let cleaned = if text.starts_with('{') && text.ends_with('}') && text.len() > 2 {
                &text[1..text.len() - 1]
            } else {
                &text
            };
            if !cleaned.trim().is_empty() {
                ctx.push(&escape_latex_text(cleaned));
                ctx.last_token = TokenType::Text;
            } else if !cleaned.is_empty() && ctx.last_token != TokenType::Newline {
                ctx.push(" ");
            }
        }

        SyntaxKind::Space => {
            if ctx.last_token != TokenType::Newline && !ctx.output.ends_with(' ') {
                ctx.push(" ");
            }
        }

        // Escape sequences: \$, \#, \%, etc.
        SyntaxKind::Escape => {
            let text = node.text().to_string();
            // Remove the leading backslash and escape for LaTeX
            let escaped_char = text.trim_start_matches('\\');
            // Map Typst escape to LaTeX escape
            let latex = match escaped_char {
                "$" => "\\$",
                "#" => "\\#",
                "%" => "\\%",
                "&" => "\\&",
                "_" => "\\_",
                "{" => "\\{",
                "}" => "\\}",
                "\\" => "\\textbackslash{}",
                "~" => "\\textasciitilde{}",
                "^" => "\\textasciicircum{}",
                "*" => "*",
                "`" => "`",
                _ => escaped_char,
            };
            ctx.push(latex);
            ctx.last_token = TokenType::Text;
        }

        SyntaxKind::Parbreak => {
            ctx.ensure_paragraph_break();
        }

        SyntaxKind::Linebreak => {
            // In table cells, use \newline instead of \\ to avoid LR mode errors
            if ctx.is_in_env(&EnvironmentContext::Table) {
                ctx.push("\\newline ");
            } else {
                ctx.push("\\\\\n");
            }
            ctx.last_token = TokenType::Newline;
        }

        // Headings
        SyntaxKind::Heading => {
            let level = count_heading_markers(node);
            let section_cmd = get_heading_command(level);

            ctx.ensure_paragraph_break();
            ctx.push(section_cmd);
            ctx.push("{");

            // Get heading content
            for child in node.children() {
                if child.kind() != SyntaxKind::HeadingMarker {
                    convert_markup_node(child, ctx);
                }
            }

            ctx.push("}\n");
            ctx.last_token = TokenType::Newline;
        }

        SyntaxKind::HeadingMarker => {
            // Skip, handled by Heading
        }

        // Strong (bold)
        SyntaxKind::Strong => {
            ctx.push("\\textbf{");
            for child in node.children() {
                if child.kind() != SyntaxKind::Star {
                    convert_markup_node(child, ctx);
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // Emphasis (italic)
        SyntaxKind::Emph => {
            ctx.push("\\textit{");
            for child in node.children() {
                if child.kind() != SyntaxKind::Underscore {
                    convert_markup_node(child, ctx);
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // Raw/Code
        SyntaxKind::Raw => {
            let (text, lang) = get_raw_text_with_lang(node);
            // Determine if this is a block code or inline code
            // Block code: starts with ```, or contains newlines, or has language tag
            let raw_text = node.text().to_string();
            let is_block = raw_text.starts_with("```") || text.contains('\n') || lang.is_some();

            if is_block {
                ctx.ensure_paragraph_break();
                // Only use lstlisting if the language is supported, otherwise use verbatim
                if let Some(ref language) = lang {
                    if is_listings_supported(language) {
                        ctx.push_line(&format!("\\begin{{lstlisting}}[language={}]", language));
                        ctx.push(&text);
                        ctx.newline();
                        ctx.push_line("\\end{lstlisting}");
                    } else {
                        // Unsupported language - use verbatim with a comment
                        ctx.push_line(&format!("% Code block (language: {})", language));
                        ctx.push_line("\\begin{verbatim}");
                        ctx.push(&text);
                        ctx.newline();
                        ctx.push_line("\\end{verbatim}");
                    }
                } else {
                    ctx.push_line("\\begin{verbatim}");
                    ctx.push(&text);
                    ctx.newline();
                    ctx.push_line("\\end{verbatim}");
                }
            } else {
                ctx.push("\\texttt{");
                ctx.push(&escape_latex_text(&text));
                ctx.push("}");
            }
            ctx.last_token = TokenType::Command;
        }

        // Links
        SyntaxKind::Link => {
            let url = node.text().to_string();
            ctx.push("\\url{");
            ctx.push(&url);
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // Lists - these are now handled by the Markup case above
        // If we encounter them directly, we're likely in a nested context
        SyntaxKind::ListItem => {
            // Check if we're already in an itemize environment
            if ctx.in_list() {
                convert_list_item(node, ctx);
            } else {
                // Standalone list item - wrap it in an environment
                ctx.push_line("\\begin{itemize}");
                convert_list_item(node, ctx);
                ctx.push_line("\\end{itemize}");
            }
        }

        SyntaxKind::EnumItem => {
            // Check if we're already in an enumerate environment
            if ctx.in_list() {
                convert_enum_item(node, ctx);
            } else {
                // Standalone enum item - wrap it in an environment
                ctx.push_line("\\begin{enumerate}");
                convert_enum_item(node, ctx);
                ctx.push_line("\\end{enumerate}");
            }
        }

        // Math (inline or display)
        SyntaxKind::Equation => {
            let in_table = ctx.is_in_env(&EnvironmentContext::Table);
            let is_block = !in_table && is_display_math(node);

            // Convert math content to a temporary buffer first
            let mut math_ctx = ConvertContext::new();
            math_ctx.in_math = true;
            for child in node.children() {
                if child.kind() == SyntaxKind::Math {
                    convert_math_node(child, &mut math_ctx);
                }
            }
            let math_content = math_ctx.finalize().trim().to_string();

            emit_rendered_math(ctx, &math_content, is_block);
        }

        SyntaxKind::Math => {
            // Delegate to math converter
            convert_math_node(node, ctx);
        }

        // Function calls (like #image, #table, etc.)
        SyntaxKind::FuncCall => {
            convert_func_call_markup(node, ctx);
        }

        // Content blocks
        SyntaxKind::ContentBlock => {
            for child in node.children() {
                convert_markup_node(child, ctx);
            }
        }

        // Code mode - process content, skip braces
        SyntaxKind::Code => {
            for child in node.children() {
                convert_markup_node(child, ctx);
            }
        }

        // Code block {expr} - skip braces, process inner content
        SyntaxKind::CodeBlock => {
            for child in node.children() {
                // Skip the braces themselves
                if child.kind() != SyntaxKind::LeftBrace && child.kind() != SyntaxKind::RightBrace {
                    convert_markup_node(child, ctx);
                }
            }
        }

        // Ignore set/show rules in markup (to avoid outputting them as text)
        SyntaxKind::SetRule | SyntaxKind::ShowRule => {
            // Do nothing
        }

        // Handle identifiers that might be content
        SyntaxKind::Ident => {
            let text = node.text().to_string();
            // Ignore common keywords that might appear as identifiers in some contexts
            if !matches!(text.as_str(), "set" | "show" | "let") {
                ctx.push(&text);
                ctx.last_token = TokenType::Letter;
            }
        }

        // References: @label -> \ref{label}
        SyntaxKind::Ref => {
            let text = get_simple_text(node);
            let label = text.trim_start_matches('@').trim();
            if !label.is_empty() {
                ctx.push(&reference_to_latex(&Reference::new(label.to_string())));
                ctx.last_token = TokenType::Command;
            }
        }

        // Labels: <label> -> \label{label}
        SyntaxKind::Label => {
            let text = node.text().to_string();
            let label = normalize_label_like_text(&text);
            if !label.is_empty() {
                ctx.push(&label_to_latex(&label));
                ctx.last_token = TokenType::Command;
            }
        }

        // Primitive types - output their text representation
        // These may appear when MiniEval evaluates expressions to primitives
        SyntaxKind::Int => {
            let text = node.text().to_string();
            ctx.push(&text);
            ctx.last_token = TokenType::Number;
        }

        SyntaxKind::Float => {
            let text = node.text().to_string();
            ctx.push(&text);
            ctx.last_token = TokenType::Number;
        }

        SyntaxKind::Numeric => {
            // Numeric includes lengths like "10pt", "2em", etc.
            let text = node.text().to_string();
            ctx.push(&text);
            ctx.last_token = TokenType::Number;
        }

        SyntaxKind::Str => {
            // String literals - output without quotes (content only)
            let text = node.text().to_string();
            // Remove surrounding quotes if present
            let content = text
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'');
            ctx.push(&escape_latex_text(content));
            ctx.last_token = TokenType::Text;
        }

        SyntaxKind::Bool => {
            // Boolean values
            let text = node.text().to_string();
            ctx.push(&text);
            ctx.last_token = TokenType::Letter;
        }

        SyntaxKind::None => {
            // None value - don't output anything
        }

        _ => {
            // Recursively process children
            let child_count = node.children().count();
            if child_count > 0 {
                for child in node.children() {
                    convert_markup_node(child, ctx);
                }
            }
        }
    }
}

/// Convert Typst function calls to LaTeX (in markup mode)
pub fn convert_func_call_markup(node: &SyntaxNode, ctx: &mut ConvertContext) {
    let children: Vec<_> = node.children().collect();
    if children.is_empty() {
        return;
    }

    let func_name = children[0].text().to_string();

    // Check if this is a math function that needs $ wrapping
    if is_math_func_in_markup(&func_name) {
        ctx.in_math = true;
        ctx.push("$");
        super::math::convert_func_call(node, ctx);
        ctx.push("$");
        ctx.in_math = false;
        ctx.last_token = TokenType::Command;
        return;
    }

    // Try to use the handler map
    if let Some(handler) = TYPST_MARKUP_HANDLERS.get(func_name.as_str()) {
        match handler {
            MarkupHandler::Wrap { prefix, suffix } => {
                ctx.push(prefix);
                convert_func_args_text(&children, ctx);
                ctx.push(suffix);
            }
            MarkupHandler::Environment { name } => {
                ctx.ensure_paragraph_break();
                ctx.push_line(&format!("\\begin{{{}}}", name));
                ctx.indent_level += 1;
                convert_func_args_text(&children, ctx);
                ctx.indent_level -= 1;
                ctx.push_line(&format!("\\end{{{}}}", name));
            }
            MarkupHandler::PassThrough => {
                convert_func_args_text(&children, ctx);
            }
            MarkupHandler::Special => {
                // Handle special cases
                handle_special_markup_func(&func_name, &children, ctx);
            }
        }
    } else {
        // Unknown function, try to output content
        convert_func_args_text(&children, ctx);
    }

    ctx.last_token = TokenType::Command;
}

/// Handle special markup functions that need custom logic
fn handle_special_markup_func(func_name: &str, children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    match func_name {
        "image" => {
            convert_image_to_latex(children, ctx);
        }

        "table" => {
            convert_table_to_latex(children, ctx);
        }

        "figure" => {
            convert_figure_to_latex(children, ctx);
        }

        "link" => {
            convert_link_to_latex(children, ctx);
        }

        "cite" => {
            convert_cite_to_latex(children, ctx);
        }

        "ref" => {
            convert_ref_to_latex(children, ctx);
        }

        "label" => {
            convert_label_to_latex(children, ctx);
        }

        "bibliography" => {
            convert_bibliography_to_latex(children, ctx);
        }

        "footnote" => {
            ctx.push("\\footnote{");
            convert_func_args_text(children, ctx);
            ctx.push("}");
        }

        "caption" => {
            ctx.push("\\caption{");
            convert_func_args_text(children, ctx);
            ctx.push("}");
        }

        // Theorem-like environments
        "theorem" | "lemma" | "proposition" | "corollary" | "definition" | "example" | "remark"
        | "proof" => {
            convert_theorem_to_latex(func_name, children, ctx);
        }

        // Block quote with attribution
        "blockquote" => {
            ctx.ensure_paragraph_break();
            ctx.push_line("\\begin{quote}");
            convert_func_args_text(children, ctx);
            ctx.push_line("\\end{quote}");
        }

        // Text formatting
        "text" => {
            convert_text_func(children, ctx);
        }

        // Layout
        "pad" | "block" => {
            // For now, just output content without the box/padding wrapper to avoid "width inset" text
            // In the future, this could map to \parbox or minipage
            convert_func_args_text(children, ctx);
        }

        // Rotation
        "rotate" => {
            convert_rotate_func(children, ctx);
        }

        // Alignment
        "center" | "align" => {
            // Check arguments to see if it's align(center, ...) or just center(...)
            let mut alignment = "center";

            if func_name == "align" {
                if let Some(args) = children.get(1) {
                    let arg_children: Vec<_> = args.children().collect();
                    for child in arg_children.iter() {
                        if child.kind() == SyntaxKind::Ident {
                            let text = child.text().to_string();
                            if text == "left" || text == "start" {
                                alignment = "flushleft";
                            } else if text == "right" || text == "end" {
                                alignment = "flushright";
                            } else if text == "center" {
                                alignment = "center";
                            }
                        }
                    }
                }
            }

            ctx.ensure_paragraph_break();
            ctx.push_line(&format!("\\begin{{{}}}", alignment));

            // Custom args processing to skip the alignment parameter
            if let Some(args) = children.get(1) {
                let arg_children: Vec<_> = args.children().collect();
                for child in arg_children.iter() {
                    // Better approach: filter out the alignment keyword if we found one
                    let is_alignment_keyword = child.kind() == SyntaxKind::Ident
                        && (child.text() == "center"
                            || child.text() == "left"
                            || child.text() == "right"
                            || child.text() == "start"
                            || child.text() == "end");

                    if func_name == "align" && is_alignment_keyword {
                        continue;
                    }

                    if child.kind() != SyntaxKind::Comma
                        && child.kind() != SyntaxKind::Colon
                        && child.kind() != SyntaxKind::LeftParen
                        && child.kind() != SyntaxKind::RightParen
                    {
                        convert_markup_node(child, ctx);
                    }
                }
            }

            ctx.push_line(&format!("\\end{{{}}}", alignment));
        }

        // Box/Frame
        "box" => {
            ctx.push("\\fbox{");
            convert_func_args_text(children, ctx);
            ctx.push("}");
        }

        // Rectangle
        "rect" => {
            convert_rect_func(children, ctx);
        }

        // Columns
        "columns" | "grid" => {
            convert_grid_to_latex(children, ctx);
        }

        // CeTZ graphics (canvas)
        "canvas" => {
            convert_cetz_to_latex(children, ctx);
        }

        // Lists
        "list" => {
            convert_list_to_latex(children, ctx, false);
        }

        "enum" => {
            convert_list_to_latex(children, ctx, true);
        }

        // Raw/verbatim blocks
        "raw" => {
            convert_raw_to_latex(children, ctx);
        }

        // Horizontal spacing: h(1fr) -> \hfill, h(1em) -> \hspace{1em}
        "h" => {
            if let Some(args) = children.get(1) {
                let arg_text = args.text().to_string();
                match parse_spacing_spec(&arg_text) {
                    Some(SpacingSpec::Flex(_)) => ctx.push("\\hfill"),
                    Some(SpacingSpec::Fixed(value)) => {
                        ctx.push(&format!("\\hspace{{{}}}", value));
                    }
                    None => ctx.push("\\hfill"),
                }
            }
        }

        // Vertical spacing: v(1em) -> \vspace{1em}
        "v" => {
            if let Some(args) = children.get(1) {
                let arg_text = args.text().to_string();
                if arg_text.contains("fr") {
                    ctx.push("\\vfill");
                } else if let Some(value) = extract_length_value(&arg_text) {
                    ctx.push(&format!("\\vspace{{{}}}", value));
                } else {
                    ctx.push("\\vspace{1em}");
                }
            }
        }

        _ => {
            // Fallback: just output content
            convert_func_args_text(children, ctx);
        }
    }
}

// ============================================================================
// Text Function Conversion
// ============================================================================

fn convert_text_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let weight = args.named_text("weight").map(str::to_string);
    let size = args.named_text("size").map(str::to_string);
    let style = args.named_text("style").map(str::to_string);
    let mut color = args
        .named_color("fill")
        .and_then(normalize_typst_color_expr);
    let mut content_nodes = Vec::new();

    for arg in args.iter().filter(|arg| arg.is_positional) {
        match arg.node.kind() {
            SyntaxKind::ContentBlock | SyntaxKind::Markup | SyntaxKind::Str => {
                content_nodes.push(arg.node);
            }
            SyntaxKind::Ident => {
                if let Some(normalized) = normalize_typst_color_expr(&arg.value) {
                    color = Some(normalized);
                }
            }
            SyntaxKind::FuncCall => {
                if let Some(normalized) = normalize_typst_color_expr(&arg.value) {
                    color = Some(normalized);
                } else {
                    content_nodes.push(arg.node);
                }
            }
            _ => content_nodes.push(arg.node),
        }
    }

    // Apply styling wrappers
    let mut suffix_count = 0;

    // Apply color first (outermost wrapper)
    if let Some(c) = &color {
        ctx.push(&format_latex_color_command("textcolor", c));
        ctx.push("{");
        suffix_count += 1;
    }

    if let Some(w) = weight {
        if w == "\"bold\"" || w == "bold" || w == "extrabold" || w == "black" {
            ctx.push("\\textbf{");
            suffix_count += 1;
        }
    }

    if let Some(s) = style {
        if s == "\"italic\"" || s == "italic" {
            ctx.push("\\textit{");
            suffix_count += 1;
        }
    }

    if let Some(s) = size {
        // Simple heuristic for size mapping
        if s.contains("pt") {
            if let Ok(pt) = s.trim_end_matches("pt").trim().parse::<f64>() {
                if pt >= 20.0 {
                    ctx.push("{\\Huge ");
                } else if pt >= 17.0 {
                    ctx.push("{\\huge ");
                } else if pt >= 14.0 {
                    ctx.push("{\\Large ");
                } else if pt >= 12.0 {
                    ctx.push("{\\large ");
                } else if pt <= 8.0 {
                    ctx.push("{\\small ");
                } else {
                    ctx.push("{");
                }
                suffix_count += 1;
            }
        }
    }

    // Output content
    if content_nodes.is_empty() {
        // Might be just setting style without content, or content is in a later block
        // For now, do nothing
    } else {
        for node in content_nodes {
            convert_markup_node(node, ctx);
        }
    }

    // Close wrappers
    for _ in 0..suffix_count {
        ctx.push("}");
    }
}

/// Convert #rotate(angle)[content] to \rotatebox{angle}{content}
fn convert_rotate_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let mut angle = args.named_angle("angle");
    let mut content_nodes = Vec::new();

    for arg in args.iter().filter(|arg| arg.is_positional) {
        match arg.node.kind() {
            SyntaxKind::ContentBlock | SyntaxKind::Markup => content_nodes.push(arg.node),
            SyntaxKind::Unary => {
                angle = parse_angle_value(&arg.value);
            }
            _ => {
                if arg.value.contains("deg")
                    || arg.value.contains("rad")
                    || arg.value.parse::<f64>().is_ok()
                {
                    angle = parse_angle_value(&arg.value);
                }
            }
        }
    }

    // Output \rotatebox{angle}{content}
    let angle_deg = angle.unwrap_or(0.0);
    ctx.push(&format!("\\rotatebox{{{}}}", angle_deg));
    ctx.push("{");

    for node in content_nodes {
        convert_markup_node(node, ctx);
    }

    ctx.push("}");
}

/// Convert #rect(...)[content] to appropriate LaTeX
/// - If has content with fill: use \colorbox (preserves content)
/// - If no content with fill and height: use \rule (solid rectangle)
/// - Otherwise, use \fbox
fn convert_rect_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);

    let width = args.named("width");
    let height = args.named("height");
    let fill = args.named("fill").and_then(normalize_typst_color_expr);

    // Get content nodes
    let mut content_nodes = Vec::new();
    if let Some(args_node) = children.get(1) {
        for child in args_node.children() {
            if matches!(child.kind(), SyntaxKind::ContentBlock | SyntaxKind::Markup) {
                content_nodes.push(child);
            }
        }
    }

    let has_content = !content_nodes.is_empty();

    // Determine the best LaTeX representation
    if let Some(f) = fill.as_deref() {
        let colorbox = format_latex_color_command("colorbox", f);
        let color_cmd = format_latex_color_command("color", f);

        if has_content {
            // Has content - must use \colorbox to preserve it
            // Optionally wrap in minipage if width specified
            if let Some(w) = width {
                let latex_width = convert_dimension_to_latex(w);
                ctx.push(&colorbox);
                ctx.push(&format!("{{\\begin{{minipage}}{{{}}}", latex_width));
                for node in &content_nodes {
                    convert_markup_node(node, ctx);
                }
                ctx.push("\\end{minipage}}");
            } else {
                ctx.push(&colorbox);
                ctx.push("{");
                for node in &content_nodes {
                    convert_markup_node(node, ctx);
                }
                ctx.push("}");
            }
        } else if let Some(h) = height {
            // No content, has height - use \rule for solid rectangle
            let latex_height = convert_dimension_to_latex(h);
            let latex_width = width
                .map(convert_dimension_to_latex)
                .unwrap_or_else(|| "\\linewidth".to_string());
            ctx.push(&format!(
                "{{{}\\rule{{{}}}{{{}}}}}",
                color_cmd, latex_width, latex_height
            ));
        } else {
            // Has fill but no height and no content - just colorbox with empty
            ctx.push(&colorbox);
            ctx.push("{}");
        }
    } else if let Some(h) = height {
        // No fill, but has height - black rule
        let latex_height = convert_dimension_to_latex(h);
        let latex_width = width
            .map(convert_dimension_to_latex)
            .unwrap_or_else(|| "\\linewidth".to_string());
        ctx.push(&format!("\\rule{{{}}}{{{}}}", latex_width, latex_height));
    } else {
        // Default - use \fbox
        ctx.push("\\fbox{");
        for node in content_nodes {
            convert_markup_node(node, ctx);
        }
        ctx.push("}");
    }
}

fn convert_image_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    // Use FuncArgs for unified argument parsing
    let args = FuncArgs::from_func_call(children);

    // Get path from first positional argument
    let path = args.first().unwrap_or("").trim_matches('"').to_string();

    // Get optional width and height
    let width = args.named("width").map(convert_dimension_to_latex);
    let height = args.named("height").map(convert_dimension_to_latex);

    // Build \includegraphics command
    ctx.push("\\includegraphics");

    // Add options if present
    let mut options = Vec::new();
    if let Some(w) = width {
        options.push(format!("width={}", w));
    }
    if let Some(h) = height {
        options.push(format!("height={}", h));
    }

    if !options.is_empty() {
        ctx.push("[");
        ctx.push(&options.join(", "));
        ctx.push("]");
    }

    ctx.push("{");
    ctx.push(&path);
    ctx.push("}");
}

/// Convert Typst dimension to LaTeX dimension
fn convert_dimension_to_latex(dim: &str) -> String {
    let dim = dim.trim();
    if dim.ends_with('%') {
        // Convert percentage to \textwidth
        let percent = dim.trim_end_matches('%');
        if let Ok(p) = percent.parse::<f64>() {
            return format!("{:.2}\\textwidth", p / 100.0);
        }
    }
    // Pass through other dimensions (pt, cm, mm, in, em)
    dim.to_string()
}

// ============================================================================
// Enhanced Table Conversion
// ============================================================================

/// Get the full function name from a FuncCall's first child, handling FieldAccess
fn get_func_call_name(node: &SyntaxNode) -> String {
    match node.kind() {
        SyntaxKind::FieldAccess => {
            // Collect parts from FieldAccess: table.header -> "table.header"
            let mut parts = Vec::new();
            for child in node.children() {
                match child.kind() {
                    SyntaxKind::FieldAccess => {
                        parts.push(get_func_call_name(child));
                    }
                    SyntaxKind::Ident | SyntaxKind::MathIdent => {
                        parts.push(child.text().to_string());
                    }
                    SyntaxKind::Dot => {
                        // Skip dots, we'll join with them
                    }
                    _ => {
                        let text = child.text().to_string();
                        if !text.is_empty() && text != "." {
                            parts.push(text);
                        }
                    }
                }
            }
            parts.join(".")
        }
        _ => node.text().to_string(),
    }
}

/// Convert a Typst table to LaTeX using the state-aware table generator
fn convert_table_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let mut columns: usize = 0;
    let mut col_aligns: Vec<LatexCellAlign> = Vec::new();
    let mut cells: Vec<LatexCell> = Vec::new();
    let mut hlines: Vec<(usize, LatexHLine)> = Vec::new(); // (cell_index, hline)
    let mut in_header = false;
    let mut header_end_idx: Option<usize> = None;

    if let Some(args_node) = children.get(1) {
        for child in args_node.children() {
            match child.kind() {
                SyntaxKind::Named => {
                    if let Some(arg) = args.arg_for_node(child) {
                        match arg.name.as_deref() {
                            Some("columns") => {
                                if let Some(n) = infer_table_columns(&arg.value) {
                                    columns = n;
                                }
                                if columns == 0 {
                                    let auto_count = arg.value.matches("auto").count();
                                    if auto_count > 0 {
                                        columns = auto_count;
                                    }
                                }
                            }
                            Some("align") => {
                                col_aligns = parse_typst_align(&arg.value);
                            }
                            _ => {}
                        }
                    }
                }
                SyntaxKind::ContentBlock | SyntaxKind::Markup | SyntaxKind::Equation => {
                    let mut cell_ctx = ConvertContext::new();
                    cell_ctx.push_env(EnvironmentContext::Table);
                    convert_markup_node(child, &mut cell_ctx);
                    let content = cell_ctx.finalize();
                    if !content.is_empty() {
                        let mut cell = LatexCell::new(content);
                        cell.is_header = in_header;
                        cells.push(cell);
                    }
                }
                SyntaxKind::FuncCall => {
                    let func_children: Vec<_> = child.children().collect();
                    if !func_children.is_empty() {
                        let func_name = get_func_call_name(func_children[0]);

                        if func_name.contains("header") {
                            // Process header cells - can contain table.cell() or plain content
                            if let Some(func_args) = func_children.get(1) {
                                for header_child in func_args.children() {
                                    match header_child.kind() {
                                        SyntaxKind::ContentBlock
                                        | SyntaxKind::Markup
                                        | SyntaxKind::Equation => {
                                            // Plain content like [Type A] or $x$
                                            let mut cell_ctx = ConvertContext::new();
                                            cell_ctx.push_env(EnvironmentContext::Table);
                                            convert_markup_node(header_child, &mut cell_ctx);
                                            let content = cell_ctx.finalize();
                                            if !content.is_empty() {
                                                let mut cell = LatexCell::new(content);
                                                cell.is_header = true;
                                                cells.push(cell);
                                            }
                                        }
                                        SyntaxKind::FuncCall => {
                                            // table.cell(...) inside header
                                            let inner_func_children: Vec<_> =
                                                header_child.children().collect();
                                            if !inner_func_children.is_empty() {
                                                let inner_func_name =
                                                    get_func_call_name(inner_func_children[0]);
                                                if inner_func_name.contains("cell") {
                                                    let mut cell = LatexCell::from_typst_cell_ast(
                                                        header_child,
                                                        ctx,
                                                    );
                                                    cell.is_header = true;
                                                    cells.push(cell);
                                                } else {
                                                    // Other function call (e.g., text(...))
                                                    let mut cell_ctx = ConvertContext::new();
                                                    cell_ctx.push_env(EnvironmentContext::Table);
                                                    convert_markup_node(
                                                        header_child,
                                                        &mut cell_ctx,
                                                    );
                                                    let content = cell_ctx.finalize();
                                                    if !content.is_empty() {
                                                        let mut cell = LatexCell::new(content);
                                                        cell.is_header = true;
                                                        cells.push(cell);
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            header_end_idx = Some(cells.len());
                            in_header = false;
                        } else if func_name.contains("cell") {
                            let cell = LatexCell::from_typst_cell_ast(child, ctx);
                            cells.push(cell);
                        } else if func_name.contains("hline") {
                            let hline = LatexHLine::from_typst_ast(child);
                            hlines.push((cells.len(), hline));
                        } else if func_name == "align" {
                            // Special case: align() inside table should extract alignment property
                            // instead of creating \begin{center} environment
                            if let Some(args) =
                                child.children().find(|c| c.kind() == SyntaxKind::Args)
                            {
                                // Default alignment
                                let mut alignment = LatexCellAlign::Center;
                                let mut content_node = None;

                                for arg in args.children() {
                                    if arg.kind() == SyntaxKind::ContentBlock {
                                        content_node = Some(arg.clone());
                                    } else if arg.kind() != SyntaxKind::Comma
                                        && arg.kind() != SyntaxKind::LeftParen
                                        && arg.kind() != SyntaxKind::RightParen
                                    {
                                        // This is likely the alignment argument
                                        // Simplified check for "left", "right" in the text
                                        let text = arg.text().to_string();
                                        if text.contains("left") || text.contains("start") {
                                            alignment = LatexCellAlign::Left;
                                        } else if text.contains("right") || text.contains("end") {
                                            alignment = LatexCellAlign::Right;
                                        } else if text.contains("center") {
                                            alignment = LatexCellAlign::Center;
                                        }
                                    }
                                }

                                if let Some(content_arg) = content_node {
                                    let mut cell_ctx = ConvertContext::new();
                                    cell_ctx.push_env(EnvironmentContext::Table);
                                    convert_markup_node(&content_arg, &mut cell_ctx);
                                    let content = cell_ctx.finalize();

                                    let mut cell = LatexCell::new(content);
                                    cell.align = Some(alignment);
                                    cells.push(cell);
                                }
                            }
                        } else {
                            // Regular cell (other function call)
                            let mut cell_ctx = ConvertContext::new();
                            cell_ctx.push_env(EnvironmentContext::Table);
                            convert_markup_node(child, &mut cell_ctx);
                            let content = cell_ctx.finalize();
                            if !content.is_empty() {
                                cells.push(LatexCell::new(content));
                            }
                        }
                    }
                }
                // Handle string arguments as table cells (from MiniEval expansion)
                SyntaxKind::Str => {
                    let text = child.text().to_string();
                    // Remove quotes from string literal
                    let content = text.trim_matches('"').to_string();
                    if !content.is_empty() {
                        cells.push(LatexCell::new(content));
                    }
                }
                // Handle other expression types as table cells
                SyntaxKind::Int | SyntaxKind::Float | SyntaxKind::Bool => {
                    let content = child.text().to_string();
                    if !content.is_empty() {
                        cells.push(LatexCell::new(content));
                    }
                }
                _ => {
                    // Ignore punctuation / structural tokens
                }
            }
        }
    }

    // Infer columns if not specified
    if columns == 0 {
        columns = if cells.len() >= 4 {
            (cells.len() as f64).sqrt().ceil() as usize
        } else {
            cells.len().max(1)
        };
    }

    // Pad col_aligns to match column count
    while col_aligns.len() < columns {
        col_aligns.push(LatexCellAlign::Center);
    }

    // Create the table generator
    let mut generator = LatexTableGenerator::new(columns, col_aligns);

    // Process cells row by row
    let mut current_row: Vec<LatexCell> = Vec::new();
    let mut cell_idx = 0;
    let mut col_in_row = 0;

    for cell in cells {
        // Check if there's an hline before this cell
        for (hline_idx, hline) in &hlines {
            if *hline_idx == cell_idx {
                generator.add_hline(hline.clone());
            }
        }

        // Check if this starts the header
        if cell_idx == 0 && cell.is_header {
            generator.begin_header();
        }

        current_row.push(cell.clone());
        col_in_row += cell.colspan;

        // Calculate effective columns needed for this row
        // (accounting for columns covered by previous rowspans)
        let covered_cols = generator.get_covered_columns();
        let effective_cols_needed = columns.saturating_sub(covered_cols);

        // If we've filled a row, process it
        if col_in_row >= effective_cols_needed {
            generator.process_row(current_row);
            current_row = Vec::new();
            col_in_row = 0;

            // Check if header ended
            if let Some(end_idx) = header_end_idx {
                if cell_idx + 1 >= end_idx {
                    generator.end_header();
                }
            }
        }

        cell_idx += 1;
    }

    // Process any remaining cells
    if !current_row.is_empty() {
        generator.process_row(current_row);
    }

    // Check for trailing hlines
    for (hline_idx, hline) in &hlines {
        if *hline_idx >= cell_idx {
            generator.add_hline(hline.clone());
        }
    }

    ctx.ensure_paragraph_break();
    ctx.push(&generator.generate_latex());
}

/// Parse Typst align specification to LaTeX column alignments
fn parse_typst_align(value: &str) -> Vec<LatexCellAlign> {
    // If it's a function (contains "=>"), we can't parse it easily, so return empty
    // The generator will fallback to default alignments
    if value.contains("=>") {
        return Vec::new();
    }

    let inner = value
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    if inner.is_empty() {
        return Vec::new();
    }

    inner
        .split(',')
        .map(|s| LatexCellAlign::from_typst(s.trim()))
        .collect()
}

/// Infer number of table columns from Typst `columns:` value.
/// Supports `columns: 3` and `columns: (auto, auto, auto)`-style specs.
fn infer_table_columns(value: &str) -> Option<usize> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }

    // Try parsing as integer first
    if let Ok(n) = v.parse::<usize>() {
        return Some(n.max(1));
    }

    // Common Typst form: (auto, auto, auto, ...) or (1fr, 1fr, 1fr)
    let inner = v
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();

    if inner.is_empty() {
        return None;
    }

    // Count commas to determine number of items
    let commas = inner.matches(',').count();
    if commas > 0 {
        return Some(commas + 1);
    }

    // Count occurrences of "auto" or "fr" as fallback
    let auto_count = inner.matches("auto").count();
    if auto_count > 0 {
        return Some(auto_count);
    }

    let fr_count = inner.matches("fr").count();
    if fr_count > 0 {
        return Some(fr_count);
    }

    // Single column spec without commas
    if !inner.is_empty() {
        return Some(1);
    }

    None
}

// ============================================================================
// Enhanced Figure Conversion
// ============================================================================

fn convert_figure_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut caption: Option<String> = None;
    let mut label: Option<String> = None;
    let mut content = String::new();
    let mut is_table = false;

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Named => {
                    let named_children: Vec<_> = child.children().collect();
                    // named_children typically: [Ident(key), Colon, Space, Content...]
                    // We want to find the key, and then process the value part
                    if !named_children.is_empty() {
                        let key = named_children[0].text().to_string();
                        match key.as_str() {
                            "caption" => {
                                // Find value node (skip key, colon, whitespace)
                                if let Some(value_node) = named_children.iter().find(|n| {
                                    n.kind() != SyntaxKind::Ident
                                        && n.kind() != SyntaxKind::Colon
                                        && n.kind() != SyntaxKind::Space
                                }) {
                                    let mut cap_ctx = ConvertContext::new();
                                    convert_markup_node(value_node, &mut cap_ctx);
                                    caption = Some(cap_ctx.finalize());
                                }
                            }
                            "label" | "supplement" => {
                                if let Some(value_node) = named_children.iter().find(|n| {
                                    n.kind() != SyntaxKind::Ident
                                        && n.kind() != SyntaxKind::Colon
                                        && n.kind() != SyntaxKind::Space
                                }) {
                                    label = Some(get_simple_text(value_node));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                SyntaxKind::FuncCall => {
                    // Check if this is a table function
                    let func_name = child
                        .children()
                        .next()
                        .map(|n| n.text().to_string())
                        .unwrap_or_default();
                    if func_name == "table" {
                        is_table = true;
                    }

                    let mut content_ctx = ConvertContext::new();
                    convert_markup_node(child, &mut content_ctx);
                    content = content_ctx.finalize();
                }
                SyntaxKind::ContentBlock => {
                    // Peek inside content block to see if it contains a table
                    for sub in child.children() {
                        if sub.kind() == SyntaxKind::FuncCall {
                            let func_name = sub
                                .children()
                                .next()
                                .map(|n| n.text().to_string())
                                .unwrap_or_default();
                            if func_name == "table" {
                                is_table = true;
                                break;
                            }
                        }
                    }

                    let mut content_ctx = ConvertContext::new();
                    convert_markup_node(child, &mut content_ctx);
                    let c = content_ctx.finalize();
                    if !c.is_empty() {
                        content = c;
                    }
                }
                _ => {}
            }
        }
    }

    ctx.ensure_paragraph_break();
    let env_name = if is_table { "table" } else { "figure" };
    ctx.push_line(&format!("\\begin{{{}}}[htbp]", env_name));
    ctx.push_line("\\centering");

    if !content.is_empty() {
        ctx.push("  ");
        ctx.push(&content);
        ctx.newline();
    }

    if let Some(cap) = caption {
        // Clean up caption: remove escaped braces from [{...}] pattern
        let clean_cap = cap
            .trim()
            .trim_start_matches("\\{")
            .trim_end_matches("\\}")
            .trim();
        ctx.push("  \\caption{");
        ctx.push(clean_cap);
        ctx.push("}\n");
    }

    // Use label from argument, or from pending_label (set by parent when processing figure + <label>)
    let final_label = label.or_else(|| ctx.pending_label.clone());
    if let Some(lbl) = final_label {
        ctx.push("  \\label{");
        ctx.push(lbl.trim_start_matches('<').trim_end_matches('>'));
        ctx.push("}\n");
    }

    ctx.push_line(&format!("\\end{{{}}}", env_name));
}

// ============================================================================
// Link Conversion
// ============================================================================

fn convert_link_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut url = String::new();
    let mut text = String::new();

    if let Some(args) = children.get(1) {
        let mut first_str = true;
        for child in args.children() {
            if is_string_or_content(child.kind()) {
                let content = get_string_content(child);
                if first_str {
                    url = content;
                    first_str = false;
                } else {
                    text = content;
                }
            } else if child.kind() == SyntaxKind::ContentBlock {
                let mut text_ctx = ConvertContext::new();
                convert_markup_node(child, &mut text_ctx);
                text = text_ctx.finalize();
            }
        }
    }

    if text.is_empty() {
        ctx.push("\\url{");
        ctx.push(&url);
        ctx.push("}");
    } else {
        ctx.push("\\href{");
        ctx.push(&url);
        ctx.push("}{");
        ctx.push(&text);
        ctx.push("}");
    }
}

// ============================================================================
// Citation and Reference Conversion
// ============================================================================

fn normalize_label_like_text(text: &str) -> String {
    text.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn normalize_citation_note_text(text: &str) -> String {
    text.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn extract_label_like_from_node(node: &SyntaxNode) -> Option<String> {
    match node.kind() {
        SyntaxKind::Label => Some(normalize_label_like_text(node.text().as_ref())),
        SyntaxKind::Str => Some(get_string_content(node)),
        _ => {
            let text = get_simple_text(node);
            let normalized = normalize_label_like_text(&text);
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        }
    }
}

fn convert_cite_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let mut group = CiteGroup::new();
    let mode = citation_mode_from_typst_form(args.named_text("form"));
    group.suffix = args
        .named_text("supplement")
        .map(normalize_citation_note_text)
        .filter(|value| !value.is_empty());

    for index in 0..args.positional_count() {
        if let Some(node) = args.positional_node(index) {
            if let Some(key) = extract_label_like_from_node(node) {
                group.push(Citation::with_mode(key, mode));
            }
        }
    }

    if group.citations.is_empty() {
        return;
    }

    ctx.push(&citation_to_latex(&group));
}

fn convert_ref_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    if let Some(node) = args.first_node() {
        if let Some(target) = extract_label_like_from_node(node) {
            ctx.push(&reference_to_latex(&Reference::new(target)));
        }
    }
}

fn convert_label_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    if let Some(node) = args.first_node() {
        if let Some(label) = extract_label_like_from_node(node) {
            ctx.push(&label_to_latex(&label));
        }
    }
}

fn convert_bibliography_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let mut bib_file = String::new();
    let style = args.named_text("style").unwrap_or("plain").to_string();

    if let Some(first_node) = args.first_node() {
        if first_node.kind() == SyntaxKind::Str {
            bib_file = get_string_content(first_node);
        }
    }

    // Remove .yml or .bib extension if present
    let bib_name = bib_file
        .trim_end_matches(".yml")
        .trim_end_matches(".yaml")
        .trim_end_matches(".bib");

    ctx.ensure_paragraph_break();
    ctx.push_line(&format!("\\bibliographystyle{{{}}}", style));
    ctx.push_line(&format!("\\bibliography{{{}}}", bib_name));
}

// ============================================================================
// Theorem Environment Conversion
// ============================================================================

fn convert_theorem_to_latex(env_name: &str, children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let latex_env = env_name;

    ctx.ensure_paragraph_break();
    ctx.push_line(&format!("\\begin{{{}}}", latex_env));
    ctx.indent_level += 1;
    convert_func_args_text(children, ctx);
    ctx.indent_level -= 1;
    ctx.push_line(&format!("\\end{{{}}}", latex_env));
}

// ============================================================================
// Grid/Columns Conversion
// ============================================================================

fn convert_grid_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let num_cols = args
        .named_text("columns")
        .and_then(|value| infer_table_columns(value).or_else(|| value.parse::<usize>().ok()))
        .unwrap_or(2);

    // Calculate column width
    let col_width = if num_cols > 0 {
        0.95 / num_cols as f64
    } else {
        0.48
    };

    ctx.ensure_paragraph_break();
    ctx.push_line(&format!(
        "\\begin{{minipage}}[t]{{{:.2}\\textwidth}}",
        col_width
    ));
    convert_func_args_text(children, ctx);
    ctx.push_line("\\end{minipage}");
}

/// Convert function arguments as text content, ignoring named argument keys
pub fn convert_func_args_text(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);

    for arg in args.iter() {
        if arg.is_positional {
            if let Some(node) = arg.value_node {
                convert_markup_node(node, ctx);
            }
            continue;
        }

        if let Some(node) = arg.value_node {
            if node.kind() == SyntaxKind::ContentBlock || node.kind() == SyntaxKind::Markup {
                convert_markup_node(node, ctx);
            }
        }
    }

    for child in children.iter().skip(1) {
        if child.kind() == SyntaxKind::ContentBlock {
            for arg_child in child.children() {
                convert_markup_node(arg_child, ctx);
            }
        }
    }
}

// ============================================================================
// CeTZ to TikZ Conversion
// ============================================================================

fn convert_cetz_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    // Extract the CeTZ code from the content block
    let mut cetz_code = String::new();

    if let Some(args) = children.get(1) {
        for child in args.children() {
            if child.kind() == SyntaxKind::ContentBlock {
                // Get the raw text content
                cetz_code = child.text().to_string();
                break;
            }
        }
    }

    if cetz_code.is_empty() {
        // Fallback: try to get all text
        for child in children.iter().skip(1) {
            cetz_code.push_str(child.text().as_ref());
        }
    }

    // Check if it looks like CeTZ code
    if is_cetz_code(&cetz_code) {
        ctx.ensure_paragraph_break();
        let tikz_code = convert_cetz_to_tikz(&cetz_code);
        ctx.push(&tikz_code);
        ctx.newline();
    } else {
        // Not CeTZ, just output as-is
        ctx.add_warning("Canvas content not recognized as CeTZ");
        convert_func_args_text(children, ctx);
    }
}

// ============================================================================
// Enhanced List Conversion with Nesting Support
// ============================================================================

fn convert_list_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext, is_enumerate: bool) {
    let env_name = if is_enumerate { "enumerate" } else { "itemize" };
    let env = if is_enumerate {
        EnvironmentContext::Enumerate
    } else {
        EnvironmentContext::Itemize
    };

    ctx.ensure_paragraph_break();
    ctx.push(&ctx.list_indent());
    ctx.push_line(&format!("\\begin{{{}}}", env_name));

    ctx.push_env(env);

    // Process list items
    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::ContentBlock | SyntaxKind::Markup => {
                    ctx.push(&ctx.list_indent());
                    ctx.push("  \\item ");

                    let mut item_ctx = ConvertContext::new();
                    item_ctx.list_depth = ctx.list_depth;
                    convert_markup_node(child, &mut item_ctx);
                    let item_text = item_ctx.finalize();
                    ctx.push(&item_text);
                    ctx.newline();
                }
                SyntaxKind::FuncCall => {
                    // Might be a nested list
                    let func_children: Vec<&SyntaxNode> = child.children().collect();
                    if !func_children.is_empty() {
                        let func_name = func_children[0].text().to_string();
                        if func_name == "list" {
                            convert_list_to_latex(&func_children, ctx, false);
                        } else if func_name == "enum" {
                            convert_list_to_latex(&func_children, ctx, true);
                        } else {
                            ctx.push(&ctx.list_indent());
                            ctx.push("  \\item ");
                            convert_markup_node(child, ctx);
                            ctx.newline();
                        }
                    }
                }
                _ => {
                    let text = get_simple_text(child);
                    if !text.is_empty() && text != "," {
                        ctx.push(&ctx.list_indent());
                        ctx.push("  \\item ");
                        ctx.push(&escape_latex_text(&text));
                        ctx.newline();
                    }
                }
            }
        }
    }

    ctx.pop_env();
    ctx.push(&ctx.list_indent());
    ctx.push_line(&format!("\\end{{{}}}", env_name));
}

// ============================================================================
// Raw/Verbatim Conversion
// ============================================================================

fn convert_raw_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let lang = args.named_text("lang").unwrap_or("").to_string();
    let mut content = String::new();
    let mut is_block = args.named_bool("block").unwrap_or(false);

    if let Some(first_node) = args.first_node() {
        match first_node.kind() {
            SyntaxKind::Str => {
                content = get_string_content(first_node);
            }
            SyntaxKind::ContentBlock => {
                content = first_node
                    .text()
                    .to_string()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                is_block = content.contains('\n');
            }
            _ => {}
        }
    }

    if is_block || content.contains('\n') {
        ctx.ensure_paragraph_break();
        if !lang.is_empty() && is_listings_supported(&lang) {
            // Use listings package with language (only if supported)
            ctx.push_line(&format!("\\begin{{lstlisting}}[language={}]", lang));
            ctx.push(&content);
            ctx.newline();
            ctx.push_line("\\end{lstlisting}");
        } else {
            // Unsupported or no language - use verbatim
            if !lang.is_empty() {
                ctx.push_line(&format!("% Code block (language: {})", lang));
            }
            ctx.push_line("\\begin{verbatim}");
            ctx.push(&content);
            ctx.newline();
            ctx.push_line("\\end{verbatim}");
        }
    } else {
        // Inline code
        ctx.push("\\texttt{");
        ctx.push(&escape_latex_text(&content));
        ctx.push("}");
    }
}

// ============================================================================
// List Item Helper Functions
// ============================================================================

/// Convert a single list item (unordered)
fn convert_list_item(node: &SyntaxNode, ctx: &mut ConvertContext) {
    ctx.push_indent();
    ctx.push("\\item ");
    for child in node.children() {
        if child.kind() != SyntaxKind::ListMarker {
            convert_markup_node(child, ctx);
        }
    }
    ctx.newline();
}

/// Convert a single enum item (ordered)
fn convert_enum_item(node: &SyntaxNode, ctx: &mut ConvertContext) {
    ctx.push_indent();
    ctx.push("\\item ");
    for child in node.children() {
        if child.kind() != SyntaxKind::EnumMarker {
            convert_markup_node(child, ctx);
        }
    }
    ctx.newline();
}
