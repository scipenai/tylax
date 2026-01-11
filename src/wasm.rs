//! WASM bindings for tylax
//!
//! This module provides JavaScript-accessible functions for LaTeX ↔ Typst conversion.

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
use serde::{Deserialize, Serialize};

/// LaTeX to Typst conversion options (exposed to WASM)
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize, Default)]
pub struct L2TConvertOptions {
    /// Whether to format the output for better readability
    #[serde(default)]
    pub pretty: bool,
    /// Whether to convert as full document (not just math)
    #[serde(default)]
    pub full_document: bool,
    /// Use shorthand symbols (e.g., `->` instead of `arrow.r`)
    #[serde(default = "default_true")]
    pub prefer_shorthands: bool,
    /// Convert simple fractions to slash notation
    #[serde(default = "default_true")]
    pub frac_to_slash: bool,
    /// Use `oo` instead of `infinity` for `\infty`
    #[serde(default)]
    pub infty_to_oo: bool,
    /// Non-strict mode: allow unknown commands to pass through
    #[serde(default = "default_true")]
    pub non_strict: bool,
    /// Apply output optimizations
    #[serde(default = "default_true")]
    pub optimize: bool,
}

/// Typst to LaTeX conversion options (exposed to WASM)
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize, Default)]
pub struct T2LConvertOptions {
    /// Whether to convert as full document
    #[serde(default)]
    pub full_document: bool,
    /// Whether we're in block math mode (affects display/inline conversion)
    #[serde(default = "default_true")]
    pub block_math_mode: bool,
}

/// Legacy conversion options for backwards compatibility
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize, Default)]
pub struct ConvertOptions {
    /// Whether to format the output for better readability
    #[serde(default)]
    pub pretty: bool,
    /// Whether to preserve comments (if supported)
    #[serde(default)]
    pub preserve_comments: bool,
    /// Whether to convert as full document (not just math)
    #[serde(default)]
    pub full_document: bool,
}

#[cfg(feature = "wasm")]
fn default_true() -> bool {
    true
}

/// Conversion result with additional metadata
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize)]
pub struct ConvertResult {
    /// The converted output
    pub output: String,
    /// Whether the conversion was successful
    pub success: bool,
    /// Error message if conversion failed
    pub error: Option<String>,
    /// Warnings during conversion
    pub warnings: Vec<String>,
}

/// Initialize panic hook for better error messages in browser console
#[cfg(feature = "wasm")]
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Convert LaTeX math to Typst math
///
/// # Arguments
/// * `input` - LaTeX math code (without $ delimiters)
///
/// # Returns
/// Typst math code
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "latexToTypst")]
pub fn latex_to_typst_wasm(input: &str) -> String {
    crate::latex_to_typst(input)
}

/// Convert Typst math to LaTeX math
///
/// # Arguments
/// * `input` - Typst math code (without $ delimiters)
///
/// # Returns
/// LaTeX math code
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "typstToLatex")]
pub fn typst_to_latex_wasm(input: &str) -> String {
    crate::typst_to_latex(input)
}

/// Convert LaTeX document to Typst document
///
/// # Arguments
/// * `input` - Full LaTeX document
///
/// # Returns
/// Typst document
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "latexDocumentToTypst")]
pub fn latex_document_to_typst_wasm(input: &str) -> String {
    crate::latex_document_to_typst(input)
}

/// Convert Typst document to LaTeX document
///
/// # Arguments
/// * `input` - Full Typst document
///
/// # Returns
/// LaTeX document
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "typstDocumentToLatex")]
pub fn typst_document_to_latex_wasm(input: &str) -> String {
    crate::typst_document_to_latex(input)
}

/// Convert LaTeX to Typst with options
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "latexToTypstWithOptions")]
pub fn latex_to_typst_with_options_wasm(input: &str, options: JsValue) -> JsValue {
    let opts: L2TConvertOptions = serde_wasm_bindgen::from_value(options).unwrap_or_default();

    // Convert WASM options to internal L2TOptions
    let l2t_opts = crate::L2TOptions {
        prefer_shorthands: opts.prefer_shorthands,
        frac_to_slash: opts.frac_to_slash,
        infty_to_oo: opts.infty_to_oo,
        keep_spaces: false,
        non_strict: opts.non_strict,
        optimize: opts.optimize,
    };

    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if opts.full_document {
            crate::latex_document_to_typst_with_options(input, &l2t_opts)
        } else {
            let mut out = crate::latex_to_typst_with_options(input, &l2t_opts);
            if opts.pretty {
                out = format_typst_output(&out);
            }
            out
        }
    })) {
        Ok(output) => ConvertResult {
            output,
            success: true,
            error: None,
            warnings: vec![],
        },
        Err(e) => {
            // Try to extract panic message for better error reporting
            let error_msg = if let Some(s) = e.downcast_ref::<&str>() {
                format!("Conversion failed: {}", s)
            } else if let Some(s) = e.downcast_ref::<String>() {
                format!("Conversion failed: {}", s)
            } else {
                "Conversion failed: unknown error (check browser console for details)".to_string()
            };
            ConvertResult {
                output: String::new(),
                success: false,
                error: Some(error_msg),
                warnings: vec![],
            }
        }
    };

    serde_wasm_bindgen::to_value(&result).unwrap()
}

