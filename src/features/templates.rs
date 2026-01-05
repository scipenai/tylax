//! Document class to Typst template mapping
//!
//! This module maps LaTeX document classes to Typst templates and configurations.
//! It handles:
//!
//! - Standard document classes (article, book, report, letter)
//! - Presentation classes (beamer -> polylux)
//! - Academic templates (IEEE, ACM, etc.)
//! - Document class options (font size, columns, paper size)
//!
//! ## Example
//!
//! ```rust
//! use tylax::templates::{parse_document_class, DocumentClass};
//!
//! let doc_class = parse_document_class(r"\documentclass[12pt,twocolumn]{article}");
//! assert_eq!(doc_class.class_name, "article");
//! assert!(doc_class.options.contains(&"12pt".to_string()));
//! ```

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fmt::Write;

lazy_static! {
    /// Mapping of LaTeX document classes to Typst configurations
    static ref CLASS_MAPPINGS: HashMap<&'static str, ClassConfig> = {
        let mut m = HashMap::new();

        // Standard classes
        m.insert("article", ClassConfig {
            paper: "a4",
            heading_style: HeadingStyle::Numbered,
            has_chapters: false,
            math_numbering: Some("(1)"),
            default_font_size: 10.0,
            typst_import: None,
        });

        m.insert("report", ClassConfig {
            paper: "a4",
            heading_style: HeadingStyle::Numbered,
            has_chapters: true,
            math_numbering: Some("(1.1)"),
            default_font_size: 10.0,
            typst_import: None,
        });

        m.insert("book", ClassConfig {
            paper: "a4",
            heading_style: HeadingStyle::Numbered,
            has_chapters: true,
            math_numbering: Some("(1.1)"),
            default_font_size: 10.0,
            typst_import: None,
        });

        m.insert("letter", ClassConfig {
            paper: "us-letter",
            heading_style: HeadingStyle::None,
            has_chapters: false,
            math_numbering: None,
            default_font_size: 10.0,
            typst_import: None,
        });

        m.insert("beamer", ClassConfig {
            paper: "presentation-16-9",
            heading_style: HeadingStyle::None,
            has_chapters: false,
            math_numbering: None,
            default_font_size: 11.0,
            typst_import: Some(r#"#import "@preview/polylux:0.3.1": *"#),
        });

        m.insert("slides", ClassConfig {
            paper: "presentation-16-9",
            heading_style: HeadingStyle::None,
            has_chapters: false,
            math_numbering: None,
            default_font_size: 20.0,
            typst_import: Some(r#"#import "@preview/polylux:0.3.1": *"#),
        });

        m.insert("memoir", ClassConfig {
            paper: "a4",
            heading_style: HeadingStyle::Numbered,
            has_chapters: true,
            math_numbering: Some("(1.1)"),
            default_font_size: 10.0,
            typst_import: None,
        });

        m.insert("scrartcl", ClassConfig {
            paper: "a4",
            heading_style: HeadingStyle::Numbered,
            has_chapters: false,
            math_numbering: Some("(1)"),
            default_font_size: 11.0,
            typst_import: None,
        });

        m.insert("scrbook", ClassConfig {
            paper: "a4",
            heading_style: HeadingStyle::Numbered,
            has_chapters: true,
            math_numbering: Some("(1.1)"),
            default_font_size: 11.0,
            typst_import: None,
        });

        m.insert("scrreprt", ClassConfig {
            paper: "a4",
            heading_style: HeadingStyle::Numbered,
            has_chapters: true,
            math_numbering: Some("(1.1)"),
            default_font_size: 11.0,
            typst_import: None,
        });

        m
    };

    /// Academic template mappings
    static ref ACADEMIC_TEMPLATES: HashMap<&'static str, AcademicTemplate> = {
        let mut m = HashMap::new();

        m.insert("IEEEtran", AcademicTemplate {
            name: "IEEE",
            columns: 2,
            paper: "us-letter",
            typst_template: Some("@preview/charged-ieee:0.1.0"),
            font_family: Some("Times New Roman"),
            font_size: 10.0,
            abstract_style: AbstractStyle::Bold,
            bib_style: Some("ieee"),
        });

        m.insert("acmart", AcademicTemplate {
            name: "ACM",
            columns: 2,
            paper: "us-letter",
            typst_template: Some("@preview/acm-article:0.1.0"),
            font_family: Some("Linux Libertine"),
            font_size: 10.0,
            abstract_style: AbstractStyle::Bold,
            bib_style: Some("acm"),
        });

        m.insert("llncs", AcademicTemplate {
            name: "LNCS",
            columns: 1,
            paper: "a4",
            typst_template: None,
            font_family: Some("Times New Roman"),
            font_size: 10.0,
            abstract_style: AbstractStyle::Italic,
            bib_style: Some("springer-mathphys-brackets"),
        });

        m.insert("elsarticle", AcademicTemplate {
            name: "Elsevier",
            columns: 1,
            paper: "a4",
            typst_template: None,
            font_family: Some("Times New Roman"),
            font_size: 12.0,
            abstract_style: AbstractStyle::Bold,
            bib_style: Some("elsevier-harvard"),
        });

        m.insert("amsart", AcademicTemplate {
            name: "AMS",
            columns: 1,
            paper: "us-letter",
            typst_template: None,
            font_family: Some("Computer Modern"),
            font_size: 10.0,
            abstract_style: AbstractStyle::Italic,
            bib_style: Some("ams"),
        });

        m.insert("revtex4-2", AcademicTemplate {
            name: "REVTeX",
            columns: 2,
            paper: "us-letter",
            typst_template: None,
            font_family: Some("Times New Roman"),
            font_size: 10.0,
            abstract_style: AbstractStyle::Bold,
            bib_style: Some("american-physics-society"),
        });

        m
    };
}

