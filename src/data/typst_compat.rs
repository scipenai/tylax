//! Typst to LaTeX compatibility mappings
//!
//! This module contains data-driven mappings for converting Typst constructs to LaTeX.
//! Centralizes hardcoded strings to enable easy maintenance and extension.

use lazy_static::lazy_static;
use std::collections::HashMap;

// ============================================================================
// Markup Function Handlers
// ============================================================================

/// Handler type for markup functions
#[derive(Debug, Clone)]
pub enum MarkupHandler {
    /// Simple wrapper: prefix + content + suffix
    Wrap {
        prefix: &'static str,
        suffix: &'static str,
    },
    /// Environment: \begin{env} + content + \end{env}
    Environment { name: &'static str },
    /// Just output the content (pass-through)
    PassThrough,
    /// Special handling required (not in map)
    Special,
}

lazy_static! {
    /// Mapping from Typst markup function names to LaTeX handlers
    pub static ref TYPST_MARKUP_HANDLERS: HashMap<&'static str, MarkupHandler> = {
        let mut m = HashMap::new();

        // Text formatting
        m.insert("strong", MarkupHandler::Wrap { prefix: "\\textbf{", suffix: "}" });
        m.insert("bold", MarkupHandler::Wrap { prefix: "\\textbf{", suffix: "}" });
        m.insert("emph", MarkupHandler::Wrap { prefix: "\\textit{", suffix: "}" });
        m.insert("italic", MarkupHandler::Wrap { prefix: "\\textit{", suffix: "}" });
        m.insert("underline", MarkupHandler::Wrap { prefix: "\\underline{", suffix: "}" });
        m.insert("strike", MarkupHandler::Wrap { prefix: "\\sout{", suffix: "}" });
        m.insert("smallcaps", MarkupHandler::Wrap { prefix: "\\textsc{", suffix: "}" });
        m.insert("raw", MarkupHandler::Wrap { prefix: "\\texttt{", suffix: "}" });
        m.insert("sub", MarkupHandler::Wrap { prefix: "\\textsubscript{", suffix: "}" });
        m.insert("super", MarkupHandler::Wrap { prefix: "\\textsuperscript{", suffix: "}" });

        // Document structure
        m.insert("heading", MarkupHandler::Wrap { prefix: "\\section{", suffix: "}\n" });
        m.insert("list", MarkupHandler::Environment { name: "itemize" });
        m.insert("enum", MarkupHandler::Environment { name: "enumerate" });
        m.insert("quote", MarkupHandler::Environment { name: "quote" });
        m.insert("figure", MarkupHandler::Special);
        m.insert("table", MarkupHandler::Special);

        // Page elements
        m.insert("pagebreak", MarkupHandler::Wrap { prefix: "\\newpage", suffix: "" });
        m.insert("linebreak", MarkupHandler::Wrap { prefix: "\\\\", suffix: "" });
        m.insert("line", MarkupHandler::Wrap { prefix: "\\hrule", suffix: "" });

        // Pass-through
        // m.insert("text", MarkupHandler::PassThrough); // Handled as Special now
        // m.insert("align", MarkupHandler::PassThrough); // Handled as Special now
        m.insert("text", MarkupHandler::Special);
        m.insert("align", MarkupHandler::Special);

        // Special handling (complex parsing required)
        m.insert("image", MarkupHandler::Special);
        m.insert("link", MarkupHandler::Special);
        m.insert("cite", MarkupHandler::Special);
        m.insert("ref", MarkupHandler::Special);
        m.insert("label", MarkupHandler::Special);
        m.insert("bibliography", MarkupHandler::Special);
        m.insert("footnote", MarkupHandler::Special);
        m.insert("caption", MarkupHandler::Special);
        m.insert("rotate", MarkupHandler::Special);
        m.insert("rect", MarkupHandler::Special);
        m.insert("box", MarkupHandler::Special);
        m.insert("block", MarkupHandler::Special);
        m.insert("pad", MarkupHandler::Special);
        m.insert("center", MarkupHandler::Special);

        // Theorem-like environments
        m.insert("theorem", MarkupHandler::Special);
        m.insert("lemma", MarkupHandler::Special);
        m.insert("proposition", MarkupHandler::Special);
        m.insert("corollary", MarkupHandler::Special);
        m.insert("definition", MarkupHandler::Special);
        m.insert("example", MarkupHandler::Special);
        m.insert("remark", MarkupHandler::Special);
        m.insert("proof", MarkupHandler::Special);

        // Layout
        m.insert("columns", MarkupHandler::Special);
        m.insert("grid", MarkupHandler::Special);
        m.insert("center", MarkupHandler::Special);
        m.insert("box", MarkupHandler::Special);
        m.insert("block", MarkupHandler::Special);
        m.insert("rect", MarkupHandler::Special);
        m.insert("blockquote", MarkupHandler::Special);

        // Spacing
        m.insert("h", MarkupHandler::Special);  // horizontal space
        m.insert("v", MarkupHandler::Special);  // vertical space
        m.insert("pad", MarkupHandler::Special); // padding

        // Visibility
        m.insert("hide", MarkupHandler::Wrap { prefix: "\\phantom{", suffix: "}" });

        m
    };
}

