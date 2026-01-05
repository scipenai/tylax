//! Utility functions for LaTeX to Typst conversion
//!
//! This module contains pure utility functions that don't depend on converter state.

use mitex_parser::syntax::{SyntaxElement, SyntaxKind, SyntaxNode};

// =============================================================================
// Text Processing Utilities
// =============================================================================

/// Normalize table cell text by cleaning up whitespace issues
/// This handles cases where MiTeX tokenizer may have added extra spaces between characters
pub fn normalize_cell_text(text: &str) -> String {
    // If the text contains special Typst cell markers, leave it as is
    if text.starts_with("___TYPST_CELL___:") {
        return text.to_string();
    }

    let mut result = String::new();
    let mut chars = text.chars().peekable();
    let mut last_was_space = false;

    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            // Check if this might be spurious space between word characters
            // Pattern: "letter space letter" with single char before space suggests bad tokenization
            if !result.is_empty() && !last_was_space {
                // Look ahead to see if next non-space is a letter
                let next_non_space = chars.clone().find(|c| !c.is_whitespace());

                // Check if last char in result is part of a word and next char continues it
                let last_char = result.chars().last();
                if let (Some(last), Some(next)) = (last_char, next_non_space) {
                    // If both are alphanumeric characters and result ends with a single character after space,
                    // this might indicate spurious tokenization (e.g. "T e X").
                    // We only collapse single-char spaces between alphanumeric characters to preserve intentional spacing.
                    if ch == ' ' && last.is_alphanumeric() && next.is_alphanumeric() {
                        // Check if this looks like broken-up text (single chars separated by spaces)
                        // by looking at context - if result has "X " pattern repeatedly, collapse
                        let result_chars: Vec<char> = result.chars().collect();
                        if result_chars.len() >= 2 {
                            let prev_prev = result_chars.get(result_chars.len() - 2);
                            // If pattern is: "char space char space" - likely broken tokenization
                            if prev_prev == Some(&' ') {
                                // Skip this space to collapse
                                last_was_space = true;
                                continue;
                            }
                        }
                    }
                }
                result.push(' ');
                last_was_space = true;
            } else if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(ch);
            last_was_space = false;
        }
    }

    result.trim().to_string()
}

/// Sanitize a label name for Typst compatibility
/// Converts colons to hyphens since Typst labels work better with hyphens
pub fn sanitize_label(label: &str) -> String {
    label.replace([':', ' ', '_'], "-")
}

/// Convert integer to Roman numeral
pub fn to_roman_numeral(num: usize) -> String {
    if num == 0 {
        return "0".to_string();
    }

    let values = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut result = String::new();
    let mut n = num;

    for (value, symbol) in values {
        while n >= value {
            result.push_str(symbol);
            n -= value;
        }
    }

    result
}

// =============================================================================
// Command Protection/Restoration
// =============================================================================

/// Protect zero-argument commands from being lost during parsing.
/// Replaces specific commands with Unicode private use area placeholders that survive the MiTeX parser.
pub fn protect_zero_arg_commands(input: &str) -> String {
    let mut result = input.to_string();
    // Use text placeholders wrapped in Private Use Area characters to avoid parser interference.
    result = result.replace("\\today", "\u{E000}TODAY\u{E001}");
    result = result.replace("\\LaTeX", "\u{E000}LATEX\u{E001}");
    result = result.replace("\\TeX", "\u{E000}TEX\u{E001}");
    result = result.replace("\\XeTeX", "\u{E000}XETEX\u{E001}");
    result = result.replace("\\LuaTeX", "\u{E000}LUATEX\u{E001}");
    result = result.replace("\\pdfTeX", "\u{E000}PDFTEX\u{E001}");
    result = result.replace("\\BibTeX", "\u{E000}BIBTEX\u{E001}");
    result
}

/// Restore protected commands after conversion
pub fn restore_protected_commands(input: &str) -> String {
    let mut result = input.to_string();
    result = result.replace("\u{E000}TODAY\u{E001}", "#datetime.today().display()");
    result = result.replace("\u{E000}LATEX\u{E001}", "LaTeX");
    result = result.replace("\u{E000}TEX\u{E001}", "TeX");
    result = result.replace("\u{E000}XETEX\u{E001}", "XeTeX");
    result = result.replace("\u{E000}LUATEX\u{E001}", "LuaTeX");
    result = result.replace("\u{E000}PDFTEX\u{E001}", "pdfTeX");
    result = result.replace("\u{E000}BIBTEX\u{E001}", "BibTeX");
    result
}

// =============================================================================
// Whitespace Cleaning
// =============================================================================