/// Configuration for a document class
#[derive(Debug, Clone)]
struct ClassConfig {
    paper: &'static str,
    heading_style: HeadingStyle,
    has_chapters: bool,
    math_numbering: Option<&'static str>,
    default_font_size: f64,
    typst_import: Option<&'static str>,
}

/// Heading numbering style
#[derive(Debug, Clone, Copy)]
enum HeadingStyle {
    None,
    Numbered,
}

/// Abstract styling
#[derive(Debug, Clone, Copy)]
enum AbstractStyle {
    Bold,
    Italic,
}

/// Academic template configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct AcademicTemplate {
    name: &'static str,
    columns: u8,
    paper: &'static str,
    typst_template: Option<&'static str>,
    font_family: Option<&'static str>,
    font_size: f64,
    abstract_style: AbstractStyle,
    bib_style: Option<&'static str>,
}

/// Parsed document class
#[derive(Debug, Clone, Default)]
pub struct DocumentClass {
    /// Class name (article, book, etc.)
    pub class_name: String,
    /// Options passed to document class
    pub options: Vec<String>,
    /// Parsed font size (pt)
    pub font_size: Option<f64>,
    /// Number of columns
    pub columns: u8,
    /// Paper size
    pub paper: Option<String>,
    /// Draft mode
    pub draft: bool,
    /// Two-sided printing
    pub twoside: bool,
    /// Landscape orientation
    pub landscape: bool,
}

impl DocumentClass {
    /// Check if this is a presentation class
    pub fn is_presentation(&self) -> bool {
        matches!(self.class_name.as_str(), "beamer" | "slides" | "powerdot")
    }

    /// Check if this is an academic template
    pub fn is_academic(&self) -> bool {
        ACADEMIC_TEMPLATES.contains_key(self.class_name.as_str())
    }

    /// Get the academic template if applicable (internal use)
    fn academic_template(&self) -> Option<&AcademicTemplate> {
        ACADEMIC_TEMPLATES.get(self.class_name.as_str())
    }
}

