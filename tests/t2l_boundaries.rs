//! Table column validation regressions.
use tylax::{typst_to_latex, typst_to_latex_with_diagnostics, T2LOptions};

fn diagnostics(source: &str) -> tylax::core::typst2latex::ConversionResult {
    typst_to_latex_with_diagnostics(source, &T2LOptions::default())
}

fn table_rows(output: &str) -> Vec<&str> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(r"\\"))
        .collect()
}

#[test]
fn table_rules_do_not_consume_cells_or_hide_layouts() {
    // Rules are not cells; existing horizontal rules must still be drawn.
    for (name, cells, width, rows, cline) in [
        (
            "table",
            "[a], [b], table.vline(), [c], [d]",
            "2",
            vec![r"a & b \\", r"c & d \\"],
            false,
        ),
        (
            "table",
            "table.vline(x: 1), [a], [b], [c]",
            "3",
            vec![r"a & b & c \\"],
            false,
        ),
        (
            "grid",
            "grid.vline(x: 1), [a], [b]",
            "2",
            vec![r"a & b \\"],
            false,
        ),
        (
            "table",
            "table . vline(), table/* hline header cell */.vline(), (table.vline)(), [a], [b]",
            "2",
            vec![r"a & b \\"],
            false,
        ),
        (
            "table",
            "table.header(table.hline(), [H1], table.vline(x: 1), [H2]), [a], [b]",
            "2",
            vec![r"H1 & H2 \\", r"a & b \\"],
            false,
        ),
        (
            "table",
            "[a], [b], table.hline(start: 0, end: 1), [c], [d]",
            "2",
            vec![r"a & b \\", r"c & d \\"],
            true,
        ),
        (
            "grid",
            "[a], [b], grid.hline(start: 0, end: 1), [c], [d]",
            "2",
            vec![r"a & b \\", r"c & d \\"],
            true,
        ),
    ] {
        for columns in [width, "tracks"] {
            let source = format!("#{name}(columns: {columns}, {cells}) After");
            let plain = typst_to_latex(&source);
            let result = diagnostics(&source);
            if columns == "tracks" {
                assert_eq!(plain, result.output, "{source}");
            }
            for output in [&plain, &result.output] {
                assert!(output.contains("\\begin{tabular}"), "{source}: {output}");
                assert_eq!(table_rows(output), rows, "{source}: {output}");
                assert!(
                    !output.contains("could not determine all table cells"),
                    "{output}"
                );
                assert_eq!(output.contains("approximate"), columns == "tracks");
                assert_eq!(output.contains("\\cline{1-1}"), cline, "{output}");
                assert!(output.ends_with("After"));
            }
            assert!(
                !result
                    .warnings
                    .iter()
                    .any(|w| w.message.contains("table cells")),
                "{:?}",
                result.warnings
            );
        }
    }
}

#[test]
fn parenthesized_table_rules_match_unwrapped_calls() {
    // Parentheses must not turn a rule into a missing cell or discard hline arguments.
    for (name, rule, header) in [
        ("table", "table.vline(x: 1)", false),
        ("grid", "grid.vline(x: 1)", false),
        ("table", "table.hline(start: 0, end: 1)", false),
        ("grid", "grid.hline(start: 0, end: 1)", false),
        ("table", "table.vline(x: 1)", true),
        ("table", "table.hline()", true),
        ("table", "(table.vline)()", false),
    ] {
        for columns in ["columns: 2, ", "", "columns: tracks, "] {
            let source = |rule: &str| {
                let cells = if header {
                    format!("table.header([H1], {rule}, [H2]), [a], [b]")
                } else {
                    format!("[a], [b], {rule}, [c], [d]")
                };
                format!("#{name}({columns}{cells}) After")
            };
            let expected = typst_to_latex(&source(rule));
            let expected_result = diagnostics(&source(rule));
            assert!(expected.contains("\\begin{tabular}"), "{expected}");
            for wrapped in [format!("({rule})"), format!("((/* rule */ {rule}))")] {
                let input = source(&wrapped);
                let plain = typst_to_latex(&input);
                let result = diagnostics(&input);
                assert_eq!(plain, expected, "{input}");
                assert_eq!(result.output, expected_result.output, "{input}");
                assert_eq!(table_rows(&plain), table_rows(&result.output), "{input}");
                assert_eq!(
                    result
                        .warnings
                        .iter()
                        .map(|w| &w.message)
                        .collect::<Vec<_>>(),
                    expected_result
                        .warnings
                        .iter()
                        .map(|w| &w.message)
                        .collect::<Vec<_>>(),
                    "{input}"
                );
            }
        }
    }
}

