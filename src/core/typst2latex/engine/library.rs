//! Standard library for the MiniEval interpreter.
//!
//! This module implements built-in functions and methods that match typst-hs
//! capabilities for full macro evaluation support.

use crate::features::refs::{citation_mode_from_typst_form, CitationMode, ReferenceType};
use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};
use indexmap::IndexMap;
use regex::Regex;
use std::sync::Arc;

use super::data;
use super::value::{
    bibliography_content_value, citation_content_value, label_content_value,
    normalize_ref_target_text, reference_content_value, Alignment, Arg, Arguments, Closure, Color,
    ContentNode, Counter, DateTime, EvalError, EvalResult, HorizAlign, Length, LengthUnit,
    Selector, State, Value, VertAlign, WrappedRegex,
};
use super::vfs::VirtualFileSystem;

/// Result of trying to call a built-in function.
///
/// This allows distinguishing between "function not found" and "function found but failed"
/// which is important for strict/compat mode handling.
pub enum BuiltinResult {
    /// Function was found and executed successfully
    Ok(Value),
    /// Function was found but execution failed
    Err(EvalError),
    /// Function was not found (undefined)
    NotFound,
}

impl From<EvalResult<Value>> for BuiltinResult {
    fn from(result: EvalResult<Value>) -> Self {
        match result {
            Ok(v) => BuiltinResult::Ok(v),
            Err(e) => BuiltinResult::Err(e),
        }
    }
}

/// Call a built-in function by name.
///
/// In strict mode, undefined functions return an error.
/// In compat mode, undefined functions return NotFound which the caller can handle.
pub fn call_builtin(
    name: &str,
    args: Vec<Value>,
    named: IndexMap<String, Value>,
    vfs: &Arc<dyn VirtualFileSystem>,
) -> BuiltinResult {
    match name {
        // Core functions
        "range" => builtin_range(args).into(),
        "str" => builtin_str(args).into(),
        "int" => builtin_int(args).into(),
        "float" => builtin_float(args).into(),
        "bool" => builtin_bool(args).into(),
        "type" => builtin_type(args).into(),
        "repr" => builtin_repr(args).into(),
        "len" => builtin_len(args).into(),
        "panic" => builtin_panic(args).into(),
        "assert" => builtin_assert(args, named).into(),

        // Type constructors
        "array" => builtin_array(args).into(),
        "dict" | "dictionary" => builtin_dict(args).into(),
        "rgb" => builtin_rgb(args).into(),
        "cmyk" => builtin_cmyk(args).into(),
        "luma" => builtin_luma(args).into(),
        "datetime" => builtin_datetime(named).into(),
        "regex" => builtin_regex(args).into(),
        "version" => builtin_version(args).into(),
        "label" => builtin_label(args).into(),
        "cite" => builtin_cite(args, named).into(),
        "ref" => builtin_ref(args).into(),
        "bibliography" => builtin_bibliography(args, named).into(),
        "arguments" => builtin_arguments(args, named).into(),

        // Length constructors
        "pt" => builtin_length(args, LengthUnit::Pt).into(),
        "mm" => builtin_length(args, LengthUnit::Mm).into(),
        "cm" => builtin_length(args, LengthUnit::Cm).into(),
        "em" => builtin_length(args, LengthUnit::Em).into(),

        // Math-related
        "numbering" => builtin_numbering(args).into(),
        "counter" => builtin_counter(args).into(),
        "state" => builtin_state(args).into(),

        // Text utilities
        "lower" => builtin_lower(args).into(),
        "upper" => builtin_upper(args).into(),
        "lorem" => builtin_lorem(args).into(),

        // Collection utilities
        "zip" => builtin_zip(args).into(),

        // Data loading
        "read" => builtin_read(args, named, vfs).into(),
        "json" => builtin_json(args, vfs).into(),
        "csv" => builtin_csv(args, named, vfs).into(),
        "yaml" => builtin_yaml(args, vfs).into(),
        "toml" => builtin_toml(args, vfs).into(),

        // Layout functions - these produce ContentNode::FuncCall
        // with their arguments preserved for the LaTeX generator
        "place" => builtin_layout_func("place", args, named).into(),
        "box" => builtin_layout_func("box", args, named).into(),
        "block" => builtin_layout_func("block", args, named).into(),
        "rect" => builtin_layout_func("rect", args, named).into(),
        "circle" => builtin_layout_func("circle", args, named).into(),
        "ellipse" => builtin_layout_func("ellipse", args, named).into(),
        "square" => builtin_layout_func("square", args, named).into(),
        "polygon" => builtin_layout_func("polygon", args, named).into(),
        "line" => builtin_layout_func("line", args, named).into(),
        "path" => builtin_layout_func("path", args, named).into(),
        "image" => builtin_image(args, named).into(),
        "figure" => builtin_layout_func("figure", args, named).into(),
        "h" => builtin_layout_func("h", args, named).into(),
        "v" => builtin_layout_func("v", args, named).into(),
        "par" => builtin_layout_func("par", args, named).into(),
        "pagebreak" => builtin_layout_func("pagebreak", args, named).into(),
        "colbreak" => builtin_layout_func("colbreak", args, named).into(),
        "grid" => builtin_layout_func("grid", args, named).into(),
        "stack" => builtin_layout_func("stack", args, named).into(),

        // Layout introspection - these require document context
        // Return mock values with warnings during static evaluation
        "measure" => builtin_measure(args).into(),
        "layout" => builtin_layout(args).into(),

        // Alignment constants (handled as functions)
        "left" | "center" | "right" | "top" | "bottom" | "horizon" | "start" | "end" => {
            builtin_alignment(name).into()
        }

        // System
        "sys" => BuiltinResult::Err(EvalError::other("sys module accessed")),

        _ => BuiltinResult::NotFound,
    }
}

/// Call a method on a value.
pub fn call_method(receiver: &Value, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match receiver {
        Value::Str(s) => call_str_method(s, method, args),
        Value::Array(arr) => call_array_method(arr, method, args),
        Value::Dict(dict) => call_dict_method(dict, method, args),
        Value::Int(i) => call_int_method(*i, method, args),
        Value::Float(f) => call_float_method(*f, method, args),
        Value::Length(l) => call_length_method(l, method, args),
        Value::Color(c) => call_color_method(c, method, args),
        Value::Regex(r) => call_regex_method(r, method, args),
        Value::Arguments(a) => call_arguments_method(a, method, args),
        Value::Version(v) => call_version_method(v, method, args),
        Value::Bytes(b) => call_bytes_method(b, method, args),
        Value::DateTime(dt) => call_datetime_method(dt, method, args),
        Value::Content(c) => call_content_method(c, method, args),
        Value::Counter(c) => call_counter_method(c, method, args),
        Value::State(s) => call_state_method(s, method, args),
        Value::Func(f) => call_func_method(f, method, args),
        Value::Selector(s) => call_selector_method(s, method, args),
        _ => Err(EvalError::invalid_op(format!(
            "{} has no method '{}'",
            receiver.type_name(),
            method
        ))),
    }
}

// ============================================================================
// Built-in functions
// ============================================================================

fn builtin_range(args: Vec<Value>) -> EvalResult<Value> {
    let (start, end, step) = match args.as_slice() {
        [end] => (0, end.as_int()?, 1),
        [start, end] => (start.as_int()?, end.as_int()?, 1),
        [start, end, step] => (start.as_int()?, end.as_int()?, step.as_int()?),
        _ => {
            return Err(EvalError::argument(
                "range expects 1-3 arguments".to_string(),
            ))
        }
    };

    if step == 0 {
        return Err(EvalError::argument("range step cannot be zero".to_string()));
    }

    let mut result = Vec::new();
    if step > 0 {
        let mut i = start;
        while i < end {
            result.push(Value::Int(i));
            i += step;
        }
    } else {
        let mut i = start;
        while i > end {
            result.push(Value::Int(i));
            i += step;
        }
    }

    Ok(Value::Array(result))
}

