//! Value types for the MiniEval interpreter.
//!
//! This module defines a complete type system matching typst-hs capabilities
//! for full macro evaluation support.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::features::images::escape_typst_string;
use crate::features::refs::{citation_to_typst, Citation, CitationMode, CiteGroup, ReferenceType};
use chrono::{NaiveDate, NaiveTime};
use indexmap::IndexMap;
use regex::Regex;

// ============================================================================
// Length and Unit Types (matching Typst.Types.Length)
// ============================================================================

/// Length unit types supported by Typst.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthUnit {
    /// Points (1/72 inch)
    Pt,
    /// Millimeters
    Mm,
    /// Centimeters
    Cm,
    /// Inches
    In,
    /// Em units (relative to font size)
    Em,
}

impl LengthUnit {
    /// Convert a value in this unit to points.
    pub fn to_pt(&self, value: f64) -> Option<f64> {
        match self {
            LengthUnit::Pt => Some(value),
            LengthUnit::Mm => Some(value * 2.835),
            LengthUnit::Cm => Some(value * 28.35),
            LengthUnit::In => Some(value * 72.0),
            LengthUnit::Em => None, // Em is context-dependent
        }
    }

    /// Get the unit suffix for display.
    pub fn suffix(&self) -> &'static str {
        match self {
            LengthUnit::Pt => "pt",
            LengthUnit::Mm => "mm",
            LengthUnit::Cm => "cm",
            LengthUnit::In => "in",
            LengthUnit::Em => "em",
        }
    }
}

/// A length value that can be absolute, relative, or a combination.
#[derive(Debug, Clone, PartialEq)]
pub enum Length {
    /// An exact length with a unit (e.g., `12pt`, `1em`)
    Exact(f64, LengthUnit),
    /// A ratio/percentage (e.g., `50%`)
    Ratio(f64),
    /// A sum of two lengths (e.g., `1pt + 50%`)
    Sum(Box<Length>, Box<Length>),
}

impl Length {
    /// Create a new exact length.
    pub fn exact(value: f64, unit: LengthUnit) -> Self {
        Length::Exact(value, unit)
    }

    /// Create a new ratio.
    pub fn ratio(value: f64) -> Self {
        Length::Ratio(value)
    }

    /// Negate this length.
    pub fn negate(&self) -> Self {
        match self {
            Length::Exact(v, u) => Length::Exact(-v, *u),
            Length::Ratio(r) => Length::Ratio(-r),
            Length::Sum(a, b) => Length::Sum(Box::new(a.negate()), Box::new(b.negate())),
        }
    }

    /// Multiply this length by a scalar.
    pub fn scale(&self, factor: f64) -> Self {
        match self {
            Length::Exact(v, u) => Length::Exact(v * factor, *u),
            Length::Ratio(r) => Length::Ratio(r * factor),
            Length::Sum(a, b) => Length::Sum(Box::new(a.scale(factor)), Box::new(b.scale(factor))),
        }
    }

    /// Display this length as Typst source.
    pub fn to_typst(&self) -> String {
        match self {
            Length::Exact(v, u) => format!("{}{}", v, u.suffix()),
            Length::Ratio(r) => format!("{}%", r * 100.0),
            Length::Sum(a, b) => format!("{} + {}", a.to_typst(), b.to_typst()),
        }
    }
}

// ============================================================================
// Color Types (matching Typst.Types.Color)
// ============================================================================

/// A color value.
#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    /// RGB color with alpha (values 0.0-1.0)
    Rgb { r: f64, g: f64, b: f64, a: f64 },
    /// CMYK color (values 0.0-1.0)
    Cmyk { c: f64, m: f64, y: f64, k: f64 },
    /// Grayscale (luma) color (value 0.0-1.0)
    Luma(f64),
}

impl Color {
    /// Create an RGB color from 0-255 values.
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color::Rgb {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: 1.0,
        }
    }

    /// Create an RGB color with alpha from 0-255 values.
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color::Rgb {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: a as f64 / 255.0,
        }
    }

    /// Create from hex string (e.g., "#ff0000" or "ff0000").
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        let len = hex.len();

        let parse = |s: &str| u8::from_str_radix(s, 16).ok();

        match len {
            3 => {
                let r = parse(&hex[0..1].repeat(2))?;
                let g = parse(&hex[1..2].repeat(2))?;
                let b = parse(&hex[2..3].repeat(2))?;
                Some(Color::rgb(r, g, b))
            }
            6 => {
                let r = parse(&hex[0..2])?;
                let g = parse(&hex[2..4])?;
                let b = parse(&hex[4..6])?;
                Some(Color::rgb(r, g, b))
            }
            8 => {
                let r = parse(&hex[0..2])?;
                let g = parse(&hex[2..4])?;
                let b = parse(&hex[4..6])?;
                let a = parse(&hex[6..8])?;
                Some(Color::rgba(r, g, b, a))
            }
            _ => None,
        }
    }

    /// Darken this color by a factor (0.0-1.0).
    pub fn darken(&self, factor: f64) -> Self {
        let factor = 1.0 - factor.clamp(0.0, 1.0);
        match self {
            Color::Rgb { r, g, b, a } => Color::Rgb {
                r: r * factor,
                g: g * factor,
                b: b * factor,
                a: *a,
            },
            Color::Cmyk { c, m, y, k } => Color::Cmyk {
                c: c * factor,
                m: m * factor,
                y: y * factor,
                k: k * factor,
            },
            Color::Luma(l) => Color::Luma(l * factor),
        }
    }

    /// Lighten this color by a factor (0.0-1.0).
    pub fn lighten(&self, factor: f64) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        match self {
            Color::Rgb { r, g, b, a } => Color::Rgb {
                r: r + (1.0 - r) * factor,
                g: g + (1.0 - g) * factor,
                b: b + (1.0 - b) * factor,
                a: *a,
            },
            Color::Cmyk { c, m, y, k } => Color::Cmyk {
                c: c + (1.0 - c) * factor,
                m: m + (1.0 - m) * factor,
                y: y + (1.0 - y) * factor,
                k: k + (1.0 - k) * factor,
            },
            Color::Luma(l) => Color::Luma(l + (1.0 - l) * factor),
        }
    }

    /// Display this color as Typst source.
    pub fn to_typst(&self) -> String {
        match self {
            Color::Rgb { r, g, b, a } => {
                if (*a - 1.0).abs() < 0.001 {
                    format!(
                        "rgb({}, {}, {})",
                        (r * 255.0).round() as u8,
                        (g * 255.0).round() as u8,
                        (b * 255.0).round() as u8
                    )
                } else {
                    format!(
                        "rgb({}, {}, {}, {}%)",
                        (r * 255.0).round() as u8,
                        (g * 255.0).round() as u8,
                        (b * 255.0).round() as u8,
                        (a * 100.0).round() as u8
                    )
                }
            }
            Color::Cmyk { c, m, y, k } => {
                format!(
                    "cmyk({}%, {}%, {}%, {}%)",
                    (c * 100.0).round() as u8,
                    (m * 100.0).round() as u8,
                    (y * 100.0).round() as u8,
                    (k * 100.0).round() as u8
                )
            }
            Color::Luma(l) => format!("luma({}%)", (l * 100.0).round() as u8),
        }
    }
}

// ============================================================================
// Alignment and Direction Types
// ============================================================================

/// Horizontal alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizAlign {
    Start,
    End,
    Left,
    Center,
    Right,
}

/// Vertical alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertAlign {
    Top,
    Horizon,
    Bottom,
}

/// Two-dimensional alignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alignment {
    pub horiz: Option<HorizAlign>,
    pub vert: Option<VertAlign>,
}

impl Alignment {
    pub fn new(horiz: Option<HorizAlign>, vert: Option<VertAlign>) -> Self {
        Self { horiz, vert }
    }

    pub fn to_typst(&self) -> String {
        match (self.horiz, self.vert) {
            (Some(h), Some(v)) => format!("{} + {}", horiz_to_str(h), vert_to_str(v)),
            (Some(h), None) => horiz_to_str(h).to_string(),
            (None, Some(v)) => vert_to_str(v).to_string(),
            (None, None) => String::new(),
        }
    }
}

fn horiz_to_str(h: HorizAlign) -> &'static str {
    match h {
        HorizAlign::Start => "start",
        HorizAlign::End => "end",
        HorizAlign::Left => "left",
        HorizAlign::Center => "center",
        HorizAlign::Right => "right",
    }
}

fn vert_to_str(v: VertAlign) -> &'static str {
    match v {
        VertAlign::Top => "top",
        VertAlign::Horizon => "horizon",
        VertAlign::Bottom => "bottom",
    }
}

/// Text direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
    Ttb,
    Btt,
}

// ============================================================================
// DateTime Type
// ============================================================================

/// A datetime value.
#[derive(Debug, Clone, PartialEq)]
pub struct DateTime {
    pub date: Option<NaiveDate>,
    pub time: Option<NaiveTime>,
}

impl DateTime {
    pub fn new(date: Option<NaiveDate>, time: Option<NaiveTime>) -> Self {
        Self { date, time }
    }

    pub fn to_typst(&self) -> String {
        match (&self.date, &self.time) {
            (Some(d), Some(t)) => format!(
                "datetime(year: {}, month: {}, day: {}, hour: {}, minute: {}, second: {})",
                d.format("%Y"),
                d.format("%m"),
                d.format("%d"),
                t.format("%H"),
                t.format("%M"),
                t.format("%S")
            ),
            (Some(d), None) => format!(
                "datetime(year: {}, month: {}, day: {})",
                d.format("%Y"),
                d.format("%m"),
                d.format("%d")
            ),
            (None, Some(t)) => format!(
                "datetime(hour: {}, minute: {}, second: {})",
                t.format("%H"),
                t.format("%M"),
                t.format("%S")
            ),
            (None, None) => "datetime()".to_string(),
        }
    }
}

// ============================================================================
// Symbol Type
// ============================================================================

/// A symbol with optional variants.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    /// The default character representation
    pub default: String,
    /// Whether this symbol can be used as an accent
    pub accent: bool,
    /// Variants of this symbol (modifier set -> char)
    pub variants: Vec<(Vec<String>, String)>,
}

impl Symbol {
    pub fn new(default: impl Into<String>) -> Self {
        Self {
            default: default.into(),
            accent: false,
            variants: Vec::new(),
        }
    }

    pub fn with_accent(mut self, accent: bool) -> Self {
        self.accent = accent;
        self
    }
}

// ============================================================================
// Selector Type
// ============================================================================

/// A selector for querying elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    /// Select by element name with optional field filters
    Element(String, Vec<(String, Value)>),
    /// Select by string content
    String(String),
    /// Select by regex pattern
    Regex(WrappedRegex),
    /// Select by label
    Label(String),
    /// Union of two selectors
    Or(Box<Selector>, Box<Selector>),
    /// Intersection of two selectors
    And(Box<Selector>, Box<Selector>),
    /// Select before another selector
    Before(Box<Selector>, Box<Selector>),
    /// Select after another selector
    After(Box<Selector>, Box<Selector>),
}