#[test]
fn none_cells_and_empty_spreads_keep_their_positions() {
    // A positional none occupies one cell; spreading none adds no arguments.
    for (cells, rows) in [
        ("[a], none, [b], [c]", vec![r"a & \\", r"b & c \\"]),
        ("[a], (none), [b], [c]", vec![r"a & \\", r"b & c \\"]),
        ("[a], ..none, [b]", vec![r"a & b \\"]),
        ("[a], ..((none)), [b]", vec![r"a & b \\"]),
        (
            "table.header([H1], none), [a], [b]",
            vec![r"H1 & \\", r"a & b \\"],
        ),
        (
            "table.header([H1], ..none, [H2]), [a], [b]",
            vec![r"H1 & H2 \\", r"a & b \\"],
        ),
        (
            "table.cell(none), [a], [b], [c]",
            vec![r"& a \\", r"b & c \\"],
        ),
        (
            "table.cell(..none), [a], [b], [c]",
            vec![r"& a \\", r"b & c \\"],
        ),
    ] {
        for columns in ["2", "tracks"] {
            let source = format!("#table(columns: {columns}, {cells}) After");
            let plain = typst_to_latex(&source);
            let result = diagnostics(&source);
            if columns == "tracks" {
                assert_eq!(plain, result.output, "{source}");
            }
            for output in [&plain, &result.output] {
                assert_eq!(table_rows(output), rows, "{source}: {output}");
                assert_eq!(output.contains("approximate"), columns == "tracks");
                assert!(!output.contains("table cells"), "{output}");
                assert!(output.ends_with("After"));
            }
        }
    }
}

#[test]
fn column_limit_covers_scalar_array_and_repeated_tracks() {
    // Alternate syntax must not bypass the guard or silently reshape a table.
    let array = format!("({},)", vec!["auto"; 1001].join(","));
    for tracks in [
        "1001",
        "+1001",
        "(1001)",
        "0x3e9",
        "(auto,) * 1001",
        "1001 * (auto,)",
        "(..((auto,) * 1001),)",
        "(..(auto, auto), ..((auto,) * 999))",
        &array,
    ] {
        for name in ["table", "grid"] {
            let source = format!("#{name}(columns: {tracks}, [kept])");
            let result = diagnostics(&source);
            let plain = typst_to_latex(&source);
            assert_eq!(plain, result.output, "{source}");
            assert!(!plain.contains("\\begin{tabular}"), "{plain}");
            assert!(plain.contains("column count 1001"), "{plain}");
            assert!(plain.contains("kept"), "{plain}");
            assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        }
    }
}

#[test]
fn unsupported_table_is_not_expanded_before_rejection() {
    // Evaluating a rejected table must not abort the rest of the document.
    let maximum = usize::MAX.to_string();
    for (tracks, count) in [
        ("(auto,) * 1001", "1001"),
        (maximum.as_str(), maximum.as_str()),
    ] {
        let result = diagnostics(&format!(
            "#table(columns: {tracks}, [#assert(false)]) After"
        ));
        assert!(result.output.contains("After"));
        assert!(result.output.contains("assert(false)"));
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0]
            .message
            .contains(&format!("column count {count}")));
    }
}

#[test]
fn column_inference_keeps_supported_layouts() {
    // Trailing commas, nested commas and scalar lengths have different cardinalities.
    for (tracks, columns) in [
        ("2", 2),
        ("+2", 2),
        ("(auto, auto,)", 2),
        ("(calc.max(1pt, 2pt), auto)", 2),
        ("(1fr,)", 1),
        ("(+1fr)", 1),
        ("(1fr,) * 3", 3),
        ("(auto,) * +3", 3),
        ("3 * (1fr,)", 3),
        ("(..(auto, auto, auto),)", 3),
        ("(..(), auto)", 1),
        ("(..none, auto)", 1),
        ("(..(auto, ..(auto,)), auto)", 3),
        ("(..(auto,), ..(auto, auto), auto)", 4),
        ("30pt", 1),
        ("30pt / 2.0", 1),
        ("auto", 1),
    ] {
        let cells = (0..columns * 2)
            .map(|n| format!("[v{n}]"))
            .collect::<Vec<_>>()
            .join(",");
        let source = format!("#table(columns: {tracks}, {cells})");
        let plain = typst_to_latex(&source);
        let evaluated = diagnostics(&source).output;
        assert_eq!(plain, evaluated, "{source}");
        assert_eq!(
            plain.lines().next().unwrap().matches('c').count(),
            columns,
            "{source}: {plain}"
        );
        assert_eq!(
            plain.lines().filter(|line| line.ends_with(r"\\")).count(),
            2,
            "{plain}"
        );
        assert!(plain.contains(&format!("v{}", columns * 2 - 1)));
    }
    // Exactly-at-limit tables still preserve all cells.
    let cells = (0..1000)
        .map(|n| format!("[v{n}]"))
        .collect::<Vec<_>>()
        .join(",");
    let result = diagnostics(&format!("#table(columns: 1000, {cells})"));
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert_eq!(
        result.output.lines().next().unwrap().matches('c').count(),
        1000
    );
    assert!(result.output.contains("v999"));
    let source = "#table([a], [b], [c], [d])";
    let plain = typst_to_latex(source);
    assert_eq!(plain, diagnostics(source).output);
    assert!(plain.starts_with("\\begin{tabular}{|c|c|}"), "{plain}");
    assert_eq!(
        plain.lines().filter(|line| line.ends_with(r"\\")).count(),
        2
    );
}

