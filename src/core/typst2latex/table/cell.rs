//! Cell types and alignment for LaTeX table generation

use typst_syntax::SyntaxNode;

use crate::core::typst2latex::context::{ConvertContext, EnvironmentContext};
use crate::core::typst2latex::markup::convert_markup_node;
use crate::core::typst2latex::utils::{
    format_latex_color_command, normalize_typst_color_expr, FuncArgs,
};

/// LaTeX cell alignment options
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LatexCellAlign {
    Left,
    #[default]
    Center,
    Right,
    /// Paragraph column with width, e.g., p{3cm}
    Para,
}

impl LatexCellAlign {
    /// Convert to LaTeX column specification character
    pub fn to_char(self) -> char {
        match self {
            LatexCellAlign::Left => 'l',
            LatexCellAlign::Center => 'c',
            LatexCellAlign::Right => 'r',
            LatexCellAlign::Para => 'p',
        }
    }

    /// Parse from Typst alignment string
    pub fn from_typst(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "left" => LatexCellAlign::Left,
            "center" => LatexCellAlign::Center,
            "right" => LatexCellAlign::Right,
            _ => LatexCellAlign::Center,
        }
    }
}

/// Represents a single table cell with span and alignment info
#[derive(Debug, Clone)]
pub struct LatexCell {
    /// Cell content (LaTeX code)
    pub content: String,
    /// Number of rows this cell spans
    pub rowspan: usize,
    /// Number of columns this cell spans
    pub colspan: usize,
    /// Optional cell-specific alignment
    pub align: Option<LatexCellAlign>,
    /// Optional cell background color
    pub fill: Option<String>,
    /// Whether this is a header cell
    pub is_header: bool,
    /// Whether this is an empty placeholder (for rowspan coverage)
    pub is_placeholder: bool,
}

impl LatexCell {
    /// Create a new cell with content
    pub fn new(content: String) -> Self {
        LatexCell {
            content,
            rowspan: 1,
            colspan: 1,
            align: None,
            fill: None,
            is_header: false,
            is_placeholder: false,
        }
    }

    /// Create an empty placeholder cell (for rowspan coverage)
    pub fn placeholder() -> Self {
        LatexCell {
            content: String::new(),
            rowspan: 1,
            colspan: 1,
            align: None,
            fill: None,
            is_header: false,
            is_placeholder: true,
        }
    }

    /// Create a cell with all attributes
    #[allow(dead_code)]
    pub fn with_spans(content: String, rowspan: usize, colspan: usize) -> Self {
        LatexCell {
            content,
            rowspan,
            colspan,
            align: None,
            fill: None,
            is_header: false,
            is_placeholder: false,
        }
    }

    /// Parse a table.cell(...) FuncCall node from Typst AST
    pub fn from_typst_cell_ast(node: &SyntaxNode, ctx: &mut ConvertContext) -> Self {
        use typst_syntax::SyntaxKind;

        let mut content = String::new();
        let mut colspan = 1usize;
        let mut rowspan = 1usize;
        let mut align = None;
        let mut fill = None;
        let children: Vec<_> = node.children().collect();
        let args = FuncArgs::from_func_call(&children);

        for child in node.children() {
            if child.kind() == SyntaxKind::Args {
                for arg in child.children() {
                    match arg.kind() {
                        SyntaxKind::Named => {
                            if let Some(parsed) = args.arg_for_node(arg) {
                                match parsed.name.as_deref() {
                                    Some("colspan") => {
                                        if let Some(n) = args.named_usize("colspan") {
                                            colspan = n;
                                        }
                                    }
                                    Some("rowspan") => {
                                        if let Some(n) = args.named_usize("rowspan") {
                                            rowspan = n;
                                        }
                                    }
                                    Some("align") => {
                                        align = Some(LatexCellAlign::from_typst(&parsed.value));
                                    }
                                    Some("fill") => {
                                        fill = normalize_typst_color_expr(&parsed.value);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        SyntaxKind::ContentBlock => {
                            // Cell content [...]
                            let mut cell_ctx = ConvertContext::new();
                            cell_ctx.push_env(EnvironmentContext::Table);
                            convert_markup_node(arg, &mut cell_ctx);
                            content = cell_ctx.finalize();
                        }
                        SyntaxKind::FuncCall => {
                            // Cell content can be a function call like text(...)[...]
                            let mut cell_ctx = ConvertContext::new();
                            cell_ctx.push_env(EnvironmentContext::Table);
                            convert_markup_node(arg, &mut cell_ctx);
                            let func_content = cell_ctx.finalize();
                            if !func_content.is_empty() {
                                content = func_content;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // If no ContentBlock was found, try to get content from trailing content
        if content.is_empty() {
            let mut cell_ctx = ConvertContext::new();
            cell_ctx.push_env(EnvironmentContext::Table);
            for child in node.children() {
                if child.kind() == SyntaxKind::ContentBlock {
                    convert_markup_node(child, &mut cell_ctx);
                }
            }
            content = cell_ctx.finalize();
        }

        // Use the passed context for any additional processing
        let _ = ctx;

        LatexCell {
            content,
            rowspan,
            colspan,
            align,
            fill,
            is_header: false,
            is_placeholder: false,
        }
    }

    /// Generate LaTeX code for this cell
    pub fn to_latex(&self, default_align: LatexCellAlign) -> String {
        if self.is_placeholder {
            return String::new();
        }

        let content_str = self.content.trim();
        let mut prefix = String::new();

        // Add cell color if present
        if let Some(ref color) = self.fill {
            prefix.push_str(&format!(
                "{} ",
                format_latex_color_command("cellcolor", color)
            ));
        }

        let content = format!("{}{}", prefix, content_str);

        // Build the inner content (potentially wrapped in \multirow)
        let inner = if self.rowspan > 1 {
            format!("\\multirow{{{}}}{{*}}{{{}}}", self.rowspan, content)
        } else {
            content
        };

        // Wrap in \multicolumn if needed
        if self.colspan > 1 {
            let align_char = self.align.unwrap_or(default_align).to_char();
            format!(
                "\\multicolumn{{{}}}{{|{}|}}{{{}}}",
                self.colspan, align_char, inner
            )
        } else {
            inner
        }
    }
}
