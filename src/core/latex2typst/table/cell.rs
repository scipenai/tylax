//! Cell types and alignment for table parsing

/// Cell alignment options
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CellAlign {
    Left,
    Center,
    Right,
    Auto,
}

impl CellAlign {
    /// Convert to Typst alignment string
    pub fn to_typst(&self) -> &'static str {
        match self {
            CellAlign::Left => "left",
            CellAlign::Center => "center",
            CellAlign::Right => "right",
            CellAlign::Auto => "auto",
        }
    }
}

/// Represents a single table cell with span and alignment info
#[derive(Debug, Clone)]
pub struct GridCell {
    /// Cell content (inner content, without table.cell wrapper)
    pub content: String,
    /// Number of rows this cell spans
    pub rowspan: usize,
    /// Number of columns this cell spans
    pub colspan: usize,
    /// Optional cell-specific alignment (from \multicolumn)
    pub align: Option<CellAlign>,
    /// Whether this cell has special properties (needs table.cell)
    pub is_special: bool,
}

impl GridCell {
    /// Create a new cell with content
    pub fn new(content: String) -> Self {
        GridCell {
            content,
            rowspan: 1,
            colspan: 1,
            align: None,
            is_special: false,
        }
    }

    /// Create an empty cell
    pub fn empty() -> Self {
        GridCell::new(String::new())
    }

    /// Parse a raw cell string and extract span/alignment info (recursively)
    pub fn parse(raw: &str) -> Self {
        // Base case: raw string is content
        let mut cell = GridCell::new(raw.to_string());

        // Check if it's a special cell marker
        if let Some(start_idx) = raw.find("___TYPST_CELL___:") {
            cell.is_special = true;
            let marker_content = &raw[start_idx..];

            // Parse attributes from the current layer
            if let Some(idx) = marker_content.find("rowspan:") {
                let rest = &marker_content[idx + 8..];
                let num_str: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || c.is_whitespace())
                    .collect();
                if let Ok(n) = num_str.trim().parse::<usize>() {
                    cell.rowspan = n;
                }
            }

            if let Some(idx) = marker_content.find("colspan:") {
                let rest = &marker_content[idx + 8..];
                let num_str: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || c.is_whitespace())
                    .collect();
                if let Ok(n) = num_str.trim().parse::<usize>() {
                    cell.colspan = n;
                }
            }

            if let Some(idx) = marker_content.find("align:") {
                let rest = &marker_content[idx + 6..].trim_start();
                if rest.starts_with("left") {
                    cell.align = Some(CellAlign::Left);
                } else if rest.starts_with("center") {
                    cell.align = Some(CellAlign::Center);
                } else if rest.starts_with("right") {
                    cell.align = Some(CellAlign::Right);
                }
            }

            // Extract inner content: table.cell(...)[INNER]
            // We look for the first '[' and the matching closing ']'
            if let Some(content_start) = marker_content.find('[') {
                let mut depth = 1;
                let mut content_end = content_start;
                let chars: Vec<char> = marker_content.chars().collect();

                for (i, &ch) in chars.iter().enumerate().skip(content_start + 1) {
                    if ch == '[' {
                        depth += 1;
                    } else if ch == ']' {
                        depth -= 1;
                        if depth == 0 {
                            content_end = i;
                            break;
                        }
                    }
                }

                if content_end > content_start {
                    let inner_raw = &marker_content[content_start + 1..content_end];

                    // Recursively parse inner content if it contains another marker
                    if inner_raw.contains("___TYPST_CELL___:") {
                        let inner_cell = GridCell::parse(inner_raw);

                        // Merge attributes for nested commands (e.g., \multicolumn wrapping \multirow).
                        // In LaTeX, these commands often set orthogonal properties (colspan/align vs rowspan).
                        // We accumulate non-default values from the inner cell to support composition.
                        if inner_cell.rowspan > 1 {
                            cell.rowspan = inner_cell.rowspan;
                        }
                        if inner_cell.colspan > 1 {
                            cell.colspan = inner_cell.colspan; // Though usually inner multirow has colspan=1
                        }
                        if inner_cell.align.is_some() {
                            // Inner alignment is used if the outer cell (e.g., \multirow) doesn't enforce one.
                            if cell.align.is_none() {
                                cell.align = inner_cell.align;
                            }
                        }

                        cell.content = inner_cell.content;
                    } else {
                        // Base content
                        cell.content = inner_raw.to_string();
                    }
                }
            } else {
                // Fallback if brackets parsing failed, just strip marker
                cell.content = raw.replace("___TYPST_CELL___:", "");
            }
        }

        cell
    }

    /// Generate Typst code for this cell
    pub fn to_typst(&self) -> String {
        let clean_content = self.content.trim();
        let content_expr = if clean_content.is_empty() {
            "[]".to_string()
        } else {
            format!("[{}]", clean_content)
        };

        if self.is_special || self.rowspan > 1 || self.colspan > 1 || self.align.is_some() {
            let mut attrs = Vec::new();

            if self.rowspan > 1 {
                attrs.push(format!("rowspan: {}", self.rowspan));
            }
            if self.colspan > 1 {
                attrs.push(format!("colspan: {}", self.colspan));
            }
            if let Some(align) = self.align {
                attrs.push(format!("align: {}", align.to_typst()));
            }

            if attrs.is_empty() && !self.is_special {
                // Just normal content if no special attrs (and not marked special explicitly)
                content_expr
            } else {
                // table.cell(attr1: val, attr2: val)[content]
                if attrs.is_empty() {
                    // Should not happen for valid special cells, but safe fallback
                    content_expr
                } else {
                    format!("table.cell({}){}", attrs.join(", "), content_expr)
                }
            }
        } else {
            content_expr
        }
    }
}