/// Convert Typst to LaTeX with options
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "typstToLatexWithOptions")]
pub fn typst_to_latex_with_options_wasm(input: &str, options: JsValue) -> JsValue {
    let opts: T2LConvertOptions = serde_wasm_bindgen::from_value(options).unwrap_or_default();

    // Convert WASM options to internal T2LOptions
    let t2l_opts = crate::T2LOptions {
        full_document: opts.full_document,
        document_class: "article".to_string(),
        title: None,
        author: None,
        math_only: !opts.full_document,
        block_math_mode: opts.block_math_mode,
    };

    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::typst_to_latex_with_options(input, &t2l_opts)
    })) {
        Ok(output) => ConvertResult {
            output,
            success: true,
            error: None,
            warnings: vec![],
        },
        Err(e) => {
            // Try to extract panic message for better error reporting
            let error_msg = if let Some(s) = e.downcast_ref::<&str>() {
                format!("Conversion failed: {}", s)
            } else if let Some(s) = e.downcast_ref::<String>() {
                format!("Conversion failed: {}", s)
            } else {
                "Conversion failed: unknown error (check browser console for details)".to_string()
            };
            ConvertResult {
                output: String::new(),
                success: false,
                error: Some(error_msg),
                warnings: vec![],
            }
        }
    };

    serde_wasm_bindgen::to_value(&result).unwrap()
}

/// Detect input format (latex or typst)
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "detectFormat")]
pub fn detect_format_wasm(input: &str) -> String {
    crate::detect_format(input).to_string()
}

/// Get version information
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "getVersion")]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Convert TikZ to CeTZ
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "tikzToCetz")]
pub fn tikz_to_cetz_wasm(input: &str) -> String {
    crate::tikz::convert_tikz_to_cetz(input)
}

/// Convert CeTZ to TikZ
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "cetzToTikz")]
pub fn cetz_to_tikz_wasm(input: &str) -> String {
    crate::tikz::convert_cetz_to_tikz(input)
}

/// Check if input is CeTZ code
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "isCetzCode")]
pub fn is_cetz_code_wasm(input: &str) -> bool {
    crate::tikz::is_cetz_code(input)
}

/// Check LaTeX for potential issues
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "checkLatex")]
pub fn check_latex_wasm(input: &str) -> JsValue {
    use crate::diagnostics::DiagnosticLevel;

    let result = crate::diagnostics::check_latex(input);

    // Group diagnostics by level
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut infos = Vec::new();

    for d in &result.diagnostics {
        match d.level {
            DiagnosticLevel::Error => errors.push(d.message.clone()),
            DiagnosticLevel::Warning => warnings.push(d.message.clone()),
            DiagnosticLevel::Info => infos.push(d.message.clone()),
        }
    }

    let summary = CheckSummary {
        errors,
        warnings,
        infos,
        has_errors: result.has_errors(),
    };
    serde_wasm_bindgen::to_value(&summary).unwrap()
}

/// Summary of LaTeX check results
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize)]
pub struct CheckSummary {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub infos: Vec<String>,
    pub has_errors: bool,
}

// ===== Table Preview Data Structures =====

/// Cell alignment for preview
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub enum PreviewCellAlign {
    Left,
    #[default]
    Center,
    Right,
}

/// A single table cell for preview
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PreviewCell {
    /// Cell content (may contain LaTeX math)
    pub content: String,
    /// Number of columns this cell spans
    #[serde(default = "default_one")]
    pub colspan: usize,
    /// Number of rows this cell spans
    #[serde(default = "default_one")]
    pub rowspan: usize,
    /// Cell alignment
    #[serde(default)]
    pub align: PreviewCellAlign,
    /// Whether this is a header cell
    #[serde(default)]
    pub is_header: bool,
}

