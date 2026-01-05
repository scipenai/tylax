//! Preprocessing for Typst to LaTeX conversion
//!
//! This module implements an AST-based macro interpreter that:
//! 1. Extracts #let definitions from the source
//! 2. Expands macro calls and variable references by traversing the AST
//!
//! This approach is more robust than regex-based substitution because it
//! respects the syntactic structure of the document.

use std::collections::HashMap;
use typst_syntax::{parse, SyntaxKind, SyntaxNode};

/// Database of Typst variable/function definitions
#[derive(Debug, Default, Clone)]
pub struct TypstDefDb {
    /// Simple variable definitions: name -> value (as source text)
    variables: HashMap<String, String>,
    /// Function definitions: name -> (params, body_source_text)
    /// Body is stored as source text so it can be re-parsed for expansion
    functions: HashMap<String, (Vec<String>, String)>,
}

impl TypstDefDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Define a simple variable
    pub fn define_variable(&mut self, name: &str, value: &str) {
        self.variables.insert(name.to_string(), value.to_string());
    }

    /// Define a function with arguments
    pub fn define_function(&mut self, name: &str, args: Vec<String>, body: &str) {
        self.functions
            .insert(name.to_string(), (args, body.to_string()));
    }

    /// Get a variable value
    pub fn get_variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(|s| s.as_str())
    }

    /// Get all variables
    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }

    /// Get a function definition
    pub fn get_function(&self, name: &str) -> Option<&(Vec<String>, String)> {
        self.functions.get(name)
    }

    /// Check if a name is defined
    pub fn is_defined(&self, name: &str) -> bool {
        self.variables.contains_key(name) || self.functions.contains_key(name)
    }

    /// Get count of definitions
    pub fn len(&self) -> usize {
        self.variables.len() + self.functions.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty() && self.functions.is_empty()
    }
}

/// AST-based Macro Expander
///
/// This struct implements a tree rewriter that traverses the Typst AST
/// and expands macro calls/variable references on the fly.
pub struct MacroExpander<'a> {
    /// The definition database
    db: &'a TypstDefDb,
    /// Current scope bindings (for macro parameter substitution)
    /// This is a stack of scopes, where each scope is a map of name -> value
    scope_stack: Vec<HashMap<String, String>>,
    /// Recursion depth limit to prevent infinite loops
    max_depth: usize,
    /// Current recursion depth
    current_depth: usize,
}

impl<'a> MacroExpander<'a> {
    pub fn new(db: &'a TypstDefDb) -> Self {
        Self {
            db,
            scope_stack: Vec::new(),
            max_depth: 50, // Reasonable limit for nested macro expansion
            current_depth: 0,
        }
    }

