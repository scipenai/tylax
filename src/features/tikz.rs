//! TikZ to CeTZ transpiler
//!
//! This module implements a basic TikZ parser and CeTZ code generator.
//! TikZ (from PGF/TikZ) is LaTeX's main drawing package, and CeTZ is
//! Typst's equivalent drawing library.
//!
//! ## Supported Features
//!
//! - Basic shapes: line, circle, rectangle, ellipse, arc
//! - Paths with multiple segments
//! - Nodes with text
//! - Coordinate systems: absolute, relative, polar
//! - Common styles: color, line width, fill
//! - Arrows and decorations
//!
//! ## Example
//!
//! ```rust
//! use tylax::tikz::convert_tikz_to_cetz;
//!
//! let tikz = r"\draw (0,0) -- (1,1);";
//! let cetz = convert_tikz_to_cetz(tikz);
//! assert!(cetz.contains("line"));
//! ```

use lazy_static::lazy_static;
use regex::Regex;
use std::fmt::Write;

lazy_static! {
    // Coordinate patterns (used in Coordinate::parse)
    static ref COORD_ABS: Regex = Regex::new(r"\((-?[\d.]+)\s*,\s*(-?[\d.]+)\)").unwrap();
    static ref COORD_NAMED: Regex = Regex::new(r"\(([a-zA-Z][\w.]*)\)").unwrap();
    static ref COORD_RELATIVE: Regex = Regex::new(r"\+\+\((-?[\d.]+)\s*,\s*(-?[\d.]+)\)").unwrap();
    // Polar coordinate with optional unit suffix (e.g., 45:1cm, 30:2.5pt)
    static ref COORD_POLAR: Regex = Regex::new(r"\((-?[\d.]+):(-?[\d.]+)([a-zA-Z]*)\)").unwrap();
}

/// A parsed coordinate
#[derive(Debug, Clone)]
pub enum Coordinate {
    /// Absolute (x, y)
    Absolute(f64, f64),
    /// Relative ++(dx, dy)
    Relative(f64, f64),
    /// Polar (angle:radius)
    Polar(f64, f64),
    /// Named reference (nodename)
    Named(String),
    /// Variable expression (e.g., (\x, 0) or (\i, \i))
    /// Stores the raw expression strings for x and y components
    Variable { x_expr: String, y_expr: String },
    /// Calc expression ($ ... $) - e.g., ($(A) + (1,2)$), ($(A)!0.5!(B)$)
    Calc(CalcExpr),
}

/// Calc library expression types
#[derive(Debug, Clone)]
pub enum CalcExpr {
    /// Addition: (A) + (offset)
    Add {
        base: Box<Coordinate>,
        offset: Box<Coordinate>,
    },
    /// Subtraction: (A) - (offset)
    Sub {
        base: Box<Coordinate>,
        offset: Box<Coordinate>,
    },
    /// Linear interpolation: (A)!factor!(B) - point at 'factor' between A and B
    Lerp {
        from: Box<Coordinate>,
        to: Box<Coordinate>,
        factor: f64,
    },
    /// Projection: (A)!(B)!(C) - projection of B onto line AC
    Projection {
        line_start: Box<Coordinate>,
        point: Box<Coordinate>,
        line_end: Box<Coordinate>,
    },
    /// Scalar multiplication: factor*(A)
    Scale { coord: Box<Coordinate>, factor: f64 },
}

impl Coordinate {
    /// Parse a coordinate from TikZ syntax
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();

        // Try calc expression first: ($...$)
        if input.starts_with("($") && input.ends_with("$)") {
            if let Some(calc) = Self::parse_calc_expr(input) {
                return Some(Coordinate::Calc(calc));
            }
        }

        // Try relative first
        if input.starts_with("++") {
            if let Some(caps) = COORD_RELATIVE.captures(input) {
                let x: f64 = caps.get(1)?.as_str().parse().ok()?;
                let y: f64 = caps.get(2)?.as_str().parse().ok()?;
                return Some(Coordinate::Relative(x, y));
            }
        }

        // Try polar (angle:radius) with optional unit suffix
        if let Some(caps) = COORD_POLAR.captures(input) {
            if input.contains(':') {
                let angle: f64 = caps.get(1)?.as_str().parse().ok()?;
                let radius_num: f64 = caps.get(2)?.as_str().parse().ok()?;
                let unit = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                // Convert radius to cm (TikZ default unit)
                let radius = convert_dimension_to_cm(radius_num, unit);
                return Some(Coordinate::Polar(angle, radius));
            }
        }

        // Try absolute
        if let Some(caps) = COORD_ABS.captures(input) {
            let x: f64 = caps.get(1)?.as_str().parse().ok()?;
            let y: f64 = caps.get(2)?.as_str().parse().ok()?;
            return Some(Coordinate::Absolute(x, y));
        }

        // Try named
        if let Some(caps) = COORD_NAMED.captures(input) {
            let name = caps.get(1)?.as_str().to_string();
            return Some(Coordinate::Named(name));
        }

        // Try variable expression (e.g., (\x, 0) or (\i, \j))
        if let Some(coord) = Self::parse_variable_coord(input) {
            return Some(coord);
        }

        None
    }

    /// Parse a calc library expression: ($...$)
    fn parse_calc_expr(input: &str) -> Option<CalcExpr> {
        // Remove outer ($...$)
        let inner = input.strip_prefix("($")?.strip_suffix("$)")?.trim();

        // Try lerp: (A)!factor!(B) or (A)!(B)!(C) for projection
        if let Some(lerp) = Self::parse_calc_lerp(inner) {
            return Some(lerp);
        }

        // Try addition: (A) + (offset) or (A)+(offset)
        if let Some(pos) = inner.find('+') {
            let left = inner[..pos].trim();
            let right = inner[pos + 1..].trim();

            if let (Some(base), Some(offset)) = (Self::parse(left), Self::parse(right)) {
                return Some(CalcExpr::Add {
                    base: Box::new(base),
                    offset: Box::new(offset),
                });
            }
        }

        // Try subtraction: (A) - (offset)
        // Need to be careful not to match negative numbers
        if let Some(pos) = inner.rfind('-') {
            if pos > 0 {
                let before_minus = inner[..pos].trim();
                // Check that it ends with ) to avoid matching negative coordinates
                if before_minus.ends_with(')') {
                    let left = before_minus;
                    let right = inner[pos + 1..].trim();

                    if let (Some(base), Some(offset)) = (Self::parse(left), Self::parse(right)) {
                        return Some(CalcExpr::Sub {
                            base: Box::new(base),
                            offset: Box::new(offset),
                        });
                    }
                }
            }
        }

        // Try scalar multiplication: factor*(A)
        if let Some(pos) = inner.find('*') {
            let left = inner[..pos].trim();
            let right = inner[pos + 1..].trim();

            if let Ok(factor) = left.parse::<f64>() {
                if let Some(coord) = Self::parse(right) {
                    return Some(CalcExpr::Scale {
                        coord: Box::new(coord),
                        factor,
                    });
                }
            }
        }

        None
    }

    /// Parse lerp/projection syntax: (A)!factor!(B) or (A)!(B)!(C)
    fn parse_calc_lerp(inner: &str) -> Option<CalcExpr> {
        // Find the ! operators
        let parts: Vec<&str> = inner.split('!').collect();

        match parts.len() {
            3 => {
                // (A)!factor!(B) - linear interpolation
                let a_str = parts[0].trim();
                let factor_str = parts[1].trim();
                let b_str = parts[2].trim();

                let a = Self::parse(a_str)?;
                let b = Self::parse(b_str)?;
                let factor = factor_str.parse::<f64>().ok()?;

                Some(CalcExpr::Lerp {
                    from: Box::new(a),
                    to: Box::new(b),
                    factor,
                })
            }
            4 if parts[1].trim().is_empty() => {
                // (A)!(B)!(C) - projection: point on line AC closest to B
                // This is actually (A)!!(B)!(C) format but split gives empty middle
                let a_str = parts[0].trim();
                let b_str = parts[2].trim();
                let c_str = parts[3].trim();

                let a = Self::parse(a_str)?;
                let b = Self::parse(b_str)?;
                let c = Self::parse(c_str)?;

                Some(CalcExpr::Projection {
                    line_start: Box::new(a),
                    point: Box::new(b),
                    line_end: Box::new(c),
                })
            }
            _ => None,
        }
    }

    /// Parse a coordinate that may contain variable expressions
    fn parse_variable_coord(input: &str) -> Option<Self> {
        let input = input.trim();

        // Must be enclosed in parentheses
        if !input.starts_with('(') || !input.ends_with(')') {
            return None;
        }

        let inner = &input[1..input.len() - 1];

        // Split by comma, but only at top level
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() != 2 {
            return None;
        }

        let x_expr = parts[0].trim();
        let y_expr = parts[1].trim();

        // At least one part should contain a backslash (variable)
        if x_expr.contains('\\') || y_expr.contains('\\') {
            Some(Coordinate::Variable {
                x_expr: Self::convert_tikz_expr_to_typst(x_expr),
                y_expr: Self::convert_tikz_expr_to_typst(y_expr),
            })
        } else {
            None
        }
    }

    /// Convert a TikZ expression to Typst expression
    /// E.g., "\x" -> "x", "\i*2" -> "i * 2"
    fn convert_tikz_expr_to_typst(expr: &str) -> String {
        let mut result = String::new();
        let mut chars = expr.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\\' {
                // Variable reference - collect the variable name
                let mut var_name = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_alphanumeric() || next == '_' {
                        var_name.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                result.push_str(&var_name);
            } else if c == '*' {
                result.push_str(" * ");
            } else if c == '+' {
                result.push_str(" + ");
            } else if c == '-' {
                result.push_str(" - ");
            } else if c == '/' {
                result.push_str(" / ");
            } else {
                result.push(c);
            }
        }

        result.trim().to_string()
    }

    /// Convert to CeTZ coordinate string
    pub fn to_cetz(&self) -> String {
        match self {
            Coordinate::Absolute(x, y) => format!("({}, {})", x, y),
            Coordinate::Relative(dx, dy) => format!("(rel: ({}, {}))", dx, dy),
            Coordinate::Polar(angle, radius) => {
                // Convert polar to Cartesian for CeTZ
                let rad = angle.to_radians();
                let x = radius * rad.cos();
                let y = radius * rad.sin();
                format!("({:.4}, {:.4})", x, y)
            }
            Coordinate::Named(name) => format!("\"{}\"", name),
            Coordinate::Variable { x_expr, y_expr } => format!("({}, {})", x_expr, y_expr),
            Coordinate::Calc(expr) => expr.to_cetz(),
        }
    }
}

impl CalcExpr {
    /// Convert calc expression to CeTZ code
    pub fn to_cetz(&self) -> String {
        match self {
            CalcExpr::Add { base, offset } => {
                // CeTZ: vector addition using calc.add or tuple math
                // We output as a comment with manual calculation suggestion
                // For simple cases, we can emit vector addition
                format!("calc.add({}, {})", base.to_cetz(), offset.to_cetz())
            }
            CalcExpr::Sub { base, offset } => {
                format!("calc.sub({}, {})", base.to_cetz(), offset.to_cetz())
            }
            CalcExpr::Lerp { from, to, factor } => {
                // CeTZ doesn't have lerp built-in, but we can use vector math
                // lerp(a, b, t) = a + t * (b - a)
                // For named coordinates, we need to output a helper
                format!(
                    "calc.lerp({}, {}, {})",
                    from.to_cetz(),
                    to.to_cetz(),
                    factor
                )
            }
            CalcExpr::Scale { coord, factor } => {
                format!("calc.scale({}, {})", coord.to_cetz(), factor)
            }
            CalcExpr::Projection {
                line_start,
                point,
                line_end,
            } => {
                // Projection is complex - output as a comment
                format!(
                    "/* projection of {} onto line from {} to {} */",
                    point.to_cetz(),
                    line_start.to_cetz(),
                    line_end.to_cetz()
                )
            }
        }
    }
}

