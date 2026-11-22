//! Math content, diagnostics and layout regressions.
use tylax::{
    typst_to_latex, typst_to_latex_with_diagnostics, typst_to_latex_with_options, T2LOptions,
};

fn math(source: &str) -> tylax::core::typst2latex::ConversionResult {
    let options = T2LOptions::math_only();
    let result = typst_to_latex_with_diagnostics(source, &options);
    assert_eq!(
        typst_to_latex_with_options(source, &options),
        result.output,
        "{source}"
    );
    let document = format!("${source}$");
    let checked = typst_to_latex_with_diagnostics(&document, &T2LOptions::default());
    assert_eq!(typst_to_latex(&document), checked.output, "{document}");
    assert_eq!(checked.output, format!("${}$", result.output), "{document}");
    assert_eq!(
        result
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>(),
        checked
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>(),
        "{source}",
    );
    result
}

#[test]
fn text_arguments_keep_content_and_empty_arguments() {
    // Dropping an argument can erase content or leave a bare \text command.
    for (source, expected) in [
        ("text(x)", r"\text{x}"),
        ("text(x + y)", r"\text{$x + y$}"),
        ("text(alpha)", r"\text{$\alpha$}"),
        ("text(frac(1,2))", r"\text{$\frac{1}{2}$}"),
        (r"text(\{)", r"\text{$\{$}"),
        ("#text($alpha$)", r"\text{$\alpha$}"),
        (r#"#text("a" + ("b" + "c"))"#, r"\text{abc}"),
        (r#"#text(/* before */ "a" /* join */ + "b")"#, r"\text{ab}"),
        (r#"#text("")"#, r"\text{}"),
        (r#"text(" and ")"#, r"\text{ and }"),
        ("#text[if $x > 0$]", r"\text{if $x > 0$}"),
    ] {
        let result = math(source);
        assert_eq!(result.output, expected, "{source}");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }
}

#[test]
fn numeric_percentages_cannot_comment_out_following_text() {
    // MiniEval's Ratio values must use the same text escaping as raw numeric nodes.
    let source = "#polygon((0%, 0%), (100%, 0%), (50%, 100%)) After";
    let result = typst_to_latex_with_diagnostics(source, &T2LOptions::default());
    assert_eq!(typst_to_latex(source), result.output);
    assert_eq!(result.output, r"0\% 0\%100\% 0\%50\% 100\% After");
}

#[test]
fn measured_context_recognition_uses_structure() {
    // Whitespace, comments and code grouping do not change the measured skip.
    for source in [
        "#context { h(-measure($x$).width); $x$ }",
        "#context { h( -measure($x$).width); $x$ }",
        "#context { h(-(measure($x$).width)); $x$ }",
        "#context { /* skip */ (h)(-((measure)($x /* measured */$)).width); $x$ }",
    ] {
        let result = math(source);
        assert_eq!(result.output, r"\llap{$x$}", "{source}");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }
    for (body, expected) in [("x/y", r"\frac{x}{y}"), ("x^2", "x^2")] {
        assert_eq!(
            math(&format!(
                "#context {{ h(-measure(${body}$).width); ${body}$ }}"
            ))
            .output,
            format!(r"\llap{{${expected}$}}"),
        );
    }
}

#[test]
fn unsupported_contexts_keep_source_and_following_content() {
    // Neither a matching substring nor a different measured body may masquerade as llap.
    for context in [
        "#context { $y$ }",
        "#context { [h(-measure(]; $x$ }",
        "#context { h(-measure($x$).width); $y$ }",
        "#context { h(-measure($x$).width); $x$; $y$ }",
        "#context { h(-measure($ x $).width); $x$ }",
        "#context { h(-measure($ x $).width); $ x $ }",
        "#context {\r\n let n = 1;\r\n $y$\r\n}",
    ] {
        let result = math(&format!("a + {context} + z"));
        for line in context.lines() {
            assert!(
                result.output.contains(&format!("% {line}\n")),
                "{}",
                result.output
            );
        }
        assert!(result.output.contains("\n\\relax{}\n"));
        assert!(result.output.ends_with("+ z"), "{}", result.output);
        assert!(!result.output.contains(r"\llap"));
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0].message.contains("unsupported context"));
    }
    let source = r"$a + #context { [`\begin{matrix}`]; $y$ } & z$";
    let output = typst_to_latex(source);
    assert_eq!(
        output,
        typst_to_latex_with_diagnostics(source, &T2LOptions::default()).output
    );
    assert!(output.starts_with(r"\begin{align}"), "{output}");
    assert!(output.contains("& z"), "{output}");
    let context = "#context { let n = 1; $y$ }";
    let source = format!("#let n = 2\n$a + {context} + z$ #n");
    let checked = typst_to_latex_with_diagnostics(&source, &T2LOptions::default());
    assert_eq!(typst_to_latex(&source), checked.output);
    assert!(checked.output.contains(&format!("% {context}\n")));
    assert!(checked.output.ends_with(" 2"), "{}", checked.output);
}

#[test]
fn stacked_limits_keep_multiline_scripts() {
    // Compiling successfully does not detect the loss of a script's second row.
    for (source, expected) in [
        (r"limits(F)_(x \ y)", r"\underset{\substack{x \\ y}}{F}"),
        (r"limits(F)^(x \ y)", r"\overset{\substack{x \\ y}}{F}"),
    ] {
        assert_eq!(math(source).output, expected);
    }
}

#[test]
fn matrix_augmentation_ignores_trivia_and_grouping() {
    // Dictionary formatting must not erase rules or invent one from a comment.
    let expected = math("mat(augment: #(vline: 1, hline: 1), a,b;c,d)").output;
    assert!(expected.contains("{c|c}"));
    assert!(expected.contains(r"\hline"));
    for layout in [
        "(vline : 1, hline : 1)",
        "(vline /* column */ : 1, hline: 1)",
        "((vline: (1), hline: (1,)))",
        "(vline: +1, hline: (+1,))",
        r#"("vline": 1, "hline": 1)"#,
    ] {
        assert_eq!(
            math(&format!("mat(augment: #{layout}, a,b;c,d)")).output,
            expected
        );
    }
    let rows_only = math("mat(augment: #(hline: 1), a,b;c,d)").output;
    assert_eq!(
        math("mat(augment: #(/* vline: 1 */ hline: 1), a,b;c,d)").output,
        rows_only,
    );
    let multiple = math("mat(augment: #(vline: (1,2), hline: (1,2)), a,b,c;d,e,f;g,h,i)").output;
    assert!(multiple.contains("{c|c|c}"));
    assert_eq!(multiple.matches(r"\hline").count(), 2);
}

#[test]
fn named_math_values_keep_their_types() {
    // Preserving numeric metadata must not turn string or content arguments into code tokens.
    for (source, canonical) in [
        ("mat(augment: #(+1), a,b)", "mat(augment: #1, a,b)"),
        ("mat(delim: #(\"‖\"), a)", "mat(delim: \"‖\", a)"),
        ("attach(x, t: #(\"a\"))", "attach(x, t: \"a\")"),
        ("attach(x, t: #([$a$]))", "attach(x, t: a)"),
    ] {
        assert_eq!(math(source).output, math(canonical).output, "{source}");
    }
}

#[test]
fn nested_math_warnings_survive_text_and_delimiters() {
    // Rendering a nested fragment must not hide an unknown name or warn about literal text.
    for source in ["lr((foo.bar))", "#text[$foo.bar$]", "#text[*if $foo.bar$*]"] {
        let result = math(source);
        assert!(result.output.contains("foo.bar"));
        assert_eq!(result.warnings.len(), 1, "{source}: {:?}", result.warnings);
        assert!(result.warnings[0].message.contains("`foo.bar`"));
    }
    assert!(math("#text[foo.bar]").warnings.is_empty());
}

#[test]
fn in_math_text_paragraphs_and_special_escapes_stay_compilable() {
    for (source, expected) in [
        ("#text[a\n\nb] + x", r"\text{a b} + x"),
        ("#text[\n\n] + x", r"\text{} + x"),
        ("#text[a\n// comment\nb] + x", r"\text{a b} + x"),
        (r"text(\$) + x", r"\text{$\$$} + x"),
        ("text(%) + x", r"\text{\%} + x"),
        (r"text(\%) + x", r"\text{$\%$} + x"),
        ("text(a + %) + x", r"\text{$a + \%$} + x"),
        ("a + % + b", r"a + \% + b"),
        (r"\$ + x", r"\$ + x"),
        (r"#text[\$ and %] + x", r"\text{\$ and \%} + x"),
    ] {
        let result = math(source);
        assert_eq!(result.output, expected, "{source}");
        assert!(
            result.warnings.is_empty(),
            "{source}: {:?}",
            result.warnings
        );
    }

    let markup = r"Cost is \$5 and 50%.";
    let diag = typst_to_latex_with_diagnostics(markup, &T2LOptions::default());
    assert_eq!(typst_to_latex(markup), diag.output);
    assert_eq!(diag.output, r"Cost is \$5 and 50\%.");
    assert!(diag.warnings.is_empty(), "{:?}", diag.warnings);
}

#[test]
fn literal_text_concatenation_matches_across_apis_and_fails_atomically() {
    for (source, expected) in [
        ("#text([hello] + [world])", r"\text{helloworld}"),
        ("#text(([hello]) + [ $x$])", r"\text{hello $x$}"),
        (r#"#text("a" + "b")"#, r"\text{ab}"),
        (r"#text([hello] + [\$])", r"\text{hello\$}"),
    ] {
        let result = math(source);
        assert_eq!(result.output, expected, "{source}");
        assert!(
            result.warnings.is_empty(),
            "{source}: {:?}",
            result.warnings
        );
    }

    let dynamic = "#let s = \"world\"\n$#text(\"hello \" + s)$";
    let diag = typst_to_latex_with_diagnostics(dynamic, &T2LOptions::default());
    assert_eq!(diag.output, r"$\text{hello world}$");
    assert!(diag.warnings.is_empty(), "{:?}", diag.warnings);
    let plain = typst_to_latex(dynamic);
    assert!(plain.contains(r#"% "hello " + s"#), "{plain}");
    assert!(plain.contains("\n\\relax{}\n"), "{plain}");
    assert!(!plain.contains(r"\text{hello"), "{plain}");

    for (bad, arg_src) in [
        ("#text([hello] + missing_var)", "[hello] + missing_var"),
        (r#"#text("a" + [b])"#, r#""a" + [b]"#),
    ] {
        let result = math(bad);
        assert!(result.output.starts_with("\\text{\n%"), "{}", result.output);
        assert!(
            result.output.contains(&format!("% {arg_src}\n")),
            "{}",
            result.output
        );
        assert!(result.output.contains("\n\\relax{}\n"), "{}", result.output);
        assert_eq!(result.warnings.len(), 1, "{bad}: {:?}", result.warnings);
        assert!(
            result.warnings[0]
                .message
                .contains("could not convert a text argument"),
            "{bad}: {:?}",
            result.warnings
        );
    }
}

#[test]
fn required_empty_raisebox_is_preserved_without_affecting_other_commands() {
    for (source, expected) in [
        ("#box(baseline: -2pt)[]", r"\raisebox{2pt}{}"),
        ("#box(baseline: -2pt)[x]", r"\raisebox{2pt}{x}"),
        ("#box(baseline: -2pt)[a\n\nb]", r"\raisebox{2pt}{a b}"),
        (
            r"#box(baseline: -2pt)[\$ and %] + x",
            r"\raisebox{2pt}{\$ and \%} + x",
        ),
    ] {
        let result = math(source);
        assert_eq!(result.output, expected, "{source}");
        assert!(
            result.warnings.is_empty(),
            "{source}: {:?}",
            result.warnings
        );
    }

    let non_required = math("bold()");
    assert_eq!(non_required.output, r"\boldsymbol");
}

#[test]
fn rlap_box_uses_math_text_and_empty_rules_consistently() {
    for (source, expected) in [
        ("#box(width: 0pt, $x/y$)", r"\rlap{$\frac{x}{y}$}"),
        ("#box(width: 0pt)[$x/y$]", r"\rlap{$\frac{x}{y}$}"),
        ("#box(width: 0pt)[$x^2$]", r"\rlap{$x^2$}"),
        ("#box(width: 0pt)[hello]", r"\rlap{hello}"),
        ("#box(width: 0pt)[hi $x/y$]", r"\rlap{hi $\frac{x}{y}$}"),
        ("#box(width: 0pt)[]", r"\rlap{}"),
        (r"#box(width: 0pt)[\$ and %] + x", r"\rlap{\$ and \%} + x"),
    ] {
        let result = math(source);
        assert_eq!(result.output, expected, "{source}");
        assert!(
            result.warnings.is_empty(),
            "{source}: {:?}",
            result.warnings
        );
    }

    let warned = math("#box(width: 0pt)[$foo.bar$]");
    assert_eq!(warned.output, r"\rlap{$foo.bar$}");
    assert_eq!(warned.warnings.len(), 1, "{:?}", warned.warnings);
    assert!(warned.warnings[0].message.contains("`foo.bar`"));
}

#[test]
fn context_llap_requires_confirmed_builtin_h_and_measure() {
    let ctx = "#context { h(-measure($x$).width); $x$ }";

    for safe in [
        format!("#[#let h(x) = [local]; ok]\n${ctx}$"),
        format!("#{{ let h(x) = [local]; [ok] }}\n${ctx}$"),
        format!("${ctx}$\n#let h(x) = [later]"),
    ] {
        let diag = typst_to_latex_with_diagnostics(&safe, &T2LOptions::default());
        assert!(
            diag.output.contains(r"\llap{$x$}"),
            "{safe}: {}",
            diag.output
        );
        assert!(diag.warnings.is_empty(), "{safe}: {:?}", diag.warnings);
        let plain = typst_to_latex(&safe);
        assert!(plain.contains(r"\llap{$x$}"), "{safe}: {plain}");
    }

    let unrelated_import = format!("#import \"helpers.typ\": unrelated\n${ctx}$");
    let diag = typst_to_latex_with_diagnostics(&unrelated_import, &T2LOptions::default());
    assert!(diag.output.contains(r"\llap{$x$}"), "{}", diag.output);
    assert!(
        !diag
            .warnings
            .iter()
            .any(|w| w.message.contains("cannot confirm")),
        "{:?}",
        diag.warnings
    );
    assert!(typst_to_latex(&unrelated_import).contains(r"\llap{$x$}"));

    let mixed = format!("${ctx}$\n#let h(x) = [H]\n$#context {{ h(-measure($y$).width); $y$ }}$");
    for (label, output, has_warning) in [
        ("plain", typst_to_latex(&mixed), None),
        (
            "diag",
            {
                let res = typst_to_latex_with_diagnostics(&mixed, &T2LOptions::default());
                res.output
            },
            Some(typst_to_latex_with_diagnostics(&mixed, &T2LOptions::default()).warnings),
        ),
    ] {
        assert!(output.contains(r"\llap{$x$}"), "{label}: {output}");
        assert!(!output.contains(r"\llap{$y$}"), "{label}: {output}");
        assert!(
            output.contains("% #context { h(-measure($y$).width); $y$ }\n"),
            "{label}: {output}"
        );
        if let Some(warnings) = has_warning {
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert!(warnings[0].message.contains("cannot confirm"));
        }
    }

    for (shadowed, needs_file_warning) in [
        (format!("#let h(x) = [H]\n${ctx}$"), false),
        (format!("#let measure(x) = (width: 0pt)\n${ctx}$"), false),
        (format!("#let (h, k) = (x => x, 1)\n${ctx}$"), false),
        (format!("#for h in (x => x,) [${ctx}$]"), false),
        (format!("#let f(h) = ${ctx}$\n#f(x => x)"), false),
        (format!("#import \"helpers.typ\": *\n${ctx}$"), true),
        (format!("#import \"helpers.typ\" as m: *\n${ctx}$"), true),
        (format!("#import \"helpers.typ\" as m: h\n${ctx}$"), true),
        (format!("#import \"helpers.typ\": h\n${ctx}$"), true),
        (
            format!("#import \"helpers.typ\": a as measure\n${ctx}$"),
            true,
        ),
        (format!("#import \"h.typ\"\n${ctx}$"), true),
    ] {
        let diag = typst_to_latex_with_diagnostics(&shadowed, &T2LOptions::default());
        let plain = typst_to_latex(&shadowed);
        for (mode, output) in [("diag", &diag.output), ("plain", &plain)] {
            assert!(!output.contains(r"\llap"), "{shadowed} [{mode}]: {output}");
            assert!(
                output.contains(&format!("% {ctx}\n")),
                "{shadowed} [{mode}]: {output}"
            );
            assert!(
                output.contains("\n\\relax{}"),
                "{shadowed} [{mode}]: {output}"
            );
        }
        assert!(
            diag.warnings
                .iter()
                .any(|w| w.message.contains("cannot confirm built-in h/measure")),
            "{shadowed}: {:?}",
            diag.warnings
        );
        if needs_file_warning {
            assert!(
                diag.warnings
                    .iter()
                    .any(|w| w.message.contains("file not found")),
                "{shadowed}: {:?}",
                diag.warnings
            );
        }
    }
}

#[test]
fn generic_distinguishes_math_zws_from_hash_identifier() {
    let math_zws = math("zws_(a)");
    assert_eq!(math_zws.output, "{}_a");
    assert!(math_zws.warnings.is_empty(), "{:?}", math_zws.warnings);

    let defined = "#let zws = \"kept\"\n$#zws$";
    let diag = typst_to_latex_with_diagnostics(defined, &T2LOptions::default());
    assert_eq!(diag.output, r"$\text{kept}$");
    assert!(diag.warnings.is_empty(), "{:?}", diag.warnings);
    assert_eq!(typst_to_latex(defined).trim(), r"$\text{kept}$");

    let bare_ident = math("#zws");
    assert_eq!(bare_ident.output, "zws");
    assert!(!bare_ident.output.contains('#'));
    assert_eq!(bare_ident.warnings.len(), 1, "{:?}", bare_ident.warnings);
    assert!(bare_ident.warnings[0].message.contains("`zws`"));
}

#[test]
fn context_llap_binding_is_scoped_per_occurrence() {
    let ctx = "#context { h(-measure($x$).width); $x$ }";

    for (source, expected_llap_count, expect_file_warning) in [
        (
            format!("${ctx}$\n\n#[\n  #let h(x) = [H]\n  ${ctx}$\n]\n\n${ctx}$"),
            2,
            false,
        ),
        (
            format!("${ctx}$\n\n#{{ let h(x) = [H]; ${ctx}$ }}\n\n${ctx}$"),
            2,
            false,
        ),
        (format!("${ctx}$\n#let h(x) = [H]\n${ctx}$"), 1, false),
        (
            format!("${ctx}$\n#for h in (x => x,) [${ctx}$]\n${ctx}$"),
            2,
            false,
        ),
        (
            format!("${ctx}$\n#let f(h) = ${ctx}$\n#f(x => x)\n${ctx}$"),
            2,
            false,
        ),
        (
            format!("${ctx}$\n#import \"helpers.typ\": *\n${ctx}$"),
            1,
            true,
        ),
        (
            format!("${ctx}$\n#[#import \"helpers.typ\": *; ${ctx}$]\n${ctx}$"),
            2,
            true,
        ),
    ] {
        let diag = typst_to_latex_with_diagnostics(&source, &T2LOptions::default());
        let plain = typst_to_latex(&source);
        for (mode, output) in [("diag", &diag.output), ("plain", &plain)] {
            assert_eq!(
                output.matches(r"\llap{$x$}").count(),
                expected_llap_count,
                "{source} [{mode}]: {output}"
            );
            assert_eq!(
                output.matches(&format!("% {ctx}\n")).count(),
                1,
                "{source} [{mode}]: {output}"
            );
            assert!(
                !output.contains("__tylax_shadowed_ctx"),
                "{source} [{mode}]: {output}"
            );
        }
        assert_eq!(
            diag.warnings
                .iter()
                .filter(|w| w.message.contains("cannot confirm built-in h/measure"))
                .count(),
            1,
            "{source}: {:?}",
            diag.warnings
        );
        assert_eq!(
            diag.warnings
                .iter()
                .any(|w| w.message.contains("file not found")),
            expect_file_warning,
            "{source}: {:?}",
            diag.warnings
        );
    }

    let with_marker_collision =
        format!("`/*__tylax_shadowed_ctx_0__*/`\n${ctx}$\n#[#let h(x) = [H]; ${ctx}$]\n${ctx}$");
    let diag = typst_to_latex_with_diagnostics(&with_marker_collision, &T2LOptions::default());
    let plain = typst_to_latex(&with_marker_collision);
    for (mode, output) in [("diag", &diag.output), ("plain", &plain)] {
        assert!(
            output.contains(r"\texttt{/*\_\_tylax\_shadowed\_ctx\_0\_\_*/}"),
            "[{mode}]: {output}"
        );
        assert_eq!(
            output.matches(r"\llap{$x$}").count(),
            2,
            "[{mode}]: {output}"
        );
        assert_eq!(
            output.matches(&format!("% {ctx}\n")).count(),
            1,
            "[{mode}]: {output}"
        );
        assert!(
            !output.contains("__tylax_shadowed_ctx_1"),
            "[{mode}]: {output}"
        );
    }
    assert_eq!(diag.warnings.len(), 1, "{:?}", diag.warnings);
    assert!(diag.warnings[0]
        .message
        .contains("cannot confirm built-in h/measure"));

    let three_blocks = format!("${ctx}$\n\n#[\n  #let h(x) = [H]\n  ${ctx}$\n]\n\n${ctx}$");
    let expanded = tylax::core::typst2latex::expand_macros(&three_blocks).unwrap();
    assert!(!expanded.contains("__tylax_shadowed_ctx"), "{expanded}");
}

#[test]
fn in_math_raw_text_and_multiline_strings_stay_compilable() {
    for (source, expected) in [
        ("#text[`a\n\nb`] + x", r"\text{\texttt{a b}} + x"),
        ("#text[```txt\na\n\nb\n```] + x", r"\text{\texttt{a b}} + x"),
        ("#text[```a\n\nb```] + x", r"\text{\texttt{b}} + x"),
        ("#text[``] + x", r"\text{\texttt{}} + x"),
        (
            r"#text[`a $ % # _ \ b`] + x",
            r"\text{\texttt{a \$ \% \# \_ \textbackslash{} b}} + x",
        ),
        (
            "#box(baseline: -2pt)[`a\n\nb`] + x",
            r"\raisebox{2pt}{\texttt{a b}} + x",
        ),
        ("#box(width: 0pt)[`a\n\nb`] + x", r"\rlap{\texttt{a b}} + x"),
        (r#"#text("a\n\nb") + x"#, r"\text{a b} + x"),
        ("\"a\n\nb\" + x", r"\text{a b} + x"),
        ("#box[`a\n\nb`] + x", r"\texttt{a b} + x"),
        ("#text[`` + ` `] + x", r"\text{\texttt{} + \texttt{ }} + x"),
    ] {
        let result = math(source);
        assert_eq!(result.output, expected, "{source}");
        assert!(
            result.warnings.is_empty(),
            "{source}: {:?}",
            result.warnings
        );
    }
}