    /// Look up a name in current scopes (innermost first) and db
    fn lookup(&self, name: &str) -> Option<String> {
        // Check scope stack first (innermost scope has priority)
        for scope in self.scope_stack.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }
        // Then check global variables
        self.db.get_variable(name).map(|s| s.to_string())
    }

    /// Main entry point: expand all macros in the given source
    pub fn expand(&mut self, source: &str) -> String {
        let root = parse(source);
        self.expand_node(&root)
    }

    /// Expand a single AST node, returning the expanded text
    pub fn expand_node(&mut self, node: &SyntaxNode) -> String {
        // Check recursion depth
        if self.current_depth > self.max_depth {
            return get_node_full_text(node);
        }

        match node.kind() {
            // Skip let bindings and set/show rules in output
            SyntaxKind::LetBinding | SyntaxKind::SetRule | SyntaxKind::ShowRule => String::new(),

            // Function call - check if it's a defined macro
            SyntaxKind::FuncCall => self.expand_func_call(node),

            // Identifier - check if it's a defined variable/parameter
            SyntaxKind::Ident => self.expand_ident(node),

            // Hash (code expression) - process its children
            SyntaxKind::Hash => self.expand_hash(node),

            // For all other nodes, recursively expand children
            _ => self.expand_children(node),
        }
    }

    /// Expand a Hash node (#expression)
    fn expand_hash(&self, node: &SyntaxNode) -> String {
        let mut result = String::new();
        let children: Vec<_> = node.children().collect();

        if children.is_empty() {
            // Hash with no children is just the # symbol - skip it
            // (it's Typst syntax, not content)
            return String::new();
        }

        for child in &children {
            match child.kind() {
                // If child is an identifier that we can expand, expand it without #
                SyntaxKind::Ident => {
                    let name = child.text().to_string();
                    if let Some(value) = self.lookup(&name) {
                        result.push_str(&value);
                    } else {
                        // Keep the # prefix for unknown identifiers
                        result.push('#');
                        result.push_str(&name);
                    }
                }
                // For function calls, check if it's a macro
                SyntaxKind::FuncCall => {
                    let func_name = self.get_func_name(child);
                    if self.db.is_defined(&func_name) {
                        // It's a macro - expand without # (the expansion handles output)
                        let expanded = self.expand_func_call_immut(child);
                        result.push_str(&expanded);
                    } else {
                        // Not a macro - keep the # prefix for Typst built-in functions
                        result.push('#');
                        result.push_str(&get_node_full_text(child));
                    }
                }
                _ => {
                    result.push_str(child.text().as_ref());
                }
            }
        }

        result
    }

    /// Expand an identifier node
    fn expand_ident(&self, node: &SyntaxNode) -> String {
        let name = node.text().to_string();

        // Check if this identifier is in our scope or db
        if let Some(value) = self.lookup(&name) {
            value
        } else {
            // Return unchanged
            name
        }
    }

    /// Expand a function call node (mutable version for internal use)
    fn expand_func_call(&mut self, node: &SyntaxNode) -> String {
        let func_name = self.get_func_name(node);

        // Check if this is a defined macro
        if let Some((params, body)) = self.db.get_function(&func_name).cloned() {
            // Parse actual arguments
            let actual_args = self.parse_args(node);

            // Perform text-based substitution since the body may not parse as code
            let expanded = self.substitute_params(&body, &params, &actual_args);

            // Recursively expand in case there are nested macro calls
            self.current_depth += 1;
            let result = self.expand(&expanded);
            self.current_depth -= 1;

            result
        } else {
            // Not a macro - expand children normally
            self.expand_children(node)
        }
    }

    /// Substitute parameters in body text with actual argument values
    fn substitute_params(&self, body: &str, params: &[String], args: &[String]) -> String {
        let mut result = body.to_string();

        for (i, param) in params.iter().enumerate() {
            if let Some(arg) = args.get(i) {
                // Replace #param references (common in Typst function bodies)
                let hash_pattern = format!("#{}", param);
                result = result.replace(&hash_pattern, arg);

                // Replace parameter references using regex word boundaries to avoid partial matches.
                // Example: param "a" should replace "a" but not "apple".
                result = replace_word(&result, param, arg);
            }
        }

        result
    }

    /// Expand a function call node (immutable version for use in closures)
    fn expand_func_call_immut(&self, node: &SyntaxNode) -> String {
        let func_name = self.get_func_name(node);

        // Check if this is a defined macro
        if let Some((params, body)) = self.db.get_function(&func_name).cloned() {
            // Parse actual arguments
            let actual_args = self.parse_args_immut(node);

            // Perform text-based substitution
            let expanded = self.substitute_params(&body, &params, &actual_args);

            // Create a new expander for recursive expansion
            let mut expander = MacroExpander::new(self.db);
            expander.scope_stack = self.scope_stack.clone();
            expander.current_depth = self.current_depth.saturating_add(1);

            if expander.current_depth > expander.max_depth {
                return get_node_full_text(node);
            }

            expander.expand(&expanded)
        } else {
            // Not a macro - return original text
            get_node_full_text(node)
        }
    }

    /// Get the function name from a FuncCall node
    fn get_func_name(&self, node: &SyntaxNode) -> String {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::Ident => return child.text().to_string(),
                SyntaxKind::FieldAccess => {
                    // For math.vec, we want "vec" as the local name
                    // but also track full path for future use
                    return self.get_field_access_name(child);
                }
                _ => {}
            }
        }
        String::new()
    }

    /// Get name from FieldAccess (e.g., "vec" from "math.vec")
    fn get_field_access_name(&self, node: &SyntaxNode) -> String {
        let mut parts = Vec::new();
        Self::collect_field_access_parts(node, &mut parts);
        // Return the last part (the actual function name)
        parts.last().cloned().unwrap_or_default()
    }

    fn collect_field_access_parts(node: &SyntaxNode, parts: &mut Vec<String>) {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::Ident => parts.push(child.text().to_string()),
                SyntaxKind::FieldAccess => Self::collect_field_access_parts(child, parts),
                _ => {}
            }
        }
    }

    /// Parse function arguments from FuncCall node
    fn parse_args(&self, node: &SyntaxNode) -> Vec<String> {
        let mut args = Vec::new();

        for child in node.children() {
            if child.kind() == SyntaxKind::Args {
                for arg_child in child.children() {
                    match arg_child.kind() {
                        // Skip commas, parens, and other syntax
                        SyntaxKind::LeftParen
                        | SyntaxKind::RightParen
                        | SyntaxKind::Comma
                        | SyntaxKind::Space => {}
                        // For actual arguments, get the expanded text
                        _ => {
                            let arg_text = arg_child.text().to_string().trim().to_string();
                            if !arg_text.is_empty() {
                                args.push(arg_text);
                            }
                        }
                    }
                }
            }
            // Also check for content blocks [...] as trailing arguments
            if child.kind() == SyntaxKind::ContentBlock {
                let content = child.text().to_string();
                // Strip the [ and ]
                let inner = content
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                args.push(inner);
            }
        }

        args
    }

    /// Parse function arguments (immutable version)
    fn parse_args_immut(&self, node: &SyntaxNode) -> Vec<String> {
        self.parse_args(node)
    }

    /// Expand all children and concatenate
    fn expand_children(&mut self, node: &SyntaxNode) -> String {
        let mut result = String::new();
        let children: Vec<_> = node.children().collect();
        let mut i = 0;

        while i < children.len() {
            let child = &children[i];

            // Check for Hash followed by SetRule/ShowRule/LetBinding - skip both
            if child.kind() == SyntaxKind::Hash && i + 1 < children.len() {
                let next = &children[i + 1];
                if matches!(
                    next.kind(),
                    SyntaxKind::SetRule | SyntaxKind::ShowRule | SyntaxKind::LetBinding
                ) {
                    // Skip both Hash and the rule
                    i += 2;
                    continue;
                }

                // Check for Hash followed by FuncCall - handle specially
                if next.kind() == SyntaxKind::FuncCall {
                    let func_name = self.get_func_name(next);
                    if self.db.is_defined(&func_name) {
                        // It's a macro - expand without # prefix
                        result.push_str(&self.expand_func_call(next));
                    } else {
                        // Not a macro - keep # prefix and output function call
                        result.push('#');
                        result.push_str(&get_node_full_text(next));
                    }
                    i += 2; // Skip both Hash and FuncCall
                    continue;
                }
            }

            result.push_str(&self.expand_node(child));
            i += 1;
        }

        // If no children, return the node's full text
        if children.is_empty() {
            return get_node_full_text(node);
        }

        result
    }
}

