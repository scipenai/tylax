//! Markup mode conversion for Typst to LaTeX
//!
//! Handles document structure, text formatting, and non-math content.

use super::context::{ConvertContext, EnvironmentContext, TokenType};
use super::math::convert_math_node;
use super::table::{LatexCell, LatexCellAlign, LatexHLine, LatexTableGenerator};
use super::utils::{
    count_heading_markers, escape_latex_text, extract_length_value, get_raw_text_with_lang,
    get_simple_text, get_string_content, is_color_name, is_display_math, is_string_or_content,
    typst_color_to_latex, FuncArgs,
};
use crate::data::typst_compat::{
    get_heading_command, is_math_func_in_markup, MarkupHandler, TYPST_MARKUP_HANDLERS,
};
use crate::tikz::{convert_cetz_to_tikz, is_cetz_code};
use typst_syntax::{SyntaxKind, SyntaxNode};

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

            // Skip empty math expressions to avoid LaTeX errors
            if math_content.is_empty() {
                return;
            }

            // Check if the converted content has alignment markers
            // If it has & outside of \begin{cases}, use align environment
            // But NEVER use align inside a table cell (LR mode)
            let has_alignment = has_unescaped_alignment(&math_content);

            if !in_table && has_alignment {
                // Always use align environment for equations with alignment markers
                ctx.push("\\begin{align}\n");
                ctx.push(&math_content);
                ctx.push("\n\\end{align}");
            } else if is_block {
                ctx.push("\\[\n");
                ctx.push(&math_content);
                ctx.push("\n\\]");
            } else {
                ctx.push("$");
                ctx.push(&math_content);
                ctx.push("$");
            }
            ctx.last_token = TokenType::Command;
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
            // Get full text recursively since node.text() may be empty for composite nodes
            let text = get_simple_text(node);
            // Extract label name (remove @ prefix)
            let label = text.trim_start_matches('@');
            if !label.is_empty() {
                ctx.push("\\ref{");
                ctx.push(label);
                ctx.push("}");
                ctx.last_token = TokenType::Command;
            }
        }

        // Labels: <label> -> \label{label}
        SyntaxKind::Label => {
            let text = node.text().to_string();
            // Extract label name (remove < and > )
            let label = text.trim_start_matches('<').trim_end_matches('>');
            ctx.push("\\label{");
            ctx.push(label);
            ctx.push("}");
            ctx.last_token = TokenType::Command;
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
                // Parse the spacing value
                if arg_text.contains("fr") {
                    // Fractional units (1fr, 2fr) -> flexible space
                    ctx.push("\\hfill");
                } else if let Some(value) = extract_length_value(&arg_text) {
                    // Fixed length units (em, pt, cm, mm, in)
                    ctx.push(&format!("\\hspace{{{}}}", value));
                } else {
                    // Fallback for unknown formats
                    ctx.push("\\hfill");
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
    let mut weight: Option<String> = None;
    let mut size: Option<String> = None;
    let mut style: Option<String> = None;
    let mut color: Option<String> = None;
    let mut content_nodes = Vec::new();

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Named => {
                    // Parse named argument: key: value
                    // Use get_simple_text to get the full text, then split on ':'
                    let full_text = get_simple_text(child);
                    if let Some(colon_pos) = full_text.find(':') {
                        let key = full_text[..colon_pos].trim();
                        let value = full_text[colon_pos + 1..].trim().to_string();

                        match key {
                            "weight" => weight = Some(value),
                            "size" => size = Some(value),
                            "style" => style = Some(value),
                            "fill" => color = Some(value),
                            _ => {}
                        }
                    }
                }
                SyntaxKind::ContentBlock | SyntaxKind::Markup | SyntaxKind::Str => {
                    content_nodes.push(child);
                }
                SyntaxKind::Ident => {
                    // Positional identifier - could be a color name
                    let text = child.text().to_string();
                    if is_color_name(&text) {
                        color = Some(text);
                    }
                }
                SyntaxKind::FuncCall => {
                    // Could be a color function like blue.lighten(50%)
                    let func_text = get_simple_text(child);
                    if func_text.contains('.') {
                        let base_color = func_text.split('.').next().unwrap_or("");
                        if is_color_name(base_color) {
                            color = Some(base_color.to_string());
                        }
                    }
                    // Or it could be content (like a nested function)
                    content_nodes.push(child);
                }
                _ => {
                    // Check for positional content (not comma/paren)
                    if child.kind() != SyntaxKind::Comma
                        && child.kind() != SyntaxKind::Colon
                        && child.kind() != SyntaxKind::LeftParen
                        && child.kind() != SyntaxKind::RightParen
                    {
                        content_nodes.push(child);
                    }
                }
            }
        }
    }

    // Apply styling wrappers
    let mut suffix_count = 0;

    // Apply color first (outermost wrapper)
    if let Some(c) = &color {
        let latex_color = typst_color_to_latex(c);
        ctx.push(&format!("\\textcolor{{{}}}", latex_color));
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
    let mut angle: Option<f64> = None;
    let mut content_nodes = Vec::new();

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Named => {
                    let named_children: Vec<_> = child.children().collect();
                    if named_children.len() >= 2 {
                        let key = named_children[0].text().to_string();
                        if key == "angle" {
                            let value = get_simple_text(named_children[1]);
                            angle = parse_angle(&value);
                        }
                    }
                }
                SyntaxKind::ContentBlock | SyntaxKind::Markup => {
                    content_nodes.push(child);
                }
                _ => {
                    // Check for positional angle argument (e.g., -45deg)
                    if child.kind() != SyntaxKind::Comma
                        && child.kind() != SyntaxKind::LeftParen
                        && child.kind() != SyntaxKind::RightParen
                    {
                        let text = child.text().to_string();
                        if text.contains("deg")
                            || text.contains("rad")
                            || text.parse::<f64>().is_ok()
                        {
                            angle = parse_angle(&text);
                        } else if child.kind() == SyntaxKind::Unary {
                            // Handle negative angles like -45deg
                            let full_text = get_simple_text(child);
                            angle = parse_angle(&full_text);
                        }
                    }
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

/// Parse angle string (e.g., "45deg", "-90deg", "1.57rad") to degrees
fn parse_angle(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.ends_with("deg") {
        s.trim_end_matches("deg").trim().parse::<f64>().ok()
    } else if s.ends_with("rad") {
        s.trim_end_matches("rad")
            .trim()
            .parse::<f64>()
            .ok()
            .map(|r| r.to_degrees())
    } else {
        // Try parsing as plain number (assume degrees)
        s.parse::<f64>().ok()
    }
}

/// Convert #rect(...)[content] to appropriate LaTeX
/// - If has content with fill: use \colorbox (preserves content)
/// - If no content with fill and height: use \rule (solid rectangle)
/// - Otherwise, use \fbox
fn convert_rect_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);

    let width = args.named("width");
    let height = args.named("height");
    let fill = args.named("fill");

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
    if let Some(f) = fill {
        let color = typst_color_to_latex(f);

        if has_content {
            // Has content - must use \colorbox to preserve it
            // Optionally wrap in minipage if width specified
            if let Some(w) = width {
                let latex_width = convert_dimension_to_latex(w);
                ctx.push(&format!("\\colorbox{{{}}}", color));
                ctx.push(&format!("{{\\begin{{minipage}}{{{}}}", latex_width));
                for node in &content_nodes {
                    convert_markup_node(node, ctx);
                }
                ctx.push("\\end{minipage}}");
            } else {
                ctx.push(&format!("\\colorbox{{{}}}", color));
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
                "{{\\color{{{}}}\\rule{{{}}}{{{}}}}}",
                color, latex_width, latex_height
            ));
        } else {
            // Has fill but no height and no content - just colorbox with empty
            ctx.push(&format!("\\colorbox{{{}}}{{}}", color));
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
    let mut columns: usize = 0;
    let mut col_aligns: Vec<LatexCellAlign> = Vec::new();
    let mut cells: Vec<LatexCell> = Vec::new();
    let mut hlines: Vec<(usize, LatexHLine)> = Vec::new(); // (cell_index, hline)
    let mut in_header = false;
    let mut header_end_idx: Option<usize> = None;

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Named => {
                    let named_children: Vec<_> = child.children().collect();
                    if !named_children.is_empty() {
                        let key = named_children[0].text().to_string();

                        if key == "columns" {
                            let full_text = get_simple_text(child);
                            if let Some(colon_pos) = full_text.find(':') {
                                let value = full_text[colon_pos + 1..].trim();
                                if let Some(n) = infer_table_columns(value) {
                                    columns = n;
                                }
                            }
                            if columns == 0 {
                                let auto_count = full_text.matches("auto").count();
                                if auto_count > 0 {
                                    columns = auto_count;
                                }
                            }
                        } else if key == "align" {
                            let full_text = get_simple_text(child);
                            if let Some(colon_pos) = full_text.find(':') {
                                let value = full_text[colon_pos + 1..].trim();
                                col_aligns = parse_typst_align(value);
                            }
                        }
                    }
                }
                SyntaxKind::ContentBlock | SyntaxKind::Markup => {
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
                                        SyntaxKind::ContentBlock | SyntaxKind::Markup => {
                                            // Plain content like [Type A]
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

fn convert_cite_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut keys: Vec<String> = Vec::new();

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Str => {
                    keys.push(get_string_content(child));
                }
                SyntaxKind::Label => {
                    let label_text = child.text().to_string();
                    keys.push(
                        label_text
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .to_string(),
                    );
                }
                _ => {
                    let text = get_simple_text(child);
                    if !text.is_empty() && text != "," {
                        keys.push(
                            text.trim_start_matches('<')
                                .trim_end_matches('>')
                                .to_string(),
                        );
                    }
                }
            }
        }
    }

    if keys.is_empty() {
        return;
    }

    ctx.push("\\cite{");
    ctx.push(&keys.join(", "));
    ctx.push("}");
}

