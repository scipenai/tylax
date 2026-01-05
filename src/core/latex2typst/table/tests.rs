//! Regression tests for table parsing

use super::*;

#[test]
fn test_basic_table() {
    let content = "A|||CELL|||B|||CELL|||C|||ROW|||1|||CELL|||2|||CELL|||3";
    let alignments = vec![CellAlign::Left, CellAlign::Center, CellAlign::Right];
    let output = parse_with_grid_parser(content, alignments);

    assert!(output.contains("[A], [B], [C]"));
    assert!(output.contains("[1], [2], [3]"));
}

#[test]
fn test_multirow() {
    // Simulate: \multirow{2}{*}{A} & B & C \\ & D & E
    // The empty & becomes an empty string between markers
    let content = "___TYPST_CELL___:table.cell(rowspan: 2)[A]|||CELL|||B|||CELL|||C|||ROW||| |||CELL|||D|||CELL|||E";
    let alignments = vec![CellAlign::Center; 3];
    let output = parse_with_grid_parser(content, alignments);

    println!("Multirow output:\n{}", output);

    // Row 1 should have 3 cells
    assert!(output.contains("table.cell(rowspan: 2)[A], [B], [C]"));
    // Row 2 should only have 2 cells (first column covered, placeholder consumed)
    assert!(output.contains("[D], [E]"));
}

#[test]
fn test_multicolumn() {
    // Simulate: A & \multicolumn{2}{c}{Wide} \\ 1 & 2 & 3
    let content =
        "A|||CELL|||___TYPST_CELL___:table.cell(colspan: 2)[Wide]|||ROW|||1|||CELL|||2|||CELL|||3";
    let alignments = vec![CellAlign::Left; 3];
    let output = parse_with_grid_parser(content, alignments);

    assert!(output.contains("[A], table.cell(colspan: 2)[Wide]"));
    assert!(output.contains("[1], [2], [3]"));
}

#[test]
fn test_sparse_data() {
    // Table with empty cells: A & & B \\ C & D &
    // Empty cells are represented as space between markers
    let content = "A|||CELL||| |||CELL|||B|||ROW|||C|||CELL|||D|||CELL||| ";
    let alignments = vec![CellAlign::Left; 3];
    let output = parse_with_grid_parser(content, alignments);

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
    let output = parse_with_grid_parser(content, alignments);

    println!("HLine output:\n{}", output);

    assert!(output.contains("table.hline()"));
}

#[test]
fn test_cmidrule() {
    // Partial line with cmidrule info: (lr)2-4
    let content = "|||HLINE|||A|||CELL|||B|||CELL|||C|||CELL|||D|||ROW|||(lr)2-4|||HLINE|||E|||CELL|||F|||CELL|||G|||CELL|||H";
    let alignments = vec![CellAlign::Center; 4];
    let output = parse_with_grid_parser(content, alignments);

    println!("Cmidrule output:\n{}", output);

    // Should not contain the raw cmidrule args
    assert!(!output.contains("(lr)"));
    assert!(!output.contains("2-4"));
}

#[test]
fn test_multirow_with_sparse() {
    // Complex: multirow in first column with sparse data in second
    // Row 1: \multirow{3}{*}{A} & B & C
    // Row 2: & & D  (first col covered, second empty)
    // Row 3: & E & F
    let content = "___TYPST_CELL___:table.cell(rowspan: 3)[A]|||CELL|||B|||CELL|||C|||ROW||| |||CELL||| |||CELL|||D|||ROW||| |||CELL|||E|||CELL|||F";
    let alignments = vec![CellAlign::Center; 3];
    let output = parse_with_grid_parser(content, alignments);

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
    let output = parse_with_grid_parser(content, alignments);

    // Should still produce valid table structure
    assert!(output.contains("table("));
    assert!(output.contains("columns:"));
}
