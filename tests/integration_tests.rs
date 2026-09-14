//! Integration tests for Tylax full document conversion

use std::io::Write;
use std::process::{Command, Stdio};

use tylax::{
    convert_auto, convert_auto_document, detect_format, latex_document_to_typst,
    latex_document_to_typst_with_options, latex_to_typst, typst_to_latex,
    typst_to_latex_with_diagnostics, typst_to_latex_with_options, L2TOptions, PreambleMode,
    T2LOptions,
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

#[cfg(not(target_arch = "wasm32"))]
mod batch_conversion_tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use tylax::batch::{convert_batch, BatchDirection, BatchError, BatchFileStatus, BatchOptions};
    use tylax::{DocumentWrapperMode, L2TOptions, PreambleMode, T2LOptions};

    struct TempProject {
        root: PathBuf,
    }

    impl TempProject {
        fn new(name: &str) -> Self {
            let mut root = std::env::temp_dir();
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos();
            root.push(format!("tylax-{name}-{nonce}"));
            fs::create_dir_all(&root).expect("temp project should be created");
            Self { root }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.path(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent directory should be created");
            }
            fs::write(path, content).expect("fixture should be written");
        }

        fn read(&self, relative: &str) -> String {
            fs::read_to_string(self.path(relative)).expect("fixture output should be readable")
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn ok_files(report: &tylax::batch::BatchReport) -> Vec<String> {
        let mut files = report
            .results
            .iter()
            .filter_map(|result| match &result.status {
                BatchFileStatus::Converted => result
                    .output_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_string()),
                BatchFileStatus::Failed(_) => None,
            })
            .collect::<Vec<_>>();
        files.sort();
        files
    }

    fn options(input: &Path, output: &Path) -> BatchOptions {
        BatchOptions {
            input: input.to_path_buf(),
            output_dir: output.to_path_buf(),
            ..Default::default()
        }
    }

    #[test]
    fn batch_non_recursive_converts_only_top_level_matching_files() {
        let project = TempProject::new("batch-non-recursive");
        project.write("top.typ", "Hello");
        project.write("nested/child.typ", "Nested");
        project.write("notes.md", "skip");

        let output = project.path("out");
        let mut opts = options(&project.root, &output);
        opts.direction = BatchDirection::TypstToLatex;

        let report = convert_batch(&opts).expect("batch conversion should succeed");

        assert_eq!(report.success_count, 1);
        assert_eq!(report.error_count, 0);
        assert!(output.join("top.tex").exists());
        assert!(!output.join("nested/child.tex").exists());
        assert_eq!(ok_files(&report), vec!["top.tex"]);
    }

    #[test]
    fn batch_recursive_preserves_relative_tree() {
        let project = TempProject::new("batch-recursive");
        project.write("Root/Intro.typ", "Intro");
        project.write("Root/Chapter/Section.typ", "Section");

        let output = project.path("converted");
        let mut opts = options(&project.root, &output);
        opts.direction = BatchDirection::TypstToLatex;
        opts.recursive = true;

        let report = convert_batch(&opts).expect("batch conversion should succeed");

        assert_eq!(report.success_count, 2);
        assert_eq!(report.error_count, 0);
        assert!(output.join("Root/Intro.tex").exists());
        assert!(output.join("Root/Chapter/Section.tex").exists());
    }

    #[test]
    fn batch_recursive_skips_existing_nested_output_directory() {
        let project = TempProject::new("batch-recursive-output");
        project.write("Root/Intro.typ", "Intro");
        project.write("out/old.typ", "old generated output");

        let output = project.path("out");
        let mut opts = options(&project.root, &output);
        opts.direction = BatchDirection::TypstToLatex;
        opts.recursive = true;

        let report = convert_batch(&opts).expect("batch conversion should succeed");

        assert_eq!(report.success_count, 1);
        assert!(output.join("Root/Intro.tex").exists());
        assert!(!output.join("out/old.tex").exists());
    }

    #[test]
    fn batch_auto_handles_mixed_sources_and_skips_unrelated_files() {
        let project = TempProject::new("batch-auto");
        project.write("paper.typ", "Typst body");
        project.write("equation.tex", r"\frac{a}{b}");
        project.write("refs.bib", "@book{a}");
        project.write("README.md", "skip");

        let output = project.path("out");
        let mut opts = options(&project.root, &output);
        opts.direction = BatchDirection::Auto;

        let report = convert_batch(&opts).expect("batch conversion should succeed");

        assert_eq!(report.success_count, 2);
        assert_eq!(report.error_count, 0);
        assert!(output.join("paper.tex").exists());
        assert!(output.join("equation.typ").exists());
        assert!(!output.join("refs.bib").exists());
        assert!(!output.join("README.md").exists());
    }

    #[test]
    fn batch_exclude_globs_skip_files_and_prune_directories() {
        let project = TempProject::new("batch-exclude");
        project.write("Root/Keep.typ", "Keep");
        project.write("Root/Draft.typ", "Draft");
        project.write("ProjectTemplate/Math.typ", "Template");

        let output = project.path("out");
        let mut opts = options(&project.root, &output);
        opts.direction = BatchDirection::TypstToLatex;
        opts.recursive = true;
        opts.excludes = vec![
            "Root/**/Draft.typ".to_string(),
            "ProjectTemplate/**".to_string(),
        ];

        let report = convert_batch(&opts).expect("batch conversion should succeed");

        assert_eq!(report.success_count, 1);
        assert_eq!(report.error_count, 0);
        assert!(output.join("Root/Keep.tex").exists());
        assert!(!output.join("Root/Draft.tex").exists());
        assert!(!output.join("ProjectTemplate/Math.tex").exists());
    }

    #[test]
    fn batch_detects_output_collisions_before_writing() {
        let project = TempProject::new("batch-collision");
        project.write("same.typ", "Typst");
        project.write("same.tex", r"\alpha");

        let output = project.path("out");
        let mut opts = options(&project.root, &output);
        opts.direction = BatchDirection::Auto;
        opts.output_extension = Some("out".to_string());

        let err = convert_batch(&opts).expect_err("same stem should collide");

        assert!(matches!(err, BatchError::OutputCollision { .. }));
        assert!(!output.join("same.out").exists());
    }

    #[test]
    fn batch_options_are_used_for_converted_documents() {
        let project = TempProject::new("batch-options");
        project.write("doc.typ", "= Hi\n\nbody");
        project.write(
            "doc.tex",
            r"\documentclass{article}\begin{document}\section{Hi}body\end{document}",
        );

        let t2l_output = project.path("out-t2l");
        let mut t2l = options(&project.path("doc.typ"), &t2l_output);
        t2l.direction = BatchDirection::TypstToLatex;
        t2l.full_document = true;
        t2l.t2l_options = T2LOptions {
            wrapper: DocumentWrapperMode::BodyOnly,
            ..Default::default()
        };

        convert_batch(&t2l).expect("t2l batch should succeed");
        let t2l_doc = project.read("out-t2l/doc.tex");
        assert!(!t2l_doc.contains(r"\documentclass"));
        assert!(t2l_doc.contains(r"\section{"));
        assert!(t2l_doc.contains("Hi"));

        let l2t_output = project.path("out-l2t");
        let mut l2t = options(&project.path("doc.tex"), &l2t_output);
        l2t.direction = BatchDirection::LatexToTypst;
        l2t.full_document = true;
        l2t.l2t_options = L2TOptions {
            preamble: PreambleMode::None,
            ..Default::default()
        };

        convert_batch(&l2t).expect("l2t batch should succeed");
        let l2t_doc = project.read("out-l2t/doc.typ");
        assert!(!l2t_doc.contains("#set page"));
        assert!(l2t_doc.contains("= Hi"));
    }

    #[test]
    fn cli_batch_accepts_recursive_and_exclude_flags() {
        let project = TempProject::new("batch-cli");
        project.write("Root/Keep.typ", "Keep");
        project.write("Root/Draft.typ", "Draft");
        project.write("ProjectTemplate/Math.typ", "Template");

        let output = project.path("out");
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_t2l"))
            .arg("batch")
            .arg(&project.root)
            .arg("--output-dir")
            .arg(&output)
            .arg("--direction")
            .arg("t2l")
            .arg("--recursive")
            .arg("--exclude")
            .arg("ProjectTemplate/**")
            .arg("--exclude")
            .arg("Root/**/Draft.typ")
            .output()
            .expect("t2l batch CLI should run");

        assert!(
            result.status.success(),
            "batch CLI failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(output.join("Root/Keep.tex").exists());
        assert!(!output.join("Root/Draft.tex").exists());
        assert!(!output.join("ProjectTemplate/Math.tex").exists());
    }
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
    fn test_math_literal_slash_becomes_slash_symbol() {
        assert_eq!(
            latex_to_typst(r"\mathbb{R} / \mathbb{Q}").trim(),
            "RR slash QQ"
        );
        assert_eq!(latex_to_typst(r"\alpha / x").trim(), "alpha slash x");
        assert_eq!(
            latex_to_typst(r"\overrightarrow{A}/\overleftarrow{B}").trim(),
            "arrow(A) slash arrow.l(B)"
        );
        assert_eq!(latex_to_typst("$a/b$").trim(), "$a slash b$");
        let no_preamble = L2TOptions {
            preamble: PreambleMode::None,
            ..Default::default()
        };
        assert_eq!(
            latex_document_to_typst_with_options(r"\mathbb{R} / \mathbb{Q}", &no_preamble).trim(),
            "RR slash QQ"
        );
        assert_eq!(
            latex_document_to_typst_with_options("text / path", &no_preamble).trim(),
            "text / path"
        );
        assert_eq!(
            latex_document_to_typst_with_options(r"\cite{a} / \cite{b}", &no_preamble).trim(),
            "#cite(<a>) / #cite(<b>)"
        );

        // `\frac` slash notation is produced by the fraction converter itself,
        // not by a LaTeX literal `/`, so the existing frac_to_slash option keeps
        // using Typst's division shorthand for simple fractions.
        assert_eq!(latex_to_typst(r"\frac{a}b").trim(), "a/b");
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
        // The `\\` row break must survive as a `;` row separator: mitex parses
        // `\\` as an `ItemNewLine`, which the Matrix env maps to `;`. Losing it
        // would merge rows into `mat(a, b c, d)` (wrong, and loses cells).
        assert!(
            result.contains(';'),
            "matrix rows must be separated by `;`, got: {}",
            result
        );

        let result = latex_to_typst(r"\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}");
        assert!(!result.contains("Error"));
        assert!(
            result.contains(';'),
            "matrix rows must be separated by `;`, got: {}",
            result
        );
    }

    #[test]
    fn test_matrix_row_spacing_optional_arg_is_dropped() {
        // Issue #41: the optional vertical-spacing argument of `\\` (e.g.
        // `\\[6pt]`) must be consumed, not leaked into the matrix as an extra
        // cell like `mat(..., a, b ;[6 p t ] c, d)`.
        let result = latex_to_typst(r"\begin{pmatrix} a & b \\[6pt] c & d \end{pmatrix}");
        assert!(!result.contains("Error"));
        assert!(
            !result.contains("6pt") && !result.contains("6 p t") && !result.contains('['),
            "row-spacing option must be dropped, got: {}",
            result
        );
        assert!(
            result.contains("a, b ; c, d"),
            "rows must stay clean and separated, got: {}",
            result
        );

        // Same for bmatrix, and with a decimal em length. (The `[` of the
        // bmatrix delimiter `delim: "["` is expected; the leaked length is not.)
        let b = latex_to_typst(r"\begin{bmatrix} 1 & 2 \\[1.5em] 3 & 4 \end{bmatrix}");
        assert!(
            !b.contains("1.5") && !b.contains("1 . 5") && b.contains("1, 2 ; 3, 4"),
            "bmatrix row spacing must be dropped, got: {}",
            b
        );

        // A non-dimension bracket right after `\\` is genuine content and must
        // be preserved (conservative: only real lengths are consumed).
        let kept = latex_to_typst(r"\begin{pmatrix} a \\[x] b \end{pmatrix}");
        assert!(
            kept.contains("[x"),
            "non-dimension bracket must be kept, got: {}",
            kept
        );

        // Do not accept a known unit as merely a prefix, or an invalid numeric
        // factor: neither `6ptfoo` nor `1..2pt` is a TeX dimension, so both
        // must remain visible instead of being silently lost.
        for invalid in ["6ptfoo", "1..2pt"] {
            let invalid_unit = latex_to_typst(&format!(
                r"\begin{{pmatrix}} a \\[{invalid}] b \end{{pmatrix}}"
            ));
            let visible = invalid
                .chars()
                .map(|character| character.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                invalid_unit.contains(&visible),
                "invalid row spacing `{invalid}` must be preserved, got: {invalid_unit}"
            );
        }

        // Standard variants that are legitimate row spacing must still be
        // consumed: a `true` unit, elastic glue, and a user length command.
        for spacing in [".5truept", "1fil", r"\baselineskip"] {
            let latex = format!(r"\begin{{pmatrix}} a \\[{spacing}] b \end{{pmatrix}}");
            let converted = latex_to_typst(&latex);
            assert!(
                converted.contains("a ; b") && !converted.contains('['),
                "valid row spacing `{spacing}` must be dropped, got: {converted}"
            );
        }

        // TeX accepts whitespace between `\\` and its optional argument.
        let next_line = latex_to_typst(concat!(
            "\\begin{pmatrix} a ",
            "\\\\",
            "\n",
            "[6pt] b \\end{pmatrix}",
        ));
        assert!(
            next_line.contains("a ; b") && !next_line.contains("6 p t"),
            "next-line row spacing must be dropped, got: {}",
            next_line
        );

        // The CLI commonly receives Windows CRLF input, so this must take the
        // same path as the LF-only form above.
        let windows_next_line = latex_to_typst(concat!(
            "\\begin{pmatrix} a ",
            "\\\\",
            "\r\n",
            "[6pt] b \\end{pmatrix}",
        ));
        assert!(
            windows_next_line.contains("a ; b") && !windows_next_line.contains("6 p t"),
            "CRLF row spacing must be dropped, got: {}",
            windows_next_line
        );

        // #41 was reported through the full-document path (`t2l -f`), which runs
        // the document converter (markup mode), not the math-only one above.
        // Guard that entry point too: `\\[6pt]` inside an `equation`-wrapped
        // `pmatrix` must still drop the spacing and keep the `;` row separator.
        let doc = latex_document_to_typst_with_options(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{equation}\nA = \\begin{pmatrix} a & b \\\\[6pt] c & d \\end{pmatrix}\n\
             \\end{equation}\n\\end{document}\n",
            &L2TOptions::default(),
        );
        assert!(
            doc.contains("a, b ; c, d") && !doc.contains("6pt") && !doc.contains("6 p t"),
            "full-document path must also drop `\\\\[6pt]`, got: {}",
            doc
        );
    }

    #[test]
    fn test_left_brace_array_becomes_cases() {
        // `\left\{ ... \right.` with a null right delimiter is a piecewise
        // definition, which Typst spells `cases(...)`. Root cause ①.
        let result = latex_to_typst(
            r"f(x)=\left\{\begin{array}{ll} x & x>0 \\ -x & x\le 0 \end{array}\right.",
        );
        assert!(
            result.contains("cases("),
            "left-brace array should become cases(), got: {}",
            result
        );
        // Both rows must be present and separated (not merged).
        assert!(
            result.contains("x & x > 0") && result.contains("- x & x <= 0"),
            "both piecewise rows must be preserved, got: {}",
            result
        );
        assert!(
            !result.contains("lr(") && !result.contains("mat("),
            "cases path should not leak lr()/mat(), got: {}",
            result
        );

        // A *balanced* `\{ ... \}` is a set, not a piecewise def: leave it alone.
        let set = latex_to_typst(r"A = \{ x \mid x > 0 \}");
        assert!(
            !set.contains("cases("),
            "a balanced brace set must not become cases(), got: {}",
            set
        );

        // Preserve the AST-level distinction between an explicit array column
        // (`&`) and a literal comma within that column. The comma must be
        // escaped in Typst's `cases(...)` syntax rather than becoming a third
        // column or a second row.
        let array_with_literal_comma =
            latex_to_typst(r"\left\{\begin{array}{ll}x & y, z \\ u & v\end{array}\right.");
        assert!(
            array_with_literal_comma.contains(r"cases(x & y\, z, u & v)"),
            "array cell comma must stay literal, got: {}",
            array_with_literal_comma
        );

        let aligned_with_literal_comma =
            latex_to_typst(r"\left\{\begin{aligned}x & y, z \\ u & v\end{aligned}\right.");
        assert!(
            aligned_with_literal_comma.contains(r"cases(x & y\, z, u & v)"),
            "aligned cell comma must stay literal, got: {}",
            aligned_with_literal_comma
        );
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
        // \cdots maps to dots.h.c (horizontal centered dots), aligned with the
        // mapping merged via #24.
        assert_eq!(latex_to_typst(r"\cdots").trim(), "dots.h.c");
    }

    #[test]
    fn test_brace_annotation_folds_into_second_argument() {
        // `\underbrace{body}_{label}` / `\overbrace{body}^{label}` express the
        // brace label as a script in LaTeX, but Typst takes it as a second
        // positional argument. Emitting `underbrace(body)_(label)` would render
        // the label as a real subscript instead of the brace annotation.
        assert_eq!(
            latex_to_typst(r"$\underbrace{a+b}_{c}$").trim(),
            "$underbrace(a + b, c)$"
        );
        assert_eq!(
            latex_to_typst(r"$\overbrace{x+y}^{n}$").trim(),
            "$overbrace(x + y, n)$"
        );
        // A bare (unbraced) script is folded the same way.
        assert_eq!(
            latex_to_typst(r"$\underbrace{a+b}_c$").trim(),
            "$underbrace(a + b, c)$"
        );
        // No script: the single-argument form is preserved unchanged.
        assert_eq!(
            latex_to_typst(r"$\underbrace{q}$").trim(),
            "$underbrace(q)$"
        );
        // Non-canonical pairings (underbrace with `^`, overbrace with `_`) are
        // left as ordinary scripts rather than mis-folded.
        assert_eq!(
            latex_to_typst(r"$\underbrace{x}^{y}$").trim(),
            "$underbrace(x)^(y)$"
        );
        assert_eq!(
            latex_to_typst(r"$\overbrace{x}_{y}$").trim(),
            "$overbrace(x)_(y)$"
        );
    }

    #[test]
    fn test_brace_annotation_top_level_comma_is_protected() {
        // A top-level comma in the label/body would make `underbrace(body,
        // label)` parse as extra positional arguments, so the operand is wrapped
        // in `{}` while still folding into the brace annotation.
        assert_eq!(
            latex_to_typst(r"$\underbrace{x}_{a, b}$").trim(),
            "$underbrace(x, {a, b})$"
        );
        assert_eq!(
            latex_to_typst(r"$\underbrace{a, b}_{c}$").trim(),
            "$underbrace({a, b}, c)$"
        );
        // Commas nested inside parens or a string literal are not top-level, so
        // these still fold safely.
        assert_eq!(
            latex_to_typst(r"$\underbrace{f(x,y)}_{c}$").trim(),
            "$underbrace(f(x,y), c)$"
        );
        assert_eq!(
            latex_to_typst(r"$\underbrace{x}_{\text{a, b}}$").trim(),
            r#"$underbrace(x, #text[a, b])$"#
        );
    }

    #[test]
    fn test_circled_operators_use_dot_o_spelling() {
        // Issue #27: Typst deprecated the `.circle` circled-operator spelling in
        // favour of `.o` (e.g. `times.o`). Emitting `times.circle` renders a
        // deprecation warning and will eventually stop compiling.
        assert_eq!(latex_to_typst(r"\otimes").trim(), "times.o");
        assert_eq!(latex_to_typst(r"\oplus").trim(), "plus.o");
        assert_eq!(latex_to_typst(r"\ominus").trim(), "minus.o");
        assert_eq!(latex_to_typst(r"\odot").trim(), "dot.o");
        assert_eq!(latex_to_typst(r"\oslash").trim(), "slash.o");
        // n-ary (big) variants take the `.o.big` suffix.
        assert_eq!(latex_to_typst(r"\bigotimes").trim(), "times.o.big");
        assert_eq!(latex_to_typst(r"\bigoplus").trim(), "plus.o.big");
        assert_eq!(latex_to_typst(r"\bigodot").trim(), "dot.o.big");
    }

    #[test]
    fn test_intersection_uses_inter_not_sect() {
        // Issue #33: Typst renamed the intersection symbol `sect` -> `inter`
        // (and `sect.big` -> `inter.big`); the old spelling now renders a
        // deprecation warning. Union was *not* renamed, so it stays `union`.
        assert_eq!(latex_to_typst(r"\cap").trim(), "inter");
        assert_eq!(latex_to_typst(r"\bigcap").trim(), "inter.big");
        assert_eq!(latex_to_typst(r"\cup").trim(), "union");
        assert_eq!(latex_to_typst(r"\bigcup").trim(), "union.big");
        // T2L round-trips the new spelling; the deprecated `sect` still maps back
        // so older Typst documents keep converting.
        assert_eq!(typst_to_latex("$A inter B$").trim(), r"$A \cap B$");
        assert_eq!(typst_to_latex("$A sect B$").trim(), r"$A \cap B$");
    }

    #[test]
    fn test_relation_before_paren_keeps_space() {
        // Issue #34: a space before `(` is significant in Typst math. After a
        // relation symbol it must be preserved, otherwise `\sim (b)` collapses
        // into the accent call `tilde(b)` (i.e. `\tilde{b}`), changing meaning.
        assert_eq!(latex_to_typst(r"$(a) \sim (b)$").trim(), "$(a) tilde (b)$");
        assert_eq!(latex_to_typst(r"$A \cap (B)$").trim(), "$A inter (B)$");
        // Genuine function application by a lone identifier/number still glues.
        assert_eq!(latex_to_typst(r"$f(x)$").trim(), "$f(x)$");
        assert_eq!(latex_to_typst(r"$f  (x)$").trim(), "$f(x)$");
        assert_eq!(
            latex_to_typst(r"$\underbrace{f(x,y)}_{c}$").trim(),
            "$underbrace(f(x,y), c)$"
        );
        // The accent call itself is emitted glued and stays that way.
        assert_eq!(latex_to_typst(r"$\tilde{b}$").trim(), "$tilde(b)$");
    }

    #[test]
    fn test_sized_vertical_delimiter_pairs_preserve_semantics() {
        // OCR corpus cases used `\big|x\big|` for absolute value. Preserve
        // the fact that these bars came from a size command until their pair is
        // known, rather than globally treating every `bar.v` as abs().
        assert_eq!(latex_to_typst(r"\big|x\big|").trim(), "abs(x)");
        assert_eq!(latex_to_typst(r"\Big\|x\Big\|").trim(), "norm(x)");
        assert_eq!(
            latex_to_typst(r"\big|x, y\big|").trim(),
            "abs({x, y})",
            "a comma stays inside the absolute-value operand"
        );

        // A matrix between paired bars is a determinant, not abs(mat(...)).
        let determinant = latex_to_typst(r"\big|\begin{matrix}a & b \\ c & d\end{matrix}\big|");
        assert!(
            determinant.contains("mat(delim: \"|\"") && !determinant.contains("abs(mat("),
            "sized matrix bars must stay matrix delimiters, got: {determinant}"
        );

        // Ordinary and one-sided bars use existing delimiter semantics; this
        // change must not promote either into an absolute-value call.
        assert_eq!(latex_to_typst(r"a \vert b").trim(), "a bar.v b");
        assert_eq!(
            latex_to_typst(r"\big|x").trim(),
            "bar.v x",
            "an unmatched sized bar must retain delimiter spacing"
        );
        assert!(
            latex_to_typst(r"\left|x\right.").contains("lr(bar.v x)"),
            "a legal null-right delimiter must remain one-sided"
        );
        assert!(
            !latex_document_to_typst(
                r"\documentclass{article}\begin{document}\big|x\end{document}"
            )
            .contains('\u{1f}'),
            "a math-only delimiter in permissive text input must not leak an internal marker"
        );
        let ambiguous_adjacent = latex_to_typst(r"\big|\big|x\big|\big|");
        assert!(
            !ambiguous_adjacent.contains("abs(zws)"),
            "adjacent bars must not be greedily paired into empty absolute values: {ambiguous_adjacent}"
        );
        assert!(
            !ambiguous_adjacent.contains('\u{1f}'),
            "the conservative fallback must resolve all internal markers: {ambiguous_adjacent}"
        );

        // A blank line ends a document paragraph and cannot be part of one
        // valid math expression. Markers must therefore never pair across it.
        // Test both public conversion paths because document mode runs only the
        // marker-pairing pass, while math mode runs the full math cleanup.
        let across_paragraph_math = latex_to_typst("a \\big| b\n\nc \\big| d");
        let across_paragraph_document = latex_document_to_typst_with_options(
            "a \\big| b\n\nc \\big| d",
            &L2TOptions {
                preamble: PreambleMode::None,
                ..Default::default()
            },
        );
        for output in [&across_paragraph_math, &across_paragraph_document] {
            assert!(
                !output.contains("abs("),
                "sized bars must not pair across paragraphs: {output}"
            );
            assert!(
                output.contains("bar.v") && !output.contains('\u{1f}'),
                "each unmatched bar must fall back without leaking markers: {output}"
            );
        }
    }

    #[test]
    fn test_plain_tex_cal_is_a_scoped_math_declaration() {
        // Unlike `\mathcal{...}`, plain TeX's `\cal` styles the remaining
        // expression in its current group. The group bounds the declaration.
        assert_eq!(latex_to_typst(r"{\cal Z}").trim(), "cal(Z)");
        assert_eq!(latex_to_typst(r"{\cal A B}").trim(), "cal(A B)");
        assert_eq!(latex_to_typst(r"{\cal Z}_n").trim(), "cal(Z)_(n)");
        assert_eq!(latex_to_typst(r"\mathcal{Z}").trim(), "cal(Z)");
    }

    #[test]
    fn test_overrightarrow_uses_arrow_accent() {
        // Issue #35: Typst has no `overrightarrow`/`overleftarrow` functions, so
        // the old fall-through emitted invalid Typst that failed to compile. The
        // arrow accents are `arrow` / `arrow.l` / `arrow.l.r`. `\vec` shares the
        // rightwards arrow with `\overrightarrow`.
        assert_eq!(latex_to_typst(r"$\vec{n}$").trim(), "$arrow(n)$");
        assert_eq!(
            latex_to_typst(r"$\overrightarrow{PC}$").trim(),
            "$arrow(P C)$"
        );
        assert_eq!(
            latex_to_typst(r"$\overleftarrow{AB}$").trim(),
            "$arrow.l(A B)$"
        );
        assert_eq!(
            latex_to_typst(r"$\overleftrightarrow{AB}$").trim(),
            "$arrow.l.r(A B)$"
        );
        assert_eq!(
            latex_to_typst(r"$\overrightarrow{AE} = \frac{2}{5}\overrightarrow{AD}$").trim(),
            "$arrow(A E) = 2/5arrow(A D)$"
        );
        // Round-trips: `arrow`/`arrow.l`/`arrow.l.r` map back to the over-arrows.
        assert_eq!(
            typst_to_latex(r"$arrow(P C)$").trim(),
            r"$\overrightarrow{P C}$"
        );
        assert_eq!(
            typst_to_latex(r"$arrow.l(A B)$").trim(),
            r"$\overleftarrow{A B}$"
        );
        assert_eq!(
            typst_to_latex(r"$arrow.l.r(A B)$").trim(),
            r"$\overleftrightarrow{A B}$"
        );
    }

    #[test]
    fn test_tex_style_switches_and_fonts_do_not_leak_invalid_typst() {
        // Corpus root cause ③: the unknown-command fallback emitted `name(args)`
        // or a bare alias, producing Typst that fails to compile ("unknown
        // variable: scriptstyle", `pmb`, `varDelta`, `textcircled`).

        // The script-size switches have no Typst equivalent, so retain their
        // contents without leaking an unknown identifier.
        let s = latex_to_typst(r"$\scriptstyle (0,+\infty)$");
        assert!(!s.contains("scriptstyle"), "leaked scriptstyle: {s}");
        assert!(s.contains("infinity"), "content dropped: {s}");

        let s = latex_to_typst(r"$a_{\scriptscriptstyle 1}=1$");
        assert!(!s.contains("scriptscriptstyle"), "leaked sss: {s}");
        assert!(s.contains("a_"), "subscript content lost: {s}");

        // `\displaystyle` is a declaration over its remaining TeX group, not a
        // command with one argument. It must become Typst's `display(...)` so
        // the larger display style is preserved rather than silently discarded.
        assert_eq!(
            latex_to_typst(r"$\displaystyle x + y$").trim(),
            "$display(x + y)$"
        );
        assert_eq!(
            latex_to_typst(r"${\displaystyle x + y} + z$").trim(),
            "$display(x + y) + z$"
        );
        assert_eq!(
            latex_to_typst(r"$\displaystyle x + \textstyle y$").trim(),
            "$display(x + inline(y))$"
        );
        assert_eq!(
            latex_to_typst(r"\begin{equation}\displaystyle x + y\end{equation}").trim(),
            "$ display(x + y) $"
        );

        // Regression for issue #42. The style declaration is immediately
        // followed by an attached command expression, which mitex stores as a
        // sibling rather than as a required argument of `\displaystyle`.
        let display_sum = latex_document_to_typst(
            r"\documentclass{article}\begin{document}\begin{equation}
              y = \frac{1}{\displaystyle\sum_{k=1}^{N}P_k}
              \end{equation}\end{document}",
        );
        assert!(
            display_sum.contains("frac(1, display(sum_(k = 1)^(N)P_(k)))"),
            "displaystyle must preserve display style in the reported formula, got: {display_sum}"
        );

        // \pmb (poor man's bold) joins \boldsymbol / \bm -> bold().
        let s = latex_to_typst(r"$\pmb{d}$");
        assert!(s.contains("bold(d)"), "pmb content lost: {s}");

        // Slanted capital Greek aliases to the plain capital.
        let s = latex_to_typst(r"$\varDelta+\varOmega$");
        assert!(
            s.contains("Delta") && s.contains("Omega") && !s.contains("var"),
            "var-capital leaked: {s}"
        );

        // \textcircled keeps its inner content instead of leaking the identifier.
        let s = latex_to_typst(r"$\textcircled{\cdot}A$");
        assert!(!s.contains("textcircled"), "leaked textcircled: {s}");
        assert!(s.contains("dot") && s.contains('A'), "content lost: {s}");
    }

    #[test]
    fn test_baseless_and_nested_attachments_are_repaired() {
        // Corpus root cause ②: attachments (`_`/`^`) with no base produced
        // Typst that fails to compile ("unexpected underscore"/"unexpected hat").

        // Nested empty-base subscript `V_{_{M-ABF}}` (an OCR double-subscript)
        // collapses to a single subscript rather than the invalid `V_(_(...))`.
        let s = latex_to_typst(r"$V _ { _ { M - A B F } }$");
        assert!(!s.contains("_(_("), "double subscript not collapsed: {s}");
        assert!(s.contains("V_(M - A B F)"), "collapsed form wrong: {s}");

        // A base-less fragment gets an empty base `""` inserted.
        let s = latex_to_typst(r"$^ { a , b }$");
        assert!(s.starts_with(r#"$""^("#), "no empty base inserted: {s}");

        let s = latex_to_typst(r"$_ { 1 - { \sqrt { 3 } } }$");
        assert!(
            s.contains(r#""" _("#) || s.contains(r#"""_("#),
            "leading sub: {s}"
        );

        // A leading inner attachment inside a group also gets the empty base,
        // while a following attachment that already has a base is left alone.
        let s = latex_to_typst(r"$S _ { _ { \triangle U } _ { V } }$");
        assert!(s.contains(r#"_(""_("#), "inner base-less not repaired: {s}");

        // Ordinary attachments with a real base must be untouched.
        let s = latex_to_typst(r"$a _ { 1 } + b ^ { 2 }$");
        assert!(!s.contains("\"\""), "empty base wrongly inserted: {s}");
        assert!(s.contains("a_(1)") && s.contains("b^(2)"), "regressed: {s}");
    }

    #[test]
    fn test_dirac_notation_uses_chevron_delimiters() {
        // Issue #29: bra-ket notation emitted `angle.l`/`angle.r`, which are not
        // valid Typst delimiter symbols; the angle brackets ⟨ ⟩ are `chevron.l`
        // / `chevron.r` (matching how `\langle`/`\rangle` already convert).
        assert_eq!(latex_to_typst(r"\ket{x}").trim(), "lr(| x chevron.r)");
        assert_eq!(latex_to_typst(r"\bra{x}").trim(), "lr(chevron.l x |)");
        assert_eq!(
            latex_to_typst(r"\braket{a}{b}").trim(),
            "lr(chevron.l a | b chevron.r)"
        );
        assert_eq!(
            latex_to_typst(r"\braket{x}").trim(),
            "lr(chevron.l x | x chevron.r)"
        );
        // The shared delimiter fix also covers the other braket-family commands.
        assert_eq!(
            latex_to_typst(r"\mel{n}{A}{m}").trim(),
            "lr(chevron.l n | A | m chevron.r)"
        );
        // `\dyad` builds the closing bra from a *second* `lr(` inside the same
        // format string; exact-match here guards the space before the operand
        // (a bare `contains` check would miss `chevron.lb`).
        assert_eq!(
            latex_to_typst(r"\dyad{a}{b}").trim(),
            "lr(| a chevron.r) lr(chevron.l b |)"
        );
    }

    #[test]
    fn test_spacing_commands_consume_dimension_arguments() {
        // hspace/vspace and their starred variants must be registered as 1-arg
        // commands so mitex consumes the dimension instead of leaking it
        // (e.g. "#h()1 e m A" instead of "#h(1em)A").
        assert!(latex_to_typst(r"\hspace{1em}A").contains("#h(1em)"));
        assert!(latex_to_typst(r"\hspace*{3em}C").contains("#h(3em)"));
        assert!(latex_to_typst(r"\vspace{1em}A").contains("#v(1em)"));
        assert!(latex_to_typst(r"\vspace*{3em}C").contains("#v(3em)"));
    }

    #[test]
    fn test_epsilon_variants_map_to_correct_typst_glyphs() {
        // LaTeX \epsilon is the lunate form (Typst epsilon.alt); \varepsilon is
        // the curly form (Typst epsilon). The mappings were previously swapped.
        assert_eq!(latex_to_typst(r"\epsilon").trim(), "epsilon.alt");
        assert_eq!(latex_to_typst(r"\varepsilon").trim(), "epsilon");
    }

    #[test]
    fn test_empty_required_args_are_zws_padded() {
        // Empty groups must be padded with a zero-width space so the surrounding
        // Typst construct stays valid: `frac(zws, zws)` not the invalid
        // `frac(,)`, `sqrt(zws)` not `sqrt()`.
        assert_eq!(latex_to_typst(r"\frac{}{}").trim(), "zws/zws");
        assert_eq!(latex_to_typst(r"\sqrt{}").trim(), "sqrt(zws)");
        assert!(latex_to_typst(r"\overset{}{=}").contains("zws"));
    }

    #[test]
    fn test_text_in_math() {
        let result = latex_to_typst(r"\text{hello}");
        assert_eq!(result.trim(), "#text[hello]");

        let issue_label = latex_to_typst(r"\underbrace{U_p}_{\text{$p$th Layer}}");
        assert_eq!(
            issue_label.trim(),
            r#"underbrace(U_(p), #text[$p$th Layer])"#
        );
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
    fn test_epsilon_variants_map_to_correct_latex_commands() {
        // Inverse of the L2T mapping: Typst epsilon (curly) -> \varepsilon,
        // Typst epsilon.alt (lunate) -> \epsilon. Locks the round-trip.
        assert_eq!(typst_to_latex("$epsilon$").trim(), r"$\varepsilon$");
        assert_eq!(typst_to_latex("$epsilon.alt$").trim(), r"$\epsilon$");
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

    /// Issue #45: `\section*{T}` binds the `*` as the command's single term
    /// argument, so the heading was named `*` and the real title was demoted to
    /// body text — losing it from the outline and the table of contents.
    #[test]
    fn starred_sections_are_unnumbered_headings_in_an_article() {
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\section{Numbered section}\n\\section*{Unnumbered section}\n\
             \\subsection*{Unnumbered subsection}\n\\end{document}\n",
        );
        assert!(
            out.contains("#heading(level: 1, numbering: none)[Unnumbered section]"),
            "a starred section must be an unnumbered heading, got:\n{out}"
        );
        assert!(
            out.contains("#heading(level: 2, numbering: none)[Unnumbered subsection]"),
            "a starred subsection must keep its depth, got:\n{out}"
        );
        // The star must not survive anywhere: as a heading name it both broke
        // the build and hid the title.
        assert!(
            !out.contains("= \\*") && !out.contains("= *"),
            "the star must not become a heading, got:\n{out}"
        );
        // The numbered form is untouched.
        assert!(
            out.contains("= Numbered section"),
            "a plain section must still use the shorthand, got:\n{out}"
        );
        // The original symptom was a build failure: Typst read the lone `*` as
        // an unclosed strong-emphasis delimiter.
        assert_compiles_with_real_typst(&out);
    }

    /// Compile a converted document with the real `typst` binary. Skipped, with
    /// a note, when `typst` is not installed.
    fn assert_compiles_with_real_typst(typst_source: &str) {
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let available = Command::new("typst")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !available {
            eprintln!("skipping typst compile check: `typst` not on PATH");
            return;
        }

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let source = std::env::temp_dir().join(format!("tylax-heading-{nonce}.typ"));
        std::fs::write(&source, typst_source).expect("typst source should be written");

        let output = Command::new("typst")
            .arg("compile")
            .arg(&source)
            .arg("--format")
            .arg("pdf")
            .arg("-")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .expect("failed to run typst compile");

        let succeeded = output.status.success();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&source);

        assert!(
            succeeded,
            "the converted document must compile:\n{typst_source}\n--- typst said ---\n{stderr}"
        );
    }

    /// Brackets the author typed are literal text. They used to be dropped
    /// wholesale in markup mode, which hid every unconsumed optional argument
    /// but also silently deleted real content.
    #[test]
    fn literal_brackets_in_body_text_are_preserved() {
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             Literal [brackets] and a range [1,2] stay visible.\n\\end{document}\n",
        );
        assert!(
            out.contains("Literal [brackets] and a range [1,2] stay visible."),
            "author-typed brackets must survive, got:\n{out}"
        );
        assert_compiles_with_real_typst(&out);
    }

    #[test]
    fn optional_arguments_are_consumed_not_leaked_as_brackets() {
        // The counterpart: now that brackets are literal, every optional
        // argument must actually be consumed by its command's grammar, or it
        // would show up as stray text.
        let out = latex_document_to_typst(
            "\\documentclass[12pt,a4paper]{article}\n\\usepackage[utf8]{inputenc}\n\
             \\begin{document}\n\
             \\begin{figure}[htbp]\n\\caption[Short cap]{Long cap}\n\\end{figure}\n\
             \\begin{enumerate}[label=(\\alph*)]\n\\item First\n\\end{enumerate}\n\
             \\end{document}\n",
        );
        for leaked in ["12pt", "a4paper", "utf8", "htbp", "Short cap", "label="] {
            assert!(
                !out.contains(leaked),
                "optional argument `{leaked}` must be consumed, got:\n{out}"
            );
        }
        assert!(
            out.contains("caption: [Long cap]"),
            "the real caption must survive, got:\n{out}"
        );
        assert!(
            out.contains("First"),
            "the list item must survive, got:\n{out}"
        );
        assert_compiles_with_real_typst(&out);
    }

    /// Environment headers are described by a per-environment signature in the
    /// command spec, so mixed, repeated and out-of-order optional/required
    /// slots all parse. A "skip one leading bracket" rule could not express any
    /// of these.
    #[test]
    fn minipage_reads_its_width_past_every_optional_slot() {
        // `\begin{minipage}[pos][height][inner-pos]{width}` — all three
        // optional slots, plus the shorter spellings LaTeX also accepts.
        for header in ["[t][2cm][c]{3cm}", "[t][2cm]{3cm}", "[t]{3cm}", "{3cm}"] {
            let out = latex_document_to_typst(&format!(
                "\\documentclass{{article}}\n\\begin{{document}}\n\
                 \\begin{{minipage}}{header}\nMini body\n\\end{{minipage}}\n\\end{{document}}\n"
            ));
            assert!(
                out.contains("#block(width: 3cm)"),
                "the width is the required slot after the optional ones, got \
                 for `{header}`:\n{out}"
            );
            for slot in ["[t]", "2cm]", "[c]"] {
                assert!(
                    !out.contains(slot),
                    "header slot `{slot}` leaked for `{header}`, got:\n{out}"
                );
            }
            assert!(
                out.contains("Mini body"),
                "body must survive for `{header}`, got:\n{out}"
            );
        }
    }

    /// `maps.rs` is generated from `tools/gen_maps.py`, so an environment
    /// signature added to only one of them is a latent regression: regenerating
    /// would silently drop it. (The generator currently refuses to overwrite a
    /// diverged `maps.rs`, which is exactly why the two must be kept in step by
    /// hand.)
    #[test]
    fn environment_signatures_match_the_generator() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let generator = std::fs::read_to_string(root.join("tools/gen_maps.py"))
            .expect("tools/gen_maps.py should be readable");
        let maps = std::fs::read_to_string(root.join("src/data/maps.rs"))
            .expect("src/data/maps.rs should be readable");

        // `ENVIRONMENT_SIGNATURES = { "name": "pattern", ... }`
        let table = generator
            .split_once("ENVIRONMENT_SIGNATURES = {")
            .and_then(|(_, rest)| rest.split_once("\n}"))
            .map(|(body, _)| body)
            .expect("ENVIRONMENT_SIGNATURES table should be present");
        // Collect the quoted strings line by line; they alternate name,
        // pattern. Splitting on `,` would not work — the patterns contain one.
        let mut from_generator: Vec<(String, String)> = Vec::new();
        for line in table.lines() {
            let line = line.split('#').next().unwrap_or(line);
            let quoted: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
            for pair in quoted.chunks(2) {
                if let [name, pattern] = pair {
                    from_generator.push((name.to_string(), pattern.to_string()));
                }
            }
        }
        assert!(
            from_generator.len() > 10,
            "failed to parse the generator table, got: {from_generator:?}"
        );

        let mut from_maps: Vec<(String, String)> = Vec::new();
        for chunk in maps.split("m.insert(\"").skip(1) {
            let Some((name, rest)) = chunk.split_once("\".to_string(), ") else {
                continue;
            };
            if !rest.starts_with("CommandSpecItem::Env(") {
                continue;
            }
            // `aligned` is the one environment with no argument pattern.
            if let Some((_, after)) = rest.split_once("GlobStr::from(\"") {
                if let Some((pattern, _)) = after.split_once('"') {
                    from_maps.push((name.to_string(), pattern.to_string()));
                }
            }
        }

        from_generator.sort();
        from_maps.sort();
        assert_eq!(
            from_generator, from_maps,
            "tools/gen_maps.py and src/data/maps.rs disagree about environment signatures"
        );
    }

    #[test]
    fn starred_and_variant_environments_share_their_signature() {
        // A converter that handles a variant (`multicols*`, `longtabu`) is not
        // enough: the variant needs its own header signature, or the header
        // leaks into the body.
        let multicols = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{multicols*}{2}[Header]\nCol body\n\\end{multicols*}\n\\end{document}\n",
        );
        assert!(
            multicols.contains("#columns(2)") && !multicols.contains("[Header]"),
            "multicols* must consume its header, got:\n{multicols}"
        );

        let longtabu = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{longtabu}[t]{cc}\na & b \\\\\n\\end{longtabu}\n\\end{document}\n",
        );
        assert!(
            longtabu.contains("columns: (auto, auto)") && !longtabu.contains("[t]"),
            "longtabu must consume its position argument, got:\n{longtabu}"
        );
    }

    #[test]
    fn multicols_reads_a_required_slot_before_its_optional_one() {
        // `\begin{multicols}{2}[Header]` — required first, optional second.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{multicols}{2}[Header]\nCol body\n\\end{multicols}\n\\end{document}\n",
        );
        assert!(
            out.contains("#columns(2)"),
            "the column count must be read, got:\n{out}"
        );
        assert!(
            out.contains("Header") && !out.contains("[Header]"),
            "the header spans the columns, it is not a literal bracket, got:\n{out}"
        );
        assert!(out.contains("Col body"), "body must survive, got:\n{out}");
    }

    #[test]
    fn lstlisting_option_is_consumed_by_the_raw_extraction_path() {
        // `lstlisting` takes the raw-source route, which bypasses the body scan
        // entirely — only a header signature can keep `[language=..]` out of
        // the code, and it also supplies the language.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{lstlisting}[language=Python]\nx = 1\n\\end{lstlisting}\n\\end{document}\n",
        );
        assert!(
            !out.contains("language=Python"),
            "the option must not land in the code, got:\n{out}"
        );
        assert!(
            out.contains("```python"),
            "the consumed option should still select the language, got:\n{out}"
        );
        assert!(out.contains("x = 1"), "the code must survive, got:\n{out}");
    }

    #[test]
    fn tabular_position_argument_does_not_disturb_the_column_spec() {
        // `\begin{tabular}[t]{cc}` — the column spec is the LAST required slot,
        // which also holds for `tabular*{width}[pos]{cols}`.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{tabular}[t]{cc}\na & b \\\\\n\\end{tabular}\n\\end{document}\n",
        );
        assert!(
            out.contains("columns: (auto, auto)"),
            "both columns must be detected, got:\n{out}"
        );
        assert!(
            !out.contains("[t]") && !out.contains("cc a"),
            "the header must not leak into the cells, got:\n{out}"
        );
    }

    #[test]
    fn an_environment_without_an_optional_argument_keeps_its_brackets() {
        // `center` declares no optional argument, so LaTeX prints `[x]`. Only
        // environments that really take one may swallow a leading bracket.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{center}\n[x] stays text\n\\end{center}\n\\end{document}\n",
        );
        assert!(
            out.contains("[x] stays text"),
            "a non-option bracket must stay text, got:\n{out}"
        );
    }

    #[test]
    fn sectioning_commands_accept_their_optional_short_title() {
        // A fixed arity of one term cannot consume `\section[short]{long}`: the
        // parser bound NO argument at all, so the heading vanished and both the
        // short and the long title ran together as body text. The optional
        // argument belongs in the command's grammar.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\section[Short toc]{Long title}\nBody.\n\\end{document}\n",
        );
        assert!(
            out.contains("= Long title"),
            "the heading must survive an optional short title, got:\n{out}"
        );
        assert!(
            !out.contains("Short toc"),
            "the short title is a table-of-contents entry, not body text, got:\n{out}"
        );
        assert!(
            out.contains("Body."),
            "the following paragraph must be intact, got:\n{out}"
        );
        assert_compiles_with_real_typst(&out);
    }

    #[test]
    fn optional_short_title_works_for_every_sectioning_depth() {
        let out = latex_document_to_typst(
            "\\documentclass{book}\n\\begin{document}\n\
             \\chapter[C]{Chapter title}\n\\section[S]{Section title}\n\
             \\subsection[Sub]{Subsection title}\n\\end{document}\n",
        );
        for title in ["Chapter title", "Section title", "Subsection title"] {
            assert!(out.contains(title), "{title} must survive, got:\n{out}");
        }
        assert!(
            out.contains("= Chapter title") && out.contains("== Section title"),
            "depths must be unchanged, got:\n{out}"
        );
    }

    #[test]
    fn starred_section_survives_a_comment_before_its_title() {
        // TeX skips whitespace AND `%` comments before the argument it scans
        // for, so this is the same command as `\section*{After comment}`.
        // Treating the comment as anything but trivia dropped the pending state
        // and demoted the title to body text.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\section* % comment\n{After comment}\nBody.\n\\end{document}\n",
        );
        assert!(
            out.contains("#heading(level: 1, numbering: none)[After comment]"),
            "a comment between `*` and the title must be trivia, got:\n{out}"
        );
        assert!(
            out.contains("Body."),
            "the following paragraph must survive, got:\n{out}"
        );
        assert_compiles_with_real_typst(&out);
    }

    #[test]
    fn starred_sections_follow_the_document_class_depth() {
        // In book/report, `\chapter` is depth 1 and `\section` depth 2; the
        // starred forms must land on the same levels as their plain twins.
        let out = latex_document_to_typst(
            "\\documentclass{book}\n\\begin{document}\n\
             \\chapter{Numbered chapter}\n\\chapter*{Unnumbered chapter}\n\
             \\section*{Unnumbered section}\n\\subsection*{Unnumbered subsection}\n\
             \\end{document}\n",
        );
        assert!(
            out.contains("= Numbered chapter")
                && out.contains("#heading(level: 1, numbering: none)[Unnumbered chapter]"),
            "a starred chapter must match its plain depth, got:\n{out}"
        );
        assert!(
            out.contains("#heading(level: 2, numbering: none)[Unnumbered section]"),
            "book `\\section*` is depth 2, got:\n{out}"
        );
        assert!(
            out.contains("#heading(level: 3, numbering: none)[Unnumbered subsection]"),
            "book `\\subsection*` is depth 3, got:\n{out}"
        );
    }

    #[test]
    fn every_starred_sectioning_form_keeps_its_title() {
        // The star is bound as the argument for every one of these, so each
        // needs the same treatment — and `\part*`/`\subparagraph*` keep the
        // layout of their unstarred twins rather than becoming plain headings.
        let out = latex_document_to_typst(
            "\\documentclass{book}\n\\begin{document}\n\
             \\part*{PartTitle}\n\\subsubsection*{SubsubTitle}\n\
             \\paragraph*{ParaTitle}\n\\subparagraph*{SubparaTitle}\n\\end{document}\n",
        );
        for title in ["PartTitle", "SubsubTitle", "ParaTitle", "SubparaTitle"] {
            assert!(out.contains(title), "{title} must survive, got:\n{out}");
        }
        assert!(
            !out.contains('*'),
            "no star may leak into the output, got:\n{out}"
        );
        // `\part*` keeps the centred part layout but drops the "Part N" line.
        assert!(
            out.contains("#text(2em, weight: \"bold\")[PartTitle]") && !out.contains("Part I"),
            "a starred part must not be numbered, got:\n{out}"
        );
        assert!(
            out.contains("_SubparaTitle_"),
            "a starred subparagraph stays run-in italics, got:\n{out}"
        );
    }

    #[test]
    fn numbered_headings_are_unchanged_by_the_starred_handling() {
        // Guard the common path: nothing about plain sectioning may change.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\section{One}\nBody text follows.\n\\subsection{Two}\n\
             \\subsubsection{Three}\n\\end{document}\n",
        );
        assert!(
            out.contains("= One") && out.contains("== Two") && out.contains("=== Three"),
            "plain headings must keep the shorthand, got:\n{out}"
        );
        assert!(
            !out.contains("#heading("),
            "no plain heading should need the explicit element, got:\n{out}"
        );
        // A greedy argument pattern would have eaten the "B" of "Body".
        assert!(
            out.contains("Body text follows."),
            "the following paragraph must be intact, got:\n{out}"
        );
    }

    #[test]
    fn eqref_keeps_the_authors_word_and_drops_typsts_supplement() {
        // Issue #43: `@eq-b` renders "Equation 2", so keeping the "equation" the
        // author wrote gives "equation Equation 2". `\eqref` renders a bare
        // number in LaTeX, so the automatic supplement is the one to suppress.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\
             See equation~\\eqref{eq:b}.\n\
             \\begin{align}\nb &= 2 \\label{eq:b}\n\\end{align}\n\\end{document}\n",
        );
        // The parentheses belong to `\eqref` itself and must be literal: a
        // supplement-less `#ref` renders the bare number even under
        // `#set math.equation(numbering: "(1)")`.
        assert!(
            out.contains("equation (#ref(<eq-b>, supplement: none))"),
            "expected `equation (<ref>)` with literal parentheses, got:\n{out}"
        );
        assert!(
            !out.contains("equation @eq-b"),
            "`@` would re-insert the supplement, got:\n{out}"
        );
    }

    #[test]
    fn eqref_targets_the_label_the_document_actually_carries() {
        // Issue #43: label names are author-chosen, so nothing may be inferred
        // from them. Prefixing equation targets with `eq-` made `\label{e}`
        // emit `<e>` while `\eqref{e}` emitted `<eq-e>`, and Typst rejected the
        // document with "label `<eq-e>` does not exist".
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\
             See equation~\\eqref{e}.\n\
             \\begin{align}\nb &= 2 \\label{e}\n\\end{align}\n\\end{document}\n",
        );
        assert!(
            out.contains("#ref(<e>, supplement: none)"),
            "the reference must target the emitted label, got:\n{out}"
        );
        assert!(
            !out.contains("eq-e"),
            "no `eq-` prefix may be invented, got:\n{out}"
        );
        assert!(
            out.contains("<e>"),
            "the label must be emitted, got:\n{out}"
        );

        // A name that already sanitizes to `eq-...` is unaffected.
        let colon = latex_document_to_typst(
            "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\
             See equation~\\eqref{eq:b}.\n\
             \\begin{align}\nb &= 2 \\label{eq:b}\n\\end{align}\n\\end{document}\n",
        );
        assert!(
            colon.contains("#ref(<eq-b>, supplement: none)") && colon.contains("<eq-b>"),
            "a `eq:`-style name must still line up, got:\n{colon}"
        );
    }

    #[test]
    fn plain_ref_still_drops_the_duplicated_supplement() {
        // The other half of the same decision: `\ref` DOES take Typst's
        // supplement, so the word the author wrote must be dropped instead.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\section{First}\n\\label{sec:first}\n\
             See Section~\\ref{sec:first}.\n\\end{document}\n",
        );
        assert!(
            out.contains("See @sec-first"),
            "the duplicated word must be dropped, got:\n{out}"
        );
    }

    #[test]
    fn nested_lists_are_indented_not_flattened() {
        // Typst nests lists by indentation, so a sublist must be written two
        // spaces further in; otherwise both levels render as siblings.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{itemize}\n  \\item Top\n  \\begin{itemize}\n\
             \\item Nested\n  \\end{itemize}\n\\end{itemize}\n\\end{document}\n",
        );
        assert!(
            out.contains("- Top") && out.contains("  - Nested"),
            "the sublist must be indented under its parent, got:\n{out}"
        );
    }

    /// Issue #41: TeX discards a `%` comment before scanning the optional
    /// row-spacing argument of `\\`, so `\\% note<newline>[6pt]` is the very
    /// same row break as `\\[6pt]`. These run through the FULL-DOCUMENT path —
    /// the existing coverage used the math-only entry point, which is a
    /// different pipeline.
    #[test]
    fn matrix_row_spacing_survives_a_comment_separator() {
        let commented = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\\[\n\\begin{pmatrix}\n\
             a & b \\\\% row spacing comment\n[6pt] c & d\n\\end{pmatrix}\n\\]\n\
             \\end{document}\n",
        );
        assert!(
            !commented.contains("6 p t") && !commented.contains("6pt"),
            "row spacing must not leak into the matrix, got:\n{commented}"
        );
        assert!(
            commented.contains("a, b ; c, d"),
            "rows must stay clean and separated, got:\n{commented}"
        );

        // The strongest form of the invariant: a comment separator changes
        // nothing at all versus the plain spelling.
        let plain = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\\[\n\\begin{pmatrix}\n\
             a & b \\\\[6pt]\nc & d\n\\end{pmatrix}\n\\]\n\
             \\end{document}\n",
        );
        assert_eq!(
            commented, plain,
            "the comment-separated form must convert identically"
        );
    }

    #[test]
    fn align_and_cases_row_spacing_survive_a_comment_separator() {
        let align = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\\begin{align}\n\
             a &= b \\\\% note\n[6pt] c &= d\n\\end{align}\n\\end{document}\n",
        );
        assert!(
            !align.contains("6 p t") && !align.contains("6pt"),
            "align row spacing must not leak, got:\n{align}"
        );

        let cases = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\[ f = \\begin{cases} a & x>0 \\\\% note\n\
             [6pt] b & x\\le 0 \\end{cases} \\]\n\\end{document}\n",
        );
        assert!(
            !cases.contains("6 p t") && !cases.contains("6pt"),
            "cases row spacing must not leak, got:\n{cases}"
        );
    }

    #[test]
    fn non_dimension_bracket_after_a_comment_is_still_content() {
        // The consumption stays conservative: only a real length is dropped, so
        // genuine bracketed content survives the comment separator too.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\\[\n\\begin{pmatrix}\n\
             a \\\\% note\n[x] b\n\\end{pmatrix}\n\\]\n\\end{document}\n",
        );
        assert!(
            out.contains("[x"),
            "a non-dimension bracket must be kept, got:\n{out}"
        );
    }

    /// Issue #39: a `figure` body used to be scanned for `\includegraphics`
    /// only, so every other kind of content was replaced by an empty `[]`
    /// placeholder and silently lost from the document.
    #[test]
    fn booktabs_table_disables_the_default_grid_and_keeps_partial_rules() {
        // Issue #43: LaTeX draws only the rules the source asks for, while
        // Typst's `#table` defaults to a full grid; and `\cmidrule(lr){3-4}`
        // spans columns 3-4, not the whole width.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\usepackage{booktabs}\n\\begin{document}\n\
             \\begin{tabular}{lccc}\n\\toprule\nName & A & B & C \\\\\n\\midrule\n\
             \\cmidrule(lr){3-4}\nx & 1 & 2 & 3 \\\\\n\\bottomrule\n\
             \\end{tabular}\n\\end{document}\n",
        );
        assert!(
            out.contains("stroke: none"),
            "an explicitly ruled table must switch off the default grid, got:\n{out}"
        );
        assert!(
            out.contains("table.hline(start: 2, end: 4)"),
            "`\\cmidrule(lr){{3-4}}` must span only columns 3-4, got:\n{out}"
        );
        // The `\midrule` next to it is a separate, full-width rule.
        assert!(
            out.contains("table.hline(),"),
            "the full-width rules must survive too, got:\n{out}"
        );
    }

    #[test]
    fn column_spec_vertical_rules_survive_the_grid_being_switched_off() {
        // Issue #43: `|` separators are real borders. Once a ruled table turns
        // Typst's default grid off, nothing else would draw them, so they have
        // to be re-emitted from the column specification.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{tabular}{|c|c|}\n\\hline\na & b \\\\\n\\hline\n\
             \\end{tabular}\n\\end{document}\n",
        );
        assert!(
            out.contains("stroke: none"),
            "an explicitly ruled table must switch off the default grid, got:\n{out}"
        );
        for boundary in [
            "table.vline(x: 0)",
            "table.vline(x: 1)",
            "table.vline(x: 2)",
        ] {
            assert!(
                out.contains(boundary),
                "missing {boundary} — the left/middle/right rules of `{{|c|c|}}` \
                 must all survive, got:\n{out}"
            );
        }
        assert!(
            out.contains("table.hline()"),
            "the horizontal rules must still be there, got:\n{out}"
        );
    }

    #[test]
    fn double_vertical_rule_is_downgraded_with_a_diagnostic() {
        // `||` is a legal double rule. Typst's `table.vline` has no double-line
        // stroke, so it is drawn as a single rule — but that downgrade must be
        // reported, not silent.
        let source = "\\documentclass{article}\n\\begin{document}\n\
                      \\begin{tabular}{||c||c||}\n\\hline\na & b \\\\\n\\hline\n\
                      \\end{tabular}\n\\end{document}\n";
        let reported = tylax::latex_to_typst_with_diagnostics(source);
        assert!(
            reported
                .warnings
                .iter()
                .any(|w| w.message.contains("Double vertical rule")),
            "the downgrade must be reported, got: {:?}",
            reported.warnings
        );

        let out = latex_document_to_typst(source);
        assert!(
            out.contains("table.vline(x: 0)") && out.contains("table.vline(x: 2)"),
            "the boundaries must still be drawn, got:\n{out}"
        );
    }

    #[test]
    fn single_vertical_rules_report_nothing() {
        // The converse: a plain `|` is drawn exactly and must not warn.
        let reported = tylax::latex_to_typst_with_diagnostics(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{tabular}{|c|c|}\n\\hline\na & b \\\\\n\\end{tabular}\n\\end{document}\n",
        );
        assert!(
            !reported
                .warnings
                .iter()
                .any(|w| w.message.contains("Double vertical rule")),
            "a single rule must not be reported as a downgrade, got: {:?}",
            reported.warnings
        );
    }

    #[test]
    fn column_spec_without_vertical_rules_adds_none() {
        // The converse: a spec with no `|` must not gain borders.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{tabular}{cc}\n\\hline\na & b \\\\\n\\end{tabular}\n\\end{document}\n",
        );
        assert!(
            !out.contains("table.vline"),
            "no vertical rule may be invented, got:\n{out}"
        );
    }

    #[test]
    fn vertical_rules_alone_still_switch_off_the_default_grid() {
        // `{|c|c|}` with no `\hline` draws only vertical rules in LaTeX, so the
        // default grid must go even though no horizontal rule was declared.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{tabular}{|c|c|}\na & b \\\\\n\\end{tabular}\n\\end{document}\n",
        );
        assert!(
            out.contains("stroke: none") && out.contains("table.vline(x: 0)"),
            "vertical rules alone must be honoured, got:\n{out}"
        );
        assert!(
            !out.contains("table.hline"),
            "no horizontal rule was declared, got:\n{out}"
        );
    }

    #[test]
    fn cline_range_is_preserved() {
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\begin{document}\n\
             \\begin{tabular}{lll}\na & b & c \\\\\n\\cline{2-3}\nd & e & f \\\\\n\
             \\end{tabular}\n\\end{document}\n",
        );
        assert!(
            out.contains("table.hline(start: 1, end: 3)"),
            "`\\cline{{2-3}}` must span only columns 2-3, got:\n{out}"
        );
    }

    #[test]
    fn figure_keeps_a_tikz_picture() {
        let latex = "\\documentclass{article}\n\\begin{document}\n\
                     \\begin{figure}\n\\centering\n\
                     \\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\
                     \\caption{A schematic}\n\\end{figure}\n\\end{document}\n";
        let out = latex_document_to_typst(latex);
        assert!(
            out.contains("canvas") && out.contains("line((0, 0), (1, 1))"),
            "the picture must survive inside a figure, got:\n{out}"
        );
        assert!(
            out.contains("caption: [A schematic]"),
            "the caption must still be lifted out, got:\n{out}"
        );
    }

    #[test]
    fn figure_keeps_non_image_content() {
        let latex = "\\documentclass{article}\n\\begin{document}\n\
                     \\begin{figure}\nSome explanatory prose.\n\
                     \\begin{tabular}{ll}\na & b \\\\\n\\end{tabular}\n\
                     \\caption{A tabular inside a figure}\n\\end{figure}\n\\end{document}\n";
        let out = latex_document_to_typst(latex);
        assert!(
            out.contains("Some explanatory prose."),
            "prose must not be dropped, got:\n{out}"
        );
        assert!(
            out.contains("#table("),
            "a tabular must not be dropped, got:\n{out}"
        );
    }

    #[test]
    fn figure_keeps_images_nested_in_an_unknown_command() {
        // Issue #44: `\subfloat` (subfig) is not a command the converter knows,
        // so its images survive only because the figure body is now walked
        // recursively rather than scanned for a top-level `\includegraphics`.
        // That makes this case a guard on the GENERAL rule: if unknown-command
        // handling is ever tightened, images must not silently vanish again.
        //
        // The subfigure layout itself is deliberately not reproduced — the
        // contract here is "nothing is lost and the result compiles", not
        // faithful subfloat typesetting.
        let out = latex_document_to_typst(
            "\\documentclass{article}\n\\usepackage{graphicx}\n\\usepackage{subfig}\n\
             \\begin{document}\n\\begin{figure}\n\\centering\n\
             \\subfloat[One]{\\includegraphics[width=0.4\\textwidth]{foto.png}}\n\
             \\subfloat[Two]{\\includegraphics[width=0.4\\textwidth]{otra.png}}\n\
             \\caption{With subfloat}\n\\end{figure}\n\\end{document}\n",
        );

        assert_eq!(
            out.matches("#image(").count(),
            2,
            "both subfloat images must survive, got:\n{out}"
        );
        assert!(
            out.contains("foto.png") && out.contains("otra.png"),
            "both image paths must survive, got:\n{out}"
        );
        assert!(
            !out.contains("[],"),
            "the figure body must not fall back to the empty placeholder, got:\n{out}"
        );
        assert!(
            out.contains("caption: [With subfloat]"),
            "the caption must still be lifted out, got:\n{out}"
        );
        // The sub-captions are kept as body text rather than dropped.
        assert!(
            out.contains("One") && out.contains("Two"),
            "sub-captions must not be lost, got:\n{out}"
        );

        compile_subfloat_figure_with_real_typst();
    }

    /// Compile the issue #44 figure with the real `typst` binary, against real
    /// image files, so the guarantee is "the images are there AND the document
    /// builds" rather than a string match. Skipped when `typst` is absent.
    fn compile_subfloat_figure_with_real_typst() {
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let available = Command::new("typst")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !available {
            eprintln!("skipping subfloat compile check: `typst` not on PATH");
            return;
        }

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tylax-subfloat-{nonce}"));
        std::fs::create_dir_all(&dir).expect("temp dir should be created");

        // A real image from the repository, so `#image(..)` resolves for real.
        let logo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/logo.svg");
        for name in ["foto.svg", "otra.svg"] {
            std::fs::copy(&logo, dir.join(name)).expect("test image should be copied");
        }

        let typst = latex_document_to_typst(
            "\\documentclass{article}\n\\usepackage{graphicx}\n\\usepackage{subfig}\n\
             \\begin{document}\n\\begin{figure}\n\\centering\n\
             \\subfloat[One]{\\includegraphics[width=0.4\\textwidth]{foto.svg}}\n\
             \\subfloat[Two]{\\includegraphics[width=0.4\\textwidth]{otra.svg}}\n\
             \\caption{With subfloat}\n\\end{figure}\n\\end{document}\n",
        );
        let source = dir.join("doc.typ");
        std::fs::write(&source, &typst).expect("typst source should be written");

        let output = Command::new("typst")
            .arg("compile")
            .arg(&source)
            .arg("--format")
            .arg("pdf")
            .arg("-")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .expect("failed to run typst compile");

        let succeeded = output.status.success();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            succeeded,
            "the converted subfloat figure must compile:\n{typst}\n--- typst said ---\n{stderr}"
        );
    }

    #[test]
    fn figure_keeps_every_image_with_its_options() {
        // Two images previously became two positional `#figure` arguments,
        // which Typst rejects outright, and their sizes were discarded.
        let latex = "\\documentclass{article}\n\\begin{document}\n\
                     \\begin{figure}\n\
                     \\includegraphics[width=3cm]{a.png}\n\
                     \\includegraphics[width=4cm]{b.png}\n\
                     \\caption{Two images}\n\\label{fig:two}\n\\end{figure}\n\\end{document}\n";
        let out = latex_document_to_typst(latex);
        assert!(
            out.contains("a.png") && out.contains("b.png"),
            "both images must be kept, got:\n{out}"
        );
        assert!(
            out.contains("width: 3cm") && out.contains("width: 4cm"),
            "image options must be preserved, got:\n{out}"
        );
        assert!(
            out.contains("<fig-two>"),
            "the label must still be lifted out, got:\n{out}"
        );
    }

    #[test]
    fn figure_without_content_still_emits_a_placeholder() {
        // The conservative fallback is unchanged for a genuinely empty body.
        let latex = "\\documentclass{article}\n\\begin{document}\n\
                     \\begin{figure}\n\\caption{Only a caption}\n\\end{figure}\n\
                     \\end{document}\n";
        let out = latex_document_to_typst(latex);
        assert!(
            out.contains("[],"),
            "an empty figure body should keep its placeholder, got:\n{out}"
        );
    }

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
    fn test_documentclass_with_optional_arg_reads_class_not_options() {
        // `\documentclass[12pt]{book}`: the `[12pt]` is an optional argument, not
        // the class. The class must be read from the following `{book}` so heading
        // levels are correct — book's `\section` is a depth-2 `==` heading, whereas
        // a misdetected class would collapse it to a top-level `=`.
        let book = latex_document_to_typst(
            "\\documentclass[12pt]{book}\n\\begin{document}\n\\section{First}\n\\end{document}",
        );
        assert!(
            book.contains("== First"),
            "book's \\section must be `== First` (depth 2), got:\n{book}"
        );

        let memoir = latex_document_to_typst(
            "\\documentclass[11pt,openany]{memoir}\n\\begin{document}\n\\section{First}\n\\end{document}",
        );
        assert!(
            memoir.contains("== First"),
            "memoir's \\section must be `== First` (depth 2), got:\n{memoir}"
        );

        // article stays a top-level `=` even with options present.
        let article = latex_document_to_typst(
            "\\documentclass[a4paper,12pt]{article}\n\\begin{document}\n\\section{First}\n\\end{document}",
        );
        assert!(
            article.contains("= First") && !article.contains("== First"),
            "article's \\section must be a top-level `= First`, got:\n{article}"
        );

        // A TeX comment between the optional argument and the class group is an
        // ignorable separator: `\documentclass[12pt]% note\n{book}` must still read
        // `book` as the class, not abort the scan at the comment.
        let commented = latex_document_to_typst(
            "\\documentclass[12pt]% pick a class\n{book}\n\\begin{document}\n\\section{First}\n\\end{document}",
        );
        assert!(
            commented.contains("== First"),
            "comment between options and class must not hide book's depth-2 \\section, got:\n{commented}"
        );
    }

    #[test]
    fn test_begin_document_with_separator_enters_document_mode() {
        // TeX ignores whitespace and `%` comments between the `\begin` control word
        // and its `{document}` argument, so `\begin {document}` and
        // `\begin% c\n{document}` are real document starts. They must enter document
        // mode (preamble consumed) rather than leaking `\documentclass` as body.
        let spaced = latex_document_to_typst(
            "\\documentclass{article}\n\\begin {document}\n\\section{First}\n\\end{document}",
        );
        assert!(
            spaced.contains("= First") && !spaced.contains("documentclass"),
            "`\\begin {{document}}` must enter document mode, got:\n{spaced}"
        );

        let commented = latex_document_to_typst(
            "\\documentclass{article}\n\\begin% start\n{document}\n\\section{First}\n\\end{document}",
        );
        assert!(
            commented.contains("= First") && !commented.contains("documentclass"),
            "comment-separated `\\begin{{document}}` must enter document mode, got:\n{commented}"
        );
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

    #[test]
    fn test_document_multirow_with_non_ascii_content() {
        // Issue #36: this formerly panicked while parsing the generated
        // `table.cell(...)[...]` marker because a byte offset was mixed with a
        // character index. Keep the public document path covered, not just the
        // table parser's internal marker format.
        let latex = r#"
\documentclass{article}
\usepackage{multirow}
\begin{document}
\begin{tabular}{ll}
\multirow{2}{*}{Guc katı} & A \\
                          & B \\
\end{tabular}
\end{document}
"#;

        let result = latex_document_to_typst(latex);
        assert!(
            result.contains("table.cell(rowspan: 2)[Guc katı]"),
            "non-ASCII multirow content must remain intact, got:\n{result}"
        );
        assert!(
            result.contains("[A]") && result.contains("[B]"),
            "multirow sibling cells must remain intact, got:\n{result}"
        );
    }

    #[test]
    fn test_description_environment_term_list_colons() {
        // Issue #32: the `description` environment must emit Typst term-list
        // items (`/ term: text`). A labelled `\item[term]` needs the bracket
        // captured as an argument; a bare `\item` needs an empty term (`/ :`)
        // so the output still parses as a term list instead of erroring.
        let latex = r"\documentclass{article}
\begin{document}
\begin{description}
   \item This is an entry \textit{without} a label.
   \item[Something short] A short one-line description.
\end{description}
\end{document}";

        let result = latex_document_to_typst(latex);
        assert!(
            result.contains("/ : This is an entry _without_ a label."),
            "unlabelled item should become `/ : ...`, got:\n{}",
            result
        );
        assert!(
            result.contains("/ Something short: A short one-line description."),
            "labelled item should become `/ term: ...`, got:\n{}",
            result
        );
    }

    #[test]
    fn test_item_optional_labels_remain_visible_outside_description() {
        // `\item[...]` is parsed globally so description lists can read their
        // term. In itemize/enumerate the optional label is user-visible content,
        // so keep the old fallback shape instead of silently dropping it.
        let itemize = latex_document_to_typst(
            r"\documentclass{article}\begin{document}\begin{itemize}\item[--] custom marker\end{itemize}\end{document}",
        );
        assert!(
            itemize.contains("- -- custom marker"),
            "itemize optional label should stay visible, got:\n{}",
            itemize
        );

        let enumerate = latex_document_to_typst(
            r"\documentclass{article}\begin{document}\begin{enumerate}\item[(a)] custom enum\end{enumerate}\end{document}",
        );
        assert!(
            enumerate.contains("+ (a) custom enum"),
            "enumerate optional label should stay visible, got:\n{}",
            enumerate
        );
    }

    #[test]
    fn test_item_label_with_math_and_commands_is_converted() {
        // Issue #32 follow-up: a list-item label may itself contain LaTeX (math
        // or text commands). It must be converted through the full pipeline, not
        // emitted raw (`$O(n)$`) nor mangled by brace-stripping (`\textbf{X}`
        // must not collapse to an empty/garbled term).
        let latex = r"\documentclass{article}
\begin{document}
\begin{description}
   \item[$O(n)$] linear time.
   \item[\textbf{Bold}] a bold term.
\end{description}
\end{document}";
        let result = latex_document_to_typst(latex);
        assert!(
            result.contains("/ $O(n)$: linear time."),
            "math label should convert to inline Typst math, got:\n{}",
            result
        );
        assert!(
            result.contains("/ *Bold*: a bold term."),
            "command label should convert (not be stripped to empty), got:\n{}",
            result
        );
    }

    #[test]
    fn test_issue_43_full_document_fixes() {
        // Issue #43 bundled four document-mode regressions. This pins all of them
        // against one document so a fix to one can't silently break another.
        let latex = r"\documentclass{article}
\usepackage{amsmath}
\begin{document}
\section{First}
\label{sec:first}
\subsection{Sub}
See Section~\ref{sec:first} and equation~\eqref{eq:b}.
\begin{align}
a &= 1 \label{eq:a}\\
b &= 2 \label{eq:b}
\end{align}
\begin{itemize}
  \item Top
  \begin{itemize}
    \item Nested
  \end{itemize}
\end{itemize}
\end{document}";
        let result = latex_document_to_typst(latex);

        // (1) Heading levels: article's `\section` is a top-level `=` heading and
        //     `\subsection` is `==` (not both collapsed to one level).
        assert!(
            result.contains("= First") && result.contains("== Sub"),
            "section/subsection should map to `=`/`==`, got:\n{}",
            result
        );

        // (2) `Section~\ref{...}` must not become "Section @sec-first": Typst's
        //     `@ref` re-inserts the supplement, so the authored word is dropped.
        assert!(
            result.contains("@sec-first") && !result.contains("Section @sec-first"),
            "ref supplement word should be stripped, got:\n{}",
            result
        );

        // (3) Per-row `\label` inside align: each row becomes its own block
        //     equation with a real, resolvable Typst label. An inline `#<..>`
        //     marker inside math does NOT create a label (Typst errors with
        //     "label does not exist"), so the rows must be split apart.
        assert!(
            result.contains("$ a & = 1 $ <eq-a>") && result.contains("$ b & = 2 $ <eq-b>"),
            "each align row should become a separately-labelled equation, got:\n{}",
            result
        );
        assert!(
            !result.contains("#<eq-"),
            "no inline `#<..>` label markers should survive (they don't compile), got:\n{}",
            result
        );
        assert!(
            result.contains("Multiple labelled rows in a LaTeX alignment"),
            "the unavoidable multi-label alignment downgrade must be explicit, got:\n{}",
            result
        );

        // (4) Nested itemize indents one level (`  - Nested`) under its parent
        //     item, with the outer list flush at column 0.
        assert!(
            result.contains("- Top") && result.contains("  - Nested"),
            "nested list should indent by two spaces, got:\n{}",
            result
        );
    }

    #[test]
    fn test_equation_family_labels_are_outside_math() {
        // `gather` supports a label on each row while `multline` has one
        // equation label. Neither may leave Typst's invalid inline `#<..>`
        // marker inside `$...$`.
        let gather = latex_to_typst(r"\begin{gather}a=1\label{eq:a}\\b=2\label{eq:b}\end{gather}");
        assert!(gather.contains("$ a = 1 $ <eq-a>"), "got: {gather}");
        assert!(gather.contains("$ b = 2 $ <eq-b>"), "got: {gather}");
        assert!(!gather.contains("#<eq-"), "got: {gather}");

        let multline = latex_to_typst(r"\begin{multline}a=1\label{eq:m}\end{multline}");
        assert!(multline.contains("$ a = 1 $ <eq-m>"), "got: {multline}");
        assert!(!multline.contains("#<eq-"), "got: {multline}");

        let nested = latex_to_typst(
            r"\begin{equation}\begin{aligned}a&=1\label{eq:nested}\end{aligned}\end{equation}",
        );
        assert!(
            nested.contains("<eq-nested>") && !nested.contains("#<eq-nested>"),
            "a nested label must attach to its enclosing equation, got: {nested}"
        );
    }

    #[test]
    fn test_single_label_align_preserves_its_row_layout() {
        // A single label belongs to the whole align block, so Typst can retain
        // its row structure instead of needlessly splitting it into unrelated
        // equations. Only independently labelled rows require the fallback.
        let aligned = latex_to_typst(r"\begin{align}a&=1\\b&=2\label{eq:last}\end{align}");
        assert!(aligned.contains("<eq-last>"), "got: {aligned}");
        assert!(
            !aligned.contains("#<eq-last>"),
            "the label must remain outside math, got: {aligned}"
        );
        assert_eq!(
            aligned.matches('$').count(),
            2,
            "one labelled align block must remain one math block, got: {aligned}"
        );
        assert!(
            aligned.contains("a & = 1") && aligned.contains("b & = 2"),
            "both rows must remain in the preserved block, got: {aligned}"
        );
        assert_compiles_with_real_typst(&aligned);
    }

    #[test]
    fn test_issue_40_siunitx_units_survive() {
        // Issue #40: siunitx units were silently dropped in full-document mode
        // because mitex emits `\SI`'s braces as following curly siblings, not
        // child clauses. The value AND the mapped unit must both survive.
        let latex = r"\documentclass{article}
\usepackage{siunitx}
\begin{document}
Inductance is \SI{47}{\micro\henry} and frequency \SI{500}{\kilo\hertz}.
\end{document}";
        let result = latex_document_to_typst(latex);
        assert!(
            result.contains(r#"$47 space "μH"$"#),
            "\\SI value and prefixed unit should both survive, got:\n{}",
            result
        );
        assert!(
            result.contains(r#"$500 space "kHz"$"#),
            "\\SI kilo-hertz should map to kHz, got:\n{}",
            result
        );
    }

    #[test]
    fn test_issue_40_siunitx_single_and_derived_units() {
        // `\si{unit}` (unit only), `\num`, `\unit`, `\ang`, and `\qty` with a
        // `\per`/`\squared` compound unit must all convert rather than vanish.
        let latex = r"\documentclass{article}
\usepackage{siunitx}
\begin{document}
Mass \si{\kilogram}, count \num{1000}, speed \unit{\metre\per\second}, angle \ang{45}, accel \qty{9.8}{\metre\per\second\squared}.
\end{document}";
        let result = latex_document_to_typst(latex);
        for expected in [
            r#"$"kg"$"#,
            "$1000$",
            r#"$"m/s"$"#,
            "$45°$",
            r#"$9.8 space "m/s²"$"#,
        ] {
            assert!(
                result.contains(expected),
                "expected {:?} in output, got:\n{}",
                expected,
                result
            );
        }
    }

    #[test]
    fn test_issue_40_siunitx_does_not_nest_math_delimiters() {
        // siunitx is valid in both text and math mode. In an existing formula,
        // it must emit math content directly rather than a nested `$...$`.
        assert_eq!(
            latex_to_typst(r"$\SI{47}{\metre}$").trim(),
            "$47 space \"m\"$"
        );
        assert_eq!(
            latex_to_typst(r"$\qty{9.8}{\metre\per\second}$").trim(),
            "$9.8 space \"m/s\"$"
        );

        // siunitx options affect number formatting, not the value/unit
        // semantics represented by Typst. They must not interrupt collection
        // of the required braced arguments.
        assert_eq!(
            latex_to_typst(r"$\SI[round-mode=places]{47}{\metre}$").trim(),
            "$47 space \"m\"$"
        );
    }

    #[test]
    fn test_siunitx_comment_between_args_still_binds_unit() {
        // A TeX comment (`%` line comment or `\iffalse..\fi` block comment)
        // between `\SI`'s value and unit groups is whitespace to the parser: it
        // must not abort argument collection and drop the unit.
        let line_comment = "\\documentclass{article}\n\\usepackage{siunitx}\n\\begin{document}\nX \\SI{47}% note\n{\\metre}.\n\\end{document}";
        assert!(
            latex_document_to_typst(line_comment).contains(r#"$47 space "m"$"#),
            "line comment between \\SI args must not drop the unit, got:\n{}",
            latex_document_to_typst(line_comment)
        );

        let block_comment = "\\documentclass{article}\n\\usepackage{siunitx}\n\\begin{document}\nX \\SI{47}\\iffalse note \\fi{\\metre}.\n\\end{document}";
        assert!(
            latex_document_to_typst(block_comment).contains(r#"$47 space "m"$"#),
            "block comment between \\SI args must not drop the unit, got:\n{}",
            latex_document_to_typst(block_comment)
        );

        // Math-mode inline form goes through the same collection path.
        assert_eq!(
            latex_to_typst("$\\SI{47}% note\n{\\metre}$").trim(),
            "$47 space \"m\"$"
        );
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
    use super::latex_document_to_typst;
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

    #[test]
    fn test_tikz_full_document_preamble_does_not_eat_first_command() {
        // Regression: when given a full LaTeX document, the picture body
        // must be extracted from inside \begin{tikzpicture}...\end{tikzpicture}.
        // Previously the preamble was concatenated with the first \draw,
        // causing the first \draw to be silently dropped.
        let tex = r"\documentclass[tikz,border=10pt]{standalone}
\usepackage{xcolor}
\begin{document}
\begin{tikzpicture}[font=\sffamily]
    \draw[->, >=stealth, thick, black] (0,0) -- (0,7.5) node[above=10pt, font=\Large\bfseries] {how long?};
    \draw[->, >=stealth, thick, black] (0.5,0) -- (8,0) node[right=10pt, font=\Large\bfseries] {for who?};
\end{tikzpicture}
\end{document}
";
        let cetz = convert_tikz_to_cetz(tex);
        assert!(
            cetz.contains("(0, 0)") && cetz.contains("(0, 7.5)"),
            "vertical axis (0,0)->(0,7.5) should survive, got: {}",
            cetz
        );
        assert!(
            cetz.contains("(0.5, 0)") && cetz.contains("(8, 0)"),
            "horizontal axis (0.5,0)->(8,0) should survive, got: {}",
            cetz
        );
        assert!(
            cetz.contains("how long?") && cetz.contains("for who?"),
            "both node labels should be present, got: {}",
            cetz
        );
        assert_eq!(
            cetz.matches("line(").count(),
            2,
            "two line() calls expected, got: {}",
            cetz
        );
        assert!(
            !cetz.contains(r"\documentclass") && !cetz.contains(r"\usepackage"),
            "preamble must not leak into output, got: {}",
            cetz
        );
    }

    #[test]
    fn test_tikz_picture_level_options_are_stripped() {
        // The optional [font=\sffamily] after \begin{tikzpicture} should be
        // removed and not interfere with command parsing.
        let tikz = r"\begin{tikzpicture}[font=\sffamily]\draw (0,0) -- (1,1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(
            cetz.contains("line(") && cetz.contains("(0, 0)") && cetz.contains("(1, 1)"),
            "drawing should survive picture-level options, got: {}",
            cetz
        );
        assert!(
            !cetz.contains("font") && !cetz.contains("sffamily"),
            "picture-level font option should not appear in output, got: {}",
            cetz
        );
    }

    #[test]
    fn test_commented_tikzpicture_stays_comment() {
        // The raw-block shield must only recognize real environments, never a
        // code sample or a commented-out drawing.
        let source =
            "% \\begin{tikzpicture}\n% \\draw (0,0) -- (1,1);\n% \\end{tikzpicture}\nVisible text.";
        let result = latex_document_to_typst(source);
        assert!(result.contains("Visible text."), "got: {result}");
        assert!(
            !result.contains("TikZ converted to CeTZ") && !result.contains("#canvas"),
            "commented TikZ must not become live CeTZ, got: {result}"
        );
    }

    #[test]
    fn test_verbatim_tikzpicture_stays_literal() {
        // A TikZ example shown inside a `verbatim` block is documentation, not a
        // drawing. Verbatim regions are shielded at the source level before any
        // LaTeX interpretation, so the example must survive byte-for-byte as a
        // Typst raw block — braces, `\begin`/`\end`, and `\draw` intact — never
        // a live CeTZ picture.
        let source = "\\documentclass{article}\n\\begin{document}\nExample:\n\\begin{verbatim}\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\\end{verbatim}\nDone.\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            !result.contains("TikZ converted to CeTZ") && !result.contains("#canvas"),
            "verbatim TikZ example must not become live CeTZ, got:\n{result}"
        );
        for literal in [
            r"\begin{tikzpicture}",
            r"\draw (0,0) -- (1,1);",
            r"\end{tikzpicture}",
        ] {
            assert!(
                result.contains(literal),
                "verbatim body must stay literal ({literal:?}), got:\n{result}"
            );
        }
        assert!(
            result.contains("```"),
            "verbatim body must be emitted as a Typst raw block, got:\n{result}"
        );
        assert!(
            result.contains("Done."),
            "surrounding prose survives, got:\n{result}"
        );
    }

    #[test]
    fn test_inline_verb_tikzpicture_stays_literal() {
        // A complete `tikzpicture` shown inside inline `\verb` is literal text.
        // The source-level shield turns it into Typst inline raw rather than
        // letting MiTeX (which has no `\verb` support) parse the body as a live
        // environment and emit an empty `#canvas`.
        let source = "\\documentclass{article}\n\\begin{document}\nInline: \\verb|\\begin{tikzpicture}\\draw (0,0)--(1,1);\\end{tikzpicture}| end.\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            !result.contains("#canvas"),
            "inline \\verb TikZ must not become CeTZ, got:\n{result}"
        );
        assert!(
            result.contains(r"`\begin{tikzpicture}\draw (0,0)--(1,1);\end{tikzpicture}`"),
            "inline \\verb body must survive as Typst inline raw, got:\n{result}"
        );
    }

    #[test]
    fn test_real_tikzpicture_still_renders_alongside_verbatim() {
        // The verbatim shield must not suppress a genuine drawing elsewhere in
        // the same document: real TikZ still converts to CeTZ.
        let source = "\\documentclass{article}\n\\begin{document}\n\\begin{verbatim}\n\\begin{tikzpicture} example \\end{tikzpicture}\n\\end{verbatim}\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            result.contains("#canvas") && result.contains("line((0, 0), (1, 1))"),
            "real TikZ after a verbatim example must still render, got:\n{result}"
        );
    }

    #[test]
    fn test_commented_verbatim_markers_do_not_protect_real_tikz() {
        // `% \begin{verbatim}` / `% \end{verbatim}` are commented out, so the
        // TikZ between them is a REAL drawing. A source-context-aware scan must
        // not treat the commented markers as a verbatim region and skip the
        // genuine picture.
        let source = "\\documentclass{article}\n\\begin{document}\n% \\begin{verbatim}\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n% \\end{verbatim}\nAfter.\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            result.contains("#canvas") && result.contains("line((0, 0), (1, 1))"),
            "real TikZ between commented-out verbatim markers must still render, got:\n{result}"
        );
    }

    #[test]
    fn test_spaced_begin_verbatim_shields_interior_tikz() {
        // TeX ignores whitespace after the `\begin`/`\end` control words, so
        // `\begin {verbatim}` / `\end {verbatim}` (with a space) is a valid
        // verbatim environment. The source-level shield must recognize it and
        // keep the interior TikZ literal — not let it become a live CeTZ.
        let source = "\\documentclass{article}\n\\begin{document}\n\\begin {verbatim}\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\\end {verbatim}\nDone.\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            !result.contains("TikZ converted to CeTZ") && !result.contains("#canvas"),
            "TikZ inside a spaced `\\begin {{verbatim}}` must stay literal, got:\n{result}"
        );
        for literal in [
            r"\begin{tikzpicture}",
            r"\draw (0,0) -- (1,1);",
            r"\end{tikzpicture}",
        ] {
            assert!(
                result.contains(literal),
                "spaced verbatim body must stay literal ({literal:?}), got:\n{result}"
            );
        }
    }

    #[test]
    fn test_fancyvrb_optional_arg_not_leaked_into_raw() {
        // fancyvrb `\begin{Verbatim}[numbers=left]` carries a `[key=val]`
        // optional argument that configures the environment; it is NOT body
        // content. The shield must drop it before emitting the Typst raw block.
        let source = "\\documentclass{article}\n\\begin{document}\n\\begin{Verbatim}[numbers=left]\nhello world\n\\end{Verbatim}\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            result.contains("hello world"),
            "Verbatim body must survive, got:\n{result}"
        );
        assert!(
            !result.contains("numbers=left") && !result.contains("[numbers=left]"),
            "fancyvrb optional argument must not leak into the raw block, got:\n{result}"
        );
    }

    #[test]
    fn test_commented_begin_verbatim_shields_interior_tikz() {
        // TeX discards a `%` comment (and its newline) while scanning a control
        // word's argument, so `\begin% note\n{verbatim}` / `\end% note\n{verbatim}`
        // is a valid verbatim environment. The shield must recognize it and keep
        // the interior TikZ literal rather than emitting live CeTZ.
        let source = "\\documentclass{article}\n\\begin{document}\n\\begin% open\n{verbatim}\n\\begin{tikzpicture}\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\\end% close\n{verbatim}\nDone.\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            !result.contains("TikZ converted to CeTZ") && !result.contains("#canvas"),
            "TikZ inside a comment-separated verbatim tag must stay literal, got:\n{result}"
        );
        for literal in [
            r"\begin{tikzpicture}",
            r"\draw (0,0) -- (1,1);",
            r"\end{tikzpicture}",
        ] {
            assert!(
                result.contains(literal),
                "comment-separated verbatim body must stay literal ({literal:?}), got:\n{result}"
            );
        }
    }

    #[test]
    fn test_fancyvrb_body_starting_with_bracket_is_preserved() {
        // fancyvrb reads the optional `[...]` only when it directly follows
        // `\begin{Verbatim}`. Here the tag is followed by a newline, so the
        // bracketed first line is literal body content and must NOT be deleted.
        let source = "\\documentclass{article}\n\\begin{document}\n\\begin{Verbatim}\n[first line is literal data]\nsecond line\n\\end{Verbatim}\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            result.contains("[first line is literal data]"),
            "a bracketed first body line must be preserved, got:\n{result}"
        );
        assert!(
            result.contains("second line"),
            "the rest of the Verbatim body must be preserved, got:\n{result}"
        );
    }

    #[test]
    fn test_fancyvrb_header_comment_then_optional_arg_is_config() {
        // Verified with TeX Live pdflatex: fancyvrb reads the `[key=val]` optional
        // argument in non-verbatim mode, so a `%` comment ending the
        // `\begin{Verbatim}` line is honored and the `[...]` on the next line is
        // configuration, not body. Only `code` is typeset.
        let source = "\\documentclass{article}\n\\begin{document}\n\\begin{Verbatim}% header comment\n[numbers=left]\ncode\n\\end{Verbatim}\n\\end{document}";
        let result = latex_document_to_typst(source);
        assert!(
            result.contains("code"),
            "Verbatim body must survive, got:\n{result}"
        );
        assert!(
            !result.contains("numbers=left")
                && !result.contains("[numbers=left]")
                && !result.contains("header comment"),
            "fancyvrb header comment and optional argument must not leak into the raw block, got:\n{result}"
        );
    }

    #[test]
    fn test_plain_verbatim_comment_and_bracket_stay_literal() {
        // Direct contrast to `test_fancyvrb_header_comment_then_optional_arg_is_config`:
        // the SAME header-comment shape (`% comment` ending the `\begin` line, then
        // a `[...]` line) must NOT be treated as a fancyvrb header for plain
        // `verbatim`. It takes no optional argument, so the `%` comment and `[...]`
        // are byte-for-byte literal body and must all survive.
        let source = "\\documentclass{article}\n\\begin{document}\n\\begin{verbatim}% header comment\n[numbers=left]\ncode\n\\end{verbatim}\n\\end{document}";
        let result = latex_document_to_typst(source);
        for literal in ["% header comment", "[numbers=left]", "code"] {
            assert!(
                result.contains(literal),
                "plain verbatim body must stay literal ({literal:?}), got:\n{result}"
            );
        }
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
        // `\eqref` renders "(2)" in LaTeX, so it must NOT pick up Typst's
        // automatic supplement — that would read "equation Equation 2" next to
        // the word the author already wrote (issue #43). The parentheses are
        // part of `\eqref` itself: a supplement-less `#ref` renders the bare
        // number even under `numbering: "(1)"`. The target is the label as
        // written — no `eq-` prefix is invented.
        assert_eq!(
            latex_to_typst(r#"\eqref{energy}"#).trim(),
            "(#ref(<energy>, supplement: none))"
        );
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
        // Typst's `#cite` takes one key, so multi-key groups split into separate
        // calls; the shared postnote attaches to the final citation.
        let citep = latex_document_to_typst(r#"See \citep[see][ch. 2]{a,b}."#);
        assert!(
            citep.contains(r#"See see #cite(<a>) #cite(<b>, supplement: [ch. 2])."#),
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
    fn test_lr_linebreak_stays_inline_fragment() {
        let result =
            typst_to_latex_with_options(r"$lr(chevron.l a \ b chevron.r)$", &T2LOptions::default());

        assert!(
            result.contains(r"\left\langle") && result.contains(r"\right\rangle"),
            "lr() should still render angle delimiters, got: {}",
            result
        );
        assert!(
            result.contains("a \\\n") && !result.contains("a \\\\\n"),
            "linebreak inside lr() content should stay an inline fragment, got: {}",
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
    fn test_fraction_grouping_parentheses_are_not_emitted() {
        let numerator = assert_t2l_paths_match("$(2 pi) / 3$");
        let denominator = assert_t2l_paths_match("$1 / (2 pi)$");
        let visible = assert_t2l_paths_match("$(2 pi) + 3$");

        assert!(
            numerator.contains(r"\frac{2 \pi}{3}"),
            "fraction grouping parentheses should be absorbed, got: {}",
            numerator
        );
        assert!(
            !numerator.contains(r"\frac{(2 \pi)}{3}"),
            "fraction grouping parentheses should not be emitted, got: {}",
            numerator
        );
        assert!(
            denominator.contains(r"\frac{1}{2 \pi}"),
            "denominator grouping parentheses should be absorbed, got: {}",
            denominator
        );
        assert!(
            visible.contains(r"\left(2 \pi\right) + 3"),
            "ordinary visible parentheses should remain emitted, got: {}",
            visible
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
    fn test_math_linebreak_in_align_becomes_double_backslash() {
        // A Typst math line break (`\`) at the row level of an aligned equation
        // must map to LaTeX's `\\` row separator; a lone `\` is not a valid row
        // break. (`\ `, backslash-space, is a Typst line break.)
        let result = typst_to_latex_with_options(r"$ a &= b \ &= c $", &T2LOptions::default());
        assert!(
            result.contains(r"\begin{align}"),
            "aligned equation should use the align environment, got: {}",
            result
        );
        assert!(
            result.contains("b \\\\") && !result.contains("b \\\n"),
            "row-level line break should emit `\\\\`, not a bare `\\`, got: {}",
            result
        );
    }

    #[test]
    fn test_non_aligned_math_linebreak_stays_inline_spacing() {
        // Without an alignment marker, the output stays in inline/display math
        // rather than an align environment. In that context, emitting `\\`
        // would create an invalid LaTeX row separator.
        let result = typst_to_latex_with_options(r"$ a = b \ c = d $", &T2LOptions::default());
        assert!(
            result.contains("b \\\n") && !result.contains("b \\\\\n"),
            "non-aligned math linebreak should stay a bare `\\`, got: {}",
            result
        );
    }

    #[test]
    fn test_text_ampersand_does_not_trigger_align_linebreak() {
        // Alignment detection must only consider top-level `&`; an ampersand
        // inside text/braces is literal content and should not upgrade a math
        // line break to a row separator.
        let result = typst_to_latex_with_options(r#"$ text("&") \ a $"#, &T2LOptions::default());
        assert!(
            !result.contains(r"\begin{align}"),
            "text ampersand should not create align environment, got: {}",
            result
        );
        assert!(
            result.contains(" \\\n") && !result.contains(" \\\\\n"),
            "text ampersand linebreak should stay a bare `\\`, got: {}",
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
    fn test_attached_primes_are_preserved() {
        let standalone = typst_to_latex_with_options("$'$", &T2LOptions::default());
        let single = typst_to_latex_with_options("$x'$", &T2LOptions::default());
        let double = typst_to_latex_with_options("$x''$", &T2LOptions::default());
        let with_subscript = typst_to_latex_with_options("$x'_i$", &T2LOptions::default());
        let with_superscript = typst_to_latex_with_options("$f'^2$", &T2LOptions::default());
        let subscript_prime = typst_to_latex_with_options("$x_i'$", &T2LOptions::default());
        let superscript_prime = typst_to_latex_with_options("$x^2'$", &T2LOptions::default());
        let applied = typst_to_latex_with_options("$f'(x)$", &T2LOptions::default());
        let consistent_subscript_prime = assert_t2l_paths_match("$x_i'$");
        let consistent_superscript_prime = assert_t2l_paths_match("$x^2'$");

        assert!(
            standalone.contains("'"),
            "standalone prime should be preserved, got: {}",
            standalone
        );
        assert!(
            single.contains("x'"),
            "attached single prime should be preserved, got: {}",
            single
        );
        assert!(
            double.contains("x''"),
            "attached double prime should be preserved, got: {}",
            double
        );
        assert!(
            with_subscript.contains("x'_i") || with_subscript.contains("x'_{i}"),
            "attached prime with subscript should be preserved, got: {}",
            with_subscript
        );
        assert!(
            with_superscript.contains("f'^2") || with_superscript.contains("f'^{2}"),
            "attached prime with superscript should be preserved, got: {}",
            with_superscript
        );
        assert!(
            subscript_prime.contains("i'"),
            "prime inside subscript should be preserved, got: {}",
            subscript_prime
        );
        assert!(
            superscript_prime.contains("2'"),
            "prime inside superscript should be preserved, got: {}",
            superscript_prime
        );
        assert!(
            consistent_subscript_prime.contains("i'"),
            "prime inside subscript should be consistent across paths, got: {}",
            consistent_subscript_prime
        );
        assert!(
            consistent_superscript_prime.contains("2'"),
            "prime inside superscript should be consistent across paths, got: {}",
            consistent_superscript_prime
        );
        assert!(
            applied.contains("f'"),
            "attached prime before function args should be preserved, got: {}",
            applied
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
            result.contains("lr(|") && result.contains("chevron.r"),
            "\\ket should produce lr(| ψ chevron.r), got: {}",
            result
        );
    }

    #[test]
    fn test_bra() {
        let result = latex_to_typst(r"\bra{\phi}");
        assert!(
            result.contains("chevron.l") && result.contains("|)"),
            "\\bra should produce lr(chevron.l φ |), got: {}",
            result
        );
    }

    #[test]
    fn test_braket_two_args() {
        let result = latex_to_typst(r"\braket{a}{b}");
        assert!(
            result.contains("chevron.l") && result.contains("|") && result.contains("chevron.r"),
            "\\braket{{a}}{{b}} should produce lr(chevron.l a | b chevron.r), got: {}",
            result
        );
    }

    #[test]
    fn test_braket_single_arg() {
        let result = latex_to_typst(r"\braket{a}");
        let output = result.trim();
        // Single-arg braket: ⟨a|a⟩
        assert!(
            output.contains("chevron.l") && output.contains("chevron.r"),
            "\\braket{{a}} should produce lr(chevron.l a | a chevron.r), got: {}",
            result
        );
    }

    #[test]
    fn test_expval_implicit() {
        let result = latex_to_typst(r"\expval{A}");
        assert!(
            result.contains("chevron.l") && result.contains("chevron.r"),
            "\\expval{{A}} should produce lr(chevron.l A chevron.r), got: {}",
            result
        );
    }

    #[test]
    fn test_expval_explicit() {
        let result = latex_to_typst(r"\expval{A}{\Psi}");
        eprintln!("expval result: {}", result);
        assert!(
            result.contains("chevron.l") && result.contains("|") && result.contains("chevron.r"),
            "\\expval{{A}}{{Ψ}} should produce lr(chevron.l Ψ | A | Ψ chevron.r), got: {}",
            result
        );
    }

    #[test]
    fn test_mel() {
        let result = latex_to_typst(r"\mel{n}{A}{m}");
        assert!(
            result.contains("chevron.l") && result.contains("|") && result.contains("chevron.r"),
            "\\mel{{n}}{{A}}{{m}} should produce lr(chevron.l n | A | m chevron.r), got: {}",
            result
        );
    }

    #[test]
    fn test_dyad() {
        let result = latex_to_typst(r"\dyad{a}{b}");
        eprintln!("dyad result: {}", result);
        // |a⟩⟨b|
        assert!(
            result.contains("chevron.r") && result.contains("chevron.l"),
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
            result.contains("chevron.l") || result.contains("lr(|"),
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
            result.contains("chevron.l") && result.contains("0") && result.contains("chevron.r"),
            "\\vev{{A}} should produce lr(chevron.l 0 | A | 0 chevron.r), got: {}",
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
            result.contains("chevron.l") && result.contains("chevron.r"),
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
        assert_eq!(TYPST_TO_TEX.get("times.o"), Some(&"\\otimes"));
        assert_eq!(TYPST_TO_TEX.get("plus.o"), Some(&"\\oplus"));
        assert_eq!(TYPST_TO_TEX.get("minus.o"), Some(&"\\ominus"));
        assert_eq!(TYPST_TO_TEX.get("dot.o"), Some(&"\\odot"));
        assert_eq!(TYPST_TO_TEX.get("slash.o"), Some(&"\\oslash"));
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

// ============================================================================
// Preamble / wrapper customization
// ============================================================================

mod preamble_customization {
    use tylax::{
        latex_document_to_typst_with_options, typst_to_latex_with_options, DocumentWrapperMode,
        L2TOptions, PreambleMode, T2LOptions,
    };

    fn sample_l2t_doc() -> &'static str {
        r"\documentclass{article}
\title{Sample}
\author{Alice}
\begin{document}
\section{Intro}
Hello.
\end{document}"
    }

    #[test]
    fn test_l2t_preamble_default_emits_set_rules() {
        let out = latex_document_to_typst_with_options(sample_l2t_doc(), &L2TOptions::default());
        assert!(out.contains("#set page(paper:"));
        assert!(out.contains("#set heading(numbering:"));
        assert!(out.contains("#set math.equation(numbering:"));
    }

    #[test]
    fn test_l2t_preamble_none_drops_set_rules_but_keeps_metadata_and_title() {
        let opts = L2TOptions {
            preamble: PreambleMode::None,
            ..Default::default()
        };
        let out = latex_document_to_typst_with_options(sample_l2t_doc(), &opts);
        assert!(
            !out.contains("#set page("),
            "page set rule should be dropped, got: {}",
            out
        );
        assert!(
            !out.contains("#set heading("),
            "heading set rule should be dropped, got: {}",
            out
        );
        assert!(
            !out.contains("#set math.equation("),
            "math.equation set rule should be dropped, got: {}",
            out
        );
        // metadata block must remain
        assert!(
            out.contains("#set document(") && out.contains("Sample") && out.contains("Alice"),
            "title metadata should survive PreambleMode::None, got: {}",
            out
        );
        // visible title block must remain
        assert!(
            out.contains("#align(center)["),
            "title block should survive PreambleMode::None, got: {}",
            out
        );
    }

    #[test]
    fn test_l2t_preamble_custom_replaces_default() {
        let opts = L2TOptions {
            preamble: PreambleMode::Custom("#set text(font: \"New Roman\")".to_string()),
            ..Default::default()
        };
        let out = latex_document_to_typst_with_options(sample_l2t_doc(), &opts);
        assert!(out.contains("#set text(font:"));
        assert!(
            !out.contains("#set page("),
            "default page set rule should be replaced, got: {}",
            out
        );
    }

    #[test]
    fn test_t2l_wrapper_default_emits_full_document() {
        let opts = T2LOptions::full_document();
        let out = typst_to_latex_with_options("= Hi\n\nbody", &opts);
        assert!(out.contains("\\documentclass{"));
        assert!(out.contains("\\usepackage{amsmath}"));
        assert!(out.contains("\\begin{document}"));
        assert!(out.contains("\\end{document}"));
    }

    #[test]
    fn test_t2l_wrapper_body_only_drops_documentclass_and_packages() {
        let opts = T2LOptions {
            full_document: true,
            wrapper: DocumentWrapperMode::BodyOnly,
            ..T2LOptions::full_document()
        };
        let out = typst_to_latex_with_options("= Hi\n\nbody", &opts);
        assert!(
            !out.contains("\\documentclass"),
            "BodyOnly should drop documentclass, got: {}",
            out
        );
        assert!(
            !out.contains("\\usepackage"),
            "BodyOnly should drop \\usepackage, got: {}",
            out
        );
        assert!(
            !out.contains("\\begin{document}") && !out.contains("\\end{document}"),
            "BodyOnly should drop document begin/end, got: {}",
            out
        );
        assert!(out.contains("\\section{"), "body must remain, got: {}", out);
    }

    #[test]
    fn test_t2l_wrapper_custom_inserts_body_at_placeholder() {
        let wrapper = DocumentWrapperMode::from_template(
            "\\documentclass{minimal}\n\\begin{document}\n{body}\n\\end{document}\n",
        )
        .expect("template should parse");
        let opts = T2LOptions {
            full_document: true,
            wrapper,
            ..T2LOptions::full_document()
        };
        let out = typst_to_latex_with_options("= Hi", &opts);
        assert!(out.starts_with("\\documentclass{minimal}"));
        assert!(out.contains("\\section{"));
        assert!(out.trim_end().ends_with("\\end{document}"));
        assert!(
            !out.contains("\\usepackage{amsmath}"),
            "custom wrapper should not include default packages, got: {}",
            out
        );
    }

    #[test]
    fn test_t2l_wrapper_template_missing_body_placeholder_errors() {
        let result = DocumentWrapperMode::from_template("\\documentclass{article}\nno placeholder");
        assert!(
            result.is_err(),
            "missing {{body}} placeholder must error, got: {:?}",
            result
        );
    }
}

/// Issue #37: reconcile `\cite` with the document's bibliography backend.
///
/// A manual `thebibliography` renders `<key>` anchors but no `#bibliography()`,
/// so `#cite(<key>)` fails to compile ("document does not contain a
/// bibliography"). Citations are emitted as deferred markers during the walk and
/// resolved once the backend is known: manual -> `@key`, external/none -> keep
/// `#cite(...)`, mixed -> keep `#cite(...)` plus a diagnostic.
mod l2t_citation_backend {
    use super::*;
    use tylax::latex_to_typst_with_diagnostics;

    const MARKER_START: char = '\u{E010}';
    const MARKER_END: char = '\u{E011}';

    fn assert_no_marker_leak(out: &str) {
        assert!(
            !out.contains(MARKER_START) && !out.contains(MARKER_END),
            "raw citation marker leaked into output:\n{out}"
        );
    }

    fn manual_doc(body: &str) -> String {
        format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\
             \\begin{{thebibliography}}{{9}}\n\
             \\bibitem{{knuth}} Knuth, D. The TeXbook.\n\
             \\bibitem{{lamport}} Lamport, L. LaTeX.\n\
             \\bibitem{{foo.bar}} Complex, K. Dotted key.\n\
             \\end{{thebibliography}}\n\\end{{document}}\n"
        )
    }

    #[test]
    fn manual_single_cite_becomes_at_ref() {
        let out = latex_document_to_typst(&manual_doc(r"See \cite{knuth}."));
        assert!(out.contains("@knuth"), "expected @knuth, got:\n{out}");
        assert!(
            !out.contains("#cite(<knuth>"),
            "manual bib must not keep #cite, got:\n{out}"
        );
        // The bib entry anchor `<knuth>` that `@knuth` targets must be present.
        assert!(out.contains("<knuth>"), "missing bib anchor, got:\n{out}");
        assert_no_marker_leak(&out);
    }

    #[test]
    fn manual_multi_cite_splits_into_separate_at_refs() {
        let out = latex_document_to_typst(&manual_doc(r"See \cite{knuth,lamport}."));
        assert!(
            out.contains("@knuth @lamport"),
            "expected `@knuth @lamport`, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn manual_postnote_kept_as_literal_after_single_key() {
        let out = latex_document_to_typst(&manual_doc(r"See \cite[p.~5]{knuth}."));
        assert!(
            out.contains("@knuth [p."),
            "postnote should follow the key literally, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn manual_postnote_emitted_once_after_multi_key() {
        let out = latex_document_to_typst(&manual_doc(r"See \cite[p.~5]{knuth,lamport}."));
        // `@knuth @lamport [p.~5]` — the postnote is attached once, after the
        // last key, not repeated per key.
        assert!(
            out.contains("@knuth @lamport [p."),
            "expected one trailing postnote, got:\n{out}"
        );
        assert_eq!(
            out.matches("[p.").count(),
            1,
            "postnote must appear exactly once, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn manual_prenote_prepended_once() {
        let out = latex_document_to_typst(&manual_doc(r"See \cite[see][p.~5]{knuth}."));
        assert!(
            out.contains("see @knuth [p."),
            "expected `see @knuth [p. ...]`, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn manual_dotted_key_falls_back_to_ref() {
        // `foo.bar` sanitizes to a label containing a dot, which is not a simple
        // `@key`, so it must degrade to `#ref(<foo.bar>)` — matching the anchor.
        let out = latex_document_to_typst(&manual_doc(r"See \cite{foo.bar}."));
        assert!(
            out.contains("#ref(<foo.bar>)"),
            "dotted key should use #ref, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn manual_author_year_mode_degrades_with_diagnostic() {
        for cmd in [r"\citet", r"\citeauthor", r"\citeyear", r"\citeyearpar"] {
            let out = latex_document_to_typst(&manual_doc(&format!("See {cmd}{{knuth}}.")));
            assert!(
                out.contains("@knuth"),
                "{cmd} should degrade to a label ref, got:\n{out}"
            );
            assert!(
                out.contains("// - ") && out.to_lowercase().contains("degraded"),
                "{cmd} under manual bib should emit a degradation diagnostic, got:\n{out}"
            );
            assert_no_marker_leak(&out);
        }
    }

    #[test]
    fn external_bibliography_keeps_cite() {
        let doc = "\\documentclass{article}\n\\begin{document}\n\
                   See \\cite{knuth}.\n\\bibliography{refs}\n\\end{document}\n";
        let out = latex_document_to_typst(doc);
        assert!(
            out.contains("#cite(<knuth>)"),
            "external bib must keep #cite, got:\n{out}"
        );
        assert!(
            !out.contains("@knuth"),
            "external bib must not rewrite to @knuth, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn external_addbibresource_keeps_cite() {
        let doc = "\\documentclass{article}\n\\addbibresource{refs.bib}\n\\begin{document}\n\
                   See \\cite{knuth,lamport}.\n\\printbibliography\n\\end{document}\n";
        let out = latex_document_to_typst(doc);
        assert!(
            out.contains("#cite(<knuth>)") && out.contains("#cite(<lamport>)"),
            "biblatex must keep #cite per key, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn no_bibliography_keeps_cite_unchanged() {
        // None backend: don't disturb (a bib may be added later, or this is a
        // stray fragment). Matches pre-issue-#37 behavior.
        let doc = "\\documentclass{article}\n\\begin{document}\n\
                   See \\cite{knuth}.\n\\end{document}\n";
        let out = latex_document_to_typst(doc);
        assert!(
            out.contains("#cite(<knuth>)") && !out.contains("@knuth"),
            "no-bib document must keep #cite, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn mixed_backend_keeps_cite_and_warns() {
        // Both a manual `thebibliography` and an external `\bibliography`: keep
        // the compilable external form and flag the ambiguity once.
        let doc = "\\documentclass{article}\n\\begin{document}\n\
                   See \\cite{knuth}.\n\\bibliography{refs}\n\
                   \\begin{thebibliography}{9}\n\
                   \\bibitem{knuth} Knuth, D.\n\\end{thebibliography}\n\\end{document}\n";
        let out = latex_document_to_typst(doc);
        assert!(
            out.contains("#cite(<knuth>)"),
            "mixed backend must keep #cite, got:\n{out}"
        );
        assert!(
            out.to_lowercase().contains("mixes a manual"),
            "mixed backend must emit a diagnostic, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    #[test]
    fn bibliographystyle_and_nocite_alone_stay_manual() {
        // `\bibliographystyle` and `\nocite` commonly accompany a manual bib and
        // must NOT flip the backend to External/Mixed.
        let doc = "\\documentclass{article}\n\\bibliographystyle{plain}\n\\begin{document}\n\
                   \\nocite{*}\nSee \\cite{knuth}.\n\
                   \\begin{thebibliography}{9}\n\
                   \\bibitem{knuth} Knuth, D.\n\\end{thebibliography}\n\\end{document}\n";
        let out = latex_document_to_typst(doc);
        assert!(
            out.contains("@knuth") && !out.contains("#cite(<knuth>)"),
            "style/nocite must not force External; expected manual @knuth, got:\n{out}"
        );
        assert!(
            !out.to_lowercase().contains("mixes a manual"),
            "must not be treated as Mixed, got:\n{out}"
        );
        assert_no_marker_leak(&out);
    }

    /// Collect the bibliography-backend diagnostics reported for a document.
    fn backend_warnings(doc: &str) -> Vec<String> {
        latex_to_typst_with_diagnostics(doc)
            .warnings
            .iter()
            .filter(|w| {
                let m = w.message.to_lowercase();
                m.contains("bibliograph")
            })
            .map(|w| w.message.clone())
            .collect()
    }

    #[test]
    fn external_bibliography_warns_even_without_any_citation() {
        // The backend diagnostic must not depend on citations: `\bibliography`
        // is dropped without emitting `#bibliography(...)` whether or not
        // anything cites it, so a document with zero `\cite` is still broken.
        let doc = "\\documentclass{article}\n\\begin{document}\n\
                   Body with no citation at all.\n\\bibliography{refs}\n\\end{document}\n";
        let warnings = backend_warnings(doc);
        assert_eq!(
            warnings.len(),
            1,
            "expected exactly one backend warning, got: {warnings:?}"
        );
        assert!(
            warnings[0].contains("#bibliography"),
            "warning should name the missing #bibliography, got: {warnings:?}"
        );
    }

    #[test]
    fn external_bibliography_warns_exactly_once_with_many_citations() {
        // One diagnostic per document, not per citation.
        let doc = "\\documentclass{article}\n\\begin{document}\n\
                   See \\cite{a}, \\cite{b} and \\cite{c}.\n\
                   \\bibliography{refs}\n\\end{document}\n";
        let warnings = backend_warnings(doc);
        assert_eq!(
            warnings.len(),
            1,
            "expected exactly one backend warning, got: {warnings:?}"
        );
    }

    #[test]
    fn mixed_backend_warns_even_without_any_citation() {
        // `Mixed` is covered by the same unconditional finalizer.
        let doc = "\\documentclass{article}\n\\begin{document}\n\
                   Body with no citation at all.\n\\bibliography{refs}\n\
                   \\begin{thebibliography}{9}\n\\bibitem{knuth} Knuth.\n\
                   \\end{thebibliography}\n\\end{document}\n";
        let warnings = backend_warnings(doc);
        assert_eq!(
            warnings.len(),
            1,
            "expected exactly one backend warning, got: {warnings:?}"
        );
        assert!(
            warnings[0].contains("mixes"),
            "expected the mixed-backend warning, got: {warnings:?}"
        );
    }

    #[test]
    fn manual_and_plain_documents_emit_no_backend_warning() {
        // No spurious noise on the paths that convert cleanly.
        assert!(
            backend_warnings(&manual_doc(r"See \cite{knuth}.")).is_empty(),
            "a fully reconciled manual bibliography must not warn"
        );
        let plain = "\\documentclass{article}\n\\begin{document}\nNo bibliography here.\n\
                     \\end{document}\n";
        assert!(
            backend_warnings(plain).is_empty(),
            "a document without any bibliography must not warn"
        );
    }

    #[test]
    fn structured_diagnostic_is_reported_via_api() {
        let out = latex_to_typst_with_diagnostics(&manual_doc(r"See \citet{knuth}."));
        assert!(
            out.warnings
                .iter()
                .any(|w| w.message.to_lowercase().contains("degraded")),
            "expected a structured bibliography-backend warning, got: {:?}",
            out.warnings
        );
    }

    #[test]
    fn converter_reuse_does_not_leak_the_backend_between_documents() {
        // The backend flags are the one piece of reuse state whose leak is
        // silent in the byte-identical test above (which pairs a manual doc
        // with a plain one). A leaked `saw_manual_bib` would classify a later
        // external-bib document as `Mixed`, changing its diagnostic — and, for
        // any future backend-dependent rendering, its citations too.
        let external = "\\documentclass{article}\n\\begin{document}\n\
                        See \\cite{knuth}.\n\\bibliography{refs}\n\\end{document}\n";
        let fresh = latex_document_to_typst(external);

        let mut reused = tylax::core::latex2typst::LatexConverter::new();
        let _ = reused.convert_document(&manual_doc(r"See \cite{knuth}."));
        let after_reuse = reused.convert_document(external);

        assert_eq!(
            after_reuse, fresh,
            "reused converter diverged:\n--- reused ---\n{after_reuse}\n--- fresh ---\n{fresh}"
        );
        assert!(
            after_reuse.contains("External bibliography"),
            "second document should be classified External, got:\n{after_reuse}"
        );
        assert!(
            !after_reuse.contains("mixes"),
            "a leaked manual-bib flag would misclassify it as Mixed, got:\n{after_reuse}"
        );
        assert_no_marker_leak(&after_reuse);
    }

    #[test]
    fn converter_reuse_does_not_drift_marker_indices() {
        // Two conversions on one converter: the second must not resolve against
        // the first document's pending citations.
        let mut converter = tylax::core::latex2typst::LatexConverter::new();
        let first = converter.convert_document(&manual_doc(r"See \cite{knuth}."));
        let second = converter.convert_document(&manual_doc(r"See \cite{lamport}."));
        assert!(first.contains("@knuth"), "first doc wrong:\n{first}");
        assert!(
            second.contains("@lamport") && !second.contains("@knuth"),
            "second doc must resolve its own cites, got:\n{second}"
        );
        assert_no_marker_leak(&first);
        assert_no_marker_leak(&second);
    }

    #[test]
    fn math_fragment_citation_keeps_cite_and_no_marker() {
        // A math-only path has no bibliography (None backend) and must still
        // resolve the marker to `#cite(...)`, never leak it.
        let out = latex_to_typst(r"\cite{knuth}");
        assert_no_marker_leak(&out);
    }

    #[test]
    fn converter_reuse_is_byte_identical_to_fresh_converter() {
        // The general invariant behind the per-conversion reset: converting a
        // document on a converter that already processed a *different* document
        // must produce exactly what a freshly constructed converter produces.
        // This covers every per-conversion field (title, counters, macros,
        // bibliography flags, pending cites, warnings) at once, so a future
        // state field cannot silently leak across reuse.
        let doc_a = "\\documentclass{article}\n\
                     \\newcommand{\\foo}{FOO}\n\
                     \\title{First Doc}\n\\begin{document}\n\
                     \\maketitle\n\\section{Alpha}\n\
                     See \\cite[p.~5]{knuth}.\n\
                     \\begin{thebibliography}{9}\n\
                     \\bibitem{knuth} Knuth, D.\n\\end{thebibliography}\n\\end{document}\n";
        let doc_b = "\\documentclass{article}\n\
                     \\title{Second Doc}\n\\begin{document}\n\
                     \\maketitle\n\\section{Beta}\nPlain \\foo body.\n\\end{document}\n";

        let fresh = latex_document_to_typst(doc_b);

        let mut reused = tylax::core::latex2typst::LatexConverter::new();
        let _ = reused.convert_document(doc_a);
        let after_reuse = reused.convert_document(doc_b);

        assert_eq!(
            after_reuse, fresh,
            "reused converter diverged from a fresh one:\n--- reused ---\n{after_reuse}\n--- fresh ---\n{fresh}"
        );
        assert_no_marker_leak(&after_reuse);
    }

    #[test]
    fn converter_reuse_does_not_leak_degradation_warning() {
        // A reused converter must not carry a prior conversion's warnings into
        // the next document. First convert a manual-bib doc whose postnote
        // degrades to a label reference (emits a bibliography-backend warning),
        // then convert a plain document with nothing to warn about.
        let mut converter = tylax::core::latex2typst::LatexConverter::new();

        let first = converter.convert_document(&manual_doc(r"See \cite[p.~5]{knuth}."));
        assert!(
            first.contains("degraded to a label reference"),
            "sanity: first doc should surface the degradation warning, got:\n{first}"
        );

        let plain = "\\documentclass{article}\n\\begin{document}\nPlain text.\n\\end{document}\n";
        let second = converter.convert_document(plain);
        assert!(
            !second.contains("degraded to a label reference"),
            "legacy warning comment leaked into a reused converter's next doc, got:\n{second}"
        );
        assert!(
            !second.to_lowercase().contains("bibliography"),
            "no bibliography-backend warning should survive into the plain doc, got:\n{second}"
        );
        assert_no_marker_leak(&second);
    }

    #[test]
    fn converter_reuse_does_not_leak_structured_warning() {
        // The structured-diagnostics sink must be isolated per conversion too.
        // The plain `convert_document` entry point does NOT drain structured
        // warnings, so a first degrading conversion leaves them populated; the
        // second document's `_with_diagnostics` take must not surface them.
        let mut converter = tylax::core::latex2typst::LatexConverter::new();

        // First: manual-bib author-year cite degrades → structured warning
        // pushed but (via plain `convert_document`) never taken.
        let first = converter.convert_document(&manual_doc(r"See \citet{knuth}."));
        assert!(
            first.contains("degraded to a label reference"),
            "sanity: first doc should degrade the citation, got:\n{first}"
        );

        let plain = "\\documentclass{article}\n\\begin{document}\nPlain text.\n\\end{document}\n";
        let second = converter.convert_document_with_diagnostics(plain);
        assert!(
            second.warnings.is_empty(),
            "structured warnings leaked into a reused converter's next doc: {:?}",
            second.warnings
        );
    }
}
