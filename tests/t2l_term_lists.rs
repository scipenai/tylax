use tylax::{expand_macros, typst_to_latex, typst_to_latex_with_diagnostics, T2LOptions};

fn evaluated(source: &str) -> String {
    let result = typst_to_latex_with_diagnostics(source, &T2LOptions::default());
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    result.output
}

#[test]
fn term_items_keep_labels_and_paragraph_boundaries() {
    let source = "Intro.\n/ Time \\: 30: Noon.\n  It starts here.\n\n  It continues here.\n\n/ Next: Second.";
    for output in [
        evaluated(source),
        typst_to_latex(&expand_macros(source).unwrap()),
    ] {
        assert!(output.contains(r"\item[Time : 30] Noon."), "{output}");
        assert!(output.contains(r"\item[Next] Second."), "{output}");
        assert_eq!(
            output.matches(r"\begin{description}").count(),
            1,
            "{output}"
        );
        assert!(
            output.find("It continues here.").unwrap() < output.find(r"\end{description}").unwrap(),
            "{output}"
        );
    }

    let output = evaluated("#let t = \"A: B\"\n/ #t: Details.");
    assert!(output.contains(r"\item[A: B] Details."), "{output}");
}

#[test]
fn term_items_keep_nested_list_structure() {
    for (source, outer) in [
        ("- Parent\n  / Inner: Child", "itemize"),
        ("+ Parent\n  / Inner: Child", "enumerate"),
        ("/ Parent: Intro.\n  / Inner: Child", "description"),
    ] {
        for output in [
            evaluated(source),
            typst_to_latex(&expand_macros(source).unwrap()),
        ] {
            let begin = output.find(&format!(r"\begin{{{outer}}}")).unwrap();
            let item = output.find(r"\item[Inner] Child").unwrap();
            let end = output.rfind(&format!(r"\end{{{outer}}}")).unwrap();
            assert!(begin < item && item < end, "{output}");
            assert_eq!(
                output.matches(r"\begin{description}").count(),
                if outer == "description" { 2 } else { 1 },
                "{output}"
            );
        }
    }
}

#[test]
fn term_description_keeps_bullets_and_terms_as_siblings() {
    let source = "/ Outer: Intro.\n  - Bullet 1\n  - Bullet 2\n  / Inner: More.";
    for output in [
        evaluated(source),
        typst_to_latex(&expand_macros(source).unwrap()),
    ] {
        assert_eq!(output.matches(r"\begin{itemize}").count(), 1, "{output}");
        let bullet_end = output.find(r"\end{itemize}").unwrap();
        let nested_term = output.find(r"\item[Inner] More.").unwrap();
        let outer_end = output.rfind(r"\end{description}").unwrap();
        assert!(
            bullet_end < nested_term && nested_term < outer_end,
            "{output}"
        );
    }
}

#[test]
fn top_level_terms_follow_list_items_without_becoming_children() {
    let source = "- Bullet\n/ Term: Definition.\n+ Numbered";
    for output in [
        evaluated(source),
        typst_to_latex(&expand_macros(source).unwrap()),
    ] {
        let bullet_end = output.find(r"\end{itemize}").unwrap();
        let term = output.find(r"\item[Term] Definition.").unwrap();
        let numbered = output.find(r"\begin{enumerate}").unwrap();
        assert!(bullet_end < term && term < numbered, "{output}");
    }
}

#[test]
fn expand_macros_on_term_lists_is_idempotent() {
    for source in [
        "/ Ligature: A merged glyph.\n/ Kerning: A spacing adjustment.",
        "/ Outer: Intro.\n  - Bullet 1\n  / Inner: More.",
    ] {
        let once = expand_macros(source).unwrap();
        let twice = expand_macros(&once).unwrap();
        assert_eq!(twice, once, "source: {source}");
    }
}

