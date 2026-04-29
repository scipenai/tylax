//! Integration tests for Tylax full document conversion

use std::io::Write;
use std::process::{Command, Stdio};

use tylax::{
    convert_auto, convert_auto_document, detect_format, latex_document_to_typst, latex_to_typst,
    typst_to_latex, typst_to_latex_with_diagnostics, typst_to_latex_with_options, T2LOptions,
};

fn run_t2l_cli(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_t2l"))
        .arg("--direction")
        .arg("t2l")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn t2l CLI");

    child
        .stdin
        .as_mut()
        .expect("t2l CLI stdin unavailable")
        .write_all(input.as_bytes())
        .expect("failed to write CLI input");

    let output = child
        .wait_with_output()
        .expect("failed to wait for t2l CLI output");
    assert!(
        output.status.success(),
        "t2l CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("CLI output was not valid UTF-8")
}

fn normalize_output(output: &str) -> &str {
    output.trim_end_matches('\n')
}

fn assert_t2l_paths_match(input: &str) -> String {
    let options = typst_to_latex_with_options(input, &T2LOptions::default());
    let diagnostics = typst_to_latex_with_diagnostics(input, &T2LOptions::default()).output;
    let cli = run_t2l_cli(input);

    assert_eq!(
        normalize_output(&options),
        normalize_output(&diagnostics),
        "with_options and with_diagnostics diverged for input:\n{}",
        input
    );
    assert_eq!(
        normalize_output(&options),
        normalize_output(&cli),
        "with_options and CLI diverged for input:\n{}",
        input
    );

    options
}

// ============================================================================
// Math Mode Tests - LaTeX to Typst
// ============================================================================

mod l2t_math {
    use super::*;

    #[test]
    fn test_greek_letters() {
        // AST converter may output Unicode Greek letters (α, β, etc.) or text names
        let letters = [
            ("\\alpha", &["alpha", "α"]),
            ("\\beta", &["beta", "β"]),
            ("\\gamma", &["gamma", "γ"]),
            ("\\Delta", &["Delta", "Δ"]),
            ("\\Omega", &["Omega", "Ω"]),
        ];

        for (latex, expected_variants) in letters {
            let result = latex_to_typst(latex);
            let found = expected_variants.iter().any(|exp| result.contains(exp));
            assert!(
                found,
                "Expected '{}' to contain one of {:?}, got '{}'",
                latex, expected_variants, result
            );
        }
    }

    #[test]
    fn test_fractions() {
        let result = latex_to_typst(r"\frac{a}{b}");
        // With frac_to_slash enabled by default, simple fractions may use slash notation
        assert!(result.contains("frac") || result.contains("/"));

        let result = latex_to_typst(r"\frac{x+1}{x-1}");
        assert!(result.contains("frac") || result.contains("/"));
    }

    #[test]
    fn test_literal_math_slash_is_escaped() {
        assert_eq!(latex_to_typst("a/b").trim(), r"a\/b");
        assert_eq!(latex_to_typst("$a/b$").trim(), r"$a\/b$");
    }

    #[test]
    fn test_fraction_with_unbraced_term_arguments() {
        assert_eq!(latex_to_typst(r"\frac{a}b").trim(), "a/b");
        assert_eq!(latex_to_typst(r"\frac12").trim(), "1/2");
    }

    #[test]
    fn test_sqrt() {
        let result = latex_to_typst(r"\sqrt{x}");
        assert!(result.contains("sqrt") || result.contains("root"));

        let result = latex_to_typst(r"\sqrt[3]{x}");
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_subscripts_superscripts() {
        let result = latex_to_typst(r"x^2");
        assert!(result.contains("x") && result.contains("2"));

        let result = latex_to_typst(r"x_i");
        assert!(result.contains("x") && result.contains("i"));

        let result = latex_to_typst(r"x_i^2");
        assert!(result.contains("x") && result.contains("i") && result.contains("2"));
    }

    #[test]
    fn test_operators() {
        let result = latex_to_typst(r"\sum_{i=1}^{n} i");
        assert!(result.contains("sum"));

        let result = latex_to_typst(r"\int_0^\infty f(x) dx");
        assert!(result.contains("int") || result.contains("integral"));

        let result = latex_to_typst(r"\prod_{i=1}^{n} a_i");
        assert!(result.contains("prod"));
    }

    #[test]
    fn test_overset_with_unbraced_symbol_base() {
        assert_eq!(
            latex_to_typst(r"\overset{p}\sim").trim(),
            "limits(tilde)^(p)"
        );
        assert_eq!(
            latex_to_typst(r"\overset{p}{\sim}").trim(),
            "limits(tilde)^(p)"
        );
    }

    #[test]
    fn test_matrices() {
        let result = latex_to_typst(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}");
        assert!(!result.contains("Error"));

        let result = latex_to_typst(r"\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}");
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_cases_escapes_source_commas() {
        let result = latex_to_typst(
            r"\begin{cases}
        0,& i\ne j,\\
        1,& i=j.
        \end{cases}",
        );

        assert!(
            result.contains(r"cases(0\,& i != j\,, 1\,& i = j .)"),
            "source commas inside cases should be escaped, got: {}",
            result
        );
    }

    #[test]
    fn test_array_environment() {
        // Simple array -> mat(delim: #none, ...)
        let result = latex_to_typst(r"\begin{array}{cc} a & b \\ c & d \end{array}");
        assert!(
            result.contains("mat("),
            "array should become mat(), got: {}",
            result
        );
        assert!(
            !result.contains("table"),
            "array should NOT become a table, got: {}",
            result
        );

        // array with \left( ... \right) wrapping (issue #6 example)
        let result = latex_to_typst(r"\left(\begin{array}{l} x \\ y \\ 1 \end{array}\right)");
        assert!(
            result.contains("mat("),
            "array inside \\left...\\right should become mat(), got: {}",
            result
        );

        // determinant-like array with single bars should become a matrix with |
        let result = latex_to_typst(r"\left|\begin{array}{cc} a & b \\ c & d \end{array}\right|");
        assert!(
            result.contains("mat(delim: \"|\"") || result.contains("mat(delim: \"|\", "),
            "array inside \\left|...\\right| should become mat(delim: \"|\", ...), got: {}",
            result
        );
        assert!(
            !result.contains("abs("),
            "array inside \\left|...\\right| should NOT become abs(...), got: {}",
            result
        );

        // determinant-like array with double bars should become a matrix with ‖
        let result = latex_to_typst(r"\left\|\begin{array}{cc} a & b \\ c & d \end{array}\right\|");
        assert!(
            result.contains("mat(delim: \"‖\"") || result.contains("mat(delim: \"‖\", "),
            "array inside \\left\\|...\\right\\| should become mat(delim: \"‖\", ...), got: {}",
            result
        );
        assert!(
            !result.contains("norm("),
            "array inside \\left\\|...\\right\\| should NOT become norm(...), got: {}",
            result
        );

        // scalar abs must remain abs(...)
        let result = latex_to_typst(r"\left|x+y\right|");
        assert!(
            result.contains("abs("),
            "scalar |...| should still become abs(...), got: {}",
            result
        );

        // scalar norm must remain norm(...)
        let result = latex_to_typst(r"\left\|x+y\right\|");
        assert!(
            result.contains("norm("),
            "scalar ||...|| should still become norm(...), got: {}",
            result
        );
    }

    #[test]
    fn test_lr_wrapped_no_intrinsic_matrix_family() {
        let result = latex_to_typst(r"\left(\begin{matrix} a & b \\ c & d \end{matrix}\right)");
        assert!(
            result.contains("mat(delim: \"(\"") || result.contains("mat(delim: \"(\", "),
            r"matrix inside \left(...\right) should inherit ( delimiter, got: {}",
            result
        );

        let result =
            latex_to_typst(r"\left[\begin{smallmatrix} a & b \\ c & d \end{smallmatrix}\right]");
        assert!(
            result.contains("mat(delim: \"[\"") || result.contains("mat(delim: \"[\", "),
            r"smallmatrix inside \left[...\right] should inherit [ delimiter, got: {}",
            result
        );

        let result = latex_to_typst(r"\left\{\begin{matrix} a & b \\ c & d \end{matrix}\right\}");
        assert!(
            result.contains("mat(delim: \"{\"") || result.contains("mat(delim: \"{\", "),
            "matrix inside brace-wrapped left/right should inherit brace delimiter, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_wrapped_intrinsic_matrix_family_preserves_nested_delims() {
        let result = latex_to_typst(r"\left|\begin{pmatrix} a & b \\ c & d \end{pmatrix}\right|");
        assert!(
            result.contains("mat(delim: \"(\"") || result.contains("mat(delim: \"(\", "),
            "pmatrix should keep its inner ( delimiter, got: {}",
            result
        );
        assert!(
            result.contains("bar.v") || result.contains("lr("),
            "outer |...| should still be preserved around pmatrix, got: {}",
            result
        );
        assert!(
            !result.contains("abs("),
            "pmatrix inside |...| should not collapse to abs(...), got: {}",
            result
        );

        let result = latex_to_typst(r"\left[\begin{pmatrix} a & b \\ c & d \end{pmatrix}\right]");
        assert!(
            result.contains("mat(delim: \"(\"") || result.contains("mat(delim: \"(\", "),
            "pmatrix should keep its inner ( delimiter under outer [], got: {}",
            result
        );
        assert!(
            !result.contains("mat(delim: \"[\"") && !result.contains("mat(delim: \"[\", "),
            "outer [] should not override inner pmatrix delimiter, got: {}",
            result
        );

        let result = latex_to_typst(r"\left\|\begin{vmatrix} a & b \\ c & d \end{vmatrix}\right\|");
        assert!(
            result.contains("mat(delim: \"|\"") || result.contains("mat(delim: \"|\", "),
            "vmatrix should keep its inner | delimiter, got: {}",
            result
        );
        assert!(
            result.contains("bar.v.double") || result.contains("lr("),
            "outer ||...|| should still be preserved around vmatrix, got: {}",
            result
        );
        assert!(
            !result.contains("norm("),
            "vmatrix inside ||...|| should not collapse to norm(...), got: {}",
            result
        );
    }

    #[test]
    fn test_lr_wrapped_matrix_with_trivial_grouping() {
        let result = latex_to_typst(r"\left|{\begin{array}{cc} a & b \\ c & d \end{array}}\right|");
        assert!(
            result.contains("mat(delim: \"|\"") || result.contains("mat(delim: \"|\", "),
            "single curly wrapper should still classify array as matrix-like, got: {}",
            result
        );
        assert!(
            !result.contains("abs("),
            "single curly wrapper should not force abs(...), got: {}",
            result
        );

        let result =
            latex_to_typst(r"\left| {{\begin{array}{cc} a & b \\ c & d \end{array}} } \right|");
        assert!(
            result.contains("mat(delim: \"|\"") || result.contains("mat(delim: \"|\", "),
            "nested trivial wrappers should still classify array as matrix-like, got: {}",
            result
        );
        assert!(
            !result.contains("abs("),
            "nested trivial wrappers should not force abs(...), got: {}",
            result
        );
    }

    #[test]
    fn test_comparison_operators() {
        let tests = [(r"\leq", "leq"), (r"\geq", "geq"), (r"\neq", "neq")];

        for (latex, _) in tests {
            let result = latex_to_typst(latex);
            assert!(
                !result.is_empty() && !result.contains("Error"),
                "Failed for {}: {}",
                latex,
                result
            );
        }
    }

    #[test]
    fn test_dots_commands() {
        assert_eq!(latex_to_typst(r"\ldots").trim(), "...");
        assert_eq!(latex_to_typst(r"\cdots").trim(), "dots.c");
    }

    #[test]
    fn test_text_in_math() {
        let result = latex_to_typst(r"\text{hello}");
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_complex_expression() {
        let expr = r"\frac{d}{dx}\left(\int_0^x f(t) dt\right) = f(x)";
        let result = latex_to_typst(expr);
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_math_spacing() {
        // \, -> thin
        let result = latex_to_typst(r"a \, b");
        assert!(
            result.contains("thin"),
            "\\, should become 'thin' in math mode, got: {}",
            result
        );

        // \: -> med (handled by TEX_COMMAND_SPEC alias)
        let result = latex_to_typst(r"a \: b");
        assert!(
            result.contains("med"),
            "\\: should become 'med' in math mode, got: {}",
            result
        );

        // \; -> thick
        let result = latex_to_typst(r"a \; b");
        assert!(
            result.contains("thick"),
            "\\; should become 'thick' in math mode, got: {}",
            result
        );

        // \quad -> quad
        let result = latex_to_typst(r"a \quad b");
        assert!(
            result.contains("quad"),
            "\\quad should become 'quad' in math mode, got: {}",
            result
        );

        // \qquad -> wide
        let result = latex_to_typst(r"a \qquad b");
        assert!(
            result.contains("wide"),
            "\\qquad should become 'wide' in math mode, got: {}",
            result
        );
    }

    #[test]
    fn test_argmin() {
        // Test variants of argmin

        // 1. operatorname with thin space (common)
        let res1 = latex_to_typst(r"\operatorname*{arg\,min}_\theta");
        assert!(
            res1.contains("argmin") || res1.contains("arg min"),
            "Failed for operatorname: {}",
            res1
        );

        // 2. Built-in command
        let res2 = latex_to_typst(r"\argmin_\theta");
        assert!(
            res2.contains("argmin") || res2.contains("arg min"),
            "Failed for built-in: {}",
            res2
        );

        // 3. DeclareMathOperator (ignored but shouldn't break argmin)
        let _res3 = latex_to_typst(r"\DeclareMathOperator*{\argmin}{arg\,min} \argmin_\theta");
        // Since DeclareMathOperator is ignored, argmin is unknown command -> outputs only argument (subscript)
        // This confirms the user's issue if they use \argmin defined this way.
        // But if they use \operatorname*{arg\,min}, it should work.
        // If my fix works, res1 should be "limits(op("argmin"))_(\theta)"

        // Let's assert res1 specifically
        assert!(
            res1.contains("limits(op(\"argmin\"))"),
            "Strict check failed for operatorname: {}",
            res1
        );
    }

    #[test]
    fn test_mathop_upright_text_becomes_operator() {
        let result = latex_to_typst(r"$f(X)=\mathop{\mathrm{Tr}} (ZX)$");
        assert!(
            result.contains(r#"op("Tr")"#),
            "mathop over upright Tr should become op(\"Tr\"), got: {}",
            result
        );
        assert!(
            !result.contains(r#"class("large", upright(Tr))"#),
            "mathop over upright Tr should not stay as class(\"large\", upright(Tr)), got: {}",
            result
        );
    }

    #[test]
    fn test_mathop_operator_like_variants() {
        let rm = latex_to_typst(r"$\mathop{\rm Tr}$");
        assert!(
            rm.contains(r#"op("Tr")"#),
            "mathop over legacy rm Tr should become op(\"Tr\"), got: {}",
            rm
        );

        let bare = latex_to_typst(r"$\mathop{Tr}$");
        assert!(
            bare.contains(r#"op("Tr")"#),
            "mathop over bare Tr should become op(\"Tr\"), got: {}",
            bare
        );

        let operatorname = latex_to_typst(r"$\mathop{\operatorname{diag}} x$");
        assert!(
            operatorname.contains(r#"op("diag") x"#) || operatorname.contains(r#"op("diag")  x"#),
            "mathop over nested operatorname diag should become op(\"diag\"), got: {}",
            operatorname
        );

        let argmax = latex_to_typst(r"$\mathop{\mathrm{argmax}}$");
        assert!(
            argmax.contains(r#"op("argmax")"#),
            "mathop over upright argmax should become op(\"argmax\"), got: {}",
            argmax
        );
    }

    #[test]
    fn test_mathop_text_wrappers_become_operator() {
        let text = latex_to_typst(r"$\mathop{\text{Tr}}$");
        assert!(
            text.contains(r#"op("Tr")"#),
            "mathop over text Tr should become op(\"Tr\"), got: {}",
            text
        );

        let textnormal = latex_to_typst(r"$\mathop{\textnormal{Tr}}$");
        assert!(
            textnormal.contains(r#"op("Tr")"#),
            "mathop over textnormal Tr should become op(\"Tr\"), got: {}",
            textnormal
        );

        let textrm = latex_to_typst(r"$\mathop{\textrm{Tr}}$");
        assert!(
            textrm.contains(r#"op("Tr")"#),
            "mathop over textrm Tr should become op(\"Tr\"), got: {}",
            textrm
        );
    }

    #[test]
    fn test_mathop_complex_cases_keep_fallback() {
        let plus = latex_to_typst(r"$\mathop{A+B}$");
        assert!(
            plus.contains(r#"class("large""#) && !plus.contains(r#"op("A+B")"#),
            "mathop over A+B should keep class fallback, got: {}",
            plus
        );

        let frac = latex_to_typst(r"$\mathop{\frac12}$");
        assert!(
            frac.contains(r#"class("large""#) && !frac.contains(r#"op("12")"#),
            "mathop over frac should keep class fallback, got: {}",
            frac
        );

        let sum = latex_to_typst(r"$\mathop{\sum}$");
        assert!(
            sum.contains(r#"class("large""#) && !sum.contains(r#"op("sum")"#),
            "mathop over sum should keep class fallback, got: {}",
            sum
        );

        let lr = latex_to_typst(r"$\mathop{\left( x \right)}$");
        assert!(
            lr.contains(r#"class("large""#),
            "mathop over left-right group should keep class fallback, got: {}",
            lr
        );

        let bold = latex_to_typst(r"$\mathop{\mathbf{T}}$");
        assert!(
            bold.contains(r#"class("large""#) && !bold.contains(r#"op("T")"#),
            "mathop over bold T should not be treated as operator name, got: {}",
            bold
        );

        let differential = latex_to_typst(r"$\mathop{\mathrm{d}}$");
        assert!(
            !differential.contains(r#"op("d")"#) && !differential.contains(r#"op("dif")"#),
            "mathop over upright d should not be promoted to op(...), got: {}",
            differential
        );
    }
}

// ============================================================================
// Math Mode Tests - Typst to LaTeX
// ============================================================================

mod t2l_math {
    use super::*;

    #[test]
    fn test_greek_letters() {
        let result = typst_to_latex("alpha + beta = gamma");
        assert!(result.contains("alpha") || result.contains("\\alpha"));
        assert!(result.contains("beta") || result.contains("\\beta"));
    }

    #[test]
    fn test_fractions() {
        let result = typst_to_latex("$frac(1, 2)$");
        assert!(result.contains("\\frac"));
        assert!(result.contains("{1}"));
        assert!(result.contains("{2}"));
    }

    #[test]
    fn test_sqrt() {
        let result = typst_to_latex("$sqrt(x)$");
        assert!(result.contains("\\sqrt"));
    }

    #[test]
    fn test_subscripts_superscripts() {
        let result = typst_to_latex("$x^2$");
        assert!(result.contains("^"));

        let result = typst_to_latex("$x_i$");
        assert!(result.contains("_"));
    }

    #[test]
    fn test_matrix() {
        let result = typst_to_latex("$mat(1, 2; 3, 4)$");
        assert!(result.contains("\\begin{matrix}") || result.contains("matrix"));
    }

    #[test]
    fn test_operators() {
        let result = typst_to_latex("a + b - c = d");
        assert!(result.contains("+"));
        assert!(result.contains("-"));
        assert!(result.contains("="));
    }

    #[test]
    fn test_math_spacing() {
        // thin -> \,
        let result = typst_to_latex("$a thin b$");
        assert!(
            result.contains("\\,"),
            "thin should become \\, , got: {}",
            result
        );

        // med -> \:
        let result = typst_to_latex("$a med b$");
        assert!(
            result.contains("\\:"),
            "med should become \\: , got: {}",
            result
        );

        // thick -> \;
        let result = typst_to_latex("$a thick b$");
        assert!(
            result.contains("\\;"),
            "thick should become \\; , got: {}",
            result
        );

        // quad -> \quad
        let result = typst_to_latex("$a quad b$");
        assert!(
            result.contains("\\quad"),
            "quad should become \\quad, got: {}",
            result
        );

        // wide -> \qquad
        let result = typst_to_latex("$a wide b$");
        assert!(
            result.contains("\\qquad"),
            "wide should become \\qquad, got: {}",
            result
        );
    }
}

// ============================================================================
// Document Mode Tests - LaTeX to Typst
// ============================================================================

mod l2t_document {
    use super::*;

    #[test]
    fn test_simple_document() {
        let latex = r#"
\documentclass{article}
\title{My Document}
\author{John Doe}
\begin{document}
\maketitle
Hello, world!
\end{document}
"#;

        let result = latex_document_to_typst(latex);

        // Should contain document content (AST converter may handle metadata differently)
        assert!(!result.is_empty(), "Result should not be empty");
        // The document should contain the body content
        assert!(
            result.contains("Hello") || result.contains("world"),
            "Missing body content: {}",
            result
        );
    }

    #[test]
    fn test_document_with_sections() {
        let latex = r#"
\documentclass{article}
\begin{document}
\section{Introduction}
This is the intro.
\section{Methods}
This is methods.
\end{document}
"#;

        let result = latex_document_to_typst(latex);
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_document_class_detection() {
        let article =
            latex_document_to_typst(r"\documentclass{article}\begin{document}Test\end{document}");
        assert!(article.contains("a4") || !article.contains("Error"));

        let book =
            latex_document_to_typst(r"\documentclass{book}\begin{document}Test\end{document}");
        assert!(book.contains("heading") || !book.contains("Error"));
    }

    #[test]
    fn test_document_with_math() {
        let latex = r#"
\documentclass{article}
\begin{document}
The formula $E = mc^2$ is famous.
\end{document}
"#;

        let result = latex_document_to_typst(latex);
        assert!(!result.contains("Error"));
    }
}

// ============================================================================
// Document Mode Tests - Typst to LaTeX
// ============================================================================

mod t2l_document {
    use super::*;

    #[test]
    fn test_heading_conversion() {
        let typst = "= Main Title";
        let result = typst_to_latex_with_options(typst, &T2LOptions::default());
        assert!(
            result.contains("\\section"),
            "Expected section, got: {}",
            result
        );
    }

    #[test]
    fn test_subsection_conversion() {
        let typst = "== Subsection";
        let result = typst_to_latex_with_options(typst, &T2LOptions::default());
        assert!(
            result.contains("\\subsection"),
            "Expected subsection, got: {}",
            result
        );
    }

    #[test]
    fn test_bold_conversion() {
        let typst = "*bold text*";
        let result = typst_to_latex_with_options(typst, &T2LOptions::default());
        assert!(
            result.contains("\\textbf"),
            "Expected textbf, got: {}",
            result
        );
    }

    #[test]
    fn test_italic_conversion() {
        let typst = "_italic text_";
        let result = typst_to_latex_with_options(typst, &T2LOptions::default());
        assert!(
            result.contains("\\textit"),
            "Expected textit, got: {}",
            result
        );
    }

    #[test]
    fn test_full_document_wrapper() {
        let typst = "= Title\n\nSome text.";
        let result = typst_to_latex_with_options(typst, &T2LOptions::full_document());

        assert!(result.contains("\\documentclass"), "Missing documentclass");
        assert!(
            result.contains("\\begin{document}"),
            "Missing begin document"
        );
        assert!(result.contains("\\end{document}"), "Missing end document");
    }

    #[test]
    fn test_inline_code() {
        let typst = "`code`";
        let result = typst_to_latex_with_options(typst, &T2LOptions::default());
        assert!(
            result.contains("\\texttt"),
            "Expected texttt, got: {}",
            result
        );
    }

    #[test]
    fn test_inline_math_in_document() {
        let typst = "The formula $x + y$ is simple.";
        let result = typst_to_latex_with_options(typst, &T2LOptions::default());
        assert!(
            result.contains("$"),
            "Expected math delimiters, got: {}",
            result
        );
    }
}

// ============================================================================
// Auto-Detection Tests
// ============================================================================

mod auto_detection {
    use super::*;

    #[test]
    fn test_detect_latex() {
        assert_eq!(detect_format(r"\documentclass{article}"), "latex");
        assert_eq!(detect_format(r"\frac{1}{2}"), "latex");
        assert_eq!(detect_format(r"\begin{document}"), "latex");
        assert_eq!(detect_format(r"\alpha + \beta"), "latex");
    }

    #[test]
    fn test_detect_typst() {
        assert_eq!(detect_format("#set page(paper: \"a4\")"), "typst");
        assert_eq!(detect_format("= Heading"), "typst");
        assert_eq!(detect_format("#import \"test.typ\""), "typst");
    }

    #[test]
    fn test_convert_auto_latex() {
        let (result, format) = convert_auto(r"\frac{1}{2}");
        assert_eq!(format, "typst");
        // With frac_to_slash enabled by default, simple fractions may use slash notation
        assert!(result.contains("frac") || result.contains("/"));
    }

    #[test]
    fn test_convert_auto_typst() {
        let (result, format) = convert_auto("alpha + beta");
        assert_eq!(format, "latex");
        assert!(result.contains("alpha"));
    }

    #[test]
    fn test_convert_auto_document_latex() {
        let input = r"\documentclass{article}\begin{document}Test\end{document}";
        let (result, format) = convert_auto_document(input);
        assert_eq!(format, "typst");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_convert_auto_document_typst() {
        let input = "= Heading\n\nSome content.";
        let (result, format) = convert_auto_document(input);
        assert_eq!(format, "latex");
        assert!(result.contains("section") || result.contains("Heading"));
    }
}

// ============================================================================
// Roundtrip Tests
// ============================================================================

mod roundtrip {
    use super::*;

    #[test]
    fn test_roundtrip_greek_letters() {
        let original = r"\alpha + \beta = \gamma";
        let typst = latex_to_typst(original);
        let back = typst_to_latex(&typst);

        // AST outputs Unicode (α, β, γ) which t2l may not convert back to \alpha
        // Accept either Unicode Greek letters or LaTeX commands in the output
        assert!(
            back.contains("alpha")
                || back.contains("\\alpha")
                || back.contains("α")
                || typst.contains("α")
                || typst.contains("alpha"),
            "Expected Greek letter alpha in roundtrip, got typst='{}' back='{}'",
            typst,
            back
        );
    }

    #[test]
    fn test_roundtrip_fraction() {
        let original = r"\frac{1}{2}";
        let typst = latex_to_typst(original);
        // Wrap in $ for round-trip to preserve math mode
        let typst_math = format!("${}$", typst);
        let back = typst_to_latex(&typst_math);

        assert!(back.contains("frac") || back.contains("\\frac") || back.contains("/"));
    }

    #[test]
    fn test_roundtrip_typst_to_latex() {
        let original = "$frac(a, b)$";
        let latex = typst_to_latex(original);
        let back = latex_to_typst(&latex);

        // With frac_to_slash enabled by default, simple fractions may use slash notation
        assert!(back.contains("frac") || back.contains("/"));
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_input() {
        let result = latex_to_typst("");
        assert!(result.is_empty() || !result.contains("Error"));

        let result = typst_to_latex("");
        assert!(result.is_empty() || !result.contains("Error"));
    }

    #[test]
    fn test_whitespace_only() {
        let result = latex_to_typst("   ");
        assert!(!result.contains("Error"));

        let result = typst_to_latex("   ");
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_special_characters() {
        // LaTeX special chars
        let result = typst_to_latex_with_options("&%$", &T2LOptions::default());
        // Should escape or handle gracefully
        assert!(!result.contains("Error"));
    }

    #[test]
    fn test_nested_structures() {
        let result = latex_to_typst(r"\frac{\frac{1}{2}}{\frac{3}{4}}");
        assert!(!result.contains("Error"));
        assert!(result.contains("frac"));
    }

    #[test]
    fn test_unicode() {
        let result = typst_to_latex_with_options("α + β = γ", &T2LOptions::default());
        // Should handle unicode gracefully
        assert!(!result.is_empty());
    }

    #[test]
    fn test_long_expression() {
        let long_expr = r"\sum_{i=1}^{100} \frac{1}{i^2} = \frac{\pi^2}{6}";
        let result = latex_to_typst(long_expr);
        assert!(!result.contains("Error"));
    }
}

// ============================================================================
// Options Tests
// ============================================================================

mod options {
    use super::*;

    #[test]
    fn test_l2t_math_only() {
        // Now using the default latex_to_typst which handles math mode
        let result = latex_to_typst(r"\frac{1}{2}");
        // With frac_to_slash enabled by default, simple fractions may use slash notation
        assert!(result.contains("frac") || result.contains("/"));
    }

    #[test]
    fn test_l2t_full_document() {
        // Now using latex_document_to_typst for full document conversion
        let latex = r"\documentclass{article}\begin{document}Test\end{document}";
        let result = latex_document_to_typst(latex);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_t2l_math_only() {
        let opts = T2LOptions::math_only();
        let result = typst_to_latex_with_options("frac(1, 2)", &opts);
        assert!(result.contains("\\frac"));
    }

    #[test]
    fn test_t2l_full_document() {
        let opts = T2LOptions::full_document();
        let result = typst_to_latex_with_options("= Title\n\nContent", &opts);
        assert!(result.contains("\\documentclass"));
        assert!(result.contains("\\begin{document}"));
    }

    #[test]
    fn test_t2l_custom_document_class() {
        let mut opts = T2LOptions::full_document();
        opts.document_class = "report".to_string();
        let result = typst_to_latex_with_options("= Title", &opts);
        assert!(result.contains("\\documentclass{report}"));
    }

    #[test]
    fn test_t2l_with_title() {
        let mut opts = T2LOptions::full_document();
        opts.title = Some("My Document".to_string());
        opts.author = Some("Author Name".to_string());
        let result = typst_to_latex_with_options("Content", &opts);
        assert!(result.contains("\\title{My Document}"));
        assert!(result.contains("\\author{Author Name}"));
        assert!(result.contains("\\maketitle"));
    }
}

// ============================================================================
// CeTZ <-> TikZ Conversion Tests
// ============================================================================

mod tikz_cetz {
    use tylax::tikz::{convert_cetz_to_tikz, convert_tikz_to_cetz, is_cetz_code};

    #[test]
    fn test_tikz_line_to_cetz() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) -- (1,1) -- (2,0);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("line"));
        assert!(cetz.contains("canvas"));
    }

    #[test]
    fn test_cetz_line_to_tikz() {
        let cetz = r#"
import "@preview/cetz:0.2.0"
canvas({
  line((0, 0), (1, 1))
})
"#;
        let tikz = convert_cetz_to_tikz(cetz);
        assert!(tikz.contains("\\begin{tikzpicture}"));
        assert!(tikz.contains("\\draw"));
        assert!(tikz.contains("\\end{tikzpicture}"));
    }

    #[test]
    fn test_cetz_detection() {
        assert!(is_cetz_code("import \"@preview/cetz:0.2.0\""));
        assert!(is_cetz_code("canvas({ line((0,0), (1,1)) })"));
        assert!(!is_cetz_code("\\begin{tikzpicture}"));
    }

    #[test]
    fn test_tikz_circle_roundtrip() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) circle (1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(
            cetz.contains("circle") || cetz.contains("line"),
            "CeTZ output: {}",
            cetz
        );
    }

    #[test]
    fn test_tikz_node_to_cetz() {
        let tikz = r"\begin{tikzpicture}\node at (0,0) {Hello};\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("content"));
        assert!(cetz.contains("Hello"));
    }
}

// ============================================================================
// Preprocessing Tests
// ============================================================================

mod preprocessing {
    use tylax::typst2latex::{extract_let_definitions, preprocess_typst};

    #[test]
    fn test_simple_let_extraction() {
        let input = r#"#let x = 5
#let name = "hello"
The value is #x"#;

        let (db, cleaned) = extract_let_definitions(input);
        assert_eq!(db.get_variable("x"), Some("5"));
        assert!(!cleaned.contains("#let x"));
    }

    #[test]
    fn test_preprocess_expansion() {
        let input = r#"#let greeting = "Hello"
#greeting World"#;

        let result = preprocess_typst(input);
        // After preprocessing, the let should be removed
        assert!(!result.contains("#let"));
    }

    #[test]
    fn test_math_variable() {
        let input = r#"#let pi = $\pi$
The value is #pi"#;

        let (db, _) = extract_let_definitions(input);
        assert!(db.is_defined("pi"));
    }

    #[test]
    fn test_multiple_definitions() {
        let input = r#"#let a = 1
#let b = 2
#let c = 3
Result: a + b + c"#;

        let (db, cleaned) = extract_let_definitions(input);
        assert_eq!(db.len(), 3);
        assert!(cleaned.contains("Result"));
    }
}

// ============================================================================
// Additional Round-trip Tests
// ============================================================================

mod roundtrip_extended {
    use super::*;

    #[test]
    fn test_simple_math_roundtrip() {
        // LaTeX -> Typst -> LaTeX
        let original = r"\frac{1}{2}";
        let typst = latex_to_typst(original);
        let back = typst_to_latex(&typst);

        // Should contain frac in some form
        assert!(
            back.contains("frac") || back.contains("/"),
            "Round-trip failed. Typst: {}, Back: {}",
            typst,
            back
        );
    }

    #[test]
    fn test_greek_roundtrip() {
        let original = r"\alpha + \beta = \gamma";
        let typst = latex_to_typst(original);

        // Typst should have alpha, beta, gamma (or Unicode equivalents)
        assert!(
            typst.contains("alpha") || typst.contains("α"),
            "Expected alpha in: {}",
            typst
        );
        assert!(
            typst.contains("beta") || typst.contains("β"),
            "Expected beta in: {}",
            typst
        );
        assert!(
            typst.contains("gamma") || typst.contains("γ"),
            "Expected gamma in: {}",
            typst
        );
    }

    #[test]
    fn test_subscript_roundtrip() {
        let original = r"x_1 + x_2";
        let typst = latex_to_typst(original);
        // Wrap in $ for round-trip to preserve math mode
        let typst_math = format!("${}$", typst);
        let back = typst_to_latex(&typst_math);

        assert!(back.contains("_"));
    }

    #[test]
    fn test_superscript_roundtrip() {
        let original = r"x^2 + y^3";
        let typst = latex_to_typst(original);
        // Wrap in $ for round-trip to preserve math mode
        let typst_math = format!("${}$", typst);
        let back = typst_to_latex(&typst_math);

        assert!(back.contains("^"));
    }
}

// ============================================================================
// Regression Tests
// ============================================================================

mod regression {
    use super::*;

    #[test]
    fn test_aligned_with_linebreak() {
        // LaTeX uses \\ for line breaks in aligned environments
        let input = r"\begin{aligned} x &= 1 \\ y &= 2 \end{aligned}";

        let result = latex_to_typst(input);

        // Should not contain "aligned(" function call
        assert!(
            !result.contains("aligned("),
            "Should not have aligned() function"
        );

        // Should not contain "Error"
        assert!(!result.contains("Error"), "Should not have error");
    }

    #[test]
    fn test_align_env_with_linebreak() {
        let input = r"\begin{align} a &= b \\ c &= d \end{align}";

        let result = latex_to_typst(input);

        assert!(!result.contains("Error"), "Should not have error");
    }
}

// ============================================================================
// Color Tests
// ============================================================================

mod color_tests {
    use tylax::latex_document_to_typst;

    // Helper: wrap content in document environment for proper parsing
    fn wrap_doc(content: &str) -> String {
        format!(
            r"\documentclass{{article}}\begin{{document}}{}\end{{document}}",
            content
        )
    }

    #[test]
    fn test_textcolor_basic() {
        let input = wrap_doc(r"\textcolor{red}{important text}");
        let result = latex_document_to_typst(&input);
        println!("Output: {}", result);
        assert!(
            result.contains("#text(fill: red)"),
            "Should have text with red fill"
        );
        assert!(
            result.contains("important text"),
            "Should contain the text content"
        );
    }

    #[test]
    fn test_textcolor_named_color() {
        // ForestGreen is a dvipsnames color → rgb("#009B55")
        let input = wrap_doc(r"\textcolor{ForestGreen}{green text}");
        let result = latex_document_to_typst(&input);
        println!("Output: {}", result);
        assert!(
            result.contains("rgb("),
            "Named color should be converted to rgb"
        );
        assert!(
            result.contains("#009B55"),
            "ForestGreen should map to #009B55"
        );
        assert!(
            result.contains("green text"),
            "Should contain the text content"
        );
    }

    #[test]
    fn test_colorbox() {
        let input = wrap_doc(r"\colorbox{yellow}{highlighted}");
        let result = latex_document_to_typst(&input);
        println!("Output: {}", result);
        assert!(
            result.contains("#box(fill: yellow"),
            "Should have box with yellow fill"
        );
        assert!(
            result.contains("highlighted"),
            "Should contain the text content"
        );
    }

    #[test]
    fn test_fcolorbox() {
        let input = wrap_doc(r"\fcolorbox{red}{yellow}{framed box}");
        let result = latex_document_to_typst(&input);
        println!("Output: {}", result);
        assert!(
            result.contains("fill: yellow"),
            "Should have yellow background"
        );
        assert!(result.contains("stroke: red"), "Should have red border");
        assert!(
            result.contains("framed box"),
            "Should contain the text content"
        );
    }

    #[test]
    fn test_color_mixing() {
        // xcolor mixing syntax: blue!50!white means 50% blue, 50% white
        let input = wrap_doc(r"\textcolor{blue!50!white}{mixed color}");
        let result = latex_document_to_typst(&input);
        println!("Output: {}", result);
        assert!(result.contains("color.mix"), "Should use Typst color.mix");
        assert!(
            result.contains("mixed color"),
            "Should contain the text content"
        );
    }

    #[test]
    fn test_highlight() {
        let input = wrap_doc(r"\hl{important}");
        let result = latex_document_to_typst(&input);
        println!("Output: {}", result);
        assert!(result.contains("#highlight"), "Should have highlight");
        assert!(
            result.contains("important"),
            "Should contain the text content"
        );
    }
}

// ============================================================================
// SOTA Macro Engine Regression Tests
// ============================================================================

mod complex_stress_test {
    use tylax::core::latex2typst::engine::expand_latex;
    use tylax::core::latex2typst::{latex_to_typst, latex_to_typst_with_eval};
    use tylax::typst_to_latex_with_eval;
    use tylax::T2LOptions;

    /// Debug test: check if text spacing is correct in L2T output
    #[test]
    fn test_l2t_text_no_char_spacing() {
        // Test 1: Engine expansion should produce correct output
        let input = r"\textbf{hello}";
        let expanded = expand_latex(input);
        println!("Engine expanded: {:?}", expanded);
        assert!(
            !expanded.contains("h e l l o"),
            "Engine should not add spaces between chars"
        );

        // Test 2: L2T conversion should not add spaces between characters
        let output = latex_to_typst(input);
        println!("L2T output: {:?}", output);
        // Check that output doesn't have "h e l l o" pattern
        assert!(
            !output.contains("h e l l o"),
            "L2T should not add spaces between text chars. Got: {}",
            output
        );
    }

    /// Debug test: check full document spacing
    #[test]
    fn test_l2t_document_spacing() {
        let input = r"\documentclass{article}
\begin{document}
\section{Hello World}
This is a test.
\end{document}";
        let output = latex_to_typst_with_eval(input);
        println!("Full document output:\n{}", output);

        // Check for characteristic spacing bugs
        assert!(
            !output.contains("a r t i c l e"),
            "Should not have spaced 'article'"
        );
        assert!(
            !output.contains("H e l l o"),
            "Should not have spaced 'Hello'"
        );
        assert!(!output.contains("T h i s"), "Should not have spaced 'This'");
    }

    /// Test delimited arguments: \def\foo#1.{...}
    #[test]
    fn test_delimited_args() {
        let input = r"\def\grabuntildot#1.{\textbf{[#1]}} \grabuntildot hello world.";
        let result = expand_latex(input);
        println!("Delimited args: {}", result);
        assert!(
            result.contains(r"\textbf{[hello world]}"),
            "Delimited arg should capture until dot. Got: {}",
            result
        );
    }

    /// Test DeferredParam (##) for nested macro definitions
    #[test]
    fn test_deferred_param() {
        let input =
            r"\def\mkinner#1{\def\inner##1{(##1 ; outer=#1)}} \mkinner{OUTERVAL} \inner{INNERVAL}";
        let result = expand_latex(input);
        println!("DeferredParam: {}", result);
        assert!(
            result.contains("INNERVAL"),
            "Inner arg should be present. Got: {}",
            result
        );
        assert!(
            result.contains("OUTERVAL"),
            "Outer arg should be present. Got: {}",
            result
        );
    }

    /// Test \csname + \expandafter for dynamic control sequences
    #[test]
    fn test_csname_expandafter() {
        let input = r"\def\setvar#1#2{\expandafter\def\csname var@#1\endcsname{#2}} \def\getvar#1{\csname var@#1\endcsname} \setvar{foo}{123} \getvar{foo}";
        let result = expand_latex(input);
        println!("csname+expandafter: {}", result);
        assert!(
            result.contains("123"),
            "Dynamic var should expand to 123. Got: {}",
            result
        );
    }

    /// Test \newif conditional
    #[test]
    fn test_newif_conditional() {
        let input = r"\newif\ifdebug \debugtrue \ifdebug YES\else NO\fi";
        let result = expand_latex(input);
        println!("newif true: {}", result);
        assert!(
            result.contains("YES"),
            "Debug true should give YES. Got: {}",
            result
        );

        let input2 = r"\newif\ifdebug \debugfalse \ifdebug YES\else NO\fi";
        let result2 = expand_latex(input2);
        println!("newif false: {}", result2);
        assert!(
            result2.contains("NO"),
            "Debug false should give NO. Got: {}",
            result2
        );
    }

    /// Test \ifx for token comparison
    #[test]
    fn test_ifx_comparison() {
        let input = r"\def\A{X} \def\B{X} \ifx\A\B SAME\else DIFF\fi";
        let result = expand_latex(input);
        println!("ifx same: {}", result);
        assert!(
            result.contains("SAME"),
            "Same definition should give SAME. Got: {}",
            result
        );

        let input2 = r"\def\A{X} \def\C{Y} \ifx\A\C SAME\else DIFF\fi";
        let result2 = expand_latex(input2);
        println!("ifx diff: {}", result2);
        assert!(
            result2.contains("DIFF"),
            "Different definition should give DIFF. Got: {}",
            result2
        );
    }

    /// Test xspace
    #[test]
    fn test_xspace() {
        let input = r"\def\TeXmacro{TeX\xspace} \TeXmacro is great.";
        let result = expand_latex(input);
        println!("xspace: {}", result);
        assert!(
            result.contains("TeX ") || result.contains("TeX  "),
            "xspace should insert space before 'is'. Got: {}",
            result
        );
    }

    /// Test complex Typst to LaTeX with MiniEval
    #[test]
    fn test_typst_fib_table() {
        let input = r#"
#let fib(n) = {
  if n <= 2 { 1 }
  else { fib(n - 1) + fib(n - 2) }
}

#let count = 5
#let nums = range(1, count + 1)

#table(
  columns: count,
  ..nums.map(n => $F_#n$),
  ..nums.map(n => str(fib(n))),
)
"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        println!("Typst fib table:\n{}", result);

        // Should contain table structure
        assert!(
            result.contains("tabular") || result.contains("table"),
            "Should have table. Got: {}",
            result
        );
        // Should have fibonacci values
        assert!(
            result.contains("1")
                && result.contains("2")
                && result.contains("3")
                && result.contains("5"),
            "Should have fib values. Got: {}",
            result
        );
    }

    /// Test Typst higher-order functions
    #[test]
    fn test_typst_higher_order() {
        let input = r#"
#let make_adder(k) = (x) => x + k
#let add3 = make_adder(3)
#let xs = (1, 2, 3)
Result: #(xs.map(add3).map(str).join(", "))
"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        println!("Higher-order: {}", result);
        assert!(
            result.contains("4") && result.contains("5") && result.contains("6"),
            "Should have 4, 5, 6 (1+3, 2+3, 3+3). Got: {}",
            result
        );
    }

    /// Test Typst conditional content
    #[test]
    fn test_typst_conditional() {
        let input = r#"
#let debug = true
#if debug {
  Debug is enabled.
} else {
  Debug is disabled.
}
"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        println!("Conditional: {}", result);
        assert!(
            result.contains("enabled"),
            "Should contain 'enabled'. Got: {}",
            result
        );
    }
}

mod macro_engine_regression {
    use tylax::core::latex2typst::engine::expand_latex;
    use tylax::latex_document_to_typst;

    /// Regression test for the \pair macro issue from Pandoc comparison
    /// This was the motivating example for the SOTA token-based engine
    #[test]
    fn test_pair_macro_nested_braces() {
        // The exact example from the Pandoc comparison
        let input = r"\newcommand{\pair}[2]{\langle #1, #2\rangle} \pair{a^2}{\frac{\pi}{2}}";
        let result = expand_latex(input);

        println!("Expanded: {}", result);

        // The nested braces should be preserved correctly
        assert!(
            result.contains(r"\langle a^2, \frac{\pi}{2}\rangle"),
            "Expected nested braces to be preserved. Got: {}",
            result
        );
    }

    #[test]
    fn test_simple_macro_expansion() {
        let input = r"\newcommand{\foo}{bar} \foo";
        let result = expand_latex(input);
        assert!(
            result.contains("bar"),
            "Simple macro should expand. Got: {}",
            result
        );
        assert!(
            !result.contains(r"\foo"),
            "Macro call should be replaced. Got: {}",
            result
        );
    }

    #[test]
    fn test_macro_with_args() {
        let input = r"\newcommand{\wrap}[1]{[#1]} \wrap{hello}";
        let result = expand_latex(input);
        assert!(
            result.contains("[hello]"),
            "Macro with args should expand. Got: {}",
            result
        );
    }

    #[test]
    fn test_def_macro() {
        let input = r"\def\foo#1{<<#1>>} \foo{world}";
        let result = expand_latex(input);
        assert!(
            result.contains("<<world>>"),
            "\\def macro should expand. Got: {}",
            result
        );
    }

    #[test]
    fn test_deeply_nested_braces() {
        let input = r"\newcommand{\deep}[1]{[#1]} \deep{a{b{c}d}e}";
        let result = expand_latex(input);
        assert!(
            result.contains("[a{b{c}d}e]"),
            "Deeply nested braces should be preserved. Got: {}",
            result
        );
    }

    #[test]
    fn test_recursive_macro_expansion() {
        let input = r"\newcommand{\outer}[1]{<\inner{#1}>} \newcommand{\inner}[1]{(#1)} \outer{x}";
        let result = expand_latex(input);
        assert!(
            result.contains("<(x)>"),
            "Recursive macros should expand. Got: {}",
            result
        );
    }

    #[test]
    fn test_full_document_with_macros() {
        let input = r"
\documentclass{article}
\newcommand{\pair}[2]{\langle #1, #2\rangle}
\begin{document}
$$\pair{a^2}{\frac{\pi}{2}}$$
\end{document}
";
        let result = latex_document_to_typst(input);
        println!("Full document result:\n{}", result);

        // The result should contain the expanded macro content
        // chevron.l is Typst 0.14+ for \langle (was angle.l)
        assert!(
            result.contains("chevron.l") || result.contains("angle.l") || result.contains("langle"),
            "Should contain left angle bracket. Got: {}",
            result
        );
        assert!(
            result.contains("chevron.r") || result.contains("angle.r") || result.contains("rangle"),
            "Should contain right angle bracket. Got: {}",
            result
        );
        // The pi should be preserved
        assert!(
            result.contains("pi") || result.contains("π"),
            "Should contain pi. Got: {}",
            result
        );
    }
}

// ============================================================================
// Warning System Tests
// ============================================================================

// ============================================================================
// Engine Edge Cases - Integration Tests
// ============================================================================

mod engine_edge_cases {
    use tylax::core::latex2typst::engine::expand_latex;
    use tylax::typst_to_latex_with_eval;
    use tylax::T2LOptions;

    // --- LaTeX Engine: Recursion Tests ---

    #[test]
    fn test_latex_direct_recursion_safety() {
        use tylax::core::latex2typst::engine::{detokenize, tokenize, Engine};
        // Direct recursion: \def\a{\a} \a
        // Use manual Engine with low depth limit to verify safety mechanism prevents stack overflow
        let mut engine = Engine::new().with_max_depth(50);
        let input = tokenize(r"\def\a{\a} \a");
        let output = engine.process(input);
        let result = detokenize(&output);
        // Should not panic, should return something (original or partial)
        assert!(!result.is_empty(), "Should not panic on direct recursion");
    }

    #[test]
    fn test_latex_indirect_recursion_safety() {
        use tylax::core::latex2typst::engine::{detokenize, tokenize, Engine};
        // Indirect recursion: \def\a{\b}\def\b{\a} \a
        let mut engine = Engine::new().with_max_depth(50);
        let input = tokenize(r"\def\a{\b}\def\b{\a} \a");
        let output = engine.process(input);
        let result = detokenize(&output);
        assert!(!result.is_empty(), "Should not panic on indirect recursion");
    }

    #[test]
    fn test_latex_deep_but_valid_chain() {
        // Deep but valid: \def\a{\b}\def\b{\c}\def\c{x} \a
        let input = r"\def\a{\b}\def\b{\c}\def\c{x} \a";
        let result = expand_latex(input);
        assert!(
            result.contains("x"),
            "Deep valid chain should resolve to x. Got: {}",
            result
        );
    }

    // --- LaTeX Engine: Scope Tests ---

    #[test]
    fn test_latex_scope_isolation_integration() {
        // Definition inside group should not leak
        let input = r"{\def\inner{INSIDE} \inner} \inner";
        let result = expand_latex(input);
        // Inside: should have INSIDE
        assert!(
            result.contains("INSIDE"),
            "Inner def should work inside group. Got: {}",
            result
        );
        // Outside: \inner should remain unexpanded
        assert!(
            result.contains(r"\inner"),
            "Inner def should not leak outside group. Got: {}",
            result
        );
    }

    #[test]
    fn test_latex_scope_shadowing_integration() {
        // Shadow and restore
        let input = r"\def\x{OUTER} {\def\x{INNER} \x} \x";
        let result = expand_latex(input);
        assert!(
            result.contains("INNER"),
            "Should have INNER inside. Got: {}",
            result
        );
        assert!(
            result.contains("OUTER"),
            "Should have OUTER outside. Got: {}",
            result
        );
    }

    #[test]
    fn test_latex_global_def_escapes() {
        // \global\def should escape the group
        let input = r"{\global\def\x{GLOBAL}} \x";
        let result = expand_latex(input);
        assert!(
            result.contains("GLOBAL"),
            "Global def should be visible outside. Got: {}",
            result
        );
    }

    // --- Typst Engine: Scope and Closure Tests ---

    #[test]
    fn test_typst_closure_capture_integration() {
        let input = r#"
#let make_adder(n) = (x) => x + n
#let add5 = make_adder(5)
#add5(10)
"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        assert!(
            result.contains("15"),
            "Closure should capture n=5 and compute 5+10=15. Got: {}",
            result
        );
    }

    #[test]
    fn test_typst_nested_scope_shadowing() {
        let input = r#"
#let x = 1
#{
  let x = 2
  [inner=#x]
}
[outer=#x]
"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        // Inner should be 2, outer should be 1
        assert!(
            result.contains("inner=2"),
            "Inner x should be 2. Got: {}",
            result
        );
        assert!(
            result.contains("outer=1"),
            "Outer x should be 1. Got: {}",
            result
        );
    }

    // --- Typst Engine: Control Flow Tests ---

    #[test]
    fn test_typst_break_only_inner_loop() {
        let input = r#"
#let test() = {
  let results = ()
  for i in range(3) {
    for j in range(3) {
      if j == 1 { break }
      results = results + (i * 10 + j,)
    }
  }
  results
}
#test()
"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        // Should have 0, 10, 20 (j=0 for each i=0,1,2)
        // Should NOT have 1, 2, 11, 12, 21, 22
        assert!(result.contains("0"), "Should have 0. Got: {}", result);
        assert!(result.contains("10"), "Should have 10. Got: {}", result);
        assert!(result.contains("20"), "Should have 20. Got: {}", result);
    }

    #[test]
    fn test_typst_return_exits_function() {
        let input = r#"
#let find_first_even() = {
  for i in range(1, 10) {
    if calc.rem(i, 2) == 0 { return i }
  }
  none
}
#find_first_even()
"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        // First even in 1..10 is 2
        assert!(result.contains("2"), "Should return 2. Got: {}", result);
    }

    // --- Graceful Degradation Tests ---

    #[test]
    fn test_typst_unknown_function_compat() {
        // Unknown function should not crash in compat mode
        let input = r#"#totally_undefined_function(1, 2, 3)"#;
        let result = typst_to_latex_with_eval(input, &T2LOptions::default());
        // Should not panic - result may be empty or contain a best-effort result
        // The key is it doesn't crash
        let _ = result;
    }
}

mod warning_system {
    use tylax::core::latex2typst::{latex_to_typst_with_diagnostics, WarningKind};

    #[test]
    fn test_warning_propagation_success() {
        // Simple conversion should produce no warnings
        let result = latex_to_typst_with_diagnostics(
            r"\documentclass{article}\begin{document}Hello\end{document}",
        );
        assert!(
            !result.has_warnings(),
            "Simple document should have no warnings"
        );
        assert!(result.output.contains("Hello"));
    }

    #[test]
    fn test_explsyntax_block_skipped() {
        // ExplSyntaxOn block should be skipped with warning
        let input = r"
\documentclass{article}
\begin{document}
Before
\ExplSyntaxOn
\cs_new:Npn \foo:n #1 { (#1) }
\ExplSyntaxOff
After
\end{document}
";
        let result = latex_to_typst_with_diagnostics(input);

        // Check that a warning was generated
        let has_latex3_warning = result.warnings.iter().any(|w| {
            matches!(w.kind, WarningKind::LaTeX3Skipped)
                || w.message.to_lowercase().contains("latex3")
                || w.message.to_lowercase().contains("expl")
        });
        assert!(
            has_latex3_warning,
            "Should warn about LaTeX3 block. Warnings: {:?}",
            result.warnings
        );

        // Content before and after should be preserved
        assert!(result.output.contains("Before"));
        assert!(result.output.contains("After"));
    }

    #[test]
    fn test_unsupported_primitive_warning() {
        // Unsupported primitives should generate warnings
        let input = r"
\documentclass{article}
\begin{document}
\catcode`\@=11
Some text
\end{document}
";
        let result = latex_to_typst_with_diagnostics(input);

        // Check for primitive warning
        let has_primitive_warning = result.warnings.iter().any(|w| {
            matches!(w.kind, WarningKind::UnsupportedPrimitive)
                || w.message.to_lowercase().contains("catcode")
                || w.message.to_lowercase().contains("primitive")
        });
        assert!(
            has_primitive_warning,
            "Should warn about unsupported primitive. Warnings: {:?}",
            result.warnings
        );

        // Main content should still be present
        assert!(result.output.contains("Some text"));
    }

    #[test]
    fn test_warning_format() {
        // Test that warnings have proper format
        let input = r"\documentclass{article}\begin{document}\catcode`\@=11 Test\end{document}";
        let result = latex_to_typst_with_diagnostics(input);

        for warning in &result.warnings {
            // Warning should have a message
            assert!(
                !warning.message.is_empty(),
                "Warning message should not be empty"
            );

            // Display format should work
            let display = warning.to_string();
            assert!(!display.is_empty(), "Warning display should not be empty");
        }
    }

    #[test]
    fn test_conversion_result_helpers() {
        let result = latex_to_typst_with_diagnostics(
            r"\documentclass{article}\begin{document}Test\end{document}",
        );

        // Test helper methods
        let _ = result.has_warnings();
        let formatted = result.format_warnings();

        // Formatted warnings should be strings
        for s in formatted {
            assert!(s.is_ascii() || !s.is_empty());
        }
    }
}

// ============================================================================
// lr() Delimiter Conversion Tests - Typst to LaTeX
// ============================================================================

mod t2l_lr_delimiters {
    use super::*;

    #[test]
    fn test_lr_angle_brackets() {
        let result = typst_to_latex("$lr(angle.l x angle.r)$");
        assert!(
            result.contains("\\left\\langle") || result.contains("\\left \\langle"),
            "Should have \\left\\langle, got: {}",
            result
        );
        assert!(
            result.contains("\\right\\rangle") || result.contains("\\right \\rangle"),
            "Should have \\right\\rangle, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_angle_brackets_with_comma() {
        let result = typst_to_latex("$lr(angle.l x, y angle.r)$");
        assert!(
            result.contains("\\left\\langle") || result.contains("\\left \\langle"),
            "Should have \\left\\langle, got: {}",
            result
        );
        assert!(
            result.contains("\\right\\rangle") || result.contains("\\right \\rangle"),
            "Should have \\right\\rangle, got: {}",
            result
        );
        assert!(
            result.contains("x, y"),
            "Should preserve comma content, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_angle_brackets_with_multiple_commas() {
        let result = typst_to_latex("$lr(angle.l x, y, z angle.r)$");
        assert!(
            result.contains("\\left\\langle") || result.contains("\\left \\langle"),
            "Should have \\left\\langle, got: {}",
            result
        );
        assert!(
            result.contains("\\right\\rangle") || result.contains("\\right \\rangle"),
            "Should have \\right\\rangle, got: {}",
            result
        );
        assert!(
            result.contains("x, y, z"),
            "Should preserve multiple commas, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_angle_brackets_with_semicolon() {
        let result = typst_to_latex("$lr(angle.l x; y angle.r)$");
        assert!(
            result.contains("\\langle"),
            "Should contain \\langle, got: {}",
            result
        );
        assert!(
            result.contains("\\rangle"),
            "Should contain \\rangle, got: {}",
            result
        );
        assert!(
            result.contains("x; y") || result.contains("x;y"),
            "Should preserve semicolon content, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_brackets_with_comma_args() {
        let result = typst_to_latex("$lr([x], [y])$");
        assert!(
            result.contains("\\left["),
            "Should have \\left[, got: {}",
            result
        );
        assert!(
            result.contains("\\right]"),
            "Should have \\right], got: {}",
            result
        );
        assert!(
            result.contains("], [") || result.contains("] , ["),
            "Should preserve bracketed args with comma, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_chevron_brackets() {
        let result = typst_to_latex("$lr(chevron.l x chevron.r)$");
        assert!(
            result.contains("\\left\\langle") || result.contains("\\left \\langle"),
            "Should have \\left\\langle for chevron.l, got: {}",
            result
        );
        assert!(
            result.contains("\\right\\rangle") || result.contains("\\right \\rangle"),
            "Should have \\right\\rangle for chevron.r, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_double_bars() {
        let result = typst_to_latex("$lr(|| x ||)$");
        assert!(
            result.contains("\\left\\|") || result.contains("\\left \\|"),
            "Should have \\left\\|, got: {}",
            result
        );
        assert!(
            result.contains("\\right\\|") || result.contains("\\right \\|"),
            "Should have \\right\\|, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_single_bars() {
        let result = typst_to_latex("$lr(| x |)$");
        assert!(
            result.contains("\\left|") || result.contains("\\left |"),
            "Should have \\left|, got: {}",
            result
        );
        assert!(
            result.contains("\\right|") || result.contains("\\right |"),
            "Should have \\right|, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_floor() {
        let result = typst_to_latex("$lr(floor.l x floor.r)$");
        assert!(
            result.contains("\\left\\lfloor") || result.contains("\\left \\lfloor"),
            "Should have \\left\\lfloor, got: {}",
            result
        );
        assert!(
            result.contains("\\right\\rfloor") || result.contains("\\right \\rfloor"),
            "Should have \\right\\rfloor, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_ceil() {
        let result = typst_to_latex("$lr(ceil.l x ceil.r)$");
        assert!(
            result.contains("\\left\\lceil") || result.contains("\\left \\lceil"),
            "Should have \\left\\lceil, got: {}",
            result
        );
        assert!(
            result.contains("\\right\\rceil") || result.contains("\\right \\rceil"),
            "Should have \\right\\rceil, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_parentheses() {
        let result = typst_to_latex("$lr((x + y))$");
        assert!(
            result.contains("\\left("),
            "Should have \\left(, got: {}",
            result
        );
        assert!(
            result.contains("\\right)"),
            "Should have \\right), got: {}",
            result
        );
    }

    #[test]
    fn test_lr_brackets() {
        let result = typst_to_latex("$lr([x + y])$");
        assert!(
            result.contains("\\left["),
            "Should have \\left[, got: {}",
            result
        );
        assert!(
            result.contains("\\right]"),
            "Should have \\right], got: {}",
            result
        );
    }

    #[test]
    fn test_lr_no_delimiter_uses_default_parentheses() {
        // When lr() has no recognizable delimiters, should use default ()
        let result = typst_to_latex("$lr(x + y)$");
        assert!(
            result.contains("\\left("),
            "Fallback should use \\left(, got: {}",
            result
        );
        assert!(
            result.contains("\\right)"),
            "Fallback should use \\right), got: {}",
            result
        );
    }

    #[test]
    fn test_lr_size_percent_uses_fixed_delimiters() {
        let result = typst_to_latex("$lr({a_n}, size: #200%)$");
        assert!(
            result.contains("\\bigg\\{") && result.contains("\\bigg\\}"),
            "size: #200% should map to fixed-size braces, got: {}",
            result
        );
        assert!(
            !result.contains("\\left") && !result.contains("\\right"),
            "Explicit size should not use \\left/\\right, got: {}",
            result
        );
        assert!(
            !result.contains("size:") && !result.contains('%'),
            "Named arg fragments should not leak, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_size_small_percent_uses_bigl() {
        let result = typst_to_latex("$lr((x+y), size: #120%)$");
        assert!(
            result.contains("\\big(") && result.contains("\\big)"),
            "size: #120% should map to \\big...\\big, got: {}",
            result
        );
        assert!(
            !result.contains("\\left") && !result.contains("\\right"),
            "Explicit size should not use \\left/\\right, got: {}",
            result
        );
        assert!(
            !result.contains("size:") && !result.contains('%'),
            "Named arg fragments should not leak, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_size_100_percent_uses_plain_delimiters() {
        let result = typst_to_latex("$lr((x+y), size: #100%)$");
        assert!(
            !result.contains("\\left") && !result.contains("\\right"),
            "size: #100% should stay plain, got: {}",
            result
        );
        assert!(
            !result.contains("\\bigl")
                && !result.contains("\\Bigl")
                && !result.contains("\\biggl")
                && !result.contains("\\Biggl"),
            "size: #100% should not use fixed-size commands, got: {}",
            result
        );
        assert!(
            !result.contains("size:") && !result.contains('%'),
            "Named arg fragments should not leak, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_size_with_empty_delimiter_dot() {
        let result = typst_to_latex("$lr(., x, size: #200%)$");
        assert!(
            result.contains("\\bigg."),
            "Dot delimiter should remain valid in fixed-size mode, got: {}",
            result
        );
        assert!(
            !result.contains("size:") && !result.contains('%'),
            "Named arg fragments should not leak, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_size_unsupported_unit_falls_back_without_leak() {
        let result = typst_to_latex("$lr((x+y), size: 2em)$");
        assert!(
            result.contains("\\left(") && result.contains("\\right)"),
            "Unsupported size units should fall back to auto sizing, got: {}",
            result
        );
        assert!(
            !result.contains("size:"),
            "Named arg fragments should not leak, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_unknown_named_arg_falls_back_without_dropping_content() {
        let result = typst_to_latex("$lr((x+y), foo: bar)$");
        assert!(
            result.contains("foo") && result.contains("bar"),
            "Unknown named args should not be silently dropped, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_size_and_unknown_named_arg_drops_only_size() {
        let result = typst_to_latex("$lr((x), size: #200%, foo: bar)$");
        assert!(
            !result.contains("size:") && !result.contains('%'),
            "Recognized size arg should still be stripped when unknown named args exist, got: {}",
            result
        );
        assert!(
            result.contains("foo") && result.contains("bar"),
            "Unknown named args should still be preserved, got: {}",
            result
        );
    }
}

// ============================================================================
// Named Argument Regression Tests - Typst to LaTeX
// ============================================================================

mod t2l_named_args {
    use super::*;

    #[test]
    fn test_rotate_named_angle_preserved() {
        let result =
            typst_to_latex_with_options("#rotate(angle: 90deg)[Hi]", &T2LOptions::default());
        assert!(
            result.contains("\\rotatebox{90}"),
            "rotate angle should be preserved, got: {}",
            result
        );
        assert!(
            result.contains("Hi"),
            "rotate content missing, got: {}",
            result
        );
    }

    #[test]
    fn test_text_named_args_no_leak() {
        let result = typst_to_latex_with_options(
            "#text(weight: \"bold\", style: \"italic\", size: 20pt)[Hello]",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("\\textbf"),
            "missing bold wrapper: {}",
            result
        );
        assert!(
            result.contains("\\textit"),
            "missing italic wrapper: {}",
            result
        );
        assert!(
            result.contains("\\Huge") || result.contains("\\huge"),
            "missing size wrapper: {}",
            result
        );
        assert!(!result.contains("size:"), "named arg leaked: {}", result);
    }

    #[test]
    fn test_raw_named_args_preserved() {
        let result = typst_to_latex_with_options(
            "#raw(lang: \"rust\", block: true)[fn main() {}]",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("\\begin{lstlisting}")
                || result.contains("\\begin{verbatim}")
                || result.contains("\\texttt"),
            "raw content should remain code-like, got: {}",
            result
        );
        assert!(
            !result.contains("lang:"),
            "lang named arg leaked: {}",
            result
        );
        assert!(
            !result.contains("block:"),
            "block named arg leaked: {}",
            result
        );
    }

    #[test]
    fn test_grid_columns_named_arg_preserved() {
        let result =
            typst_to_latex_with_options("#grid(columns: 3)[A][B][C]", &T2LOptions::default());
        assert!(
            result.contains("0.32\\textwidth"),
            "grid columns should drive width, got: {}",
            result
        );
        assert!(
            !result.contains("columns:"),
            "columns named arg leaked: {}",
            result
        );
    }

    #[test]
    fn test_grid_tuple_columns_named_arg_preserved() {
        let result = typst_to_latex_with_options(
            "#grid(columns: (auto, auto, auto))[A][B][C]",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("0.32\\textwidth"),
            "tuple-valued columns should still infer 3 columns, got: {}",
            result
        );
        assert!(
            !result.contains("columns:"),
            "columns named arg leaked: {}",
            result
        );
    }

    #[test]
    fn test_text_rgb_fill_named_arg_preserved() {
        let result = typst_to_latex_with_options(
            "#text(fill: rgb(255, 0, 0))[Hello]",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("\\textcolor[RGB]{255,0,0}"),
            "rgb fill should map to xcolor RGB, got: {}",
            result
        );
        assert!(
            !result.contains("fill:"),
            "fill named arg leaked: {}",
            result
        );
    }

    #[test]
    fn test_text_cmyk_fill_named_arg_preserved() {
        let result = typst_to_latex_with_options(
            "#text(fill: cmyk(0, 1, 1, 0))[Hello]",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("\\textcolor[cmyk]{0,1,1,0}"),
            "cmyk fill should map to xcolor cmyk, got: {}",
            result
        );
        assert!(
            !result.contains("fill:"),
            "fill named arg leaked: {}",
            result
        );
    }

    #[test]
    fn test_text_luma_fill_named_arg_preserved() {
        let result =
            typst_to_latex_with_options("#text(fill: luma(0.5))[Hello]", &T2LOptions::default());
        assert!(
            result.contains("\\textcolor[gray]{0.5}"),
            "luma fill should map to xcolor gray, got: {}",
            result
        );
        assert!(
            !result.contains("fill:"),
            "fill named arg leaked: {}",
            result
        );
    }

    #[test]
    fn test_rect_rgb_fill_named_arg_preserved() {
        let result = typst_to_latex_with_options(
            "#rect(fill: rgb(255, 0, 0))[Hello]",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("\\colorbox[RGB]{255,0,0}"),
            "rect rgb fill should map to xcolor RGB, got: {}",
            result
        );
        assert!(
            !result.contains("fill:"),
            "fill named arg leaked: {}",
            result
        );
    }

    #[test]
    fn test_bibliography_style_named_arg_preserved() {
        let result = typst_to_latex_with_options(
            "#bibliography(\"refs.bib\", style: plain)",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("\\bibliographystyle{plain}"),
            "style should be preserved, got: {}",
            result
        );
        assert!(
            result.contains("\\bibliography{refs}"),
            "bib file should be preserved, got: {}",
            result
        );
        assert!(
            !result.contains("style:"),
            "style named arg leaked: {}",
            result
        );
    }
}

mod t2l_citation_refs {
    use super::*;

    #[test]
    fn test_cite_forms_and_reference_helpers() {
        assert_eq!(
            typst_to_latex_with_options(r#"#cite(<knuth>)"#, &T2LOptions::default()).trim(),
            r#"\cite{knuth}"#
        );
        assert_eq!(
            typst_to_latex_with_options(r#"#cite(<knuth>, form: "prose")"#, &T2LOptions::default())
                .trim(),
            r#"\citet{knuth}"#
        );
        assert_eq!(
            typst_to_latex_with_options(r#"#cite(<knuth>, form: "year")"#, &T2LOptions::default())
                .trim(),
            r#"\citeyear{knuth}"#
        );
        assert_eq!(
            typst_to_latex_with_options(
                r#"#cite(<knuth>, form: "author")"#,
                &T2LOptions::default()
            )
            .trim(),
            r#"\citeauthor{knuth}"#
        );
        assert_eq!(
            typst_to_latex_with_options(r#"#cite(<a>, <b>)"#, &T2LOptions::default()).trim(),
            r#"\cite{a, b}"#
        );
        assert_eq!(
            typst_to_latex_with_options(r#"#ref(<eq-energy>)"#, &T2LOptions::default()).trim(),
            r#"\ref{eq-energy}"#
        );
        assert_eq!(
            typst_to_latex_with_options(r#"#label(<eq-energy>)"#, &T2LOptions::default()).trim(),
            r#"\label{eq-energy}"#
        );
        assert_eq!(typst_to_latex("@knuth").trim(), r#"\ref{knuth}"#);
    }
}

mod l2t_citation_refs {
    use super::*;

    #[test]
    fn test_l2t_citation_variants() {
        assert_eq!(
            latex_to_typst(r#"\cite{knuth}"#).trim(),
            r#"#cite(<knuth>)"#
        );

        let citet = latex_to_typst(r#"\citet{knuth}"#);
        assert!(
            citet.contains(r#"#cite(<knuth>, form: "prose")"#),
            "got: {}",
            citet
        );

        let citeyear = latex_to_typst(r#"\citeyear{knuth}"#);
        assert!(
            citeyear.contains(r#"#cite(<knuth>, form: "year")"#),
            "got: {}",
            citeyear
        );
        assert!(
            !citeyear.contains("<>") && !citeyear.contains("k n u t h"),
            "got: {}",
            citeyear
        );

        let citeauthor = latex_to_typst(r#"\citeauthor{knuth}"#);
        assert!(
            citeauthor.contains(r#"#cite(<knuth>, form: "author")"#),
            "got: {}",
            citeauthor
        );
        assert!(
            !citeauthor.contains("<>") && !citeauthor.contains("k n u t h"),
            "got: {}",
            citeauthor
        );

        let roundtrip = typst_to_latex(latex_to_typst(r#"\cite{knuth}"#).trim());
        assert_eq!(roundtrip.trim(), r#"\cite{knuth}"#);
    }

    #[test]
    fn test_l2t_reference_variants() {
        assert_eq!(latex_to_typst(r#"\eqref{energy}"#).trim(), "@eq-energy");
        assert_eq!(latex_to_typst(r#"\ref{fig:one}"#).trim(), "@fig-one");
        assert_eq!(
            latex_to_typst(r#"\hyperref[intro]{custom text}"#).trim(),
            "#link(<intro>)[custom text]"
        );
        let pageref = latex_to_typst(r#"\pageref{fig:one}"#);
        assert!(
            pageref.contains("#locate") && pageref.contains("@fig-one.page()"),
            "got: {}",
            pageref
        );
    }
}

// ============================================================================
// Escaped punctuation regressions - Typst to LaTeX
// ============================================================================

mod citation_edge_cases {
    use super::*;

    #[test]
    fn test_t2l_citation_edge_cases() {
        assert_eq!(
            typst_to_latex_with_options(
                r#"#cite(<a>, <b>, form: "prose")"#,
                &T2LOptions::default()
            )
            .trim(),
            r#"\citet{a, b}"#
        );
        assert_eq!(
            typst_to_latex_with_options(
                r#"#cite(<a>, supplement: [pp. 3-4])"#,
                &T2LOptions::default()
            )
            .trim(),
            r#"\cite[pp. 3-4]{a}"#
        );
        assert_eq!(
            typst_to_latex_with_options(
                r#"#cite(<a>, form: "author", supplement: [ch. 2])"#,
                &T2LOptions::default()
            )
            .trim(),
            r#"\citeauthor[ch. 2]{a}"#
        );
    }

    #[test]
    fn test_l2t_citation_edge_cases() {
        let citep = latex_document_to_typst(r#"See \citep[see][ch. 2]{a,b}."#);
        assert!(
            citep.contains(r#"See see #cite(<a>, <b>, supplement: [ch. 2])."#),
            "got: {}",
            citep
        );

        let citep_single = latex_document_to_typst(r#"See \citep[see]{a}."#);
        assert!(
            citep_single.contains(r#"See #cite(<a>, supplement: [see])."#),
            "got: {}",
            citep_single
        );

        let citeauthor_star = latex_to_typst(r#"\citeauthor*{a}"#);
        assert!(
            citeauthor_star.contains(r#"#cite(<a>, form: "author")"#)
                && !citeauthor_star.starts_with('*'),
            "got: {}",
            citeauthor_star
        );

        let citeyearpar = latex_to_typst(r#"\citeyearpar{a}"#);
        assert!(
            citeyearpar.contains(r#"#cite(<a>, form: "year")"#),
            "got: {}",
            citeyearpar
        );

        let nameref = latex_document_to_typst(r#"See \nameref{sec:intro}."#);
        assert!(nameref.contains("See @sec-intro."), "got: {}", nameref);
    }
}

mod t2l_minieval_semantic_refs {
    use super::*;
    use tylax::{
        core::typst2latex::expand_macros, typst_to_latex_with_diagnostics, typst_to_latex_with_eval,
    };

    #[test]
    fn test_expand_macros_keeps_citation_typst_via_shared_serializer() {
        assert_eq!(
            expand_macros(r#"#cite(<knuth>)"#).unwrap().trim(),
            r#"#cite(<knuth>)"#
        );
        assert_eq!(
            expand_macros(r#"#cite(<knuth>, form: "author", supplement: [ch. 2])"#)
                .unwrap()
                .trim(),
            r#"#cite(<knuth>, form: "author", supplement: [ch. 2])"#
        );
    }

    #[test]
    fn test_minieval_preserves_citation_ref_label_and_bibliography() {
        let opts = T2LOptions::full_document();

        let cite = typst_to_latex_with_eval(r#"#cite(<knuth>)"#, &opts);
        assert!(cite.contains(r#"\cite{knuth}"#), "got: {}", cite);

        let cite_prose = typst_to_latex_with_eval(r#"#cite(<knuth>, form: "prose")"#, &opts);
        assert!(
            cite_prose.contains(r#"\citet{knuth}"#),
            "got: {}",
            cite_prose
        );

        let rf = typst_to_latex_with_eval(r#"#ref(<eq-energy>)"#, &opts);
        assert!(rf.contains(r#"\ref{eq-energy}"#), "got: {}", rf);

        let label = typst_to_latex_with_eval(r#"#label(<eq-energy>)"#, &opts);
        assert!(label.contains(r#"\label{eq-energy}"#), "got: {}", label);

        let bib =
            typst_to_latex_with_diagnostics(r#"#bibliography("refs.bib", style: plain)"#, &opts);
        assert!(
            bib.output.contains(r#"\bibliographystyle{plain}"#),
            "got: {}",
            bib.output
        );
        assert!(
            bib.output.contains(r#"\bibliography{refs}"#),
            "got: {}",
            bib.output
        );
        assert!(
            bib.warnings.is_empty(),
            "unexpected warnings: {:?}",
            bib.format_warnings()
        );
    }

    #[test]
    fn test_minieval_preserves_dynamic_citation_and_reference_values() {
        let opts = T2LOptions::full_document();

        let cite = typst_to_latex_with_eval("#let k = <knuth>\n#cite(k)", &opts);
        assert!(cite.contains(r#"\cite{knuth}"#), "got: {}", cite);

        let rf = typst_to_latex_with_eval("#let lab = <eq-energy>\n#ref(lab)", &opts);
        assert!(rf.contains(r#"\ref{eq-energy}"#), "got: {}", rf);

        let looped = typst_to_latex_with_eval("#for k in (<a>, <b>) [#cite(k)]", &opts);
        assert!(looped.contains(r#"\cite{a}\cite{b}"#), "got: {}", looped);
        assert!(
            !looped.contains("[<a>]") && !looped.contains("[<b>]"),
            "got: {}",
            looped
        );

        let spaced_refs = typst_to_latex_with_eval("@a @b", &opts);
        assert!(
            spaced_refs.contains(r#"\ref{a} \ref{b}"#),
            "got: {}",
            spaced_refs
        );

        let spaced_cites = typst_to_latex_with_eval("#cite(<a>) #cite(<b>)", &opts);
        assert!(
            spaced_cites.contains(r#"\cite{a} \cite{b}"#),
            "got: {}",
            spaced_cites
        );

        let sentence_refs = typst_to_latex_with_eval("X @a @b Y", &opts);
        assert!(
            sentence_refs.contains(r#"X \ref{a} \ref{b} Y"#),
            "got: {}",
            sentence_refs
        );
    }

    #[test]
    fn test_diagnostics_preserves_bare_reference_and_spacing() {
        let opts = T2LOptions::default();

        let bare = typst_to_latex_with_diagnostics("@knuth", &opts);
        assert_eq!(bare.output.trim(), r#"\ref{knuth}"#);

        let sentence = typst_to_latex_with_diagnostics("See @knuth.", &opts);
        assert_eq!(sentence.output.trim(), r#"See \ref{knuth}."#);

        let refs = typst_to_latex_with_diagnostics("@a @b", &opts);
        assert_eq!(refs.output.trim(), r#"\ref{a} \ref{b}"#);

        let cites = typst_to_latex_with_diagnostics(r#"#cite(<a>) #cite(<b>)"#, &opts);
        assert_eq!(cites.output.trim(), r#"\cite{a} \cite{b}"#);
    }
}

mod t2l_escaped_punctuation {
    use super::*;

    #[test]
    fn test_cases_escaped_comma_is_literal_comma() {
        let typst = "$delta[n]=cases(1\\, space n=0, 0\\, space n eq.not 0)$";
        let result = typst_to_latex_with_options(typst, &T2LOptions::default());

        assert!(
            result.contains("1 , \\ n = 0") || result.contains("1, \\ n = 0"),
            "escaped comma should become a literal comma, got: {}",
            result
        );
        assert!(
            !result.contains("1 \\, \\ n = 0") && !result.contains("0 \\, \\ n \\neq 0"),
            "escaped comma should not become LaTeX thin space, got: {}",
            result
        );
    }

    #[test]
    fn test_plain_math_escaped_punctuation_is_literal() {
        let result = typst_to_latex_with_options("$x\\, y\\: z\\; w$", &T2LOptions::default());

        assert!(
            result.contains("x, y: z; w"),
            "escaped punctuation should stay literal, got: {}",
            result
        );
        assert!(
            !result.contains("\\,") && !result.contains("\\:") && !result.contains("\\;"),
            "escaped punctuation should not become spacing commands, got: {}",
            result
        );
    }

    #[test]
    fn test_lr_escaped_comma_is_literal() {
        let result =
            typst_to_latex_with_options("$lr(angle.l x\\, y angle.r)$", &T2LOptions::default());

        assert!(
            result.contains("x, y"),
            "escaped comma should remain literal inside lr(), got: {}",
            result
        );
        assert!(
            !result.contains("\\,"),
            "escaped comma inside lr() should not become thin space, got: {}",
            result
        );
    }

    #[test]
    fn test_matrix_escaped_punctuation_is_literal() {
        let result = typst_to_latex_with_options("$mat(1\\, 2; 3\\; 4)$", &T2LOptions::default());

        assert!(
            result.contains("1, 2"),
            "escaped comma should remain literal inside matrix cells, got: {}",
            result
        );
        assert!(
            result.contains("3; 4"),
            "escaped semicolon should remain literal inside matrix cells, got: {}",
            result
        );
        assert!(
            !result.contains("\\,") && !result.contains("\\;"),
            "escaped punctuation inside matrix should not become spacing commands, got: {}",
            result
        );
    }

    #[test]
    fn test_spacing_keywords_still_emit_spacing_commands() {
        let result =
            typst_to_latex_with_options("$x thin y med z thick w space q$", &T2LOptions::default());

        assert!(
            result.contains("\\,"),
            "thin should still emit LaTeX thin space, got: {}",
            result
        );
        assert!(
            result.contains("\\:"),
            "med should still emit LaTeX medium space, got: {}",
            result
        );
        assert!(
            result.contains("\\;"),
            "thick should still emit LaTeX thick space, got: {}",
            result
        );
        assert!(
            result.contains("\\ q") || result.contains("\\  q") || result.contains("\\ q"),
            "space should still emit LaTeX space command, got: {}",
            result
        );
    }

    #[test]
    fn test_cases_condition_rows_stay_paired() {
        let result = typst_to_latex_with_options("$cases(x, & y, z, & w)$", &T2LOptions::default());

        assert!(
            result.contains("x & y"),
            "cases row should keep value/condition pairing, got: {}",
            result
        );
        assert!(
            result.contains("z & w"),
            "cases second row should keep value/condition pairing, got: {}",
            result
        );
        assert!(
            !result.contains(
                "x \\
 & y"
            ) && !result.contains(
                "z \\
 & w"
            ),
            "cases condition should not be emitted as a separate row, got: {}",
            result
        );
    }

    #[test]
    fn test_big_operator_func_call_does_not_drop_arguments() {
        let result = typst_to_latex_with_options("$sum(a, b)$", &T2LOptions::default());

        assert!(
            result.contains("\\sum"),
            "big operator should still emit operator command, got: {}",
            result
        );
        assert!(
            result.contains("a") && result.contains("b"),
            "big operator func call should not drop argument content, got: {}",
            result
        );
    }
    #[test]
    fn test_script_grouping_parentheses_are_not_emitted_in_subscript() {
        let result = typst_to_latex_with_options("$sum_(i=1)^n x_i$", &T2LOptions::default());

        assert!(
            result.contains(r#"\sum_{i = 1}^n"#),
            "grouping parentheses in subscript should not be emitted, got: {}",
            result
        );
        assert!(
            !result.contains(r#"\sum_{(i = 1)}^n"#),
            "subscript should not keep grouping parentheses, got: {}",
            result
        );
    }

    #[test]
    fn test_multiline_big_operator_subscript_uses_substack() {
        let result =
            typst_to_latex_with_options("$sum_(i = 1 \\ j = 1)^n A_(i j)$", &T2LOptions::default());

        assert!(
            result.contains(r#"\sum_{\substack{i = 1 \\ j = 1}}^n"#),
            "multiline big-operator subscript should use substack, got: {}",
            result
        );
        assert!(
            !result.contains(r#"\sum_{i = 1 \\"#),
            "multiline big-operator subscript should not stay as a raw multiline brace group, got: {}",
            result
        );
    }

    #[test]
    fn test_multiline_big_operator_subscript_is_consistent_across_paths() {
        let result = assert_t2l_paths_match("$sum_(i = 1 \\ j = 1)^n A_(i j)$");
        assert!(
            result.contains(r#"\sum_{\substack{i = 1 \\ j = 1}}^n"#),
            "multiline big-operator subscript should stay on the substack path, got: {}",
            result
        );
    }

    #[test]
    fn test_multiline_limits_operator_subscript_uses_substack() {
        let result = typst_to_latex_with_options(
            r#"$limits(op("argmax"))_(x \ y)$"#,
            &T2LOptions::default(),
        );

        assert!(
            result.contains(r#"\operatorname{argmax}_{\substack{x \\ y}}"#),
            "multiline limits() subscript should use substack, got: {}",
            result
        );
    }

    #[test]
    fn test_multiline_non_limits_subscript_keeps_plain_brace_group() {
        let result = typst_to_latex_with_options("$A_(i \\ j)$", &T2LOptions::default());

        assert!(
            !result.contains(r#"\substack"#),
            "non-limits multiline subscript should not use substack, got: {}",
            result
        );
        assert!(
            result.contains("A_{i \\\n") && result.contains("j}"),
            "non-limits multiline subscript should still emit a plain multiline brace group, got: {}",
            result
        );
    }

    #[test]
    fn test_matrix_delim_named_arg_maps_to_pmatrix() {
        let result =
            typst_to_latex_with_options(r#"$mat(delim: "(", 1, 2; 3, 4)$"#, &T2LOptions::default());
        assert!(
            result.contains(r#"\begin{pmatrix}"#),
            r#"mat(delim: "(") should emit pmatrix, got: {}"#,
            result
        );
    }

    #[test]
    fn test_matrix_delim_named_arg_maps_to_bmatrix() {
        let result =
            typst_to_latex_with_options(r#"$mat(delim: "[", 1, 2; 3, 4)$"#, &T2LOptions::default());
        assert!(
            result.contains(r#"\begin{bmatrix}"#),
            r#"mat(delim: "[") should emit bmatrix, got: {}"#,
            result
        );
    }

    #[test]
    fn test_ir_preserves_dotted_symbols_and_big_operators() {
        let result = typst_to_latex_with_options(
            "$sum_(i=1)^n x_i eq.not y_i and bar.v.double$",
            &T2LOptions::default(),
        );

        assert!(
            result.contains("\\sum"),
            "big operator should still emit LaTeX command, got: {}",
            result
        );
        assert!(
            result.contains("\\neq"),
            "dotted symbol eq.not should still emit \\neq, got: {}",
            result
        );
        assert!(
            result.contains("\\|"),
            "bar.v.double should still emit double vertical bar, got: {}",
            result
        );
    }
}

// ============================================================================
// T2L Math IR Tranche-2 Tests
// ============================================================================

mod t2l_math_ir_tranche2 {
    use super::*;

    #[test]
    fn test_limits_ir_preserves_operator_and_scripts() {
        let result =
            typst_to_latex_with_options(r#"$limits(op("argmax"))_(x)$"#, &T2LOptions::default());

        assert!(
            result.contains(r#"\operatorname{argmax}"#),
            "limits() should preserve operator content, got: {}",
            result
        );
        assert!(
            result.contains(r#"_x"#) || result.contains(r#"_{x}"#),
            "limits() should still cooperate with script emission, got: {}",
            result
        );
    }

    #[test]
    fn test_display_ir_respects_block_and_inline_modes() {
        let block = typst_to_latex_with_options("$display(x+y)$", &T2LOptions::block_math());
        let inline = typst_to_latex_with_options("$display(x+y)$", &T2LOptions::inline_math());

        assert!(
            block.contains(r#"\displaystyle x + y"#),
            "display() should emit displaystyle in block mode, got: {}",
            block
        );
        assert!(
            !block.contains(r#"\textstyle"#),
            "block display() should not restore textstyle, got: {}",
            block
        );
        assert!(
            inline.contains(r#"\displaystyle x + y \textstyle"#),
            "inline display() should restore textstyle, got: {}",
            inline
        );
    }

    #[test]
    fn test_inline_ir_respects_block_and_inline_modes() {
        let block = typst_to_latex_with_options("$inline(x+y)$", &T2LOptions::block_math());
        let inline = typst_to_latex_with_options("$inline(x+y)$", &T2LOptions::inline_math());

        assert!(
            block.contains(r#"\textstyle x + y \displaystyle"#),
            "block inline() should restore displaystyle, got: {}",
            block
        );
        assert!(
            inline.contains(r#"\textstyle x + y"#),
            "inline inline() should emit textstyle content, got: {}",
            inline
        );
        assert!(
            !inline.contains(r#"\displaystyle"#),
            "inline inline() should not restore displaystyle, got: {}",
            inline
        );
    }

    #[test]
    fn test_op_ir_emits_operatorname() {
        let result = typst_to_latex_with_options(r#"$op("foo")$"#, &T2LOptions::default());
        assert!(
            result.contains(r#"\operatorname{foo}"#),
            "op() should emit operatorname, got: {}",
            result
        );
    }

    #[test]
    fn test_class_ir_emits_math_class_commands() {
        let punct =
            typst_to_latex_with_options(r#"$class("punctuation", x)$"#, &T2LOptions::default());
        let relation =
            typst_to_latex_with_options(r#"$class("relation", x)$"#, &T2LOptions::default());

        assert!(
            punct.contains(r#"\mathpunct{x}"#),
            r"class(punctuation, x) should emit \mathpunct, got: {}",
            punct
        );
        assert!(
            relation.contains(r#"\mathrel{x}"#),
            r"class(relation, x) should emit \mathrel, got: {}",
            relation
        );
    }

    #[test]
    fn test_assignment_like_relations_use_package_free_output() {
        let assign = typst_to_latex_with_options("$a := b$", &T2LOptions::default());
        let rev_assign = typst_to_latex_with_options("$a =: b$", &T2LOptions::default());
        let double_assign = typst_to_latex_with_options("$a ::= b$", &T2LOptions::default());

        assert!(
            assign.contains(r#"\mathrel{:=}"#) && !assign.contains(r#"\coloneqq"#),
            ":= should emit package-free relation output, got: {}",
            assign
        );
        assert!(
            rev_assign.contains(r#"\mathrel{=:}"#) && !rev_assign.contains(r#"\eqqcolon"#),
            "=: should emit package-free relation output, got: {}",
            rev_assign
        );
        assert!(
            double_assign.contains(r#"\mathrel{::=}"#) && !double_assign.contains(r#"\Coloneqq"#),
            "::= should emit package-free relation output, got: {}",
            double_assign
        );
    }

    #[test]
    fn test_assignment_like_relations_are_consistent_across_paths() {
        let result = assert_t2l_paths_match("$a := b$");
        assert!(
            result.contains(r#"\mathrel{:=}"#) && !result.contains(r#"\coloneqq"#),
            "assignment-like relations should stay package-free across all paths, got: {}",
            result
        );
    }

    #[test]
    fn test_full_document_assignment_like_relations_do_not_require_mathtools() {
        let result = typst_to_latex_with_options("$a := b$", &T2LOptions::full_document());
        assert!(
            result.contains(r#"\mathrel{:=}"#),
            "full document := should still use package-free output, got: {}",
            result
        );
        assert!(
            !result.contains(r#"\usepackage{mathtools}"#),
            "full document default preamble should not add mathtools, got: {}",
            result
        );
    }

    #[test]
    fn test_math_h_fixed_lengths_emit_hspace() {
        let cm = typst_to_latex_with_options("$a #h(1cm) b$", &T2LOptions::default());
        let em = typst_to_latex_with_options("$a #h(1em) b$", &T2LOptions::default());
        let issue = typst_to_latex_with_options(
            r#"The competitive ratio is defined as:

$
  "CR"((x_i)_(i in ZZ), t) :=  (sum_(j <= k) y_j)  <= r_k #h(1cm)
  v_k sum_(j >= i) z_j / v_j >= v_k z_p/v_p = r_k
$"#,
            &T2LOptions::default(),
        );

        assert!(
            cm.contains(r#"\hspace{1cm}"#),
            "math h(1cm) should emit hspace, got: {}",
            cm
        );
        assert!(
            em.contains(r#"\hspace{1em}"#),
            "math h(1em) should emit hspace, got: {}",
            em
        );
        assert!(
            issue.contains(r#"\hspace{1cm}"#),
            "display math example should emit hspace, got: {}",
            issue
        );
    }

    #[test]
    fn test_math_h_fixed_lengths_are_consistent_across_paths() {
        let inline = assert_t2l_paths_match("$a #h(1cm) b$");
        assert!(
            inline.contains(r#"\hspace{1cm}"#),
            "inline h(1cm) should become hspace across all paths, got: {}",
            inline
        );

        let display = assert_t2l_paths_match(
            r#"The competitive ratio is defined as:

$
  "CR"((x_i)_(i in ZZ), t) :=  (sum_(j <= k) y_j)  <= r_k #h(1cm)
  v_k sum_(j >= i) z_j / v_j >= v_k z_p/v_p = r_k
$"#,
        );
        assert!(
            display.contains(r#"\hspace{1cm}"#),
            "display-math h(1cm) should become hspace across all paths, got: {}",
            display
        );

        let list_item = assert_t2l_paths_match(
            r#"- We look first at constraint $x_(i,k)$ when $k < p$. $
    sum_(j:i <= j <= k) y_j = r_k - r_(i-1) <= r_k #h(1cm)
    v_k sum_(j >= i) z_j / v_j >= v_k z_p/v_p = r_k
  $"#,
        );
        assert!(
            list_item.contains(r#"\hspace{1cm}"#),
            "list-item math h(1cm) should become hspace across all paths, got: {}",
            list_item
        );
    }

    #[test]
    fn test_math_h_fr_keeps_fallback_behavior() {
        let result = typst_to_latex_with_options("$a #h(1fr) b$", &T2LOptions::default());
        assert!(
            result.contains(r#"\operatorname{h}"#),
            "unsupported math h(1fr) should keep callable fallback, got: {}",
            result
        );
        assert!(
            !result.contains(r#"\hspace{1fr}"#) && !result.contains(r#"\hfill"#),
            "unsupported math h(1fr) should not invent fixed or flex spacing, got: {}",
            result
        );
    }

    #[test]
    fn test_set_arrow_and_accent_ir_emit_wrappers() {
        let set_result = typst_to_latex_with_options("$set(x)$", &T2LOptions::default());
        let arrow_result = typst_to_latex_with_options("$arrow(x)$", &T2LOptions::default());
        let accent_arrow =
            typst_to_latex_with_options("$accent(x, arrow.r)$", &T2LOptions::default());
        let accent_hat = typst_to_latex_with_options("$accent(x, hat)$", &T2LOptions::default());

        assert!(
            set_result.contains(r#"\left\{x\right\}"#),
            "set() should emit brace delimiters, got: {}",
            set_result
        );
        assert!(
            arrow_result.contains(r#"\overrightarrow{x}"#),
            "arrow() should emit overrightarrow, got: {}",
            arrow_result
        );
        assert!(
            accent_arrow.contains(r#"\overrightarrow{x}"#),
            "accent(..., arrow.r) should emit overrightarrow, got: {}",
            accent_arrow
        );
        assert!(
            accent_hat.contains(r#"\hat{x}"#),
            "accent(..., hat) should emit hat, got: {}",
            accent_hat
        );
    }

    #[test]
    fn test_accent_ir_supports_common_accent_variants() {
        let tilde = typst_to_latex_with_options("$accent(x, tilde)$", &T2LOptions::default());
        let dot = typst_to_latex_with_options("$accent(x, dot)$", &T2LOptions::default());
        let ddot = typst_to_latex_with_options("$accent(x, ddot)$", &T2LOptions::default());
        let bar = typst_to_latex_with_options("$accent(x, bar)$", &T2LOptions::default());
        let grave = typst_to_latex_with_options("$accent(x, grave)$", &T2LOptions::default());
        let acute = typst_to_latex_with_options("$accent(x, acute)$", &T2LOptions::default());
        let breve = typst_to_latex_with_options("$accent(x, breve)$", &T2LOptions::default());
        let check = typst_to_latex_with_options("$accent(x, check)$", &T2LOptions::default());

        assert!(
            tilde.contains(r#"\tilde{x}"#),
            "accent(..., tilde) should emit tilde, got: {}",
            tilde
        );
        assert!(
            dot.contains(r#"\dot{x}"#),
            "accent(..., dot) should emit dot, got: {}",
            dot
        );
        assert!(
            ddot.contains(r#"\ddot{x}"#),
            "accent(..., ddot) should emit ddot, got: {}",
            ddot
        );
        assert!(
            bar.contains(r#"\bar{x}"#),
            "accent(..., bar) should emit bar, got: {}",
            bar
        );
        assert!(
            grave.contains(r#"\grave{x}"#),
            "accent(..., grave) should emit grave, got: {}",
            grave
        );
        assert!(
            acute.contains(r#"\acute{x}"#),
            "accent(..., acute) should emit acute, got: {}",
            acute
        );
        assert!(
            breve.contains(r#"\breve{x}"#),
            "accent(..., breve) should emit breve, got: {}",
            breve
        );
        assert!(
            check.contains(r#"\check{x}"#),
            "accent(..., check) should emit check, got: {}",
            check
        );
    }

    #[test]
    fn test_color_ir_emits_color_wrapper() {
        let result = typst_to_latex_with_options("$color(red, x)$", &T2LOptions::default());
        assert!(
            result.contains(r#"{\color{red}x}"#),
            "color() should emit color wrapper, got: {}",
            result
        );
    }

    #[test]
    fn test_escape_punctuation_preserves_literal_spacing_in_function_calls() {
        let result = typst_to_latex_with_options(r"$sum(a\, b\: c\; d)$", &T2LOptions::default());

        assert!(
            result.contains(r#"\sum(a, b: c; d)"#),
            "escaped punctuation should remain literal in function calls, got: {}",
            result
        );
        assert!(
            !result.contains(r#"\, "#) && !result.contains(r#"\:"#) && !result.contains(r#"\;"#),
            "escaped punctuation should not be reinterpreted as spacing commands, got: {}",
            result
        );
    }
}

// ============================================================================
// T2L Math Structured IR Tests
// ============================================================================

mod t2l_math_structured_ir {
    use super::*;

    #[test]
    fn test_math_vec_emits_pmatrix_rows() {
        let result = typst_to_latex_with_options("$math.vec(a, b, c)$", &T2LOptions::default());
        assert!(
            result.contains(r#"\begin{pmatrix}"#),
            "math.vec should emit pmatrix, got: {}",
            result
        );
        assert!(
            result.contains("a") && result.contains("b") && result.contains("c"),
            "math.vec should preserve row content, got: {}",
            result
        );
    }

    #[test]
    fn test_attach_emits_pre_and_post_scripts() {
        let result = typst_to_latex_with_options(
            "$attach(x, t: n, b: i, tl: a, bl: b)$",
            &T2LOptions::default(),
        );
        assert!(
            result.contains("{}_{b}^{a}x_{i}^{n}")
                || result.contains("{}_{b}^{a}x_i^n")
                || result.contains("{}_b^ax_i^n"),
            "attach should emit pre/post scripts, got: {}",
            result
        );
    }

    #[test]
    fn test_scripts_and_primes_specials() {
        let scripts = typst_to_latex_with_options("$scripts(x+y)$", &T2LOptions::default());
        let primes = typst_to_latex_with_options("$primes(3)$", &T2LOptions::default());
        assert!(
            scripts.contains(r#"\displaystyle x + y"#),
            "scripts should emit displaystyle content, got: {}",
            scripts
        );
        assert!(
            primes.contains("'''"),
            "primes(3) should emit three primes, got: {}",
            primes
        );
    }

    #[test]
    fn test_stretch_and_mid_specials() {
        let stretch = typst_to_latex_with_options("$stretch(->)$", &T2LOptions::default());
        let brace_top = typst_to_latex_with_options("$stretch(brace.t)$", &T2LOptions::default());
        let brace_bottom =
            typst_to_latex_with_options("$stretch(brace.b)$", &T2LOptions::default());
        let mid = typst_to_latex_with_options("$mid(|)$", &T2LOptions::default());
        assert!(
            stretch.contains(r#"\xrightarrow{}"#),
            "stretch(->) should emit xrightarrow, got: {}",
            stretch
        );
        assert!(
            brace_top.contains(r#"\overbrace{}"#),
            "stretch(brace.t) should emit overbrace, got: {}",
            brace_top
        );
        assert!(
            brace_bottom.contains(r#"\underbrace{}"#),
            "stretch(brace.b) should emit underbrace, got: {}",
            brace_bottom
        );
        assert!(
            mid.contains(r#"\mid"#),
            r"mid should emit \mid, got: {}",
            mid
        );
    }

    #[test]
    fn test_circle_divergence_and_curl_specials() {
        let circle = typst_to_latex_with_options("$circle(x)$", &T2LOptions::default());
        let divergence = typst_to_latex_with_options("$divergence(A)$", &T2LOptions::default());
        let curl = typst_to_latex_with_options("$curl(A)$", &T2LOptions::default());
        assert!(
            circle.contains(r#"\mathring{x}"#),
            "circle(x) should emit mathring, got: {}",
            circle
        );
        assert!(
            divergence.contains(r#"\nabla \cdot A"#),
            "divergence(A) should emit nabla dot product, got: {}",
            divergence
        );
        assert!(
            curl.contains(r#"\nabla \times A"#),
            "curl(A) should emit nabla cross product, got: {}",
            curl
        );
    }

    #[test]
    fn test_big_operator_and_unknown_func_calls_preserve_content() {
        let sum = typst_to_latex_with_options("$sum(a, b)$", &T2LOptions::default());
        let unknown = typst_to_latex_with_options("$foo(x, y)$", &T2LOptions::default());
        assert!(
            sum.contains(r#"\sum(a, b)"#),
            "big-operator function call should emit call syntax, got: {}",
            sum
        );
        assert!(
            unknown.contains(r#"\operatorname{foo}(x, y)"#),
            "unknown function call should emit operatorname call, got: {}",
            unknown
        );
    }
}

// ============================================================================
// Physics Package Tests - LaTeX to Typst
// ============================================================================

mod physics_package {
    use super::*;

    // --- Automatic bracing ---

    #[test]
    fn test_abs() {
        let result = latex_to_typst(r"\abs{x}");
        assert!(
            result.contains("abs("),
            "\\abs{{x}} should produce abs(...), got: {}",
            result
        );
    }

    #[test]
    fn test_norm() {
        let result = latex_to_typst(r"\norm{x}");
        assert!(
            result.contains("norm("),
            "\\norm{{x}} should produce norm(...), got: {}",
            result
        );
    }

    #[test]
    fn test_pqty() {
        let result = latex_to_typst(r"\pqty{x+y}");
        assert!(
            result.contains("lr(("),
            "\\pqty should produce lr((...)), got: {}",
            result
        );
    }

    #[test]
    fn test_bqty() {
        let result = latex_to_typst(r"\bqty{x+y}");
        assert!(
            result.contains("lr(["),
            "\\bqty should produce lr([...]), got: {}",
            result
        );
    }

    #[test]
    fn test_comm() {
        let result = latex_to_typst(r"\comm{A}{B}");
        assert!(
            result.contains("lr([") && result.contains(","),
            "\\comm{{A}}{{B}} should produce lr([A, B]), got: {}",
            result
        );
    }

    #[test]
    fn test_acomm() {
        let result = latex_to_typst(r"\acomm{A}{B}");
        let has_braces = result.contains('{') && result.contains(',');
        assert!(
            has_braces,
            "\\acomm{{A}}{{B}} should produce lr({{ A, B }}), got: {}",
            result
        );
    }

    #[test]
    fn test_order() {
        let result = latex_to_typst(r"\order{x^2}");
        assert!(
            result.contains("cal(O)"),
            "\\order should produce cal(O)(...), got: {}",
            result
        );
    }

    // --- Vector notation ---

    #[test]
    fn test_vb() {
        let result = latex_to_typst(r"\vb{a}");
        assert!(
            result.contains("bold("),
            "\\vb{{a}} should produce bold(a), got: {}",
            result
        );
    }

    #[test]
    fn test_va() {
        let result = latex_to_typst(r"\va{a}");
        assert!(
            result.contains("bold(") && result.contains("arrow"),
            "\\va{{a}} should produce accent(bold(a), arrow), got: {}",
            result
        );
    }

    #[test]
    fn test_vu() {
        let result = latex_to_typst(r"\vu{e}");
        assert!(
            result.contains("bold(") && result.contains("hat"),
            "\\vu{{e}} should produce accent(bold(e), hat), got: {}",
            result
        );
    }

    #[test]
    fn test_vdot_symbol() {
        let result = latex_to_typst(r"\vdot");
        assert!(
            result.contains("dot") || result.contains("dot.op"),
            "\\vdot should produce dot.op, got: {}",
            result
        );
    }

    #[test]
    fn test_cross_symbol() {
        let result = latex_to_typst(r"\cross");
        assert!(
            result.contains("times"),
            "\\cross should produce times, got: {}",
            result
        );
    }

    // --- Derivatives ---

    #[test]
    fn test_dd_bare() {
        let result = latex_to_typst(r"\dd");
        assert!(
            result.contains("dif"),
            "\\dd should produce dif, got: {}",
            result
        );
    }

    #[test]
    fn test_dd_with_arg() {
        let result = latex_to_typst(r"\dd{x}");
        assert!(
            result.contains("dif") && result.contains("x"),
            "\\dd{{x}} should produce dif x, got: {}",
            result
        );
    }

    #[test]
    fn test_dd_optional_order() {
        let result = latex_to_typst(r"\dd[3]{x}");
        assert!(
            result.contains("dif^3") && result.contains("x"),
            "\\dd[3]{{x}} should produce dif^3 x, got: {}",
            result
        );
    }

    #[test]
    fn test_dv_two_args() {
        let result = latex_to_typst(r"\dv{f}{x}");
        assert!(
            result.contains("frac") && result.contains("dif"),
            "\\dv{{f}}{{x}} should produce frac(dif f, dif x), got: {}",
            result
        );
    }

    #[test]
    fn test_dv_optional_order() {
        let result = latex_to_typst(r"\dv[2]{f}{x}");
        assert!(
            result.contains("dif^2") && result.contains("x^2"),
            "\\dv[2]{{f}}{{x}} should produce dif^2 and x^2, got: {}",
            result
        );
    }

    #[test]
    fn test_dv_star_optional_order() {
        let result = latex_to_typst(r"\dv*[2]{f}{x}");
        assert!(
            result.contains("dif^2") && result.contains("x^2") && result.contains("/"),
            "\\dv*[2]{{f}}{{x}} should produce inline dif^2 and x^2, got: {}",
            result
        );
    }

    #[test]
    fn test_dv_single_arg() {
        let result = latex_to_typst(r"\dv{x}");
        assert!(
            result.contains("frac") && result.contains("dif"),
            "\\dv{{x}} should produce frac(dif, dif x), got: {}",
            result
        );
    }

    #[test]
    fn test_pdv_two_args() {
        let result = latex_to_typst(r"\pdv{f}{x}");
        assert!(
            result.contains("frac") && result.contains("diff"),
            "\\pdv{{f}}{{x}} should produce frac(diff f, diff x), got: {}",
            result
        );
    }

    #[test]
    fn test_pdv_optional_order() {
        let result = latex_to_typst(r"\pdv[2]{f}{x}");
        assert!(
            result.contains("diff^2") && result.contains("x^2"),
            "\\pdv[2]{{f}}{{x}} should produce diff^2 and x^2, got: {}",
            result
        );
    }

    #[test]
    fn test_pdv_star_optional_order() {
        let result = latex_to_typst(r"\pdv*[3]{f}{x}");
        assert!(
            result.contains("diff^3") && result.contains("x^3") && result.contains("/"),
            "\\pdv*[3]{{f}}{{x}} should produce inline diff^3 and x^3, got: {}",
            result
        );
    }

    #[test]
    fn test_pdv_mixed_partial() {
        let result = latex_to_typst(r"\pdv{f}{x}{y}");
        assert!(
            result.contains("diff^2") && result.contains("diff x") && result.contains("diff y"),
            "\\pdv{{f}}{{x}}{{y}} should produce frac(diff^2 f, diff x diff y), got: {}",
            result
        );
    }

    #[test]
    fn test_pdv_star_mixed_partial() {
        let result = latex_to_typst(r"\pdv*{f}{x}{y}");
        assert!(
            result.contains("diff^2") && result.contains("diff x") && result.contains("diff y"),
            "\\pdv*{{f}}{{x}}{{y}} should produce inline diff^2 f / diff x diff y, got: {}",
            result
        );
    }

    #[test]
    fn test_pdv_mixed_partial_with_optional_order() {
        let result = latex_to_typst(r"\pdv[3]{f}{x}{y}");
        assert!(
            result.contains("diff^3") && result.contains("diff x") && result.contains("diff y"),
            "\\pdv[3]{{f}}{{x}}{{y}} should preserve the requested order, got: {}",
            result
        );
    }

    #[test]
    fn test_fdv() {
        let result = latex_to_typst(r"\fdv{F}{g}");
        assert!(
            result.contains("frac") && result.contains("delta"),
            "\\fdv{{F}}{{g}} should produce frac(delta F, delta g), got: {}",
            result
        );
    }

    #[test]
    fn test_fdv_optional_order() {
        let result = latex_to_typst(r"\fdv[2]{F}{g}");
        assert!(
            result.contains("delta^2") && result.contains("g^2"),
            "\\fdv[2]{{F}}{{g}} should produce delta^2 and g^2, got: {}",
            result
        );
    }

    #[test]
    fn test_fdv_star_inline() {
        let result = latex_to_typst(r"\fdv*{F}{g}");
        assert!(
            result.contains("delta") && result.contains("/") && result.contains("delta"),
            "\\fdv*{{F}}{{g}} should produce inline delta F / delta g, got: {}",
            result
        );
    }

    #[test]
    fn test_fdv_star_optional_order() {
        let result = latex_to_typst(r"\fdv*[2]{F}{g}");
        assert!(
            result.contains("delta^2") && result.contains("g^2") && result.contains("/"),
            "\\fdv*[2]{{F}}{{g}} should produce inline delta^2 F / delta g^2, got: {}",
            result
        );
    }

    // --- Dirac notation ---

    #[test]
    fn test_ket() {
        let result = latex_to_typst(r"\ket{\psi}");
        assert!(
            result.contains("lr(|") && result.contains("angle.r"),
            "\\ket should produce lr(| ψ angle.r), got: {}",
            result
        );
    }

    #[test]
    fn test_bra() {
        let result = latex_to_typst(r"\bra{\phi}");
        assert!(
            result.contains("angle.l") && result.contains("|)"),
            "\\bra should produce lr(angle.l φ |), got: {}",
            result
        );
    }

    #[test]
    fn test_braket_two_args() {
        let result = latex_to_typst(r"\braket{a}{b}");
        assert!(
            result.contains("angle.l") && result.contains("|") && result.contains("angle.r"),
            "\\braket{{a}}{{b}} should produce lr(angle.l a | b angle.r), got: {}",
            result
        );
    }

    #[test]
    fn test_braket_single_arg() {
        let result = latex_to_typst(r"\braket{a}");
        let output = result.trim();
        // Single-arg braket: ⟨a|a⟩
        assert!(
            output.contains("angle.l") && output.contains("angle.r"),
            "\\braket{{a}} should produce lr(angle.l a | a angle.r), got: {}",
            result
        );
    }

    #[test]
    fn test_expval_implicit() {
        let result = latex_to_typst(r"\expval{A}");
        assert!(
            result.contains("angle.l") && result.contains("angle.r"),
            "\\expval{{A}} should produce lr(angle.l A angle.r), got: {}",
            result
        );
    }

    #[test]
    fn test_expval_explicit() {
        let result = latex_to_typst(r"\expval{A}{\Psi}");
        eprintln!("expval result: {}", result);
        assert!(
            result.contains("angle.l") && result.contains("|") && result.contains("angle.r"),
            "\\expval{{A}}{{Ψ}} should produce lr(angle.l Ψ | A | Ψ angle.r), got: {}",
            result
        );
    }

    #[test]
    fn test_mel() {
        let result = latex_to_typst(r"\mel{n}{A}{m}");
        assert!(
            result.contains("angle.l") && result.contains("|") && result.contains("angle.r"),
            "\\mel{{n}}{{A}}{{m}} should produce lr(angle.l n | A | m angle.r), got: {}",
            result
        );
    }

    #[test]
    fn test_dyad() {
        let result = latex_to_typst(r"\dyad{a}{b}");
        eprintln!("dyad result: {}", result);
        // |a⟩⟨b|
        assert!(
            result.contains("angle.r") && result.contains("angle.l"),
            "\\dyad{{a}}{{b}} should produce |a⟩⟨b|, got: {}",
            result
        );
    }

    // --- Quick quad text ---

    #[test]
    fn test_qq() {
        let result = latex_to_typst(r"\qq{hello}");
        assert!(
            result.contains("quad") && result.contains("hello"),
            "\\qq{{hello}} should produce quad \"hello\" quad, got: {}",
            result
        );
    }

    #[test]
    fn test_qif() {
        let result = latex_to_typst(r"\qif");
        assert!(
            result.contains("quad") && result.contains("if"),
            "\\qif should produce quad \"if\" quad, got: {}",
            result
        );
    }

    #[test]
    fn test_qand() {
        let result = latex_to_typst(r"\qand");
        assert!(
            result.contains("quad") && result.contains("and"),
            "\\qand should produce quad \"and\" quad, got: {}",
            result
        );
    }

    // --- Matrix macros ---

    #[test]
    fn test_pmqty() {
        let result = latex_to_typst(r"\pmqty{a & b \\ c & d}");
        eprintln!("pmqty result: {}", result);
        assert!(
            result.contains("mat("),
            "\\pmqty should produce mat(...), got: {}",
            result
        );
    }

    #[test]
    fn test_bmqty() {
        let result = latex_to_typst(r"\bmqty{a & b \\ c & d}");
        assert!(
            result.contains("mat(") && result.contains("["),
            "\\bmqty should produce mat(delim: \"[\", ...), got: {}",
            result
        );
    }

    #[test]
    fn test_vmqty() {
        let result = latex_to_typst(r"\vmqty{a & b \\ c & d}");
        assert!(
            result.contains("mat(") && result.contains("|"),
            "\\vmqty should produce mat(delim: \"|\", ...), got: {}",
            result
        );
    }

    // --- Combined / integration ---

    #[test]
    fn test_physics_in_document() {
        // Realistic physics document snippet
        let input = r#"\documentclass{article}
\begin{document}
The Schrödinger equation: $i \hbar \pdv{}{t} \ket{\psi} = H \ket{\psi}$

Expectation value: $\expval{H}{\psi}$

Commutator: $\comm{x}{p} = i\hbar$
\end{document}
"#;
        let result = latex_document_to_typst(input);
        eprintln!("Physics document result:\n{}", result);

        // Should not contain error markers
        assert!(
            !result.contains("Error"),
            "Document conversion should not produce errors, got: {}",
            result
        );

        // Key physics constructs should be present
        assert!(
            result.contains("diff") || result.contains("frac"),
            "Should contain partial derivative, got: {}",
            result
        );
        assert!(
            result.contains("angle.l") || result.contains("lr(|"),
            "Should contain bra-ket notation, got: {}",
            result
        );
    }

    #[test]
    fn test_grad_div_curl_laplacian() {
        // Zero-argument vector calculus operators
        let result = latex_to_typst(r"\grad");
        assert!(
            result.contains("nabla"),
            "\\grad should map to nabla, got: {}",
            result
        );

        let result = latex_to_typst(r"\laplacian");
        assert!(
            result.contains("nabla"),
            "\\laplacian should map to nabla^2, got: {}",
            result
        );
    }

    #[test]
    fn test_eval() {
        let result = latex_to_typst(r"\eval{x^2}");
        assert!(
            result.contains("bar.v") || result.contains("|"),
            "\\eval should produce evaluation bar, got: {}",
            result
        );
    }

    #[test]
    fn test_vev() {
        let result = latex_to_typst(r"\vev{A}");
        assert!(
            result.contains("angle.l") && result.contains("0") && result.contains("angle.r"),
            "\\vev{{A}} should produce lr(angle.l 0 | A | 0 angle.r), got: {}",
            result
        );
    }

    // --- Vector calculus with arguments ---

    #[test]
    fn test_grad_with_arg() {
        let result = latex_to_typst(r"\grad{\Psi}");
        assert!(
            result.contains("nabla"),
            "\\grad{{Ψ}} should contain nabla, got: {}",
            result
        );
    }

    #[test]
    fn test_divergence_with_arg() {
        let result = latex_to_typst(r"\divergence{\vb{A}}");
        assert!(
            result.contains("nabla") && result.contains("dot.op"),
            "\\divergence should produce nabla dot.op ..., got: {}",
            result
        );
    }

    #[test]
    fn test_curl_with_arg() {
        let result = latex_to_typst(r"\curl{\vb{B}}");
        assert!(
            result.contains("nabla") && result.contains("times"),
            "\\curl should produce nabla times ..., got: {}",
            result
        );
    }

    #[test]
    fn test_laplacian_with_arg() {
        let result = latex_to_typst(r"\laplacian{\Psi}");
        assert!(
            result.contains("nabla^2"),
            "\\laplacian should produce nabla^2 ..., got: {}",
            result
        );
    }

    // --- Star variants ---

    #[test]
    fn test_abs_star() {
        let result = latex_to_typst(r"\abs*{x}");
        assert!(
            result.contains("abs("),
            "\\abs*{{x}} should produce abs(...), got: {}",
            result
        );
    }

    #[test]
    fn test_dv_star_inline() {
        let result = latex_to_typst(r"\dv*{f}{x}");
        assert!(
            result.contains("/") && result.contains("dif"),
            "\\dv*{{f}}{{x}} should produce inline form dif f / dif x, got: {}",
            result
        );
        // Should NOT contain frac() for star variant
        assert!(
            !result.contains("frac("),
            "\\dv* should use / not frac, got: {}",
            result
        );
    }

    #[test]
    fn test_braket_star() {
        let result = latex_to_typst(r"\braket*{a}{b}");
        assert!(
            result.contains("angle.l") && result.contains("angle.r"),
            "\\braket*{{a}}{{b}} should produce braket notation, got: {}",
            result
        );
    }

    // --- Matrix generators ---

    #[test]
    fn test_imat() {
        let result = latex_to_typst(r"\imat{2}");
        assert!(
            result.contains("mat(") && result.contains("1") && result.contains("0"),
            "\\imat{{2}} should produce 2x2 identity matrix, got: {}",
            result
        );
    }

    #[test]
    fn test_pmat_pauli() {
        let result = latex_to_typst(r"\pmat{1}");
        assert!(
            result.contains("mat(") && result.contains("0") && result.contains("1"),
            "\\pmat{{1}} should produce Pauli sigma_x matrix, got: {}",
            result
        );
    }

    #[test]
    fn test_dmat() {
        let result = latex_to_typst(r"\dmat{a,b,c}");
        assert!(
            result.contains("mat("),
            "\\dmat{{a,b,c}} should produce diagonal matrix, got: {}",
            result
        );
    }

    #[test]
    fn test_zmat() {
        let result = latex_to_typst(r"\zmat{2}{3}");
        assert!(
            result.contains("mat(") && result.contains("0"),
            "\\zmat{{2}}{{3}} should produce 2x3 zero matrix, got: {}",
            result
        );
    }

    // --- flatfrac ---

    #[test]
    fn test_flatfrac() {
        let result = latex_to_typst(r"\flatfrac{a}{b}");
        assert!(
            result.contains("/"),
            "\\flatfrac{{a}}{{b}} should produce a / b, got: {}",
            result
        );
    }
}

// ============================================================================
// T2L Symbol Mapping Tests - Typst to LaTeX
// ============================================================================

mod t2l_symbol_mappings {
    use super::*;

    // --- Direct map lookup tests (verify data is present) ---

    #[test]
    fn test_mapping_data_greek_uppercase() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("Alpha"), Some(&"A"));
        assert_eq!(TYPST_TO_TEX.get("Beta"), Some(&"B"));
        assert_eq!(TYPST_TO_TEX.get("Zeta"), Some(&"Z"));
        assert_eq!(TYPST_TO_TEX.get("digamma"), Some(&"\\digamma"));
    }

    #[test]
    fn test_mapping_data_blackboard_bold() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("BB"), Some(&"\\mathbb{B}"));
        assert_eq!(TYPST_TO_TEX.get("DD"), Some(&"\\mathbb{D}"));
        assert_eq!(TYPST_TO_TEX.get("PP"), Some(&"\\mathbb{P}"));
        assert_eq!(TYPST_TO_TEX.get("FF"), Some(&"\\mathbb{F}"));
    }

    #[test]
    fn test_mapping_data_arrows() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("arrow.r.not"), Some(&"\\nrightarrow"));
        assert_eq!(TYPST_TO_TEX.get("arrow.l.not"), Some(&"\\nleftarrow"));
        assert_eq!(TYPST_TO_TEX.get("arrow.ccw"), Some(&"\\curvearrowleft"));
        assert_eq!(TYPST_TO_TEX.get("arrow.cw"), Some(&"\\curvearrowright"));
        assert_eq!(
            TYPST_TO_TEX.get("arrow.l.r.wave"),
            Some(&"\\leftrightsquigarrow")
        );
        assert_eq!(
            TYPST_TO_TEX.get("harpoons.ltrb"),
            Some(&"leftrightharpoons")
        );
        assert_eq!(
            TYPST_TO_TEX.get("harpoons.rtlb"),
            Some(&"rightleftharpoons")
        );
    }

    #[test]
    fn test_mapping_data_comparisons() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("lt.tilde"), Some(&"\\lesssim"));
        assert_eq!(TYPST_TO_TEX.get("gt.tilde"), Some(&"\\gtrsim"));
        assert_eq!(TYPST_TO_TEX.get("lt.approx"), Some(&"\\lessapprox"));
        assert_eq!(TYPST_TO_TEX.get("gt.approx"), Some(&"\\gtrapprox"));
        assert_eq!(TYPST_TO_TEX.get("lt.tri"), Some(&"\\vartriangleleft"));
        assert_eq!(TYPST_TO_TEX.get("gt.tri.eq"), Some(&"\\trianglerighteq"));
    }

    #[test]
    fn test_mapping_data_precedence() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("prec.tilde"), Some(&"\\precsim"));
        assert_eq!(TYPST_TO_TEX.get("succ.tilde"), Some(&"\\succsim"));
        assert_eq!(TYPST_TO_TEX.get("prec.curly.eq"), Some(&"\\preccurlyeq"));
        assert_eq!(TYPST_TO_TEX.get("succ.approx"), Some(&"\\succapprox"));
    }

    #[test]
    fn test_mapping_data_sets() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("subset.neq"), Some(&"\\subsetneq"));
        assert_eq!(TYPST_TO_TEX.get("supset.neq"), Some(&"\\supsetneq"));
        assert_eq!(TYPST_TO_TEX.get("union.plus"), Some(&"\\uplus"));
        assert_eq!(TYPST_TO_TEX.get("inter.sq"), Some(&"\\sqcap"));
        assert_eq!(TYPST_TO_TEX.get("without"), Some(&"\\setminus"));
    }

    #[test]
    fn test_mapping_data_binary_ops() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("plus.square"), Some(&"\\boxplus"));
        assert_eq!(TYPST_TO_TEX.get("minus.square"), Some(&"\\boxminus"));
        assert_eq!(TYPST_TO_TEX.get("times.square"), Some(&"\\boxtimes"));
        assert_eq!(TYPST_TO_TEX.get("dot.circle"), Some(&"\\odot"));
        assert_eq!(TYPST_TO_TEX.get("minus.circle"), Some(&"\\ominus"));
    }

    #[test]
    fn test_mapping_data_misc() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("dotless.i"), Some(&"\\imath"));
        assert_eq!(TYPST_TO_TEX.get("dotless.j"), Some(&"\\jmath"));
        assert_eq!(TYPST_TO_TEX.get("product.co"), Some(&"\\coprod"));
        // Note: flat/natural/sharp use values without backslash in the original map
        assert!(TYPST_TO_TEX.get("flat").is_some());
        assert!(TYPST_TO_TEX.get("natural").is_some());
        assert!(TYPST_TO_TEX.get("sharp").is_some());
    }

    #[test]
    fn test_mapping_data_suits_triangles() {
        use tylax::data::maps::TYPST_TO_TEX;
        assert_eq!(TYPST_TO_TEX.get("suit.club.filled"), Some(&"\\clubsuit"));
        assert_eq!(TYPST_TO_TEX.get("suit.heart.stroked"), Some(&"\\heartsuit"));
        assert_eq!(TYPST_TO_TEX.get("triangle.stroked.t"), Some(&"\\triangle"));
        assert_eq!(
            TYPST_TO_TEX.get("triangle.filled.t"),
            Some(&"\\blacktriangle")
        );
    }

    // --- End-to-end pipeline tests (symbols that the parser handles correctly) ---

    #[test]
    fn test_digamma_pipeline() {
        let result = typst_to_latex("$digamma$");
        assert!(
            result.contains("digamma"),
            "digamma should convert through pipeline, got: {}",
            result
        );
    }

    #[test]
    fn test_music_symbols_pipeline() {
        let result = typst_to_latex("$flat + natural + sharp$");
        assert!(
            result.contains("flat") && result.contains("natural") && result.contains("sharp"),
            "music symbols should convert through pipeline, got: {}",
            result
        );
    }

    #[test]
    fn test_triangle_pipeline() {
        let result = typst_to_latex("$triangle.stroked.t + triangle.filled.t$");
        assert!(
            result.contains("triangle") || result.contains("blacktriangle"),
            "triangle symbols should convert through pipeline, got: {}",
            result
        );
    }

    #[test]
    fn test_greek_uppercase_pipeline() {
        // Single uppercase Greek letters - these are also valid identifiers
        let result = typst_to_latex("$Alpha$");
        // May produce "A" or "Alpha" depending on parser
        assert!(
            !result.is_empty(),
            "Alpha should produce output, got: {}",
            result
        );
    }

    // --- Overall coverage test ---

    #[test]
    fn test_typst_to_tex_mapping_count() {
        use tylax::data::maps::TYPST_TO_TEX;
        let count = TYPST_TO_TEX.len();
        eprintln!("TYPST_TO_TEX mapping count: {}", count);
        assert!(
            count > 400,
            "Expected 400+ TYPST_TO_TEX mappings after extension, got {}",
            count
        );
    }
}
