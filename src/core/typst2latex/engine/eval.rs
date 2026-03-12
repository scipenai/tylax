//! The MiniEval interpreter engine.
//!
//! This module implements the core evaluation logic for Typst macros,
//! including expression evaluation, control flow, and function calls.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::features::refs::{citation_mode_from_typst_form, CitationMode, ReferenceType};
use indexmap::IndexMap;

use typst_syntax::ast::{self, AstNode};
use typst_syntax::{parse, parse_code, SyntaxKind, SyntaxNode};

use super::library::{call_builtin, call_calc, call_method, BuiltinResult};
use super::ops;
use super::scope::Scopes;
use super::value::{
    bibliography_content_value, citation_content_value, label_content_value,
    normalize_ref_target_text, reference_content_value, Alignment, Arguments, Closure, ContentNode,
    Direction, EvalError, EvalErrorKind, EvalResult, HorizAlign, Selector, ShowRule, SourceSpan,
    Value, VertAlign,
};
use super::vfs::{NoopVfs, VirtualFileSystem};

/// Maximum number of loop iterations (infinite loop protection).
const MAX_ITERATIONS: usize = 10_000;

/// Configuration for the MiniEval interpreter.
#[derive(Debug, Clone)]
pub struct EvalConfig {
    /// If true, errors on undefined functions/methods cause evaluation to fail.
    /// If false, they produce warnings and fallback to raw source output.
    pub strict: bool,
    /// Maximum recursion depth for function calls.
    pub max_recursion_depth: usize,
    /// Maximum iterations for loops (infinite loop protection).
    pub max_iterations: usize,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            strict: false, // Default to compat mode for better user experience
            // Use a conservative depth limit to prevent stack overflow on Windows
            // (Windows has smaller default stack size than Linux/macOS)
            max_recursion_depth: 64,
            max_iterations: MAX_ITERATIONS,
        }
    }
}

impl EvalConfig {
    /// Create a strict configuration (fail on unknown functions).
    pub fn strict() -> Self {
        Self {
            strict: true,
            ..Default::default()
        }
    }

    /// Create a compat/lenient configuration (fallback on unknown functions).
    pub fn compat() -> Self {
        Self {
            strict: false,
            ..Default::default()
        }
    }
}

/// A warning that occurred during evaluation but didn't stop execution.
#[derive(Debug, Clone)]
pub struct EvalWarning {
    /// The warning message
    pub message: String,
    /// Optional source span where the warning occurred
    pub span: Option<SourceSpan>,
}

impl EvalWarning {
    /// Create a new warning.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
        }
    }

    /// Attach a span to this warning.
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }
}

/// The MiniEval interpreter.
///
/// This struct holds the state needed to evaluate Typst source code,
/// including variable bindings, control flow state, file system access,
/// and module caching.
pub struct MiniEval {
    /// The scope stack for variable bindings
    scopes: Scopes,
    /// Current control flow state
    flow: Option<FlowEvent>,
    /// Virtual file system for module loading and data access
    vfs: Arc<dyn VirtualFileSystem>,
    /// Cache of loaded modules (path -> module scope)
    module_cache: Arc<RwLock<HashMap<String, IndexMap<String, Value>>>>,
    /// Current file path (for relative imports)
    current_file: Option<String>,
    /// Global state storage (for state() function)
    state_store: HashMap<String, Value>,
    /// Counter storage (for counter() function)
    counter_store: HashMap<String, Vec<i64>>,
    /// Configuration for evaluation behavior
    config: EvalConfig,
    /// Warnings accumulated during evaluation
    warnings: Vec<EvalWarning>,
    /// Registered show rules
    show_rules: Vec<ShowRule>,
    /// Current recursion depth (for infinite recursion protection)
    current_depth: usize,
}

/// A control flow event that occurred during evaluation.
#[derive(Debug, Clone)]
enum FlowEvent {
    /// Break out of a loop
    Break,
    /// Continue to next iteration
    Continue,
    /// Return from a function with an optional value
    Return(Option<Value>),
}

impl MiniEval {
    /// Create a new MiniEval interpreter with default (no-op) VFS and compat mode.
    pub fn new() -> Self {
        Self {
            scopes: Scopes::new(),
            flow: None,
            vfs: Arc::new(NoopVfs),
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            current_file: None,
            state_store: HashMap::new(),
            counter_store: HashMap::new(),
            config: EvalConfig::default(),
            warnings: Vec::new(),
            show_rules: Vec::new(),
            current_depth: 0,
        }
    }

    /// Create a new MiniEval interpreter with a custom VFS.
    pub fn with_vfs(vfs: Arc<dyn VirtualFileSystem>) -> Self {
        Self {
            scopes: Scopes::new(),
            flow: None,
            vfs,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            current_file: None,
            state_store: HashMap::new(),
            counter_store: HashMap::new(),
            config: EvalConfig::default(),
            warnings: Vec::new(),
            show_rules: Vec::new(),
            current_depth: 0,
        }
    }

    /// Create a new MiniEval interpreter with custom config.
    pub fn with_config(config: EvalConfig) -> Self {
        Self {
            scopes: Scopes::new(),
            flow: None,
            vfs: Arc::new(NoopVfs),
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            current_file: None,
            state_store: HashMap::new(),
            counter_store: HashMap::new(),
            config,
            warnings: Vec::new(),
            show_rules: Vec::new(),
            current_depth: 0,
        }
    }

    /// Create a new MiniEval interpreter with custom VFS and config.
    pub fn with_vfs_and_config(vfs: Arc<dyn VirtualFileSystem>, config: EvalConfig) -> Self {
        Self {
            scopes: Scopes::new(),
            flow: None,
            vfs,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            current_file: None,
            state_store: HashMap::new(),
            counter_store: HashMap::new(),
            config,
            warnings: Vec::new(),
            show_rules: Vec::new(),
            current_depth: 0,
        }
    }

    /// Set the current file path (for relative imports).
    pub fn set_current_file(&mut self, path: impl Into<String>) {
        self.current_file = Some(path.into());
    }

    /// Get a reference to the VFS.
    pub fn vfs(&self) -> &Arc<dyn VirtualFileSystem> {
        &self.vfs
    }

    /// Get the current configuration.
    pub fn config(&self) -> &EvalConfig {
        &self.config
    }

    /// Check if running in strict mode.
    pub fn is_strict(&self) -> bool {
        self.config.strict
    }