fn builtin_str(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [v] => Ok(Value::Str(v.display())),
        [] => Ok(Value::Str(String::new())),
        _ => Err(EvalError::argument("str expects 0-1 arguments".to_string())),
    }
}

fn builtin_int(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Int(i)] => Ok(Value::Int(*i)),
        [Value::Float(f)] => Ok(Value::Int(*f as i64)),
        [Value::Str(s)] => s
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| EvalError::invalid_op(format!("cannot parse '{}' as int", s))),
        [Value::Bool(b)] => Ok(Value::Int(if *b { 1 } else { 0 })),
        _ => Err(EvalError::argument("int expects 1 argument".to_string())),
    }
}

fn builtin_float(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Int(i)] => Ok(Value::Float(*i as f64)),
        [Value::Float(f)] => Ok(Value::Float(*f)),
        [Value::Str(s)] => s
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| EvalError::invalid_op(format!("cannot parse '{}' as float", s))),
        [Value::Ratio(r)] => Ok(Value::Float(*r)),
        _ => Err(EvalError::argument("float expects 1 argument".to_string())),
    }
}

fn builtin_bool(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [v] => Ok(Value::Bool(v.is_truthy())),
        _ => Err(EvalError::argument("bool expects 1 argument".to_string())),
    }
}

fn builtin_type(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [v] => Ok(Value::Type(v.val_type())),
        _ => Err(EvalError::argument("type expects 1 argument".to_string())),
    }
}

fn builtin_repr(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [v] => Ok(Value::Str(format!("{:?}", v))),
        _ => Err(EvalError::argument("repr expects 1 argument".to_string())),
    }
}

fn builtin_len(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Str(s)] => Ok(Value::Int(s.chars().count() as i64)),
        [Value::Array(arr)] => Ok(Value::Int(arr.len() as i64)),
        [Value::Dict(dict)] => Ok(Value::Int(dict.len() as i64)),
        [Value::Bytes(b)] => Ok(Value::Int(b.len() as i64)),
        [v] => Err(EvalError::invalid_op(format!(
            "{} has no len",
            v.type_name()
        ))),
        _ => Err(EvalError::argument("len expects 1 argument".to_string())),
    }
}

fn builtin_panic(args: Vec<Value>) -> EvalResult<Value> {
    let msg = args.first().map(|v| v.display()).unwrap_or_default();
    Err(EvalError::other(format!("panic: {}", msg)))
}

fn builtin_assert(args: Vec<Value>, named: IndexMap<String, Value>) -> EvalResult<Value> {
    match args.first() {
        Some(v) if v.as_bool()? => Ok(Value::None),
        Some(_) => {
            let msg = named
                .get("message")
                .map(|v| v.display())
                .unwrap_or_else(|| "assertion failed".to_string());
            Err(EvalError::other(msg))
        }
        None => Err(EvalError::argument("assert expects 1 argument".to_string())),
    }
}

fn builtin_array(args: Vec<Value>) -> EvalResult<Value> {
    Ok(Value::Array(args))
}

fn builtin_dict(args: Vec<Value>) -> EvalResult<Value> {
    if args.is_empty() {
        return Ok(Value::Dict(IndexMap::new()));
    }
    // If single dict argument, return it
    if args.len() == 1 {
        if let Value::Dict(d) = &args[0] {
            return Ok(Value::Dict(d.clone()));
        }
    }
    // Convert array of pairs to dict
    if args.len() == 1 {
        if let Value::Array(pairs) = &args[0] {
            let mut dict = IndexMap::new();
            for pair in pairs {
                if let Value::Array(kv) = pair {
                    if kv.len() == 2 {
                        let key = kv[0].as_str()?.to_string();
                        dict.insert(key, kv[1].clone());
                    }
                }
            }
            return Ok(Value::Dict(dict));
        }
    }
    Err(EvalError::argument(
        "dict expects named arguments or array of pairs".to_string(),
    ))
}

fn builtin_rgb(args: Vec<Value>) -> EvalResult<Value> {
    if args.len() == 3 || args.len() == 4 {
        // rgb(r, g, b, a?) where components are numbers (0-255) or ratios
        let mut components = Vec::new();
        for arg in &args {
            if let Value::Int(i) = arg {
                components.push(*i as f64 / 255.0);
            } else if let Value::Float(f) = arg {
                components.push(*f / 255.0);
            } else if let Value::Ratio(r) = arg {
                components.push(*r);
            } else {
                return Err(EvalError::argument(
                    "rgb components must be numbers or ratios".to_string(),
                ));
            }
        }

        let r = components[0];
        let g = components[1];
        let b = components[2];
        let a = if components.len() == 4 {
            components[3]
        } else {
            1.0
        };

        Ok(Value::Color(Color::Rgb { r, g, b, a }))
    } else if args.len() == 1 {
        // rgb(hex_string)
        if let Value::Str(s) = &args[0] {
            Color::from_hex(s)
                .map(Value::Color)
                .ok_or_else(|| EvalError::argument(format!("invalid hex color: {}", s)))
        } else {
            Err(EvalError::argument(
                "single argument to rgb must be a hex string".to_string(),
            ))
        }
    } else {
        Err(EvalError::argument(
            "rgb expects 1, 3, or 4 arguments".to_string(),
        ))
    }
}

fn builtin_cmyk(args: Vec<Value>) -> EvalResult<Value> {
    if args.len() == 4 {
        let mut components = Vec::new();
        for arg in &args {
            if let Value::Ratio(r) = arg {
                components.push(*r);
            } else if let Value::Float(f) = arg {
                // Also support float 0.0-1.0
                components.push(*f);
            } else {
                return Err(EvalError::argument(
                    "cmyk components must be ratios or floats".to_string(),
                ));
            }
        }

        Ok(Value::Color(Color::Cmyk {
            c: components[0],
            m: components[1],
            y: components[2],
            k: components[3],
        }))
    } else {
        Err(EvalError::argument("cmyk expects 4 arguments".to_string()))
    }
}

fn builtin_luma(args: Vec<Value>) -> EvalResult<Value> {
    if args.len() == 1 {
        let v = match &args[0] {
            Value::Int(i) => *i as f64 / 255.0,
            Value::Float(f) => *f / 255.0,
            Value::Ratio(r) => *r,
            _ => {
                return Err(EvalError::argument(
                    "luma component must be number or ratio".to_string(),
                ));
            }
        };
        Ok(Value::Color(Color::Luma(v)))
    } else {
        Err(EvalError::argument("luma expects 1 argument".to_string()))
    }
}

fn builtin_datetime(named: IndexMap<String, Value>) -> EvalResult<Value> {
    // Basic implementation
    let year = named
        .get("year")
        .and_then(|v| v.as_int().ok())
        .map(|i| i as i32);
    let month = named
        .get("month")
        .and_then(|v| v.as_int().ok())
        .map(|i| i as u32);
    let day = named
        .get("day")
        .and_then(|v| v.as_int().ok())
        .map(|i| i as u32);

    let hour = named
        .get("hour")
        .and_then(|v| v.as_int().ok())
        .map(|i| i as u32);
    let minute = named
        .get("minute")
        .and_then(|v| v.as_int().ok())
        .map(|i| i as u32);
    let second = named
        .get("second")
        .and_then(|v| v.as_int().ok())
        .map(|i| i as u32);

    let date = if let (Some(y), Some(m), Some(d)) = (year, month, day) {
        NaiveDate::from_ymd_opt(y, m, d)
    } else {
        None
    };

    let time = if let (Some(h), Some(m), Some(s)) = (hour, minute, second) {
        NaiveTime::from_hms_opt(h, m, s)
    } else {
        None
    };

    if date.is_none() && time.is_none() {
        return Err(EvalError::argument(
            "datetime requires at least year/month/day or hour/minute/second".to_string(),
        ));
    }

    Ok(Value::DateTime(DateTime { date, time }))
}

