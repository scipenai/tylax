use tylax::{expand_macros, typst_to_latex, typst_to_latex_with_diagnostics, T2LOptions};

fn evaluated(source: &str) -> String {
    let result = typst_to_latex_with_diagnostics(source, &T2LOptions::default());
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    result.output
}

#[test]
fn nested_list_function_emits_outer_item() {
    for (source, outer_env, inner_env) in [
        ("#list(list([x], [y]), [z])", "itemize", "itemize"),
        ("#enum(list([x], [y]), [z])", "enumerate", "itemize"),
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            let outer_begin = format!("\\begin{{{outer_env}}}");
            let inner_begin = format!("\\begin{{{inner_env}}}");
            let outer_pos = output.find(&outer_begin).unwrap();
            let after_outer = &output[outer_pos + outer_begin.len()..];
            let first_item = after_outer.find(r"\item").unwrap();
            let nested_pos = after_outer.find(&inner_begin).unwrap();
            assert!(first_item < nested_pos, "{output}");
            assert!(output.contains(r"\item z"), "{output}");
        }
    }
}

fn strip_latex_line_comments(latex: &str) -> String {
    latex
        .lines()
        .map(|line| {
            let mut escaped = false;
            for (idx, ch) in line.char_indices() {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '%' {
                    return &line[..idx];
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn table_enum_max_u64_marker_avoids_overflow() {
    let single = "#table(columns: 1, [18446744073709551615. Last])";
    for output in [
        typst_to_latex_with_diagnostics(single, &T2LOptions::default()).output,
        typst_to_latex(single),
    ] {
        assert!(output.contains("18446744073709551615.~Last"), "{output}");
        assert!(!output.contains("0.~"), "{output}");
    }

    let followed = "#table(columns: 1, [18446744073709551615. Last\n+ After])";
    for output in [
        typst_to_latex_with_diagnostics(followed, &T2LOptions::default()).output,
        typst_to_latex(followed),
    ] {
        assert!(output.contains("18446744073709551615.~Last"), "{output}");
        assert!(output.contains(r"\textbullet~After"), "{output}");
        assert!(!output.contains("0.~After"), "{output}");
    }
}

#[test]
fn list_and_table_enum_ignore_interleaved_comments() {
    let list_src = "#list([A], /* note */ [B])";
    for output in [evaluated(list_src), typst_to_latex(list_src)] {
        assert_eq!(output.matches(r"\item").count(), 2, "{output}");
        assert!(output.contains(r"\item A"), "{output}");
        assert!(output.contains(r"\item B"), "{output}");
    }

    let table_enum_src = "#table(columns: 1, [#enum([A], /* note */ [B])])";
    for output in [evaluated(table_enum_src), typst_to_latex(table_enum_src)] {
        assert!(output.contains("1.~A"), "{output}");
        assert!(output.contains("2.~B"), "{output}");
        assert!(!output.contains("3.~"), "{output}");
    }
}

#[test]
fn evaluated_table_nested_enumerations_preserve_indentation_and_numbering() {
    for source in [
        "#table(columns: 1, [\n+ parent\n  + nested\n+ sibling\n])",
        "#table(columns: 1, [#(\" \")\n+ parent\n  + nested\n+ sibling\n])",
        "#table(columns: 1, [#(\"\")\n+ parent\n  + nested\n+ sibling\n])",
    ] {
        let expanded = expand_macros(source).unwrap();
        assert_eq!(expand_macros(&expanded).unwrap(), expanded, "{source}");

        for output in [
            evaluated(source),
            typst_to_latex(source),
            typst_to_latex(&expanded),
        ] {
            assert!(output.contains("1.~parent"), "{source}: {output}");
            assert!(output.contains("1.~nested"), "{source}: {output}");
            assert!(output.contains("2.~sibling"), "{source}: {output}");
            assert!(!output.contains("3.~sibling"), "{source}: {output}");
        }
    }
}

#[test]
fn list_spreads_expand_literals_and_never_emit_empty_environments() {
    let literal = "#list(..([A], [B]))";
    for output in [evaluated(literal), typst_to_latex(literal)] {
        assert_eq!(output.matches(r"\begin{itemize}").count(), 1, "{output}");
        assert!(output.contains(r"\item A"), "{output}");
        assert!(output.contains(r"\item B"), "{output}");
    }

    for dynamic in ["#list(..items)", "#enum([A], ..items, [D])"] {
        let ast_out = typst_to_latex(dynamic);
        assert!(
            !ast_out.contains(r"\begin{itemize}"),
            "{dynamic}: {ast_out}"
        );
        assert!(
            !ast_out.contains(r"\begin{enumerate}"),
            "{dynamic}: {ast_out}"
        );
        assert!(ast_out.contains("% Tylax:"), "{dynamic}: {ast_out}");
        assert!(ast_out.contains(dynamic), "{dynamic}: {ast_out}");

        let diag = typst_to_latex_with_diagnostics(dynamic, &T2LOptions::default());
        assert!(!diag.warnings.is_empty(), "{dynamic}: {:?}", diag.warnings);
        assert!(
            !diag.output.contains(r"\begin{itemize}"),
            "{dynamic}: {}",
            diag.output
        );
        assert!(
            !diag.output.contains(r"\begin{enumerate}"),
            "{dynamic}: {}",
            diag.output
        );
        assert!(
            diag.output.contains("% Tylax:"),
            "{dynamic}: {}",
            diag.output
        );
        assert!(diag.output.contains(dynamic), "{dynamic}: {}", diag.output);
    }

    for (source, expect_next) in [
        ("#table(columns: 1, [#enum([A], ..items, [D])])", false),
        (
            "#table(columns: 2, [#enum([A], ..items, [D])], [Next])",
            true,
        ),
    ] {
        let ast_out = typst_to_latex(source);
        let diag = typst_to_latex_with_diagnostics(source, &T2LOptions::default());
        assert!(!diag.warnings.is_empty(), "{source}: {:?}", diag.warnings);
        for output in [ast_out, diag.output] {
            assert!(output.contains("% Tylax:"), "{source}: {output}");
            assert!(
                output.contains("#enum([A], ..items, [D])"),
                "{source}: {output}"
            );
            assert!(!output.contains("2.~D"), "{source}: {output}");
            let active = strip_latex_line_comments(&output);
            assert!(active.contains(r"\\"), "{source}: {output}");
            assert!(active.contains(r"\end{tabular}"), "{source}: {output}");
            if expect_next {
                assert!(active.contains("& Next"), "{source}: {output}");
            }
        }
    }
}