// ============================================================================
// Math Function Handlers
// ============================================================================

/// Handler type for math functions
#[derive(Debug, Clone)]
pub enum MathHandler {
    /// Direct command: \cmd{arg1}{arg2}...
    Command { latex_cmd: &'static str },
    /// Command with optional arg: `\cmd[opt]{arg}`
    CommandWithOpt { latex_cmd: &'static str },
    /// Delimiter pair: `\left<open> content \right<close>`
    Delimiters {
        open: &'static str,
        close: &'static str,
    },
    /// Environment: \begin{env}...\end{env}
    Environment { name: &'static str },
    /// Big operator: \cmd with limits
    BigOperator { latex_cmd: &'static str },
    /// Special handling required
    Special,
}

lazy_static! {
    /// Mapping from Typst math function names to LaTeX handlers
    pub static ref TYPST_MATH_HANDLERS: HashMap<&'static str, MathHandler> = {
        let mut m = HashMap::new();

        // Fractions
        m.insert("frac", MathHandler::Command { latex_cmd: "\\frac" });
        m.insert("dfrac", MathHandler::Command { latex_cmd: "\\dfrac" });
        m.insert("tfrac", MathHandler::Command { latex_cmd: "\\tfrac" });
        m.insert("binom", MathHandler::Command { latex_cmd: "\\binom" });

        // Roots
        m.insert("sqrt", MathHandler::CommandWithOpt { latex_cmd: "\\sqrt" });
        m.insert("root", MathHandler::CommandWithOpt { latex_cmd: "\\sqrt" });

        // Decorations / Accents
        m.insert("vec", MathHandler::Command { latex_cmd: "\\vec" });
        m.insert("hat", MathHandler::Command { latex_cmd: "\\widehat" });
        m.insert("widehat", MathHandler::Command { latex_cmd: "\\widehat" });
        m.insert("tilde", MathHandler::Command { latex_cmd: "\\widetilde" });
        m.insert("widetilde", MathHandler::Command { latex_cmd: "\\widetilde" });
        m.insert("overline", MathHandler::Command { latex_cmd: "\\overline" });
        m.insert("ol", MathHandler::Command { latex_cmd: "\\overline" });
        m.insert("underline", MathHandler::Command { latex_cmd: "\\underline" });
        m.insert("ul", MathHandler::Command { latex_cmd: "\\underline" });
        m.insert("overbrace", MathHandler::Command { latex_cmd: "\\overbrace" });
        m.insert("underbrace", MathHandler::Command { latex_cmd: "\\underbrace" });
        m.insert("cancel", MathHandler::Command { latex_cmd: "\\cancel" });
        m.insert("hide", MathHandler::Command { latex_cmd: "\\phantom" });
        m.insert("box", MathHandler::Command { latex_cmd: "\\boxed" });
        m.insert("dot", MathHandler::Command { latex_cmd: "\\dot" });
        m.insert("ddot", MathHandler::Command { latex_cmd: "\\ddot" });
        m.insert("acute", MathHandler::Command { latex_cmd: "\\acute" });
        m.insert("grave", MathHandler::Command { latex_cmd: "\\grave" });
        m.insert("macron", MathHandler::Command { latex_cmd: "\\bar" });
        m.insert("bar", MathHandler::Command { latex_cmd: "\\bar" });
        m.insert("breve", MathHandler::Command { latex_cmd: "\\breve" });
        m.insert("caron", MathHandler::Command { latex_cmd: "\\check" });

        // Delimiters
        m.insert("abs", MathHandler::Delimiters { open: "\\left|", close: "\\right|" });
        m.insert("norm", MathHandler::Delimiters { open: "\\left\\|", close: "\\right\\|" });
        m.insert("floor", MathHandler::Delimiters { open: "\\left\\lfloor ", close: "\\right\\rfloor" });
        m.insert("ceil", MathHandler::Delimiters { open: "\\left\\lceil ", close: "\\right\\rceil" });
        m.insert("round", MathHandler::Delimiters { open: "\\left\\lfloor ", close: "\\right\\rceil" });

        // Big operators
        m.insert("sum", MathHandler::BigOperator { latex_cmd: "\\sum" });
        m.insert("prod", MathHandler::BigOperator { latex_cmd: "\\prod" });
        m.insert("int", MathHandler::BigOperator { latex_cmd: "\\int" });
        m.insert("oint", MathHandler::BigOperator { latex_cmd: "\\oint" });
        m.insert("iint", MathHandler::BigOperator { latex_cmd: "\\iint" });
        m.insert("iiint", MathHandler::BigOperator { latex_cmd: "\\iiint" });
        m.insert("lim", MathHandler::BigOperator { latex_cmd: "\\lim" });
        m.insert("limsup", MathHandler::BigOperator { latex_cmd: "\\limsup" });
        m.insert("liminf", MathHandler::BigOperator { latex_cmd: "\\liminf" });
        m.insert("max", MathHandler::BigOperator { latex_cmd: "\\max" });
        m.insert("min", MathHandler::BigOperator { latex_cmd: "\\min" });
        m.insert("sup", MathHandler::BigOperator { latex_cmd: "\\sup" });
        m.insert("inf", MathHandler::BigOperator { latex_cmd: "\\inf" });

        // Font styles
        m.insert("text", MathHandler::Command { latex_cmd: "\\text" });
        m.insert("upright", MathHandler::Command { latex_cmd: "\\mathrm" });
        m.insert("bold", MathHandler::Command { latex_cmd: "\\mathbf" });
        m.insert("bf", MathHandler::Command { latex_cmd: "\\mathbf" });
        m.insert("italic", MathHandler::Command { latex_cmd: "\\mathit" });
        m.insert("it", MathHandler::Command { latex_cmd: "\\mathit" });
        m.insert("cal", MathHandler::Command { latex_cmd: "\\mathcal" });
        m.insert("mathcal", MathHandler::Command { latex_cmd: "\\mathcal" });
        m.insert("frak", MathHandler::Command { latex_cmd: "\\mathfrak" });
        m.insert("mathfrak", MathHandler::Command { latex_cmd: "\\mathfrak" });
        m.insert("bb", MathHandler::Command { latex_cmd: "\\mathbb" });
        m.insert("mathbb", MathHandler::Command { latex_cmd: "\\mathbb" });
        m.insert("mono", MathHandler::Command { latex_cmd: "\\mathtt" });
        m.insert("mathtt", MathHandler::Command { latex_cmd: "\\mathtt" });
        m.insert("sans", MathHandler::Command { latex_cmd: "\\mathsf" });
        m.insert("mathsf", MathHandler::Command { latex_cmd: "\\mathsf" });

        // Matrix environments
        m.insert("mat", MathHandler::Environment { name: "matrix" });
        m.insert("matrix", MathHandler::Environment { name: "matrix" });
        m.insert("pmat", MathHandler::Environment { name: "pmatrix" });
        m.insert("pmatrix", MathHandler::Environment { name: "pmatrix" });
        m.insert("bmat", MathHandler::Environment { name: "bmatrix" });
        m.insert("bmatrix", MathHandler::Environment { name: "bmatrix" });
        m.insert("Bmat", MathHandler::Environment { name: "Bmatrix" });
        m.insert("Bmatrix", MathHandler::Environment { name: "Bmatrix" });
        m.insert("vmat", MathHandler::Environment { name: "vmatrix" });
        m.insert("vmatrix", MathHandler::Environment { name: "vmatrix" });
        m.insert("Vmat", MathHandler::Environment { name: "Vmatrix" });
        m.insert("Vmatrix", MathHandler::Environment { name: "Vmatrix" });
        m.insert("cases", MathHandler::Environment { name: "cases" });

        // Special
        m.insert("color", MathHandler::Special);
        m.insert("limits", MathHandler::Special);
        m.insert("arrow", MathHandler::Special);
        m.insert("accent", MathHandler::Special);
        m.insert("class", MathHandler::Special);
        m.insert("op", MathHandler::Special);
        m.insert("display", MathHandler::Special);
        m.insert("inline", MathHandler::Special);
        m.insert("set", MathHandler::Special);
        m.insert("Set", MathHandler::Special);

        // math.* namespace functions (need special handling)
        m.insert("math.vec", MathHandler::Special);  // Column vector -> pmatrix
        m.insert("math.mat", MathHandler::Environment { name: "matrix" });
        m.insert("math.cases", MathHandler::Environment { name: "cases" });
        m.insert("math.abs", MathHandler::Delimiters { open: "\\left|", close: "\\right|" });
        m.insert("math.norm", MathHandler::Delimiters { open: "\\left\\|", close: "\\right\\|" });
        m.insert("math.floor", MathHandler::Delimiters { open: "\\left\\lfloor ", close: "\\right\\rfloor" });
        m.insert("math.ceil", MathHandler::Delimiters { open: "\\left\\lceil ", close: "\\right\\rceil" });
        m.insert("math.round", MathHandler::Delimiters { open: "\\left\\lfloor ", close: "\\right\\rceil" });

        // lr() - auto-sizing delimiters
        m.insert("lr", MathHandler::Special);

        // attach() - for limits positioning
        m.insert("attach", MathHandler::Special);

        // scripts() - script positioning mode
        m.insert("scripts", MathHandler::Special);

        // primes() - prime marks
        m.insert("primes", MathHandler::Special);

        // stretch() - stretchy symbols
        m.insert("stretch", MathHandler::Special);

        // mid() - middle delimiter (for conditionals like P(A|B))
        m.insert("mid", MathHandler::Special);

        // circle() - circled content
        m.insert("circle", MathHandler::Special);

        // Vector calculus operators
        m.insert("gradient", MathHandler::Command { latex_cmd: "\\nabla" });
        m.insert("grad", MathHandler::Command { latex_cmd: "\\nabla" });
        m.insert("divergence", MathHandler::Special);  // nabla dot
        m.insert("curl", MathHandler::Special);  // nabla cross
        m.insert("laplacian", MathHandler::Command { latex_cmd: "\\Delta" });

        // Cross and dot products
        m.insert("dot.op", MathHandler::Command { latex_cmd: "\\cdot" });
        m.insert("cross", MathHandler::Command { latex_cmd: "\\times" });
        m.insert("times", MathHandler::Command { latex_cmd: "\\times" });

        // Additional accents from tex2typst
        m.insert("diaer", MathHandler::Command { latex_cmd: "\\ddot" });  // diaeresis
        m.insert("arrow.r", MathHandler::Command { latex_cmd: "\\overrightarrow" });
        m.insert("arrow.l", MathHandler::Command { latex_cmd: "\\overleftarrow" });

        // Spacing commands
        m.insert("thin", MathHandler::Command { latex_cmd: "\\," });
        m.insert("med", MathHandler::Command { latex_cmd: "\\:" });
        m.insert("thick", MathHandler::Command { latex_cmd: "\\;" });
        m.insert("quad", MathHandler::Command { latex_cmd: "\\quad" });
        m.insert("wide", MathHandler::Command { latex_cmd: "\\qquad" });

        // Additional operators
        m.insert("det", MathHandler::BigOperator { latex_cmd: "\\det" });
        m.insert("gcd", MathHandler::BigOperator { latex_cmd: "\\gcd" });
        m.insert("deg", MathHandler::BigOperator { latex_cmd: "\\deg" });
        m.insert("dim", MathHandler::BigOperator { latex_cmd: "\\dim" });
        m.insert("ker", MathHandler::BigOperator { latex_cmd: "\\ker" });
        m.insert("hom", MathHandler::BigOperator { latex_cmd: "\\hom" });
        m.insert("arg", MathHandler::BigOperator { latex_cmd: "\\arg" });
        m.insert("Pr", MathHandler::BigOperator { latex_cmd: "\\Pr" });
        m.insert("exp", MathHandler::BigOperator { latex_cmd: "\\exp" });
        m.insert("log", MathHandler::BigOperator { latex_cmd: "\\log" });
        m.insert("ln", MathHandler::BigOperator { latex_cmd: "\\ln" });
        m.insert("lg", MathHandler::BigOperator { latex_cmd: "\\lg" });
        m.insert("sin", MathHandler::BigOperator { latex_cmd: "\\sin" });
        m.insert("cos", MathHandler::BigOperator { latex_cmd: "\\cos" });
        m.insert("tan", MathHandler::BigOperator { latex_cmd: "\\tan" });
        m.insert("cot", MathHandler::BigOperator { latex_cmd: "\\cot" });
        m.insert("sec", MathHandler::BigOperator { latex_cmd: "\\sec" });
        m.insert("csc", MathHandler::BigOperator { latex_cmd: "\\csc" });
        m.insert("arcsin", MathHandler::BigOperator { latex_cmd: "\\arcsin" });
        m.insert("arccos", MathHandler::BigOperator { latex_cmd: "\\arccos" });
        m.insert("arctan", MathHandler::BigOperator { latex_cmd: "\\arctan" });
        m.insert("sinh", MathHandler::BigOperator { latex_cmd: "\\sinh" });
        m.insert("cosh", MathHandler::BigOperator { latex_cmd: "\\cosh" });
        m.insert("tanh", MathHandler::BigOperator { latex_cmd: "\\tanh" });
        m.insert("coth", MathHandler::BigOperator { latex_cmd: "\\coth" });

        m
    };
}

// ============================================================================
// Heading Level Mapping
// ============================================================================

lazy_static! {
    /// Mapping from heading level to LaTeX sectioning command
    pub static ref HEADING_COMMANDS: Vec<&'static str> = vec![
        "\\section",       // level 1
        "\\subsection",    // level 2
        "\\subsubsection", // level 3
        "\\paragraph",     // level 4
        "\\subparagraph",  // level 5
    ];
}

/// Get LaTeX sectioning command for a heading level (1-indexed)
pub fn get_heading_command(level: usize) -> &'static str {
    HEADING_COMMANDS
        .get(level.saturating_sub(1))
        .copied()
        .unwrap_or("\\paragraph")
}

// ============================================================================
// Math Functions that delegate to markup mode
// ============================================================================

lazy_static! {
    /// Math functions that should be wrapped in $ when encountered in markup mode
    pub static ref MATH_FUNCS_IN_MARKUP: std::collections::HashSet<&'static str> = {
        let mut s = std::collections::HashSet::new();
        s.insert("frac");
        s.insert("sqrt");
        s.insert("sum");
        s.insert("prod");
        s.insert("int");
        s.insert("lim");
        s.insert("mat");
        s.insert("pmat");
        s.insert("bmat");
        s.insert("cases");
        s.insert("vec");
        s
    };
}

