//! Regression tests for table parsing

use super::cell::GridCell;
use super::hline::{clean_cell_content, clean_hline_args};
use super::*;

#[test]
fn test_basic_table() {
    let content = "A|||CELL|||B|||CELL|||C|||ROW|||1|||CELL|||2|||CELL|||3";
    let alignments = vec![CellAlign::Left, CellAlign::Center, CellAlign::Right];
    let output = parse_with_grid_parser(content, alignments, &[]);

    assert!(output.contains("[A], [B], [C]"));
    assert!(output.contains("[1], [2], [3]"));
}

#[test]
fn test_multirow() {
    // Simulate: \multirow{2}{*}{A} & B & C \\ & D & E
    // The empty & becomes an empty string between markers
    let content = "___TYPST_CELL___:table.cell(rowspan: 2)[A]|||CELL|||B|||CELL|||C|||ROW||| |||CELL|||D|||CELL|||E";
    let alignments = vec![CellAlign::Center; 3];
    let output = parse_with_grid_parser(content, alignments, &[]);

    println!("Multirow output:\n{}", output);

    // Row 1 should have 3 cells
    assert!(output.contains("table.cell(rowspan: 2)[A], [B], [C]"));
    // Row 2 should only have 2 cells (first column covered, placeholder consumed)
    assert!(output.contains("[D], [E]"));
}

#[test]
fn test_multirow_non_ascii_no_panic() {
    // Regression for issue #36: a multi-byte character inside a \multirow cell
    // (`ı`, U+0131) used to make the bracket scanner mix a byte start index
    // with a char end index, slicing across a char boundary and panicking.
    // The inner content must now be extracted intact.
    let cell = GridCell::parse("___TYPST_CELL___:table.cell(rowspan: 2)[Guc katı]");
    assert_eq!(cell.rowspan, 2);
    assert_eq!(cell.content, "Guc katı");

    // End-to-end through the grid parser must not panic either.
    let content =
        "___TYPST_CELL___:table.cell(rowspan: 2)[Guc katı]|||CELL|||A|||ROW||| |||CELL|||B";
    let alignments = vec![CellAlign::Left; 2];
    let output = parse_with_grid_parser(content, alignments, &[]);
    assert!(output.contains("table.cell(rowspan: 2)[Guc katı]"));
}

#[test]
fn test_multicolumn() {
    // Simulate: A & \multicolumn{2}{c}{Wide} \\ 1 & 2 & 3
    let content =
        "A|||CELL|||___TYPST_CELL___:table.cell(colspan: 2)[Wide]|||ROW|||1|||CELL|||2|||CELL|||3";
    let alignments = vec![CellAlign::Left; 3];
    let output = parse_with_grid_parser(content, alignments, &[]);

    assert!(output.contains("[A], table.cell(colspan: 2)[Wide]"));
    assert!(output.contains("[1], [2], [3]"));
}

#[test]
fn test_sparse_data() {
    // Table with empty cells: A & & B \\ C & D &
    // Empty cells are represented as space between markers
    let content = "A|||CELL||| |||CELL|||B|||ROW|||C|||CELL|||D|||CELL||| ";
    let alignments = vec![CellAlign::Left; 3];
    let output = parse_with_grid_parser(content, alignments, &[]);

    println!("Sparse output:\n{}", output);

    // Empty cells should be preserved
    assert!(output.contains("[A], [], [B]"));
    assert!(output.contains("[C], [D], []"));
}

#[test]
fn test_hline() {
    // Table with hlines
    let content = "|||HLINE|||A|||CELL|||B|||ROW|||||CELL|||C|||CELL|||D|||ROW|||||HLINE|||";
    let alignments = vec![CellAlign::Center; 2];
    let output = parse_with_grid_parser(content, alignments, &[]);

    println!("HLine output:\n{}", output);

    assert!(output.contains("table.hline()"));
}

#[test]
fn test_cmidrule() {
    // A partial rule uses its own marker, with the range following it — the
    // shape the command converter emits for `\cmidrule(lr){2-4}`.
    let content = "|||HLINE|||A|||CELL|||B|||CELL|||C|||CELL|||D|||ROW||||||CHLINE|||(lr)2-4E|||CELL|||F|||CELL|||G|||CELL|||H";
    let alignments = vec![CellAlign::Center; 4];
    let output = parse_with_grid_parser(content, alignments, &[]);

    println!("Cmidrule output:\n{}", output);

    // The range must survive as a partial rule, not widen to a full one.
    assert!(
        output.contains("table.hline(start: 1, end: 4)"),
        "expected a partial rule spanning columns 2-4, got:\n{output}"
    );
    // Should not contain the raw cmidrule args
    assert!(!output.contains("(lr)"));
    assert!(!output.contains("2-4E"));
}