impl Selector {
    /// Check if this selector matches a content node.
    /// This is a simplified matching used for show rules.
    pub fn matches(&self, node: &ContentNode) -> bool {
        match self {
            Selector::Element(name, filters) => {
                let target_node = match node.labeled_parts() {
                    Some((target, _, _, _)) if target.len() == 1 => &target[0],
                    Some(_) => return false,
                    None => node,
                };
                // First check if the element type matches
                let type_matches = match (name.as_str(), target_node) {
                    ("heading", ContentNode::Heading { .. }) => true,
                    ("strong", ContentNode::Strong(_)) => true,
                    ("emph", ContentNode::Emph(_)) => true,
                    ("raw", ContentNode::Raw { .. }) => true,
                    ("math" | "math.equation", ContentNode::Math { .. }) => true,
                    ("list.item", ContentNode::ListItem(_)) => true,
                    ("enum.item", ContentNode::EnumItem { .. }) => true,
                    ("terms.item", node) if node.term_item_parts().is_some() => true,
                    ("text", ContentNode::Text(_)) => true,
                    ("cite", ContentNode::Citation { .. }) => true,
                    ("ref", ContentNode::Reference { .. }) => true,
                    ("label", ContentNode::LabelDef(_) | ContentNode::Label(_)) => true,
                    ("bibliography", ContentNode::Bibliography { .. }) => true,
                    // Element variant matches by name
                    (elem_name, ContentNode::Element { name: n, .. })
                        if elem_name == n
                            && n != LABELED_ELEMENT_NAME
                            && n != REF_ANCHOR_ELEMENT_NAME
                            && n != DUP_ELEMENT_NAME
                            && n != DUP_SNAPSHOT_NAME =>
                    {
                        true
                    }
                    // FuncCall matches by function name
                    (func_name, ContentNode::FuncCall { name, .. }) if func_name == name => true,
                    _ => false,
                };

                if !type_matches {
                    return false;
                }

                // If no filters, just type matching is enough
                if filters.is_empty() {
                    return true;
                }

                // Check filters against element fields
                let fields = self.get_node_fields(node);
                filters.iter().all(|(field_name, expected_value)| {
                    fields
                        .get(field_name)
                        .map(|v| v == expected_value)
                        .unwrap_or(false)
                })
            }
            Selector::Label(label) => node.labeled_parts().is_some_and(|(_, _, l, _)| l == label),
            Selector::String(s) => {
                matches!(node, ContentNode::Text(t) if t.contains(s))
            }
            Selector::Regex(pattern) => {
                if let ContentNode::Text(text) = node {
                    pattern.is_match(text)
                } else {
                    false
                }
            }
            Selector::Or(a, b) => a.matches(node) || b.matches(node),
            Selector::And(a, b) => a.matches(node) && b.matches(node),
            Selector::Before(_, _) | Selector::After(_, _) => {
                // Context-dependent selectors not supported in simple matching
                false
            }
        }
    }

    /// Extract fields from a content node for filter matching.
    fn get_node_fields(&self, node: &ContentNode) -> IndexMap<String, Value> {
        if let Some((target, _, label, _)) = node.labeled_parts() {
            let mut fields = if target.len() == 1 {
                self.get_node_fields(&target[0])
            } else {
                IndexMap::new()
            };
            fields.insert("label".to_string(), Value::Label(label.to_string()));
            return fields;
        }
        match node {
            ContentNode::Element { fields, .. } => fields
                .iter()
                .filter(|(k, _)| !k.starts_with("__tylax_"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            ContentNode::FuncCall { name, args } => {
                let mut fields = IndexMap::new();
                for arg in args {
                    if let Arg::Named(k, v) = arg {
                        if !k.starts_with("__tylax_") {
                            fields.insert(k.clone(), v.clone());
                        }
                    }
                }
                if name == "figure" && fields.get("kind").is_none_or(|v| matches!(v, Value::Auto)) {
                    let has_table = args.iter().any(|arg| match arg {
                        Arg::Pos(Value::Content(nodes)) => {
                            nodes.iter().any(content_node_has_table_or_grid)
                        }
                        _ => false,
                    });
                    let inferred = if has_table { "table" } else { "image" };
                    fields.insert(
                        "kind".to_string(),
                        Value::Func(Arc::new(Closure::builtin(inferred))),
                    );
                }
                fields
            }
            ContentNode::Heading { level, content } => {
                let mut fields = IndexMap::new();
                fields.insert("level".to_string(), Value::Int(*level as i64));
                fields.insert("body".to_string(), Value::Content(content.clone()));
                fields
            }
            ContentNode::Raw { text, lang, block } => {
                let mut fields = IndexMap::new();
                fields.insert("text".to_string(), Value::Str(text.clone()));
                if let Some(l) = lang {
                    fields.insert("lang".to_string(), Value::Str(l.clone()));
                }
                fields.insert("block".to_string(), Value::Bool(*block));
                fields
            }
            ContentNode::Math { segments, block } => {
                let mut fields = IndexMap::new();
                fields.insert(
                    "body".to_string(),
                    Value::Str(render_math_segments_to_typst_source(segments)),
                );
                fields.insert("block".to_string(), Value::Bool(*block));
                fields
            }
            ContentNode::Strong(content)
            | ContentNode::Emph(content)
            | ContentNode::ListItem(content) => {
                let mut fields = IndexMap::new();
                fields.insert("body".to_string(), Value::Content(content.clone()));
                fields
            }
            ContentNode::EnumItem { number, content } => {
                let mut fields = IndexMap::new();
                if let Some(n) = number {
                    fields.insert("number".to_string(), Value::Int(*n));
                }
                fields.insert("body".to_string(), Value::Content(content.clone()));
                fields
            }
            ContentNode::Text(t) => {
                let mut fields = IndexMap::new();
                fields.insert("text".to_string(), Value::Str(t.clone()));
                fields
            }
            ContentNode::Citation {
                keys,
                mode,
                supplement,
            } => {
                let mut fields = IndexMap::new();
                fields.insert(
                    "keys".to_string(),
                    Value::Array(keys.iter().cloned().map(Value::Str).collect()),
                );
                fields.insert(
                    "mode".to_string(),
                    Value::Str(
                        match mode {
                            CitationMode::Normal => "normal",
                            CitationMode::AuthorInText => "prose",
                            CitationMode::SuppressAuthor => "year",
                            CitationMode::NoParen => "author",
                        }
                        .to_string(),
                    ),
                );
                if let Some(supplement) = supplement {
                    fields.insert("supplement".to_string(), Value::Str(supplement.clone()));
                }
                fields
            }
            ContentNode::Reference { target, ref_type } => {
                let mut fields = IndexMap::new();
                fields.insert("target".to_string(), Value::Str(target.clone()));
                fields.insert(
                    "kind".to_string(),
                    Value::Str(
                        match ref_type {
                            ReferenceType::Basic => "basic",
                            ReferenceType::Named => "named",
                            ReferenceType::Page => "page",
                            ReferenceType::Equation => "equation",
                        }
                        .to_string(),
                    ),
                );
                fields
            }
            ContentNode::LabelDef(label) => {
                let mut fields = IndexMap::new();
                fields.insert("label".to_string(), Value::Str(label.clone()));
                fields
            }
            ContentNode::Bibliography { file, style } => {
                let mut fields = IndexMap::new();
                fields.insert("file".to_string(), Value::Str(file.clone()));
                if let Some(style) = style {
                    fields.insert("style".to_string(), Value::Str(style.clone()));
                }
                fields
            }
            _ => IndexMap::new(),
        }
    }

    /// Create an element selector from a string.
    pub fn element(name: impl Into<String>) -> Self {
        Selector::Element(name.into(), Vec::new())
    }

    /// Create an element selector with filters.
    pub fn element_with_filters(name: impl Into<String>, filters: Vec<(String, Value)>) -> Self {
        Selector::Element(name.into(), filters)
    }

    /// Create a label selector from a string.
    pub fn label(label: impl Into<String>) -> Self {
        Selector::Label(label.into())
    }

    /// Add a filter to an element selector.
    pub fn with_filter(self, field: impl Into<String>, value: Value) -> Self {
        match self {
            Selector::Element(name, mut filters) => {
                filters.push((field.into(), value));
                Selector::Element(name, filters)
            }
            other => other, // Non-element selectors can't have filters
        }
    }

    pub(crate) fn is_textual(&self) -> bool {
        matches!(self, Selector::String(_) | Selector::Regex(_))
    }

    pub(crate) fn may_match_container(&self, name: &str) -> bool {
        match self {
            Selector::Element(n, _) => n == name,
            Selector::Or(a, b) | Selector::And(a, b) => {
                a.may_match_container(name) || b.may_match_container(name)
            }
            Selector::Before(a, _) | Selector::After(a, _) => a.may_match_container(name),
            Selector::String(_) | Selector::Regex(_) | Selector::Label(_) => false,
        }
    }

    pub(crate) fn contains_textual(&self) -> bool {
        match self {
            Selector::String(_) | Selector::Regex(_) => true,
            Selector::Or(a, b)
            | Selector::And(a, b)
            | Selector::Before(a, b)
            | Selector::After(a, b) => a.contains_textual() || b.contains_textual(),
            Selector::Element(_, _) | Selector::Label(_) => false,
        }
    }

    pub(crate) fn find_text_match(&self, text: &str) -> Option<(usize, usize)> {
        match self {
            Selector::String(pat) if !pat.is_empty() => {
                text.find(pat.as_str()).map(|pos| (pos, pos + pat.len()))
            }
            Selector::Regex(re) => re
                .find(text)
                .and_then(|m| (m.end() > m.start()).then_some((m.start(), m.end()))),
            _ => None,
        }
    }

    pub(crate) fn from_value(val: &Value) -> Option<Self> {
        match val {
            Value::Selector(s) => Some(s.clone()),
            Value::Str(s) => Some(Selector::String(s.clone())),
            Value::Regex(r) => Some(Selector::Regex(r.clone())),
            Value::Label(l) => Some(Selector::Label(l.clone())),
            Value::Content(nodes) => match nodes.as_slice() {
                [ContentNode::Label(l) | ContentNode::LabelDef(l)] => {
                    Some(Selector::Label(l.clone()))
                }
                _ => None,
            },
            Value::Func(f) => f.builtin_name().map(Selector::element),
            _ => None,
        }
    }
}

// ============================================================================
// Counter Type
// ============================================================================

/// A counter reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Counter {
    /// A custom named counter
    Custom(String),
    /// A counter tied to a label
    Label(String),
    /// A counter tied to a selector
    Selector(String),
    /// The page counter
    Page,
}

// ============================================================================
// State Type
// ============================================================================

/// A state reference for dynamic document state.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// The state key/identifier
    pub key: String,
    /// The default/initial value
    pub init: Box<Value>,
}

// ============================================================================
// Arguments Type
// ============================================================================

/// Collected function arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct Arguments {
    pub positional: Vec<Value>,
    pub named: IndexMap<String, Value>,
}

impl Arguments {
    pub fn new() -> Self {
        Self {
            positional: Vec::new(),
            named: IndexMap::new(),
        }
    }

    pub fn with_positional(mut self, args: Vec<Value>) -> Self {
        self.positional = args;
        self
    }

    pub fn with_named(mut self, named: IndexMap<String, Value>) -> Self {
        self.named = named;
        self
    }
}

impl Default for Arguments {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Wrapped Regex (for PartialEq)
// ============================================================================

/// A wrapped regex that implements PartialEq by comparing patterns.
#[derive(Debug, Clone)]
pub struct WrappedRegex(pub Regex);

impl PartialEq for WrappedRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl std::ops::Deref for WrappedRegex {
    type Target = Regex;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// ============================================================================
// The Main Value Enum
// ============================================================================

/// A computational value in the MiniEval interpreter.
///
/// This enum covers all value types from typst-hs for full compatibility.
#[derive(Clone, Default)]
pub enum Value {
    // === Primitives ===
    /// The absence of a meaningful value
    #[default]
    None,
    /// A value indicating smart default behavior
    Auto,
    /// A boolean: `true`, `false`
    Bool(bool),
    /// An integer: `120`, `-5`
    Int(i64),
    /// A floating-point number: `1.2`, `10e-4`
    Float(f64),
    /// A string: `"hello"`
    Str(String),

    // === Numeric Types ===
    /// A length value (e.g., `12pt`, `1em + 50%`)
    Length(Length),
    /// A ratio value (e.g., `50%`)
    Ratio(f64),
    /// An angle value in degrees (e.g., `45deg`)
    Angle(f64),
    /// A fraction for flex layouts (e.g., `1fr`)
    Fraction(f64),

    // === Visual Types ===
    /// A color value
    Color(Color),
    /// An alignment value
    Alignment(Alignment),
    /// A direction value
    Direction(Direction),
    /// A symbol
    Symbol(Symbol),

    // === Collections ===
    /// An array of values: `(1, "hi", 3)`
    Array(Vec<Value>),
    /// A dictionary: `(a: 1, b: "hi")` - uses IndexMap for ordered keys
    Dict(IndexMap<String, Value>),

