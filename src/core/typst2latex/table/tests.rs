//! Tests for the Typst to LaTeX table conversion

use super::cell::{LatexCell, LatexCellAlign};
use super::generator::LatexTableGenerator;
use super::hline::LatexHLine;

#[test]
fn test_basic_table() {
    let mut gen = LatexTableGenerator::new(
        3,
        vec![
            LatexCellAlign::Left,
            LatexCellAlign::Center,
            LatexCellAlign::Right,
        ],
    );

    // Row 1
    gen.process_row(vec![
        LatexCell::new("A".to_string()),
        LatexCell::new("B".to_string()),
        LatexCell::new("C".to_string()),
    ]);

    // Row 2
    gen.process_row(vec![
        LatexCell::new("1".to_string()),
        LatexCell::new("2".to_string()),
        LatexCell::new("3".to_string()),
    ]);

    let latex = gen.generate_latex();
    assert!(latex.contains("\\begin{tabular}{|l|c|r|}"));
    assert!(latex.contains("A & B & C"));
    assert!(latex.contains("1 & 2 & 3"));
    assert!(latex.contains("\\end{tabular}"));
}

#[test]
fn test_colspan() {
    let mut gen = LatexTableGenerator::new(
        3,
        vec![
            LatexCellAlign::Center,
            LatexCellAlign::Center,
            LatexCellAlign::Center,
        ],
    );

    // Row with colspan
    gen.process_row(vec![
        LatexCell::with_spans("Header".to_string(), 1, 2),
        LatexCell::new("X".to_string()),
    ]);

    let latex = gen.generate_latex();
    assert!(latex.contains("\\multicolumn{2}{|c|}{Header}"));
}

#[test]
fn test_rowspan() {
    let mut gen = LatexTableGenerator::new(
        3,
        vec![
            LatexCellAlign::Center,
            LatexCellAlign::Center,
            LatexCellAlign::Center,
        ],
    );

    // Row 1 with rowspan
    gen.process_row(vec![
        LatexCell::with_spans("Span".to_string(), 2, 1),
        LatexCell::new("1".to_string()),
        LatexCell::new("2".to_string()),
    ]);

    // Row 2 - first column should be covered
    gen.process_row(vec![
        LatexCell::new("3".to_string()),
        LatexCell::new("4".to_string()),
    ]);

    let latex = gen.generate_latex();
    assert!(latex.contains("\\multirow{2}{*}{Span}"));
    // The second row should have empty first cell (placeholder)
    // Check that we have a row with just " & 3 & 4"
    assert!(latex.contains("& 3 & 4"));
}

#[test]
fn test_colspan_and_rowspan() {
    let mut gen = LatexTableGenerator::new(4, vec![LatexCellAlign::Center; 4]);

    // Row 1: colspan=2, rowspan=2 cell
    let mut cell = LatexCell::with_spans("Big".to_string(), 2, 2);
    cell.align = Some(LatexCellAlign::Center);

    gen.process_row(vec![
        cell,
        LatexCell::new("A".to_string()),
        LatexCell::new("B".to_string()),
    ]);

    // Row 2: first 2 columns covered
    gen.process_row(vec![
        LatexCell::new("C".to_string()),
        LatexCell::new("D".to_string()),
    ]);

    let latex = gen.generate_latex();
    // Should have both multicolumn and multirow
    assert!(latex.contains("\\multicolumn{2}"));
    assert!(latex.contains("\\multirow{2}"));
}

#[test]
fn test_partial_hline() {
    let mut gen = LatexTableGenerator::new(3, vec![LatexCellAlign::Center; 3]);

    gen.process_row(vec![
        LatexCell::new("1".to_string()),
        LatexCell::new("2".to_string()),
        LatexCell::new("3".to_string()),
    ]);

    // Add partial hline
    gen.add_hline(LatexHLine::partial(1, 3));

    gen.process_row(vec![
        LatexCell::new("4".to_string()),
        LatexCell::new("5".to_string()),
        LatexCell::new("6".to_string()),
    ]);

    let latex = gen.generate_latex();
    assert!(latex.contains("\\cline{2-3}"));
}

