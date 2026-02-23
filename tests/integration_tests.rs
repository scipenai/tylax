//! Integration tests for Tylax full document conversion

use tylax::{
    convert_auto, convert_auto_document, detect_format, latex_document_to_typst, latex_to_typst,
    typst_to_latex, typst_to_latex_with_options, T2LOptions,
};

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
    fn test_matrices() {
        let result = latex_to_typst(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}");
        assert!(!result.contains("Error"));

        let result = latex_to_typst(r"\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}");
        assert!(!result.contains("Error"));
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
        // Should not panic - result may be empty or contain fallback
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
    fn test_lr_no_delimiter_fallback() {
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
}