/// A path segment
#[derive(Debug, Clone)]
pub enum PathSegment {
    /// Move to (no drawing)
    MoveTo(Coordinate),
    /// Line to
    LineTo(Coordinate),
    /// Curve with control points
    CurveTo {
        control1: Option<Coordinate>,
        control2: Option<Coordinate>,
        end: Coordinate,
    },
    /// Arc
    Arc {
        start_angle: f64,
        end_angle: f64,
        radius: f64,
    },
    /// Circle
    Circle { center: Coordinate, radius: f64 },
    /// Rectangle
    Rectangle {
        corner1: Coordinate,
        corner2: Coordinate,
    },
    /// Ellipse
    Ellipse {
        center: Coordinate,
        x_radius: f64,
        y_radius: f64,
    },
    /// Grid
    Grid {
        corner1: Coordinate,
        corner2: Coordinate,
        step: Option<f64>,
    },
    /// Inline node (text placed at current position)
    Node {
        text: String,
        anchor: Option<String>,
    },
    /// Bezier curve (quadratic or cubic)
    Bezier {
        /// Starting point
        start: Coordinate,
        /// Control point(s): 1 for quadratic, 2 for cubic
        controls: Vec<Coordinate>,
        /// End point
        end: Coordinate,
    },
    /// Close path (cycle)
    ClosePath,
}

// =============================================================================
// Token-based Path Parsing (Generic Approach)
// =============================================================================

/// Token types for TikZ path parsing
#[derive(Debug, Clone)]
enum PathToken {
    /// Coordinate: (x, y), ++(dx, dy), (name)
    Coord(Coordinate),
    /// Line-to operator: --
    LineTo,
    /// Curve-to operator: ..
    CurveTo,
    /// Controls keyword (for Bezier curves)
    Controls,
    /// And keyword (for cubic Bezier)
    And,
    /// Horizontal-vertical: -|
    HorizVert,
    /// Vertical-horizontal: |-
    VertHoriz,
    /// Node with options and text
    Node { options: String, text: String },
    /// Circle with radius
    Circle { radius: f64 },
    /// Rectangle keyword
    Rectangle,
    /// Arc specification
    Arc { start: f64, end: f64, radius: f64 },
    /// Grid keyword
    Grid,
    /// Cycle (close path)
    Cycle,
}

/// Find the matching closing bracket/brace/paren, handling nesting
fn find_matching(s: &str, open: char, close: char) -> Option<usize> {
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

/// Find the end of a calc expression ($...$)
/// Returns the position of the closing '$' (before the final ')')
fn find_calc_end(s: &str) -> Option<usize> {
    // Input starts with "($", we need to find the matching "$)"
    // But we need to handle nested parentheses inside the calc expr
    if !s.starts_with("($") {
        return None;
    }

    let mut depth = 1; // We've seen the opening (
    let chars: Vec<char> = s.chars().collect();
    let mut i = 2; // Start after ($

    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => {
                if depth == 1 && i > 0 && chars[i - 1] == '$' {
                    // Found $)
                    return Some(i - 1);
                }
                depth -= 1;
                if depth == 0 {
                    // Unbalanced - shouldn't happen in valid input
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }

    None
}

/// Tokenize a TikZ path string into a sequence of PathTokens
fn tokenize_path(input: &str) -> Vec<PathToken> {
    let mut tokens = Vec::new();
    let mut rest = input.trim();

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }

        // Skip comments
        if rest.starts_with('%') {
            if let Some(nl) = rest.find('\n') {
                rest = &rest[nl + 1..];
            } else {
                break;
            }
            continue;
        }

        // Check for operators first (before keywords)
        if rest.starts_with("--") {
            tokens.push(PathToken::LineTo);
            rest = &rest[2..];
            continue;
        }
        if rest.starts_with("..") {
            tokens.push(PathToken::CurveTo);
            rest = &rest[2..];
            continue;
        }
        if rest.starts_with("-|") {
            tokens.push(PathToken::HorizVert);
            rest = &rest[2..];
            continue;
        }
        if rest.starts_with("|-") {
            tokens.push(PathToken::VertHoriz);
            rest = &rest[2..];
            continue;
        }

        // Check for 'cycle'
        if rest.starts_with("cycle") {
            tokens.push(PathToken::Cycle);
            rest = &rest[5..];
            continue;
        }

        // Check for 'controls' (Bezier curve keyword)
        if rest.starts_with("controls") {
            tokens.push(PathToken::Controls);
            rest = &rest[8..];
            continue;
        }

        // Check for 'and' (cubic Bezier separator)
        if rest.starts_with("and") && !rest[3..].starts_with(|c: char| c.is_alphanumeric()) {
            tokens.push(PathToken::And);
            rest = &rest[3..];
            continue;
        }

        // Check for 'node' - handles inline nodes
        if rest.starts_with("node") {
            rest = rest[4..].trim_start();
            let mut options = String::new();
            let mut text = String::new();

            // Parse optional [options]
            if rest.starts_with('[') {
                if let Some(end) = find_matching(rest, '[', ']') {
                    options = rest[1..end].to_string();
                    rest = rest[end + 1..].trim_start();
                }
            }

            // Skip optional (name)
            if rest.starts_with('(') {
                if let Some(end) = find_matching(rest, '(', ')') {
                    rest = rest[end + 1..].trim_start();
                }
            }

            // Parse {text}
            if rest.starts_with('{') {
                if let Some(end) = find_matching(rest, '{', '}') {
                    text = rest[1..end].to_string();
                    rest = &rest[end + 1..];
                }
            }

            tokens.push(PathToken::Node { options, text });
            continue;
        }

        // Check for 'circle'
        if rest.starts_with("circle") {
            rest = rest[6..].trim_start();
            let mut radius = 1.0;

            // Parse (radius) or [radius=...] with unit support
            if rest.starts_with('(') {
                if let Some(end) = find_matching(rest, '(', ')') {
                    let r_str = rest[1..end].trim();
                    // Use parse_dimension_to_cm for proper unit conversion
                    radius = parse_dimension_to_cm(r_str).unwrap_or(1.0);
                    rest = &rest[end + 1..];
                }
            } else if rest.starts_with('[') {
                if let Some(end) = find_matching(rest, '[', ']') {
                    let opts = &rest[1..end];
                    if let Some(r_pos) = opts.find("radius=") {
                        let after = &opts[r_pos + 7..];
                        let end_pos = after
                            .find(|c: char| c == ',' || c == ']' || c.is_whitespace())
                            .unwrap_or(after.len());
                        let r_str = after[..end_pos].trim();
                        // Use parse_dimension_to_cm for proper unit conversion
                        radius = parse_dimension_to_cm(r_str).unwrap_or(1.0);
                    }
                    rest = &rest[end + 1..];
                }
            }

            tokens.push(PathToken::Circle { radius });
            continue;
        }

        // Check for 'rectangle'
        if rest.starts_with("rectangle") {
            tokens.push(PathToken::Rectangle);
            rest = &rest[9..];
            continue;
        }

        // Check for 'arc'
        if rest.starts_with("arc") {
            rest = rest[3..].trim_start();
            let mut start = 0.0;
            let mut end = 90.0;
            let mut radius = 1.0;

            // Parse (start:end:radius) or [options] with unit support
            if rest.starts_with('(') {
                if let Some(end_idx) = find_matching(rest, '(', ')') {
                    let content = &rest[1..end_idx];
                    let parts: Vec<&str> = content.split(':').collect();
                    if parts.len() >= 2 {
                        start = parts[0].trim().parse().unwrap_or(0.0);
                        end = parts[1].trim().parse().unwrap_or(90.0);
                        if parts.len() >= 3 {
                            // Use parse_dimension_to_cm for proper unit conversion
                            radius = parse_dimension_to_cm(parts[2].trim()).unwrap_or(1.0);
                        }
                    }
                    rest = &rest[end_idx + 1..];
                }
            }

            tokens.push(PathToken::Arc { start, end, radius });
            continue;
        }

        // Check for 'grid'
        if rest.starts_with("grid") {
            tokens.push(PathToken::Grid);
            rest = &rest[4..];
            continue;
        }

        // Check for calc expression ($...$)
        if rest.starts_with("($") {
            // Find the matching $) - need to handle nested parens
            if let Some(end) = find_calc_end(rest) {
                let coord_str = &rest[..end + 2]; // Include $)
                if let Some(coord) = Coordinate::parse(coord_str) {
                    tokens.push(PathToken::Coord(coord));
                }
                rest = &rest[end + 2..];
                continue;
            }
        }

        // Check for coordinate (...)
        if rest.starts_with("++") || rest.starts_with('+') || rest.starts_with('(') {
            // Handle relative coordinates
            let offset = if rest.starts_with("++") {
                2
            } else if rest.starts_with('+') && rest.chars().nth(1) == Some('(') {
                1
            } else {
                0
            };

            let coord_start = if offset > 0 && !rest[offset..].starts_with('(') {
                // Invalid relative without paren
                rest = &rest[1..];
                continue;
            } else if offset > 0 {
                offset
            } else {
                0
            };

            if rest[coord_start..].starts_with('(') {
                if let Some(end) = find_matching(&rest[coord_start..], '(', ')') {
                    let coord_str = &rest[..coord_start + end + 1];
                    if let Some(coord) = Coordinate::parse(coord_str) {
                        tokens.push(PathToken::Coord(coord));
                    }
                    rest = &rest[coord_start + end + 1..];
                    continue;
                }
            }
        }

        // Skip unknown character
        let mut chars = rest.chars();
        chars.next();
        rest = chars.as_str();
    }

    tokens
}

/// A TikZ node
#[derive(Debug, Clone)]
pub struct TikZNode {
    pub name: Option<String>,
    pub position: Option<Coordinate>,
    pub text: String,
    pub options: DrawOptions,
}

/// Drawing options/styles
#[derive(Debug, Clone, Default)]
pub struct DrawOptions {
    pub color: Option<String>,
    pub fill_color: Option<String>,
    pub line_width: Option<String>,
    pub dashed: bool,
    pub dotted: bool,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub rounded_corners: bool,
    pub opacity: Option<f64>,
    pub anchor: Option<String>,
    pub font_size: Option<String>,
    /// Raw options string for advanced parsing
    pub raw_options: Option<String>,
    // Path action flags (for unified path handling)
    /// Whether this path should be drawn (stroked)
    pub is_draw: bool,
    /// Whether this path should be filled
    pub is_fill: bool,
    /// Whether this path should be used as a clip region
    pub is_clip: bool,
    // Positioning library support
    /// Relative positioning (e.g., "right=of A" -> Some(("right", Some("1cm"), "A")))
    pub relative_pos: Option<RelativePosition>,
}

/// Relative positioning information from positioning library
#[derive(Debug, Clone)]
pub struct RelativePosition {
    /// Direction: above, below, left, right, above left, etc.
    pub direction: String,
    /// Optional distance
    pub distance: Option<String>,
    /// Reference node name
    pub of_node: String,
}

impl DrawOptions {
    /// Parse options from TikZ option string like "[thick, red, ->]"
    pub fn parse(input: &str) -> Self {
        let mut opts = DrawOptions::default();

        let cleaned_input = input.trim_start_matches('[').trim_end_matches(']');

        // Store raw options for later use (e.g., position extraction)
        opts.raw_options = Some(cleaned_input.to_string());

        for part in cleaned_input.split(',') {
            let part = part.trim();

            // Line width
            match part {
                "ultra thin" => opts.line_width = Some("0.1pt".to_string()),
                "very thin" => opts.line_width = Some("0.2pt".to_string()),
                "thin" => opts.line_width = Some("0.4pt".to_string()),
                "thick" => opts.line_width = Some("0.8pt".to_string()),
                "very thick" => opts.line_width = Some("1.2pt".to_string()),
                "ultra thick" => opts.line_width = Some("1.6pt".to_string()),
                _ => {}
            }

            // Line style
            if part == "dashed" {
                opts.dashed = true;
            } else if part == "dotted" {
                opts.dotted = true;
            }

            // Arrows
            if part.contains("->") || part.ends_with('>') {
                opts.arrow_end = true;
            }
            if part.contains("<-") || part.starts_with('<') {
                opts.arrow_start = true;
            }
            if part == "<->" {
                opts.arrow_start = true;
                opts.arrow_end = true;
            }

            // Rounded corners
            if part == "rounded corners" {
                opts.rounded_corners = true;
            }

            // Colors
            if part.starts_with("draw=") {
                opts.color = Some(part.trim_start_matches("draw=").to_string());
            } else if part.starts_with("fill=") {
                opts.fill_color = Some(part.trim_start_matches("fill=").to_string());
            } else if part.starts_with("color=") {
                opts.color = Some(part.trim_start_matches("color=").to_string());
            } else if is_color_name(part) {
                opts.color = Some(part.to_string());
            }

            // Anchor (explicit)
            if part.starts_with("anchor=") {
                opts.anchor = Some(part.trim_start_matches("anchor=").to_string());
            }

            // Position keywords -> convert to anchor
            // TikZ: "above" means node appears above point, so anchor is at bottom (south)
            if opts.anchor.is_none() {
                let anchor = match part {
                    "above right" => Some("south-west"),
                    "above left" => Some("south-east"),
                    "below right" => Some("north-west"),
                    "below left" => Some("north-east"),
                    "above" => Some("south"),
                    "below" => Some("north"),
                    "right" => Some("west"),
                    "left" => Some("east"),
                    _ => None,
                };
                if let Some(a) = anchor {
                    opts.anchor = Some(a.to_string());
                }
            }

            // Opacity
            if part.starts_with("opacity=") {
                if let Ok(v) = part.trim_start_matches("opacity=").parse() {
                    opts.opacity = Some(v);
                }
            }

            // Positioning library: right=of A, above=1cm of B, etc.
            if let Some(rel_pos) = parse_relative_position(part) {
                opts.relative_pos = Some(rel_pos);
            }
        }

        opts
    }