#[test]
fn test_full_rule_is_not_mistaken_for_a_partial_one() {
    // After a plain `\hline`, a cell that merely looks like a range must stay
    // cell content; only the partial marker may consume a range.
    let content = "|||HLINE|||3-4|||CELL|||B|||ROW|||C|||CELL|||D";
    let output = parse_with_grid_parser(content, vec![CellAlign::Left; 2], &[]);
    assert!(
        output.contains("table.hline(),"),
        "expected a full-width rule, got:\n{output}"
    );
    assert!(
        output.contains("3-4"),
        "the cell content must be kept, got:\n{output}"
    );
}

#[test]
fn test_several_rules_before_one_row_are_all_kept() {
    // `\midrule` immediately followed by `\cmidrule(lr){3-4}` is two rules.
    let content = "|||HLINE||||||CHLINE|||(lr)3-4A|||CELL|||B|||CELL|||C|||CELL|||D";
    let output = parse_with_grid_parser(content, vec![CellAlign::Left; 4], &[]);
    assert!(
        output.contains("table.hline(),") && output.contains("table.hline(start: 2, end: 4)"),
        "both rules must be emitted, got:\n{output}"
    );
}

#[test]
fn test_column_vertical_rules_are_emitted() {
    // `|` separators from the column spec must be drawn explicitly once the
    // default grid is off, and they count as declared rules on their own.
    let output = parse_with_grid_parser(
        "A|||CELL|||B|||ROW|||C|||CELL|||D",
        vec![CellAlign::Center; 2],
        &[0, 1, 2],
    );
    assert!(
        output.contains("stroke: none"),
        "vertical rules alone must switch the grid off, got:\n{output}"
    );
    for boundary in [
        "table.vline(x: 0)",
        "table.vline(x: 1)",
        "table.vline(x: 2)",
    ] {
        assert!(
            output.contains(boundary),
            "missing {boundary}, got:\n{output}"
        );
    }
}

#[test]
fn test_table_with_rules_disables_the_default_grid() {
    // LaTeX draws no rules unless asked; Typst's #table defaults to a full
    // grid, so a source that declares rules must switch it off.
    let with_rules = parse_with_grid_parser(
        "|||HLINE|||A|||CELL|||B|||ROW|||C|||CELL|||D",
        vec![CellAlign::Left; 2],
        &[],
    );
    assert!(
        with_rules.contains("stroke: none"),
        "explicit rules must disable the default grid, got:\n{with_rules}"
    );

    // A table that declares no rules keeps the previous default.
    let without = parse_with_grid_parser(
        "A|||CELL|||B|||ROW|||C|||CELL|||D",
        vec![CellAlign::Left; 2],
        &[],
    );
    assert!(
        !without.contains("stroke: none"),
        "a table without rules must be unchanged, got:\n{without}"
    );
}

#[test]
fn test_multirow_with_sparse() {
    // Complex: multirow in first column with sparse data in second
    // Row 1: \multirow{3}{*}{A} & B & C
    // Row 2: & & D  (first col covered, second empty)
    // Row 3: & E & F
    let content = "___TYPST_CELL___:table.cell(rowspan: 3)[A]|||CELL|||B|||CELL|||C|||ROW||| |||CELL||| |||CELL|||D|||ROW||| |||CELL|||E|||CELL|||F";
    let alignments = vec![CellAlign::Center; 3];
    let output = parse_with_grid_parser(content, alignments, &[]);

    println!("Multirow with sparse:\n{}", output);

    // Row 1: all three cells
    assert!(output.contains("table.cell(rowspan: 3)[A], [B], [C]"));
    // Row 2: only two cells (first covered), second is empty data
    assert!(output.contains("[], [D]"));
    // Row 3: only two cells
    assert!(output.contains("[E], [F]"));
}

#[test]
fn test_clean_cell_content() {
    assert_eq!(clean_cell_content("\\toprule A"), "A");
    assert_eq!(clean_cell_content("B \\hline"), "B");
    assert_eq!(clean_cell_content("\\cmidrule(lr){2-5} C"), "C");
    assert_eq!(clean_cell_content("\\cline{1-3}"), "");
}

#[test]
fn test_clean_hline_args() {
    assert_eq!(clean_hline_args("(lr)2-5 remaining"), "remaining");
    assert_eq!(clean_hline_args("3-4"), "");
    assert_eq!(clean_hline_args("(l)1-2 text"), "text");
}

#[test]
fn test_grid_cell_parse() {
    // Normal cell
    let cell = GridCell::parse("Hello");
    assert_eq!(cell.rowspan, 1);
    assert_eq!(cell.colspan, 1);
    assert!(!cell.is_special);

    // Special cell with spans
    let cell = GridCell::parse("___TYPST_CELL___:table.cell(rowspan: 2, colspan: 3)[Content]");
    assert_eq!(cell.rowspan, 2);
    assert_eq!(cell.colspan, 3);
    assert!(cell.is_special);
}

#[test]
fn test_empty_table() {
    let content = "";
    let alignments = vec![CellAlign::Left];
    let output = parse_with_grid_parser(content, alignments, &[]);

    // Should still produce valid table structure
    assert!(output.contains("table("));
    assert!(output.contains("columns:"));
}