#[test]
fn explicit_terms_item_calls_evaluate_and_convert_string_args() {
    let dynamic = "#let author = \"Alice\"\n#terms.item([Author], [#author])";
    let dynamic_out = evaluated(dynamic);
    assert!(
        dynamic_out.contains(r"\begin{description}"),
        "{dynamic_out}"
    );
    assert!(
        dynamic_out.contains(r"\item[Author] Alice"),
        "{dynamic_out}"
    );
    assert!(dynamic_out.contains(r"\end{description}"), "{dynamic_out}");

    for (source, expected) in [
        (
            r#"#terms.item("A_1", "one & two")"#,
            r"\item[A\_1] one \& two",
        ),
        (
            r#"#terms.item("Key", [*bold*])"#,
            r"\item[Key] \textbf{bold}",
        ),
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            assert_eq!(
                output.matches(r"\begin{description}").count(),
                1,
                "{output}"
            );
            assert!(output.contains(expected), "{output}");
            assert!(output.contains(r"\end{description}"), "{output}");
        }
    }

    let shadowed = expand_macros("#let terms = 1\n#terms.item(\"A\", \"B\")");
    assert!(shadowed.is_err(), "{shadowed:?}");
}

#[test]
fn term_lists_preserve_trailing_paragraph_breaks() {
    for source in [
        "/ A: First.\n/ B: Second.\n\nFollowing paragraph.",
        "/ A: First.\n\n/ B: Second.\n\nFollowing paragraph.",
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            assert_eq!(
                output.matches(r"\begin{description}").count(),
                1,
                "{output}"
            );
            assert!(output.contains(r"\item[A] First."), "{output}");
            assert!(output.contains(r"\item[B] Second."), "{output}");
            assert!(
                output.contains("\\end{description}\n\nFollowing paragraph."),
                "{output}"
            );
        }
    }

    let before_macro = "#terms.item([A], [First.])\n\n#strong[Note]";
    for output in [evaluated(before_macro), typst_to_latex(before_macro)] {
        assert_eq!(
            output.matches(r"\begin{description}").count(),
            1,
            "{output}"
        );
        assert!(
            output.contains("\\end{description}\n\n\\textbf{Note}"),
            "{output}"
        );
    }
}

