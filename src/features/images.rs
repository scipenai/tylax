//! Image and Figure handling for LaTeX ↔ Typst conversion
//!
//! This module provides support for:
//! - `\includegraphics[options]{path}` with key-value attributes
//! - `\begin{figure}...\end{figure}` environment with captions
//! - Typst `#image()` and `#figure()` constructs
//!
//! Inspired by Pandoc's ImageSize and attribute parsing.

use std::collections::HashMap;

/// Represents a dimension value with unit
#[derive(Debug, Clone, PartialEq)]
pub enum Dimension {
    Pixel(f64),
    Centimeter(f64),
    Millimeter(f64),
    Inch(f64),
    Point(f64),
    Pica(f64),
    Percent(f64),
    Em(f64),
    /// Relative to text width (e.g., 0.5\textwidth)
    TextWidth(f64),
    /// Relative to line width
    LineWidth(f64),
    /// Relative to text height
    TextHeight(f64),
}

impl Dimension {
    /// Parse a dimension string like "10cm", "0.5\textwidth", "100px"
    pub fn parse(s: &str) -> Option<Dimension> {
        let s = s.trim();

        // Check for relative dimensions first
        if let Some(rest) = s.strip_suffix("\\textwidth") {
            let num = rest.trim().parse::<f64>().ok()?;
            return Some(Dimension::TextWidth(num));
        }
        if let Some(rest) = s.strip_suffix("\\linewidth") {
            let num = rest.trim().parse::<f64>().ok()?;
            return Some(Dimension::LineWidth(num));
        }
        if let Some(rest) = s.strip_suffix("\\textheight") {
            let num = rest.trim().parse::<f64>().ok()?;
            return Some(Dimension::TextHeight(num));
        }

        // Parse unit-based dimensions
        if let Some(rest) = s.strip_suffix("cm") {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Centimeter(num));
            }
        }
        if let Some(rest) = s.strip_suffix("mm") {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Millimeter(num));
            }
        }
        if let Some(rest) = s.strip_suffix("in") {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Inch(num));
            }
        }
        if let Some(rest) = s.strip_suffix("pt") {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Point(num));
            }
        }
        if let Some(rest) = s.strip_suffix("pc") {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Pica(num));
            }
        }
        if let Some(rest) = s.strip_suffix("px") {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Pixel(num));
            }
        }
        if let Some(rest) = s.strip_suffix("em") {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Em(num));
            }
        }
        if let Some(rest) = s.strip_suffix('%') {
            if let Ok(num) = rest.trim().parse::<f64>() {
                return Some(Dimension::Percent(num));
            }
        }

        // Try parsing as pure number (assume pt)
        if let Ok(num) = s.parse::<f64>() {
            return Some(Dimension::Point(num));
        }

        None
    }

    /// Convert to Typst dimension string
    pub fn to_typst(&self) -> String {
        match self {
            Dimension::Pixel(v) => format!("{}pt", v * 0.75), // 1px ≈ 0.75pt at 96dpi
            Dimension::Centimeter(v) => format!("{}cm", format_num(*v)),
            Dimension::Millimeter(v) => format!("{}mm", format_num(*v)),
            Dimension::Inch(v) => format!("{}in", format_num(*v)),
            Dimension::Point(v) => format!("{}pt", format_num(*v)),
            Dimension::Pica(v) => format!("{}pt", format_num(v * 12.0)), // 1pc = 12pt
            Dimension::Percent(v) => format!("{}%", format_num(*v)),
            Dimension::Em(v) => format!("{}em", format_num(*v)),
            Dimension::TextWidth(v) => format!("{}%", format_num(v * 100.0)),
            Dimension::LineWidth(v) => format!("{}%", format_num(v * 100.0)),
            Dimension::TextHeight(v) => format!("{}%", format_num(v * 100.0)),
        }
    }

    /// Convert to LaTeX dimension string
    pub fn to_latex(&self) -> String {
        match self {
            Dimension::Pixel(v) => format!("{}pt", format_num(v * 0.75)),
            Dimension::Centimeter(v) => format!("{}cm", format_num(*v)),
            Dimension::Millimeter(v) => format!("{}mm", format_num(*v)),
            Dimension::Inch(v) => format!("{}in", format_num(*v)),
            Dimension::Point(v) => format!("{}pt", format_num(*v)),
            Dimension::Pica(v) => format!("{}pc", format_num(*v)),
            Dimension::Percent(v) => format!("{}\\textwidth", format_num(v / 100.0)),
            Dimension::Em(v) => format!("{}em", format_num(*v)),
            Dimension::TextWidth(v) => format!("{}\\textwidth", format_num(*v)),
            Dimension::LineWidth(v) => format!("{}\\linewidth", format_num(*v)),
            Dimension::TextHeight(v) => format!("{}\\textheight", format_num(*v)),
        }
    }
}