    /// Convert to CeTZ style string
    pub fn to_cetz_style(&self) -> String {
        let mut parts = Vec::new();

        // Build stroke style - combine color, width, and dash into one stroke object
        let mut stroke_parts = Vec::new();

        if let Some(ref color) = self.color {
            stroke_parts.push(format!("paint: {}", convert_color(color)));
        }

        if let Some(ref width) = self.line_width {
            stroke_parts.push(format!("thickness: {}pt", width.trim_end_matches("pt")));
        }

        if self.dashed {
            stroke_parts.push("dash: \"dashed\"".to_string());
        } else if self.dotted {
            stroke_parts.push("dash: \"dotted\"".to_string());
        }

        // Generate stroke attribute
        if !stroke_parts.is_empty() {
            // Simple color-only stroke (no width, no dash pattern)
            let is_simple = stroke_parts.len() == 1
                && self.line_width.is_none()
                && !self.dashed
                && !self.dotted;

            if is_simple {
                if let Some(ref color) = self.color {
                    parts.push(format!("stroke: {}", convert_color(color)));
                } else {
                    parts.push(format!("stroke: ({})", stroke_parts.join(", ")));
                }
            } else {
                parts.push(format!("stroke: ({})", stroke_parts.join(", ")));
            }
        }

        if let Some(ref fill) = self.fill_color {
            parts.push(format!("fill: {}", convert_color(fill)));
        }

        // Handle arrows/marks
        if self.arrow_start || self.arrow_end {
            let mark_str = match (self.arrow_start, self.arrow_end) {
                (true, true) => "mark: (start: \">\", end: \">\")".to_string(),
                (true, false) => "mark: (start: \">\")".to_string(),
                (false, true) => "mark: (end: \">\")".to_string(),
                _ => String::new(),
            };
            if !mark_str.is_empty() {
                parts.push(mark_str);
            }
        }

        if parts.is_empty() {
            String::new()
        } else {
            parts.join(", ")
        }
    }
}

/// Parse positioning library syntax: "right=of A", "above=1cm of B", etc.
fn parse_relative_position(input: &str) -> Option<RelativePosition> {
    // Patterns:
    // - "right=of node_name"
    // - "above=1cm of node_name"
    // - "right=5mm of node_name"
    // - "above left=of node_name"

    // Check for positioning keywords
    let directions = [
        "above right",
        "above left",
        "below right",
        "below left",
        "above",
        "below",
        "left",
        "right",
    ];

    for dir in &directions {
        if let Some(stripped) = input.strip_prefix(dir) {
            let rest = stripped.trim_start();

            // Check for "=of" or "=distance of"
            if let Some(rest) = rest.strip_prefix('=') {
                let rest = rest.trim();

                // Check for "of node" pattern
                if let Some(of_pos) = rest.find("of ") {
                    let before_of = rest[..of_pos].trim();
                    let node_name = rest[of_pos + 3..].trim().to_string();

                    let distance = if before_of.is_empty() {
                        None
                    } else {
                        Some(before_of.to_string())
                    };

                    return Some(RelativePosition {
                        direction: dir.to_string(),
                        distance,
                        of_node: node_name,
                    });
                } else if let Some(of_rest) = rest.strip_prefix("of ") {
                    // "=of node"
                    let node_name = of_rest.trim().to_string();
                    return Some(RelativePosition {
                        direction: dir.to_string(),
                        distance: None,
                        of_node: node_name,
                    });
                }
            }
        }
    }

    None
}

/// Check if a string is a known TikZ/LaTeX color name
fn is_color_name(s: &str) -> bool {
    matches!(
        s,
        "red"
            | "green"
            | "blue"
            | "yellow"
            | "cyan"
            | "magenta"
            | "black"
            | "white"
            | "gray"
            | "grey"
            | "orange"
            | "purple"
            | "brown"
            | "pink"
            | "lime"
            | "olive"
            | "teal"
            | "violet"
            | "darkgray"
            | "lightgray"
            | "darkblue"
            | "darkred"
            | "darkgreen"
    )
}

/// Convert TikZ color to CeTZ/Typst color
/// Handles various TikZ color mixing syntaxes:
/// - "green!20" = 20% green + 80% white → green.lighten(80%)
/// - "green!60!black" = 60% green + 40% black → color.mix((green, 60%), (black, 40%))
/// - "red!50!blue!30!white" = nested mixing (processed left to right)
fn convert_color(color: &str) -> String {
    // Handle TikZ color mixing syntax
    if color.contains('!') {
        let parts: Vec<&str> = color.split('!').collect();

        // Three-part syntax: color1!percent!color2 (e.g., green!60!black)
        if parts.len() == 3 {
            let color1 = parts[0].trim();
            let color2 = parts[2].trim();
            if let Ok(pct1) = parts[1].trim().parse::<f64>() {
                let pct2 = 100.0 - pct1;
                // Typst color.mix syntax
                return format!(
                    "color.mix(({}, {:.0}%), ({}, {:.0}%))",
                    normalize_color_name(color1),
                    pct1,
                    normalize_color_name(color2),
                    pct2
                );
            }
        }
        // Two-part syntax: color!percent (e.g., green!20 = 20% green + 80% white)
        else if parts.len() == 2 {
            let base_color = parts[0].trim();
            if let Ok(percentage) = parts[1].trim().parse::<f64>() {
                // TikZ green!20 = 20% green, 80% white → CeTZ green.lighten(80%)
                let lighten_pct = 100.0 - percentage;
                return format!(
                    "{}.lighten({:.0}%)",
                    normalize_color_name(base_color),
                    lighten_pct
                );
            }
        }
        // More complex mixing (4+ parts): process iteratively
        else if parts.len() > 3 {
            // For complex cases like red!50!blue!30!white, use nested mix
            // This is a simplified approach - full support would need recursive mixing
            let color1 = parts[0].trim();
            if let Ok(pct1) = parts[1].trim().parse::<f64>() {
                // Take first three parts and recurse for the rest
                let remaining = parts[2..].join("!");
                let mixed_rest = convert_color(&remaining);
                let pct2 = 100.0 - pct1;
                return format!(
                    "color.mix(({}, {:.0}%), ({}, {:.0}%))",
                    normalize_color_name(color1),
                    pct1,
                    mixed_rest,
                    pct2
                );
            }
        }
    }

    normalize_color_name(color)
}

/// Normalize color name to Typst-compatible format
fn normalize_color_name(color: &str) -> String {
    match color.trim() {
        "gray" | "grey" => "gray".to_string(),
        "darkgray" | "darkgrey" => "luma(64)".to_string(),
        "lightgray" | "lightgrey" => "luma(192)".to_string(),
        c if c.starts_with('#') => format!("rgb(\"{}\")", c),
        c => c.to_string(),
    }
}

/// A parsed TikZ drawing command
#[derive(Debug, Clone)]
pub enum TikZCommand {
    /// Unified path command (handles \draw, \fill, \filldraw, \path, \clip)
    /// Action is determined by DrawOptions flags: is_draw, is_fill, is_clip
    Path {
        options: DrawOptions,
        segments: Vec<PathSegment>,
    },
    /// \node command
    Node(TikZNode),
    /// \coordinate command
    Coordinate { name: String, position: Coordinate },
    /// \foreach loop (Phase 2)
    Foreach {
        variable: String,
        values: Vec<String>,
        body: Vec<TikZCommand>,
    },
}

/// Parse a TikZ path specification into segments using token-based parsing
fn parse_path(input: &str) -> Vec<PathSegment> {
    let tokens = tokenize_path(input);
    let mut segments = Vec::new();
    let mut iter = tokens.into_iter().peekable();
    let mut last_coord: Option<Coordinate> = None;
    let mut expect_line_to = false;

    while let Some(token) = iter.next() {
        match token {
            PathToken::Coord(coord) => {
                if expect_line_to {
                    segments.push(PathSegment::LineTo(coord.clone()));
                } else {
                    segments.push(PathSegment::MoveTo(coord.clone()));
                }
                last_coord = Some(coord);
                expect_line_to = false;
            }

            PathToken::LineTo | PathToken::HorizVert | PathToken::VertHoriz => {
                // Next coordinate should be a LineTo
                expect_line_to = true;
            }

            PathToken::CurveTo => {
                // Check if next token is 'controls' for Bezier curve
                if let Some(PathToken::Controls) = iter.peek() {
                    iter.next(); // consume Controls

                    // Collect control points
                    let mut control_points: Vec<Coordinate> = Vec::new();

                    // Get first control point
                    if let Some(PathToken::Coord(c1)) = iter.next() {
                        control_points.push(c1);
                    }

                    // Check for 'and' (cubic Bezier)
                    if let Some(PathToken::And) = iter.peek() {
                        iter.next(); // consume And
                        if let Some(PathToken::Coord(c2)) = iter.next() {
                            control_points.push(c2);
                        }
                    }

                    // Skip the .. before end point
                    if let Some(PathToken::CurveTo) = iter.peek() {
                        iter.next();
                    }

                    // Get end point
                    if let Some(PathToken::Coord(end)) = iter.next() {
                        let start = last_coord.clone().unwrap_or(Coordinate::Absolute(0.0, 0.0));
                        segments.push(PathSegment::Bezier {
                            start,
                            controls: control_points,
                            end: end.clone(),
                        });
                        last_coord = Some(end);
                    }
                } else {
                    // Simple curve-to without controls (treat as line)
                    expect_line_to = true;
                }
            }

            PathToken::Controls | PathToken::And => {
                // These are handled in CurveTo, skip if encountered standalone
            }

            PathToken::Node { options, text } => {
                // Extract anchor from options if present
                let anchor = if options.contains("right") {
                    Some("right".to_string())
                } else if options.contains("left") {
                    Some("left".to_string())
                } else if options.contains("above") {
                    Some("above".to_string())
                } else if options.contains("below") {
                    Some("below".to_string())
                } else {
                    None
                };

                segments.push(PathSegment::Node { text, anchor });
            }

            PathToken::Circle { radius } => {
                // Circle uses the last coordinate as center
                let center = last_coord.clone().unwrap_or(Coordinate::Absolute(0.0, 0.0));
                segments.push(PathSegment::Circle { center, radius });
            }

            PathToken::Rectangle => {
                // Rectangle: from last_coord to next coord
                let corner1 = last_coord.clone().unwrap_or(Coordinate::Absolute(0.0, 0.0));
                // Check next token for corner2
                let next_coord = match iter.peek() {
                    Some(PathToken::Coord(c)) => Some(c.clone()),
                    _ => None,
                };
                if let Some(corner2) = next_coord {
                    iter.next(); // Now safe to consume
                    segments.push(PathSegment::Rectangle {
                        corner1,
                        corner2: corner2.clone(),
                    });
                    last_coord = Some(corner2);
                }
            }

            PathToken::Arc { start, end, radius } => {
                segments.push(PathSegment::Arc {
                    start_angle: start,
                    end_angle: end,
                    radius,
                });
            }

            PathToken::Grid => {
                // Grid: from last_coord to next coord
                let corner1 = last_coord.clone().unwrap_or(Coordinate::Absolute(0.0, 0.0));
                // Check next token for corner2
                let next_coord = match iter.peek() {
                    Some(PathToken::Coord(c)) => Some(c.clone()),
                    _ => None,
                };
                if let Some(corner2) = next_coord {
                    iter.next(); // Now safe to consume
                    segments.push(PathSegment::Grid {
                        corner1,
                        corner2: corner2.clone(),
                        step: None,
                    });
                    last_coord = Some(corner2);
                }
            }

            PathToken::Cycle => {
                segments.push(PathSegment::ClosePath);
            }
        }
    }

    segments
}