/// Clean up excessive whitespace in the output.
///
/// This function performs the following normalizations:
/// - Removes leading/trailing blank lines
/// - Collapses multiple consecutive blank lines into one (preserving paragraph breaks)
/// - Trims trailing whitespace on each line
/// - Preserves code blocks (```...```) exactly as-is
pub fn clean_whitespace(input: &str) -> String {
    let mut result = String::new();
    let mut consecutive_newlines = 0;
    let mut in_code_block = false;

    for line in input.lines() {
        let trimmed = line.trim_end();

        // Check for code block delimiters (``` with optional language)
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            // Output code block delimiter as-is
            result.push_str(line);
            result.push('\n');
            consecutive_newlines = 1;
            continue;
        }

        // Inside code block: preserve everything as-is
        if in_code_block {
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // Outside code block: apply whitespace cleanup
        if trimmed.is_empty() {
            consecutive_newlines += 1;
            // Allow at most one blank line (which is two newlines in a row)
            if consecutive_newlines <= 2 {
                result.push('\n');
            }
        } else {
            // Non-empty line - reset counter and output
            result.push_str(trimmed);
            result.push('\n');
            consecutive_newlines = 1; // Count this line's newline
        }
    }

    // Remove leading blank lines
    let result = result.trim_start_matches('\n').to_string();
    // Remove trailing blank lines but keep one final newline
    let result = result.trim_end().to_string();
    if result.is_empty() {
        result
    } else {
        result + "\n"
    }
}

// =============================================================================
// AST Text Extraction
// =============================================================================

/// Extract all text from a node (strips braces - use for math/simple content)
pub fn extract_node_text(node: &SyntaxNode) -> String {
    let mut text = String::new();
    for child in node.children_with_tokens() {
        match child {
            SyntaxElement::Token(t) => {
                if !matches!(
                    t.kind(),
                    SyntaxKind::TokenLBrace
                        | SyntaxKind::TokenRBrace
                        | SyntaxKind::TokenLBracket
                        | SyntaxKind::TokenRBracket
                ) {
                    text.push_str(t.text());
                }
            }
            SyntaxElement::Node(n) => {
                text.push_str(&extract_node_text(&n));
            }
        }
    }
    text
}

/// Extract all text from a node preserving braces (use for text content with commands)
pub fn extract_node_text_with_braces(node: &SyntaxNode) -> String {
    let mut text = String::new();
    for child in node.children_with_tokens() {
        match child {
            SyntaxElement::Token(t) => {
                text.push_str(t.text());
            }
            SyntaxElement::Node(n) => {
                text.push_str(&extract_node_text_with_braces(&n));
            }
        }
    }
    text
}

/// Extract text content from an argument node
pub fn extract_arg_content(node: &SyntaxNode) -> String {
    let mut content = String::new();
    for child in node.children_with_tokens() {
        match child.kind() {
            SyntaxKind::TokenLBrace
            | SyntaxKind::TokenRBrace
            | SyntaxKind::TokenLBracket
            | SyntaxKind::TokenRBracket => continue,
            SyntaxKind::ItemCurly | SyntaxKind::ItemBracket => {
                if let SyntaxElement::Node(n) = child {
                    content.push_str(&extract_node_text(&n));
                }
            }
            _ => {
                if let SyntaxElement::Token(t) = child {
                    content.push_str(t.text());
                } else if let SyntaxElement::Node(n) = child {
                    content.push_str(&extract_node_text(&n));
                }
            }
        }
    }
    content.trim().to_string()
}

/// Extract argument content preserving inner braces but stripping outermost
pub fn extract_arg_content_with_braces(node: &SyntaxNode) -> String {
    let mut content = String::new();
    for child in node.children_with_tokens() {
        match child.kind() {
            // Skip the outermost braces/brackets (direct tokens)
            SyntaxKind::TokenLBrace
            | SyntaxKind::TokenRBrace
            | SyntaxKind::TokenLBracket
            | SyntaxKind::TokenRBracket => continue,
            // For ItemCurly/ItemBracket, extract their *inner* content (skip their braces)
            SyntaxKind::ItemCurly | SyntaxKind::ItemBracket => {
                if let SyntaxElement::Node(n) = child {
                    // Recurse but skip the curly/bracket's own braces
                    content.push_str(&extract_curly_inner_content(&n));
                }
            }
            _ => {
                if let SyntaxElement::Token(t) = child {
                    content.push_str(t.text());
                } else if let SyntaxElement::Node(n) = child {
                    content.push_str(&extract_node_text_with_braces(&n));
                }
            }
        }
    }
    content.trim().to_string()
}

