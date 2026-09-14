//! Data loading functions for MiniEval.
//!
//! This module provides functions for loading data from various formats:
//! - JSON
//! - YAML
//! - TOML
//! - CSV
//!
//! These are equivalent to Typst's built-in data loading functions.

use indexmap::IndexMap;

use super::value::{EvalError, EvalResult, Value};

/// Parse JSON string into Value.
pub fn parse_json(input: &str) -> EvalResult<Value> {
    #[cfg(feature = "data-loading")]
    {
        use serde_json::Value as JsonValue;
        let json: JsonValue = serde_json::from_str(input)
            .map_err(|e| EvalError::other(format!("JSON parse error: {}", e)))?;
        json_to_value(json)
    }

    #[cfg(not(feature = "data-loading"))]
    {
        // Fallback: simple JSON parser for basic cases
        parse_json_simple(input)
    }
}

/// Parse YAML string into Value.
pub fn parse_yaml(input: &str) -> EvalResult<Value> {
    #[cfg(feature = "data-loading")]
    {
        use serde_yaml::Value as YamlValue;
        let yaml: YamlValue = serde_yaml::from_str(input)
            .map_err(|e| EvalError::other(format!("YAML parse error: {}", e)))?;
        yaml_to_value(yaml)
    }

    #[cfg(not(feature = "data-loading"))]
    {
        let _ = input; // Suppress unused variable warning
        Err(EvalError::other(
            "YAML parsing requires 'data-loading' feature".to_string(),
        ))
    }
}

/// Parse TOML string into Value.
pub fn parse_toml(input: &str) -> EvalResult<Value> {
    #[cfg(feature = "data-loading")]
    {
        let toml_value: toml::Value = input
            .parse()
            .map_err(|e| EvalError::other(format!("TOML parse error: {}", e)))?;
        toml_to_value(toml_value)
    }

    #[cfg(not(feature = "data-loading"))]
    {
        let _ = input; // Suppress unused variable warning
        Err(EvalError::other(
            "TOML parsing requires 'data-loading' feature".to_string(),
        ))
    }
}

/// Parse CSV string into array of arrays/dicts.
pub fn parse_csv(input: &str, has_header: bool) -> EvalResult<Value> {
    #[cfg(feature = "data-loading")]
    {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(has_header)
            .from_reader(input.as_bytes());

        if has_header {
            let headers: Vec<String> = reader
                .headers()
                .map_err(|e| EvalError::other(format!("CSV header error: {}", e)))?
                .iter()
                .map(|s| s.to_string())
                .collect();

            let mut rows = Vec::new();
            for result in reader.records() {
                let record =
                    result.map_err(|e| EvalError::other(format!("CSV record error: {}", e)))?;
                let mut row = IndexMap::new();
                for (i, value) in record.iter().enumerate() {
                    let key = headers.get(i).cloned().unwrap_or_else(|| i.to_string());
                    row.insert(key, Value::Str(value.to_string()));
                }
                rows.push(Value::Dict(row));
            }
            Ok(Value::Array(rows))
        } else {
            let mut rows = Vec::new();
            for result in reader.records() {
                let record =
                    result.map_err(|e| EvalError::other(format!("CSV record error: {}", e)))?;
                let row: Vec<Value> = record.iter().map(|s| Value::Str(s.to_string())).collect();
                rows.push(Value::Array(row));
            }
            Ok(Value::Array(rows))
        }
    }

    #[cfg(not(feature = "data-loading"))]
    {
        // Simple CSV parser for basic cases
        parse_csv_simple(input, has_header)
    }
}

// ============================================================================
// Conversion helpers for data-loading feature
// ============================================================================

#[cfg(feature = "data-loading")]
fn json_to_value(json: serde_json::Value) -> EvalResult<Value> {
    use serde_json::Value as JsonValue;
    Ok(match json {
        JsonValue::Null => Value::None,
        JsonValue::Bool(b) => Value::Bool(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Str(n.to_string())
            }
        }
        JsonValue::String(s) => Value::Str(s),
        JsonValue::Array(arr) => {
            let values: Result<Vec<_>, _> = arr.into_iter().map(json_to_value).collect();
            Value::Array(values?)
        }
        JsonValue::Object(obj) => {
            let mut dict = IndexMap::new();
            for (k, v) in obj {
                dict.insert(k, json_to_value(v)?);
            }
            Value::Dict(dict)
        }
    })
}