    // === Functions ===
    /// A user-defined function (closure)
    Func(Arc<Closure>),

    // === Content ===
    /// Content (markup result)
    Content(Vec<ContentNode>),

    // === Complex Types ===
    /// A regex pattern
    Regex(WrappedRegex),
    /// A datetime value
    DateTime(DateTime),
    /// A label reference
    Label(String),
    /// A selector
    Selector(Selector),
    /// A counter reference
    Counter(Counter),
    /// A state reference
    State(State),
    /// Collected arguments
    Arguments(Arguments),
    /// A module (name, exports)
    Module(String, IndexMap<String, Value>),
    /// A version (e.g., `version(0, 12, 0)`)
    Version(Vec<u32>),
    /// Bytes data
    Bytes(Vec<u8>),
    /// A type value (for `type()` function)
    Type(ValType),
    /// Styles (placeholder for set rules)
    Styles,
}

/// Type identifiers for the `type()` function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    None,
    Auto,
    Bool,
    Int,
    Float,
    Str,
    Length,
    Ratio,
    Angle,
    Fraction,
    Color,
    Alignment,
    Direction,
    Symbol,
    Array,
    Dict,
    Function,
    Content,
    Regex,
    DateTime,
    Label,
    Selector,
    Counter,
    State,
    Arguments,
    Module,
    Version,
    Bytes,
    Type,
    Styles,
}

impl ValType {
    pub fn name(&self) -> &'static str {
        match self {
            ValType::None => "none",
            ValType::Auto => "auto",
            ValType::Bool => "bool",
            ValType::Int => "int",
            ValType::Float => "float",
            ValType::Str => "str",
            ValType::Length => "length",
            ValType::Ratio => "ratio",
            ValType::Angle => "angle",
            ValType::Fraction => "fraction",
            ValType::Color => "color",
            ValType::Alignment => "alignment",
            ValType::Direction => "direction",
            ValType::Symbol => "symbol",
            ValType::Array => "array",
            ValType::Dict => "dictionary",
            ValType::Function => "function",
            ValType::Content => "content",
            ValType::Regex => "regex",
            ValType::DateTime => "datetime",
            ValType::Label => "label",
            ValType::Selector => "selector",
            ValType::Counter => "counter",
            ValType::State => "state",
            ValType::Arguments => "arguments",
            ValType::Module => "module",
            ValType::Version => "version",
            ValType::Bytes => "bytes",
            ValType::Type => "type",
            ValType::Styles => "styles",
        }
    }
}

impl fmt::Display for ValType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::None => write!(f, "none"),
            Value::Auto => write!(f, "auto"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Int(i) => write!(f, "{}", i),
            Value::Float(v) => write!(f, "{}", v),
            Value::Str(s) => write!(f, "\"{}\"", s),
            Value::Length(l) => write!(f, "{}", l.to_typst()),
            Value::Ratio(r) => write!(f, "{}%", r * 100.0),
            Value::Angle(a) => write!(f, "{}deg", a),
            Value::Fraction(fr) => write!(f, "{}fr", fr),
            Value::Color(c) => write!(f, "{}", c.to_typst()),
            Value::Alignment(a) => write!(f, "{}", a.to_typst()),
            Value::Direction(d) => write!(f, "{:?}", d),
            Value::Symbol(s) => write!(f, "{}", s.default),
            Value::Array(arr) => {
                write!(f, "(")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", v)?;
                }
                write!(f, ")")
            }
            Value::Dict(dict) => {
                write!(f, "(")?;
                for (i, (k, v)) in dict.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {:?}", k, v)?;
                }
                write!(f, ")")
            }
            Value::Func(c) => {
                if let Some(name) = c.builtin_name() {
                    write!(f, "{name}")
                } else {
                    write!(f, "<function({})>", c.params.join(", "))
                }
            }
            Value::Content(nodes) => {
                write!(f, "[")?;
                for node in nodes {
                    write!(f, "{:?}", node)?;
                }
                write!(f, "]")
            }
            Value::Regex(r) => write!(f, "regex(\"{}\")", r.as_str()),
            Value::DateTime(dt) => write!(f, "{}", dt.to_typst()),
            Value::Label(l) => write!(f, "<{}>", l),
            Value::Selector(s) => write!(f, "{:?}", s),
            Value::Counter(c) => write!(f, "{:?}", c),
            Value::Arguments(a) => write!(f, "arguments({:?}, {:?})", a.positional, a.named),
            Value::Module(name, _) => write!(f, "<module {}>", name),
            Value::Version(v) => {
                write!(f, "version(")?;
                for (i, n) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", n)?;
                }
                write!(f, ")")
            }
            Value::Bytes(b) => write!(f, "bytes({})", b.len()),
            Value::Type(t) => write!(f, "{}", t.name()),
            Value::Styles => write!(f, "<styles>"),
            Value::State(s) => write!(f, "state({:?})", s.key),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::None, Value::None) => true,
            (Value::Auto, Value::Auto) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Length(a), Value::Length(b)) => a == b,
            (Value::Ratio(a), Value::Ratio(b)) => a == b,
            (Value::Angle(a), Value::Angle(b)) => a == b,
            (Value::Fraction(a), Value::Fraction(b)) => a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
            (Value::Alignment(a), Value::Alignment(b)) => a == b,
            (Value::Direction(a), Value::Direction(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Dict(a), Value::Dict(b)) => a == b,
            (Value::Func(a), Value::Func(b)) => {
                Arc::ptr_eq(a, b)
                    || matches!(
                        (a.builtin_name(), b.builtin_name()),
                        (Some(na), Some(nb)) if na == nb
                    )
            }
            (Value::Content(a), Value::Content(b)) => a == b,
            (Value::Regex(a), Value::Regex(b)) => a == b,
            (Value::DateTime(a), Value::DateTime(b)) => a == b,
            (Value::Label(a), Value::Label(b)) => a == b,
            (Value::Selector(a), Value::Selector(b)) => a == b,
            (Value::Counter(a), Value::Counter(b)) => a == b,
            (Value::Arguments(a), Value::Arguments(b)) => a == b,
            (Value::Module(n1, m1), Value::Module(n2, m2)) => n1 == n2 && m1 == m2,
            (Value::Version(a), Value::Version(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Type(a), Value::Type(b)) => a == b,
            (Value::Styles, Value::Styles) => true,
            _ => false,
        }
    }
}

impl Value {
    /// Get the type of this value.
    pub fn val_type(&self) -> ValType {
        match self {
            Value::None => ValType::None,
            Value::Auto => ValType::Auto,
            Value::Bool(_) => ValType::Bool,
            Value::Int(_) => ValType::Int,
            Value::Float(_) => ValType::Float,
            Value::Str(_) => ValType::Str,
            Value::Length(_) => ValType::Length,
            Value::Ratio(_) => ValType::Ratio,
            Value::Angle(_) => ValType::Angle,
            Value::Fraction(_) => ValType::Fraction,
            Value::Color(_) => ValType::Color,
            Value::Alignment(_) => ValType::Alignment,
            Value::Direction(_) => ValType::Direction,
            Value::Symbol(_) => ValType::Symbol,
            Value::Array(_) => ValType::Array,
            Value::Dict(_) => ValType::Dict,
            Value::Func(_) => ValType::Function,
            Value::Content(_) => ValType::Content,
            Value::Regex(_) => ValType::Regex,
            Value::DateTime(_) => ValType::DateTime,
            Value::Label(_) => ValType::Label,
            Value::Selector(_) => ValType::Selector,
            Value::Counter(_) => ValType::Counter,
            Value::State(_) => ValType::State,
            Value::Arguments(_) => ValType::Arguments,
            Value::Module(_, _) => ValType::Module,
            Value::Version(_) => ValType::Version,
            Value::Bytes(_) => ValType::Bytes,
            Value::Type(_) => ValType::Type,
            Value::Styles => ValType::Styles,
        }
    }

    /// Get the type name of this value.
    pub fn type_name(&self) -> &'static str {
        self.val_type().name()
    }

    /// Check if this value is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::None => false,
            Value::Auto => true,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Length(_) => true,
            Value::Ratio(r) => *r != 0.0,
            Value::Angle(a) => *a != 0.0,
            Value::Fraction(f) => *f != 0.0,
            Value::Color(_) => true,
            Value::Alignment(_) => true,
            Value::Direction(_) => true,
            Value::Symbol(_) => true,
            Value::Array(arr) => !arr.is_empty(),
            Value::Dict(dict) => !dict.is_empty(),
            Value::Func(_) => true,
            Value::Content(nodes) => !nodes.is_empty(),
            Value::Regex(_) => true,
            Value::DateTime(_) => true,
            Value::Label(_) => true,
            Value::Selector(_) => true,
            Value::Counter(_) => true,
            Value::State(_) => true,
            Value::Arguments(_) => true,
            Value::Module(_, _) => true,
            Value::Version(_) => true,
            Value::Bytes(b) => !b.is_empty(),
            Value::Type(_) => true,
            Value::Styles => true,
        }
    }

    /// Try to cast this value to a boolean.
    pub fn as_bool(&self) -> Result<bool, EvalError> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(EvalError::type_mismatch("bool", self.type_name())),
        }
    }

    /// Try to cast this value to an integer.
    pub fn as_int(&self) -> Result<i64, EvalError> {
        match self {
            Value::Int(i) => Ok(*i),
            Value::Float(f) => Ok(*f as i64),
            _ => Err(EvalError::type_mismatch("int", self.type_name())),
        }
    }

    /// Try to cast this value to a float.
    pub fn as_float(&self) -> Result<f64, EvalError> {
        match self {
            Value::Int(i) => Ok(*i as f64),
            Value::Float(f) => Ok(*f),
            Value::Ratio(r) => Ok(*r),
            Value::Angle(a) => Ok(*a),
            Value::Fraction(f) => Ok(*f),
            _ => Err(EvalError::type_mismatch("float", self.type_name())),
        }
    }

    /// Try to cast this value to a string.
    pub fn as_str(&self) -> Result<&str, EvalError> {
        match self {
            Value::Str(s) => Ok(s),
            _ => Err(EvalError::type_mismatch("str", self.type_name())),
        }
    }

    /// Try to cast this value to an array.
    pub fn as_array(&self) -> Result<&Vec<Value>, EvalError> {
        match self {
            Value::Array(arr) => Ok(arr),
            _ => Err(EvalError::type_mismatch("array", self.type_name())),
        }
    }

    /// Try to cast this value to a mutable array.
    pub fn as_array_mut(&mut self) -> Result<&mut Vec<Value>, EvalError> {
        match self {
            Value::Array(arr) => Ok(arr),
            _ => Err(EvalError::type_mismatch("array", self.type_name())),
        }
    }

    /// Try to cast this value to a dictionary.
    pub fn as_dict(&self) -> Result<&IndexMap<String, Value>, EvalError> {
        match self {
            Value::Dict(dict) => Ok(dict),
            _ => Err(EvalError::type_mismatch("dictionary", self.type_name())),
        }
    }

    /// Try to cast this value to a function.
    pub fn as_func(&self) -> Result<&Arc<Closure>, EvalError> {
        match self {
            Value::Func(f) => Ok(f),
            _ => Err(EvalError::type_mismatch("function", self.type_name())),
        }
    }

    /// Convert this value to a display string.
    pub fn display(&self) -> String {
        match self {
            Value::None => String::new(),
            Value::Auto => "auto".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => format_float(*f),
            Value::Str(s) => s.clone(),
            Value::Length(l) => l.to_typst(),
            Value::Ratio(r) => format!("{}%", r * 100.0),
            Value::Angle(a) => format!("{}deg", a),
            Value::Fraction(f) => format!("{}fr", f),
            Value::Color(c) => c.to_typst(),
            Value::Alignment(a) => a.to_typst(),
            Value::Direction(d) => format!("{:?}", d).to_lowercase(),
            Value::Symbol(s) => s.default.clone(),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.display()).collect();
                format!("({})", items.join(", "))
            }
            Value::Dict(dict) => {
                let items: Vec<String> = dict
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.display()))
                    .collect();
                format!("({})", items.join(", "))
            }
            Value::Func(c) => c
                .builtin_name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("<function({})>", c.params.join(", "))),
            Value::Content(nodes) => nodes.iter().map(|n| n.to_typst()).collect(),
            Value::Regex(r) => format!("regex(\"{}\")", r.as_str()),
            Value::DateTime(dt) => dt.to_typst(),
            Value::Label(l) => format!("<{}>", l),
            Value::Selector(_) => "<selector>".to_string(),
            Value::Counter(_) => "<counter>".to_string(),
            Value::State(s) => format!("state({:?})", s.key),
            Value::Arguments(_) => "<arguments>".to_string(),
            Value::Module(name, _) => format!("<module {}>", name),
            Value::Version(v) => v
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("."),
            Value::Bytes(b) => format!("bytes({})", b.len()),
            Value::Type(t) => t.name().to_string(),
            Value::Styles => "<styles>".to_string(),
        }
    }

    /// Display value for embedding inside math mode.
    ///
    /// This is similar to `display()` but handles `ContentNode::Math` specially:
    /// instead of outputting `$content$`, it outputs just `content`.
    /// This prevents nested `$` symbols when expanding `#x` inside math like `$... #x ...$`.
    pub fn display_in_math(&self) -> String {
        value_to_typst_math_expr(self)
    }

    /// Convert to content nodes.
    pub fn into_content(self) -> Vec<ContentNode> {
        match self {
            Value::Content(nodes) => nodes,
            Value::None => vec![],
            Value::Label(l) => vec![ContentNode::Label(l)],
            other => vec![ContentNode::Text(other.display())],
        }
    }
}

