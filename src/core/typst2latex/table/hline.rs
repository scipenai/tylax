//! Horizontal line types for LaTeX table generation

use typst_syntax::SyntaxNode;

use crate::core::typst2latex::utils::get_simple_text;

/// Style of horizontal line
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum HLineStyle {
    /// Standard \hline or \cline
    #[default]
    Normal,
    /// Booktabs \toprule
    TopRule,
    /// Booktabs \midrule
    MidRule,
    /// Booktabs \bottomrule
    BottomRule,
}

/// Represents a horizontal line in a LaTeX table
#[derive(Debug, Clone)]
pub struct LatexHLine {
    /// Start column (0-indexed, None = from beginning)
    pub start: Option<usize>,
    /// End column (0-indexed exclusive, None = to end)
    pub end: Option<usize>,
    /// Line style
    pub style: HLineStyle,
}

impl LatexHLine {
    /// Create a full-width horizontal line
    pub fn full() -> Self {
        LatexHLine {
            start: None,
            end: None,
            style: HLineStyle::Normal,
        }
    }

    /// Create a partial horizontal line (cline)
    pub fn partial(start: usize, end: usize) -> Self {
        LatexHLine {
            start: Some(start),
            end: Some(end),
            style: HLineStyle::Normal,
        }
    }

    /// Create a toprule
    pub fn top_rule() -> Self {
        LatexHLine {
            start: None,
            end: None,
            style: HLineStyle::TopRule,
        }
    }

    /// Create a midrule
    pub fn mid_rule() -> Self {
        LatexHLine {
            start: None,
            end: None,
            style: HLineStyle::MidRule,
        }
    }

    /// Create a bottomrule
    pub fn bottom_rule() -> Self {
        LatexHLine {
            start: None,
            end: None,
            style: HLineStyle::BottomRule,
        }
    }

    /// Parse from a Typst table.hline(...) FuncCall node
    pub fn from_typst_ast(node: &SyntaxNode) -> Self {
        use typst_syntax::SyntaxKind;

        let mut start: Option<usize> = None;
        let mut end: Option<usize> = None;

        for child in node.children() {
            if child.kind() == SyntaxKind::Args {
                for arg in child.children() {
                    if arg.kind() == SyntaxKind::Named {
                        let named_children: Vec<_> = arg.children().collect();
                        if !named_children.is_empty() {
                            let key = named_children[0].text().to_string();
                            let full_text = get_simple_text(arg);

                            if let Some(colon_pos) = full_text.find(':') {
                                let value = full_text[colon_pos + 1..].trim();

                                match key.as_str() {
                                    "start" => {
                                        if let Ok(n) = value.parse::<usize>() {
                                            start = Some(n);
                                        }
                                    }
                                    "end" => {
                                        if let Ok(n) = value.parse::<usize>() {
                                            end = Some(n);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        LatexHLine {
            start,
            end,
            style: HLineStyle::Normal,
        }
    }

    /// Generate LaTeX code for this horizontal line
    pub fn to_latex(&self) -> String {
        match self.style {
            HLineStyle::TopRule => "\\toprule".to_string(),
            HLineStyle::MidRule => "\\midrule".to_string(),
            HLineStyle::BottomRule => "\\bottomrule".to_string(),
            HLineStyle::Normal => {
                match (self.start, self.end) {
                    (Some(s), Some(e)) => {
                        // LaTeX cline uses 1-indexed columns
                        format!("\\cline{{{}-{}}}", s + 1, e)
                    }
                    (Some(s), None) => {
                        // Partial from start to end (we don't know total columns here)
                        // This case is tricky - caller should provide end
                        format!("\\cline{{{}-}}", s + 1)
                    }
                    _ => "\\hline".to_string(),
                }
            }
        }
    }

    /// Generate LaTeX code with known column count (for partial lines without explicit end)
    pub fn to_latex_with_cols(&self, col_count: usize) -> String {
        match self.style {
            HLineStyle::TopRule => "\\toprule".to_string(),
            HLineStyle::MidRule => "\\midrule".to_string(),
            HLineStyle::BottomRule => "\\bottomrule".to_string(),
            HLineStyle::Normal => match (self.start, self.end) {
                (Some(s), Some(e)) => {
                    format!("\\cline{{{}-{}}}", s + 1, e)
                }
                (Some(s), None) => {
                    format!("\\cline{{{}-{}}}", s + 1, col_count)
                }
                (None, Some(e)) => {
                    format!("\\cline{{1-{}}}", e)
                }
                (None, None) => "\\hline".to_string(),
            },
        }
    }
}
