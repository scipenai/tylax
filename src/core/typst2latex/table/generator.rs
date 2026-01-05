//! State-aware LaTeX table generator

use super::cell::{LatexCell, LatexCellAlign};
use super::hline::LatexHLine;

/// Represents a parsed table row
#[derive(Debug, Clone)]
pub struct LatexRow {
    /// Cells in this row
    pub cells: Vec<LatexCell>,
    /// Horizontal lines before this row
    pub hlines_before: Vec<LatexHLine>,
    /// Whether this is a header row
    pub is_header: bool,
}

impl LatexRow {
    /// Create a new empty row
    pub fn new() -> Self {
        LatexRow {
            cells: Vec::new(),
            hlines_before: Vec::new(),
            is_header: false,
        }
    }

    /// Create a header row
    pub fn header() -> Self {
        LatexRow {
            cells: Vec::new(),
            hlines_before: Vec::new(),
            is_header: true,
        }
    }
}

impl Default for LatexRow {
    fn default() -> Self {
        Self::new()
    }
}

/// State-aware LaTeX table generator
///
/// This generator maintains a virtual grid state to correctly handle complex
/// Typst table features like rowspan, colspan, and partial horizontal lines.
pub struct LatexTableGenerator {
    /// Number of columns
    pub col_count: usize,
    /// Column alignments
    pub col_aligns: Vec<LatexCellAlign>,
    /// Column coverage tracking: remaining rows each column is covered by a rowspan
    col_coverage: Vec<usize>,
    /// Parsed rows
    pub rows: Vec<LatexRow>,
    /// Pending hlines to attach to the next row
    pending_hlines: Vec<LatexHLine>,
    /// Whether to use booktabs style
    pub use_booktabs: bool,
    /// Whether the table has a header
    pub has_header: bool,
    /// Track if we're currently processing header rows
    in_header: bool,
}

impl LatexTableGenerator {
    /// Create a new generator with the given column count and alignments
    pub fn new(col_count: usize, col_aligns: Vec<LatexCellAlign>) -> Self {
        let aligns = if col_aligns.len() >= col_count {
            col_aligns
        } else {
            // Pad with Center alignment
            let mut aligns = col_aligns;
            aligns.resize(col_count, LatexCellAlign::Center);
            aligns
        };

        LatexTableGenerator {
            col_count,
            col_aligns: aligns,
            col_coverage: vec![0; col_count],
            rows: Vec::new(),
            pending_hlines: Vec::new(),
            use_booktabs: false,
            has_header: false,
            in_header: false,
        }
    }

    /// Start processing header rows
    pub fn begin_header(&mut self) {
        self.has_header = true;
        self.in_header = true;
    }

    /// End processing header rows
    pub fn end_header(&mut self) {
        self.in_header = false;
    }

    /// Get the number of columns currently covered by rowspans from previous rows
    /// This is used by the caller to correctly compute when a row is complete
    pub fn get_covered_columns(&self) -> usize {
        self.col_coverage.iter().filter(|&&c| c > 0).count()
    }

    /// Add a horizontal line (will be attached to the next row)
    pub fn add_hline(&mut self, hline: LatexHLine) {
        self.pending_hlines.push(hline);
    }

    /// Add a full horizontal line
    pub fn add_full_hline(&mut self) {
        self.pending_hlines.push(LatexHLine::full());
    }