/// Format a float without unnecessary trailing zeros.
fn format_float(f: f64) -> String {
    if f.fract() == 0.0 {
        format!("{:.1}", f)
    } else {
        format!("{}", f)
    }
}

// ============================================================================
// Closure Type
// ============================================================================

/// A user-defined function (closure).
#[derive(Debug, Clone)]
pub struct Closure {
    /// Function name (if named)
    pub name: Option<String>,
    /// Parameter names
    pub params: Vec<String>,
    /// Default values as source strings (evaluated lazily at call time).
    /// This allows defaults to depend on prior parameters: `#let f(x, y: x + 1) = ...`
    pub defaults: Vec<Option<String>>,
    /// Sink parameter name (for `..args`)
    pub sink: Option<String>,
    /// The function body as raw source text (will be re-parsed when called)
    pub body_source: String,
    /// Captured variables from the enclosing scope
    pub captures: IndexMap<String, Value>,
}

impl Closure {
    /// Construct a wrapper closure representing a builtin Typst function.
    pub(crate) fn builtin(name: &str) -> Self {
        Self {
            name: Some(format!("<builtin:{name}>")),
            params: vec!["x".to_string()],
            defaults: vec![None],
            sink: None,
            body_source: format!("{name}(x)"),
            captures: IndexMap::new(),
        }
    }

    /// Return the builtin name if this closure wraps a builtin function.
    pub(crate) fn builtin_name(&self) -> Option<&str> {
        self.name
            .as_deref()
            .and_then(|n| n.strip_prefix("<builtin:"))
            .and_then(|n| n.strip_suffix('>'))
    }
}

// ============================================================================
// ContentNode Type
// ============================================================================

/// A content node representing evaluated markup.
///
/// This is a simplified representation of Typst content that can be
/// serialized back to Typst source code.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentNode {
    /// Plain text
    Text(String),
    /// A space
    Space,
    /// A line break
    Linebreak,
    /// A paragraph break
    Parbreak,
    /// Strong (bold) text
    Strong(Vec<ContentNode>),
    /// Emphasized (italic) text
    Emph(Vec<ContentNode>),
    /// Raw/code block
    Raw {
        text: String,
        lang: Option<String>,
        block: bool,
    },
    /// Math equation
    Math {
        segments: Vec<MathSegment>,
        block: bool,
    },
    /// A heading
    Heading {
        level: u8,
        content: Vec<ContentNode>,
    },
    /// A list item
    ListItem(Vec<ContentNode>),
    /// An enum item
    EnumItem {
        number: Option<i64>,
        content: Vec<ContentNode>,
    },
    /// A unified element representation for introspection.
    ///
    /// All built-in elements (heading, strong, emph, list items, etc.) can be
    /// represented using this variant when introspection methods are called.
    /// The `name` field corresponds to the Typst element function name.
    /// The `fields` map contains all element properties.
    ///
    /// This enables `content.func()`, `content.fields()`, `content.has(field)`,
    /// `content.at(field)` methods for Show Rules and advanced transformations.
    ///
    /// # Standard Field Names
    /// - `body`: The main content (for containers like heading, strong, emph)
    /// - `level`: The heading level (for heading elements)
    /// - `lang`: Language specification (for raw/code blocks)
    /// - `block`: Whether displayed as block (for raw, math)
    /// - `children`: Child nodes (for containers)
    Element {
        name: String,
        fields: IndexMap<String, Value>,
    },
    /// A label
    Label(String),
    /// A preserved citation node.
    Citation {
        keys: Vec<String>,
        mode: CitationMode,
        supplement: Option<String>,
    },
    /// A preserved reference node.
    Reference {
        target: String,
        ref_type: ReferenceType,
    },
    /// A preserved label definition node.
    LabelDef(String),
    /// A preserved bibliography node.
    Bibliography { file: String, style: Option<String> },
    /// A function call that couldn't be fully evaluated
    FuncCall { name: String, args: Vec<Arg> },
    /// Raw Typst source that should be passed through unchanged
    RawSource(String),
    /// A state value reference (for state tracking)
    State { key: String, default: Box<Value> },
    /// A counter display
    CounterDisplay { key: String, numbering: String },
}

/// An argument to a function call.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// Positional argument
    Pos(Value),
    /// Named argument
    Named(String, Value),
    /// Spread argument
    Spread(Value),
}

/// A structured fragment of math content.
#[derive(Debug, Clone, PartialEq)]
pub enum MathSegment {
    /// Raw math source that was not produced by evaluating a `#expr`.
    Source(String),
    /// A value produced by evaluating a `#expr` inside math.
    Evaluated(Value),
}

// ============================================================================
// Show Rules
// ============================================================================

static NEXT_SHOW_RULE_ID: AtomicUsize = AtomicUsize::new(1);

/// Scoped style overrides from `show <selector>: set <target>(...)`.
#[derive(Debug, Clone)]
pub(crate) struct ShowSetStyle {
    pub target: String,
    pub args: IndexMap<String, Value>,
}

/// A show rule that transforms matched content.
#[derive(Debug, Clone)]
pub struct ShowRule {
    /// The selector that determines what content matches
    pub selector: Selector,
    /// The transformation function to apply
    pub transform: Arc<Closure>,
    /// Static replacement value when rule uses a non-function replacement (`#show X: [Y]`)
    pub(crate) replacement: Option<Value>,
    /// Scoped set-rule payload (`#show X: set Y(...)`)
    pub(crate) set_style: Option<ShowSetStyle>,
    /// Unique rule identity (for cycle/revocation tracking across rule applications)
    pub(crate) id: usize,
    /// Priority (rules defined later have higher priority)
    pub priority: usize,
}

impl ShowRule {
    /// Create a new show rule.
    pub fn new(selector: Selector, transform: Arc<Closure>, priority: usize) -> Self {
        Self {
            selector,
            transform,
            replacement: None,
            set_style: None,
            id: NEXT_SHOW_RULE_ID.fetch_add(1, Ordering::Relaxed),
            priority,
        }
    }

    /// Create a show rule that replaces matched content with a static value.
    pub(crate) fn from_value(selector: Selector, replacement: Value, priority: usize) -> Self {
        Self {
            selector,
            transform: Arc::new(Closure {
                name: None,
                params: Vec::new(),
                defaults: Vec::new(),
                sink: None,
                body_source: String::new(),
                captures: IndexMap::new(),
            }),
            replacement: Some(replacement),
            set_style: None,
            id: NEXT_SHOW_RULE_ID.fetch_add(1, Ordering::Relaxed),
            priority,
        }
    }

    /// Create a show rule that applies scoped `set` styling to matched targets.
    pub(crate) fn from_set(selector: Selector, style: ShowSetStyle, priority: usize) -> Self {
        Self {
            selector,
            transform: Arc::new(Closure {
                name: None,
                params: Vec::new(),
                defaults: Vec::new(),
                sink: None,
                body_source: String::new(),
                captures: IndexMap::new(),
            }),
            replacement: None,
            set_style: Some(style),
            id: NEXT_SHOW_RULE_ID.fetch_add(1, Ordering::Relaxed),
            priority,
        }
    }
}