#[cfg(feature = "wasm")]
fn default_one() -> usize {
    1
}

/// A table row for preview
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PreviewRow {
    /// Cells in this row
    pub cells: Vec<PreviewCell>,
    /// Whether this row has a bottom border
    #[serde(default)]
    pub has_bottom_border: bool,
}

/// Structured table data for frontend preview
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TablePreviewData {
    /// Table rows
    pub rows: Vec<PreviewRow>,
    /// Whether the first row is a header
    #[serde(default)]
    pub has_header: bool,
    /// Number of columns
    pub column_count: usize,
    /// Default column alignments
    pub default_alignments: Vec<PreviewCellAlign>,
}

/// Format Typst output for better readability
#[cfg(feature = "wasm")]
fn format_typst_output(input: &str) -> String {
    // Add proper spacing around operators
    let mut output = input.to_string();

    // Normalize spacing
    output = output.replace("  ", " ");

    output.trim().to_string()
}

// ===== Table Preview Functions =====

/// Parse LaTeX table and return preview data
#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = "previewTable")]
pub fn preview_table_wasm(input: &str, format: &str) -> JsValue {
    use crate::features::tables::{parse_latex_table, parse_typst_table};
    
    let result = match format {
        "latex" => {
            if let Some(table) = parse_latex_table(input) {
                table_to_preview_data(&table)
            } else {
                return serde_wasm_bindgen::to_value(&TablePreviewError {
                    error: "Failed to parse LaTeX table".to_string(),
                }).unwrap()
            }
        }
        "typst" => {
            if let Some(table) = parse_typst_table(input) {
                table_to_preview_data(&table)
            } else {
                return serde_wasm_bindgen::to_value(&TablePreviewError {
                    error: "Failed to parse Typst table".to_string(),
                }).unwrap()
            }
        }
        _ => {
            return serde_wasm_bindgen::to_value(&TablePreviewError {
                error: format!("Unknown format: {}", format),
            }).unwrap()
        }
    };
    
    serde_wasm_bindgen::to_value(&result).unwrap()
}

/// Error response for table preview
#[cfg(feature = "wasm")]
#[derive(Serialize, Deserialize)]
pub struct TablePreviewError {
    pub error: String,
}

/// Convert internal Table to TablePreviewData
#[cfg(feature = "wasm")]
fn table_to_preview_data(table: &crate::features::tables::Table) -> TablePreviewData {
    let mut rows = Vec::new();
    let has_header = !table.header.is_empty();
    
    // Convert header rows
    for row in &table.header {
        let preview_row = row_to_preview_row(row, true);
        rows.push(preview_row);
    }
    
    // Convert body rows
    for row in &table.body {
        let preview_row = row_to_preview_row(row, false);
        rows.push(preview_row);
    }
    
    // Convert footer rows
    for row in &table.footer {
        let preview_row = row_to_preview_row(row, false);
        rows.push(preview_row);
    }
    
    // Convert column alignments
    let default_alignments: Vec<PreviewCellAlign> = table.colspecs.iter()
        .map(|spec| alignment_to_preview(&spec.alignment))
        .collect();
    
    TablePreviewData {
        rows,
        has_header,
        column_count: table.num_cols(),
        default_alignments,
    }
}

/// Convert a Row to PreviewRow
#[cfg(feature = "wasm")]
fn row_to_preview_row(row: &crate::features::tables::Row, is_header: bool) -> PreviewRow {
    let cells: Vec<PreviewCell> = row.cells.iter()
        .map(|cell| cell_to_preview_cell(cell, is_header))
        .collect();
    
    PreviewRow {
        cells,
        has_bottom_border: row.has_bottom_border,
    }
}

/// Convert a Cell to PreviewCell
#[cfg(feature = "wasm")]
fn cell_to_preview_cell(cell: &crate::features::tables::Cell, is_header: bool) -> PreviewCell {
    PreviewCell {
        content: cell.content.clone(),
        colspan: cell.colspan as usize,
        rowspan: cell.rowspan as usize,
        align: cell.alignment.map(|a| alignment_to_preview(&a)).unwrap_or_default(),
        is_header,
    }
}

/// Convert Alignment to PreviewCellAlign
#[cfg(feature = "wasm")]
fn alignment_to_preview(align: &crate::features::tables::Alignment) -> PreviewCellAlign {
    use crate::features::tables::Alignment;
    
    match align {
        Alignment::Left | Alignment::Default => PreviewCellAlign::Left,
        Alignment::Center => PreviewCellAlign::Center,
        Alignment::Right => PreviewCellAlign::Right,
    }
}