#[cfg(feature = "data-loading")]
fn yaml_to_value(yaml: serde_yaml::Value) -> EvalResult<Value> {
    use serde_yaml::Value as YamlValue;
    Ok(match yaml {
        YamlValue::Null => Value::None,
        YamlValue::Bool(b) => Value::Bool(b),
        YamlValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Str(n.to_string())
            }
        }
        YamlValue::String(s) => Value::Str(s),
        YamlValue::Sequence(arr) => {
            let values: Result<Vec<_>, _> = arr.into_iter().map(yaml_to_value).collect();
            Value::Array(values?)
        }
        YamlValue::Mapping(obj) => {
            let mut dict = IndexMap::new();
            for (k, v) in obj {
                let key = match k {
                    YamlValue::String(s) => s,
                    other => yaml_to_value(other)?.display(),
                };
                dict.insert(key, yaml_to_value(v)?);
            }
            Value::Dict(dict)
        }
        YamlValue::Tagged(tagged) => yaml_to_value(tagged.value)?,
    })
}

#[cfg(feature = "data-loading")]
fn toml_to_value(toml_val: toml::Value) -> EvalResult<Value> {
    use toml::Value as TomlValue;
    Ok(match toml_val {
        TomlValue::String(s) => Value::Str(s),
        TomlValue::Integer(i) => Value::Int(i),
        TomlValue::Float(f) => Value::Float(f),
        TomlValue::Boolean(b) => Value::Bool(b),
        TomlValue::Datetime(dt) => Value::Str(dt.to_string()),
        TomlValue::Array(arr) => {
            let values: Result<Vec<_>, _> = arr.into_iter().map(toml_to_value).collect();
            Value::Array(values?)
        }
        TomlValue::Table(obj) => {
            let mut dict = IndexMap::new();
            for (k, v) in obj {
                dict.insert(k, toml_to_value(v)?);
            }
            Value::Dict(dict)
        }
    })
}

// ============================================================================
// Fallback simple parsers (no external dependencies)
// ============================================================================

/// Simple JSON parser for basic cases (no external dependencies).
#[cfg(not(feature = "data-loading"))]
fn parse_json_simple(input: &str) -> EvalResult<Value> {
    let input = input.trim();

    if input == "null" {
        return Ok(Value::None);
    }
    if input == "true" {
        return Ok(Value::Bool(true));
    }
    if input == "false" {
        return Ok(Value::Bool(false));
    }

    // Try parsing as number
    if let Ok(i) = input.parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = input.parse::<f64>() {
        return Ok(Value::Float(f));
    }

    // Try parsing as string
    if input.starts_with('"') && input.ends_with('"') {
        let s = &input[1..input.len() - 1];
        // Basic unescape
        let s = s
            .replace("\\\"", "\"")
            .replace("\\n", "\n")
            .replace("\\t", "\t");
        return Ok(Value::Str(s));
    }

    // Try parsing as array
    if input.starts_with('[') && input.ends_with(']') {
        let inner = &input[1..input.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }
        // Simple comma splitting (doesn't handle nested structures well)
        let parts = split_json_elements(inner);
        let values: Result<Vec<_>, _> = parts.iter().map(|s| parse_json_simple(s)).collect();
        return Ok(Value::Array(values?));
    }

    // Try parsing as object
    if input.starts_with('{') && input.ends_with('}') {
        let inner = &input[1..input.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Value::Dict(IndexMap::new()));
        }
        let mut dict = IndexMap::new();
        let pairs = split_json_elements(inner);
        for pair in pairs {
            let parts: Vec<&str> = pair.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(EvalError::other(format!("Invalid JSON pair: {}", pair)));
            }
            let key = parts[0].trim();
            let value = parts[1].trim();
            // Parse key (remove quotes)
            let key = if key.starts_with('"') && key.ends_with('"') {
                key[1..key.len() - 1].to_string()
            } else {
                key.to_string()
            };
            dict.insert(key, parse_json_simple(value)?);
        }
        return Ok(Value::Dict(dict));
    }

    Err(EvalError::other(format!("Cannot parse JSON: {}", input)))
}