pub(crate) fn normalize_ref_target_text(text: &str) -> String {
    text.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

pub(crate) fn normalize_supplement_text(text: &str) -> Option<String> {
    let normalized = text
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub(crate) fn citation_content_value(
    keys: Vec<String>,
    mode: CitationMode,
    supplement: Option<String>,
) -> EvalResult<Value> {
    let keys: Vec<String> = keys
        .into_iter()
        .map(|key| normalize_ref_target_text(&key))
        .filter(|key| !key.is_empty())
        .collect();

    if keys.is_empty() {
        return Err(EvalError::argument(
            "cite expects at least one label argument".to_string(),
        ));
    }

    Ok(Value::Content(vec![ContentNode::Citation {
        keys,
        mode,
        supplement: supplement.and_then(|value| normalize_supplement_text(&value)),
    }]))
}

pub(crate) fn reference_content_value(target: String, ref_type: ReferenceType) -> Value {
    Value::Content(vec![ContentNode::Reference {
        target: normalize_ref_target_text(&target),
        ref_type,
    }])
}

pub(crate) fn label_content_value(label: String) -> Value {
    Value::Content(vec![ContentNode::LabelDef(normalize_ref_target_text(
        &label,
    ))])
}

pub(crate) fn bibliography_content_value(file: String, style: Option<String>) -> EvalResult<Value> {
    let file = file.trim().to_string();
    if file.is_empty() {
        return Err(EvalError::argument(
            "bibliography expects a file argument".to_string(),
        ));
    }

    let style = style.and_then(|value| {
        let trimmed = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    Ok(Value::Content(vec![ContentNode::Bibliography {
        file,
        style,
    }]))
}

pub(crate) fn render_math_segments_to_typst_source(segments: &[MathSegment]) -> String {
    segments
        .iter()
        .map(|segment| match segment {
            MathSegment::Source(text) => text.clone(),
            MathSegment::Evaluated(value) => value_to_typst_math_expr(value),
        })
        .collect()
}

fn render_func_call_source(
    name: &str,
    args: &[Arg],
    prefix_hash: bool,
    in_math: bool,
    emit_anchors: bool,
) -> String {
    // The arguments of a `#` call are code even inside math: `#box(a/b)` divides,
    // so math content must stay `[$a/b$]`.
    let in_math = in_math && !prefix_hash;
    let args_str: Vec<String> = args
        .iter()
        .filter(|arg| !matches!(arg, Arg::Named(k, _) if k.starts_with("__tylax_")))
        .map(|arg| match arg {
            Arg::Pos(value) => {
                if in_math {
                    value_to_typst_math_arg(value)
                } else {
                    value_to_typst_arg_inner(value, emit_anchors)
                }
            }
            Arg::Named(key, value) => {
                let rendered = if in_math {
                    value_to_typst_math_arg(value)
                } else {
                    value_to_typst_arg_inner(value, emit_anchors)
                };
                format!("{}: {}", key, rendered)
            }
            Arg::Spread(value) => {
                let rendered = if in_math {
                    value_to_typst_math_arg(value)
                } else {
                    value_to_typst_arg_inner(value, emit_anchors)
                };
                format!("..{}", rendered)
            }
        })
        .collect();
    let hash = if prefix_hash { "#" } else { "" };
    format!("{}{}({})", hash, name, args_str.join(", "))
}

fn func_call_to_typst_source_inner(name: &str, args: &[Arg], emit_anchors: bool) -> String {
    render_func_call_source(name, args, true, false, emit_anchors)
}

fn func_call_to_typst_code_source_inner(name: &str, args: &[Arg], emit_anchors: bool) -> String {
    render_func_call_source(name, args, false, false, emit_anchors)
}

pub(crate) fn func_call_to_typst_math_source(name: &str, args: &[Arg]) -> String {
    render_func_call_source(name, args, true, true, false)
}

fn func_call_to_typst_math_code_source(name: &str, args: &[Arg]) -> String {
    render_func_call_source(name, args, false, true, false)
}

pub(crate) fn value_to_typst_math_expr(value: &Value) -> String {
    match value {
        Value::Content(nodes) => nodes.iter().map(|node| node.to_typst_in_math()).collect(),
        _ => value_to_typst_arg(value),
    }
}

fn value_to_typst_math_arg(value: &Value) -> String {
    match value {
        Value::Content(nodes) => {
            if nodes.len() == 1 {
                match &nodes[0] {
                    ContentNode::FuncCall { name, args } => {
                        return func_call_to_typst_math_code_source(name, args);
                    }
                    ContentNode::Math { segments, .. } => {
                        return render_math_segments_to_typst_source(segments);
                    }
                    _ => {}
                }
            }

            nodes.iter().map(|node| node.to_typst_in_math()).collect()
        }
        _ => value_to_typst_arg(value),
    }
}

fn escape_typst_markup_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut escaped = String::with_capacity(text.len());
    let mut at_line_start = true;
    let mut line_digits_only = false;

    for (idx, &ch) in chars.iter().enumerate() {
        let prev = idx.checked_sub(1).and_then(|p| chars.get(p).copied());
        let prev2 = idx.checked_sub(2).and_then(|p| chars.get(p).copied());
        let next = chars.get(idx + 1).copied();
        let next2 = chars.get(idx + 2).copied();

        let is_ellipsis_dot = ch == '.'
            && ((prev == Some('.') && next == Some('.'))
                || (prev2 == Some('.') && prev == Some('.'))
                || (next == Some('.') && next2 == Some('.')));

        let should_escape = matches!(
            ch,
            '\\' | '$'
                | '#'
                | '['
                | ']'
                | '*'
                | '_'
                | '`'
                | '<'
                | '>'
                | '@'
                | '/'
                | '~'
                | '"'
                | '\''
        ) || (at_line_start && matches!(ch, '=' | '+' | '-'))
            || (ch == '-' && (prev == Some('-') || next == Some('-') || next == Some('?')))
            || (ch == '.'
                && ((line_digits_only && next.is_none_or(|c| c.is_whitespace()))
                    || is_ellipsis_dot));

        if should_escape {
            escaped.push('\\');
        }
        escaped.push(ch);

        if matches!(ch, '\n' | '\r') {
            at_line_start = true;
            line_digits_only = false;
        } else if matches!(ch, ' ' | '\t') && at_line_start {
            // Leading indentation keeps line-start context.
        } else if ch.is_ascii_digit() && (at_line_start || line_digits_only) {
            at_line_start = false;
            line_digits_only = true;
        } else {
            at_line_start = false;
            line_digits_only = false;
        }
    }

    escaped
}

pub(crate) const LIST_CONTAINER_NAME: &str = "list";
pub(crate) const ENUM_CONTAINER_NAME: &str = "enum";
pub(crate) const TERMS_CONTAINER_NAME: &str = "terms";
pub(crate) const TERM_ITEM_NAME: &str = "terms.item";
pub(crate) const LABELED_ELEMENT_NAME: &str = "__tylax_labeled";
pub(crate) const REF_ANCHOR_ELEMENT_NAME: &str = "__tylax_ref_anchor";
pub(crate) const DUP_ELEMENT_NAME: &str = "__tylax_dup_element";
pub(crate) const DUP_SNAPSHOT_NAME: &str = "__tylax_dup_snapshot";
pub(crate) const EXPLICIT_FIELDS_KEY: &str = "__tylax_explicit_fields";
pub(crate) const EXPLICIT_NUMBERING_ARG: &str = "__tylax_explicit_numbering";

fn content_node_has_table_or_grid(node: &ContentNode) -> bool {
    if let Some((target, _, _, _)) = node.labeled_parts() {
        return target.iter().any(content_node_has_table_or_grid);
    }
    match node {
        ContentNode::FuncCall { name, args } => {
            matches!(name.as_str(), "table" | "grid")
                || args.iter().any(|arg| match arg {
                    Arg::Pos(Value::Content(nodes))
                    | Arg::Named(_, Value::Content(nodes))
                    | Arg::Spread(Value::Content(nodes)) => {
                        nodes.iter().any(content_node_has_table_or_grid)
                    }
                    _ => false,
                })
        }
        ContentNode::Element { name, fields } => {
            matches!(name.as_str(), "table" | "grid")
                || fields
                    .iter()
                    .filter(|(k, _)| !k.starts_with("__tylax_"))
                    .any(|(_, val)| match val {
                        Value::Content(nodes) => nodes.iter().any(content_node_has_table_or_grid),
                        Value::Array(items) => items.iter().any(|item| match item {
                            Value::Content(nodes) => {
                                nodes.iter().any(content_node_has_table_or_grid)
                            }
                            _ => false,
                        }),
                        _ => false,
                    })
        }
        ContentNode::Strong(nodes)
        | ContentNode::Emph(nodes)
        | ContentNode::ListItem(nodes)
        | ContentNode::Heading { content: nodes, .. }
        | ContentNode::EnumItem { content: nodes, .. } => {
            nodes.iter().any(content_node_has_table_or_grid)
        }
        _ => false,
    }
}

pub(crate) fn mark_explicit_fields(fields: &mut IndexMap<String, Value>, names: &[&str]) {
    if names.is_empty() {
        return;
    }
    fields.insert(
        EXPLICIT_FIELDS_KEY.to_string(),
        Value::Array(names.iter().map(|n| Value::Str((*n).to_string())).collect()),
    );
}

pub(crate) fn has_explicit_field(fields: &IndexMap<String, Value>, field: &str) -> bool {
    matches!(
        fields.get(EXPLICIT_FIELDS_KEY),
        Some(Value::Array(items)) if items.iter().any(|v| matches!(v, Value::Str(s) if s == field))
    )
}

fn render_term_field_nodes_inner(nodes: &[ContentNode], emit_anchors: bool) -> String {
    let mut rendered = render_content_nodes_inner(nodes, emit_anchors);
    if nodes
        .last()
        .is_some_and(|n| n.is_plain_block_list() || n.is_heading())
        && rendered.ends_with('\n')
    {
        rendered.pop();
    }
    rendered
}

fn render_content_block_arg_inner(nodes: &[ContentNode], emit_anchors: bool) -> String {
    let rendered = render_term_field_nodes_inner(nodes, emit_anchors);
    let starts_with_list = nodes
        .iter()
        .find(|node| {
            !matches!(node, ContentNode::Space | ContentNode::Parbreak)
                && !matches!(node, ContentNode::Text(text) if text.chars().all(|c| matches!(c, ' ' | '\t')))
        })
        .is_some_and(ContentNode::is_plain_block_list);
    if starts_with_list {
        format!("[\n{rendered}]")
    } else {
        format!("[{rendered}]")
    }
}

fn format_list_item_typst_inner(
    prefix: &str,
    children: &[ContentNode],
    emit_anchors: bool,
) -> String {
    let inner = render_content_nodes_inner(children, emit_anchors);
    let inner = inner.strip_suffix('\n').unwrap_or(&inner);
    let indent = " ".repeat(prefix.len());
    let mut out = String::new();
    for (idx, line) in inner.split('\n').enumerate() {
        if idx == 0 {
            out.push_str(prefix);
            out.push_str(line);
        } else if !line.is_empty() {
            out.push_str(&indent);
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

pub(crate) fn render_content_nodes(nodes: &[ContentNode]) -> String {
    render_content_nodes_inner(nodes, false)
}

fn render_content_nodes_inner(nodes: &[ContentNode], emit_anchors: bool) -> String {
    let mut output = String::new();
    let mut text_run = String::new();
    let flush_text_run = |output: &mut String, text_run: &mut String| {
        if !text_run.is_empty() {
            output.push_str(&escape_typst_markup_text(text_run));
            text_run.clear();
        }
    };

    for node in nodes {
        if let ContentNode::Text(s) = node {
            text_run.push_str(s);
            continue;
        }
        flush_text_run(&mut output, &mut text_run);
        if node.is_block_list_like() || node.is_heading() {
            output.truncate(output.trim_end_matches([' ', '\t']).len());
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
        }
        output.push_str(&node.to_typst_inner(emit_anchors));
    }
    flush_text_run(&mut output, &mut text_run);
    output
}

fn unpack_container_children(fields: &IndexMap<String, Value>) -> Option<Vec<ContentNode>> {
    let Value::Array(children) = fields.get("children")? else {
        return None;
    };
    let mut items = Vec::with_capacity(children.len());
    for child in children {
        match child {
            Value::Content(nodes) => items.extend(nodes.iter().cloned()),
            other => items.push(ContentNode::Text(other.display())),
        }
    }
    Some(items)
}

impl ContentNode {
    pub(crate) fn is_heading(&self) -> bool {
        matches!(self, Self::Heading { .. })
            || matches!(self, Self::Element { name, .. } if name == "heading")
    }

    pub(crate) fn is_ordinary_list_item(&self) -> bool {
        matches!(self, Self::ListItem(_) | Self::EnumItem { .. })
    }

    pub(crate) fn is_plain_block_list(&self) -> bool {
        if self.is_ordinary_list_item() {
            return true;
        }
        match self {
            Self::Element { name, fields } if name == LIST_CONTAINER_NAME => fields
                .get("marker")
                .is_none_or(|v| matches!(v, Value::Auto)),
            Self::Element { name, fields } if name == ENUM_CONTAINER_NAME => {
                fields.get("start").is_none_or(|v| matches!(v, Value::Auto))
                    && fields
                        .get("numbering")
                        .is_none_or(|v| matches!(v, Value::Auto))
            }
            _ => false,
        }
    }

    pub(crate) fn is_block_list_like(&self) -> bool {
        self.is_ordinary_list_item()
            || self.term_item_parts().is_some()
            || matches!(
                self,
                Self::Element { name, .. }
                    if matches!(
                        name.as_str(),
                        LIST_CONTAINER_NAME | ENUM_CONTAINER_NAME | TERMS_CONTAINER_NAME
                    )
            )
    }

    pub(crate) fn list_container(
        items: Vec<ContentNode>,
        tight: bool,
        marker: Option<Value>,
    ) -> Self {
        let mut fields = IndexMap::new();
        fields.insert(
            "children".to_string(),
            Value::Array(items.into_iter().map(|n| Value::Content(vec![n])).collect()),
        );
        fields.insert("tight".to_string(), Value::Bool(tight));
        if let Some(m) = marker.filter(|v| !matches!(v, Value::Auto)) {
            fields.insert("marker".to_string(), m);
        }
        Self::Element {
            name: LIST_CONTAINER_NAME.to_string(),
            fields,
        }
    }

    pub(crate) fn enum_container(
        items: Vec<ContentNode>,
        tight: bool,
        start: Option<i64>,
        numbering: Option<Value>,
    ) -> Self {
        let mut fields = IndexMap::new();
        fields.insert(
            "children".to_string(),
            Value::Array(items.into_iter().map(|n| Value::Content(vec![n])).collect()),
        );
        fields.insert("tight".to_string(), Value::Bool(tight));
        fields.insert(
            "start".to_string(),
            start.map(Value::Int).unwrap_or(Value::Auto),
        );
        if let Some(num) = numbering.filter(|v| !matches!(v, Value::Auto)) {
            fields.insert("numbering".to_string(), num);
        }
        Self::Element {
            name: ENUM_CONTAINER_NAME.to_string(),
            fields,
        }
    }

    pub(crate) fn terms_container(items: Vec<ContentNode>, tight: bool) -> Self {
        let mut fields = IndexMap::new();
        fields.insert(
            "children".to_string(),
            Value::Array(items.into_iter().map(|n| Value::Content(vec![n])).collect()),
        );
        fields.insert("tight".to_string(), Value::Bool(tight));
        Self::Element {
            name: TERMS_CONTAINER_NAME.to_string(),
            fields,
        }
    }

    pub(crate) fn list_container_parts(&self) -> Option<(Vec<ContentNode>, Option<&Value>)> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != LIST_CONTAINER_NAME {
            return None;
        }
        let items = unpack_container_children(fields)?;
        let marker = fields.get("marker").filter(|v| !matches!(v, Value::Auto));
        Some((items, marker))
    }

    pub(crate) fn enum_container_parts(
        &self,
    ) -> Option<(Vec<ContentNode>, Option<i64>, Option<&Value>)> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != ENUM_CONTAINER_NAME {
            return None;
        }
        let items = unpack_container_children(fields)?;
        let start = match fields.get("start") {
            Some(Value::Int(n)) => Some(*n),
            _ => None,
        };
        let numbering = fields
            .get("numbering")
            .filter(|v| !matches!(v, Value::Auto));
        Some((items, start, numbering))
    }

    pub(crate) fn terms_container_items(&self) -> Option<Vec<ContentNode>> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != TERMS_CONTAINER_NAME {
            return None;
        }
        unpack_container_children(fields)
    }

    pub(crate) fn term_item(term: Vec<ContentNode>, description: Vec<ContentNode>) -> Self {
        Self::Element {
            name: TERM_ITEM_NAME.to_string(),
            fields: IndexMap::from([
                ("term".to_string(), Value::Content(term)),
                ("description".to_string(), Value::Content(description)),
            ]),
        }
    }

    pub(crate) fn term_item_parts(&self) -> Option<(&[ContentNode], &[ContentNode])> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != TERM_ITEM_NAME {
            return None;
        }
        let Value::Content(term) = fields.get("term")? else {
            return None;
        };
        let Value::Content(description) = fields.get("description")? else {
            return None;
        };
        Some((term, description))
    }

    pub(crate) fn into_term_item_parts(self) -> Result<(Vec<ContentNode>, Vec<ContentNode>), Self> {
        if self.term_item_parts().is_none() {
            return Err(self);
        }
        let Self::Element { mut fields, .. } = self else {
            unreachable!()
        };
        let Some(Value::Content(term)) = fields.shift_remove("term") else {
            unreachable!()
        };
        let Some(Value::Content(description)) = fields.shift_remove("description") else {
            unreachable!()
        };
        Ok((term, description))
    }

    pub(crate) fn ref_anchor(counter: &str, kind: &str, label: Option<String>, step: bool) -> Self {
        let mut fields = IndexMap::new();
        fields.insert("counter".to_string(), Value::Str(counter.to_string()));
        fields.insert("kind".to_string(), Value::Str(kind.to_string()));
        fields.insert("step".to_string(), Value::Bool(step));
        if let Some(lbl) = label {
            fields.insert("label".to_string(), Value::Label(lbl));
        }
        Self::Element {
            name: REF_ANCHOR_ELEMENT_NAME.to_string(),
            fields,
        }
    }

    pub(crate) fn ref_anchor_parts(&self) -> Option<(&str, &str, Option<&str>, bool)> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != REF_ANCHOR_ELEMENT_NAME {
            return None;
        }
        let Value::Str(counter) = fields.get("counter")? else {
            return None;
        };
        let Value::Str(kind) = fields.get("kind")? else {
            return None;
        };
        let Value::Bool(step) = fields.get("step")? else {
            return None;
        };
        let label = match fields.get("label") {
            Some(Value::Label(lbl)) => Some(lbl.as_str()),
            _ => None,
        };
        Some((counter.as_str(), kind.as_str(), label, *step))
    }

    pub(crate) fn dup_snapshot(counter: &str, origin_id: usize) -> Self {
        let mut fields = IndexMap::new();
        fields.insert("counter".to_string(), Value::Str(counter.to_string()));
        fields.insert("id".to_string(), Value::Int(origin_id as i64));
        Self::Element {
            name: DUP_SNAPSHOT_NAME.to_string(),
            fields,
        }
    }

    pub(crate) fn dup_snapshot_parts(&self) -> Option<(&str, usize)> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != DUP_SNAPSHOT_NAME {
            return None;
        }
        let Value::Str(counter) = fields.get("counter")? else {
            return None;
        };
        let Value::Int(id) = fields.get("id")? else {
            return None;
        };
        Some((counter.as_str(), usize::try_from(*id).ok()?))
    }

    pub(crate) fn dup_element(
        counter: &str,
        id: usize,
        origin_id: usize,
        body: Vec<ContentNode>,
    ) -> Self {
        let mut fields = IndexMap::new();
        fields.insert("counter".to_string(), Value::Str(counter.to_string()));
        fields.insert("id".to_string(), Value::Int(id as i64));
        fields.insert("origin".to_string(), Value::Int(origin_id as i64));
        fields.insert("body".to_string(), Value::Content(body));
        Self::Element {
            name: DUP_ELEMENT_NAME.to_string(),
            fields,
        }
    }

    pub(crate) fn dup_element_parts(&self) -> Option<(&str, usize, usize, &[ContentNode])> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != DUP_ELEMENT_NAME {
            return None;
        }
        let Value::Str(counter) = fields.get("counter")? else {
            return None;
        };
        let Value::Int(id) = fields.get("id")? else {
            return None;
        };
        let origin_id = match fields.get("origin") {
            Some(Value::Int(oid)) => usize::try_from(*oid).unwrap_or(0),
            _ => 0,
        };
        let Value::Content(body) = fields.get("body")? else {
            return None;
        };
        Some((
            counter.as_str(),
            usize::try_from(*id).ok()?,
            origin_id,
            body.as_slice(),
        ))
    }

    pub(crate) fn labeled(
        target: Vec<ContentNode>,
        trivia: Vec<ContentNode>,
        label: String,
        is_func: bool,
    ) -> Self {
        Self::Element {
            name: LABELED_ELEMENT_NAME.to_string(),
            fields: IndexMap::from([
                ("__target".to_string(), Value::Content(target)),
                ("__trivia".to_string(), Value::Content(trivia)),
                ("label".to_string(), Value::Label(label)),
                ("__label_is_func".to_string(), Value::Bool(is_func)),
            ]),
        }
    }

    pub(crate) fn labeled_parts(&self) -> Option<(&[ContentNode], &[ContentNode], &str, bool)> {
        let Self::Element { name, fields } = self else {
            return None;
        };
        if name != LABELED_ELEMENT_NAME {
            return None;
        }
        let Value::Content(target) = fields.get("__target")? else {
            return None;
        };
        let Value::Content(trivia) = fields.get("__trivia")? else {
            return None;
        };
        let Value::Label(label) = fields.get("label")? else {
            return None;
        };
        let Value::Bool(is_func) = fields.get("__label_is_func")? else {
            return None;
        };
        Some((target, trivia, label, *is_func))
    }

    pub(crate) fn into_labeled_parts(
        self,
    ) -> Result<(Vec<ContentNode>, Vec<ContentNode>, String, bool), Self> {
        if self.labeled_parts().is_none() {
            return Err(self);
        }
        let Self::Element { mut fields, .. } = self else {
            unreachable!()
        };
        let Some(Value::Content(target)) = fields.shift_remove("__target") else {
            unreachable!()
        };
        let Some(Value::Content(trivia)) = fields.shift_remove("__trivia") else {
            unreachable!()
        };
        let Some(Value::Label(label)) = fields.shift_remove("label") else {
            unreachable!()
        };
        let Some(Value::Bool(is_func)) = fields.shift_remove("__label_is_func") else {
            unreachable!()
        };
        Ok((target, trivia, label, is_func))
    }

    /// Convert this content node back to Typst source code.
    pub fn to_typst(&self) -> String {
        self.to_typst_inner(false)
    }

    /// Convert this content node to Typst source code while preserving internal reference anchors
    /// for the downstream LaTeX emitter.
    pub(crate) fn to_typst_for_latex(&self) -> String {
        self.to_typst_inner(true)
    }

    fn to_typst_inner(&self, emit_anchors: bool) -> String {
        if let Some((target, trivia, label, is_func)) = self.labeled_parts() {
            let label_node = if is_func {
                ContentNode::LabelDef(label.to_string())
            } else {
                ContentNode::Label(label.to_string())
            };
            if let [single] = target {
                if let ContentNode::ListItem(body) = single {
                    let mut combined = body.clone();
                    combined.extend(
                        trivia
                            .iter()
                            .filter(|n| matches!(n, ContentNode::Space))
                            .cloned(),
                    );
                    combined.push(label_node);
                    return format_list_item_typst_inner("- ", &combined, emit_anchors);
                }
                if let ContentNode::EnumItem { number, content } = single {
                    let mut combined = content.clone();
                    combined.extend(
                        trivia
                            .iter()
                            .filter(|n| matches!(n, ContentNode::Space))
                            .cloned(),
                    );
                    combined.push(label_node);
                    let prefix = number
                        .map(|n| format!("{n}. "))
                        .unwrap_or_else(|| "+ ".to_string());
                    return format_list_item_typst_inner(&prefix, &combined, emit_anchors);
                }
                if let Some((term, desc)) = single.term_item_parts() {
                    let mut combined_desc = desc.to_vec();
                    combined_desc.extend(
                        trivia
                            .iter()
                            .filter(|n| matches!(n, ContentNode::Space))
                            .cloned(),
                    );
                    combined_desc.push(label_node);
                    return format!(
                        "#{TERM_ITEM_NAME}({}, {})",
                        render_content_block_arg_inner(term, emit_anchors),
                        render_content_block_arg_inner(&combined_desc, emit_anchors),
                    );
                }
            }
            let mut nodes = target.to_vec();
            nodes.extend(trivia.iter().cloned());
            nodes.push(label_node);
            return render_content_nodes_inner(&nodes, emit_anchors);
        }
        if let Some((counter, kind, label, step)) = self.ref_anchor_parts() {
            if !emit_anchors {
                return String::new();
            }
            let label_arg = label
                .map(|lbl| format!(", label: <{lbl}>"))
                .unwrap_or_default();
            return format!(
                "#{REF_ANCHOR_ELEMENT_NAME}(counter: \"{counter}\", kind: \"{kind}\", step: {step}{label_arg})"
            );
        }
        if let Some((counter, origin_id)) = self.dup_snapshot_parts() {
            if !emit_anchors {
                return String::new();
            }
            return format!("#{DUP_SNAPSHOT_NAME}(counter: \"{counter}\", id: {origin_id})");
        }
        if let Some((counter, id, origin_id, body)) = self.dup_element_parts() {
            if !emit_anchors {
                return render_content_nodes_inner(body, false);
            }
            return format!(
                "#{DUP_ELEMENT_NAME}(counter: \"{counter}\", id: {id}, origin: {origin_id}){}",
                render_content_block_arg_inner(body, true)
            );
        }
        if let Some((term, description)) = self.term_item_parts() {
            // Content blocks preserve colons and paragraphs and can appear inside a list item.
            return format!(
                "#{TERM_ITEM_NAME}({}, {})",
                render_content_block_arg_inner(term, emit_anchors),
                render_content_block_arg_inner(description, emit_anchors),
            );
        }
        if let Some((items, marker)) = self.list_container_parts() {
            let Some(m) = marker else {
                return render_content_nodes_inner(&items, emit_anchors);
            };
            let mut args = vec![format!(
                "marker: {}",
                value_to_typst_arg_inner(m, emit_anchors)
            )];
            for item in &items {
                match item {
                    ContentNode::ListItem(body) => {
                        args.push(render_content_block_arg_inner(body, emit_anchors));
                    }
                    other => {
                        if let Some(([ContentNode::ListItem(body)], trivia, label, is_func)) =
                            other.labeled_parts()
                        {
                            let mut combined = body.clone();
                            combined.extend(
                                trivia
                                    .iter()
                                    .filter(|n| matches!(n, ContentNode::Space))
                                    .cloned(),
                            );
                            combined.push(if is_func {
                                ContentNode::LabelDef(label.to_string())
                            } else {
                                ContentNode::Label(label.to_string())
                            });
                            args.push(render_content_block_arg_inner(&combined, emit_anchors));
                        } else {
                            args.push(render_content_block_arg_inner(
                                std::slice::from_ref(other),
                                emit_anchors,
                            ));
                        }
                    }
                }
            }
            return format!("#list({})", args.join(", "));
        }
        if let Some((items, start, numbering)) = self.enum_container_parts() {
            if start.is_none() && numbering.is_none() {
                return render_content_nodes_inner(&items, emit_anchors);
            }
            let mut args = Vec::new();
            if let Some(s) = start {
                args.push(format!("start: {s}"));
            }
            if let Some(num) = numbering {
                args.push(format!(
                    "numbering: {}",
                    value_to_typst_arg_inner(num, emit_anchors)
                ));
            }
            for item in &items {
                match item {
                    ContentNode::EnumItem {
                        number: Some(n),
                        content,
                    } => {
                        args.push(format!(
                            "enum.item({n}, {})",
                            render_content_block_arg_inner(content, emit_anchors)
                        ));
                    }
                    ContentNode::EnumItem {
                        number: None,
                        content,
                    } => {
                        args.push(render_content_block_arg_inner(content, emit_anchors));
                    }
                    other => {
                        if let Some((
                            [ContentNode::EnumItem { number, content }],
                            trivia,
                            label,
                            is_func,
                        )) = other.labeled_parts()
                        {
                            let mut combined = content.clone();
                            combined.extend(
                                trivia
                                    .iter()
                                    .filter(|n| matches!(n, ContentNode::Space))
                                    .cloned(),
                            );
                            combined.push(if is_func {
                                ContentNode::LabelDef(label.to_string())
                            } else {
                                ContentNode::Label(label.to_string())
                            });
                            let block = render_content_block_arg_inner(&combined, emit_anchors);
                            if let Some(n) = number {
                                args.push(format!("enum.item({n}, {block})"));
                            } else {
                                args.push(block);
                            }
                        } else {
                            args.push(render_content_block_arg_inner(
                                std::slice::from_ref(other),
                                emit_anchors,
                            ));
                        }
                    }
                }
            }
            return format!("#enum({})", args.join(", "));
        }
        if let Some(items) = self.terms_container_items() {
            return render_content_nodes_inner(&items, emit_anchors);
        }
        match self {
            ContentNode::Text(s) => escape_typst_markup_text(s),
            ContentNode::Space => " ".to_string(),
            ContentNode::Linebreak => "\\\n".to_string(),
            ContentNode::Parbreak => "\n\n".to_string(),
            ContentNode::Strong(children) => {
                format!("*{}*", render_content_nodes_inner(children, emit_anchors))
            }
            ContentNode::Emph(children) => {
                format!("_{}_", render_content_nodes_inner(children, emit_anchors))
            }
            ContentNode::Raw { text, lang, block } => {
                if *block {
                    let lang_str = lang.as_deref().unwrap_or("");
                    format!("```{}\n{}\n```", lang_str, text)
                } else {
                    format!("`{}`", text)
                }
            }
            ContentNode::Math { segments, block } => {
                let content = render_math_segments_to_typst_source(segments);
                if *block {
                    format!("$ {} $", content)
                } else {
                    format!("${}$", content)
                }
            }
            ContentNode::Heading { level, content } => {
                let prefix = "=".repeat(*level as usize);
                format!(
                    "{} {}\n",
                    prefix,
                    render_content_nodes_inner(content, emit_anchors)
                )
            }
            ContentNode::ListItem(children) => {
                format_list_item_typst_inner("- ", children, emit_anchors)
            }
            ContentNode::EnumItem { number, content } => {
                if let Some(n) = number {
                    format_list_item_typst_inner(&format!("{n}. "), content, emit_anchors)
                } else {
                    format_list_item_typst_inner("+ ", content, emit_anchors)
                }
            }
            ContentNode::Element { name, fields } if name == "heading" => {
                if matches!(fields.get("numbering"), Some(Value::None)) {
                    let mut args = Vec::new();
                    let mut body_arg = None;
                    for (k, v) in fields {
                        if k == "body" {
                            body_arg = Some(match v {
                                Value::Content(nodes) => {
                                    render_content_block_arg_inner(nodes, emit_anchors)
                                }
                                other => render_content_block_arg_inner(
                                    &[ContentNode::Text(other.display())],
                                    emit_anchors,
                                ),
                            });
                        } else if !k.starts_with("__tylax_") {
                            args.push(format!(
                                "{}: {}",
                                k,
                                value_to_typst_arg_inner(v, emit_anchors)
                            ));
                        }
                    }
                    if let Some(body) = body_arg {
                        args.push(body);
                    }
                    format!("#heading({})", args.join(", "))
                } else {
                    let level = fields
                        .get("level")
                        .or_else(|| fields.get("depth"))
                        .and_then(|v| match v {
                            Value::Int(i) => Some((*i).clamp(1, 6) as usize),
                            _ => None,
                        })
                        .unwrap_or(1);
                    let prefix = "=".repeat(level);
                    let body_str = match fields.get("body") {
                        Some(Value::Content(nodes)) => {
                            render_content_nodes_inner(nodes, emit_anchors)
                        }
                        Some(other) => other.display(),
                        None => String::new(),
                    };
                    format!("{} {}\n", prefix, body_str)
                }
            }
            ContentNode::Element { name, fields } => {
                let args: Vec<String> = fields
                    .iter()
                    .filter(|(k, _)| !k.starts_with("__tylax_"))
                    .map(|(k, v)| format!("{}: {}", k, value_to_typst_arg_inner(v, emit_anchors)))
                    .collect();
                format!("#{}({})", name, args.join(", "))
            }
            ContentNode::Label(l) => format!("<{}>", l),
            ContentNode::Citation {
                keys,
                mode,
                supplement,
            } => {
                let mut group = CiteGroup::new();
                group.suffix = supplement
                    .clone()
                    .and_then(|value| normalize_supplement_text(&value));
                for key in keys {
                    group.push(Citation::with_mode(key.clone(), *mode));
                }
                citation_to_typst(&group)
            }
            ContentNode::Reference { target, ref_type } => match ref_type {
                ReferenceType::Equation => {
                    let target = if target.starts_with("eq-") {
                        target.clone()
                    } else {
                        format!("eq-{}", target)
                    };
                    format!("@{}", target)
                }
                ReferenceType::Page => format!("#locate(loc => {{@{}.page()}})", target),
                _ => format!("#ref(<{}>)", target),
            },
            ContentNode::LabelDef(l) => format!("#label(<{}>)", l),
            ContentNode::Bibliography { file, style } => {
                if let Some(style) = style {
                    format!("#bibliography(\"{}\", style: {})", file, style)
                } else {
                    format!("#bibliography(\"{}\")", file)
                }
            }
            ContentNode::FuncCall { name, args } => {
                func_call_to_typst_source_inner(name, args, emit_anchors)
            }
            ContentNode::RawSource(s) => s.clone(),
            ContentNode::State { key, default } => {
                format!(
                    "#state({:?}, {}).display()",
                    key,
                    value_to_typst_arg_inner(default, emit_anchors)
                )
            }
            ContentNode::CounterDisplay { key, numbering } => {
                if numbering.is_empty() {
                    format!("#counter({:?}).step()", key)
                } else {
                    format!("#counter({:?}).display({:?})", key, numbering)
                }
            }
        }
    }

    /// Convert this content node for embedding inside math mode.
    ///
    /// This is similar to `to_typst()` but handles `Math` nodes specially:
    /// instead of outputting `$content$`, it outputs just `content`.
    /// This prevents nested `$` symbols when expanding variables inside math.
    ///
    /// Example:
    /// - `to_typst()` on Math("x") → "$x$"
    /// - `to_typst_in_math()` on Math("x") → "x"
    pub fn to_typst_in_math(&self) -> String {
        match self {
            // For Math nodes, output just the content without $ delimiters
            ContentNode::Math { segments, .. } => render_math_segments_to_typst_source(segments),
            ContentNode::FuncCall { name, args } => func_call_to_typst_math_source(name, args),
            // For all other nodes, use regular to_typst()
            _ => self.to_typst(),
        }
    }
}