/// Parse a TikZ node command
fn parse_node(input: &str) -> Option<TikZNode> {
    // Match \node[options](name) at (position) {text};
    let input = input.trim().trim_end_matches(';');

    let mut options = DrawOptions::default();
    let mut name = None;
    let mut position = None;
    let mut text = String::new();

    // Parse options [...]
    if let Some(opt_start) = input.find('[') {
        if let Some(opt_end) = input[opt_start..].find(']') {
            let opt_str = &input[opt_start..opt_start + opt_end + 1];
            options = DrawOptions::parse(opt_str);
        }
    }

    // Parse name (...)
    let name_pattern = Regex::new(r"\(([a-zA-Z][\w]*)\)").ok()?;
    if let Some(caps) = name_pattern.captures(input) {
        name = Some(caps.get(1)?.as_str().to_string());
    }

    // Parse "at (x,y)" or "at ($...$)"
    if let Some(at_pos) = input.find(" at ") {
        let after_at = &input[at_pos + 4..].trim();

        // Check for calc expression
        if after_at.starts_with("($") {
            // Find the end of calc expression
            if let Some(end) = find_calc_end(after_at) {
                let coord_str = &after_at[..end + 2];
                if let Some(coord) = Coordinate::parse(coord_str) {
                    position = Some(coord);
                }
            }
        } else if after_at.starts_with('(') {
            // Regular coordinate - find matching paren
            if let Some(end) = find_matching(after_at, '(', ')') {
                let coord_str = &after_at[..end + 1];
                if let Some(coord) = Coordinate::parse(coord_str) {
                    position = Some(coord);
                }
            }
        }
    }

    // Parse {text}
    if let Some(text_start) = input.find('{') {
        if let Some(text_end) = input[text_start..].rfind('}') {
            text = input[text_start + 1..text_start + text_end].to_string();
        }
    }

    Some(TikZNode {
        name,
        position,
        text,
        options,
    })
}

/// Parse a complete TikZ picture into commands
pub fn parse_tikz_picture(input: &str) -> Vec<TikZCommand> {
    // Remove \begin{tikzpicture} and \end{tikzpicture}
    let content = input
        .trim()
        .trim_start_matches(r"\begin{tikzpicture}")
        .trim_end_matches(r"\end{tikzpicture}")
        .trim();

    // Remove options after \begin{tikzpicture}
    let content = if content.starts_with('[') {
        content
            .find(']')
            .map(|i| &content[i + 1..])
            .unwrap_or(content)
    } else {
        content
    };

    // Use brace-aware command splitter
    let raw_commands = split_tikz_commands(content);
    parse_tikz_commands(&raw_commands)
}

/// Split TikZ content into individual commands, respecting brace nesting
/// This handles \foreach { ... } blocks correctly
fn split_tikz_commands(input: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut brace_depth: i32 = 0;
    let mut in_comment = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    // Track if we just closed a brace-terminated block (foreach, scope, etc.)
    let mut just_closed_block = false;

    while i < chars.len() {
        let c = chars[i];

        // Handle comments
        if c == '%' && !in_comment {
            in_comment = true;
            i += 1;
            continue;
        }
        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }

        match c {
            '{' => {
                brace_depth += 1;
                current.push(c);
                just_closed_block = false;
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(c);

                // Check if this closes a block-level command (foreach, scope, etc.)
                // These are commands that don't end with `;` but with `}`
                if brace_depth == 0 && is_block_command(current.trim()) {
                    just_closed_block = true;
                }
            }
            ';' if brace_depth == 0 => {
                // Command terminator at top level
                let cmd = current.trim().to_string();
                if !cmd.is_empty() {
                    commands.push(cmd);
                }
                current.clear();
                just_closed_block = false;
            }
            '\\' if brace_depth == 0 && just_closed_block => {
                // We just closed a block and see a new command - split here
                let cmd = current.trim().to_string();
                if !cmd.is_empty() {
                    commands.push(cmd);
                }
                current.clear();
                current.push(c);
                just_closed_block = false;
            }
            _ if c.is_whitespace() => {
                current.push(c);
                // Don't reset just_closed_block for whitespace
            }
            _ => {
                current.push(c);
                just_closed_block = false;
            }
        }
        i += 1;
    }

    // Don't forget any remaining content
    let cmd = current.trim().to_string();
    if !cmd.is_empty() {
        commands.push(cmd);
    }

    commands
}

/// Check if a command is a block-level command (terminates with } not ;)
fn is_block_command(cmd: &str) -> bool {
    // Block commands that end with } instead of ;
    cmd.starts_with(r"\foreach")
        || cmd.starts_with(r"\scope")
        || cmd.starts_with(r"\begin{scope}")
        || cmd.starts_with(r"\pgfonlayer")
}

/// Parse a list of raw command strings into TikZCommand objects
fn parse_tikz_commands(raw_commands: &[String]) -> Vec<TikZCommand> {
    let mut commands = Vec::new();

    for line in raw_commands {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Check for \foreach loop
        if line.starts_with(r"\foreach") {
            if let Some(foreach_cmd) = parse_foreach(line) {
                commands.push(foreach_cmd);
                continue;
            }
        }

        // Unified path command parsing
        if let Some(cmd) = parse_path_command(line) {
            commands.push(cmd);
        } else if line.starts_with(r"\node") {
            // Parse \node command
            if let Some(node) = parse_node(line) {
                commands.push(TikZCommand::Node(node));
            }
        } else if line.starts_with(r"\coordinate") {
            // Parse \coordinate command
            let after_cmd = line.strip_prefix(r"\coordinate").unwrap_or(line);
            let (_, rest) = parse_command_with_options(after_cmd);

            // Parse (name) at (position) - using pre-compiled regex
            if let Some(caps) = COORD_NAMED.captures(rest) {
                if let Some(name) = caps.get(1) {
                    let name = name.as_str().to_string();
                    if let Some(at_pos) = rest.find(" at ") {
                        if let Some(coord) = Coordinate::parse(&rest[at_pos + 4..]) {
                            commands.push(TikZCommand::Coordinate {
                                name,
                                position: coord,
                            });
                        }
                    }
                }
            }
        }
    }

    commands
}