/// Parse a \documentclass command
pub fn parse_document_class(input: &str) -> DocumentClass {
    let mut doc_class = DocumentClass {
        columns: 1,
        ..Default::default()
    };

    // Find \documentclass
    let Some(class_pos) = input.find(r"\documentclass") else {
        return doc_class;
    };

    let after_cmd = &input[class_pos + 14..]; // len of "\documentclass"

    // Parse optional arguments [...]
    let after_opts = if after_cmd.trim_start().starts_with('[') {
        let bracket_start = after_cmd
            .find('[')
            .expect("opening bracket must exist due to starts_with check");
        if let Some(bracket_end) = find_matching_bracket(&after_cmd[bracket_start..], '[', ']') {
            let opts_str = &after_cmd[bracket_start + 1..bracket_start + bracket_end];

            // Parse options
            for opt in opts_str.split(',') {
                let opt = opt.trim();
                if opt.is_empty() {
                    continue;
                }

                doc_class.options.push(opt.to_string());

                // Parse specific options
                if opt.ends_with("pt") {
                    if let Ok(size) = opt.trim_end_matches("pt").parse::<f64>() {
                        doc_class.font_size = Some(size);
                    }
                } else if opt == "twocolumn" {
                    doc_class.columns = 2;
                } else if opt == "onecolumn" {
                    doc_class.columns = 1;
                } else if opt == "draft" {
                    doc_class.draft = true;
                } else if opt == "twoside" {
                    doc_class.twoside = true;
                } else if opt == "landscape" {
                    doc_class.landscape = true;
                } else if opt == "a4paper" {
                    doc_class.paper = Some("a4".to_string());
                } else if opt == "letterpaper" {
                    doc_class.paper = Some("us-letter".to_string());
                } else if opt == "a5paper" {
                    doc_class.paper = Some("a5".to_string());
                } else if opt == "b5paper" {
                    doc_class.paper = Some("b5".to_string());
                } else if opt == "legalpaper" {
                    doc_class.paper = Some("us-legal".to_string());
                } else if opt == "executivepaper" {
                    doc_class.paper = Some("us-executive".to_string());
                }
            }

            &after_cmd[bracket_start + bracket_end + 1..]
        } else {
            after_cmd
        }
    } else {
        after_cmd
    };

    // Parse class name {...}
    let trimmed = after_opts.trim_start();
    if trimmed.starts_with('{') {
        if let Some(brace_end) = find_matching_bracket(trimmed, '{', '}') {
            doc_class.class_name = trimmed[1..brace_end].trim().to_string();
        }
    }

    doc_class
}

