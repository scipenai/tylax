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

/// Maximum nesting depth accepted by the fallback JSON parser.
#[cfg(not(feature = "data-loading"))]
const MAX_JSON_DEPTH: usize = 128;

#[cfg(not(feature = "data-loading"))]
struct JsonParser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

#[cfg(not(feature = "data-loading"))]
impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn consume_literal(&mut self, lit: &[u8]) -> bool {
        if self.bytes.get(self.pos..self.pos + lit.len()) == Some(lit) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn parse_hex4(&mut self) -> EvalResult<u16> {
        let end = self.pos.saturating_add(4);
        let Some(hex) = self.input.get(self.pos..end) else {
            return Err(EvalError::other("Incomplete JSON unicode escape"));
        };
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(EvalError::other(format!(
                "Invalid JSON unicode escape: \\u{hex}"
            )));
        }
        self.pos = end;
        u16::from_str_radix(hex, 16)
            .map_err(|_| EvalError::other(format!("Invalid JSON unicode escape: \\u{hex}")))
    }

    fn parse_string(&mut self) -> EvalResult<String> {
        if self.peek() != Some(b'"') {
            return Err(EvalError::other("Expected opening quote in JSON string"));
        }
        self.pos += 1;
        let mut out = String::new();

        while let Some(ch) = self.input[self.pos..].chars().next() {
            self.pos += ch.len_utf8();
            match ch {
                '"' => return Ok(out),
                c if (c as u32) < 0x20 => {
                    return Err(EvalError::other(
                        "Unescaped control character in JSON string",
                    ));
                }
                '\\' => {
                    let Some(esc) = self.input[self.pos..].chars().next() else {
                        return Err(EvalError::other("Unterminated JSON escape sequence"));
                    };
                    self.pos += esc.len_utf8();
                    match esc {
                        '"' | '\\' | '/' => out.push(esc),
                        'b' => out.push('\u{0008}'),
                        'f' => out.push('\u{000C}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => {
                            let hi = self.parse_hex4()?;
                            if (0xD800..=0xDBFF).contains(&hi) {
                                if !self.consume_literal(b"\\u") {
                                    return Err(EvalError::other(
                                        "Missing low surrogate in JSON unicode escape",
                                    ));
                                }
                                let lo = self.parse_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&lo) {
                                    return Err(EvalError::other(
                                        "Invalid low surrogate in JSON unicode escape",
                                    ));
                                }
                                let cp = 0x10000
                                    + (((hi - 0xD800) as u32) << 10)
                                    + ((lo - 0xDC00) as u32);
                                let decoded = char::from_u32(cp).ok_or_else(|| {
                                    EvalError::other("Invalid JSON surrogate pair")
                                })?;
                                out.push(decoded);
                            } else if (0xDC00..=0xDFFF).contains(&hi) {
                                return Err(EvalError::other(
                                    "Unpaired low surrogate in JSON unicode escape",
                                ));
                            } else {
                                let decoded = char::from_u32(hi as u32).ok_or_else(|| {
                                    EvalError::other("Invalid JSON unicode scalar")
                                })?;
                                out.push(decoded);
                            }
                        }
                        other => {
                            return Err(EvalError::other(format!(
                                "Invalid JSON escape character: \\{other}"
                            )));
                        }
                    }
                }
                other => out.push(other),
            }
        }

        Err(EvalError::other("Unterminated JSON string"))
    }

    fn parse_number(&mut self) -> EvalResult<Value> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err(EvalError::other("Leading zero in JSON number"));
                }
            }
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => {
                return Err(EvalError::other(format!(
                    "Invalid JSON number near byte {}",
                    self.pos
                )));
            }
        }

        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(EvalError::other(
                    "Expected fractional digits in JSON number",
                ));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(EvalError::other("Expected exponent digits in JSON number"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }

        let num_str = &self.input[start..self.pos];
        if !is_float {
            if let Ok(i) = num_str.parse::<i64>() {
                return Ok(Value::Int(i));
            }
        }
        let f: f64 = num_str
            .parse()
            .map_err(|_| EvalError::other(format!("Invalid JSON number: {num_str}")))?;
        if !f.is_finite() {
            return Err(EvalError::other(format!(
                "Out-of-range JSON number: {num_str}"
            )));
        }
        Ok(Value::Float(f))
    }

    fn parse_array(&mut self, depth: usize) -> EvalResult<Value> {
        self.pos += 1; // '['
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(Vec::new()));
        }

        let mut arr = Vec::new();
        loop {
            arr.push(self.parse_value(depth)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(arr));
                }
                _ => {
                    return Err(EvalError::other(format!(
                        "Expected ',' or ']' in JSON array near byte {}",
                        self.pos
                    )));
                }
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> EvalResult<Value> {
        self.pos += 1; // '{'
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Dict(IndexMap::new()));
        }

        let mut dict = IndexMap::new();
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(EvalError::other(format!(
                    "Expected string key in JSON object near byte {}",
                    self.pos
                )));
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(EvalError::other(format!(
                    "Expected ':' after JSON key near byte {}",
                    self.pos
                )));
            }
            self.pos += 1;
            let value = self.parse_value(depth)?;
            dict.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Dict(dict));
                }
                _ => {
                    return Err(EvalError::other(format!(
                        "Expected ',' or '}}' in JSON object near byte {}",
                        self.pos
                    )));
                }
            }
        }
    }

    fn parse_value(&mut self, depth: usize) -> EvalResult<Value> {
        if depth > MAX_JSON_DEPTH {
            return Err(EvalError::other("JSON nesting depth limit exceeded"));
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'n') => {
                if self.consume_literal(b"null") {
                    Ok(Value::None)
                } else {
                    Err(EvalError::other("Invalid JSON literal"))
                }
            }
            Some(b't') => {
                if self.consume_literal(b"true") {
                    Ok(Value::Bool(true))
                } else {
                    Err(EvalError::other("Invalid JSON literal"))
                }
            }
            Some(b'f') => {
                if self.consume_literal(b"false") {
                    Ok(Value::Bool(false))
                } else {
                    Err(EvalError::other("Invalid JSON literal"))
                }
            }
            Some(b'"') => self.parse_string().map(Value::Str),
            Some(b'[') => self.parse_array(depth + 1),
            Some(b'{') => self.parse_object(depth + 1),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(ch) => Err(EvalError::other(format!(
                "Unexpected character '{}' in JSON at byte {}",
                ch as char, self.pos
            ))),
            None => Err(EvalError::other("Unexpected end of JSON input")),
        }
    }
}