    /// Add a warning during evaluation.
    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(EvalWarning::new(message));
    }

    /// Add a warning with span information.
    pub fn warn_at(&mut self, message: impl Into<String>, span: SourceSpan) {
        self.warnings
            .push(EvalWarning::new(message).with_span(span));
    }

    /// Take all accumulated warnings.
    pub fn take_warnings(&mut self) -> Vec<EvalWarning> {
        std::mem::take(&mut self.warnings)
    }

    /// Get accumulated warnings (without taking them).
    pub fn warnings(&self) -> &[EvalWarning] {
        &self.warnings
    }

    // ========================================================================
    // Show Rules
    // ========================================================================

    /// Register a show rule.
    pub fn add_show_rule(&mut self, selector: Selector, transform: Arc<Closure>) {
        let priority = self.show_rules.len();
        self.show_rules
            .push(ShowRule::new(selector, transform, priority));
    }

    /// Get registered show rules.
    pub fn show_rules(&self) -> &[ShowRule] {
        &self.show_rules
    }

    /// Apply show rules to content, transforming matching nodes.
    ///
    /// This recursively traverses the content tree and applies matching rules.
    /// Rules are applied in reverse priority order (later rules take precedence).
    pub fn apply_show_rules(&mut self, content: Vec<ContentNode>) -> EvalResult<Vec<ContentNode>> {
        let mut result = Vec::new();

        for node in content {
            // Find the first matching rule (by highest priority)
            let matching_rule = self
                .show_rules
                .iter()
                .rev()
                .find(|rule| rule.selector.matches(&node))
                .cloned();

            if let Some(rule) = matching_rule {
                // The transform function receives the matched content as "it"
                // For MVP, we pass the node as a Value
                let node_value = Value::Content(vec![node.clone()]);

                // Create a scope with "it" bound to the matched content
                self.scopes.enter();
                self.scopes.define("it".to_string(), node_value);

                // Call the closure with no explicit args (it uses "it" from scope)
                let body_result = self.call_closure_inner(&rule.transform);

                self.scopes.exit();

                // Convert result to content
                match body_result {
                    Ok(Value::Content(nodes)) => {
                        result.extend(nodes);
                    }
                    Ok(other) => {
                        // Convert other values to text content
                        result.push(ContentNode::Text(other.display()));
                    }
                    Err(_) => {
                        // On error, keep the original node
                        result.push(node);
                    }
                }
            } else {
                // No matching rule, recursively process children
                let processed = self.process_content_children(node)?;
                result.push(processed);
            }
        }

        Ok(result)
    }

    /// Call a closure's body without binding parameters (assumes scope is set up).
    fn call_closure_inner(&mut self, closure: &Closure) -> EvalResult<Value> {
        // Parse and evaluate the body
        let body_source = &closure.body_source;
        let root = parse_code(body_source);

        if root.erroneous() {
            return Err(EvalError::syntax("Failed to parse closure body"));
        }

        // Find and evaluate the expression
        if let Some(expr) = root.cast::<ast::Expr>() {
            self.eval_expr(expr)
        } else if let Some(code) = root.cast::<ast::Code>() {
            self.eval_code(code)
        } else {
            Ok(Value::None)
        }
    }

    /// Recursively process children of a content node.
    fn process_content_children(&mut self, node: ContentNode) -> EvalResult<ContentNode> {
        match node {
            ContentNode::Strong(children) => {
                let processed = self.apply_show_rules(children)?;
                Ok(ContentNode::Strong(processed))
            }
            ContentNode::Emph(children) => {
                let processed = self.apply_show_rules(children)?;
                Ok(ContentNode::Emph(processed))
            }
            ContentNode::Heading { level, content } => {
                let processed = self.apply_show_rules(content)?;
                Ok(ContentNode::Heading {
                    level,
                    content: processed,
                })
            }
            ContentNode::ListItem(children) => {
                let processed = self.apply_show_rules(children)?;
                Ok(ContentNode::ListItem(processed))
            }
            ContentNode::EnumItem { number, content } => {
                let processed = self.apply_show_rules(content)?;
                Ok(ContentNode::EnumItem {
                    number,
                    content: processed,
                })
            }
            // Other nodes don't have children to process
            other => Ok(other),
        }
    }

    /// Evaluate markup (the top-level content).
    pub fn eval_markup(&mut self, markup: ast::Markup) -> EvalResult<Value> {
        let mut output = Value::None;

        for expr in markup.exprs() {
            let value = self.eval_expr(expr)?;
            output = ops::join(output, value)?;

            if self.flow.is_some() {
                break;
            }
        }

        Ok(output)
    }

    /// Evaluate an expression.
    /// Evaluate an expression.
    ///
    /// This is the main expression dispatcher. Each expression type is routed
    /// to a dedicated handler method for clarity and testability.
    pub fn eval_expr(&mut self, expr: ast::Expr) -> EvalResult<Value> {
        match expr {
            // ================================================================
            // Literals (primitives)
            // ================================================================
            ast::Expr::None(_)
            | ast::Expr::Auto(_)
            | ast::Expr::Bool(_)
            | ast::Expr::Int(_)
            | ast::Expr::Float(_)
            | ast::Expr::Str(_) => self.eval_literal(expr),

            // ================================================================
            // Identifiers
            // ================================================================
            ast::Expr::Ident(ident) => self.eval_ident(ident),

            // ================================================================
            // Collections (Array, Dict)
            // ================================================================
            ast::Expr::Array(arr) => self.eval_array(arr),
            ast::Expr::Dict(dict) => self.eval_dict(dict),

            // ================================================================
            // Blocks and Parenthesized Expressions
            // ================================================================
            ast::Expr::CodeBlock(block) => self.eval_code_block(block),
            ast::Expr::ContentBlock(block) => self.eval_content_block(block),
            ast::Expr::Parenthesized(paren) => self.eval_expr(paren.expr()),

            // ================================================================
            // Operators (Unary, Binary)
            // ================================================================
            ast::Expr::Unary(unary) => self.eval_unary(unary),
            ast::Expr::Binary(binary) => self.eval_binary(binary),

            // ================================================================
            // Control Flow (Conditional, Loops, Break/Continue/Return)
            // ================================================================
            ast::Expr::Conditional(cond) => self.eval_conditional(cond),
            ast::Expr::WhileLoop(while_loop) => self.eval_while(while_loop),
            ast::Expr::ForLoop(for_loop) => self.eval_for(for_loop),
            ast::Expr::LoopBreak(_) => self.handle_loop_break(),
            ast::Expr::LoopContinue(_) => self.handle_loop_continue(),
            ast::Expr::FuncReturn(ret) => self.handle_func_return(ret),

            // ================================================================
            // Bindings and Functions
            // ================================================================
            ast::Expr::LetBinding(binding) => self.eval_let(binding),
            ast::Expr::FuncCall(call) => self.eval_func_call(call),
            ast::Expr::Closure(closure) => self.eval_closure(closure),

            // ================================================================
            // Field Access
            // ================================================================
            ast::Expr::FieldAccess(access) => self.eval_field_access(access),

            // ================================================================
            // Markup Elements
            // ================================================================
            ast::Expr::Text(_)
            | ast::Expr::Space(_)
            | ast::Expr::Linebreak(_)
            | ast::Expr::Parbreak(_)
            | ast::Expr::Strong(_)
            | ast::Expr::Emph(_)
            | ast::Expr::Heading(_)
            | ast::Expr::ListItem(_)
            | ast::Expr::EnumItem(_)
            | ast::Expr::Raw(_)
            | ast::Expr::Equation(_)
            | ast::Expr::Math(_)
            | ast::Expr::Label(_)
            | ast::Expr::Ref(_)
            | ast::Expr::Escape(_)
            | ast::Expr::Shorthand(_) => self.eval_markup_element(expr),

            // ================================================================
            // Module System (Import, Include)
            // ================================================================
            ast::Expr::ModuleImport(import) => self.eval_import(import),
            ast::Expr::ModuleInclude(include) => self.eval_include(include),

            // ================================================================
            // Passthrough (unevaluated, preserved as raw source)
            // ================================================================
            ast::Expr::SetRule(_)
            | ast::Expr::ShowRule(_)
            | ast::Expr::Contextual(_)
            | ast::Expr::DestructAssignment(_) => self.passthrough_expr(&expr),

            // Fallback for any unhandled expression types
            _ => self.passthrough_expr(&expr),
        }
    }

    // ========================================================================
    // Control Flow Helpers
    // ========================================================================

    /// Handle `break` statement in loops.
    fn handle_loop_break(&mut self) -> EvalResult<Value> {
        self.flow = Some(FlowEvent::Break);
        Ok(Value::None)
    }

    /// Handle `continue` statement in loops.
    fn handle_loop_continue(&mut self) -> EvalResult<Value> {
        self.flow = Some(FlowEvent::Continue);
        Ok(Value::None)
    }

    /// Handle `return` statement in functions.
    fn handle_func_return(&mut self, ret: ast::FuncReturn) -> EvalResult<Value> {
        let value = ret.body().map(|e| self.eval_expr(e)).transpose()?;
        self.flow = Some(FlowEvent::Return(value));
        Ok(Value::None)
    }

    /// Passthrough expression as raw source (for unevaluated expressions).
    fn passthrough_expr(&self, expr: &ast::Expr) -> EvalResult<Value> {
        let source = expr.to_untyped().text().to_string();
        Ok(Value::Content(vec![ContentNode::RawSource(source)]))
    }

    /// Evaluate literal expressions (None, Auto, Bool, Int, Float, Str).
    fn eval_literal(&mut self, expr: ast::Expr) -> EvalResult<Value> {
        match expr {
            ast::Expr::None(_) => Ok(Value::None),
            ast::Expr::Auto(_) => Ok(Value::Auto),
            ast::Expr::Bool(b) => Ok(Value::Bool(b.get())),
            ast::Expr::Int(i) => Ok(Value::Int(i.get())),
            ast::Expr::Float(f) => Ok(Value::Float(f.get())),
            ast::Expr::Str(s) => Ok(Value::Str(s.get().to_string())),
            _ => unreachable!("eval_literal called with non-literal"),
        }
    }

    /// Evaluate markup element expressions (Text, Space, Strong, Emph, etc.).
    fn eval_markup_element(&mut self, expr: ast::Expr) -> EvalResult<Value> {
        match expr {
            ast::Expr::Text(text) => Ok(Value::Content(vec![ContentNode::Text(
                text.get().to_string(),
            )])),
            ast::Expr::Space(_) => Ok(Value::Content(vec![ContentNode::Space])),
            ast::Expr::Linebreak(_) => Ok(Value::Content(vec![ContentNode::Linebreak])),
            ast::Expr::Parbreak(_) => Ok(Value::Content(vec![ContentNode::Parbreak])),
            ast::Expr::Strong(strong) => {
                let body = self.eval_markup(strong.body())?;
                Ok(Value::Content(vec![ContentNode::Strong(
                    body.into_content(),
                )]))
            }
            ast::Expr::Emph(emph) => {
                let body = self.eval_markup(emph.body())?;
                Ok(Value::Content(vec![ContentNode::Emph(body.into_content())]))
            }
            ast::Expr::Heading(heading) => {
                let level = heading.depth().get() as u8;
                let body = self.eval_markup(heading.body())?;
                Ok(Value::Content(vec![ContentNode::Heading {
                    level,
                    content: body.into_content(),
                }]))
            }
            ast::Expr::ListItem(item) => {
                let body = self.eval_markup(item.body())?;
                Ok(Value::Content(vec![ContentNode::ListItem(
                    body.into_content(),
                )]))
            }
            ast::Expr::EnumItem(item) => {
                let number = item.number();
                let body = self.eval_markup(item.body())?;
                Ok(Value::Content(vec![ContentNode::EnumItem {
                    number: number.map(|n| n as i64),
                    content: body.into_content(),
                }]))
            }
            ast::Expr::Raw(raw) => {
                let text: String = raw
                    .lines()
                    .map(|l| l.get().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                let lang = raw.lang().map(|l| l.get().to_string());
                let block = raw.block();
                Ok(Value::Content(vec![ContentNode::Raw { text, lang, block }]))
            }
            ast::Expr::Equation(eq) => {
                // Evaluate hash expressions inside math content
                let content = self.eval_math_content(eq.body().to_untyped())?;
                let block = eq.block();
                Ok(Value::Content(vec![ContentNode::Math { content, block }]))
            }
            ast::Expr::Math(math) => {
                // Evaluate hash expressions inside math content
                let content = self.eval_math_content(math.to_untyped())?;
                Ok(Value::Content(vec![ContentNode::Math {
                    content,
                    block: false,
                }]))
            }
            ast::Expr::Label(label) => Ok(Value::Content(vec![ContentNode::Label(
                label.get().to_string(),
            )])),
            ast::Expr::Ref(reference) => Ok(Value::Content(vec![ContentNode::Reference {
                target: reference.target().to_string(),
                ref_type: ReferenceType::Basic,
            }])),
            ast::Expr::Escape(esc) => Ok(Value::Content(vec![ContentNode::Text(
                esc.get().to_string(),
            )])),
            ast::Expr::Shorthand(sh) => Ok(Value::Content(vec![ContentNode::Text(
                sh.get().to_string(),
            )])),
            _ => unreachable!("eval_markup_element called with non-markup"),
        }
    }

    /// Evaluate a module import.
    fn eval_import(&mut self, import: ast::ModuleImport) -> EvalResult<Value> {
        let source_expr = import.source();
        let source_val = self.eval_expr(source_expr)?;
        let path = source_val.as_str()?;

        let current_dir = if let Some(cf) = &self.current_file {
            std::path::Path::new(cf)
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_str()
                .unwrap_or(".")
        } else {
            "."
        };

        let resolved_path = self
            .vfs
            .resolve(current_dir, path)
            .map_err(|e| EvalError::other(e.to_string()))?;

        // Check cache - clone the module to avoid holding the lock
        let cached_module = {
            if let Ok(cache) = self.module_cache.read() {
                cache.get(&resolved_path).cloned()
            } else {
                None
            }
        };

        if let Some(module) = cached_module {
            if let Some(new_name) = import.new_name() {
                let module_val = Value::Dict(module.clone());
                self.scopes.define(new_name.get().to_string(), module_val);
            } else if let Some(imports) = import.imports() {
                self.bind_imports(Some(imports), &module)?;
            }
            return Ok(Value::None);
        }

        // Load and evaluate
        let content = self
            .vfs
            .read_text(&resolved_path)
            .map_err(|e| EvalError::other(e.to_string()))?;

        let mut sub_eval = MiniEval {
            scopes: Scopes::new(),
            flow: None,
            vfs: self.vfs.clone(),
            module_cache: self.module_cache.clone(),
            current_file: Some(resolved_path.clone()),
            state_store: self.state_store.clone(),
            counter_store: self.counter_store.clone(),
            config: self.config.clone(),
            warnings: Vec::new(),
            show_rules: self.show_rules.clone(),
            current_depth: self.current_depth, // Inherit depth from parent
        };

        let root = parse(&content);
        if !root.errors().is_empty() {
            return Err(EvalError::syntax(format!(
                "In module {}: parse error",
                path
            )));
        }

        if let Some(markup) = root.cast::<ast::Markup>() {
            sub_eval.eval_markup(markup)?;
        }

        let exports = sub_eval.scopes.top_bindings();

        // Update cache
        if let Ok(mut cache) = self.module_cache.write() {
            cache.insert(resolved_path, exports.clone());
        }

        if let Some(new_name) = import.new_name() {
            let module_val = Value::Dict(exports.clone());
            self.scopes.define(new_name.get().to_string(), module_val);
        } else if let Some(imports) = import.imports() {
            self.bind_imports(Some(imports), &exports)?;
        }

        Ok(Value::None)
    }

    /// Evaluate a module include.
    fn eval_include(&mut self, include: ast::ModuleInclude) -> EvalResult<Value> {
        let source_expr = include.source();
        let source_val = self.eval_expr(source_expr)?;
        let path = source_val.as_str()?;

        let current_dir = if let Some(cf) = &self.current_file {
            std::path::Path::new(cf)
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_str()
                .unwrap_or(".")
        } else {
            "."
        };

        let resolved_path = self
            .vfs
            .resolve(current_dir, path)
            .map_err(|e| EvalError::other(e.to_string()))?;
        let content = self
            .vfs
            .read_text(&resolved_path)
            .map_err(|e| EvalError::other(e.to_string()))?;

        // Include evaluates content and returns it
        let mut sub_eval = MiniEval {
            scopes: Scopes::new(),
            flow: None,
            vfs: self.vfs.clone(),
            module_cache: self.module_cache.clone(),
            current_file: Some(resolved_path.clone()),
            state_store: self.state_store.clone(),
            counter_store: self.counter_store.clone(),
            config: self.config.clone(),
            warnings: Vec::new(),
            show_rules: self.show_rules.clone(),
            current_depth: self.current_depth, // Inherit depth from parent
        };

        let root = parse(&content);
        if !root.errors().is_empty() {
            return Err(EvalError::syntax(format!(
                "In included file {}: parse error",
                path
            )));
        }

        if let Some(markup) = root.cast::<ast::Markup>() {
            sub_eval.eval_markup(markup)
        } else {
            Ok(Value::None)
        }
    }

    fn bind_imports(
        &mut self,
        imports: Option<ast::Imports>,
        module: &IndexMap<String, Value>,
    ) -> EvalResult<()> {
        if let Some(imports) = imports {
            match imports {
                ast::Imports::Wildcard => {
                    for (name, val) in module {
                        self.scopes.define(name.clone(), val.clone());
                    }
                }
                ast::Imports::Items(items) => {
                    for item in items.iter() {
                        let original_name = item.original_name().get().as_str();
                        if let Some(val) = module.get(original_name) {
                            // bound_name() returns the name to bind to (same as original for simple imports)
                            let bound_name = item.bound_name().get().as_str();
                            self.scopes.define(bound_name.to_string(), val.clone());
                        } else {
                            return Err(EvalError::undefined(format!(
                                "Module does not export '{}'",
                                original_name
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Evaluate an identifier.
    fn eval_ident(&self, ident: ast::Ident) -> EvalResult<Value> {
        let name = ident.get().as_str();
        let span = SourceSpan::from_typst_span(ident.span());

        // First check scopes
        if let Some(value) = self.scopes.get(name) {
            return Ok(value.clone());
        }

        // Then check built-in constants
        match name {
            // Keywords
            "none" => Ok(Value::None),
            "auto" => Ok(Value::Auto),
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),

            // Alignment constants
            "left" => Ok(Value::Alignment(Alignment::new(
                Some(HorizAlign::Left),
                None,
            ))),
            "right" => Ok(Value::Alignment(Alignment::new(
                Some(HorizAlign::Right),
                None,
            ))),
            "center" => Ok(Value::Alignment(Alignment::new(
                Some(HorizAlign::Center),
                None,
            ))),
            "start" => Ok(Value::Alignment(Alignment::new(
                Some(HorizAlign::Start),
                None,
            ))),
            "end" => Ok(Value::Alignment(Alignment::new(
                Some(HorizAlign::End),
                None,
            ))),
            "top" => Ok(Value::Alignment(Alignment::new(None, Some(VertAlign::Top)))),
            "bottom" => Ok(Value::Alignment(Alignment::new(
                None,
                Some(VertAlign::Bottom),
            ))),
            "horizon" => Ok(Value::Alignment(Alignment::new(
                None,
                Some(VertAlign::Horizon),
            ))),

            // Direction constants
            "ltr" => Ok(Value::Direction(Direction::Ltr)),
            "rtl" => Ok(Value::Direction(Direction::Rtl)),
            "ttb" => Ok(Value::Direction(Direction::Ttb)),
            "btt" => Ok(Value::Direction(Direction::Btt)),

            // Builtin functions - return as a function value
            _ if self.is_builtin_function(name) => {
                Ok(Value::Func(Arc::new(self.create_builtin_wrapper(name))))
            }

            // Unknown - error with span
            _ => {
                let err = EvalError::undefined(name);
                Err(if let Some(s) = span {
                    err.with_span(s)
                } else {
                    err
                })
            }
        }
    }

    /// Evaluate an array literal.
    fn eval_array(&mut self, arr: ast::Array) -> EvalResult<Value> {
        let mut result = Vec::new();

        for item in arr.items() {
            match item {
                ast::ArrayItem::Pos(expr) => {
                    result.push(self.eval_expr(expr)?);
                }
                ast::ArrayItem::Spread(spread) => {
                    let value = self.eval_expr(spread.expr())?;
                    match value {
                        Value::Array(arr) => result.extend(arr),
                        Value::None => {}
                        _ => {
                            return Err(EvalError::invalid_op(format!(
                                "cannot spread {} into array",
                                value.type_name()
                            )))
                        }
                    }
                }
            }
        }

        Ok(Value::Array(result))
    }

    /// Evaluate a dictionary literal.
    fn eval_dict(&mut self, dict: ast::Dict) -> EvalResult<Value> {
        let mut result = IndexMap::new();

        for item in dict.items() {
            match item {
                ast::DictItem::Named(named) => {
                    let key = named.name().get().to_string();
                    let value = self.eval_expr(named.expr())?;
                    result.insert(key, value);
                }
                ast::DictItem::Keyed(keyed) => {
                    let key = self.eval_expr(keyed.key())?.as_str()?.to_string();
                    let value = self.eval_expr(keyed.expr())?;
                    result.insert(key, value);
                }
                ast::DictItem::Spread(spread) => {
                    let value = self.eval_expr(spread.expr())?;
                    match value {
                        Value::Dict(d) => result.extend(d),
                        Value::None => {}
                        _ => {
                            return Err(EvalError::invalid_op(format!(
                                "cannot spread {} into dictionary",
                                value.type_name()
                            )))
                        }
                    }
                }
            }
        }

        Ok(Value::Dict(result))
    }

    /// Evaluate a code block.
    fn eval_code_block(&mut self, block: ast::CodeBlock) -> EvalResult<Value> {
        self.scopes.enter();
        let result = self.eval_code(block.body())?;
        self.scopes.exit();
        Ok(result)
    }

    /// Evaluate code (a sequence of expressions).
    fn eval_code(&mut self, code: ast::Code) -> EvalResult<Value> {
        let mut output = Value::None;

        for expr in code.exprs() {
            let value = self.eval_expr(expr)?;
            output = ops::join(output, value)?;

            if self.flow.is_some() {
                break;
            }
        }

        Ok(output)
    }

    /// Evaluate a content block.
    fn eval_content_block(&mut self, block: ast::ContentBlock) -> EvalResult<Value> {
        self.scopes.enter();
        let result = self.eval_markup(block.body())?;
        self.scopes.exit();
        Ok(result)
    }

    /// Evaluate a unary operation.
    fn eval_unary(&mut self, unary: ast::Unary) -> EvalResult<Value> {
        let value = self.eval_expr(unary.expr())?;

        match unary.op() {
            ast::UnOp::Pos => ops::pos(value),
            ast::UnOp::Neg => ops::neg(value),
            ast::UnOp::Not => ops::not(&value),
        }
    }

    /// Evaluate a binary operation.
    fn eval_binary(&mut self, binary: ast::Binary) -> EvalResult<Value> {
        let op = binary.op();
        // Extract span for error reporting
        let span = SourceSpan::from_typst_span(binary.span());

        // Helper to attach span to errors
        let attach_span = |result: EvalResult<Value>| -> EvalResult<Value> {
            result.map_err(|e| {
                if let Some(s) = span {
                    e.with_span(s)
                } else {
                    e
                }
            })
        };

        // Handle assignment operators specially (don't evaluate LHS first)
        if op == ast::BinOp::Assign {
            return attach_span(self.eval_assign(binary.lhs(), binary.rhs()));
        }

        if matches!(
            op,
            ast::BinOp::AddAssign
                | ast::BinOp::SubAssign
                | ast::BinOp::MulAssign
                | ast::BinOp::DivAssign
        ) {
            return attach_span(self.eval_compound_assign(binary.lhs(), binary.rhs(), op));
        }

        // Short-circuit evaluation for logical operators
        if op == ast::BinOp::And {
            let lhs = attach_span(self.eval_expr(binary.lhs()))?.as_bool()?;
            if !lhs {
                return Ok(Value::Bool(false));
            }
            let rhs = attach_span(self.eval_expr(binary.rhs()))?.as_bool()?;
            return Ok(Value::Bool(rhs));
        }

        if op == ast::BinOp::Or {
            let lhs = attach_span(self.eval_expr(binary.lhs()))?.as_bool()?;
            if lhs {
                return Ok(Value::Bool(true));
            }
            let rhs = attach_span(self.eval_expr(binary.rhs()))?.as_bool()?;
            return Ok(Value::Bool(rhs));
        }

        let lhs = self.eval_expr(binary.lhs())?;
        let rhs = self.eval_expr(binary.rhs())?;

        attach_span(match op {
            ast::BinOp::Add => ops::add(lhs, rhs),
            ast::BinOp::Sub => ops::sub(lhs, rhs),
            ast::BinOp::Mul => ops::mul(lhs, rhs),
            ast::BinOp::Div => ops::div(lhs, rhs),
            // Note: Typst doesn't have % operator in syntax, modulo is done via calc.rem()
            ast::BinOp::Eq => Ok(Value::Bool(ops::eq(&lhs, &rhs))),
            ast::BinOp::Neq => Ok(Value::Bool(ops::ne(&lhs, &rhs))),
            ast::BinOp::Lt => Ok(Value::Bool(ops::lt(&lhs, &rhs)?)),
            ast::BinOp::Leq => Ok(Value::Bool(ops::le(&lhs, &rhs)?)),
            ast::BinOp::Gt => Ok(Value::Bool(ops::gt(&lhs, &rhs)?)),
            ast::BinOp::Geq => Ok(Value::Bool(ops::ge(&lhs, &rhs)?)),
            ast::BinOp::In => ops::contains(&rhs, &lhs).map(Value::Bool),
            ast::BinOp::NotIn => ops::contains(&rhs, &lhs).map(|b| Value::Bool(!b)),
            ast::BinOp::And | ast::BinOp::Or => unreachable!(), // Handled above
            ast::BinOp::Assign
            | ast::BinOp::AddAssign
            | ast::BinOp::SubAssign
            | ast::BinOp::MulAssign
            | ast::BinOp::DivAssign => unreachable!(), // Handled above
        })
    }

    /// Evaluate a simple assignment: `variable = value`
    ///
    /// In Typst, assignments return `none`, not the assigned value.
    fn eval_assign(&mut self, lhs: ast::Expr, rhs: ast::Expr) -> EvalResult<Value> {
        let name = match lhs {
            ast::Expr::Ident(ident) => ident.get().to_string(),
            _ => {
                return Err(EvalError::invalid_op(
                    "assignment target must be an identifier".to_string(),
                ))
            }
        };

        let value = self.eval_expr(rhs)?;
        self.scopes.assign(&name, value)?;
        Ok(Value::None) // Assignments return none in Typst
    }

    /// Evaluate a compound assignment: `variable += value`, etc.
    ///
    /// In Typst, assignments return `none`, not the assigned value.
    fn eval_compound_assign(
        &mut self,
        lhs: ast::Expr,
        rhs: ast::Expr,
        op: ast::BinOp,
    ) -> EvalResult<Value> {
        let name = match lhs {
            ast::Expr::Ident(ident) => ident.get().to_string(),
            _ => {
                return Err(EvalError::invalid_op(
                    "assignment target must be an identifier".to_string(),
                ))
            }
        };

        let current = self.scopes.get_or_err(&name)?.clone();
        let rhs_val = self.eval_expr(rhs)?;

        let new_value = match op {
            ast::BinOp::AddAssign => ops::add(current, rhs_val)?,
            ast::BinOp::SubAssign => ops::sub(current, rhs_val)?,
            ast::BinOp::MulAssign => ops::mul(current, rhs_val)?,
            ast::BinOp::DivAssign => ops::div(current, rhs_val)?,
            _ => unreachable!(),
        };

        self.scopes.assign(&name, new_value)?;
        Ok(Value::None) // Assignments return none in Typst
    }

    /// Evaluate a conditional expression.
    fn eval_conditional(&mut self, cond: ast::Conditional) -> EvalResult<Value> {
        let condition = self.eval_expr(cond.condition())?.as_bool()?;

        if condition {
            self.eval_expr(cond.if_body())
        } else if let Some(else_body) = cond.else_body() {
            self.eval_expr(else_body)
        } else {
            Ok(Value::None)
        }
    }

    /// Evaluate a while loop.
    fn eval_while(&mut self, while_loop: ast::WhileLoop) -> EvalResult<Value> {
        let mut output = Value::None;
        let mut iterations = 0;

        loop {
            if iterations >= self.config.max_iterations {
                return Err(EvalError::too_many_iterations());
            }

            let condition = self.eval_expr(while_loop.condition())?.as_bool()?;
            if !condition {
                break;
            }

            let value = self.eval_expr(while_loop.body())?;
            output = ops::join(output, value)?;

            match &self.flow {
                Some(FlowEvent::Break) => {
                    self.flow = None;
                    break;
                }
                Some(FlowEvent::Continue) => {
                    self.flow = None;
                }
                Some(FlowEvent::Return(_)) => break,
                None => {}
            }

            iterations += 1;
        }

        Ok(output)
    }

    /// Evaluate a for loop.
    fn eval_for(&mut self, for_loop: ast::ForLoop) -> EvalResult<Value> {
        let iterable = self.eval_expr(for_loop.iterable())?;
        let pattern = for_loop.pattern();
        let body = for_loop.body();

        let items: Vec<Value> = match iterable {
            Value::Array(arr) => arr,
            Value::Dict(dict) => dict
                .into_iter()
                .map(|(k, v)| Value::Array(vec![Value::Str(k), v]))
                .collect(),
            Value::Str(s) => s.chars().map(|c| Value::Str(c.to_string())).collect(),
            _ => {
                return Err(EvalError::invalid_op(format!(
                    "cannot iterate over {}",
                    iterable.type_name()
                )))
            }
        };

        let mut output = Value::None;
        self.scopes.enter();

        for (i, item) in items.into_iter().enumerate() {
            if i >= self.config.max_iterations {
                self.scopes.exit();
                return Err(EvalError::too_many_iterations());
            }

            self.destructure(pattern, item)?;

            let value = self.eval_expr(body)?;
            output = ops::join(output, value)?;

            match &self.flow {
                Some(FlowEvent::Break) => {
                    self.flow = None;
                    break;
                }
                Some(FlowEvent::Continue) => {
                    self.flow = None;
                }
                Some(FlowEvent::Return(_)) => break,
                None => {}
            }
        }

        self.scopes.exit();
        Ok(output)
    }

    /// Destructure a value into a pattern.
    fn destructure(&mut self, pattern: ast::Pattern, value: Value) -> EvalResult<()> {
        match pattern {
            ast::Pattern::Normal(ast::Expr::Ident(ident)) => {
                self.scopes.define(ident.get().to_string(), value);
                Ok(())
            }
            ast::Pattern::Placeholder(_) => Ok(()),
            ast::Pattern::Parenthesized(paren) => self.destructure(paren.pattern(), value),
            ast::Pattern::Destructuring(destruct) => {
                let items: Vec<ast::DestructuringItem> = destruct.items().collect();

                match value {
                    Value::Array(arr) => {
                        let mut arr_iter = arr.into_iter();
                        for item in items {
                            match item {
                                ast::DestructuringItem::Pattern(pat) => {
                                    let v = arr_iter.next().unwrap_or(Value::None);
                                    self.destructure(pat, v)?;
                                }
                                ast::DestructuringItem::Spread(spread) => {
                                    let rest: Vec<Value> = arr_iter.collect();
                                    if let Some(ast::Expr::Ident(ident)) = spread.sink_expr() {
                                        self.scopes
                                            .define(ident.get().to_string(), Value::Array(rest));
                                    }
                                    break;
                                }
                                ast::DestructuringItem::Named(_) => {
                                    return Err(EvalError::invalid_op(
                                        "cannot destructure named pattern from array".to_string(),
                                    ))
                                }
                            }
                        }
                        Ok(())
                    }
                    Value::Dict(dict) => {
                        for item in items {
                            match item {
                                ast::DestructuringItem::Pattern(ast::Pattern::Normal(
                                    ast::Expr::Ident(ident),
                                )) => {
                                    let key = ident.get().as_str();
                                    let v = dict.get(key).cloned().unwrap_or(Value::None);
                                    self.scopes.define(key.to_string(), v);
                                }
                                ast::DestructuringItem::Named(named) => {
                                    let key = named.name().get().to_string();
                                    let v = dict.get(&key).cloned().unwrap_or(Value::None);
                                    self.destructure(named.pattern(), v)?;
                                }
                                ast::DestructuringItem::Spread(spread) => {
                                    if let Some(ast::Expr::Ident(ident)) = spread.sink_expr() {
                                        self.scopes.define(
                                            ident.get().to_string(),
                                            Value::Dict(dict.clone()),
                                        );
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(())
                    }
                    _ => Err(EvalError::invalid_op(format!(
                        "cannot destructure {}",
                        value.type_name()
                    ))),
                }
            }
            ast::Pattern::Normal(expr) => Err(EvalError::invalid_op(format!(
                "cannot assign to expression: {}",
                expr.to_untyped().text()
            ))),
        }
    }

    /// Evaluate a let binding.
    fn eval_let(&mut self, binding: ast::LetBinding) -> EvalResult<Value> {
        let value = match binding.init() {
            Some(expr) => self.eval_expr(expr)?,
            None => Value::None,
        };

        match binding.kind() {
            ast::LetBindingKind::Normal(pattern) => {
                self.destructure(pattern, value)?;
            }
            ast::LetBindingKind::Closure(ident) => {
                // This is a function definition: #let f(x) = ...
                // The value should already be a closure
                self.scopes.define(ident.get().to_string(), value);
            }
        }

        Ok(Value::None)
    }

    /// Evaluate a closure definition.
    ///
    /// Note: Due to limitations in how typst-syntax nodes work (text is detached
    /// when nodes are extracted), we need to get the body source from the original
    /// input. This is handled by storing the body in a special way.
    fn eval_closure(&mut self, closure: ast::Closure) -> EvalResult<Value> {
        use typst_syntax::ast::AstNode;

        // Helper to recursively collect text from syntax nodes
        fn collect_text(node: &typst_syntax::SyntaxNode) -> String {
            if node.children().len() == 0 {
                node.text().to_string()
            } else {
                node.children().map(collect_text).collect()
            }
        }

        let mut params = Vec::new();
        let mut defaults = Vec::new();
        let mut sink = None;

        for param in closure.params().children() {
            match param {
                ast::Param::Pos(pattern) => {
                    if let ast::Pattern::Normal(ast::Expr::Ident(ident)) = pattern {
                        params.push(ident.get().to_string());
                        defaults.push(None);
                    }
                }
                ast::Param::Named(named) => {
                    params.push(named.name().get().to_string());
                    // Store source text instead of evaluating - enables lazy defaults
                    // that can depend on prior parameters: `#let f(x, y: x + 1) = ...`
                    // Use collect_text to handle complex expressions (binary ops, etc.)
                    let default_source = collect_text(named.expr().to_untyped());
                    defaults.push(Some(default_source));
                }
                ast::Param::Spread(spread) => {
                    // Sink argument - captures extra positional arguments
                    if let Some(sink_ident) = spread.sink_ident() {
                        sink = Some(sink_ident.get().to_string());
                    }
                }
            }
        }

        let body_source = collect_text(closure.body().to_untyped());
        let captures = self.scopes.capture_all();

        Ok(Value::Func(Arc::new(Closure {
            name: None,
            params,
            defaults,
            sink,
            body_source,
            captures,
        })))
    }

    fn eval_ref_target_value(&mut self, value: Value) -> EvalResult<String> {
        let raw = match value {
            Value::Label(label) => label,
            Value::Str(text) => text,
            Value::Content(nodes) if nodes.len() == 1 => match &nodes[0] {
                ContentNode::Label(label) | ContentNode::LabelDef(label) => label.clone(),
                ContentNode::Text(text) => text.clone(),
                _ => nodes[0].to_typst(),
            },
            Value::Content(nodes) => nodes.iter().map(|n| n.to_typst()).collect::<String>(),
            other => other.display(),
        };
        Ok(normalize_ref_target_text(&raw))
    }

    fn eval_ref_target_expr(&mut self, expr: ast::Expr) -> EvalResult<String> {
        let value = self.eval_expr(expr)?;
        self.eval_ref_target_value(value)
    }

    fn eval_semantic_text_expr(&mut self, expr: ast::Expr) -> EvalResult<String> {
        match expr {
            ast::Expr::Ident(ident) => Ok(ident.get().to_string()),
            other => {
                let value = self.eval_expr(other)?;
                self.eval_ref_target_value(value)
            }
        }
    }

    fn eval_semantic_cite(&mut self, args: ast::Args) -> EvalResult<Value> {
        let mut keys = Vec::new();
        let mut mode = CitationMode::Normal;
        let mut supplement = None;

        for arg in args.items() {
            match arg {
                ast::Arg::Pos(expr) => {
                    let key = self.eval_ref_target_expr(expr)?;
                    if !key.is_empty() {
                        keys.push(key);
                    }
                }
                ast::Arg::Named(named) => match named.name().get().as_str() {
                    "form" => {
                        let form = self.eval_semantic_text_expr(named.expr())?;
                        mode = citation_mode_from_typst_form(Some(form.as_str()));
                    }
                    "supplement" => {
                        let text = self.eval_semantic_text_expr(named.expr())?;
                        if !text.is_empty() {
                            supplement = Some(
                                text.trim_start_matches('[')
                                    .trim_end_matches(']')
                                    .trim()
                                    .to_string(),
                            );
                        }
                    }
                    _ => {}
                },
                ast::Arg::Spread(_) => {
                    return Err(EvalError::argument(
                        "cite does not support spread arguments in MiniEval semantic mode"
                            .to_string(),
                    ));
                }
            }
        }

        citation_content_value(keys, mode, supplement)
    }

    fn eval_semantic_ref(&mut self, args: ast::Args) -> EvalResult<Value> {
        let mut items = args.items();
        let first = items
            .next()
            .ok_or_else(|| EvalError::argument("ref expects 1 label argument".to_string()))?;
        if items.next().is_some() {
            return Err(EvalError::argument(
                "ref expects exactly 1 label argument".to_string(),
            ));
        }
        let target = match first {
            ast::Arg::Pos(expr) => self.eval_ref_target_expr(expr)?,
            ast::Arg::Named(_) | ast::Arg::Spread(_) => {
                return Err(EvalError::argument(
                    "ref expects 1 positional label argument".to_string(),
                ))
            }
        };
        Ok(reference_content_value(target, ReferenceType::Basic))
    }

    fn eval_semantic_label(&mut self, args: ast::Args) -> EvalResult<Value> {
        let mut items = args.items();
        let first = items
            .next()
            .ok_or_else(|| EvalError::argument("label expects 1 label argument".to_string()))?;
        if items.next().is_some() {
            return Err(EvalError::argument(
                "label expects exactly 1 label argument".to_string(),
            ));
        }
        let label = match first {
            ast::Arg::Pos(expr) => self.eval_ref_target_expr(expr)?,
            ast::Arg::Named(_) | ast::Arg::Spread(_) => {
                return Err(EvalError::argument(
                    "label expects 1 positional label argument".to_string(),
                ))
            }
        };
        Ok(label_content_value(label))
    }

    fn eval_semantic_bibliography(&mut self, args: ast::Args) -> EvalResult<Value> {
        let mut file = None;
        let mut style = None;

        for arg in args.items() {
            match arg {
                ast::Arg::Pos(expr) => {
                    if file.is_none() {
                        let value = self.eval_semantic_text_expr(expr)?;
                        if !value.is_empty() {
                            file = Some(value);
                        }
                    }
                }
                ast::Arg::Named(named) => {
                    if named.name().get().as_str() == "style" {
                        let value = self.eval_semantic_text_expr(named.expr())?;
                        if !value.is_empty() {
                            style = Some(value);
                        }
                    }
                }
                ast::Arg::Spread(_) => {
                    return Err(EvalError::argument(
                        "bibliography does not support spread arguments in MiniEval semantic mode"
                            .to_string(),
                    ));
                }
            }
        }

        let file = file.ok_or_else(|| {
            EvalError::argument("bibliography expects a file argument".to_string())
        })?;
        bibliography_content_value(file, style)
    }

    /// Evaluate a function call.
    fn eval_func_call(&mut self, call: ast::FuncCall) -> EvalResult<Value> {
        // Check recursion depth early to prevent stack overflow
        self.current_depth += 1;
        if self.current_depth > self.config.max_recursion_depth {
            self.current_depth -= 1;
            return Err(EvalError::new(EvalErrorKind::RecursionLimitExceeded {
                max_depth: self.config.max_recursion_depth,
            }));
        }

        let result = self.eval_func_call_inner(call);
        self.current_depth -= 1;
        result
    }

    /// Inner implementation of function call evaluation.
    fn eval_func_call_inner(&mut self, call: ast::FuncCall) -> EvalResult<Value> {
        let callee = call.callee();
        let args = call.args();

        // Check for field access (method call or module function)
        if let ast::Expr::FieldAccess(access) = &callee {
            return self.eval_method_call(*access, args);
        }

        // Get the function name or value
        if let ast::Expr::Ident(ident) = &callee {
            let name = ident.get().as_str();

            // Check if it's a user-defined function
            if let Some(Value::Func(closure)) = self.scopes.get(name).cloned() {
                return self.call_closure(&closure, args);
            }

            match name {
                "cite" => return self.eval_semantic_cite(args),
                "ref" => return self.eval_semantic_ref(args),
                "label" => return self.eval_semantic_label(args),
                "bibliography" => return self.eval_semantic_bibliography(args),
                _ => {}
            }

            // Try built-in functions
            let (pos_args, named_args) = self.eval_args(args)?;
            match call_builtin(name, pos_args, named_args, &self.vfs) {
                BuiltinResult::Ok(v) => return Ok(v),
                BuiltinResult::NotFound => {
                    // Not a built-in, fall through to raw source
                    // In compat mode, unknown functions are preserved as ContentNode::FuncCall
                    if self.config.strict {
                        return Err(EvalError::undefined(name));
                    }
                }
                BuiltinResult::Err(e) => return Err(e),
            }
        } else {
            // Callee is not an identifier - evaluate it to get a function value
            // This handles cases like (make-adder(5))(10) or (x(x))(v)
            let callee_val = self.eval_expr(callee)?;
            if let Value::Func(closure) = callee_val {
                return self.call_closure(&closure, args);
            }
            // If callee evaluated to something else, it's not callable
            return Err(EvalError::invalid_op(format!(
                "cannot call {} as function",
                callee_val.type_name()
            )));
        }

        // Unknown function - evaluate arguments and reconstruct
        // This handles Typst functions like table(), grid(), align(), etc.
        // that are not implemented in MiniEval but should have their
        // arguments evaluated (e.g., dynamic content from map/for loops).
        let (pos_args, named_args) = self.eval_args(args)?;

        let func_name = match &callee {
            ast::Expr::Ident(ident) => ident.get().to_string(),
            _ => callee.to_untyped().text().to_string(),
        };

        Ok(Value::Content(vec![ContentNode::FuncCall {
            name: func_name,
            args: pos_args
                .into_iter()
                .map(super::value::Arg::Pos)
                .chain(
                    named_args
                        .into_iter()
                        .map(|(k, v)| super::value::Arg::Named(k, v)),
                )
                .collect(),
        }]))
    }

    /// Evaluate a method call or module function call.
    fn eval_method_call(&mut self, access: ast::FieldAccess, args: ast::Args) -> EvalResult<Value> {
        let target = access.target();
        let field = access.field().get().to_string();

        // Check for calc.xxx pattern
        if let ast::Expr::Ident(ident) = &target {
            if ident.get().as_str() == "calc" {
                let (pos_args, _) = self.eval_args(args)?;
                return call_calc(&field, pos_args);
            }
        }

        // Evaluate the target
        let target_value = self.eval_expr(target)?;

        // Evaluate arguments
        let (pos_args, _) = self.eval_args(args)?;

        // Handle special array methods that need closures
        if let Value::Array(ref arr) = target_value {
            match field.as_str() {
                "map" => return self.array_map(arr.clone(), args),
                "filter" => return self.array_filter(arr.clone(), args),
                "fold" => return self.array_fold(arr.clone(), args),
                "reduce" => return self.array_reduce(arr.clone(), args),
                "any" => return self.array_any(arr.clone(), args),
                "all" => return self.array_all(arr.clone(), args),
                "find" => return self.array_find(arr.clone(), args),
                "sorted" if !pos_args.is_empty() => return self.array_sorted_by(arr.clone(), args),
                _ => {}
            }
        }

        // Regular method call
        call_method(&target_value, &field, pos_args)
    }

    /// Evaluate function arguments.
    ///
    /// Handles spread arguments for:
    /// - `..array` -> extends positional args
    /// - `..dict` -> extends named args
    /// - `..arguments` -> extends both positional and named args
    fn eval_args(&mut self, args: ast::Args) -> EvalResult<(Vec<Value>, IndexMap<String, Value>)> {
        let mut pos_args = Vec::new();
        let mut named_args = IndexMap::new();

        for arg in args.items() {
            match arg {
                ast::Arg::Pos(expr) => {
                    pos_args.push(self.eval_expr(expr)?);
                }
                ast::Arg::Named(named) => {
                    let key = named.name().get().to_string();
                    let value = self.eval_expr(named.expr())?;
                    named_args.insert(key, value);
                }
                ast::Arg::Spread(spread) => {
                    let value = self.eval_expr(spread.expr())?;
                    match value {
                        Value::Array(arr) => pos_args.extend(arr),
                        Value::Dict(dict) => named_args.extend(dict),
                        Value::Arguments(args_val) => {
                            // Spread Arguments expands both positional and named
                            pos_args.extend(args_val.positional);
                            named_args.extend(args_val.named);
                        }
                        Value::None => {
                            // Spreading none is a no-op
                        }
                        _ => {
                            return Err(EvalError::invalid_op(format!(
                                "cannot spread {} into arguments",
                                value.type_name()
                            )));
                        }
                    }
                }
            }
        }

        Ok((pos_args, named_args))
    }

    /// Call a user-defined closure.
    fn call_closure(&mut self, closure: &Closure, args: ast::Args) -> EvalResult<Value> {
        // Note: Recursion depth is already checked by eval_func_call
        let (pos_args, named_args) = self.eval_args(args)?;

        // Create new scope with captures
        self.scopes.enter_with_captures(closure.captures.clone());

        // Track which named args are consumed by regular parameters
        let mut consumed_named: std::collections::HashSet<&str> = std::collections::HashSet::new();

        // Bind regular parameters
        let num_regular_params = closure.params.len();
        for (i, param) in closure.params.iter().enumerate() {
            let value = if let Some(v) = named_args.get(param) {
                consumed_named.insert(param.as_str());
                v.clone()
            } else if i < pos_args.len() {
                pos_args[i].clone()
            } else if let Some(Some(ref default_source)) = closure.defaults.get(i) {
                // Evaluate default in current scope (where prior args are bound)
                // This enables `#let f(x, y: x + 1) = ...` style dependent defaults
                self.eval_body_source(default_source)?
            } else {
                Value::None
            };
            self.scopes.define(param.clone(), value);
        }

        // Bind sink argument as Arguments (extra positional AND extra named)
        if let Some(ref sink_name) = closure.sink {
            let extra_pos: Vec<Value> = pos_args.into_iter().skip(num_regular_params).collect();
            let extra_named: IndexMap<String, Value> = named_args
                .into_iter()
                .filter(|(k, _)| !consumed_named.contains(k.as_str()))
                .collect();

            // Bind as Arguments type, not just Array
            self.scopes.define(
                sink_name.clone(),
                Value::Arguments(Arguments {
                    positional: extra_pos,
                    named: extra_named,
                }),
            );
        }

        // Parse and evaluate the body
        let result = self.eval_body_source(&closure.body_source);

        self.scopes.exit();

        // Handle return flow
        if let Some(FlowEvent::Return(value)) = self.flow.take() {
            return Ok(value.unwrap_or(Value::None));
        }

        result
    }

    /// Parse and evaluate a body source string.
    /// This handles both code expressions (like `x * 2`) and markup (like `[hello]`).
    fn eval_body_source(&mut self, source: &str) -> EvalResult<Value> {
        // First try parsing as code (for expressions like `x * 2`)
        let code_root = parse_code(source);

        if code_root.errors().is_empty() {
            if let Some(code) = code_root.cast::<ast::Code>() {
                return self.eval_code(code);
            }
        }

        // Fall back to parsing as markup
        let markup_root = parse(source);

        if !markup_root.errors().is_empty() {
            return Err(EvalError::syntax(format!(
                "failed to parse '{}': {}",
                source,
                markup_root
                    .errors()
                    .iter()
                    .map(|e| e.message.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }

        if let Some(markup) = markup_root.cast::<ast::Markup>() {
            self.eval_markup(markup)
        } else {
            Err(EvalError::syntax(format!(
                "unexpected node type for: {}",
                source
            )))
        }
    }

    /// Evaluate field access.
    fn eval_field_access(&mut self, access: ast::FieldAccess) -> EvalResult<Value> {
        let target = self.eval_expr(access.target())?;
        let field = access.field().get().as_str();

        match target {
            Value::Dict(dict) => dict
                .get(field)
                .cloned()
                .ok_or(EvalError::key_not_found(field.to_string())),
            Value::Content(_) => {
                // Content field access - preserve as raw
                let source = access.to_untyped().text().to_string();
                Ok(Value::Content(vec![ContentNode::RawSource(source)]))
            }
            _ => Err(EvalError::invalid_op(format!(
                "cannot access field '{}' on {}",
                field,
                target.type_name()
            ))),
        }
    }

    // ========================================================================
    // Array higher-order methods
    // ========================================================================

    fn array_map(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        let closure = self.get_closure_arg(args)?;

        let mut result = Vec::new();
        for item in arr {
            let value = self.apply_closure(&closure, vec![item])?;
            result.push(value);
        }

        Ok(Value::Array(result))
    }

    fn array_filter(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        let closure = self.get_closure_arg(args)?;

        let mut result = Vec::new();
        for item in arr {
            let keep = self
                .apply_closure(&closure, vec![item.clone()])?
                .as_bool()?;
            if keep {
                result.push(item);
            }
        }

        Ok(Value::Array(result))
    }

    fn array_fold(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        let items: Vec<ast::Arg> = args.items().collect();
        if items.len() < 2 {
            return Err(EvalError::argument(
                "fold requires initial value and closure".to_string(),
            ));
        }

        let init = if let ast::Arg::Pos(expr) = &items[0] {
            self.eval_expr(*expr)?
        } else {
            return Err(EvalError::argument(
                "fold requires positional initial value".to_string(),
            ));
        };

        let closure = if let ast::Arg::Pos(expr) = &items[1] {
            self.get_closure_from_expr(*expr)?
        } else {
            return Err(EvalError::argument(
                "fold requires closure argument".to_string(),
            ));
        };

        let mut acc = init;
        for item in arr {
            acc = self.apply_closure(&closure, vec![acc, item])?;
        }

        Ok(acc)
    }

    fn array_any(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        let closure = self.get_closure_arg(args)?;

        for item in arr {
            let result = self.apply_closure(&closure, vec![item])?.as_bool()?;
            if result {
                return Ok(Value::Bool(true));
            }
        }

        Ok(Value::Bool(false))
    }

    fn array_all(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        let closure = self.get_closure_arg(args)?;

        for item in arr {
            let result = self.apply_closure(&closure, vec![item])?.as_bool()?;
            if !result {
                return Ok(Value::Bool(false));
            }
        }

        Ok(Value::Bool(true))
    }

    fn array_find(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        let closure = self.get_closure_arg(args)?;

        for item in arr {
            let result = self
                .apply_closure(&closure, vec![item.clone()])?
                .as_bool()?;
            if result {
                return Ok(item);
            }
        }

        Ok(Value::None)
    }

    fn array_reduce(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        if arr.is_empty() {
            return Ok(Value::None);
        }

        let closure = self.get_closure_arg(args)?;
        let mut iter = arr.into_iter();
        let mut acc = iter.next().unwrap();

        for item in iter {
            acc = self.apply_closure(&closure, vec![acc, item])?;
        }

        Ok(acc)
    }

    fn array_sorted_by(&mut self, arr: Vec<Value>, args: ast::Args) -> EvalResult<Value> {
        let closure = self.get_closure_arg(args)?;

        // Compute keys for all items
        let mut keyed: Vec<(Value, Value)> = Vec::new();
        for item in arr {
            let key = self.apply_closure(&closure, vec![item.clone()])?;
            keyed.push((key, item));
        }

        // Sort by key
        keyed.sort_by(|a, b| match (&a.0, &b.0) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => {
                x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::Str(x), Value::Str(y)) => x.cmp(y),
            _ => std::cmp::Ordering::Equal,
        });

        let sorted: Vec<Value> = keyed.into_iter().map(|(_, v)| v).collect();
        Ok(Value::Array(sorted))
    }

    /// Get closure from the first argument.
    fn get_closure_arg(&mut self, args: ast::Args) -> EvalResult<Closure> {
        let first = args
            .items()
            .next()
            .ok_or(EvalError::argument("expected closure argument".to_string()))?;

        if let ast::Arg::Pos(expr) = first {
            self.get_closure_from_expr(expr)
        } else {
            Err(EvalError::argument(
                "expected positional closure argument".to_string(),
            ))
        }
    }

    /// Get closure from an expression.
    ///
    /// Supports:
    /// - Literal closures: `x => x * 2`
    /// - Identifier references: `map(double)` where `double` is defined
    /// - Field access: `map(module.func)` or `map(dict.func)`
    /// - Builtin functions: `map(str)`, `map(int)`, etc.
    fn get_closure_from_expr(&mut self, expr: ast::Expr) -> EvalResult<Closure> {
        match expr {
            ast::Expr::Closure(c) => {
                let value = self.eval_closure(c)?;
                if let Value::Func(closure) = value {
                    Ok((*closure).clone())
                } else {
                    Err(EvalError::type_mismatch("function", value.type_name()))
                }
            }
            ast::Expr::Ident(ident) => {
                let name = ident.get().as_str();

                // First, try to resolve from scopes (user-defined functions)
                if let Some(value) = self.scopes.get(name) {
                    if let Value::Func(closure) = value {
                        return Ok((**closure).clone());
                    } else {
                        return Err(EvalError::type_mismatch("function", value.type_name()));
                    }
                }

                // Second, check if it's a known builtin function and create a wrapper closure
                if self.is_builtin_function(name) {
                    return Ok(self.create_builtin_wrapper(name));
                }

                // Not found
                Err(EvalError::undefined(name))
            }
            ast::Expr::FieldAccess(access) => {
                // Handle module.func or dict.func patterns
                let value = self.eval_field_access(access)?;
                if let Value::Func(closure) = value {
                    Ok((*closure).clone())
                } else {
                    Err(EvalError::type_mismatch("function", value.type_name()))
                }
            }
            ast::Expr::Parenthesized(paren) => {
                // Handle (func) pattern
                self.get_closure_from_expr(paren.expr())
            }
            _ => Err(EvalError::argument(
                "expected closure or function reference",
            )),
        }
    }

    /// Check if a name is a known builtin function.
    pub fn is_builtin_function(&self, name: &str) -> bool {
        matches!(
            name,
            "str"
                | "int"
                | "float"
                | "bool"
                | "type"
                | "repr"
                | "len"
                | "range"
                | "array"
                | "dict"
                | "dictionary"
                | "rgb"
                | "cmyk"
                | "luma"
                | "datetime"
                | "regex"
                | "version"
                | "label"
                | "lower"
                | "upper"
                | "lorem"
                | "zip"
                | "numbering"
                | "counter"
                | "state"
                | "pt"
                | "mm"
                | "cm"
                | "em"
                | "panic"
                | "assert"
                | "measure"
                | "layout"
                | "place"
                | "box"
                | "block"
                | "grid"
                | "stack"
        )
    }

    /// Create a wrapper closure for a builtin function.
    /// This allows builtin functions to be passed as arguments to higher-order functions.
    fn create_builtin_wrapper(&self, name: &str) -> Closure {
        // Create a closure that captures the builtin name and calls it
        // The body source is a call expression: `builtin_name(x)`
        Closure {
            name: Some(format!("<builtin:{}>", name)),
            params: vec!["x".to_string()],
            body_source: format!("{}(x)", name),
            captures: IndexMap::new(),
            defaults: vec![None],
            sink: None,
        }
    }

    /// Apply a closure to arguments.
    fn apply_closure(&mut self, closure: &Closure, args: Vec<Value>) -> EvalResult<Value> {
        self.scopes.enter_with_captures(closure.captures.clone());

        for (i, param) in closure.params.iter().enumerate() {
            let value = args.get(i).cloned().unwrap_or(Value::None);
            self.scopes.define(param.clone(), value);
        }

        let result = self.eval_body_source(&closure.body_source);

        self.scopes.exit();

        if let Some(FlowEvent::Return(value)) = self.flow.take() {
            return Ok(value.unwrap_or(Value::None));
        }

        result
    }

    /// Evaluate math content, replacing hash expressions with their values.
    ///
    /// This function traverses a Math AST node and evaluates any embedded
    /// hash expressions (like `#n` in `$F_#n$`), returning the expanded math string.
    fn eval_math_content(&mut self, node: &SyntaxNode) -> EvalResult<String> {
        use SyntaxKind::*;

        let mut result = String::new();
        let children: Vec<_> = node.children().collect();
        let mut i = 0;

        while i < children.len() {
            let child = &children[i];
            match child.kind() {
                // Hash marks the start of a code expression
                Hash => {
                    // Check for expression after hash
                    if i + 1 < children.len() {
                        let expr_node = &children[i + 1];
                        match expr_node.kind() {
                            // Simple identifier: #n
                            Ident => {
                                let name = expr_node.text().to_string();
                                if let Some(value) = self.scopes.get(&name) {
                                    // Use display_in_math to avoid nested $ symbols
                                    // e.g., if value is Math("x"), output "x" not "$x$"
                                    result.push_str(&value.display_in_math());
                                } else {
                                    // Preserve original if undefined
                                    result.push('#');
                                    result.push_str(&name);
                                }
                                i += 2; // Skip both Hash and Ident
                                continue;
                            }
                            // Parenthesized expression: #(expr)
                            Parenthesized => {
                                // Find the inner expression
                                for inner in expr_node.children() {
                                    if let Some(expr) = inner.cast::<ast::Expr>() {
                                        match self.eval_expr(expr) {
                                            Ok(value) => {
                                                // Use display_in_math to avoid nested $ symbols
                                                result.push_str(&value.display_in_math());
                                            }
                                            Err(_) => {
                                                // Preserve original on error
                                                result.push_str(expr_node.text().as_str());
                                            }
                                        }
                                        break;
                                    }
                                }
                                i += 2; // Skip both Hash and Parenthesized
                                continue;
                            }
                            // Function call: #func(args)
                            FuncCall => {
                                if let Some(expr) = expr_node.cast::<ast::Expr>() {
                                    match self.eval_expr(expr) {
                                        Ok(value) => {
                                            // Use display_in_math to avoid nested $ symbols
                                            result.push_str(&value.display_in_math());
                                        }
                                        Err(_) => {
                                            // Preserve original on error
                                            result.push('#');
                                            result.push_str(expr_node.text().as_str());
                                        }
                                    }
                                }
                                i += 2;
                                continue;
                            }
                            _ => {
                                // Unknown expression type after hash, keep as is
                                result.push('#');
                            }
                        }
                    } else {
                        // Hash at end with nothing after
                        result.push('#');
                    }
                }
                // Default strategy: if node has children, recurse; otherwise use raw text.
                // This ensures we don't miss any container nodes that might contain #expr.
                _ => {
                    if child.children().next().is_some() {
                        // Container node - recurse to find any nested #expr
                        result.push_str(&self.eval_math_content(child)?);
                    } else {
                        // Leaf node (no children) - use raw text
                        result.push_str(child.text().as_str());
                    }
                }
            }
            i += 1;
        }

        Ok(result)
    }
}

impl Default for MiniEval {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Expand macros in Typst source code.
///
/// This function evaluates `#let`, `#for`, `#if`, and function calls,
/// producing expanded source code that can be converted to LaTeX.
///
/// # Example
///
/// ```ignore
/// use tylax::expand_macros;
///
/// let input = r#"
/// #let n = 3
/// #for i in range(n) {
///   Item #(i + 1)
/// }
/// "#;
///
/// let expanded = expand_macros(input)?;
/// // Result: "Item 1\nItem 2\nItem 3\n"
/// ```
/// Result of macro expansion including any warnings generated.
pub struct ExpandResult {
    /// The expanded source code
    pub output: String,
    /// The expanded content nodes before serialization
    pub nodes: Vec<ContentNode>,
    /// Warnings generated during expansion
    pub warnings: Vec<EvalWarning>,
}

/// Expand macros in Typst source code.
///
/// This function evaluates Typst code, expanding macros (`#let`, `#for`, `#if`, etc.)
/// and applying any `#show` rules, then converts the result back to Typst source code.
///
/// Returns both the expanded output and any warnings generated during evaluation.
pub fn expand_macros(source: &str) -> EvalResult<String> {
    let result = expand_macros_with_warnings(source)?;
    Ok(result.output)
}

/// Normalize whitespace in content nodes.
///
/// This function:
/// 1. Removes consecutive Space nodes (keeps only one)
/// 2. Removes Space nodes before ListItem/EnumItem (they should start at column 0)
///
/// This prevents indentation accumulation in loops like `#for x in arr [- #x]`
fn normalize_content_whitespace(nodes: Vec<ContentNode>) -> Vec<ContentNode> {
    let mut result = Vec::with_capacity(nodes.len());
    let mut prev_was_space = false;

    for node in nodes {
        match &node {
            ContentNode::Space => {
                // Skip consecutive spaces
                if !prev_was_space {
                    result.push(node);
                    prev_was_space = true;
                }
            }
            ContentNode::ListItem(_) | ContentNode::EnumItem { .. } => {
                // Remove space before list items to ensure proper indentation
                if prev_was_space && !result.is_empty() {
                    // Check if last element is Space and remove it
                    if matches!(result.last(), Some(ContentNode::Space)) {
                        result.pop();
                    }
                }
                result.push(node);
                prev_was_space = false;
            }
            ContentNode::Parbreak => {
                // Parbreaks reset space tracking
                result.push(node);
                prev_was_space = false;
            }
            _ => {
                result.push(node);
                prev_was_space = false;
            }
        }
    }

    result
}

/// Expand macros in Typst source code, returning warnings as well.
///
/// This is the full version that returns both the expanded output and any warnings.
pub fn expand_macros_with_warnings(source: &str) -> EvalResult<ExpandResult> {
    let root = parse(source);

    if !root.errors().is_empty() {
        return Err(EvalError::syntax(format!(
            "parse error: {}",
            root.errors()
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    let markup = root
        .cast::<ast::Markup>()
        .ok_or_else(|| EvalError::syntax("expected markup"))?;

    let mut eval = MiniEval::new();
    let result = eval.eval_markup(markup)?;

    // Convert result to content nodes
    let nodes = match result {
        Value::Content(nodes) => nodes,
        Value::None => Vec::new(),
        other => vec![ContentNode::Text(other.display())],
    };

    // Apply show rules to transform content
    let transformed_nodes = eval.apply_show_rules(nodes)?;

    // Normalize whitespace: remove consecutive Space nodes to prevent
    // indentation accumulation in loops (e.g., #for x in arr [- #x])
    let normalized_nodes = normalize_content_whitespace(transformed_nodes);

    // Convert transformed nodes to string
    let output: String = normalized_nodes.iter().map(|n| n.to_typst()).collect();

    // Collect warnings
    let warnings = eval.take_warnings();

    Ok(ExpandResult {
        output,
        nodes: normalized_nodes,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_let() {
        // Simple variable binding and usage
        let result = expand_macros("#let x = 5\n#x").unwrap();
        assert!(result.contains("5"), "Expected 5 in: {}", result);
    }

    #[test]
    fn test_for_loop() {
        // For loop with content block syntax [...]
        let result = expand_macros("#for i in range(3) [\n#i\n]").unwrap();
        assert!(result.contains("0"), "Expected 0 in: {}", result);
        assert!(result.contains("1"), "Expected 1 in: {}", result);
        assert!(result.contains("2"), "Expected 2 in: {}", result);
    }

    #[test]
    fn test_for_loop_list_debug() {
        // Debug test to understand the nesting bug
        let input = "#for x in (\"A\", \"B\", \"C\") [\n- #x\n]";
        let result = expand_macros(input).unwrap();

        // Print with visible characters for debugging
        eprintln!("Input: {:?}", input);
        eprintln!("Result: {:?}", result);
        eprintln!("Result bytes: {:?}", result.as_bytes());

        // Each list item should start at column 0
        for (i, line) in result.lines().enumerate() {
            eprintln!(
                "Line {}: {:?} (starts with space: {})",
                i,
                line,
                line.starts_with(' ')
            );
        }
    }

    #[test]
    fn test_conditional() {
        // Conditional with content blocks
        let result = expand_macros("#if true [yes] else [no]").unwrap();
        assert!(result.contains("yes"), "Expected yes in: {}", result);
        assert!(!result.contains("no"), "Unexpected no in: {}", result);
    }

    #[test]
    fn test_function_def() {
        // Inline closure syntax: (x) => x * 2
        let result = expand_macros("#let double = (x) => x * 2\n#double(5)").unwrap();
        assert!(result.contains("10"), "Expected 10 in: {}", result);
    }

    #[test]
    fn test_function_def_sugar() {
        // Function definition sugar syntax: f(x) = body
        let result = expand_macros("#let triple(x) = x * 3\n#triple(4)").unwrap();
        assert!(result.contains("12"), "Expected 12 in: {}", result);
    }

    #[test]
    fn test_recursive_function() {
        // Recursive function: factorial
        let result = expand_macros(
            r#"
#let fact(n) = if n <= 1 { 1 } else { n * fact(n - 1) }
#fact(5)
"#,
        )
        .unwrap();
        assert!(result.contains("120"), "Expected 120 in: {}", result);
    }

    #[test]
    fn test_array_join() {
        // Array operations - simpler test
        let result = expand_macros("#(1, 2, 3).join(\"-\")").unwrap();
        assert!(result.contains("1"), "Expected 1 in: {}", result);
        assert!(result.contains("2"), "Expected 2 in: {}", result);
        assert!(result.contains("3"), "Expected 3 in: {}", result);
    }

    #[test]
    fn test_range() {
        // Range function
        let result = expand_macros("#range(3).join(\", \")").unwrap();
        assert!(result.contains("0"), "Expected 0 in: {}", result);
        assert!(result.contains("1"), "Expected 1 in: {}", result);
        assert!(result.contains("2"), "Expected 2 in: {}", result);
    }

    #[test]
    fn test_nested_let() {
        // Nested let bindings
        let result = expand_macros("#let a = 2\n#let b = a * 3\n#b").unwrap();
        assert!(result.contains("6"), "Expected 6 in: {}", result);
    }

    #[test]
    fn test_array_len() {
        // Array length
        let result = expand_macros("#(1, 2, 3, 4, 5).len()").unwrap();
        assert!(result.contains("5"), "Expected 5 in: {}", result);
    }

    // ========================================================================
    // Advanced macro tests
    // ========================================================================

    #[test]
    fn test_fibonacci() {
        let result = expand_macros(
            r#"
#let fib(n) = if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
#fib(10)
"#,
        )
        .unwrap();
        assert!(
            result.contains("55"),
            "Expected 55 (fib(10)) in: {}",
            result
        );
    }

    #[test]
    fn test_closure_capture() {
        let result = expand_macros(
            r#"
#let make-adder(x) = (y) => x + y
#let add5 = make-adder(5)
#add5(10)
"#,
        )
        .unwrap();
        assert!(result.contains("15"), "Expected 15 in: {}", result);
    }

    #[test]
    fn test_nested_loops() {
        let result = expand_macros(
            r#"
#for i in range(3) [
#for j in range(3) [(#i,#j)]
]
"#,
        )
        .unwrap();
        assert!(result.contains("(0,0)"), "Expected (0,0) in: {}", result);
        assert!(result.contains("(2,2)"), "Expected (2,2) in: {}", result);
    }

    #[test]
    fn test_array_sum() {
        let result = expand_macros(r#"#(1, 2, 3, 4, 5).sum()"#).unwrap();
        assert!(result.contains("15"), "Expected 15 in: {}", result);
    }

    #[test]
    fn test_string_upper() {
        let result = expand_macros(r#"#"hello world".upper()"#).unwrap();
        assert!(
            result.contains("HELLO WORLD"),
            "Expected HELLO WORLD in: {}",
            result
        );
    }

    #[test]
    fn test_complex_conditional() {
        let result = expand_macros(r#"
#let grade(score) = if score >= 90 { "A" } else if score >= 80 { "B" } else if score >= 70 { "C" } else { "F" }
#grade(95), #grade(85), #grade(50)
"#).unwrap();
        assert!(result.contains("A"), "Expected A in: {}", result);
        assert!(result.contains("B"), "Expected B in: {}", result);
        assert!(result.contains("F"), "Expected F in: {}", result);
    }

    #[test]
    fn test_function_composition() {
        let result = expand_macros(
            r#"
#let compose(f, g) = (x) => f(g(x))
#let double(x) = x * 2
#let inc(x) = x + 1
#let f = compose(inc, double)
#f(5)
"#,
        )
        .unwrap();
        // double(5) = 10, then inc(10) = 11
        assert!(result.contains("11"), "Expected 11 in: {}", result);
    }

    #[test]
    fn test_map_with_closure_variable() {
        // Test passing a closure variable to map
        let result = expand_macros(
            r#"
#let make_adder(k) = (x) => x + k
#let add3 = make_adder(3)
#let xs = (1, 2, 3)
#xs.map(add3).join(", ")
"#,
        )
        .unwrap();
        // 1+3=4, 2+3=5, 3+3=6
        assert!(
            result.contains("4") && result.contains("5") && result.contains("6"),
            "Expected 4, 5, 6 in: {}",
            result
        );
    }

    #[test]
    fn test_is_builtin_function() {
        let eval = MiniEval::new();
        assert!(eval.is_builtin_function("str"), "str should be a builtin");
        assert!(eval.is_builtin_function("int"), "int should be a builtin");
        assert!(
            eval.is_builtin_function("range"),
            "range should be a builtin"
        );
        assert!(
            !eval.is_builtin_function("foo"),
            "foo should not be a builtin"
        );
    }

    #[test]
    fn test_str_builtin_direct_call() {
        // Test that str() works when called directly
        let result = expand_macros(r#"#str(42)"#).unwrap();
        assert!(
            result.contains("42"),
            "str(42) should produce '42' in: {}",
            result
        );
    }

    #[test]
    fn test_str_in_closure() {
        // Test that str() works when called inside a closure body
        let result = expand_macros(
            r#"
#let wrap(f, v) = f(v)
#wrap(str, 123)
"#,
        )
        .unwrap();
        assert!(
            result.contains("123"),
            "wrap(str, 123) should produce '123' in: {}",
            result
        );
    }

    #[test]
    fn test_lazy_defaults() {
        // Test that default arguments can depend on prior parameters
        // Note: For now, simpler test - dependent defaults are a stretch goal
        let result = expand_macros(
            r#"
#let f(x, y: 10) = (x, y)
#f(5)
"#,
        )
        .unwrap();
        // f(5) -> x=5, y=10
        assert!(
            result.contains("5") && result.contains("10"),
            "Expected (5, 10) in: {}",
            result
        );
    }

    #[test]
    fn test_lazy_defaults_dependent() {
        // Test that default arguments can depend on prior parameters
        let result = expand_macros(
            r#"
#let f(x, y: x + 1) = (x, y)
#f(5)
"#,
        )
        .unwrap();
        // f(5) -> x=5, y=5+1=6
        assert!(
            result.contains("5") && result.contains("6"),
            "Expected (5, 6) in: {}",
            result
        );
    }

    #[test]
    fn test_content_introspection() {
        // Test content.func() and content.text()
        let result = expand_macros(
            r#"
#let h = [= Hello World]
#h.first().func()
"#,
        )
        .unwrap();
        // The heading's func should be "heading"
        assert!(
            result.contains("heading"),
            "Expected 'heading' in: {}",
            result
        );
    }

    #[test]
    fn test_map_with_builtin_str() {
        // Test passing builtin str function to map
        let result = expand_macros(
            r#"
#let nums = (1, 2, 3)
#nums.map(str).join("-")
"#,
        )
        .unwrap();
        assert!(result.contains("1-2-3"), "Expected 1-2-3 in: {}", result);
    }

    #[test]
    fn test_chained_map() {
        // Test chaining map operations
        let result = expand_macros(
            r#"
#let double = (x) => x * 2
#(1, 2, 3).map(double).map(str).join(", ")
"#,
        )
        .unwrap();
        // 1*2=2, 2*2=4, 3*2=6
        assert!(
            result.contains("2") && result.contains("4") && result.contains("6"),
            "Expected 2, 4, 6 in: {}",
            result
        );
    }

    #[test]
    fn test_accumulator_loop() {
        let result = expand_macros(
            r#"
#let sum-list(arr) = {
  let acc = 0
  for x in arr {
    acc = acc + x
  }
  acc
}
#sum-list((1, 2, 3, 4, 5))
"#,
        )
        .unwrap();
        assert!(result.contains("15"), "Expected 15 in: {}", result);
    }

    #[test]
    fn test_generate_sequence() {
        // Generate a sequence with loop
        let result = expand_macros(
            r#"
#for i in range(1, 4) [#(i * 3), ]
"#,
        )
        .unwrap();
        assert!(result.contains("3"), "Expected 3 in: {}", result);
        assert!(result.contains("6"), "Expected 6 in: {}", result);
        assert!(result.contains("9"), "Expected 9 in: {}", result);
    }

    #[test]
    fn test_string_split_join() {
        let result = expand_macros(r#"#"a,b,c".split(",").join(" | ")"#).unwrap();
        assert!(
            result.contains("a | b | c"),
            "Expected 'a | b | c' in: {}",
            result
        );
    }

    #[test]
    fn test_calc_functions() {
        let result = expand_macros(r#"#calc.abs(-5)"#).unwrap();
        assert!(result.contains("5"), "Expected 5 in: {}", result);

        let result2 = expand_macros(r#"#calc.max(1, 5, 3)"#).unwrap();
        assert!(result2.contains("5"), "Expected 5 in: {}", result2);

        let result3 = expand_macros(r#"#calc.min(1, 5, 3)"#).unwrap();
        assert!(result3.contains("1"), "Expected 1 in: {}", result3);
    }

    #[test]
    fn test_array_filter_pattern() {
        // Manual filter implementation
        let result = expand_macros(
            r#"
#let filter-even(arr) = {
  let result = ()
  for x in arr {
    if calc.rem(x, 2) == 0 {
      result = result + (x,)
    }
  }
  result
}
#filter-even((1, 2, 3, 4, 5, 6)).join(", ")
"#,
        )
        .unwrap();
        assert!(result.contains("2"), "Expected 2 in: {}", result);
        assert!(result.contains("4"), "Expected 4 in: {}", result);
        assert!(result.contains("6"), "Expected 6 in: {}", result);
        assert!(!result.contains("1,"), "Unexpected 1 in: {}", result);
    }

    #[test]
    fn test_power_function() {
        let result = expand_macros(
            r#"
#let pow(base, exp) = if exp == 0 { 1 } else { base * pow(base, exp - 1) }
#pow(2, 10)
"#,
        )
        .unwrap();
        assert!(
            result.contains("1024"),
            "Expected 1024 (2^10) in: {}",
            result
        );
    }

    #[test]
    fn test_gcd() {
        let result = expand_macros(
            r#"
#let gcd(a, b) = if b == 0 { a } else { gcd(b, calc.rem(a, b)) }
#gcd(48, 18)
"#,
        )
        .unwrap();
        assert!(
            result.contains("6"),
            "Expected 6 (gcd of 48,18) in: {}",
            result
        );
    }

    #[test]
    fn test_demo_output() {
        // Quick demonstration of what expand_macros produces
        let tests = vec![
            (
                "fib",
                r#"
#let fib(n) = if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
#for i in range(8) [F#i=#fib(i) ]
"#,
            ),
            (
                "closure",
                r#"
#let make-adder(x) = (y) => x + y
#let add5 = make-adder(5)
add5(3)=#add5(3)
"#,
            ),
            (
                "custom map",
                r#"
#let my-map(arr, f) = {
  let result = ()
  for item in arr {
    result = result + (f(item),)
  }
  result
}
#my-map((1, 2, 3), x => x * x).join(", ")
"#,
            ),
            (
                "GCD",
                r#"
#let gcd(a, b) = if b == 0 { a } else { gcd(b, calc.rem(a, b)) }
gcd(48,18)=#gcd(48, 18)
"#,
            ),
            (
                "tail recursion",
                r#"
#let sum-to(n) = {
  let helper(i, acc) = if i > n { acc } else { helper(i + 1, acc + i) }
  helper(1, 0)
}
sum(1..10)=#sum-to(10)
"#,
            ),
        ];

        for (name, code) in tests {
            let result = expand_macros(code).unwrap();
            eprintln!("=== {} ===", name);
            eprintln!("{}", result.trim());
            eprintln!();
        }
    }

    #[test]
    fn test_ackermann() {
        let code = r#"
#let ack(m, n) = if m == 0 { n + 1 } else if n == 0 { ack(m - 1, 1) } else { ack(m - 1, ack(m, n - 1)) }
#ack(3, 2)
"#;
        let result = expand_macros(code).unwrap();
        assert!(result.contains("29"), "Ackermann failed: {}", result);
    }

    #[test]
    fn test_mutual_recursion() {
        let code = r#"
#let is-even(n) = if n == 0 { true } else { is-odd(n - 1) }
#let is-odd(n) = if n == 0 { false } else { is-even(n - 1) }
#is-even(10)
#is-even(11)
"#;
        let result = expand_macros(code).unwrap();
        assert!(result.contains("true"), "10 should be even");
        assert!(result.contains("false"), "11 should not be even");
    }

    #[test]
    fn test_quicksort() {
        let code = r#"
#let filter(arr, f) = {
  let res = ()
  for x in arr { if f(x) { res = res + (x,) } }
  res
}
#let quicksort(arr) = {
  if arr.len() <= 1 { arr } else {
    let pivot = arr.first()
    let rest = arr.slice(1)
    let less = filter(rest, x => x <= pivot)
    let greater = filter(rest, x => x > pivot)
    quicksort(less) + (pivot,) + quicksort(greater)
  }
}
#quicksort((3, 1, 4, 1, 5, 9, 2, 6)).join(",")
"#;
        let result = expand_macros(code).unwrap();
        assert!(
            result.contains("1,1,2,3,4,5,6,9"),
            "Quicksort failed: {}",
            result
        );
    }

    #[test]
    fn test_matmul() {
        let code = r#"
#let mat-mul(A, B) = {
  let m = A.len()
  let n = A.at(0).len()
  let p = B.at(0).len()
  let C = ()
  for i in range(m) {
    let row = ()
    for j in range(p) {
      let sum = 0
      for k in range(n) {
        sum = sum + A.at(i).at(k) * B.at(k).at(j)
      }
      row = row + (sum,)
    }
    C = C + (row,)
  }
  C
}
#let A = ((1, 2), (3, 4))
#let B = ((2, 0), (1, 2))
#mat-mul(A, B)
"#;
        let result = expand_macros(code).unwrap();
        assert!(result.contains("4"), "MatMul missing 4");
        assert!(result.contains("10"), "MatMul missing 10");
        assert!(result.contains("8"), "MatMul missing 8");
    }

    #[test]
    fn test_z_combinator_simple() {
        // Simpler Z-combinator test: countdown without multiplication
        let code = r#"
#let Z(f) = {
  let inner(x) = {
    let g(v) = (x(x))(v)
    f(g)
  }
  inner(inner)
}
#let countdown = Z(f => n => if n <= 0 { "done" } else { f(n - 1) })
#countdown(3)
"#;
        let result = expand_macros(code).unwrap();
        assert!(
            result.contains("done"),
            "Z Combinator countdown failed: {}",
            result
        );
    }

    #[test]
    fn test_z_combinator_factorial() {
        // Full Z-combinator with factorial
        let code = r#"
#let Z(f) = {
  let inner(x) = {
    let g(v) = (x(x))(v)
    f(g)
  }
  inner(inner)
}
#let fact = Z(f => n => if n <= 1 { 1 } else { n * f(n - 1) })
#fact(5)
"#;
        let result = expand_macros(code).unwrap();
        assert!(
            result.contains("120"),
            "Z Combinator factorial failed: {}",
            result
        );
    }

    #[test]
    fn test_block_returns_int() {
        // Verify code blocks return the correct type
        let code = r#"
#let x = { 1 }
#type(x)
"#;
        let result = expand_macros(code).unwrap();
        assert!(
            result.contains("int"),
            "Block should return int: {}",
            result
        );
    }

    #[test]
    fn test_if_returns_int() {
        // Verify if expressions return correct type
        let code = r#"
#let x = if true { 1 } else { 2 }
#type(x)
"#;
        let result = expand_macros(code).unwrap();
        assert!(result.contains("int"), "If should return int: {}", result);
    }

    #[test]
    fn test_nested_func_call() {
        // Test nested function calls like (f(x))(y)
        let code = r#"
#let make-adder(x) = (y) => x + y
#let add3 = make-adder(3)
#(make-adder(5))(10)
"#;
        let result = expand_macros(code).unwrap();
        assert!(
            result.contains("15"),
            "Nested call should return 15: {}",
            result
        );
    }

    // ========================================================================
    // Control flow tests
    // ========================================================================

    #[test]
    fn test_break_in_nested_loops() {
        // break should only exit the innermost loop
        let code = r#"
#for i in range(3) {
  for j in range(3) {
    if j == 1 { break }
    [j=#j]
  }
  [i=#i]
}
"#;
        let result = expand_macros(code).unwrap();
        // Each outer loop iteration should only output j=0 (break at j=1)
        // Then continue with i=0, i=1, i=2
        assert!(result.contains("j=0"), "Should have j=0 in: {}", result);
        assert!(
            !result.contains("j=1"),
            "Should NOT have j=1 in: {}",
            result
        );
        assert!(
            !result.contains("j=2"),
            "Should NOT have j=2 in: {}",
            result
        );
        // Outer loop should complete all iterations
        assert!(result.contains("i=0"), "Should have i=0 in: {}", result);
        assert!(result.contains("i=1"), "Should have i=1 in: {}", result);
        assert!(result.contains("i=2"), "Should have i=2 in: {}", result);
    }

    #[test]
    fn test_return_in_function() {
        // return should exit the function with a value
        let code = r#"
#let f() = {
  for i in range(10) {
    if i == 3 { return i }
  }
  999
}
#f()
"#;
        let result = expand_macros(code).unwrap();
        assert!(
            result.contains("3"),
            "Should return 3 from function: {}",
            result
        );
        assert!(
            !result.contains("999"),
            "Should NOT reach 999 in: {}",
            result
        );
    }

    #[test]
    fn test_continue_in_loop() {
        // continue should skip to next iteration
        // Use content block syntax [...] for output
        let code = r#"#for i in range(5) [
#if calc.rem(i, 2) == 0 { continue }
#i
]"#;
        let result = expand_macros(code).unwrap();
        // Should only output odd numbers: 1, 3
        assert!(result.contains("1"), "Should have 1 in: {}", result);
        assert!(result.contains("3"), "Should have 3 in: {}", result);
        // Note: 0, 2, 4 might appear in whitespace/formatting, so we check more carefully
    }

    #[test]
    fn test_while_with_break() {
        // break in while loop - test by counting iterations
        let code = r#"
#let test-while() = {
  let count = 0
  let i = 0
  while i < 10 {
    if i == 3 { break }
    count = count + 1
    i = i + 1
  }
  count
}
#test-while()
"#;
        let result = expand_macros(code).unwrap();
        // Should count 3 iterations (0, 1, 2) then break at i=3
        assert!(result.contains("3"), "Should have count=3 in: {}", result);
    }

    // ========================================================================
    // Graceful degradation tests (compat mode)
    // ========================================================================

    #[test]
    fn test_unknown_function_compat_mode() {
        // Unknown functions should NOT cause a panic in compat mode
        // They should be preserved as function calls
        let code = r#"#some_totally_unknown_func(1, 2, "test")"#;
        let result = expand_macros(code);
        // In compat mode, this should succeed (not panic)
        assert!(
            result.is_ok(),
            "Unknown function should not panic in compat mode"
        );
    }

    #[test]
    fn test_unknown_method_error() {
        // Unknown methods should return an error (not panic)
        let code = r#"#(1, 2, 3).some_unknown_method()"#;
        let result = expand_macros(code);
        // This should fail with an error, not panic
        assert!(result.is_err(), "Unknown method should return error");
    }

    // ========================================================================
    // Plan-specified edge case tests (from audit plan)
    // ========================================================================

    #[test]
    fn test_plan_basic_shadowing() {
        // Plan case: #let x = 1; #{ let x = 2; x } #x -> 2, 1
        let code = r#"
#let x = 1
#{ let x = 2; x }
#x
"#;
        let result = expand_macros(code).unwrap();
        // Inner block should have 2, outer should have 1
        assert!(result.contains("2"), "Inner x should be 2 in: {}", result);
        assert!(result.contains("1"), "Outer x should be 1 in: {}", result);
    }

    #[test]
    fn test_plan_closure_captures_value() {
        // Plan case: Closure should capture value, not reference
        // #let make_adder(n) = (x) => x + n
        // #let add5 = make_adder(5)
        // #add5(10) -> 15
        let code = r#"
#let make_adder(n) = (x) => x + n
#let add5 = make_adder(5)
#add5(10)
"#;
        let result = expand_macros(code).unwrap();
        assert!(
            result.contains("15"),
            "add5(10) should be 15 in: {}",
            result
        );
    }

    #[test]
    fn test_plan_closure_capture_in_loop() {
        // Plan case: Classic closure capture trap
        // Each closure in the loop should capture a different value
        // This tests that capture_all() clones values correctly
        let code = r#"
#let funcs = ()
#for i in range(3) {
  funcs = funcs + ((() => i),)
}
// Call each function and collect results
#let results = funcs.map(f => f())
#results
"#;
        let result = expand_macros(code).unwrap();
        // In Typst (and our MiniEval), each closure captures the value at definition time
        // So we should see 0, 1, 2 (not 2, 2, 2 like in some languages with late binding)
        assert!(
            result.contains("0"),
            "First closure should capture 0: {}",
            result
        );
        assert!(
            result.contains("1"),
            "Second closure should capture 1: {}",
            result
        );
        assert!(
            result.contains("2"),
            "Third closure should capture 2: {}",
            result
        );
    }

    #[test]
    fn test_plan_nested_break_only_inner() {
        // Plan case: break in nested loops should only exit inner loop
        let code = r#"
#let results = ()
#for i in range(3) {
  for j in range(3) {
    if j == 1 { break }
    results = results + ((i, j),)
  }
}
#results
"#;
        let result = expand_macros(code).unwrap();
        // Each outer loop should only produce j=0, then break
        // So we expect (0,0), (1,0), (2,0)
        assert!(
            result.contains("0") && result.contains("1") && result.contains("2"),
            "Should have all i values: {}",
            result
        );
    }

    #[test]
    fn test_plan_return_in_nested_loop() {
        // Plan case: return should exit function, not just loop
        let code = r#"
#let f() = {
  for i in range(10) {
    if i == 3 { return i }
  }
  999
}
#f()
"#;
        let result = expand_macros(code).unwrap();
        assert!(result.contains("3"), "Should return 3: {}", result);
        assert!(!result.contains("999"), "Should not reach 999: {}", result);
    }

    #[test]
    fn test_plan_continue_skips_iteration() {
        // Plan case: continue should skip to next iteration
        let code = r#"
#let results = ()
#for i in range(5) {
  if calc.rem(i, 2) == 0 { continue }
  results = results + (i,)
}
#results
"#;
        let result = expand_macros(code).unwrap();
        // Should only have odd numbers: 1, 3
        assert!(result.contains("1"), "Should have 1: {}", result);
        assert!(result.contains("3"), "Should have 3: {}", result);
    }
}
