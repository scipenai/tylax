//! Robust Table Handling Module
//!
//! This module provides comprehensive table parsing and conversion,
//! inspired by Pandoc's grid table logic. Supports multicolumn, multirow,
//! and various alignment specifications.

#![allow(clippy::while_let_on_iterator)]

/// Cell alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Default,
    Left,
    Center,
    Right,
}

impl Alignment {
    /// Parse from LaTeX alignment character
    pub fn from_latex_char(c: char) -> Self {
        match c {
            'l' => Alignment::Left,
            'c' => Alignment::Center,
            'r' => Alignment::Right,
            'p' | 'm' | 'b' | 'X' => Alignment::Left, // paragraph types
            _ => Alignment::Default,
        }
    }

    /// Convert to LaTeX alignment character
    pub fn to_latex_char(&self) -> char {
        match self {
            Alignment::Left | Alignment::Default => 'l',
            Alignment::Center => 'c',
            Alignment::Right => 'r',
        }
    }

    /// Convert to Typst alignment
    pub fn to_typst(&self) -> &'static str {
        match self {
            Alignment::Left | Alignment::Default => "left",
            Alignment::Center => "center",
            Alignment::Right => "right",
        }
    }
}

/// Column width specification
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ColWidth {
    /// Auto-determined width
    #[default]
    Auto,
    /// Fixed width (in some unit, normalized to 0-1 fraction)
    Fixed(f64),
    /// Percentage of table width
    Percent(f64),
}

/// Column specification
#[derive(Debug, Clone, Default)]
pub struct ColSpec {
    pub alignment: Alignment,
    pub width: ColWidth,
    pub has_left_border: bool,
    pub has_right_border: bool,
}

/// A single table cell
#[derive(Debug, Clone)]
pub struct Cell {
    /// Cell content (can contain formatted text, math, etc.)
    pub content: String,
    /// Number of columns this cell spans
    pub colspan: u32,
    /// Number of rows this cell spans
    pub rowspan: u32,
    /// Cell-specific alignment (overrides column default)
    pub alignment: Option<Alignment>,
}

impl Cell {
    pub fn new(content: String) -> Self {
        Self {
            content,
            colspan: 1,
            rowspan: 1,
            alignment: None,
        }
    }

    pub fn with_span(content: String, colspan: u32, rowspan: u32) -> Self {
        Self {
            content,
            colspan,
            rowspan,
            alignment: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::new(String::new())
    }
}

/// A table row
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// Whether this row has a bottom border (hline)
    pub has_bottom_border: bool,
}

impl Row {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, cell: Cell) {
        self.cells.push(cell);
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() || self.cells.iter().all(|c| c.is_empty())
    }
}

/// Table caption
#[derive(Debug, Clone, Default)]
pub struct Caption {
    pub short: Option<String>,
    pub long: String,
}

/// Complete table structure
#[derive(Debug, Clone)]
pub struct Table {
    /// Column specifications
    pub colspecs: Vec<ColSpec>,
    /// Header rows
    pub header: Vec<Row>,
    /// Body rows
    pub body: Vec<Row>,
    /// Footer rows
    pub footer: Vec<Row>,
    /// Table caption
    pub caption: Option<Caption>,
    /// Table label for cross-referencing
    pub label: Option<String>,
    /// Whether the table has a top border
    pub has_top_border: bool,
}

impl Table {
    pub fn new(num_cols: usize) -> Self {
        Self {
            colspecs: vec![ColSpec::default(); num_cols],
            header: Vec::new(),
            body: Vec::new(),
            footer: Vec::new(),
            caption: None,
            label: None,
            has_top_border: false,
        }
    }

    pub fn num_cols(&self) -> usize {
        self.colspecs.len()
    }

    pub fn push_header(&mut self, row: Row) {
        self.header.push(row);
    }

    pub fn push_body(&mut self, row: Row) {
        self.body.push(row);
    }

    pub fn push_footer(&mut self, row: Row) {
        self.footer.push(row);
    }
}

// ============================================================================
// LaTeX Table Parsing
// ============================================================================