/// RFC 8259 JSON parser for fallback builds without external dependencies.
#[cfg(not(feature = "data-loading"))]
fn parse_json_simple(input: &str) -> EvalResult<Value> {
    let mut parser = JsonParser::new(input);
    let value = parser.parse_value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(EvalError::other(format!(
            "Trailing JSON content at byte {}",
            parser.pos
        )));
    }
    Ok(value)
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

    #[test]
    fn test_parse_json_malformed_inputs_return_err_without_panic() {
        for bad in ["\"", "\"\\", "{\": 1}", "{\"a\": \"}", "", "{", "["] {
            assert!(
                parse_json(bad).is_err(),
                "expected Err for malformed JSON `{bad}`"
            );
        }
    }

    #[test]
    fn test_parse_json_rfc8259_escapes_colons_and_unicode() {
        let obj = parse_json(r#"{"a:b":"VALUE", "c,d": [1, -2.5, 1e3, true, false, null]}"#)
            .expect("valid JSON with colon and comma inside key");
        let Value::Dict(dict) = obj else {
            panic!("expected dict, got {obj:?}");
        };
        assert_eq!(dict.get("a:b"), Some(&Value::Str("VALUE".to_string())));
        let Some(Value::Array(arr)) = dict.get("c,d") else {
            panic!("expected array for c,d");
        };
        assert_eq!(arr[0], Value::Int(1));
        assert_eq!(arr[1], Value::Float(-2.5));
        assert_eq!(arr[2], Value::Float(1000.0));
        assert_eq!(arr[3], Value::Bool(true));
        assert_eq!(arr[4], Value::Bool(false));
        assert_eq!(arr[5], Value::None);

        let slash = parse_json(r#""a\\b""#).expect("escaped backslash JSON string");
        let Value::Str(slash_str) = slash else {
            panic!("expected string");
        };
        assert_eq!(slash_str, "a\\b");
        assert_eq!(slash_str.chars().count(), 3);

        let literal_newline = parse_json(r#""\\n""#).expect("escaped \\n literal");
        assert_eq!(literal_newline, Value::Str("\\n".to_string()));

        let unicode = parse_json(r#""\u0041\uD83D\uDE00""#).expect("unicode surrogate pair");
        assert_eq!(unicode, Value::Str("A😀".to_string()));

        for invalid in ["1 2", "[1,]", "{\"a\":1,}", "\"\n\"", r#""\uD800""#] {
            assert!(
                parse_json(invalid).is_err(),
                "expected Err for invalid JSON `{invalid}`"
            );
        }
    }
}