#[test]
fn test_header_with_booktabs() {
    let mut gen = LatexTableGenerator::new(
        3,
        vec![
            LatexCellAlign::Left,
            LatexCellAlign::Center,
            LatexCellAlign::Right,
        ],
    );
    gen.use_booktabs = true;

    gen.begin_header();
    gen.process_row(vec![
        LatexCell::new("Col1".to_string()),
        LatexCell::new("Col2".to_string()),
        LatexCell::new("Col3".to_string()),
    ]);
    gen.end_header();

    gen.process_row(vec![
        LatexCell::new("A".to_string()),
        LatexCell::new("B".to_string()),
        LatexCell::new("C".to_string()),
    ]);

    let latex = gen.generate_latex();
    assert!(latex.contains("\\toprule"));
    assert!(latex.contains("\\midrule"));
    assert!(latex.contains("\\bottomrule"));
}

#[test]
fn test_hline_to_latex() {
    let full = LatexHLine::full();
    assert_eq!(full.to_latex(), "\\hline");

    let partial = LatexHLine::partial(1, 3);
    assert_eq!(partial.to_latex(), "\\cline{2-3}");

    let top = LatexHLine::top_rule();
    assert_eq!(top.to_latex(), "\\toprule");
}

#[test]
fn test_cell_align() {
    assert_eq!(LatexCellAlign::Left.to_char(), 'l');
    assert_eq!(LatexCellAlign::Center.to_char(), 'c');
    assert_eq!(LatexCellAlign::Right.to_char(), 'r');

    assert_eq!(LatexCellAlign::from_typst("left"), LatexCellAlign::Left);
    assert_eq!(LatexCellAlign::from_typst("CENTER"), LatexCellAlign::Center);
    assert_eq!(LatexCellAlign::from_typst(" right "), LatexCellAlign::Right);
}

#[test]
fn test_cell_fill() {
    let mut cell = LatexCell::new("Content".to_string());
    cell.fill = Some("blue".to_string());

    let latex = cell.to_latex(LatexCellAlign::Center);
    assert!(latex.contains("\\cellcolor{blue}"));
    assert!(latex.contains("Content"));

    let mut cell2 = LatexCell::new("Gray".to_string());
    cell2.fill = Some("silver".to_string()); // silver maps to gray!50
    let latex2 = cell2.to_latex(LatexCellAlign::Center);
    assert!(latex2.contains("\\cellcolor{gray!50}"));
}

#[test]
fn test_nested_table_cell() {
    // Test that a cell containing a nested tabular works
    let nested_tabular = r"\begin{tabular}{|c|c|}
\hline
  a & b \\
\hline
\end{tabular}
";
    let cell = LatexCell::new(nested_tabular.to_string());
    let latex = cell.to_latex(LatexCellAlign::Center);
    // Content should be preserved as-is
    assert!(latex.contains("tabular"));
}

#[test]
fn test_complex_rowspan_colspan() {
    // Test a 4x4 table with complex spanning
    let mut gen = LatexTableGenerator::new(4, vec![LatexCellAlign::Center; 4]);

    // Row 1: Big cell (2x2) + 2 normal cells
    let mut big_cell = LatexCell::with_spans("Big".to_string(), 2, 2);
    big_cell.align = Some(LatexCellAlign::Center);
    gen.process_row(vec![
        big_cell,
        LatexCell::new("A".to_string()),
        LatexCell::new("B".to_string()),
    ]);

    // Row 2: First 2 cols covered, then 2 normal cells
    gen.process_row(vec![
        LatexCell::new("C".to_string()),
        LatexCell::new("D".to_string()),
    ]);

    // Row 3: 4 normal cells
    gen.process_row(vec![
        LatexCell::new("1".to_string()),
        LatexCell::new("2".to_string()),
        LatexCell::new("3".to_string()),
        LatexCell::new("4".to_string()),
    ]);

    let latex = gen.generate_latex();

    // Should have multicolumn wrapping multirow
    assert!(latex.contains("\\multicolumn{2}"));
    assert!(latex.contains("\\multirow{2}"));

    // Second row should have placeholder empty cells
    // The output should contain lines with just " & C & D"
    assert!(latex.contains("C & D"));
}