fn builtin_regex(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Str(s)] => {
            let re = Regex::new(s).map_err(|e| EvalError::other(e.to_string()))?;
            Ok(Value::Regex(WrappedRegex(re)))
        }
        _ => Err(EvalError::argument(
            "regex expects 1 string argument".to_string(),
        )),
    }
}

fn builtin_version(args: Vec<Value>) -> EvalResult<Value> {
    // Support version(1, 2, 3) or version((1, 2, 3))
    let parts = if args.len() == 1 {
        if let Value::Array(arr) = &args[0] {
            arr.clone()
        } else {
            args
        }
    } else {
        args
    };

    let mut version = Vec::new();
    for part in parts {
        version.push(part.as_int()? as u32);
    }

    Ok(Value::Version(version))
}

fn value_to_ref_target(value: &Value) -> EvalResult<String> {
    let raw = match value {
        Value::Label(l) => l.clone(),
        Value::Str(s) => s.clone(),
        Value::Content(nodes) if nodes.len() == 1 => match &nodes[0] {
            ContentNode::Label(l) => l.clone(),
            ContentNode::Text(t) => t.clone(),
            _ => content_node_to_text(&nodes[0]).trim().to_string(),
        },
        Value::Content(nodes) => nodes
            .iter()
            .map(content_node_to_text)
            .collect::<String>()
            .trim()
            .to_string(),
        other => other.display().trim().to_string(),
    };
    Ok(normalize_ref_target_text(&raw))
}

fn value_to_plain_text(value: &Value) -> String {
    match value {
        Value::Str(s) => s.clone(),
        Value::Label(l) => l.clone(),
        Value::Content(nodes) => nodes
            .iter()
            .map(content_node_to_text)
            .collect::<String>()
            .trim()
            .to_string(),
        other => other.display().trim().to_string(),
    }
}

fn builtin_label(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [value] => Ok(label_content_value(value_to_ref_target(value)?)),
        _ => Err(EvalError::argument(
            "label expects 1 string argument".to_string(),
        )),
    }
}

fn builtin_cite(args: Vec<Value>, named: IndexMap<String, Value>) -> EvalResult<Value> {
    let keys = args
        .iter()
        .map(value_to_ref_target)
        .collect::<EvalResult<Vec<_>>>()?;
    let mode = citation_mode_from_typst_form(named.get("form").map(value_to_plain_text).as_deref());
    let supplement = named.get("supplement").map(value_to_plain_text);
    citation_content_value(keys, mode, supplement)
}

fn builtin_ref(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [value] => Ok(reference_content_value(
            value_to_ref_target(value)?,
            ReferenceType::Basic,
        )),
        _ => Err(EvalError::argument(
            "ref expects 1 label argument".to_string(),
        )),
    }
}

fn builtin_bibliography(args: Vec<Value>, named: IndexMap<String, Value>) -> EvalResult<Value> {
    let file = args.first().map(value_to_plain_text).unwrap_or_default();
    let style = named.get("style").map(value_to_plain_text);
    bibliography_content_value(file, style)
}

fn builtin_arguments(args: Vec<Value>, named: IndexMap<String, Value>) -> EvalResult<Value> {
    Ok(Value::Arguments(Arguments {
        positional: args,
        named,
    }))
}

fn builtin_length(args: Vec<Value>, unit: LengthUnit) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Int(i)] => Ok(Value::Length(Length::exact(*i as f64, unit))),
        [Value::Float(f)] => Ok(Value::Length(Length::exact(*f, unit))),
        _ => Err(EvalError::argument(format!(
            "{} expects 1 number argument",
            unit.suffix()
        ))),
    }
}

fn builtin_numbering(_args: Vec<Value>) -> EvalResult<Value> {
    Ok(Value::Str("1".to_string()))
}

fn builtin_counter(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Label(l)] => Ok(Value::Counter(Counter::Label(l.clone()))),
        [Value::Str(s)] => Ok(Value::Counter(Counter::Custom(s.clone()))),
        _ => Err(EvalError::argument(
            "counter expects label or string key".to_string(),
        )),
    }
}

fn builtin_state(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Str(key), default] => Ok(Value::State(State {
            key: key.clone(),
            init: Box::new(default.clone()),
        })),
        [Value::Str(key)] => Ok(Value::State(State {
            key: key.clone(),
            init: Box::new(Value::None),
        })),
        _ => Err(EvalError::argument(
            "state expects key string and optional default value".to_string(),
        )),
    }
}

fn builtin_lower(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Str(s)] => Ok(Value::Str(s.to_lowercase())),
        [Value::Content(_)] => Ok(args[0].clone()),
        _ => Err(EvalError::argument(
            "lower expects string or content".to_string(),
        )),
    }
}

fn builtin_upper(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Str(s)] => Ok(Value::Str(s.to_uppercase())),
        [Value::Content(_)] => Ok(args[0].clone()),
        _ => Err(EvalError::argument(
            "upper expects string or content".to_string(),
        )),
    }
}

fn builtin_lorem(args: Vec<Value>) -> EvalResult<Value> {
    match args.as_slice() {
        [Value::Int(n)] => {
            // Simplified lorem ipsum
            let words = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua";
            let repeated = std::iter::repeat_n(words, (*n as usize + 10) / 10)
                .collect::<Vec<_>>()
                .join(" ");
            let result: String = repeated
                .split_whitespace()
                .take(*n as usize)
                .collect::<Vec<_>>()
                .join(" ");
            Ok(Value::Str(result))
        }
        _ => Err(EvalError::argument("lorem expects word count".to_string())),
    }
}

fn builtin_zip(args: Vec<Value>) -> EvalResult<Value> {
    if args.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }

    // Check all args are arrays
    let arrays: Result<Vec<&Vec<Value>>, _> = args
        .iter()
        .map(|v| {
            if let Value::Array(arr) = v {
                Ok(arr)
            } else {
                Err(EvalError::argument(
                    "zip arguments must be arrays".to_string(),
                ))
            }
        })
        .collect();
    let arrays = arrays?;

    let min_len = arrays.iter().map(|a| a.len()).min().unwrap_or(0);
    let mut result = Vec::new();

    for i in 0..min_len {
        let row: Vec<Value> = arrays.iter().map(|a| a[i].clone()).collect();
        result.push(Value::Array(row));
    }

    Ok(Value::Array(result))
}

fn builtin_alignment(name: &str) -> EvalResult<Value> {
    match name {
        "left" => Ok(Value::Alignment(Alignment::new(
            Some(HorizAlign::Left),
            None,
        ))),
        "start" => Ok(Value::Alignment(Alignment::new(
            Some(HorizAlign::Start),
            None,
        ))),
        "center" => Ok(Value::Alignment(Alignment::new(
            Some(HorizAlign::Center),
            None,
        ))),
        "right" => Ok(Value::Alignment(Alignment::new(
            Some(HorizAlign::Right),
            None,
        ))),
        "end" => Ok(Value::Alignment(Alignment::new(
            Some(HorizAlign::End),
            None,
        ))),
        "top" => Ok(Value::Alignment(Alignment::new(None, Some(VertAlign::Top)))),
        "horizon" => Ok(Value::Alignment(Alignment::new(
            None,
            Some(VertAlign::Horizon),
        ))),
        "bottom" => Ok(Value::Alignment(Alignment::new(
            None,
            Some(VertAlign::Bottom),
        ))),
        _ => Err(EvalError::undefined(name.to_string())),
    }
}

// Data loading builtins

fn builtin_read(
    args: Vec<Value>,
    named: IndexMap<String, Value>,
    vfs: &Arc<dyn VirtualFileSystem>,
) -> EvalResult<Value> {
    let path = args
        .first()
        .ok_or(EvalError::argument("read expects path"))?
        .as_str()?;
    // Check encoding, defaults to utf8 which is what read_text does
    if let Some(enc) = named.get("encoding") {
        if enc.as_str()? != "utf8" {
            return Err(EvalError::other("Only utf8 encoding supported"));
        }
    }
    let content = vfs
        .read_text(path)
        .map_err(|e| EvalError::other(e.to_string()))?;
    Ok(Value::Str(content))
}