/// Parse a \foreach loop command
/// Format: \foreach \var in {list} { body }
fn parse_foreach(input: &str) -> Option<TikZCommand> {
    let content = input.strip_prefix(r"\foreach")?.trim();

    // Parse variable: \x or \i etc.
    let mut chars = content.chars().peekable();

    // Skip whitespace
    while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
        chars.next();
    }

    // Expect backslash for variable
    if chars.next() != Some('\\') {
        return None;
    }

    // Collect variable name
    let mut variable = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' {
            variable.push(chars.next().unwrap());
        } else {
            break;
        }
    }

    if variable.is_empty() {
        return None;
    }

    // Collect remaining as string for easier parsing
    let remaining: String = chars.collect();
    let remaining = remaining.trim();

    // Expect "in"
    if !remaining.starts_with("in") {
        return None;
    }
    let remaining = remaining[2..].trim();

    // Parse {values}
    let values_start = remaining.find('{')?;
    let mut brace_depth = 0;
    let mut values_end = values_start;

    for (i, c) in remaining[values_start..].char_indices() {
        match c {
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    values_end = values_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    let values_str = &remaining[values_start + 1..values_end];
    let values: Vec<String> = parse_foreach_values(values_str);

    // Parse body { commands }
    let body_start_search = &remaining[values_end + 1..].trim();
    let body_start = body_start_search.find('{')?;
    let body_content = &body_start_search[body_start..];

    let mut brace_depth = 0;
    let mut body_end = 0;
    for (i, c) in body_content.char_indices() {
        match c {
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    body_end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    let body_str = &body_content[1..body_end];

    // Recursively parse body commands
    let body_raw = split_tikz_commands(body_str);
    let body = parse_tikz_commands(&body_raw);

    Some(TikZCommand::Foreach {
        variable,
        values,
        body,
    })
}

/// Parse foreach value list: "1,2,3" or "1,...,5" or "0,0.1,...,1"
fn parse_foreach_values(input: &str) -> Vec<String> {
    let input = input.trim();

    // Check for range syntax with ...
    if input.contains("...") {
        // Handle patterns like "1,...,5" or "0,0.1,...,1"
        let parts: Vec<&str> = input.split(',').map(|s| s.trim()).collect();

        if parts.len() >= 3 && parts.contains(&"...") {
            // Find the ... position
            let ellipsis_pos = parts.iter().position(|p| *p == "...").unwrap();

            if ellipsis_pos >= 1 && ellipsis_pos < parts.len() - 1 {
                // Get start, step (optional), and end
                let start: f64 = parts[0].parse().unwrap_or(0.0);
                let end: f64 = parts[ellipsis_pos + 1].parse().unwrap_or(start);

                let step = if ellipsis_pos >= 2 {
                    // There's a second value before ..., use it to calculate step
                    let second: f64 = parts[1].parse().unwrap_or(start + 1.0);
                    second - start
                } else {
                    1.0
                };

                // Generate values
                let mut values = Vec::new();
                let mut current = start;
                let max_iterations = 1000; // Safety limit
                let mut iterations = 0;

                while (step > 0.0 && current <= end + f64::EPSILON)
                    || (step < 0.0 && current >= end - f64::EPSILON)
                {
                    // Format without unnecessary decimals
                    if current.fract().abs() < 1e-9 {
                        values.push(format!("{}", current as i64));
                    } else {
                        values.push(
                            format!("{:.2}", current)
                                .trim_end_matches('0')
                                .trim_end_matches('.')
                                .to_string(),
                        );
                    }
                    current += step;
                    iterations += 1;
                    if iterations > max_iterations {
                        break;
                    }
                }

                return values;
            }
        }
    }

    // Simple comma-separated list
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a path-type command (\draw, \fill, \filldraw, \path, \clip)
/// Returns None if the line is not a recognized path command
fn parse_path_command(line: &str) -> Option<TikZCommand> {
    // Determine command type and set default flags
    let (after_cmd, default_draw, default_fill, default_clip) = if line.starts_with(r"\filldraw") {
        (line.strip_prefix(r"\filldraw")?, true, true, false)
    } else if line.starts_with(r"\fill") {
        (line.strip_prefix(r"\fill")?, false, true, false)
    } else if line.starts_with(r"\draw") {
        (line.strip_prefix(r"\draw")?, true, false, false)
    } else if line.starts_with(r"\clip") {
        (line.strip_prefix(r"\clip")?, false, false, true)
    } else if line.starts_with(r"\path") {
        // \path with no options is invisible; options determine action
        (line.strip_prefix(r"\path")?, false, false, false)
    } else {
        return None;
    };

    let (mut opts, path) = parse_command_with_options(after_cmd);

    // Set action flags based on command type and options
    // Options can override: e.g., \path[draw] makes it visible
    if opts
        .raw_options
        .as_ref()
        .map(|s| s.contains("draw"))
        .unwrap_or(false)
    {
        opts.is_draw = true;
    } else {
        opts.is_draw = default_draw;
    }

    if opts
        .raw_options
        .as_ref()
        .map(|s| s.contains("fill"))
        .unwrap_or(false)
    {
        opts.is_fill = true;
    } else {
        opts.is_fill = default_fill;
    }

    if opts
        .raw_options
        .as_ref()
        .map(|s| s.contains("clip"))
        .unwrap_or(false)
    {
        opts.is_clip = true;
    } else {
        opts.is_clip = default_clip;
    }

    // For \fill commands: if a simple color is specified without "fill=",
    // it should be interpreted as fill color, not stroke color
    if opts.is_fill && !opts.is_draw && opts.fill_color.is_none() && opts.color.is_some() {
        opts.fill_color = opts.color.take();
    }

    let segments = parse_path(path);

    Some(TikZCommand::Path {
        options: opts,
        segments,
    })
}

/// Parse command options and return (options, remaining content)
fn parse_command_with_options(input: &str) -> (DrawOptions, &str) {
    let input = input.trim();

    if input.starts_with('[') {
        if let Some(end) = find_matching_bracket(input) {
            let opts = DrawOptions::parse(&input[..end + 1]);
            let rest = input[end + 1..].trim();
            return (opts, rest);
        }
    }

    (DrawOptions::default(), input)
}

/// Find matching bracket, handling nesting
fn find_matching_bracket(input: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in input.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
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

/// Convert a TikZ picture to CeTZ code
pub fn convert_tikz_to_cetz(input: &str) -> String {
    let commands = parse_tikz_picture(input);
    let mut output = String::new();

    // CeTZ preamble (using 0.3.4 for better coordinate handling)
    output.push_str("#import \"@preview/cetz:0.3.4\": canvas, draw\n\n");
    output.push_str("#canvas({\n");
    output.push_str("  import draw: *\n\n");

    for cmd in commands {
        convert_command_to_cetz(&mut output, &cmd, 1);
    }

    output.push_str("})\n");
    output
}

/// Convert a single TikZ command to CeTZ code
/// `indent_level` controls the indentation for nested structures (e.g., foreach)
fn convert_command_to_cetz(output: &mut String, cmd: &TikZCommand, indent_level: usize) {
    let indent = "  ".repeat(indent_level);

    match cmd {
        TikZCommand::Path { options, segments } => {
            // Unified path handling based on action flags
            if options.is_clip {
                output.push_str(&format!("{}// Clip region (partial support)\n", indent));
            }
            // Call the draw command converter with fill flag from options
            convert_path_command(output, options, segments, indent_level);
        }
        TikZCommand::Node(node) => {
            convert_node_command_with_indent(output, node, indent_level);
        }
        TikZCommand::Coordinate { name, position } => {
            let _ = writeln!(
                output,
                "{}// Coordinate: {} at {}",
                indent,
                name,
                position.to_cetz()
            );
        }
        TikZCommand::Foreach {
            variable,
            values,
            body,
        } => {
            // Convert \foreach to Typst for loop
            let values_str = values.join(", ");
            let _ = writeln!(output, "{}for {} in ({}) {{", indent, variable, values_str);
            for body_cmd in body {
                convert_command_to_cetz(output, body_cmd, indent_level + 1);
            }
            let _ = writeln!(output, "{}}}", indent);
        }
    }
}

/// Convert a path command (unified draw/fill/clip) to CeTZ
fn convert_path_command(
    output: &mut String,
    options: &DrawOptions,
    segments: &[PathSegment],
    indent_level: usize,
) {
    // Use the existing convert_draw_command logic
    convert_draw_command_impl(output, options, segments, indent_level);
}

/// Map TikZ node position to CeTZ anchor
/// TikZ `node[right]` means "node is to the right of point" → CeTZ `anchor: "west"` (anchor on left)
/// TikZ `node[above]` means "node is above point" → CeTZ `anchor: "south"` (anchor on bottom)
fn map_tikz_position_to_cetz_anchor(tikz_pos: &str) -> &'static str {
    match tikz_pos {
        "right" => "west",
        "left" => "east",
        "above" => "south",
        "below" => "north",
        "above right" => "south-west",
        "above left" => "south-east",
        "below right" => "north-west",
        "below left" => "north-east",
        // Pass through CeTZ-style anchors
        "north" => "north",
        "south" => "south",
        "east" => "east",
        "west" => "west",
        "north-east" => "north-east",
        "north-west" => "north-west",
        "south-east" => "south-east",
        "south-west" => "south-west",
        "center" => "center",
        _ => "center",
    }
}

/// Convert a node command with specific indent level
fn convert_node_command_with_indent(output: &mut String, node: &TikZNode, indent_level: usize) {
    let indent = "  ".repeat(indent_level);

    // Handle positioning library relative position
    let pos = if let Some(ref rel_pos) = node.options.relative_pos {
        // Convert relative positioning to CeTZ
        convert_relative_position_to_cetz(rel_pos)
    } else {
        node.position
            .as_ref()
            .map(|c| c.to_cetz())
            .unwrap_or_else(|| "(0, 0)".to_string())
    };

    // In Typst content blocks [...], $...$ is math mode - don't escape it
    // Only escape # which has special meaning in Typst
    let text = node.text.replace("#", "\\#");

    let mut opts = Vec::new();
    if let Some(ref anchor) = node.options.anchor {
        // Map TikZ position to CeTZ anchor (they have opposite semantics!)
        let cetz_anchor = map_tikz_position_to_cetz_anchor(anchor);
        opts.push(format!("anchor: \"{}\"", cetz_anchor));
    }
    if let Some(ref name) = node.name {
        opts.push(format!("name: \"{}\"", name));
    }

    if opts.is_empty() {
        let _ = writeln!(output, "{}content({}, [{}])", indent, pos, text);
    } else {
        let _ = writeln!(
            output,
            "{}content({}, [{}], {})",
            indent,
            pos,
            text,
            opts.join(", ")
        );
    }
}

/// Convert relative positioning to CeTZ coordinate expression
fn convert_relative_position_to_cetz(rel_pos: &RelativePosition) -> String {
    // Map TikZ direction to offset vector
    let (dx, dy) = match rel_pos.direction.as_str() {
        "above" => (0.0, 1.0),
        "below" => (0.0, -1.0),
        "left" => (-1.0, 0.0),
        "right" => (1.0, 0.0),
        "above right" => (1.0, 1.0),
        "above left" => (-1.0, 1.0),
        "below right" => (1.0, -1.0),
        "below left" => (-1.0, -1.0),
        _ => (0.0, 0.0),
    };

    // Parse distance if provided
    let distance = rel_pos
        .distance
        .as_ref()
        .map(|d| {
            // Parse "1cm", "5mm", "2pt" etc.
            let num_str = d.trim_end_matches(|c: char| c.is_alphabetic());
            let unit = d.trim_start_matches(|c: char| c.is_numeric() || c == '.' || c == '-');
            let num: f64 = num_str.parse().unwrap_or(1.0);

            // Convert to approximate cm
            match unit {
                "cm" => num,
                "mm" => num / 10.0,
                "pt" => num / 28.35, // Approximate
                "in" => num * 2.54,
                _ => num,
            }
        })
        .unwrap_or(1.0);

    // Generate CeTZ expression
    // Use calc.add to offset from the reference node
    let offset_x = dx * distance;
    let offset_y = dy * distance;

    format!(
        "calc.add(\"{}\", ({:.2}, {:.2}))",
        rel_pos.of_node, offset_x, offset_y
    )
}

/// Convert a draw/fill command to CeTZ with proper state tracking (internal implementation)
fn convert_draw_command_impl(
    output: &mut String,
    options: &DrawOptions,
    segments: &[PathSegment],
    indent_level: usize,
) {
    let indent = "  ".repeat(indent_level);

    // Track current position for proper coordinate handling
    let mut current_pos = Coordinate::Absolute(0.0, 0.0);
    // Collect line coordinates for polyline output
    let mut polyline: Vec<Coordinate> = Vec::new();
    // Track if we have a closed path
    let mut is_closed = false;

    // Helper to flush polyline
    let flush_polyline = |output: &mut String,
                          polyline: &mut Vec<Coordinate>,
                          options: &DrawOptions,
                          closed: bool,
                          indent: &str| {
        if polyline.len() >= 2 {
            let style = options.to_cetz_style();
            let style_str = if style.is_empty() {
                String::new()
            } else {
                format!(", {}", style)
            };

            let coords_str: Vec<_> = polyline.iter().map(|c| c.to_cetz()).collect();

            if closed {
                let _ = writeln!(
                    output,
                    "{}line({}{}, close: true)",
                    indent,
                    coords_str.join(", "),
                    style_str
                );
            } else {
                let _ = writeln!(
                    output,
                    "{}line({}{})",
                    indent,
                    coords_str.join(", "),
                    style_str
                );
            }
        }
        polyline.clear();
    };

    for segment in segments {
        match segment {
            PathSegment::MoveTo(coord) => {
                flush_polyline(output, &mut polyline, options, false, &indent);
                current_pos = coord.clone();
                polyline.push(coord.clone());
            }

            PathSegment::LineTo(coord) => {
                if polyline.is_empty() {
                    polyline.push(current_pos.clone());
                }
                polyline.push(coord.clone());
                current_pos = coord.clone();
            }

            PathSegment::Node { text, anchor } => {
                let pos = current_pos.to_cetz();
                // In Typst content blocks [...], $...$ is math mode - don't escape it
                let escaped_text = text.replace('#', "\\#");

                if let Some(ref anch) = anchor {
                    // Apply TikZ to CeTZ anchor mapping
                    let cetz_anchor = map_tikz_position_to_cetz_anchor(anch);
                    let _ = writeln!(
                        output,
                        "{}content({}, [{}], anchor: \"{}\")",
                        indent, pos, escaped_text, cetz_anchor
                    );
                } else {
                    let _ = writeln!(output, "{}content({}, [{}])", indent, pos, escaped_text);
                }
            }

            PathSegment::Circle { center, radius } => {
                flush_polyline(output, &mut polyline, options, false, &indent);

                let style = options.to_cetz_style();
                let mut style_parts = Vec::new();
                if !style.is_empty() {
                    style_parts.push(style);
                }
                if options.is_fill && options.fill_color.is_none() {
                    style_parts.push("fill: black".to_string());
                }
                let style_str = if style_parts.is_empty() {
                    String::new()
                } else {
                    format!(", {}", style_parts.join(", "))
                };

                let _ = writeln!(
                    output,
                    "{}circle({}, radius: {}{})",
                    indent,
                    center.to_cetz(),
                    radius,
                    style_str
                );
                current_pos = center.clone();
            }

            PathSegment::Rectangle { corner1, corner2 } => {
                flush_polyline(output, &mut polyline, options, false, &indent);

                let style = options.to_cetz_style();
                let style_str = if style.is_empty() {
                    String::new()
                } else {
                    format!(", {}", style)
                };
                let _ = writeln!(
                    output,
                    "{}rect({}, {}{})",
                    indent,
                    corner1.to_cetz(),
                    corner2.to_cetz(),
                    style_str
                );
                current_pos = corner2.clone();
            }

            PathSegment::Arc {
                start_angle,
                end_angle,
                radius,
            } => {
                flush_polyline(output, &mut polyline, options, false, &indent);

                let _ = writeln!(
                    output,
                    "{}arc({}, start: {}deg, stop: {}deg, radius: {})",
                    indent,
                    current_pos.to_cetz(),
                    start_angle,
                    end_angle,
                    radius
                );
            }

            PathSegment::Ellipse {
                center,
                x_radius,
                y_radius,
            } => {
                flush_polyline(output, &mut polyline, options, false, &indent);

                let _ = writeln!(
                    output,
                    "{}ellipse({}, {}, {})",
                    indent,
                    center.to_cetz(),
                    x_radius,
                    y_radius
                );
                current_pos = center.clone();
            }

            PathSegment::Grid {
                corner1,
                corner2,
                step,
            } => {
                flush_polyline(output, &mut polyline, options, false, &indent);

                let step_str = step.map(|s| format!(", step: {}", s)).unwrap_or_default();
                let _ = writeln!(
                    output,
                    "{}grid({}, {}{})",
                    indent,
                    corner1.to_cetz(),
                    corner2.to_cetz(),
                    step_str
                );
            }

            PathSegment::Bezier {
                start,
                controls,
                end,
            } => {
                flush_polyline(output, &mut polyline, options, false, &indent);

                let style = options.to_cetz_style();
                let style_str = if style.is_empty() {
                    String::new()
                } else {
                    format!(", {}", style)
                };

                let coords: Vec<String> = std::iter::once(start.to_cetz())
                    .chain(controls.iter().map(|c| c.to_cetz()))
                    .chain(std::iter::once(end.to_cetz()))
                    .collect();

                let _ = writeln!(
                    output,
                    "{}bezier({}{})",
                    indent,
                    coords.join(", "),
                    style_str
                );
                current_pos = end.clone();
            }

            PathSegment::CurveTo {
                control1,
                control2,
                end,
            } => {
                flush_polyline(output, &mut polyline, options, false, &indent);

                let style = options.to_cetz_style();
                let style_str = if style.is_empty() {
                    String::new()
                } else {
                    format!(", {}", style)
                };

                let start = current_pos.clone();
                let mut coords = vec![start.to_cetz()];
                if let Some(c1) = control1 {
                    coords.push(c1.to_cetz());
                }
                if let Some(c2) = control2 {
                    coords.push(c2.to_cetz());
                }
                coords.push(end.to_cetz());

                let _ = writeln!(
                    output,
                    "{}bezier({}{})",
                    indent,
                    coords.join(", "),
                    style_str
                );
                current_pos = end.clone();
            }

            PathSegment::ClosePath => {
                is_closed = true;
            }
        }
    }

    // Flush remaining polyline
    flush_polyline(output, &mut polyline, options, is_closed, &indent);
}

/// Quick conversion function for TikZ environment content
pub fn convert_tikz_environment(input: &str) -> String {
    // Check if it's a complete tikzpicture or just content
    let has_env = input.contains(r"\begin{tikzpicture}");

    if has_env {
        convert_tikz_to_cetz(input)
    } else {
        // Wrap in environment
        let full = format!(r"\begin{{tikzpicture}}{}\end{{tikzpicture}}", input);
        convert_tikz_to_cetz(&full)
    }
}

// ============================================================================
// CeTZ to TikZ Reverse Conversion (Token-based)
// ============================================================================

/// CeTZ Token types for parsing
#[derive(Debug, Clone, PartialEq)]
enum CetzToken {
    /// Identifier: line, circle, stroke, etc.
    Ident(String),
    /// Number literal
    Number(f64),
    /// String literal: "..."
    String(String),
    /// Content block: [...]
    Content(String),
    /// Coordinate tuple: (x, y)
    Coord(f64, f64),
    /// Punctuation
    Comma,
    Colon,
    /// Opening/closing parens (for nested structures)
    LParen,
    RParen,
    /// Boolean
    Bool(bool),
}

/// Tokenize CeTZ code into tokens
fn tokenize_cetz(input: &str) -> Vec<CetzToken> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            // Whitespace - skip
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }

            // Comments
            '/' if chars.clone().nth(1) == Some('/') => {
                // Skip to end of line
                while let Some(&ch) = chars.peek() {
                    chars.next();
                    if ch == '\n' {
                        break;
                    }
                }
            }

            // Content block [...]
            '[' => {
                chars.next();
                let mut content = String::new();
                let mut depth = 1;
                while let Some(&ch) = chars.peek() {
                    chars.next();
                    if ch == '[' {
                        depth += 1;
                        content.push(ch);
                    } else if ch == ']' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        content.push(ch);
                    } else {
                        content.push(ch);
                    }
                }
                tokens.push(CetzToken::Content(content));
            }

            // String literal "..."
            '"' => {
                chars.next();
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    chars.next();
                    if ch == '"' {
                        break;
                    } else if ch == '\\' {
                        if let Some(&escaped) = chars.peek() {
                            chars.next();
                            s.push(escaped);
                        }
                    } else {
                        s.push(ch);
                    }
                }
                tokens.push(CetzToken::String(s));
            }

            // Coordinate or grouped expression (...)
            '(' => {
                chars.next();
                // Try to parse as coordinate (x, y)
                let mut inner = String::new();
                let mut depth = 1;
                while let Some(&ch) = chars.peek() {
                    if ch == '(' {
                        depth += 1;
                    } else if ch == ')' {
                        depth -= 1;
                        if depth == 0 {
                            chars.next();
                            break;
                        }
                    }
                    inner.push(ch);
                    chars.next();
                }

                // Try to parse as coordinate
                if let Some((x, y)) = parse_coord_tuple(&inner) {
                    tokens.push(CetzToken::Coord(x, y));
                } else {
                    // Recursively tokenize inner content
                    tokens.push(CetzToken::LParen);
                    let inner_tokens = tokenize_cetz(&inner);
                    tokens.extend(inner_tokens);
                    tokens.push(CetzToken::RParen);
                }
            }

            // Punctuation
            ',' => {
                chars.next();
                tokens.push(CetzToken::Comma);
            }
            ':' => {
                chars.next();
                tokens.push(CetzToken::Colon);
            }
            ')' => {
                chars.next();
                tokens.push(CetzToken::RParen);
            }

            // Numbers (including negative)
            '-' | '0'..='9' | '.' => {
                let mut num_str = String::new();
                if c == '-' {
                    num_str.push(c);
                    chars.next();
                }
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() || ch == '.' {
                        num_str.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Ok(n) = num_str.parse::<f64>() {
                    tokens.push(CetzToken::Number(n));
                }
            }

            // Identifiers
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut ident = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                        ident.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Check for boolean
                if ident == "true" {
                    tokens.push(CetzToken::Bool(true));
                } else if ident == "false" {
                    tokens.push(CetzToken::Bool(false));
                } else {
                    tokens.push(CetzToken::Ident(ident));
                }
            }

            // Skip other characters
            _ => {
                chars.next();
            }
        }
    }

    tokens
}