/// Convert a value to a Typst argument string.
pub fn value_to_typst_arg(v: &Value) -> String {
    value_to_typst_arg_inner(v, false)
}

fn value_to_typst_arg_inner(v: &Value, emit_anchors: bool) -> String {
    match v {
        Value::None => "none".to_string(),
        Value::Auto => "auto".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format_float(*f),
        Value::Str(s) => format!("\"{}\"", escape_typst_string(s)),
        Value::Length(l) => l.to_typst(),
        Value::Ratio(r) => format!("{}%", r * 100.0),
        Value::Angle(a) => format!("{}deg", a),
        Value::Fraction(f) => format!("{}fr", f),
        Value::Color(c) => c.to_typst(),
        Value::Alignment(a) => a.to_typst(),
        Value::Direction(d) => format!("{:?}", d).to_lowercase(),
        Value::Symbol(s) => s.default.clone(),
        Value::Array(arr) => {
            let items: Vec<String> = arr
                .iter()
                .map(|item| value_to_typst_arg_inner(item, emit_anchors))
                .collect();
            format!("({})", items.join(", "))
        }
        Value::Dict(dict) => {
            let items: Vec<String> = dict
                .iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_typst_arg_inner(v, emit_anchors)))
                .collect();
            format!("({})", items.join(", "))
        }
        Value::Func(c) => c
            .builtin_name()
            .map(str::to_string)
            .unwrap_or_else(|| "<function>".to_string()),
        Value::Content(nodes) => {
            // If content contains a single function call, output it directly without []
            // This avoids wrapping like [#table(...)] which breaks nested conversion
            if nodes.len() == 1 {
                if let ContentNode::FuncCall { name, args } = &nodes[0] {
                    return func_call_to_typst_code_source_inner(name, args, emit_anchors);
                }
            }
            render_content_block_arg_inner(nodes, emit_anchors)
        }
        Value::Regex(r) => format!("regex(\"{}\")", r.as_str()),
        Value::DateTime(dt) => dt.to_typst(),
        Value::Label(l) => format!("<{}>", l),
        Value::Selector(_) => "<selector>".to_string(),
        Value::Counter(_) => "<counter>".to_string(),
        Value::State(s) => format!("state({:?})", s.key),
        Value::Arguments(_) => "<arguments>".to_string(),
        Value::Module(name, _) => format!("<module {}>", name),
        Value::Version(v) => format!(
            "version({})",
            v.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Bytes(b) => format!("bytes({})", b.len()),
        Value::Type(t) => t.name().to_string(),
        Value::Styles => "<styles>".to_string(),
    }
}