    /// Process a row of cells from Typst
    ///
    /// This method implements the state machine logic:
    /// 1. For each column position, check if it's covered by a previous rowspan
    /// 2. If covered: output placeholder, decrement coverage
    /// 3. If not covered: take input cell, update coverage if rowspan > 1
    pub fn process_row(&mut self, input_cells: Vec<LatexCell>) {
        let mut row = if self.in_header {
            LatexRow::header()
        } else {
            LatexRow::new()
        };

        // Attach pending hlines
        row.hlines_before.append(&mut self.pending_hlines);

        let mut input_iter = input_cells.into_iter().peekable();
        let mut current_col = 0;

        while current_col < self.col_count {
            // Ensure col_coverage is large enough
            if current_col >= self.col_coverage.len() {
                self.col_coverage.resize(current_col + 1, 0);
            }

            if self.col_coverage[current_col] > 0 {
                // This column is covered by a previous rowspan
                // Output a placeholder cell
                row.cells.push(LatexCell::placeholder());

                // Decrement coverage
                self.col_coverage[current_col] -= 1;

                current_col += 1;
            } else if let Some(cell) = input_iter.next() {
                // Not covered, process the input cell

                // Update coverage for future rows if this cell has rowspan
                if cell.rowspan > 1 {
                    let rows_to_cover = cell.rowspan - 1;

                    // Ensure coverage vec size
                    if current_col + cell.colspan > self.col_coverage.len() {
                        self.col_coverage.resize(current_col + cell.colspan, 0);
                    }

                    // Mark coverage for all columns this cell spans
                    for i in 0..cell.colspan {
                        if current_col + i < self.col_coverage.len() {
                            self.col_coverage[current_col + i] = rows_to_cover;
                        }
                    }
                }

                // Add cell to row
                row.cells.push(cell.clone());

                // Advance by colspan
                current_col += cell.colspan;
            } else {
                // No more input cells, fill with empty
                row.cells.push(LatexCell::new(String::new()));
                current_col += 1;
            }
        }

        // Only add row if it has cells or hlines
        if !row.cells.is_empty() || !row.hlines_before.is_empty() {
            self.rows.push(row);
        }
    }

    /// Generate the complete LaTeX tabular code
    pub fn generate_latex(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        // Build column specification
        let col_spec = self.build_column_spec();
        let _ = writeln!(output, "\\begin{{tabular}}{{{}}}", col_spec);

        // Top line
        if self.use_booktabs {
            let _ = writeln!(output, "\\toprule");
        } else {
            let _ = writeln!(output, "\\hline");
        }

        let mut after_header = false;

        // Output rows
        for (row_idx, row) in self.rows.iter().enumerate() {
            // Emit hlines before this row
            for hline in &row.hlines_before {
                let _ = writeln!(output, "{}", hline.to_latex_with_cols(self.col_count));
            }

            // Emit cells
            if !row.cells.is_empty() {
                let mut first = true;
                output.push_str("  ");

                for (col_idx, cell) in row.cells.iter().enumerate() {
                    if !first {
                        output.push_str(" & ");
                    }
                    first = false;

                    // Get default alignment for this column
                    let default_align = self
                        .col_aligns
                        .get(col_idx)
                        .copied()
                        .unwrap_or(LatexCellAlign::Center);

                    output.push_str(&cell.to_latex(default_align));
                }

                let _ = writeln!(output, " \\\\");

                // Add midrule after header row
                if row.is_header && !after_header {
                    after_header = true;
                    if self.use_booktabs {
                        let _ = writeln!(output, "\\midrule");
                    } else {
                        let _ = writeln!(output, "\\hline");
                    }
                }
            }

            // Add hline after each row (if not booktabs style and not the last row)
            if !self.use_booktabs && row_idx < self.rows.len() - 1 && !row.is_header {
                // Skip - we'll add hlines based on pending_hlines
            }
        }

        // Bottom line
        if self.use_booktabs {
            let _ = writeln!(output, "\\bottomrule");
        } else {
            let _ = writeln!(output, "\\hline");
        }

        let _ = write!(output, "\\end{{tabular}}");

        output
    }

    /// Build the column specification string (e.g., "|l|c|r|")
    fn build_column_spec(&self) -> String {
        let mut spec = String::from("|");

        for align in &self.col_aligns {
            spec.push(align.to_char());
            spec.push('|');
        }

        // If col_aligns is shorter than col_count, fill with 'c'
        for _ in self.col_aligns.len()..self.col_count {
            spec.push('c');
            spec.push('|');
        }

        spec
    }
}
