//! Markup and command handling for LaTeX to Typst conversion
//!
//! This module handles LaTeX commands like \section, \textbf, \cite, etc.

use mitex_parser::syntax::{CmdItem, SyntaxElement};
use rowan::ast::AstNode;
use std::fmt::Write;

use crate::data::colors::parse_color_expression;
use crate::data::constants::{CodeBlockOptions, LANGUAGE_MAP};
use crate::data::extended_symbols::EXTENDED_SYMBOLS;
use crate::data::maps::TEX_COMMAND_SPEC;
use crate::data::shorthands::apply_shorthand;
use crate::data::symbols::{
    BIBLATEX_COMMANDS, CHAR_COMMANDS, GREEK_LETTERS, LETTER_COMMANDS, MISC_SYMBOLS, NAME_COMMANDS,
    TEXT_FORMAT_COMMANDS,
};
use mitex_spec::CommandSpecItem;

use super::context::{
    ConversionMode, EnvironmentContext, LatexConverter, MacroDef, PendingOperator,
};
use super::utils::{sanitize_label, to_roman_numeral};
use crate::features::images::ImageAttributes;

/// Convert a command symbol (e.g., \alpha, \beta, or special chars like \$, \%)
pub fn convert_command_sym(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    if let SyntaxElement::Token(t) = elem {
        let text = t.text();

        // Skip \begin and \end - these are handled by environment conversion
        if text == "\\begin" || text == "\\end" {
            return;
        }

        // Get the character(s) after backslash
        let cmd_name = &text[1..];

        if cmd_name.is_empty() {
            return;
        }

        // Skip in preamble for most symbols (but not escape chars)
        let is_escape_char = matches!(
            cmd_name,
            "$" | "%" | "&" | "#" | "_" | "{" | "}" | "~" | "@" | "*"
        );
        if conv.state.in_preamble && !is_escape_char {
            return;
        }

        // Handle special character escapes that need proper handling for Typst
        match cmd_name {
            // Characters that need escaping in Typst
            "$" => {
                output.push_str("\\$"); // $ starts math mode in Typst
                return;
            }
            "#" => {
                output.push_str("\\#"); // # starts code mode in Typst
                return;
            }
            "_" => {
                if matches!(conv.state.mode, ConversionMode::Math) {
                    output.push('_');
                } else {
                    output.push_str("\\_"); // _ causes emphasis in text
                }
                return;
            }
            "*" => {
                if matches!(conv.state.mode, ConversionMode::Math) {
                    output.push('*');
                } else {
                    output.push_str("\\*"); // * causes emphasis in text
                }
                return;
            }
            "@" => {
                output.push_str("\\@"); // @ is special in Typst
                return;
            }
            // Characters safe to output directly
            "%" => {
                output.push('%');
                return;
            }
            "&" => {
                output.push('&');
                return;
            }
            "{" => {
                output.push('{');
                return;
            }
            "}" => {
                output.push('}');
                return;
            }
            "~" => {
                output.push('~');
                return;
            }
            _ => {}
        }

        // Handle special options
        if cmd_name == "infty" && conv.state.options.infty_to_oo {
            output.push_str("oo");
            output.push(' ');
            return;
        }

        // Try symbol maps
        if let Some(typst) = lookup_symbol(cmd_name) {
            output.push_str(typst);
            output.push(' ');
        } else {
            // Pass through unknown symbols
            output.push_str(cmd_name);
            output.push(' ');
        }
    }
}

/// Look up a symbol in various symbol tables
fn lookup_symbol(name: &str) -> Option<&'static str> {
    // First check TEX_COMMAND_SPEC for aliases - these give proper Typst symbol names
    if let Some(CommandSpecItem::Cmd(shape)) = TEX_COMMAND_SPEC.get(name) {
        if let Some(ref alias) = shape.alias {
            // Return static string - we leak a bit here but it's acceptable
            return Some(Box::leak(alias.clone().into_boxed_str()));
        }
    }

    // Check extended symbols
    if let Some(typst) = EXTENDED_SYMBOLS.get(name) {
        return Some(*typst);
    }

    let key = format!("\\{}", name);

    // Check misc symbols
    if let Some(typst) = MISC_SYMBOLS.get(key.as_str()) {
        return Some(*typst);
    }

    // Check char commands (e.g., \textquoteleft)
    if let Some(typst) = CHAR_COMMANDS.get(key.as_str()) {
        return Some(*typst);
    }

    // Check Greek letters
    if let Some(typst) = GREEK_LETTERS.get(key.as_str()) {
        return Some(*typst);
    }

    // Check letter commands (e.g., \i, \j)
    if let Some(typst) = LETTER_COMMANDS.get(key.as_str()) {
        return Some(*typst);
    }

    // Check biblatex commands
    if let Some(typst) = BIBLATEX_COMMANDS.get(key.as_str()) {
        return Some(*typst);
    }

    // Check name commands (e.g., \LaTeX, \TeX)
    if let Some(typst) = NAME_COMMANDS.get(key.as_str()) {
        return Some(*typst);
    }

    None
}