// ============================================================================
// Source Span Types
// ============================================================================

/// A source span representing a range in the source code.
///
/// This is a simplified representation that doesn't depend on typst_syntax internals,
/// making it easier to work with and serialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceSpan {
    /// Start byte offset in the source
    pub start: usize,
    /// End byte offset in the source  
    pub end: usize,
}

impl SourceSpan {
    /// Create a new source span.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Create from a typst_syntax Span.
    /// Note: typst_syntax::Span requires a Source context for range extraction.
    /// This is a simplified version that uses the span's raw range if available.
    pub fn from_typst_span(span: typst_syntax::Span) -> Option<Self> {
        // typst_syntax::Span::range() returns Option<Range<usize>>
        let range = span.range()?;
        Some(Self {
            start: range.start,
            end: range.end,
        })
    }

    /// Check if this span is empty/default.
    pub fn is_empty(&self) -> bool {
        self.start == 0 && self.end == 0
    }

    /// Get the length of this span.
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Extract the text covered by this span from a source string.
    pub fn extract<'a>(&self, source: &'a str) -> Option<&'a str> {
        if self.end <= source.len() {
            Some(&source[self.start..self.end])
        } else {
            None
        }
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// The kind of evaluation error (without span information).
#[derive(Debug, Clone)]
pub enum EvalErrorKind {
    /// Type mismatch
    TypeMismatch {
        expected: &'static str,
        got: &'static str,
    },
    /// Undefined variable
    UndefinedVariable(String),
    /// Division by zero
    DivisionByZero,
    /// Invalid operation
    InvalidOperation(String),
    /// Too many iterations (infinite loop protection)
    TooManyIterations,
    /// Recursion depth exceeded (infinite recursion protection)
    RecursionLimitExceeded { max_depth: usize },
    /// Function argument error
    ArgumentError(String),
    /// Index out of bounds
    IndexOutOfBounds { index: i64, len: usize },
    /// Key not found
    KeyNotFound(String),
    /// Syntax error in source
    SyntaxError(String),
    /// File not found
    FileNotFound(String),
    /// Import error
    ImportError(String),
    /// Regex error
    RegexError(String),
    /// Generic error
    Other(String),
}