#[test]
fn evaluated_column_counts_use_the_same_limit() {
    // Variables are unknown to static inference and must be checked after expansion.
    for source in [
        "#let n = 1001; #table(columns: n, [kept]) After",
        "#let tracks = (auto,) * 1001; #grid(columns: tracks, [kept]) After",
    ] {
        let result = diagnostics(source);
        assert!(!result.output.contains("\\begin{tabular}"));
        assert!(result.output.contains("kept"));
        assert!(result.output.contains("After"));
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
        assert!(result.warnings[0].message.contains("column count 1001"));
    }
}

#[test]
fn unresolved_columns_keep_known_cells_with_a_warning() {
    // Unknown widths must not hide obtainable cells or silently claim an exact layout.
    for (source, columns) in [
        ("#table(columns: tracks, [kept], [last]) After", 2),
        ("#table(columns: (..tracks, auto), [kept], [last]) After", 2),
        (
            "#let n = 1001; #table(columns: (auto,) * n, [#calc.sin(30deg) kept], [last]) After",
            2,
        ),
        (
            "#table(columns: tracks, table.header([kept], [H]), table.cell[last], []) After",
            2,
        ),
        (
            "#table(columns: tracks, \"\", [kept], [last], [end]) After",
            2,
        ),
        ("#grid(columns: tracks, text[kept], [last]) After", 2),
    ] {
        let plain = typst_to_latex(source);
        let result = diagnostics(source);
        assert_eq!(plain, result.output, "{source}");
        assert!(
            plain.contains(&format!("an approximate {columns}-column layout")),
            "{plain}"
        );
        assert!(!plain.contains("layout not converted"), "{plain}");
        let table = plain.find("\\begin{tabular}").unwrap();
        let end = plain[table..].find("\\end{tabular}").unwrap() + table;
        assert!(plain[table..end].contains("kept"), "{plain}");
        assert!(plain[table..end].contains("last"), "{plain}");
        assert_eq!(
            plain[table..].lines().next().unwrap().matches('c').count(),
            columns
        );
        assert!(plain.ends_with("After"), "{plain}");
        assert_eq!(
            result
                .warnings
                .iter()
                .filter(|w| w.message.contains("approximate"))
                .count(),
            1
        );
    }
}