/// Format number, removing trailing zeros
fn format_num(v: f64) -> String {
    let s = format!("{:.5}", v);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}

/// Image attributes parsed from LaTeX options
#[derive(Debug, Clone, Default)]
pub struct ImageAttributes {
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub scale: Option<f64>,
    pub angle: Option<f64>,
    pub trim: Option<(f64, f64, f64, f64)>, // left, bottom, right, top
    pub clip: bool,
    pub alt: Option<String>,
    pub keepaspectratio: bool,
    /// Other key-value pairs
    pub other: HashMap<String, String>,
}

impl ImageAttributes {
    /// Parse LaTeX key-value options like "width=0.5\textwidth, height=3cm"
    pub fn parse(options: &str) -> Self {
        let mut attrs = ImageAttributes::default();

        for part in split_keyvals(options) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some((key, value)) = part.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "width" => attrs.width = Dimension::parse(value),
                    "height" => attrs.height = Dimension::parse(value),
                    "scale" => attrs.scale = value.parse().ok(),
                    "angle" => attrs.angle = value.parse().ok(),
                    "alt" => attrs.alt = Some(value.to_string()),
                    "keepaspectratio" => {
                        attrs.keepaspectratio = value == "true" || value.is_empty()
                    }
                    "clip" => attrs.clip = value == "true" || value.is_empty(),
                    "trim" => {
                        // Parse "left bottom right top"
                        let parts: Vec<f64> = value
                            .split_whitespace()
                            .filter_map(|s| {
                                // Remove unit suffix for parsing
                                let s = s.trim_end_matches(|c: char| c.is_alphabetic());
                                s.parse().ok()
                            })
                            .collect();
                        if parts.len() == 4 {
                            attrs.trim = Some((parts[0], parts[1], parts[2], parts[3]));
                        }
                    }
                    _ => {
                        attrs.other.insert(key.to_string(), value.to_string());
                    }
                }
            } else {
                // Boolean flag without value
                match part {
                    "keepaspectratio" => attrs.keepaspectratio = true,
                    "clip" => attrs.clip = true,
                    _ => {
                        attrs.other.insert(part.to_string(), String::new());
                    }
                }
            }
        }

        attrs
    }

    /// Convert to Typst image() arguments
    pub fn to_typst_args(&self) -> String {
        let mut args = Vec::new();

        if let Some(ref w) = self.width {
            args.push(format!("width: {}", w.to_typst()));
        }
        if let Some(ref h) = self.height {
            args.push(format!("height: {}", h.to_typst()));
        }
        if let Some(alt) = &self.alt {
            args.push(format!("alt: \"{}\"", escape_typst_string(alt)));
        }

        // Handle fit based on keepaspectratio
        if self.width.is_some() && self.height.is_some() && !self.keepaspectratio {
            args.push("fit: \"stretch\"".to_string());
        }

        args.join(", ")
    }

    /// Convert to LaTeX includegraphics options
    pub fn to_latex_options(&self) -> String {
        let mut opts = Vec::new();

        if let Some(ref w) = self.width {
            opts.push(format!("width={}", w.to_latex()));
        }
        if let Some(ref h) = self.height {
            opts.push(format!("height={}", h.to_latex()));
        }
        if let Some(scale) = self.scale {
            opts.push(format!("scale={}", format_num(scale)));
        }
        if let Some(angle) = self.angle {
            opts.push(format!("angle={}", format_num(angle)));
        }
        if self.keepaspectratio {
            opts.push("keepaspectratio".to_string());
        }
        if self.clip {
            opts.push("clip".to_string());
        }
        if let Some((l, b, r, t)) = self.trim {
            opts.push(format!(
                "trim={} {} {} {}",
                format_num(l),
                format_num(b),
                format_num(r),
                format_num(t)
            ));
        }

        opts.join(", ")
    }
}