fn builtin_json(args: Vec<Value>, vfs: &Arc<dyn VirtualFileSystem>) -> EvalResult<Value> {
    let path = args
        .first()
        .ok_or(EvalError::argument("json expects path"))?
        .as_str()?;
    let content = vfs
        .read_text(path)
        .map_err(|e| EvalError::other(e.to_string()))?;
    data::parse_json(&content)
}

fn builtin_csv(
    args: Vec<Value>,
    named: IndexMap<String, Value>,
    vfs: &Arc<dyn VirtualFileSystem>,
) -> EvalResult<Value> {
    let path = args
        .first()
        .ok_or(EvalError::argument("csv expects path"))?
        .as_str()?;
    let content = vfs
        .read_text(path)
        .map_err(|e| EvalError::other(e.to_string()))?;

    let delimiter = if let Some(d) = named.get("delimiter") {
        d.as_str()?
    } else {
        ","
    };
    if delimiter != "," {
        return Err(EvalError::other("Custom CSV delimiters not yet supported"));
    }

    let has_header = if let Some(row_type) = named.get("row-type") {
        row_type.as_str()? == "dictionary"
    } else {
        true // default is dictionary which implies header? Typst default is `dictionary`.
    };

    data::parse_csv(&content, has_header)
}

fn builtin_yaml(args: Vec<Value>, vfs: &Arc<dyn VirtualFileSystem>) -> EvalResult<Value> {
    let path = args
        .first()
        .ok_or(EvalError::argument("yaml expects path"))?
        .as_str()?;
    let content = vfs
        .read_text(path)
        .map_err(|e| EvalError::other(e.to_string()))?;
    data::parse_yaml(&content)
}

fn builtin_toml(args: Vec<Value>, vfs: &Arc<dyn VirtualFileSystem>) -> EvalResult<Value> {
    let path = args
        .first()
        .ok_or(EvalError::argument("toml expects path"))?
        .as_str()?;
    let content = vfs
        .read_text(path)
        .map_err(|e| EvalError::other(e.to_string()))?;
    data::parse_toml(&content)
}

// ============================================================================
// calc module functions
// ============================================================================

/// Call a calc module function.
pub fn call_calc(name: &str, args: Vec<Value>) -> EvalResult<Value> {
    match name {
        "abs" => match args.first() {
            Some(Value::Int(i)) => Ok(Value::Int(i.abs())),
            Some(Value::Float(f)) => Ok(Value::Float(f.abs())),
            _ => Err(EvalError::argument("calc.abs expects number")),
        },
        "max" => {
            if args.is_empty() {
                return Err(EvalError::argument(
                    "calc.max expects at least one argument",
                ));
            }
            let mut max_val = args[0].clone();
            for arg in &args[1..] {
                if super::ops::gt(arg, &max_val)? {
                    max_val = arg.clone();
                }
            }
            Ok(max_val)
        }
        "min" => {
            if args.is_empty() {
                return Err(EvalError::argument(
                    "calc.min expects at least one argument",
                ));
            }
            let mut min_val = args[0].clone();
            for arg in &args[1..] {
                if super::ops::lt(arg, &min_val)? {
                    min_val = arg.clone();
                }
            }
            Ok(min_val)
        }
        "floor" => match args.first() {
            Some(Value::Float(f)) => Ok(Value::Int(f.floor() as i64)),
            Some(Value::Int(i)) => Ok(Value::Int(*i)),
            _ => Err(EvalError::argument("calc.floor expects number")),
        },
        "ceil" => match args.first() {
            Some(Value::Float(f)) => Ok(Value::Int(f.ceil() as i64)),
            Some(Value::Int(i)) => Ok(Value::Int(*i)),
            _ => Err(EvalError::argument("calc.ceil expects number")),
        },
        "round" => match args.first() {
            Some(Value::Float(f)) => Ok(Value::Int(f.round() as i64)),
            Some(Value::Int(i)) => Ok(Value::Int(*i)),
            _ => Err(EvalError::argument("calc.round expects number")),
        },
        "sqrt" => match args.first() {
            Some(Value::Float(f)) => Ok(Value::Float(f.sqrt())),
            Some(Value::Int(i)) => Ok(Value::Float((*i as f64).sqrt())),
            _ => Err(EvalError::argument("calc.sqrt expects number")),
        },
        "pow" => match args.as_slice() {
            [Value::Int(a), Value::Int(b)] => {
                if *b >= 0 {
                    Ok(Value::Int(a.pow(*b as u32)))
                } else {
                    Ok(Value::Float((*a as f64).powi(*b as i32)))
                }
            }
            [Value::Float(a), Value::Int(b)] => Ok(Value::Float(a.powi(*b as i32))),
            [Value::Int(a), Value::Float(b)] => Ok(Value::Float((*a as f64).powf(*b))),
            [Value::Float(a), Value::Float(b)] => Ok(Value::Float(a.powf(*b))),
            _ => Err(EvalError::argument("calc.pow expects two numbers")),
        },
        "rem" => match args.as_slice() {
            [Value::Int(a), Value::Int(b)] => {
                if *b == 0 {
                    Err(EvalError::other("division by zero"))
                } else {
                    Ok(Value::Int(a % b))
                }
            }
            [Value::Float(a), Value::Float(b)] => Ok(Value::Float(a % b)),
            [Value::Int(a), Value::Float(b)] => Ok(Value::Float(*a as f64 % b)),
            [Value::Float(a), Value::Int(b)] => Ok(Value::Float(a % *b as f64)),
            _ => Err(EvalError::argument("calc.rem expects two numbers")),
        },
        "quo" => match args.as_slice() {
            [Value::Int(a), Value::Int(b)] => {
                if *b == 0 {
                    Err(EvalError::other("division by zero"))
                } else {
                    Ok(Value::Int(a / b))
                }
            }
            _ => Err(EvalError::argument("calc.quo expects two integers")),
        },
        "sin" => match args.first() {
            Some(Value::Float(f)) => Ok(Value::Float(f.to_radians().sin())),
            Some(Value::Int(i)) => Ok(Value::Float((*i as f64).to_radians().sin())),
            Some(Value::Angle(a)) => Ok(Value::Float(a.to_radians().sin())),
            _ => Err(EvalError::argument("calc.sin expects angle or number")),
        },
        "cos" => match args.first() {
            Some(Value::Float(f)) => Ok(Value::Float(f.to_radians().cos())),
            Some(Value::Int(i)) => Ok(Value::Float((*i as f64).to_radians().cos())),
            Some(Value::Angle(a)) => Ok(Value::Float(a.to_radians().cos())),
            _ => Err(EvalError::argument("calc.cos expects angle or number")),
        },
        "tan" => match args.first() {
            Some(Value::Float(f)) => Ok(Value::Float(f.to_radians().tan())),
            Some(Value::Int(i)) => Ok(Value::Float((*i as f64).to_radians().tan())),
            Some(Value::Angle(a)) => Ok(Value::Float(a.to_radians().tan())),
            _ => Err(EvalError::argument("calc.tan expects angle or number")),
        },
        "log" => match args.as_slice() {
            [Value::Float(f)] => Ok(Value::Float(f.ln())),
            [Value::Int(i)] => Ok(Value::Float((*i as f64).ln())),
            [Value::Float(f), Value::Int(base)] => Ok(Value::Float(f.log(*base as f64))),
            [Value::Int(i), Value::Int(base)] => Ok(Value::Float((*i as f64).log(*base as f64))),
            _ => Err(EvalError::argument(
                "calc.log expects number and optional base",
            )),
        },
        _ => Err(EvalError::undefined(format!("calc.{}", name))),
    }
}

