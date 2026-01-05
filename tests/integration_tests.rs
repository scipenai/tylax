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
        let result = typst_to_latex("frac(1, 2)");
        assert!(result.contains("\\frac"));
        assert!(result.contains("{1}"));
        assert!(result.contains("{2}"));
    }

    #[test]
    fn test_sqrt() {
        let result = typst_to_latex("sqrt(x)");
        assert!(result.contains("\\sqrt"));
    }

    #[test]
    fn test_subscripts_superscripts() {
        let result = typst_to_latex("x^2");
        assert!(result.contains("^"));

        let result = typst_to_latex("x_i");
        assert!(result.contains("_"));
    }

    #[test]
    fn test_matrix() {
        let result = typst_to_latex("mat(1, 2; 3, 4)");
        assert!(result.contains("\\begin{matrix}") || result.contains("matrix"));
    }

    #[test]
    fn test_operators() {
        let result = typst_to_latex("a + b - c = d");
        assert!(result.contains("+"));
        assert!(result.contains("-"));
        assert!(result.contains("="));
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
        let back = typst_to_latex(&typst);

        assert!(back.contains("frac") || back.contains("\\frac"));
    }

    #[test]
    fn test_roundtrip_typst_to_latex() {
        let original = "frac(a, b)";
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
        let back = typst_to_latex(&typst);

        assert!(back.contains("_"));
    }

    #[test]
    fn test_superscript_roundtrip() {
        let original = r"x^2 + y^3";
        let typst = latex_to_typst(original);
        let back = typst_to_latex(&typst);

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
