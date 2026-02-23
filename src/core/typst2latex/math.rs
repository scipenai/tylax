//! Math mode conversion for Typst to LaTeX
//!
//! Handles mathematical expressions, formulas, and math-specific constructs.

use super::context::{ConvertContext, TokenType};
use super::utils::{get_simple_text, is_content_node, UNICODE_TO_LATEX};
use crate::data::maps::{DELIMITER_MAP, TYPST_TO_TEX};
use crate::data::typst_compat::{MathHandler, TYPST_MATH_HANDLERS};
use typst_syntax::{SyntaxKind, SyntaxNode};

/// Convert a math node to LaTeX
pub fn convert_math_node(node: &SyntaxNode, ctx: &mut ConvertContext) {
    match node.kind() {
        SyntaxKind::MathIdent => {
            let text = node.text();
            let text_str = text.as_str();

            // Skip Typst-specific invisible characters
            if matches!(text_str, "zws" | "zwsp" | "nbsp" | "wj" | "shy") {
                // zws = zero-width space, nbsp = non-breaking space, wj = word joiner, shy = soft hyphen
                return;
            }

            // Handle spacing commands that are the same in LaTeX
            match text_str {
                "quad" => {
                    ctx.push("\\quad ");
                    ctx.last_token = TokenType::Command;
                    return;
                }
                "qquad" => {
                    ctx.push("\\qquad ");
                    ctx.last_token = TokenType::Command;
                    return;
                }
                "space" | "sp" => {
                    ctx.push("\\ ");
                    ctx.last_token = TokenType::Command;
                    return;
                }
                "thin" => {
                    ctx.push("\\, ");
                    ctx.last_token = TokenType::Command;
                    return;
                }
                "med" => {
                    ctx.push("\\: ");
                    ctx.last_token = TokenType::Command;
                    return;
                }
                "thick" => {
                    ctx.push("\\; ");
                    ctx.last_token = TokenType::Command;
                    return;
                }
                "wide" => {
                    ctx.push("\\qquad ");
                    ctx.last_token = TokenType::Command;
                    return;
                }
                // Basic arithmetic operators (Typst keywords)
                "plus" => {
                    ctx.push_with_spacing("+", TokenType::Operator);
                    return;
                }
                "minus" => {
                    ctx.push_with_spacing("-", TokenType::Operator);
                    return;
                }
                "eq" => {
                    ctx.push_with_spacing("=", TokenType::Operator);
                    return;
                }
                "lt" => {
                    ctx.push_with_spacing("<", TokenType::Operator);
                    return;
                }
                "gt" => {
                    ctx.push_with_spacing(">", TokenType::Operator);
                    return;
                }
                _ => {}
            }

            // Check the full identifier in symbol map
            if let Some(tex) = TYPST_TO_TEX.get(text_str) {
                if !tex.is_empty() {
                    // tex may already include backslash
                    if tex.starts_with('\\') {
                        ctx.push(tex);
                    } else {
                        ctx.push("\\");
                        ctx.push(tex);
                    }
                    ctx.last_token = TokenType::Command;
                }
            } else if text_str.len() == 1 {
                if let Some(ch) = text_str.chars().next() {
                    // Check if it's a Unicode math character
                    if let Some(latex) = UNICODE_TO_LATEX.get(&ch) {
                        ctx.push(latex);
                        ctx.last_token = TokenType::Command;
                    } else if ch.is_alphabetic() {
                        // Single ASCII letter, just output
                        ctx.push_with_spacing(text_str, TokenType::Letter);
                    } else {
                        ctx.push_with_spacing(text_str, TokenType::Letter);
                    }
                }
            } else {
                // Multi-letter identifier - check for Unicode chars
                let converted = convert_unicode_in_text(text_str);
                ctx.push_with_spacing(&converted, TokenType::Letter);
            }
        }

        SyntaxKind::FieldAccess => {
            // Handle dotted symbols like alpha.alt, eq.not, bar.v.double, etc.
            // Get the full text by collecting all child node texts
            let full_text = collect_field_access_text(node);
            let full_text_str = full_text.as_str();

            // First, try to find in symbol map
            if let Some(tex) = TYPST_TO_TEX.get(full_text_str) {
                if !tex.is_empty() {
                    // tex may already include backslash (like "\\|" for bar.v.double)
                    if tex.starts_with('\\') {
                        ctx.push(tex);
                    } else {
                        ctx.push("\\");
                        ctx.push(tex);
                    }
                    ctx.last_token = TokenType::Command;
                }
            } else if full_text_str == "square.stroked" || full_text_str == "square.filled" {
                ctx.push("\\blacksquare");
                ctx.last_token = TokenType::Command;
            } else if full_text_str == "bar.v.double" || full_text_str.ends_with(".v.double") {
                ctx.push("\\|");
                ctx.last_token = TokenType::Operator;
            } else if full_text_str == "bar.v" {
                ctx.push("|");
                ctx.last_token = TokenType::Operator;
            } else {
                // Fallback: just output the text
                ctx.push_with_spacing(&full_text, TokenType::Letter);
            }
        }

        SyntaxKind::Space => {
            if !ctx.output.ends_with(' ') && !ctx.output.ends_with('{') {
                ctx.push(" ");
            }
            ctx.last_token = TokenType::None;
        }

        SyntaxKind::Linebreak => {
            // In aligned equations, linebreaks become \\
            ctx.push(" \\\\\n");
            ctx.last_token = TokenType::Newline;
        }

        SyntaxKind::MathAttach => {
            // Subscript/Superscript: base _ {sub} ^ {sup}
            // Filter out Space nodes to handle various AST structures
            let children: Vec<&SyntaxNode> = node
                .children()
                .filter(|c| c.kind() != SyntaxKind::Space)
                .collect();

            // Find the base - it's the first content node (not Hat/Underscore)
            let base_idx = children
                .iter()
                .position(|c| c.kind() != SyntaxKind::Hat && c.kind() != SyntaxKind::Underscore);

            if let Some(idx) = base_idx {
                // Output the base
                convert_math_node(children[idx], ctx);

                // Process remaining children for sub/superscripts
                let mut i = 0;
                while i < children.len() {
                    if i == idx {
                        i += 1;
                        continue; // Skip the base we already output
                    }

                    match children[i].kind() {
                        SyntaxKind::Hat => {
                            ctx.push("^");
                            ctx.last_token = TokenType::Operator;
                            // Next non-space child is the superscript content
                            if i + 1 < children.len()
                                && children[i + 1].kind() != SyntaxKind::Hat
                                && children[i + 1].kind() != SyntaxKind::Underscore
                            {
                                let content = extract_subscript_content(children[i + 1]);
                                output_subscript_content(&content, ctx);
                                i += 1; // Skip the content we just processed
                            }
                        }
                        SyntaxKind::Underscore => {
                            ctx.push("_");
                            ctx.last_token = TokenType::Operator;
                            // Next non-space child is the subscript content
                            if i + 1 < children.len()
                                && children[i + 1].kind() != SyntaxKind::Hat
                                && children[i + 1].kind() != SyntaxKind::Underscore
                            {
                                let content = extract_subscript_content(children[i + 1]);
                                output_subscript_content(&content, ctx);
                                i += 1; // Skip the content we just processed
                            }
                        }
                        _ => {
                            // Standalone content after base - might be sub/superscript without explicit marker
                            let content = extract_subscript_content(children[i]);
                            if !content.is_empty() {
                                output_subscript_content(&content, ctx);
                            }
                        }
                    }
                    i += 1;
                }
            } else {
                // No base found, just output all children
                for child in &children {
                    convert_math_node(child, ctx);
                }
            }
        }

        SyntaxKind::FuncCall => {
            convert_func_call(node, ctx);
        }

        SyntaxKind::MathFrac => {
            // Inline fraction: a/b style
            // MathFrac can have various structures:
            // 1. [numerator, Slash, denominator] - simple case
            // 2. Complex expressions where Slash might not be a direct child
            let children: Vec<&SyntaxNode> = node.children().collect();

            // Find the Slash position to split numerator and denominator
            let slash_pos = children.iter().position(|c| c.kind() == SyntaxKind::Slash);

            ctx.push("\\frac{");

            if let Some(pos) = slash_pos {
                // Everything before Slash is numerator
                for child in &children[..pos] {
                    convert_math_node(child, ctx);
                }
                ctx.push("}{");
                // Everything after Slash is denominator
                for child in &children[pos + 1..] {
                    convert_math_node(child, ctx);
                }
            } else {
                // No Slash found - might be a different structure
                // Try to split in half
                if children.len() >= 2 {
                    let mid = children.len() / 2;
                    for child in &children[..mid] {
                        convert_math_node(child, ctx);
                    }
                    ctx.push("}{");
                    for child in &children[mid..] {
                        convert_math_node(child, ctx);
                    }
                } else if !children.is_empty() {
                    convert_math_node(children[0], ctx);
                    ctx.push("}{");
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        SyntaxKind::MathRoot => {
            // Root: sqrt style - Typst syntax: sqrt(x) or root(n, x)
            // Filter out non-content nodes (Space, Comma, etc.)
            let content_children: Vec<&SyntaxNode> =
                node.children().filter(|c| is_content_node(c)).collect();

            match content_children.len() {
                0 => {
                    // Empty root, just output \sqrt{}
                    ctx.push("\\sqrt{}");
                }
                1 => {
                    // Simple square root: sqrt(x)
                    ctx.push("\\sqrt{");
                    convert_math_node(content_children[0], ctx);
                    ctx.push("}");
                }
                _ => {
                    // nth root: root(n, x) - first is index, rest is radicand
                    ctx.push("\\sqrt[");
                    convert_math_node(content_children[0], ctx);
                    ctx.push("]{");
                    // All remaining children form the radicand
                    for child in &content_children[1..] {
                        convert_math_node(child, ctx);
                    }
                    ctx.push("}");
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // Delimiters - only use plain delimiters here
        // \left/\right are handled by lr() function in convert_func_call
        SyntaxKind::LeftParen => {
            ctx.push("(");
            ctx.last_token = TokenType::OpenParen;
        }
        SyntaxKind::RightParen => {
            ctx.push(")");
            ctx.last_token = TokenType::CloseParen;
        }
        SyntaxKind::LeftBracket => {
            ctx.push("[");
            ctx.last_token = TokenType::OpenParen;
        }
        SyntaxKind::RightBracket => {
            ctx.push("]");
            ctx.last_token = TokenType::CloseParen;
        }
        SyntaxKind::LeftBrace => {
            ctx.push("\\{");
            ctx.last_token = TokenType::OpenParen;
        }
        SyntaxKind::RightBrace => {
            ctx.push("\\}");
            ctx.last_token = TokenType::CloseParen;
        }

        // Operators
        SyntaxKind::Plus => {
            ctx.push(" + ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Minus => {
            ctx.push(" - ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Star => {
            ctx.push(" \\cdot ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Slash => {
            ctx.push("/");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Eq => {
            ctx.push(" = ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::EqEq => {
            ctx.push(" \\equiv ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Lt => {
            ctx.push(" < ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Gt => {
            ctx.push(" > ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::LtEq => {
            ctx.push(" \\leq ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::GtEq => {
            ctx.push(" \\geq ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Comma => {
            ctx.push(", ");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Colon => {
            ctx.push(":");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Semicolon => {
            ctx.push(";");
            ctx.last_token = TokenType::Operator;
        }
        SyntaxKind::Dots => {
            ctx.push("\\ldots ");
            ctx.last_token = TokenType::Command;
        }

        // Literals
        SyntaxKind::Int => {
            ctx.push_with_spacing(node.text().as_str(), TokenType::Number);
        }
        SyntaxKind::Float => {
            ctx.push_with_spacing(node.text().as_str(), TokenType::Number);
        }
        SyntaxKind::Str => {
            // String in math mode -> \text{}
            let text = node.text();
            let text_str = text.as_str();
            let inner = text_str.trim_matches('"');
            ctx.push("\\text{");
            ctx.push(inner);
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // Math delimited (parentheses with content)
        // Uses unified delimiter detection via DELIMITER_MAP
        SyntaxKind::MathDelimited => {
            // Filter out Space nodes (consistent with lr() path)
            let children: Vec<&SyntaxNode> = node
                .children()
                .filter(|n| n.kind() != SyntaxKind::Space)
                .collect();

            if children.is_empty() {
                return;
            }

            // Use unified delimiter detection (same as lr() path)
            let first = children[0];
            let last = children.last().unwrap_or(&first);

            let first_text = get_node_delimiter_text(first);
            let last_text = get_node_delimiter_text(last);

            let first_is_delim = is_delimiter(&first_text);
            let last_is_delim = is_delimiter(&last_text);

            if first_is_delim && last_is_delim {
                // Output \left<delim> ... \right<delim>
                ctx.push("\\left");
                let left_delim = get_latex_delimiter(&first_text, true);
                ctx.push(left_delim);
                set_last_token_after_delimiter(ctx, left_delim);

                // Output content (skip first and last delimiter nodes)
                for child in &children[1..children.len() - 1] {
                    convert_math_node(child, ctx);
                }

                ctx.push("\\right");
                let right_delim = get_latex_delimiter(&last_text, false);
                ctx.push(right_delim);
            } else {
                // Fallback: just output all children
                for child in children {
                    convert_math_node(child, ctx);
                }
            }
        }

        // Math primes
        SyntaxKind::MathPrimes => {
            let count = node.text().as_str().chars().filter(|&c| c == '\'').count();
            for _ in 0..count {
                ctx.push("'");
            }
        }

        _ => {
            // Recursively process children
            let child_count = node.children().count();
            if child_count > 0 {
                for child in node.children() {
                    convert_math_node(child, ctx);
                }
            } else {
                // Leaf node, just output text
                let text = node.text();
                let text_str = text.as_str();

                // Skip Typst invisible characters
                if matches!(text_str, "zws" | "zwsp" | "nbsp" | "wj" | "shy") {
                    return;
                }

                // Check if it's a known typst symbol (including compound names like square.stroked)
                if let Some(tex) = TYPST_TO_TEX.get(text_str) {
                    if !tex.is_empty() {
                        if tex.starts_with('\\') {
                            ctx.push(tex);
                        } else {
                            ctx.push("\\");
                            ctx.push(tex);
                        }
                        ctx.last_token = TokenType::Command;
                    }
                } else if !text_str.trim().is_empty() {
                    // Convert Unicode math characters to LaTeX
                    let converted = convert_unicode_in_text(text_str);
                    ctx.push_with_spacing(&converted, TokenType::Letter);
                }
            }
        }
    }
}

/// Extract function name from the first child of a FuncCall, handling FieldAccess
fn get_math_func_name(node: &SyntaxNode) -> String {
    match node.kind() {
        SyntaxKind::FieldAccess => {
            // Collect parts for dotted names like math.floor
            collect_field_access_text(node)
        }
        _ => node.text().to_string(),
    }
}

/// Convert function calls like frac(), sqrt(), sum(), etc.
pub fn convert_func_call(node: &SyntaxNode, ctx: &mut ConvertContext) {
    let children: Vec<&SyntaxNode> = node.children().collect();
    if children.is_empty() {
        return;
    }

    // Get function name, handling FieldAccess (e.g., math.floor)
    let func_str = get_math_func_name(children[0]);
    let func_str = func_str.as_str();

    // Try to use the handler map
    if let Some(handler) = TYPST_MATH_HANDLERS.get(func_str) {
        match handler {
            MathHandler::Command { latex_cmd } => {
                ctx.push(latex_cmd);
                if let Some(args_node) = children.get(1) {
                    let args: Vec<&SyntaxNode> = args_node
                        .children()
                        .filter(|n| is_content_node(n))
                        .collect();
                    for arg in args {
                        ctx.push("{");
                        convert_math_node(arg, ctx);
                        ctx.push("}");
                    }
                }
                ctx.last_token = TokenType::Command;
            }

            MathHandler::CommandWithOpt { latex_cmd } => {
                // Handle sqrt/root with optional argument
                ctx.push(latex_cmd);
                if let Some(args_node) = children.get(1) {
                    let args: Vec<&SyntaxNode> = args_node
                        .children()
                        .filter(|n| is_content_node(n))
                        .collect();

                    if args.len() == 2 {
                        // nth root: sqrt(n, x) -> \sqrt[n]{x}
                        ctx.push("[");
                        convert_math_node(args[0], ctx);
                        ctx.push("]{");
                        convert_math_node(args[1], ctx);
                        ctx.push("}");
                    } else {
                        ctx.push("{");
                        for arg in args {
                            convert_math_node(arg, ctx);
                        }
                        ctx.push("}");
                    }
                }
                ctx.last_token = TokenType::Command;
            }

            MathHandler::Delimiters { open, close } => {
                ctx.push(open);
                if let Some(args_node) = children.get(1) {
                    for arg in args_node.children().filter(|n| is_content_node(n)) {
                        convert_math_node(arg, ctx);
                    }
                }
                ctx.push(close);
                ctx.last_token = TokenType::Command;
            }

            MathHandler::BigOperator { latex_cmd } => {
                ctx.push(latex_cmd);
                // Handle limits if any
                for child in &children[1..] {
                    convert_math_node(child, ctx);
                }
                ctx.last_token = TokenType::Command;
            }

            MathHandler::Environment { name } => {
                if name == &"cases" {
                    convert_cases(node, ctx);
                } else {
                    convert_matrix(node, ctx, name);
                }
            }

            MathHandler::Special => {
                handle_special_math_func(func_str, &children, ctx);
            }
        }
    } else {
        // Unknown function - check symbol map or use operatorname
        // For symbol functions (like phi, psi), we need to preserve parentheses
        // For operatorname functions, we also use parentheses (standard math notation)
        if let Some(tex) = TYPST_TO_TEX.get(func_str) {
            ctx.push("\\");
            ctx.push(tex);
        } else {
            // Use operatorname for unknown functions
            ctx.push("\\operatorname{");
            ctx.push(func_str);
            ctx.push("}");
        }

        // Arguments - use () for symbol functions and operatorname (math notation)
        // Use {} only for LaTeX commands with brace arguments (handled by MathHandler::Command)
        for child in &children[1..] {
            if child.kind() == SyntaxKind::Args {
                // For symbols and operatorname, use parentheses (mathematical function notation)
                ctx.push("(");
                let args: Vec<&SyntaxNode> =
                    child.children().filter(|n| is_content_node(n)).collect();
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        ctx.push(", ");
                    }
                    convert_math_node(arg, ctx);
                }
                ctx.push(")");
            } else {
                convert_math_node(child, ctx);
            }
        }
        ctx.last_token = TokenType::Command;
    }
}

/// Handle special math functions that need custom logic
fn handle_special_math_func(func_str: &str, children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    match func_str {
        // math.vec(a, b, c) -> column vector as pmatrix
        "math.vec" => {
            ctx.push("\\begin{pmatrix}\n");
            if let Some(args_node) = children.get(1) {
                let args: Vec<&SyntaxNode> = args_node
                    .children()
                    .filter(|n| is_content_node(n))
                    .collect();
                for (i, arg) in args.iter().enumerate() {
                    ctx.push("  ");
                    convert_math_node(arg, ctx);
                    if i < args.len() - 1 {
                        ctx.push(" \\\\\n");
                    } else {
                        ctx.push("\n");
                    }
                }
            }
            ctx.push("\\end{pmatrix}");
            ctx.last_token = TokenType::Command;
        }

        // lr(delim1 content delim2) -> \left<delim1> content \right<delim2>
        "lr" => {
            if let Some(args_node) = children.get(1) {
                convert_lr_to_latex(args_node, ctx);
            }
            ctx.last_token = TokenType::Command;
        }

        // attach(base, t: top, b: bottom, tl: topleft, tr: topright, bl: bottomleft, br: bottomright)
        // -> base with various decorations
        "attach" => {
            if let Some(args_node) = children.get(1) {
                let mut base_content = String::new();
                let mut top = None;
                let mut bottom = None;
                let mut topleft = None;
                let mut topright = None;
                let mut bottomleft = None;
                let mut bottomright = None;

                for child in args_node.children() {
                    match child.kind() {
                        SyntaxKind::Named => {
                            let named_children: Vec<&SyntaxNode> = child.children().collect();
                            if named_children.len() >= 2 {
                                let key = named_children[0].text().to_string();
                                let mut val_ctx = ConvertContext::new();
                                val_ctx.in_math = true;
                                for nc in &named_children[1..] {
                                    if is_content_node(nc) {
                                        convert_math_node(nc, &mut val_ctx);
                                    }
                                }
                                let val = val_ctx.finalize();
                                match key.as_str() {
                                    "t" | "top" => top = Some(val),
                                    "b" | "bottom" => bottom = Some(val),
                                    "tl" => topleft = Some(val),
                                    "tr" => topright = Some(val),
                                    "bl" => bottomleft = Some(val),
                                    "br" => bottomright = Some(val),
                                    _ => {}
                                }
                            }
                        }
                        _ if is_content_node(child) && base_content.is_empty() => {
                            let mut base_ctx = ConvertContext::new();
                            base_ctx.in_math = true;
                            convert_math_node(child, &mut base_ctx);
                            base_content = base_ctx.finalize();
                        }
                        _ => {}
                    }
                }

                // Handle pre-scripts (topleft, bottomleft)
                if topleft.is_some() || bottomleft.is_some() {
                    ctx.push("{}");
                    if let Some(bl) = &bottomleft {
                        ctx.push("_{");
                        ctx.push(bl);
                        ctx.push("}");
                    }
                    if let Some(tl) = &topleft {
                        ctx.push("^{");
                        ctx.push(tl);
                        ctx.push("}");
                    }
                }

                // Output base
                ctx.push(&base_content);

                // Handle post-scripts
                if let Some(b) = &bottom {
                    ctx.push("_{");
                    ctx.push(b);
                    ctx.push("}");
                }
                if let Some(t) = &top {
                    ctx.push("^{");
                    ctx.push(t);
                    ctx.push("}");
                }

                // Handle topright/bottomright (less common, approximate with regular scripts)
                if bottomright.is_some() || topright.is_some() {
                    ctx.push("{}");
                    if let Some(br) = &bottomright {
                        ctx.push("_{");
                        ctx.push(br);
                        ctx.push("}");
                    }
                    if let Some(tr) = &topright {
                        ctx.push("^{");
                        ctx.push(tr);
                        ctx.push("}");
                    }
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // scripts(content) -> content with scripts positioning
        "scripts" => {
            ctx.push("\\displaystyle ");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // primes(n) -> '''... (n primes)
        "primes" => {
            if let Some(args_node) = children.get(1) {
                let args: Vec<&SyntaxNode> = args_node
                    .children()
                    .filter(|n| is_content_node(n))
                    .collect();
                if !args.is_empty() {
                    let count_text = get_simple_text(args[0]);
                    if let Ok(count) = count_text.trim().parse::<usize>() {
                        for _ in 0..count {
                            ctx.push("'");
                        }
                    }
                }
            }
            ctx.last_token = TokenType::Letter;
        }

        // stretch(symbol) -> extensible version of symbol
        "stretch" => {
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    // For arrows and braces, use extensible versions
                    let text = get_simple_text(arg);
                    match text.as_str() {
                        "arrow.r" | "->" => ctx.push("\\xrightarrow{}"),
                        "arrow.l" | "<-" => ctx.push("\\xleftarrow{}"),
                        "arrow.l.r" | "<->" => ctx.push("\\xleftrightarrow{}"),
                        "brace.t" => ctx.push("\\overbrace{}"),
                        "brace.b" => ctx.push("\\underbrace{}"),
                        _ => convert_math_node(arg, ctx),
                    }
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // mid(delim) -> \mid or \middle|
        "mid" => {
            ctx.push("\\mid ");
            ctx.last_token = TokenType::Operator;
        }

        // circle(content) -> \circled{content} or \textcircled
        "circle" => {
            ctx.push("\\mathring{");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // divergence(field) -> \nabla \cdot field
        "divergence" => {
            ctx.push("\\nabla \\cdot ");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // curl(field) -> \nabla \times field
        "curl" => {
            ctx.push("\\nabla \\times ");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            ctx.last_token = TokenType::Command;
        }

        "color" => {
            if let Some(args_node) = children.get(1) {
                let args: Vec<&SyntaxNode> = args_node
                    .children()
                    .filter(|n| is_content_node(n))
                    .collect();
                if args.len() >= 2 {
                    ctx.push("{\\color{");
                    convert_math_node(args[0], ctx);
                    ctx.push("}");
                    convert_math_node(args[1], ctx);
                    ctx.push("}");
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // limits(base)^(sup)_(sub) -> \overset{sup}{\underset{sub}{base}}
        // This is how tex2typst handles it in the reverse direction
        "limits" => {
            // For now, just output the base; the sup/sub will be handled by MathAttach
            if let Some(args_node) = children.get(1) {
                let args: Vec<&SyntaxNode> = args_node
                    .children()
                    .filter(|n| is_content_node(n))
                    .collect();
                if !args.is_empty() {
                    convert_math_node(args[0], ctx);
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // arrow(content) -> \overrightarrow{content}
        "arrow" => {
            ctx.push("\\overrightarrow{");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // accent(base, accent) -> depends on accent type
        "accent" => {
            if let Some(args_node) = children.get(1) {
                let args: Vec<&SyntaxNode> = args_node
                    .children()
                    .filter(|n| is_content_node(n))
                    .collect();
                if args.len() >= 2 {
                    let accent_text = args[1].text().to_string();
                    let latex_cmd = match accent_text.as_str() {
                        "arrow.l" => "\\overleftarrow",
                        "arrow.r" => "\\overrightarrow",
                        "arrow.l.r" => "\\overleftrightarrow",
                        _ => "\\hat",
                    };
                    ctx.push(latex_cmd);
                    ctx.push("{");
                    convert_math_node(args[0], ctx);
                    ctx.push("}");
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // class("type", content) -> appropriate LaTeX class command
        "class" => {
            if let Some(args_node) = children.get(1) {
                let args: Vec<&SyntaxNode> = args_node
                    .children()
                    .filter(|n| is_content_node(n))
                    .collect();
                if args.len() >= 2 {
                    let class_type = get_simple_text(args[0]);
                    let latex_cmd = match class_type.trim_matches('"') {
                        "relation" => "\\mathrel",
                        "binary" => "\\mathbin",
                        "large" => "\\mathop",
                        "opening" => "\\mathopen",
                        "closing" => "\\mathclose",
                        "punctuation" => "\\mathpunct",
                        _ => "",
                    };
                    if !latex_cmd.is_empty() {
                        ctx.push(latex_cmd);
                        ctx.push("{");
                    }
                    convert_math_node(args[1], ctx);
                    if !latex_cmd.is_empty() {
                        ctx.push("}");
                    }
                }
            }
            ctx.last_token = TokenType::Command;
        }

        // op("name") -> \operatorname{name}
        "op" => {
            ctx.push("\\operatorname{");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    let text = get_simple_text(arg);
                    // Remove quotes if present
                    let text = text.trim_matches('"');
                    ctx.push(text);
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // display(content) -> behavior depends on block_math_mode
        // - block mode: \displaystyle content (no need to restore)
        // - inline mode: \displaystyle content \textstyle (restore to inline)
        "display" => {
            ctx.push("\\displaystyle ");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            // If we're in inline math mode, restore to textstyle after display content
            if !ctx.options.block_math_mode {
                ctx.push(" \\textstyle ");
            }
            ctx.last_token = TokenType::Command;
        }

        // inline(content) -> behavior depends on block_math_mode
        // - block mode: \textstyle content \displaystyle (restore to block)
        // - inline mode: \textstyle content (no need to restore)
        "inline" => {
            ctx.push("\\textstyle ");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            // If we're in block math mode, restore to displaystyle after inline content
            if ctx.options.block_math_mode {
                ctx.push(" \\displaystyle ");
            }
            ctx.last_token = TokenType::Command;
        }

        // set(content) or Set(content) -> \{content\}
        "set" | "Set" => {
            ctx.push("\\left\\{");
            if let Some(args_node) = children.get(1) {
                for arg in args_node.children().filter(|n| is_content_node(n)) {
                    convert_math_node(arg, ctx);
                }
            }
            ctx.push("\\right\\}");
            ctx.last_token = TokenType::Command;
        }

        _ => {
            // Fallback
            ctx.push("\\operatorname{");
            ctx.push(func_str);
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }
    }
}

/// Convert matrix-like constructs
pub fn convert_matrix(node: &SyntaxNode, ctx: &mut ConvertContext, env_name: &str) {
    let children: Vec<&SyntaxNode> = node.children().collect();

    // Determine the actual environment name based on delim: parameter
    let mut actual_env = env_name.to_string();

    if let Some(args_node) = children.get(1) {
        for child in args_node.children() {
            if child.kind() == SyntaxKind::Named {
                let named_children: Vec<&SyntaxNode> = child.children().collect();
                if !named_children.is_empty() {
                    let key = named_children[0].text().to_string();
                    if key == "delim" {
                        // Find the value (skip key, colon, space)
                        for nc in &named_children {
                            let val = nc.text().to_string();
                            let val = val.trim().trim_matches('"').trim_matches('\'');
                            match val {
                                "[" => {
                                    actual_env = "bmatrix".to_string();
                                    break;
                                }
                                "(" => {
                                    actual_env = "pmatrix".to_string();
                                    break;
                                }
                                "{" => {
                                    actual_env = "Bmatrix".to_string();
                                    break;
                                }
                                "|" => {
                                    actual_env = "vmatrix".to_string();
                                    break;
                                }
                                "||" => {
                                    actual_env = "Vmatrix".to_string();
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    ctx.push("\\begin{");
    ctx.push(&actual_env);
    ctx.push("}\n");
    ctx.in_environment = true;

    if let Some(args_node) = children.get(1) {
        // Find array content - matrix rows are separated by Semicolon
        // Each row can be an Array node or individual content nodes
        let mut rows: Vec<Vec<String>> = vec![];
        let mut current_row: Vec<String> = vec![];

        for child in args_node.children() {
            match child.kind() {
                SyntaxKind::Named => {
                    // Skip named arguments like delim: "["
                    continue;
                }
                SyntaxKind::Semicolon => {
                    // Row separator
                    if !current_row.is_empty() {
                        rows.push(current_row);
                        current_row = vec![];
                    }
                }
                SyntaxKind::Comma
                | SyntaxKind::Space
                | SyntaxKind::LeftParen
                | SyntaxKind::RightParen => {
                    // Skip separators and parentheses
                }
                SyntaxKind::Array => {
                    // Array node contains a row of cells
                    // Extract cells from the Array, separated by commas
                    for arr_child in child.children() {
                        match arr_child.kind() {
                            SyntaxKind::Comma | SyntaxKind::Space => {
                                // Skip separators
                            }
                            _ if is_content_node(arr_child) => {
                                let mut cell_ctx = ConvertContext::new();
                                convert_math_node(arr_child, &mut cell_ctx);
                                let cell = cell_ctx.finalize();
                                if !cell.trim().is_empty() {
                                    current_row.push(cell);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ if is_content_node(child) => {
                    let mut cell_ctx = ConvertContext::new();
                    convert_math_node(child, &mut cell_ctx);
                    let cell = cell_ctx.finalize();
                    if !cell.trim().is_empty() {
                        current_row.push(cell);
                    }
                }
                _ => {}
            }
        }

        if !current_row.is_empty() {
            rows.push(current_row);
        }

        // Output rows
        for (i, row) in rows.iter().enumerate() {
            ctx.push("  ");
            ctx.push(&row.join(" & "));
            if i < rows.len() - 1 {
                ctx.push(" \\\\\n");
            } else {
                ctx.push("\n");
            }
        }
    }

    ctx.push("\\end{");
    ctx.push(&actual_env);
    ctx.push("}");
    ctx.in_environment = false;
    ctx.last_token = TokenType::Command;
}

/// Convert cases construct
pub fn convert_cases(node: &SyntaxNode, ctx: &mut ConvertContext) {
    ctx.push("\\begin{cases}\n");
    ctx.in_environment = true;

    let children: Vec<&SyntaxNode> = node.children().collect();
    if let Some(args_node) = children.get(1) {
        // Collect items, grouping by & separator
        // In Typst cases: cases(value1, & condition1, value2, & condition2)
        // Items starting with & are conditions for the previous value
        let mut items: Vec<String> = vec![];

        for child in args_node.children() {
            if is_content_node(child) {
                let mut item_ctx = ConvertContext::new();
                convert_math_node(child, &mut item_ctx);
                let item = item_ctx.finalize();
                if !item.is_empty() {
                    items.push(item);
                }
            }
        }

        // Process items: pair value with its condition (which starts with &)
        let mut i = 0;
        while i < items.len() {
            let value = &items[i];
            ctx.push("  ");
            ctx.push(value);

            // Check if next item is a condition (starts with &)
            if i + 1 < items.len() {
                let next = items[i + 1].trim();
                if next.starts_with('&') {
                    // It's a condition, add it (& is already there)
                    ctx.push(" ");
                    ctx.push(next);
                    i += 2;
                } else {
                    // No condition, just the value
                    i += 1;
                }
            } else {
                i += 1;
            }

            if i < items.len() {
                ctx.push(" \\\\\n");
            } else {
                ctx.push("\n");
            }
        }
    }

    ctx.push("\\end{cases}");
    ctx.in_environment = false;
    ctx.last_token = TokenType::Command;
}

// =============================================================================
// Delimiter Helper Functions (using DELIMITER_MAP as single source of truth)
// =============================================================================

/// Check if a text string represents a recognized delimiter for lr()
fn is_delimiter(text: &str) -> bool {
    DELIMITER_MAP.contains_key(text)
}

/// Get LaTeX delimiter string from text, with fallback
fn get_latex_delimiter(text: &str, is_left: bool) -> &'static str {
    DELIMITER_MAP
        .get(text)
        .copied()
        .unwrap_or(if is_left { "(" } else { ")" })
}

/// Update last token type after emitting a delimiter
fn set_last_token_after_delimiter(ctx: &mut ConvertContext, delim: &str) {
    if delim.starts_with('\\') {
        ctx.last_token = TokenType::Command;
    } else {
        ctx.last_token = TokenType::OpenParen;
    }
}

// =============================================================================
// lr() Conversion Functions
// =============================================================================

/// Convert Typst lr() function to LaTeX \left...\right
/// Extracted to a separate function for clarity and maintainability
fn convert_lr_to_latex(args_node: &SyntaxNode, ctx: &mut ConvertContext) {
    let args: Vec<&SyntaxNode> = args_node.children().collect();
    let content_nodes = collect_lr_content_nodes(&args);

    if content_nodes.is_empty() {
        ctx.push("\\left(\\right)");
        return;
    }

    convert_lr_content_nodes(&content_nodes, ctx);
}

/// Collect lr() content nodes while preserving separators
fn collect_lr_content_nodes<'a>(args: &'a [&'a SyntaxNode]) -> Vec<&'a SyntaxNode> {
    let mut content = Vec::new();

    for child in args {
        match child.kind() {
            SyntaxKind::LeftParen | SyntaxKind::RightParen | SyntaxKind::Space => {}
            SyntaxKind::Math | SyntaxKind::MathDelimited => {
                push_lr_inner_nodes(child, &mut content);
            }
            SyntaxKind::Comma | SyntaxKind::Semicolon => {
                content.push(*child);
            }
            _ => {
                if is_content_node(child) {
                    content.push(*child);
                }
            }
        }
    }

    content
}

fn push_lr_inner_nodes<'a>(node: &'a SyntaxNode, content: &mut Vec<&'a SyntaxNode>) {
    for inner in node.children().filter(|n| n.kind() != SyntaxKind::Space) {
        if inner.kind() == SyntaxKind::MathDelimited {
            for nested in inner.children().filter(|n| n.kind() != SyntaxKind::Space) {
                content.push(nested);
            }
        } else {
            content.push(inner);
        }
    }
}

/// Convert lr() content nodes with delimiter detection.
/// Only looks at edge nodes for delimiters - does NOT scan interior to avoid
/// mistakenly treating inner symbols as delimiters and losing content.
fn convert_lr_content_nodes(content: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let left_delim = find_lr_edge_delimiter(content, true);
    let right_delim = find_lr_edge_delimiter(content, false);

    if left_delim.is_some() || right_delim.is_some() {
        let left_text = left_delim
            .as_ref()
            .map(|delim| delim.text.as_str())
            .unwrap_or(".");
        let right_text = right_delim
            .as_ref()
            .map(|delim| delim.text.as_str())
            .unwrap_or(".");

        // Output \left<first_delim>
        ctx.push("\\left");
        let left_latex = get_latex_delimiter(left_text, true);
        ctx.push(left_latex);
        set_last_token_after_delimiter(ctx, left_latex);

        for (idx, child) in content.iter().enumerate() {
            if should_skip_lr_index(idx, left_delim.as_ref(), right_delim.as_ref()) {
                continue;
            }
            convert_math_node(child, ctx);
        }

        // Output \right<last_delim>
        ctx.push("\\right");
        ctx.push(get_latex_delimiter(right_text, false));
    } else {
        // No recognizable delimiters at edges, use default parentheses
        // and output ALL content (safe fallback, no data loss)
        ctx.push("\\left(");
        for child in content {
            convert_math_node(child, ctx);
        }
        ctx.push("\\right)");
    }
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

/// Convert Unicode math characters in text to LaTeX commands
fn convert_unicode_in_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(latex) = UNICODE_TO_LATEX.get(&ch) {
            result.push_str(latex);
            // Add space if next char is alphanumeric to prevent command merging
            if chars.peek().map(|c| c.is_alphanumeric()).unwrap_or(false) {
                result.push(' ');
            }
        } else {
            result.push(ch);
        }
    }

    result
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

/// Output subscript/superscript content with proper bracing
fn output_subscript_content(content: &str, ctx: &mut ConvertContext) {
    if content.is_empty() {
        return;
    }
    // Single alphanumeric character doesn't need braces
    if content.len() == 1
        && content
            .chars()
            .next()
            .map(|c| c.is_alphanumeric())
            .unwrap_or(false)
    {
        ctx.push(content);
    } else {
        ctx.push("{");
        ctx.push(content);
        ctx.push("}");
    }
}

/// Extract content for subscript/superscript, unwrapping parentheses if they're just grouping
/// In Typst, ^(2) means superscript 2, not (2) with parens
fn extract_subscript_content(node: &SyntaxNode) -> String {
    let children: Vec<&SyntaxNode> = node.children().collect();

    // Check if it's a Math node or MathDelimited with parentheses for grouping
    // Typst parses x_(1:T) as: Math -> [LeftParen, Math(content), RightParen]
    if matches!(node.kind(), SyntaxKind::MathDelimited | SyntaxKind::Math) {
        // Find opening and closing delimiters (ignoring whitespace)
        let open_idx = children
            .iter()
            .position(|c| c.kind() != SyntaxKind::Space && c.kind() != SyntaxKind::Math);
        let close_idx = children
            .iter()
            .rposition(|c| c.kind() != SyntaxKind::Space && c.kind() != SyntaxKind::Math);

        if let (Some(oi), Some(ci)) = (open_idx, close_idx) {
            if ci > oi {
                let open_kind = children[oi].kind();
                let close_kind = children[ci].kind();
                let open_text = children[oi].text().as_str();
                let close_text = children[ci].text().as_str();

                // Check for LeftParen/RightParen kinds OR "("/")" text
                let is_paren_open = open_kind == SyntaxKind::LeftParen || open_text == "(";
                let is_paren_close = close_kind == SyntaxKind::RightParen || close_text == ")";

                // If it's parentheses used for grouping, extract inner content without the parens
                if is_paren_open && is_paren_close {
                    // Extract the content inside the parens
                    let mut inner_ctx = ConvertContext::new();
                    inner_ctx.in_math = true;

                    for child in &children[oi + 1..ci] {
                        convert_math_node(child, &mut inner_ctx);
                    }

                    return inner_ctx.finalize().trim().to_string();
                }
            }
        }
    }

    // For other node types, convert normally
    let mut ctx = ConvertContext::new();
    ctx.in_math = true;
    convert_math_node(node, &mut ctx);
    let result = ctx.finalize().trim().to_string();

    // Clean up \left( and \right) if they exist (these are grouping parens, not math delimiters)
    if result.starts_with("\\left(") && result.ends_with("\\right)") {
        let inner = result
            .trim_start_matches("\\left(")
            .trim_end_matches("\\right)")
            .trim();
        return inner.to_string();
    }

    result
}