#[test]
fn term_lists_inside_tables_avoid_lr_mode_errors() {
    let single = "#table(columns: 1, [/ A: B])";
    for output in [evaluated(single), typst_to_latex(single)] {
        assert!(!output.contains(r"\begin{description}"), "{output}");
        assert!(!output.contains(r"\item["), "{output}");
        assert!(output.contains(r"\begin{tabular}"), "{output}");
        assert!(output.contains(r"\textbf{A} B"), "{output}");
    }

    let multiple = "#table(columns: 1, [/ A: B\n/ C: D])";
    for output in [evaluated(multiple), typst_to_latex(multiple)] {
        assert!(!output.contains(r"\begin{description}"), "{output}");
        assert!(!output.contains(r"\item["), "{output}");
        assert_eq!(output.matches(r"\begin{tabular}").count(), 2, "{output}");
        assert!(
            output
                .contains(r"\begin{tabular}[t]{@{}l@{}}\textbf{A} B \\ \textbf{C} D\end{tabular}"),
            "{output}"
        );
    }

    for source in [
        "#table(columns: 1, [#terms.item([A], [- x\n- y])])",
        "#table(columns: 1, [/ A: \n  - x\n  - y])",
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            assert!(!output.contains(r"\begin{description}"), "{output}");
            assert!(!output.contains(r"\begin{itemize}"), "{output}");
            assert!(output.contains(r"\textbf{A}"), "{output}");
            assert!(output.contains(r"\textbullet~x"), "{output}");
            assert!(output.contains(r"\textbullet~y"), "{output}");
        }
    }

    let ordered = "#table(columns: 1, [#terms.item([A], [+ x\n+ y])])";
    for output in [evaluated(ordered), typst_to_latex(ordered)] {
        assert!(!output.contains(r"\begin{description}"), "{output}");
        assert!(!output.contains(r"\begin{enumerate}"), "{output}");
        assert!(output.contains(r"\textbf{A}"), "{output}");
        assert!(output.contains(r"1.~x"), "{output}");
        assert!(output.contains(r"2.~y"), "{output}");
    }

    let heading = "#table(columns: 1, [#terms.item([A], [= Title])])";
    for output in [evaluated(heading), typst_to_latex(heading)] {
        assert!(!output.contains(r"\begin{description}"), "{output}");
        assert!(!output.contains(r"\section{"), "{output}");
        assert!(output.contains(r"\textbf{A} \textbf{Title}"), "{output}");
    }

    let list_fn = "#table(columns: 1, [#terms.item([A], [#list([x], [y])])])";
    for output in [evaluated(list_fn), typst_to_latex(list_fn)] {
        assert!(!output.contains(r"\begin{description}"), "{output}");
        assert!(!output.contains(r"\begin{itemize}"), "{output}");
        assert!(output.contains(r"\textbf{A}"), "{output}");
        assert!(output.contains(r"\textbullet~x"), "{output}");
        assert!(output.contains(r"\textbullet~y"), "{output}");
    }

    let enum_fn = "#table(columns: 1, [#terms.item([A], [#enum([x], [y])])])";
    for output in [evaluated(enum_fn), typst_to_latex(enum_fn)] {
        assert!(!output.contains(r"\begin{description}"), "{output}");
        assert!(!output.contains(r"\begin{enumerate}"), "{output}");
        assert!(output.contains(r"\textbf{A}"), "{output}");
        assert!(output.contains(r"1.~x"), "{output}");
        assert!(output.contains(r"2.~y"), "{output}");
    }

    for (code_src, expected_a, expected_b) in [
        (
            "#table(columns: 1, [#terms.item([A], [```txt\nline 1\nline 2\n```])])",
            r"\texttt{line 1}",
            r"\texttt{line 2}",
        ),
        (
            "#table(columns: 1, [#terms.item([A], [```rust\nlet x = 1;\n```])])",
            r"\texttt{let x = 1;}",
            r"\texttt{let x = 1;}",
        ),
    ] {
        for output in [evaluated(code_src), typst_to_latex(code_src)] {
            assert!(!output.contains(r"\begin{description}"), "{output}");
            assert!(!output.contains(r"\begin{verbatim}"), "{output}");
            assert!(!output.contains(r"\begin{lstlisting}"), "{output}");
            assert!(!output.contains("% Code block"), "{output}");
            assert!(output.contains(r"\textbf{A}"), "{output}");
            assert!(output.contains(expected_a), "{output}");
            assert!(output.contains(expected_b), "{output}");
        }
    }
}