/// Split key-value string, respecting nested braces
fn split_keyvals(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in s.chars() {
        match c {
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                result.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }

    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    result
}

/// Escape string for Typst
fn escape_typst_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Represents a parsed figure environment
#[derive(Debug, Clone, Default)]
pub struct Figure {
    pub image_path: String,
    pub image_attrs: ImageAttributes,
    pub caption: Option<String>,
    pub label: Option<String>,
    pub placement: Option<String>, // h, t, b, p, !
    pub centering: bool,
}

impl Figure {
    /// Parse a LaTeX figure environment
    pub fn parse_latex(content: &str) -> Option<Figure> {
        let mut figure = Figure {
            centering: content.contains("\\centering"),
            ..Default::default()
        };

        // Parse placement specifier from \begin{figure}[htbp]
        // This is typically passed separately, but we check content too

        // Parse \includegraphics
        if let Some(img_start) = content.find("\\includegraphics") {
            let after_cmd = &content[img_start + "\\includegraphics".len()..];

            // Parse optional arguments [...]
            let (options, rest) = if after_cmd.trim_start().starts_with('[') {
                parse_bracket_content(after_cmd.trim_start())
            } else {
                (String::new(), after_cmd)
            };

            figure.image_attrs = ImageAttributes::parse(&options);

            // Parse path {path}
            if let Some(path) = extract_braced(rest) {
                figure.image_path = path;
            }
        }

        // Parse \caption
        if let Some(cap_start) = content.find("\\caption") {
            let after_cmd = &content[cap_start + "\\caption".len()..];
            // Skip short caption [...]
            let rest = if after_cmd.trim_start().starts_with('[') {
                let (_, r) = parse_bracket_content(after_cmd.trim_start());
                r
            } else {
                after_cmd
            };
            if let Some(caption) = extract_braced(rest) {
                figure.caption = Some(caption);
            }
        }

        // Parse \label
        if let Some(label_start) = content.find("\\label") {
            let after_cmd = &content[label_start + "\\label".len()..];
            if let Some(label) = extract_braced(after_cmd) {
                figure.label = Some(label);
            }
        }

        if !figure.image_path.is_empty() {
            Some(figure)
        } else {
            None
        }
    }

    /// Convert to Typst figure code
    pub fn to_typst(&self) -> String {
        let mut result = String::new();

        result.push_str("#figure(\n");

        // Image
        let args = self.image_attrs.to_typst_args();
        if args.is_empty() {
            result.push_str(&format!(
                "  image(\"{}\"),\n",
                escape_typst_string(&self.image_path)
            ));
        } else {
            result.push_str(&format!(
                "  image(\"{}\", {}),\n",
                escape_typst_string(&self.image_path),
                args
            ));
        }

        // Caption
        if let Some(ref caption) = self.caption {
            result.push_str(&format!("  caption: [{}],\n", caption));
        }

        result.push(')');

        // Label
        if let Some(ref label) = self.label {
            result.push_str(&format!(" <{}>", label));
        }

        result
    }

    /// Convert to LaTeX figure code
    pub fn to_latex(&self) -> String {
        let mut result = String::new();

        let placement = self.placement.as_deref().unwrap_or("htbp");
        result.push_str(&format!("\\begin{{figure}}[{}]\n", placement));

        if self.centering {
            result.push_str("  \\centering\n");
        }

        // Image
        let opts = self.image_attrs.to_latex_options();
        if opts.is_empty() {
            result.push_str(&format!("  \\includegraphics{{{}}}\n", self.image_path));
        } else {
            result.push_str(&format!(
                "  \\includegraphics[{}]{{{}}}\n",
                opts, self.image_path
            ));
        }

        // Caption and label
        if let Some(ref caption) = self.caption {
            result.push_str(&format!("  \\caption{{{}}}\n", caption));
        }
        if let Some(ref label) = self.label {
            result.push_str(&format!("  \\label{{{}}}\n", label));
        }

        result.push_str("\\end{figure}");

        result
    }
}

/// Parse bracketed content like [options], returns (content, rest)
fn parse_bracket_content(s: &str) -> (String, &str) {
    if !s.starts_with('[') {
        return (String::new(), s);
    }

    let mut depth = 0;
    let mut end_idx = 0;

    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end_idx = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if end_idx > 0 {
        (s[1..end_idx].to_string(), &s[end_idx + 1..])
    } else {
        (String::new(), s)
    }
}

/// Extract content within braces
fn extract_braced(s: &str) -> Option<String> {
    let s = s.trim_start();
    if !s.starts_with('{') {
        return None;
    }

    let mut depth = 0;
    let mut start = 0;
    let mut end = 0;

    for (i, c) in s.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    start = i + 1;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if end > start {
        Some(s[start..end].to_string())
    } else {
        None
    }
}

/// Parse Typst image function call
pub fn parse_typst_image(content: &str) -> Option<(String, ImageAttributes)> {
    // Match #image("path", ...) or image("path", ...)
    let content = content.trim();
    let content = content.strip_prefix('#').unwrap_or(content);

    if !content.starts_with("image(") {
        return None;
    }

    let inner = &content["image(".len()..];
    let inner = inner.strip_suffix(')')?.trim();

    // Parse path (first argument)
    let (path, rest) = if inner.starts_with('"') {
        parse_quoted_string(inner)?
    } else {
        return None;
    };

    // Parse remaining arguments
    let mut attrs = ImageAttributes::default();
    let rest = rest.trim().strip_prefix(',').unwrap_or(rest).trim();

    for part in split_keyvals(rest) {
        let part = part.trim();
        if let Some((key, value)) = part.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "width" => attrs.width = parse_typst_dimension(value),
                "height" => attrs.height = parse_typst_dimension(value),
                "alt" => {
                    if let Some((alt, _)) = parse_quoted_string(value) {
                        attrs.alt = Some(alt);
                    }
                }
                "fit" => {
                    if value.contains("stretch") {
                        attrs.keepaspectratio = false;
                    }
                }
                _ => {}
            }
        }
    }

    Some((path, attrs))
}

