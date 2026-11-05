//! Recovery and source-boundary regressions.
use tylax::{latex_document_to_typst, latex_to_typst};

#[test]
fn script_recovery_handles_comments_in_both_modes() {
    // A comment may separate a script operator from the command that panics MiTeX.
    for source in [
        "__% comment\n\\limits",
        "^^% comment\r\n\\over",
        "_% first\n^% second\n\\choose",
        "__\u{2009}\\limits",
    ] {
        latex_to_typst(source);
        latex_document_to_typst(&format!("\\begin{{document}}${source}$\\end{{document}}"));
        tylax::diagnostics::check_latex(source);
    }
}

#[test]
fn literal_code_survives_recovery_and_macro_expansion() {
    // Protect before expansion and cleanup, retaining code indentation and line endings.
    for newline in ["\n", "\r\n"] {
        let body = format!("    \\sample{newline}__\\limits{newline}% literal comment{newline}^^\\over{newline}\tcode  {newline}{newline}    done()");
        for (env, header, separator) in [
            (
                "lstlisting",
                "[language=TeX,caption={a [b]}]",
                "% tag comment\n",
            ),
            ("minted", "{tex}", " "),
            ("verbatim", "", ""),
            ("Verbatim*", "% header note\n", ""),
        ] {
            let code = format!("\\begin{separator}{{{env}}}{header}{newline}{body}{newline}\\end{separator}{{{env}}}");
            let source = format!("\\newcommand{{\\sample}}{{expanded}}\n{code}");
            for options in [tylax::L2TOptions::default(), tylax::L2TOptions::no_expand()] {
                let result = tylax::latex_to_typst_with_diagnostics_options(&source, options);
                assert!(result.output.contains(&body), "{source}: {}", result.output);
                assert!(!result.output.contains("TylaxParserLiteral"));
                if env == "Verbatim*" {
                    assert!(!result.output.contains("header note"));
                }
            }
            let output = latex_to_typst(&code);
            assert!(output.contains(&body), "{code}: {output}");
        }
    }
}

#[test]
fn header_comments_do_not_close_literal_environments() {
    // End tags in header comments are text; braces in the body remain literal.
    let body = "% literal comment\n  { \\catcode";
    for header in [
        "% example \\end{lstlisting}\n[language=TeX,caption={\\SI{1}{m}}]",
        "[language=TeX,% example \\end{lstlisting}\ncaption={\\SI{1}{m}}]",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = format!(
                "\\begin{{lstlisting}}{header}\n{body}\n\\end{{lstlisting}}\nAfter \\catcode"
            )
            .replace('\n', newline);
            let body = body.replace('\n', newline);
            let output = latex_document_to_typst(&source);
            let result = tylax::latex_to_typst_with_diagnostics_options(
                &source,
                tylax::L2TOptions::default(),
            );
            assert_eq!(output, result.output);
            assert!(output.contains(&body), "{output}");
            assert!(output.contains("After"), "{output}");
            let math = latex_to_typst(&source);
            assert!(math.contains(&body), "{math}");
            assert_eq!(
                math,
                tylax::latex_math_to_typst_with_diagnostics(&source).output
            );
            assert!(math
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .contains("After"));
            let report = tylax::diagnostics::check_latex(&source);
            assert_eq!(report.errors, 0, "{report:?}");
            for command in ["SI", "catcode"] {
                let diagnostics: Vec<_> = report
                    .diagnostics
                    .iter()
                    .filter(|d| d.message.contains(&format!("'\\{command}'")))
                    .collect();
                assert_eq!(diagnostics.len(), 1, "{report:?}");
                let at = if command == "SI" {
                    source.find("\\SI").unwrap()
                } else {
                    source.rfind("\\catcode").unwrap()
                };
                let line = source[..at].bytes().filter(|b| *b == b'\n').count() + 1;
                let column = at - source[..at].rfind('\n').map_or(0, |n| n + 1) + 1;
                assert_eq!(
                    (diagnostics[0].line, diagnostics[0].column),
                    (Some(line), Some(column))
                );
            }
        }
    }
}

#[test]
fn malformed_headers_cannot_capture_following_text() {
    // A closing delimiter after the environment must not complete its header.
    for (env, header, ending) in [
        ("lstlisting", "[language=TeX", "]"),
        ("minted", "[linenos", "]"),
        ("minted", "{python", "}"),
    ] {
        let source = format!("\\begin{{{env}}}{header}\nx = 1\n\\end{{{env}}}\nText{ending} more");
        for output in [latex_document_to_typst(&source), latex_to_typst(&source)] {
            let compact: String = output.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(compact.contains("more"), "{source}: {output}");
            assert!(!compact.contains("Tylax"), "{source}: {output}");
        }
        tylax::diagnostics::check_latex(&source);
    }
}

#[test]
fn literal_markers_do_not_replace_user_content() {
    // User text resembling either marker namespace must survive every restoration pass.
    let mut source = String::new();
    let mut bodies = Vec::new();
    for i in 0..2 {
        let body = format!(
            "  block-{i} \u{E010}TylaxVerbatim{i}X\u{E011} \u{E010}TylaxParserLiteral{i}X\u{E011}"
        );
        source.push_str(&format!(
            "\\begin{{lstlisting}}\n{body}\n\\end{{lstlisting}}\n"
        ));
        bodies.push(body);
    }
    for output in [latex_document_to_typst(&source), latex_to_typst(&source)] {
        for body in &bodies {
            assert_eq!(output.matches(body).count(), 1, "{output}");
        }
    }
}

#[test]
fn recovered_diagnostics_use_original_positions() {
    // Recovery insertions and contracted environment tags must compose, including UTF-8/CRLF.
    for prefix in [
        "__\\limits ",
        "__% comment\n\\limits ^^\\over\n",
        "文本 __\\limits\r\n",
        "\\begin{minted}{tex}\n__\\limits\n\\end{minted}\n__\\limits\n",
    ] {
        let source = format!("{prefix}\\SI{{1}}{{m}} \\catcode");
        let report = tylax::diagnostics::check_latex(&source);
        for command in ["SI", "catcode"] {
            let at = source.find(&format!("\\{command}")).unwrap();
            let line = source[..at].bytes().filter(|b| *b == b'\n').count() + 1;
            let column = at - source[..at].rfind('\n').map_or(0, |i| i + 1) + 1;
            let diagnostic = report
                .diagnostics
                .iter()
                .find(|d| d.message.contains(&format!("'\\{command}'")))
                .unwrap();
            assert_eq!(
                (diagnostic.line, diagnostic.column),
                (Some(line), Some(column)),
                "{source}"
            );
        }
    }
}

#[test]
fn inline_literal_content_and_diagnostic_bounds_agree() {
    // Shared scanning must distinguish escaped content, starred verbs and byte offsets.
    let body = "文本 __\\limits % \\catcode { [";
    for star in ["", "*"] {
        for delimiter in ['|', '界'] {
            let source = format!("前缀 \\verb{star}{delimiter}{body}{delimiter} 后文 \\catcode");
            let output = latex_document_to_typst(&source);
            assert!(output.contains(body), "{source}: {output}");
            let report = tylax::diagnostics::check_latex(&source);
            let diagnostics: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|d| d.message.contains("'\\catcode'"))
                .collect();
            assert_eq!(diagnostics.len(), 1, "{report:?}");
            assert_eq!(
                diagnostics[0].column,
                Some(source.rfind("\\catcode").unwrap() + 1)
            );
        }
    }
}