#[test]
fn string_args_preserve_comment_sequences_and_leading_slashes() {
    for (source, expected) in [
        (r#"#terms.item("Key", "A // B")"#, r"\item[Key] A // B"),
        (
            r#"#terms.item("Key", "A /* B */ C")"#,
            r"\item[Key] A /* B */ C",
        ),
        (
            r#"#terms.item("Key", "/ Nested: Fake")"#,
            r"\item[Key] / Nested: Fake",
        ),
        (
            r#"#terms.item("Key", "= literal")"#,
            r"\item[Key] = literal",
        ),
        (
            r#"#terms.item("Key", "- literal")"#,
            r"\item[Key] - literal",
        ),
        (
            r#"#terms.item("Key", "+ literal")"#,
            r"\item[Key] + literal",
        ),
        (
            r#"#terms.item("Key", "1. literal")"#,
            r"\item[Key] 1. literal",
        ),
        (r#"#terms.item("Key", "A -- B")"#, r"\item[Key] A -{}- B"),
        (
            r#"#terms.item("Key", "A --- B")"#,
            r"\item[Key] A -{}-{}- B",
        ),
        (r#"#terms.item("Key", "A ... B")"#, r"\item[Key] A ... B"),
        (
            r#"#terms.item("Key", "A ~ B")"#,
            r"\item[Key] A \textasciitilde{} B",
        ),
        (r#"#terms.item("Key", "a-?b")"#, r"\item[Key] a-?b"),
        (
            r#"#terms.item("Key", "'a' \"b\"")"#,
            r#"\item[Key] 'a' "b""#,
        ),
    ] {
        let once = expand_macros(source).unwrap();
        let twice = expand_macros(&once).unwrap();
        assert_eq!(twice, once, "{source}");

        for output in [
            evaluated(source),
            typst_to_latex(source),
            typst_to_latex(&once),
        ] {
            assert_eq!(
                output.matches(r"\begin{description}").count(),
                1,
                "{output}"
            );
            assert!(output.contains(expected), "{output}");
            assert!(!output.contains(r"\section"), "{output}");
            assert!(!output.contains(r"\begin{itemize}"), "{output}");
            assert!(!output.contains(r"\begin{enumerate}"), "{output}");
        }
    }
}

#[test]
fn term_description_lists_preserve_continuation_paragraphs() {
    let unordered = "/ Outer: Intro.\n  - Bullet\n\n    Continued.";
    let once = expand_macros(unordered).unwrap();
    let twice = expand_macros(&once).unwrap();
    assert_eq!(twice, once);

    for output in [
        evaluated(unordered),
        typst_to_latex(unordered),
        typst_to_latex(&once),
    ] {
        let itemize_begin = output.find(r"\begin{itemize}").unwrap();
        let cont = output.find("Continued.").unwrap();
        let itemize_end = output.find(r"\end{itemize}").unwrap();
        let desc_end = output.find(r"\end{description}").unwrap();
        assert!(itemize_begin < cont && cont < itemize_end, "{output}");
        assert!(itemize_end < desc_end, "{output}");
    }

    let ordered = "/ Outer: Intro.\n  + Step 1\n\n    Details.";
    let once_ord = expand_macros(ordered).unwrap();
    let twice_ord = expand_macros(&once_ord).unwrap();
    assert_eq!(twice_ord, once_ord);

    for output in [
        evaluated(ordered),
        typst_to_latex(ordered),
        typst_to_latex(&once_ord),
    ] {
        let enum_begin = output.find(r"\begin{enumerate}").unwrap();
        let details = output.find("Details.").unwrap();
        let enum_end = output.find(r"\end{enumerate}").unwrap();
        let desc_end = output.find(r"\end{description}").unwrap();
        assert!(enum_begin < details && details < enum_end, "{output}");
        assert!(enum_end < desc_end, "{output}");
    }
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

#[test]
fn table_aligned_math_uses_aligned_environment() {
    for source in [
        "#table(columns: 1, [/ A: $ a &= b $])",
        "#table(columns: 1, [#terms.item([A], [$ a &= b \\ c &= d $])])",
        "#table(columns: 1, [$ [a] &= b $])",
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            assert!(!output.contains(r"\begin{align}"), "{output}");
            assert!(output.contains(r"$\begin{aligned}"), "{output}");
            assert!(output.contains(r"\end{aligned}$"), "{output}");
        }
    }
}

#[test]
fn table_code_block_preserves_indentation() {
    for source in [
        "#table(columns: 1, [#terms.item([A], [```txt\nroot\n  child\n    leaf\n```])])",
        "#table(columns: 1, [\n  #terms.item([A], [\n    ```txt\n    root\n      child\n        leaf\n    ```\n  ])\n])",
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            assert!(output.contains(r"\texttt{root}"), "{output}");
            assert!(output.contains(r"\texttt{~~child}"), "{output}");
            assert!(output.contains(r"\texttt{~~~~leaf}"), "{output}");
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
fn table_retained_math_diagnostic_keeps_comment_newlines() {
    let source = "#table(columns: 1, [/ A: $#context { showy }$])";
    for output in [
        typst_to_latex_with_diagnostics(source, &T2LOptions::default()).output,
        typst_to_latex(source),
    ] {
        let active = strip_latex_line_comments(&output);
        assert!(active.contains(r"\textbf{A}"), "output:\n{output}");
        assert_eq!(
            active.matches('$').count() % 2,
            0,
            "unclosed math in active TeX:\n{output}"
        );
        assert!(
            active.contains(r"\\"),
            "missing table row terminator:\n{output}"
        );
        assert!(
            active.contains(r"\end{tabular}"),
            "missing table end:\n{output}"
        );
    }
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
fn ast_and_evaluated_terms_item_support_expr_and_parenthesized_args() {
    for (source, expected) in [
        (
            "#terms.item(strong[A], emph[B])",
            r"\item[\textbf{A}] \textit{B}",
        ),
        ("#terms.item(([A]), (\"B\"))", r"\item[A] B"),
        (
            "#terms.item((strong[A]), (emph[B]))",
            r"\item[\textbf{A}] \textit{B}",
        ),
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            assert_eq!(
                output.matches(r"\begin{description}").count(),
                1,
                "{source}: {output}"
            );
            assert!(output.contains(expected), "{source}: {output}");
        }
    }

    let shadowed = "#let terms = (item: (a, b) => [#a #b])\n#terms.item(strong[A], emph[B])";
    for output in [
        typst_to_latex_with_diagnostics(shadowed, &T2LOptions::default()).output,
        typst_to_latex(shadowed),
    ] {
        assert!(!output.contains(r"\begin{description}"), "{output}");
        assert!(!output.contains(r"\item["), "{output}");
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
fn terms_item_handles_concatenated_content_and_string_exprs() {
    for (source, expected) in [
        ("#terms.item([A] + [B], [C])", r"\item[AB] C"),
        (
            "#terms.item(\"A\" + \"B\", [C] + emph[D])",
            r"\item[AB] C\textit{D}",
        ),
    ] {
        for output in [evaluated(source), typst_to_latex(source)] {
            assert_eq!(
                output.matches(r"\begin{description}").count(),
                1,
                "{source}: {output}"
            );
            assert!(output.contains(expected), "{source}: {output}");
        }
    }

    let standalone = "#terms.item([A], [+ First] + [+ Second])";
    for output in [evaluated(standalone), typst_to_latex(standalone)] {
        assert_eq!(output.matches(r"\begin{enumerate}").count(), 1, "{output}");
        assert_eq!(output.matches(r"\item ").count(), 2, "{output}");
    }

    let in_table = "#table(columns: 1, [#terms.item([A], [+ First] + [+ Second])])";
    for output in [evaluated(in_table), typst_to_latex(in_table)] {
        assert!(output.contains("1.~First"), "{output}");
        assert!(output.contains("2.~Second"), "{output}");
        assert_eq!(output.matches("1.~").count(), 1, "{output}");
    }
}

#[test]
fn shorthand_lists_and_terms_stay_grouped_across_comments() {
    let table_enum = "#table(columns: 1, [\n+ First\n// note\n+ Second\n])";
    for output in [evaluated(table_enum), typst_to_latex(table_enum)] {
        assert!(output.contains("1.~First"), "{output}");
        assert!(output.contains("2.~Second"), "{output}");
        assert_eq!(output.matches("1.~").count(), 1, "{output}");
    }

    let terms_comment = "/ A: One\n/* note */\n/ B: Two";
    for output in [evaluated(terms_comment), typst_to_latex(terms_comment)] {
        assert_eq!(
            output.matches(r"\begin{description}").count(),
            1,
            "{output}"
        );
        assert!(output.contains(r"\item[A] One"), "{output}");
        assert!(output.contains(r"\item[B] Two"), "{output}");
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