/// Parse Typst figure function
pub fn parse_typst_figure(content: &str) -> Option<Figure> {
    let content = content.trim();
    let content = content.strip_prefix('#').unwrap_or(content);

    if !content.starts_with("figure(") {
        return None;
    }

    // Typst figures are centered by default
    let mut figure = Figure {
        centering: true,
        ..Default::default()
    };

    // Find the image inside
    if let Some(img_start) = content.find("image(") {
        let img_content = &content[img_start..];
        // Find matching paren
        if let Some(end) = find_matching_paren(img_content, '(', ')') {
            let img_call = &img_content[..end + 1];
            if let Some((path, attrs)) = parse_typst_image(img_call) {
                figure.image_path = path;
                figure.image_attrs = attrs;
            }
        }
    }

    // Find caption
    if let Some(cap_start) = content.find("caption:") {
        let after = &content[cap_start + "caption:".len()..];
        let after = after.trim();
        if after.starts_with('[') {
            if let Some(end) = find_matching_paren(after, '[', ']') {
                figure.caption = Some(after[1..end].to_string());
            }
        }
    }

    // Find label <label>
    if let Some(label_start) = content.rfind('<') {
        if let Some(label_end) = content[label_start..].find('>') {
            let label = &content[label_start + 1..label_start + label_end];
            figure.label = Some(label.to_string());
        }
    }

    if !figure.image_path.is_empty() {
        Some(figure)
    } else {
        None
    }
}

/// Parse quoted string, returns (content, rest)
fn parse_quoted_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim();
    if !s.starts_with('"') {
        return None;
    }

    let mut escaped = false;
    let mut end_idx = 0;

    for (i, c) in s[1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            end_idx = i + 1;
            break;
        }
    }

    if end_idx > 0 {
        let content = &s[1..end_idx];
        let rest = &s[end_idx + 1..];
        Some((content.to_string(), rest))
    } else {
        None
    }
}

/// Parse Typst dimension like "50%", "3cm"
fn parse_typst_dimension(s: &str) -> Option<Dimension> {
    let s = s.trim();
    Dimension::parse(s)
}

/// Find matching closing parenthesis/bracket
fn find_matching_paren(s: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0;

    for (i, c) in s.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }

    None
}

/// Convert LaTeX \includegraphics to Typst #image
pub fn convert_includegraphics_to_typst(latex: &str) -> Option<String> {
    let latex = latex.trim();

    if !latex.starts_with("\\includegraphics") {
        return None;
    }

    let after_cmd = &latex["\\includegraphics".len()..];

    // Parse optional arguments [...]
    let (options, rest) = if after_cmd.trim_start().starts_with('[') {
        parse_bracket_content(after_cmd.trim_start())
    } else {
        (String::new(), after_cmd)
    };

    let attrs = ImageAttributes::parse(&options);

    // Parse path {path}
    let path = extract_braced(rest)?;

    let args = attrs.to_typst_args();
    if args.is_empty() {
        Some(format!("#image(\"{}\")", escape_typst_string(&path)))
    } else {
        Some(format!(
            "#image(\"{}\", {})",
            escape_typst_string(&path),
            args
        ))
    }
}