/// Check if a function should be treated as math when in markup mode
pub fn is_math_func_in_markup(name: &str) -> bool {
    MATH_FUNCS_IN_MARKUP.contains(name)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markup_handlers() {
        assert!(TYPST_MARKUP_HANDLERS.contains_key("strong"));
        assert!(TYPST_MARKUP_HANDLERS.contains_key("emph"));

        if let Some(MarkupHandler::Wrap { prefix, suffix }) = TYPST_MARKUP_HANDLERS.get("strong") {
            assert_eq!(*prefix, "\\textbf{");
            assert_eq!(*suffix, "}");
        } else {
            panic!("Expected Wrap handler for strong");
        }
    }

    #[test]
    fn test_math_handlers() {
        assert!(TYPST_MATH_HANDLERS.contains_key("frac"));
        assert!(TYPST_MATH_HANDLERS.contains_key("sqrt"));

        if let Some(MathHandler::Command { latex_cmd }) = TYPST_MATH_HANDLERS.get("frac") {
            assert_eq!(*latex_cmd, "\\frac");
        } else {
            panic!("Expected Command handler for frac");
        }
    }

    #[test]
    fn test_heading_commands() {
        assert_eq!(get_heading_command(1), "\\section");
        assert_eq!(get_heading_command(2), "\\subsection");
        assert_eq!(get_heading_command(3), "\\subsubsection");
        assert_eq!(get_heading_command(6), "\\paragraph"); // fallback
    }

    #[test]
    fn test_math_funcs_in_markup() {
        assert!(is_math_func_in_markup("frac"));
        assert!(is_math_func_in_markup("sqrt"));
        assert!(!is_math_func_in_markup("strong"));
    }
}