// ============================================================================
// String methods
// ============================================================================

fn call_str_method(s: &str, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "len" => Ok(Value::Int(s.chars().count() as i64)),
        "contains" => {
            let needle = args
                .first()
                .ok_or(EvalError::argument("contains expects argument"))?
                .as_str()?;
            Ok(Value::Bool(s.contains(needle)))
        }
        "starts-with" => {
            let prefix = args
                .first()
                .ok_or(EvalError::argument("starts-with expects argument"))?
                .as_str()?;
            Ok(Value::Bool(s.starts_with(prefix)))
        }
        "ends-with" => {
            let suffix = args
                .first()
                .ok_or(EvalError::argument("ends-with expects argument"))?
                .as_str()?;
            Ok(Value::Bool(s.ends_with(suffix)))
        }
        "trim" => Ok(Value::Str(s.trim().to_string())),
        "split" => {
            let sep = args
                .first()
                .ok_or(EvalError::argument("split expects separator"))?
                .as_str()?;
            let parts: Vec<Value> = s.split(sep).map(|p| Value::Str(p.to_string())).collect();
            Ok(Value::Array(parts))
        }
        "replace" => {
            let from = args
                .first()
                .ok_or(EvalError::argument("replace expects 2 arguments"))?
                .as_str()?;
            let to = args
                .get(1)
                .ok_or(EvalError::argument("replace expects 2 arguments"))?
                .as_str()?;
            Ok(Value::Str(s.replace(from, to)))
        }
        "upper" => Ok(Value::Str(s.to_uppercase())),
        "lower" => Ok(Value::Str(s.to_lowercase())),
        "first" => s
            .chars()
            .next()
            .map(|c| Value::Str(c.to_string()))
            .ok_or(EvalError::other("empty string")),
        "last" => s
            .chars()
            .last()
            .map(|c| Value::Str(c.to_string()))
            .ok_or(EvalError::other("empty string")),
        "at" => {
            let idx = args
                .first()
                .ok_or(EvalError::argument("at expects index"))?
                .as_int()? as usize;
            s.chars()
                .nth(idx)
                .map(|c| Value::Str(c.to_string()))
                .ok_or(EvalError::other(format!("index {} out of bounds", idx)))
        }
        "slice" => {
            let start = args.first().map(|v| v.as_int()).transpose()?.unwrap_or(0) as usize;
            let end = args
                .get(1)
                .map(|v| v.as_int())
                .transpose()?
                .map(|i| i as usize);
            let chars: Vec<char> = s.chars().collect();
            let end = end.unwrap_or(chars.len()).min(chars.len());
            let start = start.min(chars.len());
            Ok(Value::Str(chars[start..end].iter().collect()))
        }
        "rev" => Ok(Value::Str(s.chars().rev().collect())),
        "clusters" => {
            let clusters: Vec<Value> = s.chars().map(|c| Value::Str(c.to_string())).collect();
            Ok(Value::Array(clusters))
        }
        "codepoints" => {
            let codepoints: Vec<Value> = s.chars().map(|c| Value::Int(c as i64)).collect();
            Ok(Value::Array(codepoints))
        }
        _ => Err(EvalError::invalid_op(format!(
            "str has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Array methods
// ============================================================================

fn call_array_method(arr: &[Value], method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "len" => Ok(Value::Int(arr.len() as i64)),
        "first" => arr.first().cloned().ok_or(EvalError::other("empty array")),
        "last" => arr.last().cloned().ok_or(EvalError::other("empty array")),
        "at" => {
            let idx = args
                .first()
                .ok_or(EvalError::argument("at expects index"))?
                .as_int()?;
            let idx = if idx < 0 {
                (arr.len() as i64 + idx) as usize
            } else {
                idx as usize
            };
            arr.get(idx)
                .cloned()
                .ok_or(EvalError::other(format!("index {} out of bounds", idx)))
        }
        "slice" => {
            let start = args.first().map(|v| v.as_int()).transpose()?.unwrap_or(0) as usize;
            let count = args
                .get(1)
                .map(|v| v.as_int())
                .transpose()?
                .map(|i| i as usize);
            let end = count.map(|c| start + c).unwrap_or(arr.len()).min(arr.len());
            let start = start.min(arr.len());
            Ok(Value::Array(arr[start..end].to_vec()))
        }
        "contains" => {
            let needle = args
                .first()
                .ok_or(EvalError::argument("contains expects argument"))?;
            Ok(Value::Bool(arr.iter().any(|v| super::ops::eq(v, needle))))
        }
        "find" => {
            let needle = args
                .first()
                .ok_or(EvalError::argument("find expects argument"))?;
            Ok(arr
                .iter()
                .find(|v| super::ops::eq(v, needle))
                .cloned()
                .unwrap_or(Value::None))
        }
        "position" => {
            let needle = args
                .first()
                .ok_or(EvalError::argument("position expects argument"))?;
            Ok(arr
                .iter()
                .position(|v| super::ops::eq(v, needle))
                .map(|i| Value::Int(i as i64))
                .unwrap_or(Value::None))
        }
        "rev" => Ok(Value::Array(arr.iter().rev().cloned().collect())),
        "join" => {
            let sep = args.first().map(|v| v.display()).unwrap_or_default();
            let joined: String = arr
                .iter()
                .map(|v| v.display())
                .collect::<Vec<_>>()
                .join(&sep);
            Ok(Value::Str(joined))
        }
        "sum" => {
            let mut sum = Value::Int(0);
            for v in arr {
                sum = super::ops::add(sum, v.clone())?;
            }
            Ok(sum)
        }
        "product" => {
            let mut product = Value::Int(1);
            for v in arr {
                product = super::ops::mul(product, v.clone())?;
            }
            Ok(product)
        }
        "sorted" => {
            let mut sorted = arr.to_vec();
            sorted.sort_by(|a, b| match (a, b) {
                (Value::Int(x), Value::Int(y)) => x.cmp(y),
                (Value::Float(x), Value::Float(y)) => {
                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                }
                (Value::Str(x), Value::Str(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            });
            Ok(Value::Array(sorted))
        }
        "dedup" => {
            let mut seen = Vec::new();
            for v in arr {
                if !seen.iter().any(|s| super::ops::eq(s, v)) {
                    seen.push(v.clone());
                }
            }
            Ok(Value::Array(seen))
        }
        "flatten" => {
            let mut result = Vec::new();
            for v in arr {
                if let Value::Array(inner) = v {
                    result.extend(inner.clone());
                } else {
                    result.push(v.clone());
                }
            }
            Ok(Value::Array(result))
        }
        "intersperse" => {
            let sep = args
                .first()
                .ok_or(EvalError::argument("intersperse expects separator"))?
                .clone();
            let mut result = Vec::new();
            for (i, v) in arr.iter().enumerate() {
                if i > 0 {
                    result.push(sep.clone());
                }
                result.push(v.clone());
            }
            Ok(Value::Array(result))
        }
        "enumerate" => {
            let result: Vec<Value> = arr
                .iter()
                .enumerate()
                .map(|(i, v)| Value::Array(vec![Value::Int(i as i64), v.clone()]))
                .collect();
            Ok(Value::Array(result))
        }
        // windows(size) - returns sliding windows of given size
        "windows" => {
            let size = args
                .first()
                .ok_or(EvalError::argument("windows expects size argument"))?
                .as_int()? as usize;
            if size == 0 {
                return Err(EvalError::argument("windows size must be positive"));
            }
            if size > arr.len() {
                return Ok(Value::Array(Vec::new()));
            }
            let result: Vec<Value> = arr
                .windows(size)
                .map(|w| Value::Array(w.to_vec()))
                .collect();
            Ok(Value::Array(result))
        }
        // chunks(size) - splits array into non-overlapping chunks
        "chunks" => {
            let size = args
                .first()
                .ok_or(EvalError::argument("chunks expects size argument"))?
                .as_int()? as usize;
            if size == 0 {
                return Err(EvalError::argument("chunks size must be positive"));
            }
            let result: Vec<Value> = arr.chunks(size).map(|c| Value::Array(c.to_vec())).collect();
            Ok(Value::Array(result))
        }
        // zip(other) - zip two arrays together
        "zip" => {
            let other = args
                .first()
                .ok_or(EvalError::argument("zip expects another array"))?
                .as_array()?;
            let result: Vec<Value> = arr
                .iter()
                .zip(other.iter())
                .map(|(a, b)| Value::Array(vec![a.clone(), b.clone()]))
                .collect();
            Ok(Value::Array(result))
        }
        // split(at) - split array at index
        "split" => {
            let at = args
                .first()
                .ok_or(EvalError::argument("split expects index"))?
                .as_int()? as usize;
            let at = at.min(arr.len());
            let (left, right) = arr.split_at(at);
            Ok(Value::Array(vec![
                Value::Array(left.to_vec()),
                Value::Array(right.to_vec()),
            ]))
        }
        // any() - check if any element is truthy
        "any" => Ok(Value::Bool(arr.iter().any(|v| v.is_truthy()))),
        // all() - check if all elements are truthy
        "all" => Ok(Value::Bool(arr.iter().all(|v| v.is_truthy()))),
        _ => Err(EvalError::invalid_op(format!(
            "array has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Dictionary methods
// ============================================================================

fn call_dict_method(
    dict: &IndexMap<String, Value>,
    method: &str,
    args: Vec<Value>,
) -> EvalResult<Value> {
    match method {
        "len" => Ok(Value::Int(dict.len() as i64)),
        "at" => {
            let key = args
                .first()
                .ok_or(EvalError::argument("at expects key"))?
                .as_str()?;
            dict.get(key).cloned().ok_or(EvalError::key_not_found(key))
        }
        "keys" => {
            let keys: Vec<Value> = dict.keys().map(|k| Value::Str(k.clone())).collect();
            Ok(Value::Array(keys))
        }
        "values" => {
            let values: Vec<Value> = dict.values().cloned().collect();
            Ok(Value::Array(values))
        }
        "pairs" => {
            let pairs: Vec<Value> = dict
                .iter()
                .map(|(k, v)| Value::Array(vec![Value::Str(k.clone()), v.clone()]))
                .collect();
            Ok(Value::Array(pairs))
        }
        "contains" => {
            let key = args
                .first()
                .ok_or(EvalError::argument("contains expects key"))?
                .as_str()?;
            Ok(Value::Bool(dict.contains_key(key)))
        }
        _ => Err(EvalError::invalid_op(format!(
            "dictionary has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Numeric methods
// ============================================================================

fn call_int_method(i: i64, method: &str, _args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "signum" => Ok(Value::Int(i.signum())),
        "abs" => Ok(Value::Int(i.abs())),
        _ => Err(EvalError::invalid_op(format!(
            "int has no method '{}'",
            method
        ))),
    }
}

fn call_float_method(f: f64, method: &str, _args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "signum" => Ok(Value::Float(f.signum())),
        "abs" => Ok(Value::Float(f.abs())),
        "is-nan" => Ok(Value::Bool(f.is_nan())),
        "is-infinite" => Ok(Value::Bool(f.is_infinite())),
        _ => Err(EvalError::invalid_op(format!(
            "float has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Length methods
// ============================================================================

fn call_length_method(l: &Length, method: &str, _args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "pt" => {
            if let Length::Exact(v, LengthUnit::Pt) = l {
                Ok(Value::Float(*v))
            } else {
                Err(EvalError::other("cannot convert to pt"))
            }
        }
        "em" => {
            if let Length::Exact(v, LengthUnit::Em) = l {
                Ok(Value::Float(*v))
            } else {
                Err(EvalError::other("cannot convert to em"))
            }
        }
        _ => Err(EvalError::invalid_op(format!(
            "length has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Color methods
// ============================================================================

fn call_color_method(c: &Color, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "lighten" => {
            let factor = args
                .first()
                .ok_or(EvalError::argument("lighten expects factor"))?;
            let factor = match factor {
                Value::Ratio(r) => *r,
                Value::Float(f) => *f,
                _ => return Err(EvalError::argument("lighten expects ratio")),
            };
            Ok(Value::Color(c.lighten(factor)))
        }
        "darken" => {
            let factor = args
                .first()
                .ok_or(EvalError::argument("darken expects factor"))?;
            let factor = match factor {
                Value::Ratio(r) => *r,
                Value::Float(f) => *f,
                _ => return Err(EvalError::argument("darken expects ratio")),
            };
            Ok(Value::Color(c.darken(factor)))
        }
        _ => Err(EvalError::invalid_op(format!(
            "color has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Regex methods
// ============================================================================

fn call_regex_method(r: &WrappedRegex, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "is-match" => {
            let s = args
                .first()
                .ok_or(EvalError::argument("is-match expects string"))?
                .as_str()?;
            Ok(Value::Bool(r.0.is_match(s)))
        }
        "find" => {
            let s = args
                .first()
                .ok_or(EvalError::argument("find expects string"))?
                .as_str()?;
            Ok(r.0
                .find(s)
                .map(|m| Value::Str(m.as_str().to_string()))
                .unwrap_or(Value::None))
        }
        _ => Err(EvalError::invalid_op(format!(
            "regex has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Other type methods
// ============================================================================

fn call_arguments_method(a: &Arguments, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "pos" => Ok(Value::Array(a.positional.clone())),
        "named" => Ok(Value::Dict(a.named.clone())),
        "at" => {
            let key = args.first().ok_or(EvalError::argument("at expects key"))?;
            match key {
                Value::Int(i) => a
                    .positional
                    .get(*i as usize)
                    .cloned()
                    .ok_or(EvalError::other("index out of bounds")),
                Value::Str(s) => a
                    .named
                    .get(s)
                    .cloned()
                    .ok_or(EvalError::key_not_found(s.clone())),
                _ => Err(EvalError::argument("at expects int or string")),
            }
        }
        _ => Err(EvalError::invalid_op(format!(
            "arguments has no method '{}'",
            method
        ))),
    }
}

fn call_version_method(v: &[u32], method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "at" => {
            let idx = args
                .first()
                .map(|a| a.as_int().unwrap_or(0) as usize)
                .unwrap_or(0);
            Ok(Value::Int(v.get(idx).copied().unwrap_or(0) as i64))
        }
        _ => Err(EvalError::invalid_op(format!(
            "version has no method '{}'",
            method
        ))),
    }
}

fn call_bytes_method(b: &[u8], method: &str, _args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "len" => Ok(Value::Int(b.len() as i64)),
        _ => Err(EvalError::invalid_op(format!(
            "bytes has no method '{}'",
            method
        ))),
    }
}

fn call_datetime_method(dt: &DateTime, method: &str, _args: Vec<Value>) -> EvalResult<Value> {
    match method {
        "year" => Ok(dt
            .date
            .map(|d| Value::Int(d.year() as i64))
            .unwrap_or(Value::None)),
        "month" => Ok(dt
            .date
            .map(|d| Value::Int(d.month() as i64))
            .unwrap_or(Value::None)),
        "day" => Ok(dt
            .date
            .map(|d| Value::Int(d.day() as i64))
            .unwrap_or(Value::None)),
        "hour" => Ok(dt
            .time
            .map(|t| Value::Int(t.hour() as i64))
            .unwrap_or(Value::None)),
        "minute" => Ok(dt
            .time
            .map(|t| Value::Int(t.minute() as i64))
            .unwrap_or(Value::None)),
        "second" => Ok(dt
            .time
            .map(|t| Value::Int(t.second() as i64))
            .unwrap_or(Value::None)),
        _ => Err(EvalError::invalid_op(format!(
            "datetime has no method '{}'",
            method
        ))),
    }
}

fn call_content_method(c: &[ContentNode], method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        // Introspection: Get the element function name
        "func" => {
            if let Some(first) = c.first() {
                let name = match first {
                    ContentNode::Element { name, .. } => name.clone(),
                    ContentNode::Heading { .. } => "heading".to_string(),
                    ContentNode::Strong(_) => "strong".to_string(),
                    ContentNode::Emph(_) => "emph".to_string(),
                    ContentNode::Raw { .. } => "raw".to_string(),
                    ContentNode::Math { .. } => "math".to_string(),
                    ContentNode::ListItem(_) => "list.item".to_string(),
                    ContentNode::EnumItem { .. } => "enum.item".to_string(),
                    ContentNode::Text(_) => "text".to_string(),
                    ContentNode::Space => "space".to_string(),
                    ContentNode::Linebreak => "linebreak".to_string(),
                    ContentNode::Parbreak => "parbreak".to_string(),
                    ContentNode::Label(_) => "label".to_string(),
                    ContentNode::Citation { .. } => "cite".to_string(),
                    ContentNode::Reference { .. } => "ref".to_string(),
                    ContentNode::LabelDef(_) => "label".to_string(),
                    ContentNode::Bibliography { .. } => "bibliography".to_string(),
                    ContentNode::FuncCall { name, .. } => name.clone(),
                    ContentNode::RawSource(_) => "raw".to_string(),
                    ContentNode::State { .. } => "state".to_string(),
                    ContentNode::CounterDisplay { .. } => "counter".to_string(),
                };
                Ok(Value::Str(name))
            } else {
                Err(EvalError::other("empty content"))
            }
        }
        // Introspection: Get all fields as a dictionary
        "fields" => {
            if let Some(first) = c.first() {
                let fields = content_node_to_fields(first);
                Ok(Value::Dict(fields))
            } else {
                Ok(Value::Dict(IndexMap::new()))
            }
        }
        // Introspection: Check if a field exists
        "has" => {
            let field = args
                .first()
                .ok_or(EvalError::argument("has expects field name"))?
                .as_str()?;
            if let Some(first) = c.first() {
                let fields = content_node_to_fields(first);
                Ok(Value::Bool(fields.contains_key(field)))
            } else {
                Ok(Value::Bool(false))
            }
        }
        // Introspection: Get a field value
        "at" => {
            let field = args
                .first()
                .ok_or(EvalError::argument("at expects field name"))?
                .as_str()?;
            if let Some(first) = c.first() {
                let fields = content_node_to_fields(first);
                fields
                    .get(field)
                    .cloned()
                    .ok_or(EvalError::key_not_found(field.to_string()))
            } else {
                Err(EvalError::other("empty content"))
            }
        }
        // Get first element
        "first" => c
            .first()
            .map(|node| Value::Content(vec![node.clone()]))
            .ok_or(EvalError::other("empty content")),
        // Get last element
        "last" => c
            .last()
            .map(|node| Value::Content(vec![node.clone()]))
            .ok_or(EvalError::other("empty content")),
        // Get child nodes
        "children" => Ok(Value::Array(
            c.iter()
                .map(|node| Value::Content(vec![node.clone()]))
                .collect(),
        )),
        // Get text content
        "text" => {
            let text: String = c.iter().map(content_node_to_text).collect();
            Ok(Value::Str(text))
        }
        _ => Err(EvalError::invalid_op(format!(
            "content has no method '{}'",
            method
        ))),
    }
}

/// Convert a ContentNode to its fields dictionary for introspection.
fn content_node_to_fields(node: &ContentNode) -> IndexMap<String, Value> {
    match node {
        ContentNode::Element { fields, .. } => fields.clone(),
        ContentNode::Heading { level, content } => {
            let mut fields = IndexMap::new();
            fields.insert("level".to_string(), Value::Int(*level as i64));
            fields.insert("body".to_string(), Value::Content(content.clone()));
            fields
        }
        ContentNode::Strong(content) => {
            let mut fields = IndexMap::new();
            fields.insert("body".to_string(), Value::Content(content.clone()));
            fields
        }
        ContentNode::Emph(content) => {
            let mut fields = IndexMap::new();
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
        ContentNode::Math { content, block } => {
            let mut fields = IndexMap::new();
            fields.insert("body".to_string(), Value::Str(content.clone()));
            fields.insert("block".to_string(), Value::Bool(*block));
            fields
        }
        ContentNode::ListItem(content) => {
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
        ContentNode::Label(l) => {
            let mut fields = IndexMap::new();
            fields.insert("label".to_string(), Value::Str(l.clone()));
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
        ContentNode::LabelDef(l) => {
            let mut fields = IndexMap::new();
            fields.insert("label".to_string(), Value::Str(l.clone()));
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

/// Extract plain text from a ContentNode for text() method.
fn content_node_to_text(node: &ContentNode) -> String {
    match node {
        ContentNode::Text(t) => t.clone(),
        ContentNode::Space => " ".to_string(),
        ContentNode::Linebreak | ContentNode::Parbreak => "\n".to_string(),
        ContentNode::Strong(c) | ContentNode::Emph(c) | ContentNode::ListItem(c) => {
            c.iter().map(content_node_to_text).collect()
        }
        ContentNode::EnumItem { content, .. } => content.iter().map(content_node_to_text).collect(),
        ContentNode::Heading { content, .. } => content.iter().map(content_node_to_text).collect(),
        ContentNode::Raw { text, .. } => text.clone(),
        ContentNode::Math { content, .. } => content.clone(),
        ContentNode::Element { fields, .. } => fields
            .get("body")
            .and_then(|v| {
                if let Value::Content(c) = v {
                    Some(c.iter().map(content_node_to_text).collect::<String>())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        ContentNode::Citation { keys, .. } => keys.join(", "),
        ContentNode::Reference { target, .. } => target.clone(),
        ContentNode::LabelDef(l) => l.clone(),
        ContentNode::Bibliography { file, .. } => file.clone(),
        ContentNode::RawSource(s) => s.clone(),
        _ => String::new(),
    }
}

fn call_counter_method(c: &Counter, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        // step() increments counter by 1 (or specified level for heading counters)
        // Returns content that displays nothing but has side effect
        "step" => {
            let _level = args.first().and_then(|v| v.as_int().ok()).unwrap_or(1);
            // In static evaluation, step returns a placeholder content
            // The actual stepping happens during document layout
            Ok(Value::Content(vec![ContentNode::FuncCall {
                name: format!("counter({:?}).step", c),
                args: args.into_iter().map(Arg::Pos).collect(),
            }]))
        }
        // update() sets counter to specific value
        "update" => {
            let _value = args.first().cloned().unwrap_or(Value::Int(0));
            Ok(Value::Content(vec![ContentNode::FuncCall {
                name: format!("counter({:?}).update", c),
                args: args.into_iter().map(Arg::Pos).collect(),
            }]))
        }
        // display() formats current counter value
        "display" => {
            let numbering = args
                .first()
                .and_then(|v| v.as_str().ok().map(|s| s.to_string()))
                .unwrap_or_else(|| "1".to_string());
            // Get a string key for the counter
            let key = match c {
                Counter::Custom(s) => s.clone(),
                Counter::Label(s) => format!("label:{}", s),
                Counter::Selector(s) => format!("selector:{}", s),
                Counter::Page => "page".to_string(),
            };
            // Return a placeholder that will be replaced during layout
            Ok(Value::Content(vec![ContentNode::CounterDisplay {
                key,
                numbering,
            }]))
        }
        // get() returns current value - requires document context
        // In static eval, return array of zeros as placeholder
        "get" => {
            // Counter values are arrays (for hierarchical counters like headings)
            Ok(Value::Array(vec![Value::Int(0)]))
        }
        // at() and final() require layout - return placeholders with warning
        "at" | "final" => {
            // These truly need document context
            Ok(Value::Array(vec![Value::Int(0)]))
        }
        _ => Err(EvalError::invalid_op(format!(
            "counter has no method '{}'",
            method
        ))),
    }
}

fn call_state_method(s: &State, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        // update() sets state to new value (or applies function)
        "update" => {
            let _new_value = args.first().cloned().unwrap_or(Value::None);
            // Return content that represents a state update operation
            Ok(Value::Content(vec![ContentNode::FuncCall {
                name: format!("state({:?}).update", s.key),
                args: args.into_iter().map(Arg::Pos).collect(),
            }]))
        }
        // display() formats current state value
        "display" => {
            // If given a function, it formats the value
            // In static eval, return the initial value formatted
            Ok(Value::Content(vec![ContentNode::Text((*s.init).display())]))
        }
        // get() returns current value - use initial in static eval
        "get" => Ok((*s.init).clone()),
        // at() and final() require layout context
        "at" | "final" => Ok((*s.init).clone()),
        _ => Err(EvalError::invalid_op(format!(
            "state has no method '{}'",
            method
        ))),
    }
}

/// Call methods on function values (primarily `.where()`).
fn call_func_method(f: &Arc<Closure>, method: &str, _args: Vec<Value>) -> EvalResult<Value> {
    match method {
        // .where() creates a selector with field filters
        // Usage: heading.where(level: 1)
        "where" => {
            // The function name becomes the element name
            let element_name = f
                .name
                .clone()
                .map(|n| {
                    n.trim_start_matches("<builtin:")
                        .trim_end_matches('>')
                        .to_string()
                })
                .unwrap_or_default();

            // All arguments to .where() are named - they become filters
            // In this context, args would be named pairs, but since we receive Vec<Value>,
            // we need to handle them specially (via eval.rs call site)
            // For now, create a basic element selector
            Ok(Value::Selector(Selector::element(element_name)))
        }
        "with" => {
            // .with() creates a partial application
            // For now, just return the function unchanged
            Ok(Value::Func(f.clone()))
        }
        _ => Err(EvalError::invalid_op(format!(
            "function has no method '{}'",
            method
        ))),
    }
}

/// Call methods on selectors (or, and, before, after).
fn call_selector_method(s: &Selector, method: &str, args: Vec<Value>) -> EvalResult<Value> {
    match method {
        // Union: selector.or(other)
        "or" => {
            let other = args
                .first()
                .ok_or(EvalError::argument("or expects a selector"))?;
            if let Value::Selector(other_sel) = other {
                Ok(Value::Selector(Selector::Or(
                    Box::new(s.clone()),
                    Box::new(other_sel.clone()),
                )))
            } else {
                Err(EvalError::type_mismatch("selector", other.type_name()))
            }
        }
        // Intersection: selector.and(other)
        "and" => {
            let other = args
                .first()
                .ok_or(EvalError::argument("and expects a selector"))?;
            if let Value::Selector(other_sel) = other {
                Ok(Value::Selector(Selector::And(
                    Box::new(s.clone()),
                    Box::new(other_sel.clone()),
                )))
            } else {
                Err(EvalError::type_mismatch("selector", other.type_name()))
            }
        }
        // Context selector: selector.before(other)
        "before" => {
            let other = args
                .first()
                .ok_or(EvalError::argument("before expects a selector"))?;
            if let Value::Selector(other_sel) = other {
                Ok(Value::Selector(Selector::Before(
                    Box::new(s.clone()),
                    Box::new(other_sel.clone()),
                )))
            } else {
                Err(EvalError::type_mismatch("selector", other.type_name()))
            }
        }
        // Context selector: selector.after(other)
        "after" => {
            let other = args
                .first()
                .ok_or(EvalError::argument("after expects a selector"))?;
            if let Value::Selector(other_sel) = other {
                Ok(Value::Selector(Selector::After(
                    Box::new(s.clone()),
                    Box::new(other_sel.clone()),
                )))
            } else {
                Err(EvalError::type_mismatch("selector", other.type_name()))
            }
        }
        _ => Err(EvalError::invalid_op(format!(
            "selector has no method '{}'",
            method
        ))),
    }
}

// ============================================================================
// Layout Functions
// ============================================================================

/// Generic layout function that preserves its name and arguments as a ContentNode::FuncCall.
/// This allows the LaTeX generator to properly translate these to LaTeX equivalents.
fn builtin_layout_func(
    name: &str,
    args: Vec<Value>,
    named: IndexMap<String, Value>,
) -> EvalResult<Value> {
    let mut func_args: Vec<Arg> = args.into_iter().map(Arg::Pos).collect();
    func_args.extend(named.into_iter().map(|(k, v)| Arg::Named(k, v)));

    Ok(Value::Content(vec![ContentNode::FuncCall {
        name: name.to_string(),
        args: func_args,
    }]))
}

/// Image function - loads and returns an image reference.
/// The actual image loading happens at a higher level; we just preserve the path and options.
fn builtin_image(args: Vec<Value>, named: IndexMap<String, Value>) -> EvalResult<Value> {
    let path = args
        .first()
        .ok_or(EvalError::argument("image expects path argument"))?
        .as_str()?
        .to_string();

    // Preserve all the arguments for the LaTeX generator
    let mut func_args: Vec<Arg> = vec![Arg::Pos(Value::Str(path))];
    func_args.extend(named.into_iter().map(|(k, v)| Arg::Named(k, v)));

    Ok(Value::Content(vec![ContentNode::FuncCall {
        name: "image".to_string(),
        args: func_args,
    }]))
}

/// measure() - Returns the dimensions of content.
/// In static evaluation, returns mock dimensions since actual measurement requires layout.
///
/// # Typst signature: measure(content, styles?) -> dictionary
/// Returns: { width: length, height: length }
fn builtin_measure(args: Vec<Value>) -> EvalResult<Value> {
    // In static evaluation we cannot actually measure content
    // Return a reasonable default and the caller should be aware this is approximate
    let _content = args.first().cloned().unwrap_or(Value::None);

    // Return a mock measurement dictionary with zero dimensions
    // Real measurements require document layout context
    let mut result = IndexMap::new();
    result.insert(
        "width".to_string(),
        Value::Length(Length::exact(0.0, LengthUnit::Pt)),
    );
    result.insert(
        "height".to_string(),
        Value::Length(Length::exact(0.0, LengthUnit::Pt)),
    );

    Ok(Value::Dict(result))
}

/// layout() - Access layout context information.
/// In static evaluation, returns a mock context since actual layout requires document processing.
///
/// # Typst signature: layout(func) -> content
/// The function receives a context with width, height, region info
fn builtin_layout(args: Vec<Value>) -> EvalResult<Value> {
    // layout(func) takes a function that receives layout context
    // In static evaluation, we call the function with mock context
    let func = args.first().cloned().unwrap_or(Value::None);

    if let Value::Func(closure) = func {
        // Create a mock layout context
        let mut context = IndexMap::new();
        context.insert(
            "width".to_string(),
            Value::Length(Length::exact(595.0, LengthUnit::Pt)),
        ); // A4 width
        context.insert(
            "height".to_string(),
            Value::Length(Length::exact(842.0, LengthUnit::Pt)),
        ); // A4 height

        let mut region = IndexMap::new();
        region.insert(
            "width".to_string(),
            Value::Length(Length::exact(595.0, LengthUnit::Pt)),
        );
        region.insert(
            "height".to_string(),
            Value::Length(Length::exact(842.0, LengthUnit::Pt)),
        );
        context.insert("region".to_string(), Value::Dict(region));

        // Return content that represents calling the closure with this context
        // The actual call should happen in the evaluator
        Ok(Value::Content(vec![ContentNode::FuncCall {
            name: "layout".to_string(),
            args: vec![
                Arg::Pos(Value::Func(closure)),
                Arg::Named("context".to_string(), Value::Dict(context)),
            ],
        }]))
    } else {
        Err(EvalError::type_mismatch("function", func.type_name()))
    }
}