/// Parse LaTeX table environment
pub fn parse_latex_table(input: &str) -> Option<Table> {
    // Detect table environment type
    let is_tabular = input.contains("\\begin{tabular}");
    let is_longtable = input.contains("\\begin{longtable}");
    let is_tabularx = input.contains("\\begin{tabularx}");

    if !is_tabular && !is_longtable && !is_tabularx {
        return None;
    }

    // Extract column specification
    let colspecs = extract_colspecs(input)?;
    let mut table = Table::new(colspecs.len());
    table.colspecs = colspecs;

    // Extract caption if present
    if let Some(caption) = extract_caption(input) {
        table.caption = Some(caption);
    }

    // Extract label if present
    if let Some(label) = extract_label(input) {
        table.label = Some(label);
    }

    // Parse rows
    let content = extract_table_content(input);
    let rows = parse_rows(&content, table.num_cols());

    // Determine header vs body (first row after hline is typically header)
    let mut in_header = true;
    for row in rows {
        if in_header && (row.has_bottom_border || table.header.is_empty()) {
            table.push_header(row);
            in_header = false;
        } else {
            table.push_body(row);
        }
    }

    Some(table)
}

/// Extract column specifications from LaTeX
fn extract_colspecs(input: &str) -> Option<Vec<ColSpec>> {
    // Find the alignment string {|c|c|c|} or similar
    let begin_pattern = if input.contains("\\begin{tabularx}") {
        "\\begin{tabularx}"
    } else if input.contains("\\begin{longtable}") {
        "\\begin{longtable}"
    } else {
        "\\begin{tabular}"
    };

    let start = input.find(begin_pattern)? + begin_pattern.len();

    // Skip width argument for tabularx
    let rest = &input[start..];
    let rest = if begin_pattern == "\\begin{tabularx}" {
        // Skip {width}
        skip_braced_arg(rest)
    } else {
        rest.trim_start()
    };

    // Find the column spec in braces
    if !rest.starts_with('{') {
        return None;
    }

    let end = find_matching_brace(rest)?;
    let spec_str = &rest[1..end];

    let mut colspecs: Vec<ColSpec> = Vec::new();
    let mut has_left_border = false;
    let mut chars = spec_str.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '|' => {
                if colspecs.is_empty() {
                    has_left_border = true;
                } else if let Some(last) = colspecs.last_mut() {
                    last.has_right_border = true;
                }
            }
            'l' | 'c' | 'r' => {
                let spec = ColSpec {
                    alignment: Alignment::from_latex_char(c),
                    has_left_border,
                    ..Default::default()
                };
                has_left_border = false;
                colspecs.push(spec);
            }
            'p' | 'm' | 'b' => {
                // Skip width specification
                if chars.peek() == Some(&'{') {
                    skip_braced_content(&mut chars);
                }
                let spec = ColSpec {
                    alignment: Alignment::Left,
                    has_left_border,
                    ..Default::default()
                };
                has_left_border = false;
                colspecs.push(spec);
            }
            'X' => {
                // tabularx X column
                let spec = ColSpec {
                    alignment: Alignment::Left,
                    width: ColWidth::Fixed(1.0), // Equal distribution
                    has_left_border,
                    ..Default::default()
                };
                has_left_border = false;
                colspecs.push(spec);
            }
            '*' => {
                // *{n}{spec} - repeat specification
                if chars.peek() == Some(&'{') {
                    if let Some(count) = extract_repeat_count(&mut chars) {
                        if let Some(repeat_spec) = extract_repeat_spec(&mut chars) {
                            for _ in 0..count {
                                // Parse the repeated spec
                                for rc in repeat_spec.chars() {
                                    if rc == '|' {
                                        if colspecs.is_empty() {
                                            has_left_border = true;
                                        } else if let Some(last) = colspecs.last_mut() {
                                            last.has_right_border = true;
                                        }
                                    } else if "lcr".contains(rc) {
                                        let spec = ColSpec {
                                            alignment: Alignment::from_latex_char(rc),
                                            has_left_border,
                                            ..Default::default()
                                        };
                                        has_left_border = false;
                                        colspecs.push(spec);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            '@' | '>' | '<' | '!' => {
                // Skip these specifications
                if chars.peek() == Some(&'{') {
                    skip_braced_content(&mut chars);
                }
            }
            _ => {}
        }
    }

    if colspecs.is_empty() {
        None
    } else {
        Some(colspecs)
    }
}

/// Skip a braced argument and return the rest
fn skip_braced_arg(s: &str) -> &str {
    let s = s.trim_start();
    if !s.starts_with('{') {
        return s;
    }

    if let Some(end) = find_matching_brace(s) {
        &s[end + 1..]
    } else {
        s
    }
}

/// Find the position of the matching closing brace
fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Skip braced content in a char iterator
fn skip_braced_content(chars: &mut std::iter::Peekable<std::str::Chars>) {
    if chars.peek() != Some(&'{') {
        return;
    }
    chars.next(); // consume '{'
    let mut depth = 1;
    while let Some(c) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
}

/// Extract repeat count from *{n}
fn extract_repeat_count(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<usize> {
    if chars.peek() != Some(&'{') {
        return None;
    }
    chars.next(); // consume '{'

    let mut num_str = String::new();
    while let Some(&c) = chars.peek() {
        if c == '}' {
            chars.next();
            break;
        }
        num_str.push(c);
        chars.next();
    }

    num_str.trim().parse().ok()
}

/// Extract repeat spec from *{n}{spec}
fn extract_repeat_spec(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    if chars.peek() != Some(&'{') {
        return None;
    }
    chars.next(); // consume '{'

    let mut spec = String::new();
    let mut depth = 1;
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                depth += 1;
                spec.push(c);
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                spec.push(c);
            }
            _ => spec.push(c),
        }
    }

    Some(spec)
}

/// Extract caption from LaTeX table
fn extract_caption(input: &str) -> Option<Caption> {
    if let Some(start) = input.find("\\caption") {
        let rest = &input[start + "\\caption".len()..];

        // Check for short caption
        let (short, rest) = if rest.trim_start().starts_with('[') {
            let trimmed = rest.trim_start();
            if let Some(end) = trimmed.find(']') {
                (Some(trimmed[1..end].to_string()), &trimmed[end + 1..])
            } else {
                (None, rest)
            }
        } else {
            (None, rest)
        };

        // Get long caption
        let rest = rest.trim_start();
        if rest.starts_with('{') {
            if let Some(end) = find_matching_brace(rest) {
                return Some(Caption {
                    short,
                    long: rest[1..end].to_string(),
                });
            }
        }
    }
    None
}

/// Extract label from LaTeX table
fn extract_label(input: &str) -> Option<String> {
    if let Some(start) = input.find("\\label{") {
        let rest = &input[start + "\\label{".len()..];
        if let Some(end) = rest.find('}') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Extract table content between begin and end
fn extract_table_content(input: &str) -> String {
    // Find content between \begin{...} and \end{...}
    let env_patterns = [
        ("\\begin{tabular}", "\\end{tabular}"),
        ("\\begin{tabularx}", "\\end{tabularx}"),
        ("\\begin{longtable}", "\\end{longtable}"),
    ];

    for (begin, end) in env_patterns {
        if let Some(begin_pos) = input.find(begin) {
            // Find the end of the begin statement (after column spec)
            let after_begin = &input[begin_pos + begin.len()..];
            // Skip optional arguments and column spec
            let after_spec = if let Some(brace_start) = after_begin.find('{') {
                let from_brace = &after_begin[brace_start..];
                if let Some(brace_end) = find_matching_brace(from_brace) {
                    &after_begin[brace_start + brace_end + 1..]
                } else {
                    after_begin
                }
            } else {
                after_begin
            };

            if let Some(end_pos) = after_spec.find(end) {
                return after_spec[..end_pos].to_string();
            }
        }
    }

    input.to_string()
}

/// Parse table rows from content
fn parse_rows(content: &str, _num_cols: usize) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut current_row = Row::new();
    let mut current_cell = String::new();
    let mut in_brace = 0;

    let lines: Vec<&str> = content.lines().collect();

    for line in lines {
        let line = line.trim();

        // Skip hline/cline commands but track them
        if line.starts_with("\\hline")
            || line.starts_with("\\toprule")
            || line.starts_with("\\midrule")
            || line.starts_with("\\bottomrule")
            || line.starts_with("\\cmidrule")
        {
            if !current_row.is_empty() {
                current_row.has_bottom_border = true;
            } else if let Some(last) = rows.last_mut() {
                last.has_bottom_border = true;
            }
            continue;
        }

        // Skip endhead, endfirsthead for longtable
        if line.starts_with("\\endhead") || line.starts_with("\\endfirsthead") {
            continue;
        }

        // Parse row content
        for c in line.chars() {
            match c {
                '{' => {
                    in_brace += 1;
                    current_cell.push(c);
                }
                '}' => {
                    in_brace -= 1;
                    current_cell.push(c);
                }
                '&' if in_brace == 0 => {
                    // Cell separator
                    let cell = parse_cell(current_cell.trim());
                    current_row.push(cell);
                    current_cell.clear();
                }
                _ => {
                    current_cell.push(c);
                }
            }
        }

        // Check for row end
        if line.ends_with("\\\\") || line.contains("\\\\") {
            // Remove trailing \\
            let cell_content = current_cell
                .trim()
                .trim_end_matches("\\\\")
                .trim_end_matches('\\')
                .trim();
            if !cell_content.is_empty() || !current_row.cells.is_empty() {
                let cell = parse_cell(cell_content);
                current_row.push(cell);
            }

            if !current_row.is_empty() {
                rows.push(current_row);
            }
            current_row = Row::new();
            current_cell.clear();
        }
    }

    // Handle last row without \\
    if !current_cell.trim().is_empty() {
        let cell = parse_cell(current_cell.trim());
        current_row.push(cell);
    }
    if !current_row.is_empty() {
        rows.push(current_row);
    }

    rows
}

/// Parse a single cell, handling multicolumn/multirow
fn parse_cell(content: &str) -> Cell {
    let content = content.trim();

    // Check for \multicolumn{n}{align}{content}
    if content.starts_with("\\multicolumn{") {
        return parse_multicolumn(content);
    }

    // Check for \multirow{n}{width}{content}
    if content.starts_with("\\multirow{") {
        return parse_multirow(content);
    }

    Cell::new(content.to_string())
}

/// Parse multicolumn cell
fn parse_multicolumn(content: &str) -> Cell {
    let rest = &content["\\multicolumn{".len()..];

    // Get colspan
    let colspan = if let Some(end) = rest.find('}') {
        rest[..end].parse().unwrap_or(1)
    } else {
        return Cell::new(content.to_string());
    };

    // Skip to alignment spec
    let rest = &rest[rest.find('}').unwrap_or(0) + 1..];
    let rest = rest.trim_start();

    // Get alignment
    let alignment = if rest.starts_with('{') {
        if let Some(end) = find_matching_brace(rest) {
            let align_str = &rest[1..end];
            Some(Alignment::from_latex_char(
                align_str
                    .chars()
                    .find(|c| "lcr".contains(*c))
                    .unwrap_or('c'),
            ))
        } else {
            None
        }
    } else {
        None
    };

    // Skip to content
    let rest = if rest.starts_with('{') {
        if let Some(end) = find_matching_brace(rest) {
            &rest[end + 1..]
        } else {
            rest
        }
    } else {
        rest
    };

    // Get content
    let cell_content = if rest.trim_start().starts_with('{') {
        let trimmed = rest.trim_start();
        if let Some(end) = find_matching_brace(trimmed) {
            trimmed[1..end].to_string()
        } else {
            rest.to_string()
        }
    } else {
        rest.to_string()
    };

    let mut cell = Cell::with_span(cell_content, colspan, 1);
    cell.alignment = alignment;
    cell
}

/// Parse multirow cell
fn parse_multirow(content: &str) -> Cell {
    let rest = &content["\\multirow{".len()..];

    // Get rowspan
    let rowspan = if let Some(end) = rest.find('}') {
        rest[..end].parse().unwrap_or(1)
    } else {
        return Cell::new(content.to_string());
    };

    // Skip width argument
    let rest = &rest[rest.find('}').unwrap_or(0) + 1..];
    let rest = if rest.trim_start().starts_with('{') {
        skip_braced_arg(rest)
    } else {
        rest
    };

    // Get content
    let cell_content = if rest.trim_start().starts_with('{') {
        let trimmed = rest.trim_start();
        if let Some(end) = find_matching_brace(trimmed) {
            trimmed[1..end].to_string()
        } else {
            rest.to_string()
        }
    } else {
        rest.to_string()
    };

    Cell::with_span(cell_content, 1, rowspan)
}

// ============================================================================
// Typst Table Parsing
// ============================================================================

/// Parse Typst table
pub fn parse_typst_table(input: &str) -> Option<Table> {
    if !input.contains("table(") && !input.contains("#table(") {
        return None;
    }

    // This is a simplified parser for common Typst table patterns
    // Full parsing would require proper Typst syntax parsing

    // Try to extract columns count (default to 2)
    let num_cols = extract_typst_columns(input).unwrap_or(2);

    let mut table = Table::new(num_cols);

    // Extract cell contents
    if let Some(cells) = extract_typst_cells(input) {
        let mut current_row = Row::new();
        for (i, cell_content) in cells.iter().enumerate() {
            current_row.push(Cell::new(cell_content.clone()));

            if (i + 1) % num_cols == 0 {
                table.push_body(current_row);
                current_row = Row::new();
            }
        }

        if !current_row.is_empty() {
            table.push_body(current_row);
        }
    }

    Some(table)
}

/// Extract number of columns from Typst table
fn extract_typst_columns(input: &str) -> Option<usize> {
    // Look for columns: n or columns: (...)
    if let Some(start) = input.find("columns:") {
        let rest = &input[start + "columns:".len()..];
        let rest = rest.trim_start();

        // Check for simple number
        if let Some(c) = rest.chars().next() {
            if c.is_ascii_digit() {
                let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                return num_str.parse().ok();
            }

            // Check for tuple (auto, auto, ...)
            if c == '(' {
                let mut count = 0;
                let mut depth = 0;
                for c in rest.chars() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                count += 1;
                                break;
                            }
                        }
                        ',' if depth == 1 => count += 1,
                        _ => {}
                    }
                }
                return Some(count);
            }
        }
    }

    None
}

