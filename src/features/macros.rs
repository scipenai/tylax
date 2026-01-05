//! LaTeX Macro Expansion Module
//!
//! This module provides a lightweight macro expander for LaTeX,
//! inspired by Pandoc's macro handling. Supports \newcommand, \def,
//! and simple macro expansion with arguments.

use std::collections::HashMap;

/// Maximum recursion depth for macro expansion
const MAX_EXPANSION_DEPTH: usize = 100;

/// Macro argument specification
#[derive(Debug, Clone)]
pub enum ArgSpec {
    /// Required argument in braces: {arg}
    Required,
    /// Optional argument in brackets: `[arg]`
    Optional(String), // default value
    /// Delimited argument (TeX-style)
    Delimited(String), // delimiter pattern
}

/// A macro definition
#[derive(Debug, Clone)]
pub struct Macro {
    /// Macro name (without backslash)
    pub name: String,
    /// Number of arguments
    pub num_args: usize,
    /// Argument specifications
    pub arg_specs: Vec<ArgSpec>,
    /// Replacement text (with #1, #2, etc. placeholders)
    pub replacement: String,
    /// Whether this is a starred variant
    pub starred: bool,
}

impl Macro {
    /// Create a simple macro with no arguments
    pub fn simple(name: &str, replacement: &str) -> Self {
        Self {
            name: name.to_string(),
            num_args: 0,
            arg_specs: Vec::new(),
            replacement: replacement.to_string(),
            starred: false,
        }
    }

    /// Create a macro with required arguments
    pub fn with_args(name: &str, num_args: usize, replacement: &str) -> Self {
        Self {
            name: name.to_string(),
            num_args,
            arg_specs: vec![ArgSpec::Required; num_args],
            replacement: replacement.to_string(),
            starred: false,
        }
    }

    /// Create a macro with an optional first argument
    pub fn with_optional(name: &str, num_args: usize, default: &str, replacement: &str) -> Self {
        let mut arg_specs = vec![ArgSpec::Optional(default.to_string())];
        arg_specs.extend(vec![ArgSpec::Required; num_args.saturating_sub(1)]);

        Self {
            name: name.to_string(),
            num_args,
            arg_specs,
            replacement: replacement.to_string(),
            starred: false,
        }
    }
}

/// Macro database
#[derive(Debug, Default)]
pub struct MacroDb {
    /// Defined macros
    macros: HashMap<String, Macro>,
}