fn convert_ref_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    if let Some(args) = children.get(1) {
        for child in args.children() {
            let text = get_simple_text(child);
            if !text.is_empty() {
                let label = text.trim_start_matches('<').trim_end_matches('>');
                ctx.push("\\ref{");
                ctx.push(label);
                ctx.push("}");
                return;
            }
        }
    }
}

fn convert_label_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    if let Some(args) = children.get(1) {
        for child in args.children() {
            let text = get_simple_text(child);
            if !text.is_empty() {
                let label = text.trim_start_matches('<').trim_end_matches('>');
                ctx.push("\\label{");
                ctx.push(label);
                ctx.push("}");
                return;
            }
        }
    }
}

fn convert_bibliography_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut bib_file = String::new();
    let mut style = "plain".to_string();

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Str => {
                    bib_file = get_string_content(child);
                }
                SyntaxKind::Named => {
                    let named_children: Vec<_> = child.children().collect();
                    if named_children.len() >= 2 {
                        let key = named_children[0].text().to_string();
                        if key == "style" {
                            style = get_simple_text(named_children[1]);
                        }
                    }
                }
                _ => {}
            }
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
    let mut num_cols = 2usize;

    if let Some(args) = children.get(1) {
        for child in args.children() {
            if child.kind() == SyntaxKind::Named {
                let named_children: Vec<_> = child.children().collect();
                if named_children.len() >= 2 {
                    let key = named_children[0].text().to_string();
                    if key == "columns" {
                        if let Ok(n) = get_simple_text(named_children[1]).parse::<usize>() {
                            num_cols = n;
                        }
                    }
                }
            }
        }
    }

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
    for (i, child) in children.iter().enumerate() {
        if i == 0 {
            continue; // Skip function name
        }
        if child.kind() == SyntaxKind::Args {
            for arg_child in child.children() {
                match arg_child.kind() {
                    SyntaxKind::ContentBlock | SyntaxKind::Markup => {
                        convert_markup_node(arg_child, ctx);
                    }
                    SyntaxKind::Named => {
                        // For named args, only process the value if it's content
                        // Skip the key
                        let named_children: Vec<_> = arg_child.children().collect();
                        if named_children.len() >= 2 {
                            let value = named_children[1];
                            if value.kind() == SyntaxKind::ContentBlock
                                || value.kind() == SyntaxKind::Markup
                            {
                                convert_markup_node(value, ctx);
                            }
                            // Otherwise ignore scalar values like size: 10pt
                        }
                    }
                    _ => {
                        // Check if it is "body" (pos arg)
                        if arg_child.kind() != SyntaxKind::Comma
                            && arg_child.kind() != SyntaxKind::Colon
                            && arg_child.kind() != SyntaxKind::LeftParen
                            && arg_child.kind() != SyntaxKind::RightParen
                        {
                            convert_markup_node(arg_child, ctx);
                        }
                    }
                }
            }
        } else if child.kind() == SyntaxKind::ContentBlock {
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
    let mut lang = String::new();
    let mut content = String::new();
    let mut is_block = false;

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Str => {
                    content = get_string_content(child);
                }
                SyntaxKind::Named => {
                    let named_children: Vec<_> = child.children().collect();
                    if named_children.len() >= 2 {
                        let key = named_children[0].text().to_string();
                        if key == "lang" {
                            lang = get_simple_text(named_children[1]);
                        } else if key == "block" {
                            let val = get_simple_text(named_children[1]);
                            is_block = val == "true";
                        }
                    }
                }
                SyntaxKind::ContentBlock => {
                    content = child
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