/// Convert a LaTeX command
pub fn convert_command(conv: &mut LatexConverter, elem: SyntaxElement, output: &mut String) {
    let node = match &elem {
        SyntaxElement::Node(n) => n.clone(),
        _ => return,
    };

    let cmd = match CmdItem::cast(node.clone()) {
        Some(c) => c,
        None => return,
    };

    let cmd_name = cmd.name_tok().map(|t| t.text().to_string());
    let cmd_str = cmd_name.as_deref().unwrap_or("");

    // Skip empty commands
    if cmd_str.is_empty() {
        return;
    }

    // Remove leading backslash for matching
    let base_name = cmd_str.trim_start_matches('\\');

    // Handle preamble commands
    if conv.state.in_preamble {
        match base_name {
            "documentclass" => {
                if let Some(class) = conv.get_required_arg(&cmd, 0) {
                    conv.state.document_class = Some(class);
                }
                return;
            }
            "title" => {
                conv.state.title = conv.extract_metadata_arg(&cmd);
                return;
            }
            "author" => {
                conv.state.author = conv.extract_metadata_arg(&cmd);
                return;
            }
            "date" => {
                conv.state.date = conv.extract_metadata_arg(&cmd);
                return;
            }
            "newcommand" | "renewcommand" | "providecommand" => {
                handle_newcommand(conv, &cmd);
                return;
            }
            "def" => {
                handle_def(conv, &cmd);
                return;
            }
            "newacronym" => {
                handle_newacronym(conv, &cmd);
                return;
            }
            "newglossaryentry" => {
                handle_newglossaryentry(conv, &cmd);
                return;
            }
            // Preamble/setup commands to ignore
            "usepackage" | "RequirePackage" | "input" | "include" | "includeonly"
            | "bibliography" | "bibliographystyle" | "maketitle" | "pagestyle" 
            | "thispagestyle" | "pagenumbering" | "setcounter" | "addtocounter" 
            | "setlength" | "addtolength" | "newtheorem" | "theoremstyle" 
            | "allowdisplaybreaks" | "numberwithin" | "DeclareMathOperator"
            | "DeclarePairedDelimiter" | "sisetup" | "NewDocumentCommand"
            | "RenewDocumentCommand" | "ProvideDocumentCommand" | "DeclareDocumentCommand"
            // Layout and spacing
            | "geometry" | "onehalfspacing" | "doublespacing" | "singlespacing"
            | "linespread" | "baselinestretch" | "parindent" | "parskip"
            // AtBegin/AtEnd hooks
            | "makeatletter" | "makeatother" | "AtBeginDocument" | "AtEndDocument"
            // Environment definitions
            | "newenvironment" | "renewenvironment"
            // Hyperref and colors
            | "hypersetup" | "definecolor" | "colorlet"
            // Graphics
            | "graphicspath" | "DeclareGraphicsExtensions"
            // Captions and floats
            | "captionsetup" | "floatsetup"
            // Lists
            | "setlist"
            // Glossary and acronyms
            | "makeglossaries" | "printglossaries"
            // Table of contents
            | "tableofcontents" | "listoffigures" | "listoftables"
            // Citations
            | "nocite"
            // TeX primitives and conditionals
            | "newif" | "fi" | "else" | "or" 
            | "begingroup" | "endgroup" | "relax"
            // Keywords and IEEEtran specific commands
            | "IEEEkeywords" | "keywords" | "IEEEPARstart" | "IEEEpeerreviewmaketitle"
            // More preamble commands
            | "DeclareOption" | "ProcessOptions" | "ExecuteOptions"
            | "PackageWarning" | "PackageError" | "ClassWarning" | "ClassError"
            // Font and encoding setup
            | "DeclareRobustCommand" | "newrobustcmd" | "robustify"
            | "DeclareFontFamily" | "DeclareFontShape" | "DeclareSymbolFont"
            | "SetSymbolFont" | "DeclareMathSymbol" | "DeclareMathOperator*"
            // Listings and minted setup
            | "lstset" | "lstdefinestyle" | "lstdefinelanguage"
            | "usemintedstyle" | "setminted"
            // Additional formatting commands
            | "protect" | "unexpanded" | "expandafter" | "csname" | "endcsname"
            | "let" | "gdef" | "edef" | "xdef" | "futurelet"
            // Conditional flags (often used in preambles)
            | "iftrue" | "iffalse" | "ifx" | "ifnum" | "ifdim" | "ifcat" | "ifmmode" => {
                return;
            }
            _ => {}
        }
    }

    // Check for user-defined macros
    if let Some(macro_def) = conv.state.macros.get(base_name).cloned() {
        let expanded = expand_user_macro(conv, &cmd, &macro_def);
        output.push_str(&expanded);
        return;
    }

    // Handle document commands
    match base_name {
        // Section commands - Part gets special formatting with Roman numerals
        "part" => {
            let title = conv
                .convert_required_arg(&cmd, 0)
                .or_else(|| conv.get_required_arg(&cmd, 0));
            let part_num = conv.state.next_counter("part");
            let roman = to_roman_numeral(part_num as usize);
            output.push_str("\n#v(2em)\n");
            output.push_str("#align(center)[\n");
            let _ = writeln!(output, "  #text(1.2em)[Part {}]", roman);
            let _ = writeln!(output, "  #v(0.5em)");
            if let Some(t) = title {
                let _ = writeln!(output, "  #text(2em, weight: \"bold\")[{}]", t);
            }
            output.push_str("]\n");
            output.push_str("#v(2em)\n\n");
        }
        "chapter" => {
            let title = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            output.push('\n');
            output.push_str("= ");
            output.push_str(&title);
            output.push('\n');
        }
        // Sectioning - adjust level based on documentclass
        "section" => {
            // article: section = level 1 (=), report/book: section = level 2 (==)
            let base_level = if conv.state.document_class.as_deref() == Some("article") {
                0
            } else {
                1
            };
            convert_section(conv, &cmd, base_level, output);
        }
        "subsection" => {
            let base_level = if conv.state.document_class.as_deref() == Some("article") {
                1
            } else {
                2
            };
            convert_section(conv, &cmd, base_level, output);
        }
        "subsubsection" => {
            let base_level = if conv.state.document_class.as_deref() == Some("article") {
                2
            } else {
                3
            };
            convert_section(conv, &cmd, base_level, output);
        }
        "paragraph" => {
            let base_level = if conv.state.document_class.as_deref() == Some("article") {
                3
            } else {
                4
            };
            convert_section(conv, &cmd, base_level, output);
        }
        "subparagraph" => {
            let title = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "\n_{}_\n", title);
        }

        // Text formatting
        "textbf" | "bf" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "*{}*", content);
        }
        "textit" | "it" | "emph" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "_{}_", content);
        }
        "texttt" | "tt" => {
            let content = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "`{}`", content);
        }
        "underline" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#underline[{}]", content);
        }
        "textsc" | "sc" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#smallcaps[{}]", content);
        }
        "textsuperscript" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#super[{}]", content);
        }
        "textsubscript" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#sub[{}]", content);
        }
        // Text in math - these commands output text in math mode
        "text" | "textrm" | "textup" | "textnormal" => {
            if let Some(arg) = conv.get_required_arg(&cmd, 0) {
                let _ = write!(output, "\"{}\" ", arg);
            }
        }

        // Labels and references
        "label" => {
            // Skip label output if we're inside equation/align environments
            // because those environments handle labels at the end of the math block
            if conv.state.is_inside(&EnvironmentContext::Equation)
                || conv.state.is_inside(&EnvironmentContext::Align)
            {
                return;
            }
            let label = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let clean_label = sanitize_label(&label);
            let _ = write!(output, "<{}>", clean_label);
        }
        "ref" | "autoref" | "cref" | "Cref" => {
            let label = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let clean_label = sanitize_label(&label);
            let _ = write!(output, "@{}", clean_label);
        }
        "eqref" => {
            let label = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let sanitized = sanitize_label(&label);
            if !sanitized.starts_with("eq-") {
                let _ = write!(output, "@eq-{}", sanitized);
            } else {
                let _ = write!(output, "@{}", sanitized);
            }
        }
        "pageref" => {
            let label = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let clean_label = sanitize_label(&label);
            let _ = write!(output, "#locate(loc => {{@{}.page()}})", clean_label);
        }
        "nameref" => {
            let label = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let clean_label = sanitize_label(&label);
            let _ = write!(output, "@{}", clean_label);
        }

        // Citations - Full set from l2t.rs (40+ variants)
        "cite" | "Cite" | "citep" | "citep*" | "citet" | "citet*" | "citeal"
        | "citealp" | "citealp*" | "citealt" | "citealt*"
        | "autocite" | "Autocite" | "textcite" | "Textcite"
        | "parencite" | "Parencite" | "footcite" | "Footcite"
        | "smartcite" | "Smartcite" | "supercite" | "fullcite"
        | "footfullcite" | "cites" | "Cites" | "textcites" | "Textcites"
        | "parencites" | "Parencites" | "autocites" | "Autocites" => {
            let keys = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let opt = conv.get_optional_arg(&cmd, 0);
            for (i, key) in keys.split(',').enumerate() {
                if i > 0 {
                    output.push(' ');
                }
                let _ = write!(output, "@{}", key.trim());
            }
            if let Some(note) = opt {
                let _ = write!(output, " [{}]", note);
            }
        }
        "citeauthor" | "citeauthor*" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#cite(<{}>, form: \"author\")", key.trim());
        }
        "citeyear" | "citeyearpar" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#cite(<{}>, form: \"year\")", key.trim());
        }

        // URLs and hyperlinks
        "url" => {
            if let Some(url) = conv.get_required_arg(&cmd, 0) {
                let _ = write!(output, "#link(\"{}\")", url);
            }
        }
        "href" => {
            let url = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let text = conv.get_required_arg(&cmd, 1).unwrap_or_else(|| url.clone());
            let _ = write!(output, "#link(\"{}\")[{}]", url, text);
        }

        // Footnotes
        "footnote" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#footnote[{}]", content);
        }
        "footnotetext" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#footnote[{}]", content);
        }
        "footnotemark" => {
            output.push_str("#super[]");
        }

        // Graphics - use images module for proper parsing
        "includegraphics" => {
            let options = conv.get_optional_arg(&cmd, 0).unwrap_or_default();
            let path = conv.get_required_arg(&cmd, 0).unwrap_or_default();

            // Use the images module for proper parsing
            let attrs = ImageAttributes::parse(&options);
            let args = attrs.to_typst_args();

            if args.is_empty() {
                let _ = write!(output, "#image(\"{}\")", path);
            } else {
                let _ = write!(output, "#image(\"{}\", {})", path, args);
            }
        }

        // Caption
        "caption" => {
            let content = conv.get_converted_required_arg(&cmd, 0).unwrap_or_default();
            match conv.state.current_env() {
                EnvironmentContext::Figure => {
                    let _ = write!(output, "  )\n  #figure.caption[{}]\n", content);
                }
                EnvironmentContext::Table => {
                    let _ = write!(output, "  ), caption: [{}]", content);
                }
                _ => {
                    let _ = write!(output, "[{}]", content);
                }
            }
        }

        // List item
        "item" => {
            output.push('\n');
            for _ in 0..conv.state.indent {
                output.push(' ');
            }
            match conv.state.current_env() {
                EnvironmentContext::Enumerate => {
                    // Check for optional label
                    if let Some(label) = conv.get_optional_arg(&cmd, 0) {
                        let _ = write!(output, "+ [{}] ", label);
                    } else {
                        output.push_str("+ ");
                    }
                }
                EnvironmentContext::Description => {
                    if let Some(term) = conv.get_optional_arg(&cmd, 0) {
                        let _ = write!(output, "/ {}: ", term);
                    } else {
                        output.push_str("/ ");
                    }
                }
                _ => {
                    output.push_str("- ");
                }
            }
        }

        // Math operators (in math mode)
        // \operatorname and \operatorname* - handled via pending_op state machine
        "operatorname" | "operatorname*" => {
            // Try to get the argument (if parsed as part of the command)
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                // Clean up: remove spaces
                let clean_content: String =
                    content.chars().filter(|c| !c.is_whitespace()).collect();

                let op_name = if clean_content == "argmin" {
                    "argmin"
                } else if clean_content == "argmax" {
                    "argmax"
                } else {
                    &clean_content
                };

                // operatorname* usually implies limits
                let _ = write!(output, "limits(op(\"{}\")) ", op_name);
            } else {
                // Argument not captured, set pending state for next ItemCurly
                conv.state.pending_op = Some(PendingOperator { is_limits: false });
            }
        }

        // Math fractions
        "frac" => {
            let num = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let den = conv.convert_required_arg(&cmd, 1).unwrap_or_default();

            // Check if we can use slash notation
            if conv.state.options.frac_to_slash
                && conv.is_simple_term(&num)
                && conv.is_simple_term(&den)
            {
                let _ = write!(output, "{}/{}", num.trim(), den.trim());
            } else {
                let _ = write!(output, "frac({}, {})", num.trim(), den.trim());
            }
        }
        "dfrac" => {
            // dfrac always uses frac() for proper display style
            let num = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let den = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            let _ = write!(output, "display(frac({}, {}))", num.trim(), den.trim());
        }
        "tfrac" => {
            // tfrac might use slash if enabled and simple
            let num = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let den = conv.convert_required_arg(&cmd, 1).unwrap_or_default();

            if conv.state.options.frac_to_slash
                && conv.is_simple_term(&num)
                && conv.is_simple_term(&den)
            {
                let _ = write!(output, "{}/{}", num.trim(), den.trim());
            } else {
                let _ = write!(output, "inline(frac({}, {}))", num.trim(), den.trim());
            }
        }
        "cfrac" => {
            let num = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let den = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            let _ = write!(output, "frac({}, {})", num.trim(), den.trim());
        }

        // Math roots
        "sqrt" => {
            let opt = conv.get_optional_arg(&cmd, 0);
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(n) = opt {
                let _ = write!(output, "root({}, {})", n, content.trim());
            } else {
                let _ = write!(output, "sqrt({})", content.trim());
            }
        }

        // Math accents and decorations (with argument)
        "hat" | "widehat" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "hat({}) ", arg);
            }
        }
        "tilde" | "widetilde" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "tilde({}) ", arg);
            }
        }
        "bar" | "overline" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "overline({}) ", arg);
            }
        }
        "vec" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "arrow({}) ", arg);
            }
        }
        "dot" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "dot({}) ", arg);
            }
        }
        "overbrace" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "overbrace({}) ", arg);
            }
        }
        "underbrace" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "underbrace({}) ", arg);
            }
        }
        "ddot" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "dot.double({}) ", arg);
            }
        }
        "mathbf" => {
            // \mathbf{x} -> upright(bold(x)) for proper bold upright
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "upright(bold({})) ", content);
            }
        }
        "boldsymbol" | "bm" => {
            // \boldsymbol and \bm just use bold()
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "bold({}) ", content);
            }
        }
        "mathit" => {
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "italic({}) ", content);
            }
        }
        "mathrm" => {
            // Check for special case: \mathrm{d} -> dif (differential)
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                if content.trim() == "d" || content.trim() == "dif" {
                    output.push_str("dif ");
                } else {
                    let _ = write!(output, "upright({}) ", content);
                }
            }
        }
        "rm" => {
            // \rm is an old-style font switch (no braces)
            if let Some(content) = conv.get_required_arg(&cmd, 0) {
                let _ = write!(output, "upright({}) ", content);
            }
            // If no argument, just skip
        }
        "mathbb" => {
            if let Some(content) = conv.get_required_arg(&cmd, 0) {
                let content = content.trim();
                // Only use short forms for standard number sets that Typst supports as symbols
                if ["R", "Z", "N", "C", "Q"].contains(&content) {
                    let c = content.chars().next().unwrap();
                    let _ = write!(output, "{}{} ", c, c);
                } else {
                    let _ = write!(output, "bb({}) ", content);
                }
            }
        }
        "mathcal" => {
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "cal({}) ", content);
            }
        }
        "mathfrak" => {
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "frak({}) ", content);
            }
        }
        "mathsf" => {
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "sans({}) ", content);
            }
        }
        "mathtt" => {
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "mono({}) ", content);
            }
        }
        "mathscr" => {
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "scr({}) ", content);
            }
        }
        "cancel" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "cancel({})", content.trim());
        }
        // Boxed content - handle differently in math vs text mode
        "boxed" | "fbox" | "framebox" => {
            let arg = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if conv.state.mode == ConversionMode::Math {
                // In math mode, wrap with $...$ for math content
                let _ = write!(
                    output,
                    "#box(stroke: 0.5pt, inset: 2pt, baseline: 20%)[$ {} $] ",
                    arg.trim()
                );
            } else {
                // In text mode, output directly without math wrapper
                let _ = write!(
                    output,
                    "#box(stroke: 0.5pt, inset: 2pt)[{}] ",
                    arg.trim()
                );
            }
        }

        // Equation tag (custom numbering)
        "tag" | "tag*" => {
            // \tag{label} - custom equation number
            // In Typst, we can simulate this with right-aligned text
            if let Some(content) = conv.convert_required_arg(&cmd, 0) {
                // Use #h(1fr) to push to the right, wrap in parentheses
                let _ = write!(output, " #h(1fr) \"({})\"", content.trim());
            }
        }

        // siunitx commands
        "SI" | "si" => {
            let value = conv.get_required_arg(&cmd, 0);
            let unit = conv.get_required_arg(&cmd, 1);
            match (value, unit) {
                (Some(v), Some(u)) => {
                    let unit_str = conv.process_si_unit(&u);
                    let _ = write!(output, "${} space {}$", v, unit_str);
                }
                (None, Some(u)) => {
                    let unit_str = conv.process_si_unit(&u);
                    let _ = write!(output, "${}$", unit_str);
                }
                _ => {}
            }
        }
        "qty" => {
            let value = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let unit = conv.get_required_arg(&cmd, 1).unwrap_or_default();
            let unit_str = conv.process_si_unit(&unit);
            let _ = write!(output, "${} space {}$", value, unit_str);
        }
        "num" => {
            let value = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "${}$", value);
        }
        "unit" => {
            let unit = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let unit_str = conv.process_si_unit(&unit);
            let _ = write!(output, "${}$", unit_str);
        }
        "ang" => {
            let angle = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "${}°$", angle);
        }

        // Acronym commands - auto (first use = full, subsequent = short)
        "ac" | "gls" | "Ac" | "Gls" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some((text, _is_first)) = conv.state.use_acronym(&key) {
                let text = if base_name.starts_with('G') || base_name.starts_with("Ac") {
                    let mut chars = text.chars();
                    match chars.next() {
                        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                        None => text,
                    }
                } else {
                    text
                };
                output.push_str(&text);
            } else if let Some(name) = conv.state.get_glossary_name(&key) {
                output.push_str(&name);
            } else {
                output.push_str(&key);
            }
        }
        // Acronym commands - plural forms
        "glspl" | "acp" | "Glspl" | "Acp" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(acr) = conv.state.acronyms.get(&key) {
                let plural = acr.short_plural();
                let text = if base_name.starts_with('G') || base_name.starts_with("Ac") {
                    let mut chars = plural.chars();
                    match chars.next() {
                        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                        None => plural,
                    }
                } else {
                    plural
                };
                output.push_str(&text);
            } else {
                output.push_str(&key);
                output.push('s');
            }
        }
        // Acronym commands - short form only
        "acs" | "acrshort" | "Acs" | "Acrshort" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(short) = conv.state.get_acronym_short(&key) {
                let text = if base_name.starts_with("Acs") || base_name.starts_with("Acr") && base_name.chars().nth(3) == Some('s') {
                    let mut chars = short.chars();
                    match chars.next() {
                        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                        None => short,
                    }
                } else {
                    short
                };
                output.push_str(&text);
            } else {
                output.push_str(&key);
            }
        }
        // Acronym commands - long form only
        "acl" | "acrlong" | "Acl" | "Acrlong" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(long) = conv.state.get_acronym_long(&key) {
                let text = if base_name.starts_with("Acl") || base_name.starts_with("Acr") && base_name.chars().nth(3) == Some('l') {
                    let mut chars = long.chars();
                    match chars.next() {
                        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                        None => long,
                    }
                } else {
                    long
                };
                output.push_str(&text);
            } else {
                output.push_str(&key);
            }
        }
        // Acronym commands - full form (always)
        "acf" | "acrfull" | "Acf" | "Acrfull" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(full) = conv.state.get_acronym_full(&key) {
                let text = if base_name.starts_with("Acf") || base_name.starts_with("Acr") && base_name.chars().nth(3) == Some('f') {
                    let mut chars = full.chars();
                    match chars.next() {
                        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                        None => full,
                    }
                } else {
                    full
                };
                output.push_str(&text);
            } else {
                output.push_str(&key);
            }
        }
        // Glossary description
        "glsdesc" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(name) = conv.state.get_glossary_name(&key) {
                output.push_str(&name);
            } else if let Some(long) = conv.state.get_acronym_long(&key) {
                output.push_str(&long);
            } else {
                output.push_str(&key);
            }
        }
        // Plural full/short/long forms
        "acfp" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(acr) = conv.state.acronyms.get(&key) {
                output.push_str(&acr.full_plural());
            } else {
                output.push_str(&key);
                output.push('s');
            }
        }
        "acsp" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(acr) = conv.state.acronyms.get(&key) {
                output.push_str(&acr.short_plural());
            } else {
                output.push_str(&key);
                output.push('s');
            }
        }
        "aclp" => {
            let key = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(acr) = conv.state.acronyms.get(&key) {
                output.push_str(&acr.long_plural());
            } else {
                output.push_str(&key);
                output.push('s');
            }
        }

        // Spacing commands
        "hspace" | "hspace*" => {
            let dim = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#h({})", convert_dimension(&dim));
        }
        "vspace" | "vspace*" => {
            let dim = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#v({})", convert_dimension(&dim));
        }
        "quad" => output.push_str("  "),
        "qquad" => output.push_str("    "),
        "," | "thinspace" => output.push(' '),
        ";" | "thickspace" => output.push_str("  "),
        "!" | "negthinspace" => {}
        "enspace" => output.push(' '),

        // Line breaks
        "newline" | "linebreak" => {
            output.push_str("\\ ");
        }
        "par" | "bigskip" | "medskip" | "smallskip" => {
            output.push_str("\n\n");
        }

        // Special math symbols
        "infty" => {
            if conv.state.options.infty_to_oo {
                output.push_str("oo");
            } else {
                output.push_str("infinity");
            }
        }

        // Special characters
        "LaTeX" => output.push_str("LaTeX"),
        "TeX" => output.push_str("TeX"),
        "today" => output.push_str("#datetime.today().display()"),
        "ldots" | "dots" | "cdots" => output.push_str("..."),
        "copyright" => output.push('©'),
        "trademark" | "texttrademark" => output.push('™'),
        "registered" | "textregistered" => output.push('®'),
        "dag" | "dagger" => output.push('†'),
        "ddag" | "ddagger" => output.push('‡'),
        "S" => output.push('§'),
        "P" => output.push('¶'),
        "pounds" | "textsterling" => output.push('£'),
        "euro" => output.push('€'),
        "textbackslash" => output.push('\\'),
        "textasciitilde" => output.push('~'),
        "textasciicircum" => output.push('^'),
        "%" => output.push('%'),
        "&" => output.push('&'),
        "$" => output.push('$'),
        "#" => output.push('#'),
        "_" => output.push('_'),
        "{" => output.push('{'),
        "}" => output.push('}'),

        // Accents in text mode
        "`" => {
            // grave accent
            let content = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            output.push_str(&apply_text_accent(&content, '`'));
        }
        "'" => {
            // acute accent
            let content = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            output.push_str(&apply_text_accent(&content, '\''));
        }
        "^" => {
            // circumflex
            let content = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            output.push_str(&apply_text_accent(&content, '^'));
        }
        "~" => {
            // tilde
            let content = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            output.push_str(&apply_text_accent(&content, '~'));
        }
        "\"" => {
            // umlaut
            let content = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            output.push_str(&apply_text_accent(&content, '"'));
        }
        "c" => {
            // cedilla
            let content = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            output.push_str(&apply_cedilla(&content));
        }

        // Color commands (using parse_color_expression for proper color mapping)
        "textcolor" => {
            let color = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let content = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            let typst_color = parse_color_expression(&color);
            let _ = write!(output, "#text(fill: {})[{}]", typst_color, content);
        }
        "colorbox" => {
            let color = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let content = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            let typst_color = parse_color_expression(&color);
            let _ = write!(output, "#box(fill: {}, inset: 2pt)[{}]", typst_color, content);
        }
        "fcolorbox" => {
            let frame_color = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let bg_color = conv.get_required_arg(&cmd, 1).unwrap_or_default();
            let content = conv.convert_required_arg(&cmd, 2).unwrap_or_default();
            let typst_frame = parse_color_expression(&frame_color);
            let typst_bg = parse_color_expression(&bg_color);
            let _ = write!(
                output,
                "#box(fill: {}, stroke: {}, inset: 2pt)[{}]",
                typst_bg, typst_frame, content
            );
        }
        "highlight" | "hl" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "#highlight[{}]", content);
        }

        // Math limits and bounds - add trailing space to prevent merging (e.g. \arg\min -> "arg min")
        "lim" => output.push_str("lim "),
        "sup" => output.push_str("sup "),
        "inf" => output.push_str("inf "),
        "max" => output.push_str("max "),
        "min" => output.push_str("min "),
        "arg" => output.push_str("arg "),
        "det" => output.push_str("det "),
        "gcd" => output.push_str("gcd "),
        "lcm" => output.push_str("op(\"lcm\") "),
        "log" => output.push_str("log "),
        "ln" => output.push_str("ln "),
        "lg" => output.push_str("lg "),
        "exp" => output.push_str("exp "),
        "sin" => output.push_str("sin "),
        "cos" => output.push_str("cos "),
        "tan" => output.push_str("tan "),
        "cot" => output.push_str("cot "),
        "sec" => output.push_str("sec "),
        "csc" => output.push_str("csc "),
        "sinh" => output.push_str("sinh "),
        "cosh" => output.push_str("cosh "),
        "tanh" => output.push_str("tanh "),
        "coth" => output.push_str("coth "),
        "arcsin" => output.push_str("arcsin "),
        "arccos" => output.push_str("arccos "),
        "arctan" => output.push_str("arctan "),
        "Pr" => output.push_str("op(\"Pr\") "),
        "hom" => output.push_str("hom "),
        "ker" => output.push_str("ker "),
        "dim" => output.push_str("dim "),
        "deg" => output.push_str("deg "),

        // Big operators - trailing space prevents merging with following content
        "sum" => output.push_str("sum "),
        "prod" => output.push_str("product "),
        "int" => output.push_str("integral "),
        "iint" => output.push_str("integral.double "),
        "iiint" => output.push_str("integral.triple "),
        "oint" => output.push_str("integral.cont "),
        "bigcup" => output.push_str("union.big "),
        "bigcap" => output.push_str("sect.big "),
        "bigoplus" => output.push_str("plus.circle.big "),
        "bigotimes" => output.push_str("times.circle.big "),
        "bigsqcup" => output.push_str("union.sq.big "),
        "biguplus" => output.push_str("union.plus.big "),
        "bigvee" => output.push_str("or.big "),
        "bigwedge" => output.push_str("and.big "),
        "coprod" => output.push_str("product.co "),

        // Delimiters
        "left" | "right" | "bigl" | "bigr" | "Bigl" | "Bigr" | "biggl" | "biggr" | "Biggl"
        | "Biggr" | "middle" => {
            // These are handled by ItemLR
        }

        // Phantom and spacing - in math mode, use #hide() since hide() alone isn't a math function
        "phantom" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if conv.state.mode == ConversionMode::Math {
                let _ = write!(output, "#hide[$ {} $]", content.trim());
            } else {
                let _ = write!(output, "#hide[{}]", content);
            }
        }
        "hphantom" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if conv.state.mode == ConversionMode::Math {
                let _ = write!(output, "#hide[$ {} $]", content.trim());
            } else {
                let _ = write!(output, "#hide[{}]", content);
            }
        }
        "vphantom" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if conv.state.mode == ConversionMode::Math {
                let _ = write!(output, "#hide[$ {} $]", content.trim());
            } else {
                let _ = write!(output, "#hide[{}]", content);
            }
        }

        // Stacking - tex2typst style with limits()
        "overset" => {
            // \overset{top}{base} -> limits(base)^(top)
            // Special optimization: \overset{\text{def}}{=} -> eq.def
            let top = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let base = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            let top_trimmed = top.trim().replace("\"", "");
            if (top_trimmed == "def" || top_trimmed.contains("def"))
                && (base.trim() == "=" || base.trim() == "eq")
            {
                output.push_str("eq.def ");
            } else {
                let _ = write!(output, "limits({})^({}) ", base, top);
            }
        }
        "underset" => {
            // \underset{bottom}{base} -> limits(base)_(bottom)
            let bottom = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let base = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            let _ = write!(output, "limits({})_({}) ", base, bottom);
        }
        "stackrel" => {
            // \stackrel{top}{relation} -> limits(relation)^(top)
            let top = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let base = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            let _ = write!(output, "limits({})^({}) ", base, top);
        }
        "substack" => {
            // \substack{a \\ b} -> directly output content
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                output.push_str(&arg);
            }
        }

        // Protect / misc
        "protect" => {
            // ignore
        }
        "mbox" | "makebox" | "hbox" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            let _ = write!(output, "\"{}\"", content);
        }
        "raisebox" => {
            let _height = conv.get_required_arg(&cmd, 0);
            let content = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            output.push_str(&content);
        }
        "parbox" => {
            let _width = conv.get_required_arg(&cmd, 0);
            let content = conv.convert_required_arg(&cmd, 1).unwrap_or_default();
            output.push_str(&content);
        }
        "minipage" => {
            let content = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            output.push_str(&content);
        }

        // Table commands
        "hline" | "toprule" | "midrule" | "bottomrule" => {
            output.push_str("|||HLINE|||");
        }
        "cline" | "cmidrule" => {
            output.push_str("|||HLINE|||");
        }
        "multicolumn" => {
            let ncols = conv.get_required_arg(&cmd, 0).unwrap_or("1".to_string());
            let _align = conv.get_required_arg(&cmd, 1);
            let content = conv.convert_required_arg(&cmd, 2).unwrap_or_default();
            let _ = write!(output, "___TYPST_CELL___:table.cell(colspan: {})[{}]", ncols, content);
        }
        "multirow" => {
            let nrows = conv.get_required_arg(&cmd, 0).unwrap_or("1".to_string());
            let _width = conv.get_required_arg(&cmd, 1);
            let content = conv.convert_required_arg(&cmd, 2).unwrap_or_default();
            let _ = write!(output, "___TYPST_CELL___:table.cell(rowspan: {})[{}]", nrows, content);
        }

        // Extensible arrows with text above/below
        "xleftarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.l.long)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.l.long)^({}) ", above);
            }
        }
        "xrightarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.r.long)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.r.long)^({}) ", above);
            }
        }
        "xmapsto" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.r.long.bar)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.r.long.bar)^({}) ", above);
            }
        }
        "xleftrightarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.l.r.long)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.l.r.long)^({}) ", above);
            }
        }
        "xLeftarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.l.double.long)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.l.double.long)^({}) ", above);
            }
        }
        "xRightarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.r.double.long)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.r.double.long)^({}) ", above);
            }
        }
        "xLeftrightarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.l.r.double.long)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.l.r.double.long)^({}) ", above);
            }
        }
        "xhookleftarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.l.hook)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.l.hook)^({}) ", above);
            }
        }
        "xhookrightarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.r.hook)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.r.hook)^({}) ", above);
            }
        }
        "xtwoheadleftarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.l.twohead)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.l.twohead)^({}) ", above);
            }
        }
        "xtwoheadrightarrow" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrow.r.twohead)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrow.r.twohead)^({}) ", above);
            }
        }
        "xleftharpoonup" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(harpoon.lt)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(harpoon.lt)^({}) ", above);
            }
        }
        "xrightharpoonup" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(harpoon.rt)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(harpoon.rt)^({}) ", above);
            }
        }
        "xleftharpoondown" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(harpoon.lb)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(harpoon.lb)^({}) ", above);
            }
        }
        "xrightharpoondown" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(harpoon.rb)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(harpoon.rb)^({}) ", above);
            }
        }
        "xleftrightharpoons" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(harpoons.ltrb)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(harpoons.ltrb)^({}) ", above);
            }
        }
        "xrightleftharpoons" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(harpoons.rtlb)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(harpoons.rtlb)^({}) ", above);
            }
        }
        "xtofrom" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(arrows.rl)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(arrows.rl)^({}) ", above);
            }
        }
        "xlongequal" => {
            let below = conv.get_optional_arg(&cmd, 0);
            let above = conv.convert_required_arg(&cmd, 0).unwrap_or_default();
            if let Some(b) = below {
                let _ = write!(output, "limits(eq.triple)^({})_({}) ", above, b);
            } else {
                let _ = write!(output, "limits(eq.triple)^({}) ", above);
            }
        }

        // Modular arithmetic
        "bmod" => output.push_str("mod "),
        "pmod" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "(mod {}) ", arg);
            }
        }
        "pod" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "({}) ", arg);
            }
        }

        // Math class commands (spacing/classification)
        "mathrel" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "class(\"relation\", {}) ", arg);
            }
        }
        "mathbin" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "class(\"binary\", {}) ", arg);
            }
        }
        "mathop" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "class(\"large\", {}) ", arg);
            }
        }
        "mathord" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "class(\"normal\", {}) ", arg);
            }
        }
        "mathopen" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "class(\"opening\", {}) ", arg);
            }
        }
        "mathclose" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "class(\"closing\", {}) ", arg);
            }
        }
        "mathpunct" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "class(\"punctuation\", {}) ", arg);
            }
        }
        "mathinner" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                output.push_str(&arg);
                output.push(' ');
            }
        }

        // Displaylines
        "displaylines" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                output.push_str(&arg);
            }
        }

        // Set notation (braket package)
        "set" | "Set" => {
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let _ = write!(output, "{{ {} }} ", arg);
            }
        }

        // Comparison aliases (with shorthand support)
        "ne" | "neq" => {
            let sym = apply_shorthand("eq.not", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "le" | "leq" => {
            let sym = apply_shorthand("lt.eq", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "ge" | "geq" => {
            let sym = apply_shorthand("gt.eq", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }

        // Common math operators/symbols
        "times" => output.push_str("times "),
        "cdot" => output.push_str("dot "),
        "div" => output.push_str("div "),
        "pm" => output.push_str("plus.minus "),
        "mp" => output.push_str("minus.plus "),
        "ast" => output.push_str("ast "),
        "star" => output.push_str("star "),
        "circ" => output.push_str("circle.small "),
        "bullet" => output.push_str("bullet "),

        // Arrows (with shorthand support)
        "rightarrow" | "to" => {
            let sym = apply_shorthand("arrow.r", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "leftarrow" => {
            let sym = apply_shorthand("arrow.l", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "leftrightarrow" => {
            let sym = apply_shorthand("arrow.l.r", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "Rightarrow" | "implies" => {
            let sym = apply_shorthand("arrow.r.double", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "Leftarrow" => {
            let sym = apply_shorthand("arrow.l.double", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "Leftrightarrow" | "iff" => {
            let sym = apply_shorthand("arrow.l.r.double", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "mapsto" => {
            let sym = apply_shorthand("arrow.r.bar", conv.state.options.prefer_shorthands);
            let _ = write!(output, "{} ", sym);
        }
        "uparrow" => output.push_str("arrow.t "),
        "downarrow" => output.push_str("arrow.b "),

        // Set operations
        "in" => output.push_str("in "),
        "notin" => output.push_str("in.not "),
        "subset" => output.push_str("subset "),
        "subseteq" => output.push_str("subset.eq "),
        "supset" => output.push_str("supset "),
        "supseteq" => output.push_str("supset.eq "),
        "cup" => output.push_str("union "),
        "cap" => output.push_str("sect "),
        "emptyset" | "varnothing" => output.push_str("emptyset "),

        // Logic
        "land" | "wedge" => output.push_str("and "),
        "lor" | "vee" => output.push_str("or "),
        "lnot" | "neg" => output.push_str("not "),
        "forall" => output.push_str("forall "),
        "exists" => output.push_str("exists "),

        // Relations
        "approx" => output.push_str("approx "),
        "sim" => output.push_str("tilde "),
        "simeq" => output.push_str("tilde.eq "),
        "cong" => output.push_str("tilde.equiv "),
        "equiv" => output.push_str("equiv "),
        "propto" => output.push_str("prop "),
        "parallel" => output.push_str("parallel "),
        "perp" => output.push_str("perp "),

        // Dots
        "vdots" => output.push_str("dots.v "),
        "ddots" => output.push_str("dots.down "),

        // Misc symbols
        "partial" => output.push_str("partial "),
        "nabla" => output.push_str("nabla "),
        "prime" => output.push_str("prime "),
        "degree" => output.push_str("degree "),
        "angle" => output.push_str("angle "),
        "ell" => output.push_str("ell "),
        "hbar" => output.push_str("planck.reduce "),
        "Re" => output.push_str("Re "),
        "Im" => output.push_str("Im "),
        "wp" => output.push_str("wp "),
        "aleph" => output.push_str("aleph "),
        "beth" => output.push_str("beth "),
        "gimel" => output.push_str("gimel "),

        // Additional integrals
        "iiiint" => output.push_str("integral.quad "),
        "oiint" => output.push_str("integral.surf "),
        "oiiint" => output.push_str("integral.vol "),

        // Additional limits
        "liminf" => output.push_str("liminf "),
        "limsup" => output.push_str("limsup "),
        "injlim" => output.push_str("op(\"inj lim\")"),
        "projlim" => output.push_str("op(\"proj lim\")"),
        "varinjlim" => output.push_str("underline(lim, arrow.r) "),
        "varprojlim" => output.push_str("underline(lim, arrow.l) "),
        "mod" => output.push_str("mod "),

        // Brackets and delimiters
        "langle" => output.push_str("angle.l "),
        "rangle" => output.push_str("angle.r "),
        "lfloor" => output.push_str("floor.l "),
        "rfloor" => output.push_str("floor.r "),
        "lceil" => output.push_str("ceil.l "),
        "rceil" => output.push_str("ceil.r "),
        "lvert" => output.push_str("bar.v "),
        "rvert" => output.push_str("bar.v "),
        "lVert" => output.push_str("bar.v.double "),
        "rVert" => output.push_str("bar.v.double "),

        // Big delimiters - handled via data module
        _ if crate::data::symbols::is_big_delimiter_command(base_name) => {
            if let Some(delim) = conv.get_required_arg(&cmd, 0) {
                if let Some(typst_delim) = crate::data::symbols::convert_delimiter(delim.trim()) {
                    if !typst_delim.is_empty() {
                        output.push_str(typst_delim);
                        output.push(' ');
                    }
                } else {
                    output.push_str(delim.trim());
                    output.push(' ');
                }
            }
        }

        // Custom Operators with limits
        "argmin" | "argmax" | "Argmin" | "Argmax" => {
            let op_name = match base_name {
                "Argmin" => "Argmin",
                "Argmax" => "Argmax",
                "argmax" => "argmax",
                _ => "argmin",
            };
            let _ = write!(output, "limits(op(\"{}\")) ", op_name);
        }

        // Custom Operators without limits
        "Var" | "Cov" | "Corr" | "tr" | "Tr" | "diag" | "rank" | "sgn" | "sign"
        | "supp" | "proj" | "prox" | "dist" | "dom" | "epi" | "graph" | "conv"
        | "softmax" | "relu" | "ReLU" | "KL" => {
            let op_name = match base_name {
                "tr" | "Tr" => "tr",
                "relu" | "ReLU" => "ReLU",
                _ => base_name,
            };
            let _ = write!(output, "op(\"{}\") ", op_name);
        }

        // Special symbols
        "E" => output.push_str("bb(E) "),
        "iid" => output.push_str("\"i.i.d.\""),

        // Negation command - \not followed by a symbol
        "not" => {
            // \not X -> X.not (for symbols that support it)
            // or cancel(X) as fallback
            if let Some(arg) = conv.convert_required_arg(&cmd, 0) {
                let arg = arg.trim();
                // Common negatable symbols
                let negated = match arg {
                    "=" | "eq" => "eq.not",
                    "<" | "lt" => "lt.not",
                    ">" | "gt" => "gt.not",
                    "in" => "in.not",
                    "subset" => "subset.not",
                    "supset" => "supset.not",
                    "equiv" => "equiv.not",
                    "approx" => "approx.not",
                    "sim" | "tilde.op" => "tilde.not",
                    "parallel" => "parallel.not",
                    "exists" => "exists.not",
                    "ni" | "in.rev" => "in.rev.not",
                    "mid" | "divides" => "divides.not",
                    "prec" => "prec.not",
                    "succ" => "succ.not",
                    "subset.eq" => "subset.eq.not",
                    "supset.eq" => "supset.eq.not",
                    "lt.eq" => "lt.eq.not",
                    "gt.eq" => "gt.eq.not",
                    "arrow.l" => "arrow.l.not",
                    "arrow.r" => "arrow.r.not",
                    "arrow.l.double" => "arrow.l.double.not",
                    "arrow.r.double" => "arrow.r.double.not",
                    "tack.r" => "tack.r.not",
                    "forces" => "forces.not",
                    _ => {
                        // Try appending .not for any symbol
                        if arg.chars().all(|c| c.is_alphanumeric() || c == '.') {
                            // Output as symbol.not
                            let _ = write!(output, "{}.not ", arg);
                            return;
                        } else {
                            // Fallback: use cancel
                            let _ = write!(output, "cancel({}) ", arg);
                            return;
                        }
                    }
                };
                output.push_str(negated);
                output.push(' ');
            }
        }

        // Inline code
        "verb" => {
            if let Some(content) = conv.get_required_arg(&cmd, 0) {
                let _ = write!(output, "`{}`", content);
            } else {
                let text = cmd.syntax().text().to_string();
                for delim in ['|', '!', '+', '@', '#', '"', '\''] {
                    let pattern = format!("verb{}", delim);
                    if let Some(start) = text.find(&pattern) {
                        let rest = &text[start + pattern.len()..];
                        if let Some(end) = rest.find(delim) {
                            let code = &rest[..end];
                            let _ = write!(output, "`{}`", code);
                            break;
                        }
                    }
                }
            }
        }
        "lstinline" => {
            if let Some(content) = conv.get_required_arg(&cmd, 0) {
                let options_str = conv.get_optional_arg(&cmd, 0).unwrap_or_default();
                let options = CodeBlockOptions::parse(&options_str);
                let lang = options.get_typst_language();
                if lang.is_empty() {
                    let _ = write!(output, "`{}`", content);
                } else {
                    let _ = write!(output, "```{} {} ```", lang, content);
                }
            }
        }
        "mintinline" => {
            let lang_raw = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let content = conv.get_required_arg(&cmd, 1).unwrap_or_default();
            let lang = LANGUAGE_MAP.get(lang_raw.as_str()).copied().unwrap_or("");
            if lang.is_empty() {
                let _ = write!(output, "`{}`", content);
            } else {
                let _ = write!(output, "```{} {} ```", lang, content);
            }
        }

        // QED symbols
        "qed" | "qedsymbol" => output.push('∎'),

        // Special character commands (Scandinavian, etc.)
        "o" => output.push('ø'),   // \o -> ø
        "O" => output.push('Ø'),   // \O -> Ø
        "aa" => output.push('å'),  // \aa -> å
        "AA" => output.push('Å'),  // \AA -> Å
        "ae" => output.push('æ'),  // \ae -> æ
        "AE" => output.push('Æ'),  // \AE -> Æ
        "oe" => output.push('œ'),  // \oe -> œ
        "OE" => output.push('Œ'),  // \OE -> Œ
        "ss" => output.push('ß'),  // \ss -> ß

        // Newcommand and def in body
        "newcommand" | "renewcommand" | "providecommand" => {
            handle_newcommand(conv, &cmd);
        }
        "def" => {
            handle_def(conv, &cmd);
        }

        // Page breaks
        "newpage" | "clearpage" | "cleardoublepage" => {
            output.push_str("\n#pagebreak()\n");
        }

        // Appendix
        "appendix" => {
            output.push_str("\n// Appendix\n");
            output.push_str("#counter(heading).update(0)\n");
            output.push_str("#set heading(numbering: \"A.\")\n\n");
        }

        // Color command (scope-based, hard to convert perfectly)
        "color" => {
            // \color{red} affects following text until scope ends
            // Typst doesn't have an equivalent - output as comment with mapped color
            let color_name = conv.get_required_arg(&cmd, 0).unwrap_or_default();
            let typst_color = parse_color_expression(&color_name);
            let _ = write!(output, "/* \\color{{{}}} -> {} */", color_name, typst_color);
        }

        // Ignored commands - alignment and layout
        "centering" | "raggedright" | "raggedleft" | "noindent" | "indent"
        | "pagebreak" | "nopagebreak" | "enlargethispage"
        | "null" | "relax" | "ignorespaces" | "obeylines" | "obeyspaces" | "frenchspacing"
        | "nonfrenchspacing" | "normalfont" | "rmfamily" | "sffamily" | "ttfamily" | "bfseries"
        | "mdseries" | "itshape" | "scshape" | "upshape" | "slshape" | "normalsize" | "tiny"
        | "scriptsize" | "footnotesize" | "small" | "large" | "Large" | "LARGE" | "huge"
        | "Huge" | "nocite" | "printbibliography" | "printglossary" | "printacronyms"
        | "glsresetall" | "tableofcontents" | "listoffigures" | "listoftables"
        | "frontmatter" | "mainmatter" | "backmatter"
        // IEEE and conference specific
        | "IEEEauthorblockN" | "IEEEauthorblockA" | "IEEEoverridecommandlockouts"
        | "IEEEaftertitletext" | "IEEEmembership" | "IEEEspecialpapernotice"
        | "markboth" | "markright" | "thanks" | "and"
        // Additional formatting switches (excluding already handled: it, bf, tt, sc, rm)
        | "em" | "sf" | "sl"
        // Floats and placement
        | "suppressfloats" | "FloatBarrier" | "clearfloats"
        // Spacing (excluding already handled: smallskip, medskip, bigskip)
        | "vfill" | "hfill" | "hfil" | "vfil" | "break" | "allowbreak" | "nobreak"
        | "goodbreak" | "penalty"
        // Margin and page setup
        | "marginpar" | "marginparpush" | "reversemarginpar" | "normalmarginpar"
        // Misc invisible commands (excluding already handled: protect)
        | "expandafter" | "global" | "long" | "outer" | "inner"
        | "noexpand" | "csname" | "endcsname" | "string" | "number" 
        // More bibliography
        | "addbibresource" | "bibdata" | "bibstyle" 
        // Index
        | "makeindex" | "printindex" | "index" | "glossary" => {
            // Ignore these
        }

        // Try symbol maps for unknown commands
        _ => {
            // Try symbol lookup
            if let Some(typst) = lookup_symbol(base_name) {
                output.push_str(typst);
                output.push(' ');
                return;
            }

            // Check text format commands (these return prefix/suffix pairs)
            let lookup_name = format!("\\{}", base_name);
            if let Some((prefix, suffix)) = TEXT_FORMAT_COMMANDS.get(lookup_name.as_str()) {
                if let Some(content) = conv.get_required_arg(&cmd, 0) {
                    output.push_str(prefix);
                    output.push_str(&content);
                    output.push_str(suffix);
                }
                return;
            }

            // Pass through unknown commands using AST-based processing
            // This preserves the behavior of convert_default_command from old version
            if conv.state.options.non_strict {
                use mitex_parser::syntax::SyntaxKind;

                if matches!(conv.state.mode, ConversionMode::Math) {
                    // In math mode, output as function call: \cmd{arg} -> cmd(arg)
                    let has_args = cmd
                        .syntax()
                        .children()
                        .any(|c| c.kind() == SyntaxKind::ClauseArgument);
                    if has_args {
                        output.push_str(base_name);
                        output.push('(');
                        let mut first = true;
                        for child in cmd.syntax().children_with_tokens() {
                            if child.kind() == SyntaxKind::ClauseArgument {
                                if !first {
                                    output.push_str(", ");
                                }
                                first = false;
                                if let SyntaxElement::Node(n) = child {
                                    conv.visit_node(&n, output);
                                }
                            }
                        }
                        output.push(')');
                    } else {
                        // No arguments - just output the name as identifier
                        output.push_str(base_name);
                    }
                } else {
                    // In text mode, output name as comment to avoid garbage text
                    let _ = write!(output, "/* \\{} */", base_name);
                    for child in cmd.syntax().children_with_tokens() {
                        if child.kind() == SyntaxKind::ClauseArgument {
                            if let SyntaxElement::Node(n) = child {
                                conv.visit_node(&n, output);
                            }
                        }
                    }
                }
            } else {
                conv.state.warnings.push(format!("Unknown command: {}", cmd_str));
            }
        }
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Handle \newcommand or \renewcommand
fn handle_newcommand(conv: &mut LatexConverter, cmd: &CmdItem) {
    // \newcommand{\name}[nargs][default]{replacement}
    let name = conv
        .get_required_arg(cmd, 0)
        .map(|n| n.trim_start_matches('\\').to_string());
    let replacement = conv.get_required_arg(cmd, 1);

    if let (Some(name), Some(replacement)) = (name, replacement) {
        let num_args = conv
            .get_optional_arg(cmd, 0)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let default_arg = conv.get_optional_arg(cmd, 1);

        conv.state.macros.insert(
            name.clone(),
            MacroDef {
                name,
                num_args,
                default_arg,
                replacement,
            },
        );
    }
}

/// Handle \def
fn handle_def(conv: &mut LatexConverter, cmd: &CmdItem) {
    // Parse \def\name{replacement} or \def\name#1#2{replacement}
    // The syntax is: \def<control-sequence><parameter-text>{<replacement>}

    // Extract the raw text of the entire \def command (with braces preserved)
    let full_text = super::utils::extract_node_text_with_braces(cmd.syntax());

    // Pattern: starts with the macro name (e.g., \Loss, \R)
    // then optionally parameters (#1, #2, etc.), then {replacement}
    let text = full_text.trim();

    // Find the macro name - it should start with \
    if let Some(name_start) = text.find('\\') {
        let after_name = &text[name_start + 1..];
        // Find end of macro name (first non-alpha character)
        let name_end = after_name
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(after_name.len());
        let macro_name = &after_name[..name_end];

        if macro_name.is_empty() {
            return;
        }

        // Count parameter markers (#1, #2, etc.)
        let rest = &after_name[name_end..];
        let num_args = rest.matches('#').count().min(9);

        // Find the replacement text in braces - handle nested braces correctly
        if let Some(brace_start) = rest.find('{') {
            let after_brace = &rest[brace_start + 1..];
            // Find matching closing brace
            let mut depth = 1;
            let mut end_pos = 0;
            for (i, c) in after_brace.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let replacement = &after_brace[..end_pos];

            conv.state.macros.insert(
                macro_name.to_string(),
                MacroDef {
                    name: macro_name.to_string(),
                    num_args,
                    default_arg: None,
                    replacement: replacement.to_string(),
                },
            );
        }
    }
}

/// Handle \newacronym
fn handle_newacronym(conv: &mut LatexConverter, cmd: &CmdItem) {
    let key = conv.get_required_arg(cmd, 0);
    let short = conv.get_required_arg(cmd, 1);
    let long = conv.get_required_arg(cmd, 2);

    if let (Some(key), Some(short), Some(long)) = (key, short, long) {
        conv.state.register_acronym(&key, &short, &long);
    }
}

/// Handle \newglossaryentry
fn handle_newglossaryentry(conv: &mut LatexConverter, cmd: &CmdItem) {
    let key = conv.get_required_arg(cmd, 0);
    let opts = conv.get_required_arg(cmd, 1).unwrap_or_default();

    if let Some(key) = key {
        // Parse name and description from opts
        let mut name = String::new();
        let mut description = String::new();

        for part in opts.split(',') {
            let part = part.trim();
            if let Some(n) = part.strip_prefix("name=") {
                name = n.trim_matches(|c| c == '{' || c == '}').to_string();
            } else if let Some(d) = part.strip_prefix("description=") {
                description = d.trim_matches(|c| c == '{' || c == '}').to_string();
            }
        }

        conv.state.register_glossary(&key, &name, &description);
    }
}

/// Expand a user-defined macro
fn expand_user_macro(conv: &mut LatexConverter, cmd: &CmdItem, macro_def: &MacroDef) -> String {
    let mut result = macro_def.replacement.clone();

    // Collect arguments
    for i in 0..macro_def.num_args {
        let arg = conv
            .convert_required_arg(cmd, i)
            .or_else(|| macro_def.default_arg.clone())
            .unwrap_or_default();

        let placeholder = format!("#{}", i + 1);
        result = result.replace(&placeholder, &arg);
    }

    // Convert the expanded macro
    let mut output = String::new();
    let tree = mitex_parser::parse(&result, conv.spec.clone());
    conv.visit_node(&tree, &mut output);
    output
}

/// Convert a LaTeX dimension to Typst
fn convert_dimension(dim: &str) -> String {
    let dim = dim.trim();

    // Handle \linewidth, \textwidth, etc.
    if dim.contains("\\linewidth") || dim.contains("\\textwidth") || dim.contains("\\columnwidth") {
        // Extract multiplier if present
        if let Some(mult) = dim.strip_suffix("\\linewidth") {
            let mult = mult.trim();
            if mult.is_empty() || mult == "1" {
                return "100%".to_string();
            }
            if let Ok(f) = mult.parse::<f32>() {
                return format!("{}%", (f * 100.0) as i32);
            }
        }
        if let Some(mult) = dim.strip_suffix("\\textwidth") {
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

    // Handle standard units
    let dim = dim
        .replace("\\fill", "1fr")
        .replace("\\stretch", "1fr")
        .replace("\\hfill", "1fr");

    // Already has a unit
    if dim.ends_with("pt")
        || dim.ends_with("em")
        || dim.ends_with("ex")
        || dim.ends_with("mm")
        || dim.ends_with("cm")
        || dim.ends_with("in")
        || dim.ends_with("pc")
        || dim.ends_with("bp")
        || dim.ends_with("%")
        || dim.ends_with("fr")
    {
        return dim;
    }

    // Just a number, assume pt
    if dim.parse::<f32>().is_ok() {
        return format!("{}pt", dim);
    }

    dim
}

/// Apply a text accent to a character
fn apply_text_accent(content: &str, accent: char) -> String {
    let c = content.chars().next().unwrap_or(' ');
    match accent {
        '`' => match c {
            'a' => "à".to_string(),
            'e' => "è".to_string(),
            'i' => "ì".to_string(),
            'o' => "ò".to_string(),
            'u' => "ù".to_string(),
            'A' => "À".to_string(),
            'E' => "È".to_string(),
            'I' => "Ì".to_string(),
            'O' => "Ò".to_string(),
            'U' => "Ù".to_string(),
            _ => content.to_string(),
        },
        '\'' => match c {
            'a' => "á".to_string(),
            'e' => "é".to_string(),
            'i' => "í".to_string(),
            'o' => "ó".to_string(),
            'u' => "ú".to_string(),
            'y' => "ý".to_string(),
            'A' => "Á".to_string(),
            'E' => "É".to_string(),
            'I' => "Í".to_string(),
            'O' => "Ó".to_string(),
            'U' => "Ú".to_string(),
            'Y' => "Ý".to_string(),
            _ => content.to_string(),
        },
        '^' => match c {
            'a' => "â".to_string(),
            'e' => "ê".to_string(),
            'i' => "î".to_string(),
            'o' => "ô".to_string(),
            'u' => "û".to_string(),
            'A' => "Â".to_string(),
            'E' => "Ê".to_string(),
            'I' => "Î".to_string(),
            'O' => "Ô".to_string(),
            'U' => "Û".to_string(),
            _ => content.to_string(),
        },
        '~' => match c {
            'a' => "ã".to_string(),
            'n' => "ñ".to_string(),
            'o' => "õ".to_string(),
            'A' => "Ã".to_string(),
            'N' => "Ñ".to_string(),
            'O' => "Õ".to_string(),
            _ => content.to_string(),
        },
        '"' => match c {
            'a' => "ä".to_string(),
            'e' => "ë".to_string(),
            'i' => "ï".to_string(),
            'o' => "ö".to_string(),
            'u' => "ü".to_string(),
            'y' => "ÿ".to_string(),
            'A' => "Ä".to_string(),
            'E' => "Ë".to_string(),
            'I' => "Ï".to_string(),
            'O' => "Ö".to_string(),
            'U' => "Ü".to_string(),
            _ => content.to_string(),
        },
        _ => content.to_string(),
    }
}

/// Apply cedilla
fn apply_cedilla(content: &str) -> String {
    let c = content.chars().next().unwrap_or(' ');
    match c {
        'c' => "ç".to_string(),
        'C' => "Ç".to_string(),
        _ => content.to_string(),
    }
}

/// Convert section heading with proper level
fn convert_section(conv: &mut LatexConverter, cmd: &CmdItem, level: u8, output: &mut String) {
    if let Some(title) = conv.get_required_arg(cmd, 0) {
        output.push('\n');
        for _ in 0..=level {
            output.push('=');
        }
        output.push(' ');
        output.push_str(&title);
        output.push('\n');
    }
}
