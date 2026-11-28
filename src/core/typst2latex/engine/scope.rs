//! Scope management for the MiniEval interpreter.
//!
//! This module implements a stack-based scope system for variable binding
//! and lookup, supporting lexical scoping and closures.

use indexmap::IndexMap;

use super::value::{EvalError, EvalResult, Value};

/// A stack of scopes for variable management.
///
/// The scope stack implements lexical scoping:
/// - Inner scopes can shadow variables from outer scopes
/// - Variable lookup proceeds from innermost to outermost scope
/// - Entering a new scope (e.g., for a function call or block) pushes a new frame
#[derive(Debug, Clone)]
pub struct Scopes {
    /// The stack of scope frames
    stack: Vec<Scope>,
}

/// A single scope frame containing variable bindings.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// Variable bindings in this scope (IndexMap preserves insertion order)
    bindings: IndexMap<String, Value>,
}

impl Scope {
    /// Create a new empty scope.
    pub fn new() -> Self {
        Self {
            bindings: IndexMap::new(),
        }
    }

    /// Create a scope from existing bindings (for closures).
    pub fn from_captures(captures: IndexMap<String, Value>) -> Self {
        Self { bindings: captures }
    }

    /// Define a variable in this scope.
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        self.bindings.insert(name.into(), value);
    }

    /// Get a variable from this scope.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.bindings.get(name)
    }

    /// Get a mutable reference to a variable in this scope.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.bindings.get_mut(name)
    }

    /// Check if a variable exists in this scope.
    pub fn contains(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Get all bindings in this scope.
    pub fn bindings(&self) -> &IndexMap<String, Value> {
        &self.bindings
    }

    /// Take all bindings from this scope.
    pub fn into_bindings(self) -> IndexMap<String, Value> {
        self.bindings
    }
}

impl Scopes {
    /// Create a new scope stack with a single empty scope.
    pub fn new() -> Self {
        Self {
            stack: vec![Scope::new()],
        }
    }

    /// Create a scope stack with pre-defined standard library bindings.
    pub fn with_stdlib(stdlib: IndexMap<String, Value>) -> Self {
        Self {
            stack: vec![Scope::from_captures(stdlib)],
        }
    }

    /// Enter a new scope (push a new frame onto the stack).
    pub fn enter(&mut self) {
        self.stack.push(Scope::new());
    }

    /// Enter a new scope with captured variables (for closures).
    pub fn enter_with_captures(&mut self, captures: IndexMap<String, Value>) {
        self.stack.push(Scope::from_captures(captures));
    }

    /// Exit the current scope (pop a frame from the stack).
    ///
    /// Returns the exited scope, or None if only the global scope remains.
    pub fn exit(&mut self) -> Option<Scope> {
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    /// Define a variable in the current (topmost) scope.
    pub fn define(&mut self, name: impl Into<String>, value: Value) {
        if let Some(scope) = self.stack.last_mut() {
            scope.define(name, value);
        }
    }

    /// Look up a variable by name, searching from innermost to outermost scope.
    pub fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.stack.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value);
            }
        }
        None
    }

    /// Look up a variable or return an error if not found.
    pub fn get_or_err(&self, name: &str) -> EvalResult<&Value> {
        self.get(name)
            .ok_or_else(|| EvalError::undefined(name.to_string()))
    }

    /// Get a mutable reference to a variable, searching from innermost to outermost.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Value> {
        for scope in self.stack.iter_mut().rev() {
            if scope.contains(name) {
                return scope.get_mut(name);
            }
        }
        None
    }

    /// Assign to an existing variable (updates in the scope where it was defined).
    pub fn assign(&mut self, name: &str, value: Value) -> EvalResult<()> {
        for scope in self.stack.iter_mut().rev() {
            if scope.contains(name) {
                scope.define(name, value);
                return Ok(());
            }
        }
        Err(EvalError::undefined(name.to_string()))
    }

    /// Check if a variable exists in any scope.
    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Get the current scope depth (1 = global only).
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Get the current (topmost) scope.
    pub fn current(&self) -> Option<&Scope> {
        self.stack.last()
    }

    /// Get the current (topmost) scope mutably.
    pub fn current_mut(&mut self) -> Option<&mut Scope> {
        self.stack.last_mut()
    }

    /// Get bindings of the top-most scope (useful for module exports).
    pub fn top_bindings(&self) -> IndexMap<String, Value> {
        self.stack
            .last()
            .map(|s| s.bindings.clone())
            .unwrap_or_default()
    }

    /// Borrow bindings of the root (outermost) scope frame.
    pub(crate) fn root_bindings(&self) -> Option<&IndexMap<String, Value>> {
        self.stack.first().map(|s| s.bindings())
    }

    /// Capture all visible variables for a closure.
    ///
    /// This creates a snapshot of all variables visible from the current scope,
    /// which can be stored in a closure for later use.
    pub fn capture_all(&self) -> IndexMap<String, Value> {
        let mut captures = IndexMap::new();
        // Iterate from outermost to innermost so inner values override outer
        for scope in &self.stack {
            for (name, value) in scope.bindings() {
                captures.insert(name.clone(), value.clone());
            }
        }
        captures
    }

    /// Capture specific variables for a closure.
    pub fn capture(&self, names: &[&str]) -> IndexMap<String, Value> {
        let mut captures = IndexMap::new();
        for name in names {
            if let Some(value) = self.get(name) {
                captures.insert((*name).to_string(), value.clone());
            }
        }
        captures
    }
}

impl Default for Scopes {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_define_and_get() {
        let mut scopes = Scopes::new();
        scopes.define("x", Value::Int(42));

        assert_eq!(scopes.get("x"), Some(&Value::Int(42)));
        assert_eq!(scopes.get("y"), None);
    }

    #[test]
    fn test_scope_shadowing() {
        let mut scopes = Scopes::new();
        scopes.define("x", Value::Int(1));

        scopes.enter();
        scopes.define("x", Value::Int(2));
        assert_eq!(scopes.get("x"), Some(&Value::Int(2)));

        scopes.exit();
        assert_eq!(scopes.get("x"), Some(&Value::Int(1)));
    }

    #[test]
    fn test_scope_nested_lookup() {
        let mut scopes = Scopes::new();
        scopes.define("outer", Value::Int(1));

        scopes.enter();
        scopes.define("inner", Value::Int(2));

        // Can see both
        assert_eq!(scopes.get("outer"), Some(&Value::Int(1)));
        assert_eq!(scopes.get("inner"), Some(&Value::Int(2)));

        scopes.exit();

        // Only outer visible
        assert_eq!(scopes.get("outer"), Some(&Value::Int(1)));
        assert_eq!(scopes.get("inner"), None);
    }

    #[test]
    fn test_capture_all() {
        let mut scopes = Scopes::new();
        scopes.define("a", Value::Int(1));
        scopes.enter();
        scopes.define("b", Value::Int(2));
        scopes.define("a", Value::Int(10)); // Shadow outer a

        let captures = scopes.capture_all();
        assert_eq!(captures.get("a"), Some(&Value::Int(10))); // Inner value
        assert_eq!(captures.get("b"), Some(&Value::Int(2)));
    }
}