/// Convert Typst #image to LaTeX \includegraphics
pub fn convert_image_to_latex(typst: &str) -> Option<String> {
    let (path, attrs) = parse_typst_image(typst)?;

    let opts = attrs.to_latex_options();
    if opts.is_empty() {
        Some(format!("\\includegraphics{{{}}}", path))
    } else {
        Some(format!("\\includegraphics[{}]{{{}}}", opts, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimension_parse() {
        assert_eq!(Dimension::parse("10cm"), Some(Dimension::Centimeter(10.0)));
        assert_eq!(Dimension::parse("5mm"), Some(Dimension::Millimeter(5.0)));
        assert_eq!(Dimension::parse("2in"), Some(Dimension::Inch(2.0)));
        assert_eq!(Dimension::parse("12pt"), Some(Dimension::Point(12.0)));
        assert_eq!(Dimension::parse("50%"), Some(Dimension::Percent(50.0)));
        assert_eq!(
            Dimension::parse("0.5\\textwidth"),
            Some(Dimension::TextWidth(0.5))
        );
    }

    #[test]
    fn test_dimension_to_typst() {
        assert_eq!(Dimension::Centimeter(10.0).to_typst(), "10cm");
        assert_eq!(Dimension::Percent(50.0).to_typst(), "50%");
        assert_eq!(Dimension::TextWidth(0.5).to_typst(), "50%");
    }

    #[test]
    fn test_image_attributes_parse() {
        let attrs = ImageAttributes::parse("width=0.5\\textwidth, height=3cm");
        assert_eq!(attrs.width, Some(Dimension::TextWidth(0.5)));
        assert_eq!(attrs.height, Some(Dimension::Centimeter(3.0)));
    }

    #[test]
    fn test_image_attributes_to_typst() {
        let attrs = ImageAttributes::parse("width=50%, height=3cm");
        let args = attrs.to_typst_args();
        assert!(args.contains("width: 50%"));
        assert!(args.contains("height: 3cm"));
    }

    #[test]
    fn test_figure_parse_latex() {
        let latex = r#"
\centering
\includegraphics[width=0.8\textwidth]{images/diagram.png}
\caption{A sample diagram}
\label{fig:diagram}
"#;
        let fig = Figure::parse_latex(latex).unwrap();
        assert_eq!(fig.image_path, "images/diagram.png");
        assert_eq!(fig.caption, Some("A sample diagram".to_string()));
        assert_eq!(fig.label, Some("fig:diagram".to_string()));
        assert!(fig.centering);
    }

    #[test]
    fn test_figure_to_typst() {
        let fig = Figure {
            image_path: "test.png".to_string(),
            caption: Some("Test caption".to_string()),
            label: Some("fig:test".to_string()),
            ..Default::default()
        };

        let typst = fig.to_typst();
        assert!(typst.contains("#figure("));
        assert!(typst.contains("image(\"test.png\")"));
        assert!(typst.contains("caption: [Test caption]"));
        assert!(typst.contains("<fig:test>"));
    }

    #[test]
    fn test_convert_includegraphics() {
        let latex = r"\includegraphics[width=0.5\textwidth]{image.png}";
        let typst = convert_includegraphics_to_typst(latex).unwrap();
        assert!(typst.contains("#image(\"image.png\""));
        assert!(typst.contains("width: 50%"));
    }

    #[test]
    fn test_parse_typst_image() {
        let typst = r#"#image("test.png", width: 50%, height: 3cm)"#;
        let (path, attrs) = parse_typst_image(typst).unwrap();
        assert_eq!(path, "test.png");
        assert_eq!(attrs.width, Some(Dimension::Percent(50.0)));
        assert_eq!(attrs.height, Some(Dimension::Centimeter(3.0)));
    }

    #[test]
    fn test_convert_image_to_latex() {
        let typst = r#"#image("test.png", width: 50%)"#;
        let latex = convert_image_to_latex(typst).unwrap();
        assert!(latex.contains("\\includegraphics"));
        assert!(latex.contains("test.png"));
    }

    #[test]
    fn test_split_keyvals() {
        let result = split_keyvals("a=1, b={2,3}, c=4");
        assert_eq!(result, vec!["a=1", "b={2,3}", "c=4"]);
    }
}