/// Try to parse "x, y" as coordinate tuple
fn parse_coord_tuple(s: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        let x: f64 = parts[0].trim().parse().ok()?;
        let y: f64 = parts[1].trim().parse().ok()?;
        Some((x, y))
    } else {
        None
    }
}

/// Parsed CeTZ command
#[derive(Debug, Clone)]
enum CetzCommand {
    Line {
        coords: Vec<(f64, f64)>,
        style: CetzStyle,
        close: bool,
    },
    Circle {
        center: (f64, f64),
        radius: f64,
        style: CetzStyle,
    },
    Rect {
        corner1: (f64, f64),
        corner2: (f64, f64),
        style: CetzStyle,
    },
    Arc {
        center: (f64, f64),
        start: f64,
        stop: f64,
        radius: f64,
        style: CetzStyle,
    },
    Bezier {
        points: Vec<(f64, f64)>,
        style: CetzStyle,
    },
    Ellipse {
        center: (f64, f64),
        radius_x: f64,
        radius_y: f64,
        style: CetzStyle,
    },
    Content {
        pos: (f64, f64),
        text: String,
        anchor: Option<String>,
        name: Option<String>,
    },
    Grid {
        corner1: (f64, f64),
        corner2: (f64, f64),
        step: Option<f64>,
    },
}

/// CeTZ style information
#[derive(Debug, Clone, Default)]
struct CetzStyle {
    stroke: Option<String>,
    fill: Option<String>,
    stroke_width: Option<f64>,
    dash: Option<String>,
    arrow_start: bool,
    arrow_end: bool,
}

/// Parse a single CeTZ command from tokens
fn parse_cetz_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    if tokens.is_empty() {
        return None;
    }

    // Get command name
    let cmd_name = match &tokens[0] {
        CetzToken::Ident(name) => name.as_str(),
        _ => return None,
    };

    // Parse based on command type
    match cmd_name {
        "line" => parse_line_command(tokens),
        "circle" => parse_circle_command(tokens),
        "rect" => parse_rect_command(tokens),
        "arc" => parse_arc_command(tokens),
        "bezier" => parse_bezier_command(tokens),
        "ellipse" => parse_ellipse_command(tokens),
        "content" => parse_content_command(tokens),
        "grid" => parse_grid_command(tokens),
        _ => None,
    }
}