/// Extract cell contents from Typst table
fn extract_typst_cells(input: &str) -> Option<Vec<String>> {
    // Find the table content
    let start = input.find("table(")? + "table(".len();
    let rest = &input[start..];

    // Find matching closing paren
    let mut depth = 1;
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let content = &rest[..end];

    // Extract bracketed content [...]
    let mut cells = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = content.chars().collect();

    while i < chars.len() {
        if chars[i] == '[' {
            let mut cell = String::new();
            let mut bracket_depth = 1;
            i += 1;

            while i < chars.len() && bracket_depth > 0 {
                if chars[i] == '[' {
                    bracket_depth += 1;
                } else if chars[i] == ']' {
                    bracket_depth -= 1;
                    if bracket_depth == 0 {
                        break;
                    }
                }
                cell.push(chars[i]);
                i += 1;
            }

            cells.push(cell.trim().to_string());
        }
        i += 1;
    }

    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

// ============================================================================
// Table to LaTeX Conversion
// ============================================================================

/// Convert Table to LaTeX
pub fn table_to_latex(table: &Table) -> String {
    let mut output = String::new();

    // Begin table environment if we have caption
    if table.caption.is_some() {
        output.push_str("\\begin{table}[htbp]\n");
        output.push_str("\\centering\n");
    }

    // Build column spec
    let col_spec: String = table
        .colspecs
        .iter()
        .map(|spec| {
            let mut s = String::new();
            if spec.has_left_border {
                s.push('|');
            }
            s.push(spec.alignment.to_latex_char());
            if spec.has_right_border {
                s.push('|');
            }
            s
        })
        .collect();

    output.push_str(&format!("\\begin{{tabular}}{{{}}}\n", col_spec));

    if table.has_top_border {
        output.push_str("\\hline\n");
    }

    // Header rows
    for row in &table.header {
        output.push_str(&row_to_latex(row));
        if row.has_bottom_border {
            output.push_str("\\hline\n");
        }
    }

    // Body rows
    for row in &table.body {
        output.push_str(&row_to_latex(row));
        if row.has_bottom_border {
            output.push_str("\\hline\n");
        }
    }

    // Footer rows
    for row in &table.footer {
        output.push_str(&row_to_latex(row));
        if row.has_bottom_border {
            output.push_str("\\hline\n");
        }
    }

    output.push_str("\\end{tabular}\n");

    // Caption and label
    if let Some(ref caption) = table.caption {
        if let Some(ref short) = caption.short {
            output.push_str(&format!("\\caption[{}]{{{}}}\n", short, caption.long));
        } else {
            output.push_str(&format!("\\caption{{{}}}\n", caption.long));
        }
    }

    if let Some(ref label) = table.label {
        output.push_str(&format!("\\label{{{}}}\n", label));
    }

    if table.caption.is_some() {
        output.push_str("\\end{table}\n");
    }

    output
}

/// Convert a row to LaTeX
fn row_to_latex(row: &Row) -> String {
    let cells: Vec<String> = row.cells.iter().map(cell_to_latex).collect();

    format!("{} \\\\\n", cells.join(" & "))
}

/// Convert a cell to LaTeX
fn cell_to_latex(cell: &Cell) -> String {
    if cell.colspan > 1 {
        let align = cell.alignment.unwrap_or(Alignment::Center);
        format!(
            "\\multicolumn{{{}}}{{{}}}{{{}}}",
            cell.colspan,
            align.to_latex_char(),
            cell.content
        )
    } else if cell.rowspan > 1 {
        format!("\\multirow{{{}}}{{*}}{{{}}}", cell.rowspan, cell.content)
    } else {
        cell.content.clone()
    }
}

// ============================================================================
// Table to Typst Conversion
// ============================================================================

/// Convert Table to Typst
pub fn table_to_typst(table: &Table) -> String {
    let mut output = String::new();

    // Figure wrapper if we have caption
    if table.caption.is_some() {
        output.push_str("#figure(\n");
    }

    // Table
    output.push_str("  table(\n");

    // Columns
    let _cols = table.num_cols();
    let widths: Vec<String> = table
        .colspecs
        .iter()
        .map(|spec| match spec.width {
            ColWidth::Auto => "auto".to_string(),
            ColWidth::Fixed(w) => format!("{}%", w * 100.0),
            ColWidth::Percent(p) => format!("{}%", p),
        })
        .collect();

    output.push_str(&format!("    columns: ({}),\n", widths.join(", ")));

    // Alignment
    let aligns: Vec<&str> = table
        .colspecs
        .iter()
        .map(|spec| spec.alignment.to_typst())
        .collect();
    output.push_str(&format!("    align: ({}),\n", aligns.join(", ")));

    // Stroke for borders
    if table.has_top_border
        || table
            .colspecs
            .iter()
            .any(|s| s.has_left_border || s.has_right_border)
    {
        output.push_str("    stroke: 0.5pt,\n");
    }

    // Header rows
    if !table.header.is_empty() {
        output.push_str("    table.header(\n");
        for row in &table.header {
            output.push_str(&row_to_typst(row));
        }
        output.push_str("    ),\n");
    }

    // Body rows
    for row in &table.body {
        output.push_str(&row_to_typst(row));
    }

    // Footer rows
    if !table.footer.is_empty() {
        output.push_str("    table.footer(\n");
        for row in &table.footer {
            output.push_str(&row_to_typst(row));
        }
        output.push_str("    ),\n");
    }

    output.push_str("  )");

    // Caption
    if let Some(ref caption) = table.caption {
        output.push_str(",\n");
        output.push_str(&format!("  caption: [{}]\n", caption.long));
        output.push(')');

        // Label
        if let Some(ref label) = table.label {
            output.push_str(&format!(" <{}>", label));
        }
    }

    output.push('\n');
    output
}

/// Convert a row to Typst
fn row_to_typst(row: &Row) -> String {
    let cells: Vec<String> = row.cells.iter().map(cell_to_typst).collect();

    format!("      {},\n", cells.join(", "))
}

/// Convert a cell to Typst
fn cell_to_typst(cell: &Cell) -> String {
    if cell.colspan > 1 || cell.rowspan > 1 {
        let mut args = Vec::new();
        if cell.colspan > 1 {
            args.push(format!("colspan: {}", cell.colspan));
        }
        if cell.rowspan > 1 {
            args.push(format!("rowspan: {}", cell.rowspan));
        }
        format!("table.cell({})[{}]", args.join(", "), cell.content)
    } else {
        format!("[{}]", cell.content)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_table() {
        let input = r#"
\begin{tabular}{|c|c|c|}
\hline
A & B & C \\
\hline
1 & 2 & 3 \\
4 & 5 & 6 \\
\hline
\end{tabular}
"#;
        let table = parse_latex_table(input).unwrap();
        assert_eq!(table.num_cols(), 3);
        assert!(!table.header.is_empty());
    }

    #[test]
    fn test_colspec_parsing() {
        let specs = extract_colspecs("\\begin{tabular}{|l|c|r|}").unwrap();
        assert_eq!(specs.len(), 3);
        assert_eq!(specs[0].alignment, Alignment::Left);
        assert_eq!(specs[1].alignment, Alignment::Center);
        assert_eq!(specs[2].alignment, Alignment::Right);
    }

    #[test]
    fn test_multicolumn() {
        let cell = parse_multicolumn("\\multicolumn{3}{c}{Merged}");
        assert_eq!(cell.colspan, 3);
        assert_eq!(cell.alignment, Some(Alignment::Center));
    }

    #[test]
    fn test_table_to_latex() {
        let mut table = Table::new(2);
        table.colspecs[0].alignment = Alignment::Left;
        table.colspecs[1].alignment = Alignment::Right;

        let mut row = Row::new();
        row.push(Cell::new("A".to_string()));
        row.push(Cell::new("B".to_string()));
        table.push_body(row);

        let latex = table_to_latex(&table);
        assert!(latex.contains("\\begin{tabular}{lr}"));
        assert!(latex.contains("A & B"));
    }

    #[test]
    fn test_table_to_typst() {
        let mut table = Table::new(2);

        let mut row = Row::new();
        row.push(Cell::new("A".to_string()));
        row.push(Cell::new("B".to_string()));
        table.push_body(row);

        let typst = table_to_typst(&table);
        assert!(typst.contains("table("));
        assert!(typst.contains("columns:"));
        assert!(typst.contains("[A]"));
    }

    #[test]
    fn test_caption_extraction() {
        let input = r#"\caption[Short]{Long caption}"#;
        let caption = extract_caption(input).unwrap();
        assert_eq!(caption.short, Some("Short".to_string()));
        assert_eq!(caption.long, "Long caption");
    }

    #[test]
    fn test_colspan_cell() {
        let cell = Cell::with_span("Merged".to_string(), 3, 1);
        let latex = cell_to_latex(&cell);
        assert!(latex.contains("\\multicolumn{3}"));

        let typst = cell_to_typst(&cell);
        assert!(typst.contains("colspan: 3"));
    }

    #[test]
    fn test_rowspan_cell() {
        let cell = Cell::with_span("Vertical".to_string(), 1, 2);
        let latex = cell_to_latex(&cell);
        assert!(latex.contains("\\multirow{2}"));

        let typst = cell_to_typst(&cell);
        assert!(typst.contains("rowspan: 2"));
    }
}