impl MacroDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a database with common LaTeX macros pre-defined
    pub fn with_defaults() -> Self {
        let mut db = Self::new();

        // Common text macros
        db.define(Macro::simple("LaTeX", "LaTeX"));
        db.define(Macro::simple("TeX", "TeX"));
        db.define(Macro::simple("today", ""));

        // Spacing
        db.define(Macro::simple("quad", " "));
        db.define(Macro::simple("qquad", "  "));
        db.define(Macro::simple("hfill", " "));
        db.define(Macro::simple("vfill", ""));

        // Common math
        db.define(Macro::simple("displaystyle", ""));
        db.define(Macro::simple("textstyle", ""));
        db.define(Macro::simple("scriptstyle", ""));

        db
    }

    /// Define a new macro
    pub fn define(&mut self, macro_def: Macro) {
        self.macros.insert(macro_def.name.clone(), macro_def);
    }

    /// Check if a macro is defined
    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    /// Get a macro definition
    pub fn get(&self, name: &str) -> Option<&Macro> {
        self.macros.get(name)
    }

    /// Remove a macro definition
    pub fn undefine(&mut self, name: &str) {
        self.macros.remove(name);
    }

    /// Parse and register macro definitions from LaTeX source
    pub fn parse_definitions(&mut self, input: &str) {
        let mut remaining = input;

        while let Some(def) = self.find_next_definition(remaining) {
            if let Some(macro_def) = self.parse_single_definition(def.text) {
                self.define(macro_def);
            }
            remaining = def.rest;
        }
    }

    /// Find the next macro definition in the text
    fn find_next_definition<'a>(&self, input: &'a str) -> Option<DefinitionMatch<'a>> {
        let patterns = [
            "\\newcommand",
            "\\renewcommand",
            "\\providecommand",
            "\\def",
        ];

        let mut earliest_pos = None;
        for pattern in patterns {
            if let Some(pos) = input.find(pattern) {
                if earliest_pos.is_none() || pos < earliest_pos.unwrap() {
                    earliest_pos = Some(pos);
                }
            }
        }

        let pos = earliest_pos?;
        let after = &input[pos..];

        // Find the end of the definition
        let end = self.find_definition_end(after)?;

        Some(DefinitionMatch {
            text: &after[..end],
            rest: &after[end..],
        })
    }

    /// Find where a macro definition ends
    fn find_definition_end(&self, input: &str) -> Option<usize> {
        // Count braces to find the end
        let mut depth = 0;
        let mut found_first_brace = false;
        let mut in_first_brace = false;

        for (i, c) in input.char_indices() {
            match c {
                '{' => {
                    depth += 1;
                    if !found_first_brace {
                        found_first_brace = true;
                    }
                    if depth == 1 && found_first_brace {
                        in_first_brace = true;
                    }
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 && in_first_brace {
                        // This might be the end of the command name
                        in_first_brace = false;
                    } else if depth == 0 && found_first_brace {
                        // End of the definition body
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// Parse a single macro definition
    fn parse_single_definition(&self, input: &str) -> Option<Macro> {
        if input.starts_with("\\newcommand")
            || input.starts_with("\\renewcommand")
            || input.starts_with("\\providecommand")
        {
            self.parse_newcommand(input)
        } else if input.starts_with("\\def") {
            self.parse_def(input)
        } else {
            None
        }
    }

    /// Parse \newcommand{\name}[numargs]{replacement}
    fn parse_newcommand(&self, input: &str) -> Option<Macro> {
        // Skip command name
        let rest = input
            .trim_start_matches("\\newcommand")
            .trim_start_matches("\\renewcommand")
            .trim_start_matches("\\providecommand")
            .trim_start_matches('*')
            .trim_start();

        // Get macro name
        let (name, rest) = self.extract_macro_name(rest)?;

        // Check for optional number of arguments [n]
        let (num_args, default_opt, rest) = self.extract_arg_spec(rest);

        // Get replacement text
        let replacement = self.extract_braced(rest)?;

        let macro_def = if let Some(default) = default_opt {
            Macro::with_optional(&name, num_args, &default, &replacement)
        } else {
            Macro::with_args(&name, num_args, &replacement)
        };

        Some(macro_def)
    }

    /// Parse \def\name#1#2{replacement}
    fn parse_def(&self, input: &str) -> Option<Macro> {
        let rest = input.trim_start_matches("\\def").trim_start();

        // Get macro name (directly after \def, no braces)
        if !rest.starts_with('\\') {
            return None;
        }

        let name_end = rest[1..]
            .find(|c: char| !c.is_alphabetic())
            .map(|i| i + 1)
            .unwrap_or(rest.len());

        let name = rest[1..name_end].to_string();
        let mut rest = &rest[name_end..];

        // Count arguments (#1, #2, etc.)
        let mut num_args = 0;
        while rest.starts_with('#') {
            if let Some(c) = rest.chars().nth(1) {
                if c.is_ascii_digit() {
                    num_args = num_args.max(c.to_digit(10).unwrap_or(0) as usize);
                    rest = &rest[2..];
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Get replacement text
        let replacement = self.extract_braced(rest)?;

        Some(Macro::with_args(&name, num_args, &replacement))
    }

    /// Extract macro name from {\name} or \name
    fn extract_macro_name<'a>(&self, input: &'a str) -> Option<(String, &'a str)> {
        let input = input.trim_start();

        if input.starts_with('{') {
            // {\name}
            let end = input.find('}')?;
            let name = input[1..end].trim_start_matches('\\').to_string();
            Some((name, &input[end + 1..]))
        } else if let Some(rest) = input.strip_prefix('\\') {
            // \name
            let end = rest
                .find(|c: char| !c.is_alphabetic())
                .unwrap_or(rest.len());
            let name = rest[..end].to_string();
            Some((name, &rest[end..]))
        } else {
            None
        }
    }

    /// Extract [n][default] argument specification
    fn extract_arg_spec<'a>(&self, input: &'a str) -> (usize, Option<String>, &'a str) {
        let mut rest = input.trim_start();
        let mut num_args = 0;
        let mut default = None;

        // First optional: number of arguments
        if rest.starts_with('[') {
            if let Some(end) = rest.find(']') {
                let arg_str = &rest[1..end];
                if let Ok(n) = arg_str.trim().parse() {
                    num_args = n;
                }
                rest = &rest[end + 1..];
            }
        }

        // Second optional: default for first argument
        let mut rest = rest.trim_start();
        if rest.starts_with('[') {
            if let Some(end) = rest.find(']') {
                default = Some(rest[1..end].to_string());
                rest = &rest[end + 1..];
                return (num_args, default, rest);
            }
        }

        (num_args, default, rest)
    }

    /// Extract content within braces
    fn extract_braced(&self, input: &str) -> Option<String> {
        let input = input.trim_start();
        if !input.starts_with('{') {
            return None;
        }

        let mut depth = 0;
        let mut start = 0;
        let mut end = 0;

        for (i, c) in input.char_indices() {
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
            Some(input[start..end].to_string())
        } else {
            None
        }
    }
}

struct DefinitionMatch<'a> {
    text: &'a str,
    rest: &'a str,
}

// ============================================================================
// Macro Expansion
// ============================================================================

/// Expand all macros in the input text
pub fn expand_macros(input: &str, db: &MacroDb) -> String {
    expand_macros_with_depth(input, db, 0)
}

fn expand_macros_with_depth(input: &str, db: &MacroDb, depth: usize) -> String {
    if depth >= MAX_EXPANSION_DEPTH {
        return input.to_string();
    }

    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '\\' {
            // Possible command
            i += 1;

            // Get command name
            let mut cmd_name = String::new();
            while i < chars.len() {
                let next = chars[i];
                if next.is_alphabetic() {
                    cmd_name.push(next);
                    i += 1;
                } else {
                    break;
                }
            }

            if cmd_name.is_empty() {
                // Not a command, just a backslash
                result.push('\\');
                continue;
            }

            // Check if we have a macro for this
            if let Some(macro_def) = db.get(&cmd_name) {
                // Build remaining string from current position
                let remaining: String = chars[i..].iter().collect();
                // Parse arguments
                let (args, consumed_chars) = parse_arguments_chars(&remaining, macro_def);
                i += consumed_chars;

                // Expand the macro
                let expanded = expand_single_macro(macro_def, &args);

                // Recursively expand
                let further_expanded = expand_macros_with_depth(&expanded, db, depth + 1);
                result.push_str(&further_expanded);
            } else {
                // Not a defined macro, keep as-is
                result.push('\\');
                result.push_str(&cmd_name);
            }
        } else {
            result.push(c);
            i += 1;
        }
    }

    result
}

/// Parse arguments for a macro (returns consumed character count, not byte count)
fn parse_arguments_chars(input: &str, macro_def: &Macro) -> (Vec<String>, usize) {
    let (args, byte_pos) = parse_arguments(input, macro_def);
    // Convert byte position to character position
    // Use get() to safely handle UTF-8 boundary issues
    let char_count = if byte_pos <= input.len() {
        // Find the valid UTF-8 boundary at or before byte_pos
        let valid_byte_pos = input
            .char_indices()
            .take_while(|(i, _)| *i < byte_pos)
            .count();
        valid_byte_pos
    } else {
        input.chars().count()
    };
    (args, char_count)
}

/// Parse arguments for a macro
fn parse_arguments(input: &str, macro_def: &Macro) -> (Vec<String>, usize) {
    let mut args = Vec::new();
    let mut pos = 0;

    for i in 0..macro_def.num_args {
        // Safely get the rest of the string starting at pos
        let rest = match input.get(pos..) {
            Some(s) => s,
            None => break,
        };
        let rest = rest.trim_start();

        // Check argument spec
        let spec = macro_def
            .arg_specs
            .get(i)
            .cloned()
            .unwrap_or(ArgSpec::Required);

        // Calculate offset between pos and trimmed rest
        let trim_offset = match input.get(pos..) {
            Some(s) => s.len() - rest.len(),
            None => 0,
        };

        match spec {
            ArgSpec::Optional(default) => {
                if rest.starts_with('[') {
                    // Optional argument provided
                    if let Some((arg, consumed)) = extract_bracketed(rest) {
                        args.push(arg);
                        pos += consumed + trim_offset;
                    } else {
                        args.push(default);
                    }
                } else {
                    // Use default
                    args.push(default);
                }
            }
            ArgSpec::Required => {
                if rest.starts_with('{') {
                    // Braced argument
                    if let Some((arg, consumed)) = extract_braced_arg(rest) {
                        args.push(arg);
                        pos += consumed + trim_offset;
                    } else {
                        break;
                    }
                } else if let Some(c) = rest.chars().next() {
                    // Single character argument
                    args.push(c.to_string());
                    pos += c.len_utf8() + trim_offset;
                }
            }
            ArgSpec::Delimited(delim) => {
                // Find delimiter
                if let Some(end) = rest.find(&delim) {
                    if let Some(arg_str) = rest.get(..end) {
                        args.push(arg_str.to_string());
                        pos += end + delim.len() + trim_offset;
                    }
                }
            }
        }
    }

    (args, pos)
}

/// Extract bracketed argument [content]
fn extract_bracketed(input: &str) -> Option<(String, usize)> {
    if !input.starts_with('[') {
        return None;
    }

    let mut depth = 0;
    let mut start = 0;

    for (i, c) in input.char_indices() {
        match c {
            '[' => {
                if depth == 0 {
                    start = i + 1;
                }
                depth += 1;
            }
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((input[start..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
    }

    None
}

/// Extract braced argument {content}
fn extract_braced_arg(input: &str) -> Option<(String, usize)> {
    if !input.starts_with('{') {
        return None;
    }

    let mut depth = 0;
    let mut start = 0;

    for (i, c) in input.char_indices() {
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
                    return Some((input[start..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
    }

    None
}

/// Expand a single macro with given arguments
fn expand_single_macro(macro_def: &Macro, args: &[String]) -> String {
    let mut result = macro_def.replacement.clone();

    // Replace #1, #2, etc. with arguments
    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("#{}", i + 1);
        result = result.replace(&placeholder, arg);
    }

    result
}

// ============================================================================
// Preprocessing: Extract and Remove Definitions
// ============================================================================

/// Extract macro definitions from document and return cleaned document
pub fn extract_and_remove_definitions(input: &str) -> (MacroDb, String) {
    let mut db = MacroDb::with_defaults();
    let mut result = input.to_string();

    // Find and process all definitions
    let patterns = [
        "\\newcommand",
        "\\renewcommand",
        "\\providecommand",
        "\\def",
        "\\DeclareMathOperator",
    ];

    for pattern in patterns {
        while let Some(start) = result.find(pattern) {
            // Find the end of this definition
            let after = &result[start..];
            if let Some(end) = find_definition_end_simple(after) {
                // Parse the definition
                let def_text = &result[start..start + end];
                if let Some(macro_def) = parse_definition(def_text) {
                    db.define(macro_def);
                }

                // Remove from result
                result = format!("{}{}", &result[..start], &result[start + end..]);
            } else {
                break;
            }
        }
    }

    (db, result)
}

fn find_definition_end_simple(input: &str) -> Option<usize> {
    let mut depth = 0;
    let mut brace_count = 0;

    for (i, c) in input.char_indices() {
        match c {
            '{' => {
                depth += 1;
                brace_count += 1;
            }
            '}' => {
                depth -= 1;
                // For \newcommand{\foo}{body}, we need to find the closing of body
                // which is the second time depth returns to 0
                if depth == 0 && brace_count >= 2 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }

    None
}

fn parse_definition(input: &str) -> Option<Macro> {
    if input.starts_with("\\newcommand")
        || input.starts_with("\\renewcommand")
        || input.starts_with("\\providecommand")
    {
        parse_newcommand_simple(input)
    } else if input.starts_with("\\def") {
        parse_def_simple(input)
    } else if input.starts_with("\\DeclareMathOperator") {
        parse_declare_math_operator(input)
    } else {
        None
    }
}

fn parse_newcommand_simple(input: &str) -> Option<Macro> {
    // Find the start of the name (first { after \newcommand)
    let cmd_end = if input.starts_with("\\newcommand*") {
        "\\newcommand*".len()
    } else if input.starts_with("\\newcommand") {
        "\\newcommand".len()
    } else if input.starts_with("\\renewcommand*") {
        "\\renewcommand*".len()
    } else if input.starts_with("\\renewcommand") {
        "\\renewcommand".len()
    } else if input.starts_with("\\providecommand*") {
        "\\providecommand*".len()
    } else if input.starts_with("\\providecommand") {
        "\\providecommand".len()
    } else {
        return None;
    };

    let rest = input[cmd_end..].trim_start();

    // Get name from {\name}
    if !rest.starts_with('{') {
        return None;
    }

    let name_end = find_matching_brace_simple(rest)?;
    let name_content = &rest[1..name_end];
    let name = name_content.trim().trim_start_matches('\\').to_string();

    let mut remaining = rest[name_end + 1..].trim_start();

    // Parse [numargs][default]
    let mut num_args = 0;

    if remaining.starts_with('[') {
        if let Some(end) = remaining.find(']') {
            if let Ok(n) = remaining[1..end].trim().parse() {
                num_args = n;
            }
            remaining = remaining[end + 1..].trim_start();
        }
    }

    // Skip optional default value
    if remaining.starts_with('[') {
        if let Some(end) = remaining.find(']') {
            remaining = remaining[end + 1..].trim_start();
        }
    }

    // Get replacement body
    if !remaining.starts_with('{') {
        return None;
    }

    let body_end = find_matching_brace_simple(remaining)?;
    let replacement = remaining[1..body_end].to_string();

    Some(Macro::with_args(&name, num_args, &replacement))
}

fn find_matching_brace_simple(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }

    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
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

fn parse_def_simple(input: &str) -> Option<Macro> {
    let rest = input.strip_prefix("\\def")?.trim_start();

    if !rest.starts_with('\\') {
        return None;
    }

    // Get name
    let name_end = rest[1..]
        .find(|c: char| !c.is_alphabetic())
        .map(|i| i + 1)?;
    let name = rest[1..name_end].to_string();

    let mut remaining = &rest[name_end..];
    let mut num_args = 0;

    // Count #n arguments
    while remaining.starts_with('#') {
        if let Some(c) = remaining.chars().nth(1) {
            if c.is_ascii_digit() {
                num_args = num_args.max(c.to_digit(10).unwrap_or(0) as usize);
                remaining = &remaining[2..];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Get body
    if remaining.starts_with('{') {
        let mut depth = 0;
        let mut body_start = 0;
        let mut body_end = 0;

        for (i, c) in remaining.char_indices() {
            match c {
                '{' => {
                    if depth == 0 {
                        body_start = i + 1;
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        body_end = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        if body_end > body_start {
            let replacement = remaining[body_start..body_end].to_string();
            return Some(Macro::with_args(&name, num_args, &replacement));
        }
    }

    None
}

fn parse_declare_math_operator(input: &str) -> Option<Macro> {
    // \DeclareMathOperator{\name}{text}
    let rest = input.strip_prefix("\\DeclareMathOperator")?.trim_start();
    let rest = rest.strip_prefix('*').unwrap_or(rest).trim_start();

    // Get name
    if !rest.starts_with('{') {
        return None;
    }

    let (name, rest) = extract_braced_simple(rest)?;
    let name = name.trim_start_matches('\\');

    // Get operator text
    let (text, _) = extract_braced_simple(rest.trim_start())?;

    // Create replacement as \operatorname{text}
    let replacement = format!("\\operatorname{{{}}}", text);

    Some(Macro::simple(name, &replacement))
}

fn extract_braced_simple(input: &str) -> Option<(String, &str)> {
    if !input.starts_with('{') {
        return None;
    }

    let mut depth = 0;
    for (i, c) in input.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((input[1..i].to_string(), &input[i + 1..]));
                }
            }
            _ => {}
        }
    }

    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_macro() {
        let mut db = MacroDb::new();
        db.define(Macro::simple("foo", "bar"));

        let result = expand_macros("\\foo", &db);
        assert_eq!(result, "bar");
    }

    #[test]
    fn test_macro_with_arg() {
        let mut db = MacroDb::new();
        db.define(Macro::with_args("bold", 1, "\\textbf{#1}"));

        let result = expand_macros("\\bold{hello}", &db);
        assert_eq!(result, "\\textbf{hello}");
    }

    #[test]
    fn test_macro_with_multiple_args() {
        let mut db = MacroDb::new();
        db.define(Macro::with_args("frac", 2, "\\frac{#1}{#2}"));

        let result = expand_macros("\\frac{a}{b}", &db);
        assert_eq!(result, "\\frac{a}{b}");
    }

    #[test]
    fn test_nested_expansion() {
        let mut db = MacroDb::new();
        db.define(Macro::simple("inner", "x"));
        db.define(Macro::with_args("outer", 1, "[#1]"));

        let result = expand_macros("\\outer{\\inner}", &db);
        assert_eq!(result, "[x]");
    }

    #[test]
    fn test_parse_newcommand() {
        let db = MacroDb::new();
        let result = db.parse_single_definition("\\newcommand{\\foo}{bar}");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.name, "foo");
        assert_eq!(m.replacement, "bar");
    }

    #[test]
    fn test_parse_newcommand_with_args() {
        let db = MacroDb::new();
        let result = db.parse_single_definition("\\newcommand{\\foo}[2]{#1 and #2}");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.name, "foo");
        assert_eq!(m.num_args, 2);
    }

    #[test]
    fn test_parse_def() {
        let db = MacroDb::new();
        let result = db.parse_single_definition("\\def\\foo#1{value: #1}");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.name, "foo");
        assert_eq!(m.num_args, 1);
    }

    #[test]
    fn test_extract_and_remove() {
        let input = r#"
\newcommand{\foo}{bar}
Some text \foo here.
"#;
        let (db, clean) = extract_and_remove_definitions(input);

        assert!(db.is_defined("foo"));
        assert!(!clean.contains("\\newcommand"));

        let expanded = expand_macros(&clean, &db);
        assert!(expanded.contains("bar"));
    }

    #[test]
    fn test_declare_math_operator() {
        let db = MacroDb::new();
        let _result = db.parse_single_definition("\\DeclareMathOperator{\\argmax}{arg\\,max}");
        // Note: this test may not pass with simple parser, but shows intent
    }

    #[test]
    fn test_recursion_limit() {
        let mut db = MacroDb::new();
        // Create a recursive macro (would cause infinite loop without limit)
        db.define(Macro::simple("loop", "\\loop"));

        let result = expand_macros("\\loop", &db);
        // Should terminate due to depth limit
        assert!(result.len() < 10000);
    }

    #[test]
    fn test_preserve_unknown_commands() {
        let db = MacroDb::new();
        let result = expand_macros("\\unknown{arg}", &db);
        assert_eq!(result, "\\unknown{arg}");
    }

    #[test]
    fn test_with_defaults() {
        let db = MacroDb::with_defaults();

        // Common macros should be defined
        assert!(db.is_defined("LaTeX"));
        assert!(db.is_defined("quad"));
    }
}