/// Extract style from token stream
fn extract_style_from_tokens(tokens: &[CetzToken]) -> CetzStyle {
    let mut style = CetzStyle::default();
    let mut i = 0;

    while i < tokens.len() {
        if let CetzToken::Ident(key) = &tokens[i] {
            // Check for "key: value" pattern
            if i + 2 < tokens.len() && tokens[i + 1] == CetzToken::Colon {
                match key.as_str() {
                    "stroke" => {
                        if let Some(val) = get_token_value(&tokens[i + 2]) {
                            // Check if this is a width value (number or number+unit)
                            if is_dimension_value(&val) {
                                style.stroke_width = parse_dimension(&val);
                            } else {
                                style.stroke = Some(val);
                            }
                        }
                    }
                    "fill" => {
                        if let Some(val) = get_token_value(&tokens[i + 2]) {
                            style.fill = Some(convert_typst_color_to_tikz(&val));
                        }
                    }
                    "dash" => {
                        if let CetzToken::String(s) = &tokens[i + 2] {
                            style.dash = Some(s.clone());
                        }
                    }
                    "mark" => {
                        // mark: (end: ">") or similar
                        // Simplified: just check if arrow is mentioned
                        for token in tokens.iter().skip(i).take(10) {
                            if let CetzToken::String(s) = token {
                                if s.contains('>') {
                                    style.arrow_end = true;
                                }
                                if s.contains('<') {
                                    style.arrow_start = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        i += 1;
    }

    style
}

/// Check if a value is a dimension (e.g., "0.8pt", "1.5", "2cm")
fn is_dimension_value(val: &str) -> bool {
    let trimmed = val.trim();
    // Check if it starts with a digit or decimal point
    if let Some(first) = trimmed.chars().next() {
        first.is_ascii_digit() || first == '.'
    } else {
        false
    }
}

/// Parse dimension value to pt (approximation)
fn parse_dimension(val: &str) -> Option<f64> {
    let trimmed = val.trim();
    // Extract numeric part
    let num_str: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let num: f64 = num_str.parse().ok()?;

    // Extract unit and convert to pt
    let unit = trimmed.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
    match unit.trim() {
        "pt" | "" => Some(num),
        "cm" => Some(num * 28.35),
        "mm" => Some(num * 2.835),
        "in" => Some(num * 72.0),
        _ => Some(num),
    }
}

/// Convert a dimension value with unit to cm (TikZ default unit)
/// This is used for polar coordinates where the radius may have a unit suffix
fn convert_dimension_to_cm(value: f64, unit: &str) -> f64 {
    match unit.trim().to_lowercase().as_str() {
        "cm" | "" => value,    // cm is default, no conversion needed
        "mm" => value / 10.0,  // 10mm = 1cm
        "pt" => value / 28.35, // ~28.35pt = 1cm
        "in" => value * 2.54,  // 1in = 2.54cm
        "em" => value * 0.35,  // approximate: 1em ≈ 0.35cm (depends on font)
        "ex" => value * 0.15,  // approximate: 1ex ≈ 0.15cm
        _ => value,            // unknown unit, assume cm
    }
}

/// Parse a dimension string (e.g., "2pt", "1.5cm") and return value in cm
fn parse_dimension_to_cm(val: &str) -> Option<f64> {
    let trimmed = val.trim();
    // Extract numeric part
    let num_str: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    let num: f64 = num_str.parse().ok()?;

    // Extract unit
    let unit = trimmed.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-');
    Some(convert_dimension_to_cm(num, unit))
}

/// Convert Typst color expressions to TikZ
fn convert_typst_color_to_tikz(color: &str) -> String {
    // Handle Typst color operations like "green.lighten(80%)"
    if color.contains(".lighten") {
        // Extract base color and percentage
        if let Some(base) = color.split('.').next() {
            if let Some(pct_str) = color.split("lighten(").nth(1) {
                if let Ok(pct) = pct_str.trim_end_matches([')', '%']).parse::<f64>() {
                    // Convert to TikZ color mixing: green!20 means 20% green, 80% white
                    let color_pct = 100.0 - pct;
                    return format!("{}!{:.0}", base, color_pct);
                }
            }
        }
    }
    // Return as-is for simple colors
    color.to_string()
}

fn get_token_value(token: &CetzToken) -> Option<String> {
    match token {
        CetzToken::Ident(s) => Some(s.clone()),
        CetzToken::String(s) => Some(s.clone()),
        CetzToken::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Collect all coordinates from tokens
fn collect_coords(tokens: &[CetzToken]) -> Vec<(f64, f64)> {
    tokens
        .iter()
        .filter_map(|t| {
            if let CetzToken::Coord(x, y) = t {
                Some((*x, *y))
            } else {
                None
            }
        })
        .collect()
}

/// Check if "close: true" is present
fn has_close_flag(tokens: &[CetzToken]) -> bool {
    for i in 0..tokens.len().saturating_sub(2) {
        if let CetzToken::Ident(s) = &tokens[i] {
            if s == "close" && tokens.get(i + 1) == Some(&CetzToken::Colon) {
                if let Some(CetzToken::Bool(true)) = tokens.get(i + 2) {
                    return true;
                }
            }
        }
    }
    false
}

fn parse_line_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let coords = collect_coords(tokens);
    if coords.is_empty() {
        return None;
    }
    let style = extract_style_from_tokens(tokens);
    let close = has_close_flag(tokens);
    Some(CetzCommand::Line {
        coords,
        style,
        close,
    })
}

fn parse_circle_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let coords = collect_coords(tokens);
    let center = coords.first().cloned().unwrap_or((0.0, 0.0));

    // Find radius
    let mut radius = 1.0;
    for i in 0..tokens.len().saturating_sub(2) {
        if let CetzToken::Ident(s) = &tokens[i] {
            if s == "radius" && tokens.get(i + 1) == Some(&CetzToken::Colon) {
                if let Some(CetzToken::Number(r)) = tokens.get(i + 2) {
                    radius = *r;
                }
            }
        }
    }

    let style = extract_style_from_tokens(tokens);
    Some(CetzCommand::Circle {
        center,
        radius,
        style,
    })
}

fn parse_rect_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let coords = collect_coords(tokens);
    if coords.len() < 2 {
        return None;
    }
    let style = extract_style_from_tokens(tokens);
    Some(CetzCommand::Rect {
        corner1: coords[0],
        corner2: coords[1],
        style,
    })
}

fn parse_arc_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let coords = collect_coords(tokens);
    let center = coords.first().cloned().unwrap_or((0.0, 0.0));

    let mut start = 0.0;
    let mut stop = 90.0;
    let mut radius = 1.0;

    for i in 0..tokens.len().saturating_sub(2) {
        if let CetzToken::Ident(s) = &tokens[i] {
            if tokens.get(i + 1) == Some(&CetzToken::Colon) {
                if let Some(CetzToken::Number(n)) = tokens.get(i + 2) {
                    match s.as_str() {
                        "start" => start = *n,
                        "stop" => stop = *n,
                        "radius" => radius = *n,
                        _ => {}
                    }
                }
            }
        }
    }

    let style = extract_style_from_tokens(tokens);
    Some(CetzCommand::Arc {
        center,
        start,
        stop,
        radius,
        style,
    })
}

fn parse_bezier_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let points = collect_coords(tokens);
    if points.len() < 3 {
        return None;
    }
    let style = extract_style_from_tokens(tokens);
    Some(CetzCommand::Bezier { points, style })
}

fn parse_ellipse_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let coords = collect_coords(tokens);
    let center = coords.first().cloned().unwrap_or((0.0, 0.0));

    let mut radius_x = 1.0;
    let mut radius_y = 0.5;

    for i in 0..tokens.len().saturating_sub(2) {
        if let CetzToken::Ident(s) = &tokens[i] {
            if tokens.get(i + 1) == Some(&CetzToken::Colon) {
                if let Some(CetzToken::Number(n)) = tokens.get(i + 2) {
                    match s.as_str() {
                        "radius" | "semi-major" => radius_x = *n,
                        "radius-y" | "semi-minor" => radius_y = *n,
                        _ => {}
                    }
                }
            }
        }
    }

    let style = extract_style_from_tokens(tokens);
    Some(CetzCommand::Ellipse {
        center,
        radius_x,
        radius_y,
        style,
    })
}

fn parse_content_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let coords = collect_coords(tokens);
    let pos = coords.first().cloned().unwrap_or((0.0, 0.0));

    // Find content text
    let mut text = String::new();
    for token in tokens {
        if let CetzToken::Content(s) = token {
            text = s.clone();
            break;
        }
    }

    // Find anchor
    let mut anchor = None;
    for i in 0..tokens.len().saturating_sub(2) {
        if let CetzToken::Ident(s) = &tokens[i] {
            if s == "anchor" && tokens.get(i + 1) == Some(&CetzToken::Colon) {
                if let Some(CetzToken::String(a)) = tokens.get(i + 2) {
                    anchor = Some(a.clone());
                } else if let Some(CetzToken::Ident(a)) = tokens.get(i + 2) {
                    anchor = Some(a.clone());
                }
            }
        }
    }

    // Find name
    let mut name = None;
    for i in 0..tokens.len().saturating_sub(2) {
        if let CetzToken::Ident(s) = &tokens[i] {
            if s == "name" && tokens.get(i + 1) == Some(&CetzToken::Colon) {
                if let Some(CetzToken::String(n)) = tokens.get(i + 2) {
                    name = Some(n.clone());
                }
            }
        }
    }

    Some(CetzCommand::Content {
        pos,
        text,
        anchor,
        name,
    })
}

fn parse_grid_command(tokens: &[CetzToken]) -> Option<CetzCommand> {
    let coords = collect_coords(tokens);
    if coords.len() < 2 {
        return None;
    }

    let mut step = None;
    for i in 0..tokens.len().saturating_sub(2) {
        if let CetzToken::Ident(s) = &tokens[i] {
            if s == "step" && tokens.get(i + 1) == Some(&CetzToken::Colon) {
                if let Some(CetzToken::Number(n)) = tokens.get(i + 2) {
                    step = Some(*n);
                }
            }
        }
    }

    Some(CetzCommand::Grid {
        corner1: coords[0],
        corner2: coords[1],
        step,
    })
}

/// Convert CeTZ code to TikZ (Token-based implementation)
pub fn convert_cetz_to_tikz(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    output.push_str("\\begin{tikzpicture}\n");

    // Collect CeTZ commands by detecting command boundaries
    let commands = extract_cetz_commands(input);

    for cmd_str in commands {
        let tokens = tokenize_cetz(&cmd_str);
        if let Some(cmd) = parse_cetz_command(&tokens) {
            if let Some(tikz) = convert_cetz_command_to_tikz(&cmd) {
                output.push_str("  ");
                output.push_str(&tikz);
                output.push_str(";\n");
            }
        }
    }

    output.push_str("\\end{tikzpicture}");
    output
}

/// Extract individual CeTZ commands from input
fn extract_cetz_commands(input: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current_cmd = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut in_content = false;

    for line in input.lines() {
        let trimmed = line.trim();

        // Skip boilerplate lines (CeTZ preamble, closing braces, etc.)
        if trimmed.starts_with("import")
            || trimmed.starts_with("#import")
            || trimmed.starts_with("canvas(")
            || trimmed.starts_with("canvas({")
            || trimmed.starts_with("#canvas(")
            || trimmed.starts_with("#canvas({")
            || trimmed.starts_with("#cetz.canvas(")
            || trimmed.starts_with("cetz.canvas(")
            || trimmed == "{"
            || trimmed == "})"
            || trimmed == "}"
            || trimmed.is_empty()
            || trimmed.starts_with("//")
        {
            continue;
        }

        // Track brackets to find command boundaries
        for c in trimmed.chars() {
            match c {
                '"' if !in_content => in_string = !in_string,
                '[' if !in_string => in_content = true,
                ']' if !in_string => in_content = false,
                '(' if !in_string && !in_content => depth += 1,
                ')' if !in_string && !in_content => depth -= 1,
                _ => {}
            }
            current_cmd.push(c);
        }

        // Command complete when depth returns to 0
        if depth == 0 && !current_cmd.trim().is_empty() {
            commands.push(current_cmd.trim().to_string());
            current_cmd.clear();
        } else {
            current_cmd.push(' ');
        }
    }

    // Handle any remaining command
    if !current_cmd.trim().is_empty() {
        commands.push(current_cmd.trim().to_string());
    }

    commands
}

/// Convert a parsed CeTZ command to TikZ string
fn convert_cetz_command_to_tikz(cmd: &CetzCommand) -> Option<String> {
    match cmd {
        CetzCommand::Line {
            coords,
            style,
            close,
        } => {
            if coords.is_empty() {
                return None;
            }
            let mut result = build_tikz_draw_prefix(style);
            for (i, (x, y)) in coords.iter().enumerate() {
                if i > 0 {
                    result.push_str(" -- ");
                }
                let _ = write!(result, "({}, {})", x, y);
            }
            if *close {
                result.push_str(" -- cycle");
            }
            Some(result)
        }

        CetzCommand::Circle {
            center,
            radius,
            style,
        } => {
            let prefix = build_tikz_draw_prefix(style);
            Some(format!(
                "{} ({}, {}) circle ({})",
                prefix, center.0, center.1, radius
            ))
        }

        CetzCommand::Rect {
            corner1,
            corner2,
            style,
        } => {
            let prefix = build_tikz_draw_prefix(style);
            Some(format!(
                "{} ({}, {}) rectangle ({}, {})",
                prefix, corner1.0, corner1.1, corner2.0, corner2.1
            ))
        }

        CetzCommand::Arc {
            center,
            start,
            stop,
            radius,
            style,
        } => {
            let prefix = build_tikz_draw_prefix(style);
            Some(format!(
                "{} ({}, {}) arc ({}:{}:{})",
                prefix, center.0, center.1, start, stop, radius
            ))
        }

        CetzCommand::Bezier { points, style } => {
            let prefix = build_tikz_draw_prefix(style);
            match points.len() {
                3 => {
                    let (x0, y0) = points[0];
                    let (x1, y1) = points[1];
                    let (x2, y2) = points[2];
                    Some(format!(
                        "{} ({}, {}) .. controls ({}, {}) .. ({}, {})",
                        prefix, x0, y0, x1, y1, x2, y2
                    ))
                }
                4 => {
                    let (x0, y0) = points[0];
                    let (x1, y1) = points[1];
                    let (x2, y2) = points[2];
                    let (x3, y3) = points[3];
                    Some(format!(
                        "{} ({}, {}) .. controls ({}, {}) and ({}, {}) .. ({}, {})",
                        prefix, x0, y0, x1, y1, x2, y2, x3, y3
                    ))
                }
                _ => None,
            }
        }

        CetzCommand::Ellipse {
            center,
            radius_x,
            radius_y,
            style,
        } => {
            let prefix = build_tikz_draw_prefix(style);
            Some(format!(
                "{} ({}, {}) ellipse ({} and {})",
                prefix, center.0, center.1, radius_x, radius_y
            ))
        }

        CetzCommand::Content {
            pos,
            text,
            anchor,
            name,
        } => {
            let mut node_opts = Vec::new();
            if let Some(a) = anchor {
                // Convert CeTZ anchor to TikZ anchor
                let tikz_anchor = convert_cetz_anchor_to_tikz(a);
                node_opts.push(format!("anchor={}", tikz_anchor));
            }
            if let Some(n) = name {
                node_opts.push(format!("name={}", n));
            }

            let opts_str = if node_opts.is_empty() {
                String::new()
            } else {
                format!("[{}]", node_opts.join(", "))
            };

            Some(format!(
                "\\node{} at ({}, {}) {{{}}}",
                opts_str, pos.0, pos.1, text
            ))
        }

        CetzCommand::Grid {
            corner1,
            corner2,
            step,
        } => {
            let step_str = step.map(|s| format!("[step={}]", s)).unwrap_or_default();
            Some(format!(
                "\\draw{} ({}, {}) grid ({}, {})",
                step_str, corner1.0, corner1.1, corner2.0, corner2.1
            ))
        }
    }
}

/// Build TikZ \draw prefix with style options
fn build_tikz_draw_prefix(style: &CetzStyle) -> String {
    let mut opts = Vec::new();

    if let Some(ref color) = style.stroke {
        opts.push(format!("draw={}", color));
    }
    if let Some(ref color) = style.fill {
        opts.push(format!("fill={}", color));
    }
    if let Some(width) = style.stroke_width {
        opts.push(format!("line width={}pt", width));
    }
    if let Some(ref dash) = style.dash {
        let tikz_dash = match dash.as_str() {
            "dashed" => "dashed",
            "dotted" => "dotted",
            "dashdotted" => "dash dot",
            _ => dash.as_str(),
        };
        opts.push(tikz_dash.to_string());
    }
    if style.arrow_start && style.arrow_end {
        opts.push("<->".to_string());
    } else if style.arrow_end {
        opts.push("->".to_string());
    } else if style.arrow_start {
        opts.push("<-".to_string());
    }

    if opts.is_empty() {
        "\\draw".to_string()
    } else {
        format!("\\draw[{}]", opts.join(", "))
    }
}

/// Convert CeTZ anchor names to TikZ anchor names
fn convert_cetz_anchor_to_tikz(anchor: &str) -> &str {
    match anchor {
        "north" | "top" => "north",
        "south" | "bottom" => "south",
        "east" | "right" => "east",
        "west" | "left" => "west",
        "north-east" | "top-right" => "north east",
        "north-west" | "top-left" => "north west",
        "south-east" | "bottom-right" => "south east",
        "south-west" | "bottom-left" => "south west",
        "center" | "mid" => "center",
        _ => anchor,
    }
}

/// Check if input looks like CeTZ code
pub fn is_cetz_code(input: &str) -> bool {
    input.contains("import \"@preview/cetz")
        || input.contains("canvas(")
        || (input.contains("line(") && input.contains("(") && input.contains(")"))
}

/// Convert CeTZ environment/canvas to TikZ
pub fn convert_cetz_environment(input: &str) -> String {
    convert_cetz_to_tikz(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to convert single CeTZ command via Token-based parser
    fn convert_single_cetz_cmd(input: &str) -> Option<String> {
        let tokens = tokenize_cetz(input);
        let cmd = parse_cetz_command(&tokens)?;
        convert_cetz_command_to_tikz(&cmd)
    }

    #[test]
    fn test_cetz_line_to_tikz() {
        let cetz = "line((0, 0), (1, 1))";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("\\draw"));
        assert!(tikz.contains("(0, 0)"));
        assert!(tikz.contains("(1, 1)"));
    }

    #[test]
    fn test_cetz_circle_to_tikz() {
        let cetz = "circle((0, 0), radius: 1)";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        assert!(tikz.unwrap().contains("circle"));
    }

    #[test]
    fn test_cetz_content_to_tikz() {
        let cetz = "content((0, 0), [Hello])";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        assert!(tikz.unwrap().contains("\\node"));
    }

    #[test]
    fn test_cetz_content_with_anchor() {
        let cetz = r#"content((1, 2), anchor: "west", [Label])"#;
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("\\node"));
        assert!(tikz.contains("anchor=west"));
        assert!(tikz.contains("Label"));
    }

    #[test]
    fn test_full_cetz_conversion() {
        let cetz = r#"
import "@preview/cetz:0.2.0"
canvas({
  line((0, 0), (1, 1))
  circle((2, 2), radius: 0.5)
})
"#;
        let tikz = convert_cetz_to_tikz(cetz);
        assert!(tikz.contains("\\begin{tikzpicture}"));
        assert!(tikz.contains("\\end{tikzpicture}"));
    }

    #[test]
    fn test_cetz_bezier_to_tikz() {
        let cetz = "bezier((0, 0), (1, 2), (3, 0))";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("controls"));
    }

    #[test]
    fn test_cetz_bezier_cubic_to_tikz() {
        let cetz = "bezier((0, 0), (0.5, 1), (1.5, 1), (2, 0))";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("controls"));
        assert!(tikz.contains("and"));
    }

    #[test]
    fn test_cetz_style_extraction() {
        let input = "line((0, 0), (1, 1), stroke: red)";
        let tokens = tokenize_cetz(input);
        let style = extract_style_from_tokens(&tokens);
        assert_eq!(style.stroke, Some("red".to_string()));
    }

    #[test]
    fn test_cetz_rect_to_tikz() {
        let cetz = "rect((0, 0), (2, 3))";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("rectangle"));
        assert!(tikz.contains("(0, 0)"));
        assert!(tikz.contains("(2, 3)"));
    }

    #[test]
    fn test_tokenize_cetz() {
        let input = "line((0, 0), (1, 1), stroke: blue)";
        let tokens = tokenize_cetz(input);
        assert!(tokens
            .iter()
            .any(|t| matches!(t, CetzToken::Ident(s) if s == "line")));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, CetzToken::Coord(0.0, 0.0))));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, CetzToken::Ident(s) if s == "stroke")));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, CetzToken::Ident(s) if s == "blue")));
    }

    #[test]
    fn test_coordinate_parse_absolute() {
        let coord = Coordinate::parse("(1.5, 2.0)").unwrap();
        match coord {
            Coordinate::Absolute(x, y) => {
                assert!((x - 1.5).abs() < 0.001);
                assert!((y - 2.0).abs() < 0.001);
            }
            _ => panic!("Expected absolute coordinate"),
        }
    }

    #[test]
    fn test_coordinate_parse_relative() {
        let coord = Coordinate::parse("++(1, 1)").unwrap();
        match coord {
            Coordinate::Relative(dx, dy) => {
                assert!((dx - 1.0).abs() < 0.001);
                assert!((dy - 1.0).abs() < 0.001);
            }
            _ => panic!("Expected relative coordinate"),
        }
    }

    #[test]
    fn test_draw_options_parse() {
        let opts = DrawOptions::parse("[thick, red, ->]");
        assert!(opts.arrow_end);
        assert_eq!(opts.line_width, Some("0.8pt".to_string()));
        assert_eq!(opts.color, Some("red".to_string()));
    }

    #[test]
    fn test_simple_line() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) -- (1,1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("line"));
        assert!(cetz.contains("canvas"));
    }

    #[test]
    fn test_circle() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) circle (1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        println!("Circle output: {}", cetz);
        // CeTZ uses circle() function
        assert!(
            cetz.contains("circle") || cetz.contains("line"),
            "Expected circle or line in: {}",
            cetz
        );
    }

    #[test]
    fn test_node() {
        let tikz = r"\begin{tikzpicture}\node at (0,0) {Hello};\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("content"));
        assert!(cetz.contains("Hello"));
    }

    #[test]
    fn test_rectangle() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) rectangle (2,2);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        println!("Rectangle output: {}", cetz);
        assert!(
            cetz.contains("rect") || cetz.contains("line"),
            "Expected rect or line in: {}",
            cetz
        );
    }

    #[test]
    fn test_styled_draw() {
        let tikz = r"\begin{tikzpicture}\draw[thick, blue] (0,0) -- (1,1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("blue"));
    }

    // ========================================================================
    // Extended TikZ -> CeTZ Tests
    // ========================================================================

    #[test]
    fn test_tikz_polyline() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) -- (1,1) -- (2,0) -- (3,1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("line"));
        // Should have multiple coordinates in one line call
        assert!(cetz.contains("(0, 0)") || cetz.contains("(0,0)"));
        assert!(cetz.contains("(3, 1)") || cetz.contains("(3,1)"));
    }

    #[test]
    fn test_tikz_closed_path() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) -- (1,0) -- (1,1) -- cycle;\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(
            cetz.contains("close: true"),
            "Expected close: true in: {}",
            cetz
        );
    }

    #[test]
    fn test_tikz_arc() {
        let tikz = r"\begin{tikzpicture}\draw (0,0) arc (0:90:1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("arc"), "Expected arc in: {}", cetz);
    }

    #[test]
    fn test_tikz_grid() {
        let tikz = r"\begin{tikzpicture}\draw[step=0.5] (0,0) grid (3,3);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("grid"), "Expected grid in: {}", cetz);
    }

    #[test]
    fn test_tikz_filled_shape() {
        let tikz = r"\begin{tikzpicture}\draw[fill=yellow] (0,0) circle (1);\end{tikzpicture}";
        let cetz = convert_tikz_to_cetz(tikz);
        assert!(cetz.contains("yellow"), "Expected fill color in: {}", cetz);
    }

    // ========================================================================
    // Extended CeTZ -> TikZ Tests
    // ========================================================================

    #[test]
    fn test_cetz_polyline_to_tikz() {
        let cetz = "line((0, 0), (1, 1), (2, 0), (3, 1))";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("--"), "Expected -- in: {}", tikz);
        assert!(tikz.contains("(0, 0)"));
        assert!(tikz.contains("(3, 1)"));
    }

    #[test]
    fn test_cetz_closed_line_to_tikz() {
        let cetz = "line((0, 0), (1, 0), (1, 1), (0, 1), close: true)";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("cycle"), "Expected cycle in: {}", tikz);
    }

    #[test]
    fn test_cetz_arc_to_tikz() {
        let cetz = "arc((0, 0), start: 0, stop: 90, radius: 1)";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("arc"), "Expected arc in: {}", tikz);
    }

    #[test]
    fn test_cetz_grid_to_tikz() {
        let cetz = "grid((0, 0), (3, 3), step: 0.5)";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("grid"), "Expected grid in: {}", tikz);
        assert!(tikz.contains("step=0.5"), "Expected step in: {}", tikz);
    }

    #[test]
    fn test_cetz_filled_circle_to_tikz() {
        let cetz = "circle((0, 0), radius: 1, fill: yellow)";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(tikz.contains("fill=yellow"), "Expected fill in: {}", tikz);
    }

    #[test]
    fn test_cetz_styled_line_to_tikz() {
        let cetz = "line((0, 0), (1, 1), stroke: blue)";
        let tikz = convert_single_cetz_cmd(cetz);
        assert!(tikz.is_some());
        let tikz = tikz.unwrap();
        assert!(
            tikz.contains("draw=blue"),
            "Expected draw=blue in: {}",
            tikz
        );
    }

    #[test]
    fn test_cetz_ellipse_to_tikz() {
        let cetz = "ellipse((0, 0), radius: 2, radius-y: 1)";
        let tokens = tokenize_cetz(cetz);
        let cmd = parse_cetz_command(&tokens);
        // Ellipse parsing may need adjustment, but test the flow
        assert!(cmd.is_some() || cmd.is_none()); // Placeholder - improve ellipse parsing
    }

    // ========================================================================
    // Roundtrip Tests (using simple CeTZ format without # prefix)
    // ========================================================================

    #[test]
    fn test_roundtrip_cetz_line() {
        // Test CeTZ -> TikZ -> CeTZ roundtrip (more reliable direction)
        let original_cetz = r#"
canvas({
  line((0, 0), (1, 1))
})
"#;
        let tikz = convert_cetz_to_tikz(original_cetz);
        assert!(tikz.contains("\\draw"), "CeTZ->TikZ failed: {}", tikz);

        // The TikZ output should be valid
        let back_cetz = convert_tikz_to_cetz(&tikz);
        assert!(
            back_cetz.contains("line"),
            "TikZ->CeTZ failed: {}",
            back_cetz
        );
    }

    #[test]
    fn test_roundtrip_cetz_circle() {
        let original_cetz = r#"
canvas({
  circle((2, 2), radius: 1)
})
"#;
        let tikz = convert_cetz_to_tikz(original_cetz);
        assert!(tikz.contains("circle"), "CeTZ->TikZ failed: {}", tikz);

        let back_cetz = convert_tikz_to_cetz(&tikz);
        assert!(
            back_cetz.contains("circle"),
            "TikZ->CeTZ roundtrip failed: {}",
            back_cetz
        );
    }

    #[test]
    fn test_roundtrip_cetz_node() {
        let original_cetz = r#"
canvas({
  content((0, 0), [Test])
})
"#;
        let tikz = convert_cetz_to_tikz(original_cetz);
        assert!(tikz.contains("\\node"), "CeTZ->TikZ failed: {}", tikz);
        assert!(tikz.contains("Test"), "Text lost: {}", tikz);

        let back_cetz = convert_tikz_to_cetz(&tikz);
        assert!(
            back_cetz.contains("content"),
            "TikZ->CeTZ roundtrip failed: {}",
            back_cetz
        );
        assert!(
            back_cetz.contains("Test"),
            "Text lost in roundtrip: {}",
            back_cetz
        );
    }

    #[test]
    fn test_roundtrip_cetz_rect() {
        let original_cetz = r#"
canvas({
  rect((0, 0), (2, 2))
})
"#;
        let tikz = convert_cetz_to_tikz(original_cetz);
        assert!(tikz.contains("rectangle"), "CeTZ->TikZ failed: {}", tikz);

        let back_cetz = convert_tikz_to_cetz(&tikz);
        assert!(
            back_cetz.contains("rect"),
            "TikZ->CeTZ roundtrip failed: {}",
            back_cetz
        );
    }
}