/// Find matching bracket
fn find_matching_bracket(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            c if c == open => depth += 1,
            c if c == close => {
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

/// Generate Typst preamble from document class
pub fn generate_typst_preamble(doc_class: &DocumentClass) -> String {
    let mut preamble = String::new();

    // Check for academic template first
    if let Some(template) = doc_class.academic_template() {
        generate_academic_preamble(&mut preamble, doc_class, template);
        return preamble;
    }

    // Check for class-specific configuration
    let config = CLASS_MAPPINGS.get(doc_class.class_name.as_str());

    // Imports
    if let Some(cfg) = config {
        if let Some(import) = cfg.typst_import {
            preamble.push_str(import);
            preamble.push('\n');
        }
    }

    // Page setup
    let paper = doc_class
        .paper
        .as_deref()
        .or(config.map(|c| c.paper))
        .unwrap_or("a4");

    let _ = write!(preamble, "#set page(paper: \"{}\"", paper);

    if doc_class.columns > 1 {
        let _ = write!(preamble, ", columns: {}", doc_class.columns);
    }

    if doc_class.landscape {
        preamble.push_str(", flipped: true");
    }

    preamble.push_str(")\n");

    // Font size
    let font_size = doc_class
        .font_size
        .or(config.map(|c| c.default_font_size))
        .unwrap_or(10.0);
    let _ = writeln!(preamble, "#set text(size: {}pt)", font_size);

    // Heading numbering
    if let Some(cfg) = config {
        if matches!(cfg.heading_style, HeadingStyle::Numbered) {
            if cfg.has_chapters {
                preamble.push_str("#set heading(numbering: \"1.1\")\n");
            } else {
                preamble.push_str("#set heading(numbering: \"1.\")\n");
            }
        }

        // Math equation numbering
        if let Some(numbering) = cfg.math_numbering {
            let _ = writeln!(preamble, "#set math.equation(numbering: \"{}\")", numbering);
        }
    }

    // Draft watermark
    if doc_class.draft {
        preamble.push_str(
            "#set page(background: rotate(45deg, text(size: 60pt, fill: luma(230))[DRAFT]))\n",
        );
    }

    preamble.push('\n');
    preamble
}

/// Generate preamble for academic templates
fn generate_academic_preamble(
    output: &mut String,
    doc_class: &DocumentClass,
    template: &AcademicTemplate,
) {
    // Try to use Typst template if available
    if let Some(typst_template) = template.typst_template {
        let _ = writeln!(output, "#import \"{}\": *", typst_template);
        output.push('\n');
        return;
    }

    // Manual configuration for templates without Typst equivalent
    let _ = write!(output, "#set page(paper: \"{}\"", template.paper);

    if template.columns > 1 {
        let _ = write!(output, ", columns: {}", template.columns);
    }

    // Academic papers usually have specific margins
    output.push_str(", margin: (x: 1in, y: 1in)");
    output.push_str(")\n");

    // Font
    if let Some(font) = template.font_family {
        let _ = writeln!(
            output,
            "#set text(font: \"{}\", size: {}pt)",
            font, template.font_size
        );
    } else {
        let _ = writeln!(output, "#set text(size: {}pt)", template.font_size);
    }

    // Heading and equation numbering
    output.push_str("#set heading(numbering: \"1.\")\n");
    output.push_str("#set math.equation(numbering: \"(1)\")\n");

    // Draft mode
    if doc_class.draft {
        output.push_str(
            "#set page(background: rotate(45deg, text(size: 60pt, fill: luma(230))[DRAFT]))\n",
        );
    }

    // Bibliography style hint
    if let Some(bib_style) = template.bib_style {
        let _ = writeln!(output, "// Recommended bibliography style: {}", bib_style);
    }

    output.push('\n');
}

/// Generate title block from metadata
pub fn generate_title_block(
    title: Option<&str>,
    author: Option<&str>,
    date: Option<&str>,
    abstract_text: Option<&str>,
) -> String {
    let mut output = String::new();

    // Title
    if title.is_some() || author.is_some() {
        output.push_str("#align(center)[\n");

        if let Some(t) = title {
            let _ = writeln!(output, "  #text(size: 20pt, weight: \"bold\")[{}]", t);
            output.push_str("  #v(1em)\n");
        }

        if let Some(a) = author {
            // Handle multiple authors separated by \and
            let authors: Vec<&str> = a.split(r"\and").collect();
            if authors.len() == 1 {
                let _ = writeln!(output, "  #text(size: 12pt)[{}]", a.trim());
            } else {
                output.push_str("  #stack(dir: ltr, spacing: 2em,\n");
                for auth in authors {
                    let _ = writeln!(output, "    text(size: 12pt)[{}],", auth.trim());
                }
                output.push_str("  )\n");
            }
            output.push_str("  #v(0.5em)\n");
        }

        if let Some(d) = date {
            if d == r"\today" {
                output.push_str("  #datetime.today().display()\n");
            } else {
                let _ = writeln!(output, "  {}", d);
            }
        }

        output.push_str("]\n\n");
    }

    // Abstract
    if let Some(abs) = abstract_text {
        output.push_str("#block(width: 100%, inset: 1em)[\n");
        output.push_str("  #align(center)[#text(weight: \"bold\")[Abstract]]\n");
        output.push_str("  #v(0.5em)\n");
        let _ = writeln!(output, "  {}", abs.trim());
        output.push_str("]\n\n");
    }

    output
}

/// Generate Typst configuration for a beamer presentation
pub fn generate_beamer_config(theme: Option<&str>, color_theme: Option<&str>) -> String {
    let mut config = String::new();

    config.push_str("#import \"@preview/polylux:0.3.1\": *\n\n");
    config.push_str("#set page(paper: \"presentation-16-9\")\n");
    config.push_str("#set text(size: 20pt)\n");

    // Theme configuration
    let theme_name = theme.unwrap_or("default");
    let _ = writeln!(
        config,
        "// Beamer theme: {} (manual adaptation needed)",
        theme_name
    );

    // Color theme
    if let Some(color) = color_theme {
        let _ = writeln!(config, "// Color theme: {}", color);
    }

    // Frame macro
    config.push_str(
        r#"
#let frame(title: none, body) = polylux-slide[
  #if title != none [
    #text(size: 24pt, weight: "bold")[#title]
    #v(1em)
  ]
  #body
]
"#,
    );

    config.push('\n');
    config
}