/// Split JSON elements by comma, respecting nesting.
#[cfg(not(feature = "data-loading"))]
fn split_json_elements(input: &str) -> Vec<String> {
    let mut elements = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for c in input.chars() {
        if escape {
            current.push(c);
            escape = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                current.push(c);
                escape = true;
            }
            '"' => {
                current.push(c);
                in_string = !in_string;
            }
            '[' | '{' if !in_string => {
                current.push(c);
                depth += 1;
            }
            ']' | '}' if !in_string => {
                current.push(c);
                depth -= 1;
            }
            ',' if !in_string && depth == 0 => {
                elements.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }

    if !current.trim().is_empty() {
        elements.push(current.trim().to_string());
    }

    elements
}

/// Simple CSV parser for basic cases (no external dependencies).
/// Used as fallback when `data-loading` feature is not enabled.
#[cfg(not(feature = "data-loading"))]
fn parse_csv_simple(input: &str, has_header: bool) -> EvalResult<Value> {
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }

    let parse_row = |line: &str| -> Vec<String> {
        // Simple CSV parsing (doesn't handle quoted fields with commas)
        line.split(',').map(|s| s.trim().to_string()).collect()
    };

    if has_header {
        let headers = parse_row(lines[0]);
        let mut rows = Vec::new();
        for line in lines.iter().skip(1) {
            if line.is_empty() {
                continue;
            }
            let values = parse_row(line);
            let mut row = IndexMap::new();
            for (i, value) in values.into_iter().enumerate() {
                let key = headers.get(i).cloned().unwrap_or_else(|| i.to_string());
                row.insert(key, Value::Str(value));
            }
            rows.push(Value::Dict(row));
        }
        Ok(Value::Array(rows))
    } else {
        let mut rows = Vec::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            let values: Vec<Value> = parse_row(line).into_iter().map(Value::Str).collect();
            rows.push(Value::Array(values));
        }
        Ok(Value::Array(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // `3.14` below is JSON test data, not an approximation of PI.
    #[allow(clippy::approx_constant)]
    fn test_parse_json_simple_primitives() {
        // Use public parse_json API which works with or without data-loading feature
        assert_eq!(parse_json("null").unwrap(), Value::None);
        assert_eq!(parse_json("true").unwrap(), Value::Bool(true));
        assert_eq!(parse_json("false").unwrap(), Value::Bool(false));
        assert_eq!(parse_json("42").unwrap(), Value::Int(42));
        assert_eq!(parse_json("3.14").unwrap(), Value::Float(3.14));
        assert_eq!(
            parse_json("\"hello\"").unwrap(),
            Value::Str("hello".to_string())
        );
    }

    #[test]
    fn test_parse_json_simple_array() {
        let result = parse_json("[1, 2, 3]").unwrap();
        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Int(1));
            assert_eq!(arr[1], Value::Int(2));
            assert_eq!(arr[2], Value::Int(3));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_parse_json_simple_object() {
        let result = parse_json("{\"name\": \"test\", \"value\": 42}").unwrap();
        if let Value::Dict(dict) = result {
            assert_eq!(dict.get("name"), Some(&Value::Str("test".to_string())));
            assert_eq!(dict.get("value"), Some(&Value::Int(42)));
        } else {
            panic!("Expected dict");
        }
    }

    #[test]
    fn test_parse_csv_simple() {
        let csv = "name,age\nAlice,30\nBob,25";
        let result = parse_csv(csv, true).unwrap();
        if let Value::Array(rows) = result {
            assert_eq!(rows.len(), 2);
            if let Value::Dict(ref row) = rows[0] {
                assert_eq!(row.get("name"), Some(&Value::Str("Alice".to_string())));
                assert_eq!(row.get("age"), Some(&Value::Str("30".to_string())));
            }
        } else {
            panic!("Expected array");
        }
    }
}