/// Extract inner content of a curly/bracket node, skipping its braces
pub fn extract_curly_inner_content(node: &SyntaxNode) -> String {
    let mut content = String::new();
    for child in node.children_with_tokens() {
        match child.kind() {
            // Skip the braces of this curly node
            SyntaxKind::TokenLBrace
            | SyntaxKind::TokenRBrace
            | SyntaxKind::TokenLBracket
            | SyntaxKind::TokenRBracket => continue,
            _ => {
                if let SyntaxElement::Token(t) = child {
                    content.push_str(t.text());
                } else if let SyntaxElement::Node(n) = child {
                    // For nested structures, preserve their braces
                    content.push_str(&extract_node_text_with_braces(&n));
                }
            }
        }
    }
    content
}

// =============================================================================
// Caption Text Conversion
// =============================================================================

/// Convert caption/title/author text that may contain inline math and formatting commands
/// Handles LaTeX math mode ($...$) and text formatting commands
pub fn convert_caption_text(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            // Collect math content until closing $
            let mut math_content = String::new();
            while let Some(&next) = chars.peek() {
                if next == '$' {
                    chars.next(); // consume closing $
                    break;
                }
                math_content.push(chars.next().unwrap());
            }
            // Convert the math content
            let converted = super::latex_math_to_typst(&math_content);
            result.push('$');
            result.push_str(&converted);
            result.push('$');
        } else if ch == '\\' {
            // Handle backslash commands in text mode
            let mut cmd = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphabetic() {
                    cmd.push(chars.next().unwrap());
                } else {
                    break;
                }
            }

            // Check if this command takes a braced argument
            let has_arg = crate::data::symbols::is_caption_text_command(&cmd);

            // Extract argument content if present
            let arg_content = if has_arg {
                // Skip whitespace
                while let Some(&' ') = chars.peek() {
                    chars.next();
                }
                // Check for opening brace
                if chars.peek() == Some(&'{') {
                    chars.next(); // consume '{'
                    let mut content = String::new();
                    let mut brace_depth = 1;
                    for c in chars.by_ref() {
                        if c == '{' {
                            brace_depth += 1;
                            content.push(c);
                        } else if c == '}' {
                            brace_depth -= 1;
                            if brace_depth == 0 {
                                break;
                            }
                            content.push(c);
                        } else {
                            content.push(c);
                        }
                    }
                    Some(content)
                } else {
                    None
                }
            } else {
                None
            };

            // Convert common text-mode commands
            match cmd.as_str() {
                "textbf" | "bf" => {
                    result.push('*');
                    if let Some(content) = arg_content {
                        result.push_str(&convert_caption_text(&content));
                    }
                    result.push('*');
                }
                "textit" | "it" | "emph" => {
                    result.push('_');
                    if let Some(content) = arg_content {
                        result.push_str(&convert_caption_text(&content));
                    }
                    result.push('_');
                }
                "texttt" => {
                    result.push('`');
                    if let Some(content) = arg_content {
                        result.push_str(&content); // Don't recurse for monospace
                    }
                    result.push('`');
                }
                "textsc" => {
                    result.push_str("#smallcaps[");
                    if let Some(content) = arg_content {
                        result.push_str(&convert_caption_text(&content));
                    }
                    result.push(']');
                }
                "underline" => {
                    result.push_str("#underline[");
                    if let Some(content) = arg_content {
                        result.push_str(&convert_caption_text(&content));
                    }
                    result.push(']');
                }
                "textrm" | "text" | "mbox" | "hbox" => {
                    // Just include the content
                    if let Some(content) = arg_content {
                        result.push_str(&convert_caption_text(&content));
                    }
                }
                "textsf" => {
                    result.push_str("#text(font: \"sans-serif\")[");
                    if let Some(content) = arg_content {
                        result.push_str(&convert_caption_text(&content));
                    }
                    result.push(']');
                }
                // Date/time commands
                "today" => result.push_str("#datetime.today().display()"),

                // LaTeX logo commands
                "LaTeX" => result.push_str("LaTeX"),
                "TeX" => result.push_str("TeX"),
                "XeTeX" => result.push_str("XeTeX"),
                "LuaTeX" => result.push_str("LuaTeX"),
                "pdfTeX" => result.push_str("pdfTeX"),
                "BibTeX" => result.push_str("BibTeX"),

                // Common escapes
                "&" => result.push('&'),
                "%" => result.push('%'),
                "_" => result.push_str("\\_"), // _ needs escaping in text mode
                "#" => result.push_str("\\#"), // # needs escaping in Typst
                "$" => result.push_str("\\$"), // $ needs escaping in Typst
                "{" => result.push('{'),
                "}" => result.push('}'),
                "\\" => result.push_str("\\ "), // line break
                "" => {
                    // Just a backslash followed by non-alpha (like \\ or \&)
                    // Already consumed, do nothing
                }
                _ => {
                    // For unknown commands, skip the backslash (don't output raw LaTeX)
                    // If there's an argument, output its content
                    if let Some(content) = arg_content {
                        result.push_str(&convert_caption_text(&content));
                    }
                    // Otherwise, just skip the unknown command
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}
