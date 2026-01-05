//! Horizontal line types and utilities for table parsing

/// Represents a horizontal line (full or partial)
#[derive(Debug, Clone)]
pub struct HLine {
    /// Starting column (1-based, as in LaTeX \cline{start-end})
    pub start_col: Option<usize>,
    /// Ending column (1-based, inclusive)
    pub end_col: Option<usize>,
}

impl HLine {
    /// Create a full horizontal line
    pub fn full() -> Self {
        HLine {
            start_col: None,
            end_col: None,
        }
    }

    /// Create a partial horizontal line
    pub fn partial(start: usize, end: usize) -> Self {
        HLine {
            start_col: Some(start),
            end_col: Some(end),
        }
    }

    /// Generate Typst code for this hline
    pub fn to_typst(&self) -> String {
        match (self.start_col, self.end_col) {
            (Some(s), Some(e)) => {
                // Typst uses 0-based indexing for stroke positions
                format!("table.hline(start: {}, end: {})", s - 1, e)
            }
            _ => "table.hline()".to_string(),
        }
    }
}

/// Extract hline range from cline/cmidrule if present
/// Returns Some((start, end)) for partial lines, None for full lines
pub fn extract_hline_range(row_str: &str) -> Option<(usize, usize)> {
    let s = row_str.trim();

    // Skip optional (lr) part
    let s = if s.starts_with('(') {
        if let Some(end) = s.find(')') {
            &s[end + 1..]
        } else {
            s
        }
    } else {
        s
    };

    let s = s.trim_start();

    // Parse range: number-number
    let chars: Vec<char> = s.chars().collect();
    let mut pos = 0;

    // Parse first number
    let start_n1 = pos;
    while pos < chars.len() && chars[pos].is_ascii_digit() {
        pos += 1;
    }

    if pos == start_n1 {
        return None; // No number found
    }

    let n1: usize = s[start_n1..pos].parse().ok()?;

    // Expect hyphen
    if pos >= chars.len() || chars[pos] != '-' {
        return None;
    }
    pos += 1;

    // Parse second number
    let start_n2 = pos;
    while pos < chars.len() && chars[pos].is_ascii_digit() {
        pos += 1;
    }

    if pos == start_n2 {
        return None;
    }

    let n2: usize = s[start_n2..pos].parse().ok()?;

    Some((n1, n2))
}

/// Clean arguments left over from cline/cmidrule after |||HLINE|||
pub fn clean_hline_args(s: &str) -> String {
    let mut result = s.trim_start().to_string();
    loop {
        let original_len = result.len();

        // Remove optional (lr) part
        if result.starts_with('(') {
            if let Some(end) = result.find(')') {
                let inner = &result[1..end];
                if inner.chars().all(|c| c == 'l' || c == 'r') {
                    result = result[end + 1..].trim_start().to_string();
                }
            }
        }

        // Remove range part like 2-5 or 3-4
        let chars: Vec<char> = result.chars().collect();
        let mut pos = 0;

        // Parse first number
        let start_n1 = pos;
        while pos < chars.len() && chars[pos].is_ascii_digit() {
            pos += 1;
        }
        let len_n1 = pos - start_n1;

        if len_n1 > 0 && len_n1 <= 2 && pos < chars.len() && chars[pos] == '-' {
            pos += 1;
            if pos < chars.len() && chars[pos] == '-' {
                pos += 1;
            }

            let start_n2 = pos;
            while pos < chars.len() && chars[pos].is_ascii_digit() {
                pos += 1;
            }
            let len_n2 = pos - start_n2;

            if len_n2 > 0 && len_n2 <= 2 {
                result = result[pos..].trim_start().to_string();
            }
        }

        if result.len() == original_len {
            break;
        }
    }
    result
}

/// Find the end of a command's arguments (including optional () and required {})
pub fn find_cmd_args_end(s: &str) -> Option<usize> {
    let mut pos = 0;
    let chars: Vec<char> = s.chars().collect();

    // Skip command name (starts with \)
    if pos < chars.len() && chars[pos] == '\\' {
        pos += 1;
        while pos < chars.len() && chars[pos].is_ascii_alphabetic() {
            pos += 1;
        }
    } else {
        return None;
    }

    // Skip optional whitespace
    while pos < chars.len() && chars[pos].is_whitespace() {
        pos += 1;
    }

    // Skip optional arguments in () or []
    while pos < chars.len() && (chars[pos] == '(' || chars[pos] == '[') {
        let open_char = chars[pos];
        let close_char = if open_char == '(' { ')' } else { ']' };

        pos += 1;
        while pos < chars.len() && chars[pos] != close_char {
            pos += 1;
        }
        if pos < chars.len() {
            pos += 1;
        }

        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
    }

    // Find and skip required arguments in {}
    while pos < chars.len() && chars[pos] == '{' {
        let mut depth = 1;
        pos += 1;
        while pos < chars.len() && depth > 0 {
            if chars[pos] == '{' {
                depth += 1;
            } else if chars[pos] == '}' {
                depth -= 1;
            }
            pos += 1;
        }

        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
    }

    Some(pos)
}

/// Clean a cell string by removing booktabs commands
pub fn clean_cell_content(cell: &str) -> String {
    let mut clean = cell.to_string();

    clean = clean.replace("\\toprule", "");
    clean = clean.replace("\\midrule", "");
    clean = clean.replace("\\bottomrule", "");
    clean = clean.replace("\\hline", "");

    while let Some(pos) = clean.find("\\cline") {
        if let Some(end) = find_cmd_args_end(&clean[pos..]) {
            clean = format!("{}{}", &clean[..pos], &clean[pos + end..]);
        } else {
            clean = clean.replace("\\cline", "");
        }
    }
    while let Some(pos) = clean.find("\\cmidrule") {
        if let Some(end) = find_cmd_args_end(&clean[pos..]) {
            clean = format!("{}{}", &clean[..pos], &clean[pos + end..]);
        } else {
            clean = clean.replace("\\cmidrule", "");
        }
    }

    clean.trim().to_string()
}