#[test]
fn unresolved_cells_keep_the_whole_table_as_source() {
    // A known prefix must not conceal a dropped spread, header item or cell body.
    for columns in ["tracks", "2"] {
        for (cells, recovered) in [
            ("..cells", false),
            ("[visible], ..cells", true),
            ("[visible], cell", true),
            ("table.header([visible], ..cells)", true),
            ("table.header([visible], cell)", true),
            ("table.header([visible], table.cell(cell))", true),
            ("table.cell(cell)", false),
            ("table.cell(..cells)", false),
            ("align(left, body)", false),
        ] {
            if columns == "2" && recovered {
                continue;
            }
            let table = format!("#table(columns: {columns}, {cells})");
            let source = format!("{table} After");
            let plain = typst_to_latex(&source);
            let result = diagnostics(&source);
            assert_eq!(plain, result.output, "{source}");
            assert!(!plain.contains("\\begin{tabular}"), "{plain}");
            assert!(
                plain.contains("could not determine all table cells"),
                "{plain}"
            );
            assert!(
                plain.contains(&format!("% {table}\n\\relax{{}}")),
                "{plain}"
            );
            assert!(plain.ends_with("After"));
            assert_eq!(
                result
                    .warnings
                    .iter()
                    .filter(|w| w.message.contains("could not determine all table cells"))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn known_columns_keep_partial_cells_with_a_warning() {
    // Recoverable bodies before or after unknown arguments must not be discarded.
    for cells in [
        "[KEPT], ..cells, [LAST]",
        "[KEPT], missing, [LAST]",
        "[KEPT], sym.checkmark, [LAST]",
        "table.header([KEPT], ..cells), [LAST]",
        "table.cell([KEPT], ..attrs), [LAST]",
        "table.cell(..attrs)[KEPT], [LAST]",
        "table.header(table.cell(..attrs)[KEPT]), [LAST]",
    ] {
        let source = format!("#table(columns: 2, {cells}) After");
        let plain = typst_to_latex(&source);
        let result = diagnostics(&source);
        assert_eq!(plain, result.output, "{source}");
        assert_eq!(table_rows(&plain), vec![r"KEPT & LAST \\"], "{plain}");
        assert!(!plain.contains("layout not converted"), "{plain}");
        assert!(plain.contains("may be incomplete or misaligned"), "{plain}");
        assert!(plain.ends_with("After"));
        assert_eq!(
            result
                .warnings
                .iter()
                .filter(|w| w.message.contains("may be incomplete or misaligned"))
                .count(),
            1
        );
    }
}

#[test]
fn successful_evaluation_keeps_exact_dynamic_layouts() {
    // Safe fallback must neither enable evaluation in the plain API nor disable it in MiniEval.
    let source = "#let n = 3; #table(columns: n, [a], [b], [c], [d]) After";
    let plain = typst_to_latex(source);
    assert!(plain.contains("an approximate 2-column layout"), "{plain}");
    let result = diagnostics(source);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(
        result.output.contains("\\begin{tabular}{|c|c|c|}"),
        "{}",
        result.output
    );
    for output in [&plain, &result.output] {
        let table = &output[output.find("\\begin{tabular}").unwrap()..];
        assert!(table.contains(" a & b"), "{table}");
        assert!(table.contains(" d"), "{table}");
        assert!(output.ends_with("After"));
    }
    let source = "#let cells = ([A], [B]); #table(columns: 2, ..cells) After";
    assert!(!typst_to_latex(source).contains("\\begin{tabular}"));
    let result = diagnostics(source);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(result.output.contains(" A & B"), "{}", result.output);
    assert!(result.output.ends_with("After"));

    // MiniEval's existing raw and parenthesized cells must keep working.
    for value in ["`RESOLVED`", "([RESOLVED])"] {
        let source = format!("#table(columns: 2, [KEPT], {value}, [LAST]) After");
        let plain = typst_to_latex(&source);
        assert_eq!(table_rows(&plain), vec![r"KEPT & LAST \\"], "{plain}");
        assert!(plain.contains("may be incomplete or misaligned"));
        let result = diagnostics(&source);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(result.output.contains("RESOLVED"), "{}", result.output);
        assert!(result.output.contains("KEPT"));
        assert!(result.output.contains("LAST"));
        assert!(result.output.ends_with("After"));
    }
}

#[test]
fn nested_layout_warnings_preserve_content_and_parent() {
    // A child warning must survive, and its source comment must not consume its parent.
    let table = "#table(columns: 1001, [two  spaces], [kept])";
    for source in [
        format!("#figure[{table}] After"),
        format!("#table(columns: 2, [{table}], [After])"),
        format!("#table(columns: 2, table.cell[{table}], [After])"),
    ] {
        let plain = typst_to_latex(&source);
        assert!(
            plain.contains(&format!("% {table}\n\\relax{{}}")),
            "{plain}"
        );
        let result = diagnostics(&source);
        assert!(result.output.contains("After"), "{}", result.output);
        assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
    }
    // Approximate layouts must also keep their warning outside nested LaTeX syntax.
    let table = "#table(columns: tracks, [INNERONE], [INNERTWO])";
    for source in [
        format!("#figure[{table}] After"),
        format!("#table(columns: 2, [{table}], [After])"),
        format!("#table(columns: 2, table.cell[{table}], [After])"),
        format!("#box[{table}] After"),
        format!("- {table}\n\nAfter"),
    ] {
        let plain = typst_to_latex(&source);
        let result = diagnostics(&source);
        assert_eq!(plain, result.output, "{source}");
        assert!(plain.contains("INNERONE & INNERTWO"), "{plain}");
        assert!(plain.contains("After"), "{plain}");
        assert_eq!(
            result
                .warnings
                .iter()
                .filter(|w| w.message.contains("approximate"))
                .count(),
            1
        );
    }
    // Discard warnings from abandoned cells, retaining warnings for earlier output.
    let source = format!("{table}\n#table(columns: tracks, [{table}], ..cells) After");
    let plain = typst_to_latex(&source);
    let result = diagnostics(&source);
    assert_eq!(plain, result.output);
    assert_eq!(plain.matches("approximate").count(), 1, "{plain}");
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|w| w.message.contains("approximate"))
            .count(),
        1,
        "{:?}",
        result.warnings
    );
    assert!(plain.contains("could not determine all table cells"));
    assert!(plain.ends_with("After"));
}

#[test]
fn column_guard_ignores_raw_text_and_shadowed_functions() {
    // A function called table need not be Typst's table, and raw code is literal.
    for source in [
        "`#table(columns: 66666666, [kept])`",
        "#let table(body, columns: 0) = body; #table(columns: 1001)[kept]",
    ] {
        let result = diagnostics(source);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(result.output.contains("kept"));
        assert!(!result.output.contains("supported limit"));
    }
}