impl fmt::Display for EvalErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalErrorKind::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {}, got {}", expected, got)
            }
            EvalErrorKind::UndefinedVariable(name) => {
                write!(f, "undefined variable: {}", name)
            }
            EvalErrorKind::DivisionByZero => write!(f, "division by zero"),
            EvalErrorKind::InvalidOperation(msg) => write!(f, "invalid operation: {}", msg),
            EvalErrorKind::TooManyIterations => {
                write!(f, "loop seems infinite (>10000 iterations)")
            }
            EvalErrorKind::RecursionLimitExceeded { max_depth } => {
                write!(
                    f,
                    "recursion depth exceeded maximum ({}). Possible infinite recursion.",
                    max_depth
                )
            }
            EvalErrorKind::ArgumentError(msg) => write!(f, "argument error: {}", msg),
            EvalErrorKind::IndexOutOfBounds { index, len } => {
                write!(f, "index {} out of bounds for length {}", index, len)
            }
            EvalErrorKind::KeyNotFound(key) => write!(f, "key not found: {}", key),
            EvalErrorKind::SyntaxError(msg) => write!(f, "syntax error: {}", msg),
            EvalErrorKind::FileNotFound(path) => write!(f, "file not found: {}", path),
            EvalErrorKind::ImportError(msg) => write!(f, "import error: {}", msg),
            EvalErrorKind::RegexError(msg) => write!(f, "regex error: {}", msg),
            EvalErrorKind::Other(msg) => write!(f, "{}", msg),
        }
    }
}

/// Errors that can occur during evaluation, with optional source span.
#[derive(Debug, Clone)]
pub struct EvalError {
    /// The kind of error
    pub kind: EvalErrorKind,
    /// Optional source span where the error occurred
    pub span: Option<SourceSpan>,
    /// Optional file path where the error occurred
    pub file: Option<String>,
}

impl EvalError {
    /// Create a new error from a kind.
    pub fn new(kind: EvalErrorKind) -> Self {
        Self {
            kind,
            span: None,
            file: None,
        }
    }

    /// Attach a span to this error.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a file path to this error.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    /// Create a type mismatch error.
    pub fn type_mismatch(expected: &'static str, got: &'static str) -> Self {
        Self::new(EvalErrorKind::TypeMismatch { expected, got })
    }

    /// Create an undefined variable error.
    pub fn undefined(name: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::UndefinedVariable(name.into()))
    }

    /// Create a division by zero error.
    pub fn div_zero() -> Self {
        Self::new(EvalErrorKind::DivisionByZero)
    }

    /// Create an invalid operation error.
    pub fn invalid_op(msg: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::InvalidOperation(msg.into()))
    }

    /// Create a too many iterations error.
    pub fn too_many_iterations() -> Self {
        Self::new(EvalErrorKind::TooManyIterations)
    }

    /// Create an argument error.
    pub fn argument(msg: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::ArgumentError(msg.into()))
    }

    /// Create an index out of bounds error.
    pub fn index_oob(index: i64, len: usize) -> Self {
        Self::new(EvalErrorKind::IndexOutOfBounds { index, len })
    }

    /// Create a key not found error.
    pub fn key_not_found(key: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::KeyNotFound(key.into()))
    }

    /// Create a syntax error.
    pub fn syntax(msg: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::SyntaxError(msg.into()))
    }

    /// Create a file not found error.
    pub fn file_not_found(path: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::FileNotFound(path.into()))
    }

    /// Create an import error.
    pub fn import(msg: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::ImportError(msg.into()))
    }

    /// Create a regex error.
    pub fn regex(msg: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::RegexError(msg.into()))
    }

    /// Create a generic error.
    pub fn other(msg: impl Into<String>) -> Self {
        Self::new(EvalErrorKind::Other(msg.into()))
    }

    /// Get the error kind.
    pub fn kind(&self) -> &EvalErrorKind {
        &self.kind
    }

    /// Format error with source context if available.
    pub fn format_with_source(&self, source: &str) -> String {
        let mut msg = self.kind.to_string();

        if let Some(span) = &self.span {
            // Calculate line and column
            let prefix = &source[..span.start.min(source.len())];
            let line = prefix.lines().count().max(1);
            let last_newline = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
            let col = span.start - last_newline + 1;

            msg = format!("{}:{}: {}", line, col, msg);

            // Add source context
            if let Some(extract) = span.extract(source) {
                let snippet = if extract.len() > 40 {
                    format!("{}...", &extract[..40])
                } else {
                    extract.to_string()
                };
                msg = format!("{}\n  --> `{}`", msg, snippet);
            }
        }

        if let Some(file) = &self.file {
            msg = format!("{}: {}", file, msg);
        }

        msg
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(span) = &self.span {
            write!(f, " at {}", span)?;
        }
        Ok(())
    }
}

impl std::error::Error for EvalError {}

// Legacy compatibility: Allow creating EvalError from EvalErrorKind variants directly
impl From<EvalErrorKind> for EvalError {
    fn from(kind: EvalErrorKind) -> Self {
        Self::new(kind)
    }
}

/// Result type for evaluation operations.
pub type EvalResult<T> = Result<T, EvalError>;

// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_func_call_to_typst_uses_code_syntax_for_nested_calls() {
        let nested = ContentNode::FuncCall {
            name: "h".to_string(),
            args: vec![Arg::Pos(Value::Content(vec![ContentNode::FuncCall {
                name: "image".to_string(),
                args: vec![Arg::Pos(Value::Str("x.png".to_string()))],
            }]))],
        };

        assert_eq!(nested.to_typst(), r#"#h(image("x.png"))"#);
    }

    #[test]
    fn test_math_nodes_serialize_preserved_func_calls_without_bracket_wrapping() {
        let math = ContentNode::Math {
            segments: vec![
                MathSegment::Source("a ".to_string()),
                MathSegment::Evaluated(Value::Content(vec![ContentNode::FuncCall {
                    name: "h".to_string(),
                    args: vec![Arg::Pos(Value::Length(Length::exact(1.0, LengthUnit::Cm)))],
                }])),
                MathSegment::Source(" b".to_string()),
            ],
            block: false,
        };

        assert_eq!(math.to_typst(), "$a #h(1cm) b$");
        assert_eq!(math.to_typst_in_math(), "a #h(1cm) b");
    }

    #[test]
    fn test_func_call_to_typst_math_source_serializes_common_spacing_args() {
        let fixed = func_call_to_typst_math_source(
            "h",
            &[Arg::Pos(Value::Length(Length::exact(1.0, LengthUnit::Cm)))],
        );
        let flex = func_call_to_typst_math_source("h", &[Arg::Pos(Value::Fraction(1.0))]);
        let plain = func_call_to_typst_math_source(
            "h",
            &[Arg::Pos(Value::Content(vec![ContentNode::Text(
                "foo".to_string(),
            )]))],
        );

        assert_eq!(fixed, "#h(1cm)");
        assert_eq!(flex, "#h(1fr)");
        // The arguments of a `#` call are code, where bare `foo` is a variable.
        assert_eq!(plain, "#h([foo])");
    }

    #[test]
    fn test_terms_and_list_selectors_separate_containers_from_items() {
        let term = vec![ContentNode::Text("Key".to_string())];
        let desc = vec![ContentNode::Text("Value".to_string())];
        let term_item = ContentNode::term_item(term.clone(), desc);
        let terms_box = ContentNode::terms_container(vec![term_item.clone()], true);

        let list_item = ContentNode::ListItem(vec![ContentNode::Text("bullet".to_string())]);
        let list_box = ContentNode::list_container(vec![list_item.clone()], true, None);

        let enum_item = ContentNode::EnumItem {
            number: Some(2),
            content: vec![ContentNode::Text("numbered".to_string())],
        };
        let enum_box = ContentNode::enum_container(vec![enum_item.clone()], true, Some(2), None);
        let text_node = ContentNode::Text("Other".to_string());

        let terms_sel = Selector::element("terms");
        let term_item_sel = Selector::element("terms.item");
        let list_sel = Selector::element("list");
        let list_item_sel = Selector::element("list.item");
        let enum_sel = Selector::element("enum");
        let enum_item_sel = Selector::element("enum.item");
        let filtered_sel = Selector::Element(
            "terms.item".to_string(),
            vec![("term".to_string(), Value::Content(term))],
        );
        let wrong_filter_sel = Selector::Element(
            "terms.item".to_string(),
            vec![(
                "term".to_string(),
                Value::Content(vec![ContentNode::Text("Missing".to_string())]),
            )],
        );

        assert!(terms_sel.matches(&terms_box));
        assert!(!terms_sel.matches(&term_item));
        assert!(term_item_sel.matches(&term_item));
        assert!(!term_item_sel.matches(&terms_box));

        assert!(list_sel.matches(&list_box));
        assert!(!list_sel.matches(&list_item));
        assert!(list_item_sel.matches(&list_item));
        assert!(!list_item_sel.matches(&list_box));

        assert!(enum_sel.matches(&enum_box));
        assert!(!enum_sel.matches(&enum_item));
        assert!(enum_item_sel.matches(&enum_item));
        assert!(!enum_item_sel.matches(&enum_box));

        assert!(!terms_sel.matches(&text_node));
        assert!(!term_item_sel.matches(&text_node));
        assert!(filtered_sel.matches(&term_item));
        assert!(!wrong_filter_sel.matches(&term_item));
    }

    #[test]
    fn test_escape_typst_markup_text_escapes_comment_slashes() {
        assert_eq!(escape_typst_markup_text("A // B"), r"A \/\/ B");
        assert_eq!(escape_typst_markup_text("A /* B */ C"), r"A \/\* B \*\/ C");
        assert_eq!(escape_typst_markup_text("= literal"), r"\= literal");
        assert_eq!(escape_typst_markup_text("- literal"), r"\- literal");
        assert_eq!(escape_typst_markup_text("+ literal"), r"\+ literal");
        assert_eq!(escape_typst_markup_text("1. literal"), r"1\. literal");
        assert_eq!(escape_typst_markup_text("A -- B"), r"A \-\- B");
        assert_eq!(escape_typst_markup_text("A --- B"), r"A \-\-\- B");
        assert_eq!(escape_typst_markup_text("A ... B"), r"A \.\.\. B");
        assert_eq!(escape_typst_markup_text("A ~ B"), r"A \~ B");
        assert_eq!(escape_typst_markup_text("'a' \"b\""), r#"\'a\' \"b\""#);
        assert_eq!(escape_typst_markup_text("1-2-3"), "1-2-3");
        assert_eq!(escape_typst_markup_text("a = b"), "a = b");
        assert_eq!(escape_typst_markup_text("Hello."), "Hello.");
        assert_eq!(escape_typst_markup_text("3.14"), "3.14");

        let item = ContentNode::term_item(
            vec![ContentNode::Text("Key".to_string())],
            vec![ContentNode::Text("A // B".to_string())],
        );
        assert_eq!(item.to_typst(), r"#terms.item([Key], [A \/\/ B])");
    }
}
