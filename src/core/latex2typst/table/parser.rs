//! State-aware table grid parser

use super::cell::{CellAlign, GridCell};
use super::hline::{clean_cell_content, clean_hline_args, extract_hline_range, HLine};

/// Represents a parsed table row
#[derive(Debug, Clone)]
pub struct GridRow {
    /// Cells in this row
    pub cells: Vec<GridCell>,
    /// Horizontal lines before this row
    pub hlines_before: Vec<HLine>,
}

impl GridRow {
    /// Create a new empty row
    pub fn new() -> Self {
        GridRow {
            cells: Vec::new(),
            hlines_before: Vec::new(),
        }
    }
}

impl Default for GridRow {
    fn default() -> Self {
        Self::new()
    }
}

/// State-aware table grid parser
///
/// This parser maintains a virtual grid state to correctly handle complex
/// LaTeX table features like multirow, multicolumn, and sparse data.
pub struct TableGridParser {
    /// Column coverage tracking: remaining rows each column is covered by a multirow
    col_coverage: Vec<usize>,
    /// Parsed rows
    pub rows: Vec<GridRow>,
    /// Default column alignments from \begin{tabular}{...}
    pub default_alignments: Vec<CellAlign>,
    /// Pending hlines to attach to the next row
    pending_hlines: Vec<HLine>,
}

impl TableGridParser {
    /// Create a new parser with the given default alignments
    pub fn new(alignments: Vec<CellAlign>) -> Self {
        TableGridParser {
            col_coverage: Vec::new(),
            rows: Vec::new(),
            default_alignments: alignments,
            pending_hlines: Vec::new(),
        }
    }

    /// Add a full horizontal line
    pub fn add_hline(&mut self) {
        self.pending_hlines.push(HLine::full());
    }

    /// Add a partial horizontal line (cline/cmidrule)
    pub fn add_partial_hline(&mut self, start: usize, end: usize) {
        self.pending_hlines.push(HLine::partial(start, end));
    }

    /// Process a row of raw cells
    pub fn process_row(&mut self, raw_cells: Vec<String>) {
        let mut row = GridRow::new();

        // Attach pending hlines
        row.hlines_before.append(&mut self.pending_hlines);

        let mut input_idx = 0;
        let mut current_col = 0;

        while input_idx < raw_cells.len() {
            // Ensure col_coverage is large enough
            if current_col >= self.col_coverage.len() {
                self.col_coverage.resize(current_col + 1, 0);
            }

            if self.col_coverage[current_col] > 0 {
                // Column is covered by a previous multirow.
                // Check if the input contains a multi-column placeholder (e.g., \multicolumn{2}{c}{}).
                // We must advance current_col by the placeholder's span to correctly align subsequent data.
                let raw = &raw_cells[input_idx];
                let cell = GridCell::parse(raw);

                // Decrement coverage for active columns.
                // If a column wasn't covered but is spanned by the placeholder, we ignore it
                // as it suggests a malformed table structure.
                let span = cell.colspan;

                for i in 0..span {
                    if current_col + i < self.col_coverage.len()
                        && self.col_coverage[current_col + i] > 0
                    {
                        self.col_coverage[current_col + i] -= 1;
                    }
                }

                // Consume the placeholder cell from input
                // but DO NOT emit a cell - Typst handles the spanned area
                input_idx += 1;
                current_col += span;
            } else {
                // Not covered, process the input cell
                let raw = &raw_cells[input_idx];
                let cell = GridCell::parse(raw);

                // Update coverage for future rows
                let rows_to_cover = cell.rowspan.saturating_sub(1);

                // Ensure coverage vec size
                if current_col + cell.colspan > self.col_coverage.len() {
                    self.col_coverage.resize(current_col + cell.colspan, 0);
                }

                // Mark coverage for all columns this cell spans
                for i in 0..cell.colspan {
                    self.col_coverage[current_col + i] = rows_to_cover;
                }

                // Add cell to row (handle backslash artifacts)
                if raw != "\\" {
                    row.cells.push(cell.clone());
                } else {
                    row.cells.push(GridCell::empty());
                }

                input_idx += 1;
                current_col += cell.colspan;
            }
        }

        if !row.cells.is_empty() || !row.hlines_before.is_empty() {
            self.rows.push(row);
        }
    }

    /// Generate Typst table code
    pub fn generate_typst(&self, col_count: usize) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Generate columns spec
        let col_tuple: Vec<&str> = vec!["auto"; col_count.max(1)];
        let _ = writeln!(output, "#table(");
        let _ = writeln!(output, "    columns: ({}),", col_tuple.join(", "));

        // Generate alignment spec
        if !self.default_alignments.is_empty() {
            let aligns: Vec<&str> = self
                .default_alignments
                .iter()
                .map(|a| a.to_typst())
                .collect();
            let _ = writeln!(output, "    align: ({}),", aligns.join(", "));
        }

        // Generate rows
        for row in &self.rows {
            // Emit hlines before this row
            for hline in &row.hlines_before {
                let _ = writeln!(output, "    {},", hline.to_typst());
            }

            // Emit cells
            if !row.cells.is_empty() {
                let cells_str: Vec<String> = row.cells.iter().map(|c| c.to_typst()).collect();
                let _ = writeln!(output, "    {},", cells_str.join(", "));
            }
        }

        // Emit any remaining pending hlines
        for hline in &self.pending_hlines {
            let _ = writeln!(output, "    {},", hline.to_typst());
        }

        output.push_str(")\n");
        output
    }
}

/// Parse table content using the state-aware TableGridParser
pub fn parse_with_grid_parser(content: &str, alignments: Vec<CellAlign>) -> String {
    let col_count = alignments.len().max(1);
    let mut parser = TableGridParser::new(alignments);

    for row_str in content.split("|||ROW|||") {
        let row_str = row_str.trim();
        if row_str.is_empty() {
            continue;
        }

        // Check for HLINE markers and extract partial line info
        if row_str.contains("|||HLINE|||") {
            let hline_info = extract_hline_range(row_str);
            match hline_info {
                Some((start, end)) => parser.add_partial_hline(start, end),
                None => parser.add_hline(),
            }
        }

        // Remove HLINE marker to process content
        let clean_row = row_str.replace("|||HLINE|||", "");
        let clean_row = clean_hline_args(&clean_row);

        if clean_row.trim().is_empty() {
            continue;
        }

        // Split into cells and clean each one
        let raw_cells: Vec<String> = clean_row
            .split("|||CELL|||")
            .map(clean_cell_content)
            .collect();

        parser.process_row(raw_cells);
    }

    // Handle single row without ROW markers (edge case)
    if parser.rows.is_empty() && content.contains("|||CELL|||") {
        let clean_content = content.replace("|||HLINE|||", "");
        let raw_cells: Vec<String> = clean_content
            .split("|||CELL|||")
            .map(clean_cell_content)
            .collect();
        parser.process_row(raw_cells);
    }

    parser.generate_typst(col_count)
}