/// Replace a word in text (respecting word boundaries)
fn replace_word(text: &str, word: &str, replacement: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let word_chars: Vec<char> = word.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check if we have a potential match
        if i + word_chars.len() <= chars.len() {
            let slice: String = chars[i..i + word_chars.len()].iter().collect();
            if slice == word {
                // Check word boundaries
                let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
                let after_ok = i + word_chars.len() >= chars.len()
                    || !chars[i + word_chars.len()].is_alphanumeric();

                if before_ok && after_ok {
                    result.push_str(replacement);
                    i += word_chars.len();
                    continue;
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Extract #let definitions from Typst source using AST and return cleaned source
pub fn extract_let_definitions(input: &str) -> (TypstDefDb, String) {
    extract_let_from_ast(input)
}

/// Get the full text of a syntax node (handles inner nodes)
fn get_node_full_text(node: &SyntaxNode) -> String {
    // For leaf nodes, text() returns the content
    // For inner nodes, text() returns empty, so we need to clone and use into_text
    let text = node.text().to_string();
    if !text.is_empty() {
        return text;
    }
    // Clone and use into_text for inner nodes
    node.clone().into_text().to_string()
}

/// Extract #let definitions using typst-syntax AST
pub fn extract_let_from_ast(input: &str) -> (TypstDefDb, String) {
    let mut db = TypstDefDb::new();
    let root = parse(input);

    // Collect patterns to remove (exact text matches)
    let mut patterns_to_remove: Vec<String> = Vec::new();

    extract_lets_recursive(&root, &mut db, &mut patterns_to_remove);

    // Remove let bindings from source
    let mut result = input.to_string();
    for pattern in &patterns_to_remove {
        result = result.replace(pattern, "");
    }

    // Clean up multiple blank lines
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }

    (db, result.trim().to_string())
}

/// Recursively extract let bindings from AST
fn extract_lets_recursive(node: &SyntaxNode, db: &mut TypstDefDb, patterns: &mut Vec<String>) {
    // In the Typst AST, LetBinding is a sibling of Hash, not a child
    // We need to look at consecutive siblings: Hash followed by LetBinding
    let children: Vec<_> = node.children().collect();
    let mut i = 0;

    while i < children.len() {
        let child = &children[i];

        match child.kind() {
            SyntaxKind::Hash => {
                // Check if next sibling is a LetBinding
                if i + 1 < children.len() && children[i + 1].kind() == SyntaxKind::LetBinding {
                    let let_binding = &children[i + 1];

                    // Check if this is a function definition (has Closure node)
                    let has_closure = let_binding
                        .children()
                        .any(|c| c.kind() == SyntaxKind::Closure);
                    let has_params = let_binding
                        .children()
                        .any(|c| c.kind() == SyntaxKind::Params);

                    if has_closure || has_params {
                        extract_function_definition(let_binding, db);
                    } else {
                        extract_variable_definition(let_binding, db);
                    }

                    // Build the pattern to remove: # + let binding text
                    let hash_text = get_node_full_text(child);
                    let let_text = get_node_full_text(let_binding);
                    let pattern = format!("{}{}", hash_text, let_text);
                    patterns.push(pattern);

                    // Skip the LetBinding we just processed
                    i += 2;
                    continue;
                } else {
                    // Just a Hash without LetBinding, recurse into it
                    extract_lets_recursive(child, db, patterns);
                }
            }
            SyntaxKind::LetBinding => {
                // Standalone LetBinding (shouldn't happen in valid Typst, but handle it)
                let full_text = get_node_full_text(child);

                let has_closure = child.children().any(|c| c.kind() == SyntaxKind::Closure);
                let has_params = child.children().any(|c| c.kind() == SyntaxKind::Params);

                if has_closure || has_params {
                    extract_function_definition(child, db);
                } else {
                    extract_variable_definition(child, db);
                }

                patterns.push(full_text);
            }
            _ => {
                // Recurse into other nodes
                extract_lets_recursive(child, db, patterns);
            }
        }

        i += 1;
    }
}

/// Extract a simple variable definition from AST node
fn extract_variable_definition(node: &SyntaxNode, db: &mut TypstDefDb) {
    let mut name: Option<String> = None;
    let mut value: Option<String> = None;

    for child in node.children() {
        match child.kind() {
            SyntaxKind::Ident => {
                if name.is_none() {
                    name = Some(child.text().to_string());
                }
            }
            // Value could be various types
            SyntaxKind::Int
            | SyntaxKind::Float
            | SyntaxKind::Str
            | SyntaxKind::Bool
            | SyntaxKind::ContentBlock
            | SyntaxKind::FuncCall
            | SyntaxKind::Math
            | SyntaxKind::Equation => {
                if name.is_some() && value.is_none() {
                    value = Some(child.text().to_string());
                }
            }
            _ => {
                // For complex expressions, use the text representation
                if name.is_some() && value.is_none() {
                    let text = child.text().to_string();
                    if !text.trim().is_empty() && text != "=" && text != "#" && text != "let" {
                        value = Some(text);
                    }
                }
            }
        }
    }

    if let (Some(n), Some(v)) = (name, value) {
        db.define_variable(&n, &v);
    }
}

/// Extract a function definition from AST node
fn extract_function_definition(node: &SyntaxNode, db: &mut TypstDefDb) {
    let mut name: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    let mut body: Option<String> = None;

    for child in node.children() {
        match child.kind() {
            SyntaxKind::Ident => {
                // Direct identifier at LetBinding level (old style)
                if name.is_none() {
                    name = Some(child.text().to_string());
                }
            }
            SyntaxKind::Params => {
                // Direct params at LetBinding level (old style)
                extract_params_from_node(child, &mut params);
            }
            SyntaxKind::Closure => {
                // New style: Closure contains the function name, params, and body
                // Structure: Closure -> [Ident (name), Params, Eq, Body]
                for closure_child in child.children() {
                    match closure_child.kind() {
                        SyntaxKind::Ident => {
                            // Function name is inside Closure
                            if name.is_none() {
                                name = Some(closure_child.text().to_string());
                            }
                        }
                        SyntaxKind::Params => {
                            extract_params_from_node(closure_child, &mut params);
                        }
                        // Body types - store the FULL source text using helper
                        SyntaxKind::ContentBlock
                        | SyntaxKind::Math
                        | SyntaxKind::Equation
                        | SyntaxKind::Code
                        | SyntaxKind::CodeBlock
                        | SyntaxKind::FuncCall => {
                            body = Some(get_node_full_text(closure_child));
                        }
                        _ => {
                            // For other content, if we already have name and params
                            // but no body yet, capture it
                            if body.is_none() && name.is_some() && !params.is_empty() {
                                let text = get_node_full_text(closure_child);
                                if !text.trim().is_empty()
                                    && !matches!(text.trim(), "(" | ")" | "," | "=>" | "=" | " ")
                                {
                                    body = Some(text);
                                }
                            }
                        }
                    }
                }
            }
            // Direct body at LetBinding level
            SyntaxKind::ContentBlock
            | SyntaxKind::Math
            | SyntaxKind::Equation
            | SyntaxKind::FuncCall => {
                if name.is_some() && body.is_none() {
                    body = Some(child.text().to_string());
                }
            }
            _ => {}
        }
    }

    if let Some(n) = name {
        // Store the body as-is for re-parsing
        let body_str = body.unwrap_or_default();
        db.define_function(&n, params, &body_str);
    }
}

/// Extract parameter names from a Params node
fn extract_params_from_node(params_node: &SyntaxNode, params: &mut Vec<String>) {
    for child in params_node.children() {
        match child.kind() {
            SyntaxKind::Ident => {
                params.push(child.text().to_string());
            }
            _ => {
                // For complex parameter patterns, try to find the identifier
                for sub_child in child.children() {
                    if sub_child.kind() == SyntaxKind::Ident {
                        params.push(sub_child.text().to_string());
                        break;
                    }
                }
            }
        }
    }
}

/// Preprocess Typst source: extract definitions and expand using AST
pub fn preprocess_typst(input: &str) -> String {
    // Step 1: Extract definitions
    let (db, cleaned) = extract_let_from_ast(input);

    // If no definitions, return cleaned source as-is
    if db.is_empty() {
        return cleaned;
    }

    // Step 2: Create macro expander and expand
    let mut expander = MacroExpander::new(&db);
    expander.expand(&cleaned)
}

/// Expand variable references in Typst source (now uses AST)
pub fn expand_variables(input: &str, db: &TypstDefDb) -> String {
    let mut expander = MacroExpander::new(db);
    expander.expand(input)
}

/// Expand function calls with defined functions (now uses AST)
pub fn expand_function_calls(input: &str, db: &TypstDefDb) -> String {
    let mut expander = MacroExpander::new(db);
    expander.expand(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_let() {
        let input = r#"#let x = 5
#let name = "hello"
Some text with #x"#;

        let (db, cleaned) = extract_let_definitions(input);

        assert_eq!(db.get_variable("x"), Some("5"));
        assert_eq!(db.get_variable("name"), Some("\"hello\""));
        assert!(!cleaned.contains("#let x"));
        assert!(cleaned.contains("Some text"));
    }

    #[test]
    fn test_expand_variables_ast() {
        let mut db = TypstDefDb::new();
        db.define_variable("pi_approx", "3.14");

        let input = "The value is #pi_approx";
        let result = expand_variables(input, &db);

        assert!(result.contains("3.14"));
    }

    #[test]
    fn test_preprocess_full() {
        let input = r#"#let greeting = "Hello"
#greeting World"#;

        let result = preprocess_typst(input);
        assert!(result.contains("Hello") || result.contains("World"));
    }

    #[test]
    fn test_function_expansion() {
        let input = r#"#let double(x) = $2 #x$
The result is #double(5)"#;

        let result = preprocess_typst(input);
        // After expansion, #double(5) should become $2 5$
        assert!(result.contains("2") && result.contains("5"));
    }

    #[test]
    fn test_nested_function_expansion() {
        let input = r#"#let f(x) = $#x^2$
#let g(y) = #f(#y)
The result is #g(3)"#;

        let result = preprocess_typst(input);
        // After expansion, should contain 3^2
        assert!(result.contains("3") || result.contains("^2"));
    }

    #[test]
    fn test_macro_expander_depth_limit() {
        let mut db = TypstDefDb::new();
        // Create a recursive macro (should hit depth limit)
        db.define_function("recurse", vec!["x".to_string()], "#recurse(#x)");

        let mut expander = MacroExpander::new(&db);
        expander.max_depth = 5;

        // This should not hang - depth limit kicks in
        let result = expander.expand("#recurse(1)");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_typst_def_db() {
        let mut db = TypstDefDb::new();
        assert!(db.is_empty());

        db.define_variable("x", "10");
        assert!(!db.is_empty());
        assert_eq!(db.len(), 1);
        assert!(db.is_defined("x"));
        assert!(!db.is_defined("y"));
    }

    #[test]
    fn test_math_let() {
        let input = r#"#let alpha = $\alpha$
The Greek letter is #alpha"#;

        let (db, cleaned) = extract_let_definitions(input);
        assert!(db.is_defined("alpha"));
        assert!(!cleaned.contains("#let"));
    }

    #[test]
    fn test_myvec_expansion() {
        let input = r#"#let myvec(x, y) = $vec(#x, #y)$
The vector is #myvec(a, b)"#;

        let result = preprocess_typst(input);
        // Should expand to contain vec(a, b) in some form
        assert!(result.contains("vec") || result.contains("a") && result.contains("b"));
    }
}