/// Convert \frame command to polylux slide
pub fn convert_beamer_frame(content: &str, title: Option<&str>) -> String {
    let mut output = String::new();

    output.push_str("#polylux-slide[\n");

    if let Some(t) = title {
        let _ = writeln!(output, "  #text(size: 24pt, weight: \"bold\")[{}]", t);
        output.push_str("  #v(1em)\n");
    }

    // Process content
    let processed = content.trim();
    for line in processed.lines() {
        let _ = writeln!(output, "  {}", line);
    }

    output.push_str("]\n\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_class() {
        let doc = parse_document_class(r"\documentclass{article}");
        assert_eq!(doc.class_name, "article");
        assert!(doc.options.is_empty());
    }

    #[test]
    fn test_parse_class_with_options() {
        let doc = parse_document_class(r"\documentclass[12pt,twocolumn]{article}");
        assert_eq!(doc.class_name, "article");
        assert_eq!(doc.font_size, Some(12.0));
        assert_eq!(doc.columns, 2);
    }

    #[test]
    fn test_parse_paper_size() {
        let doc = parse_document_class(r"\documentclass[a4paper]{report}");
        assert_eq!(doc.class_name, "report");
        assert_eq!(doc.paper, Some("a4".to_string()));
    }

    #[test]
    fn test_parse_beamer() {
        let doc = parse_document_class(r"\documentclass{beamer}");
        assert!(doc.is_presentation());
    }

    #[test]
    fn test_parse_ieee() {
        let doc = parse_document_class(r"\documentclass{IEEEtran}");
        assert!(doc.is_academic());
        assert!(doc.academic_template().is_some());
    }

    #[test]
    fn test_generate_preamble_article() {
        let doc = parse_document_class(r"\documentclass[11pt]{article}");
        let preamble = generate_typst_preamble(&doc);

        assert!(preamble.contains("set page"));
        assert!(preamble.contains("11pt"));
    }

    #[test]
    fn test_generate_preamble_twocolumn() {
        let doc = parse_document_class(r"\documentclass[twocolumn]{article}");
        let preamble = generate_typst_preamble(&doc);

        assert!(preamble.contains("columns: 2"));
    }

    #[test]
    fn test_generate_preamble_beamer() {
        let doc = parse_document_class(r"\documentclass{beamer}");
        let preamble = generate_typst_preamble(&doc);

        assert!(preamble.contains("polylux"));
        assert!(preamble.contains("presentation"));
    }

    #[test]
    fn test_title_block() {
        let block = generate_title_block(Some("My Paper"), Some("John Doe"), Some("2026"), None);

        assert!(block.contains("My Paper"));
        assert!(block.contains("John Doe"));
        assert!(block.contains("2026"));
    }

    #[test]
    fn test_title_block_with_abstract() {
        let block = generate_title_block(Some("Paper"), None, None, Some("This is the abstract."));

        assert!(block.contains("Abstract"));
        assert!(block.contains("This is the abstract"));
    }

    #[test]
    fn test_beamer_frame() {
        let frame = convert_beamer_frame("Hello world!", Some("Introduction"));

        assert!(frame.contains("polylux-slide"));
        assert!(frame.contains("Introduction"));
        assert!(frame.contains("Hello world"));
    }
}
