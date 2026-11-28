//! The MiniEval interpreter engine.
//!
//! This module implements the core evaluation logic for Typst macros,
//! including expression evaluation, control flow, and function calls.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use crate::features::refs::{citation_mode_from_typst_form, CitationMode, ReferenceType};
use indexmap::IndexMap;

use typst_syntax::ast::{self, AstNode};
use typst_syntax::{parse, parse_code, SyntaxKind, SyntaxNode};
use unicode_segmentation::UnicodeSegmentation;

use super::library::{call_builtin, call_calc, call_method, BuiltinResult};
use super::ops;
use super::scope::Scopes;
use super::value::{
    bibliography_content_value, citation_content_value, has_explicit_field, label_content_value,
    mark_explicit_fields, normalize_ref_target_text, reference_content_value, Alignment, Arg,
    Arguments, Closure, ContentNode, Direction, EvalError, EvalErrorKind, EvalResult, HorizAlign,
    Length, LengthUnit, MathSegment, Selector, ShowRule, ShowSetStyle, SourceSpan, Value,
    VertAlign, ENUM_CONTAINER_NAME, EXPLICIT_NUMBERING_ARG, LABELED_ELEMENT_NAME,
    LIST_CONTAINER_NAME, TERMS_CONTAINER_NAME,
};
use super::vfs::{NoopVfs, VirtualFileSystem};

/// Maximum number of loop iterations (infinite loop protection).
const MAX_ITERATIONS: usize = 10_000;
/// Maximum nested/chained show-rule application depth.
const MAX_SHOW_DEPTH: usize = 64;
/// Maximum nested module `#import`/`#include` depth.
const MAX_IMPORT_DEPTH: usize = 8;
/// Maximum cumulative bytes read across nested `#import`/`#include` files.
const MAX_TOTAL_IMPORT_BYTES: usize = 4 * 1024 * 1024;
/// Internal capture key storing a closure's defining VFS file path without breaking `pub struct Closure`.
const CLOSURE_FILE_CAPTURE_KEY: &str = "\0tylax_module_file";

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
    /// Scoped `#set` style state
    styles: EvalStyles,
    /// Entry file virtual path (to catch indirect cycles back to the entry file)
    root_file: Option<String>,
    /// Current recursion depth (for infinite recursion protection)
    current_depth: usize,
    /// Internal marker spliced into shadowed `llap`-shaped `#context` nodes.
    shadowed_context_marker: Option<String>,
    /// Active nested module `#import`/`#include` stack (logical virtual paths)
    import_stack: Vec<String>,
    /// Shared byte budget remaining across `#import`/`#include` evaluations
    import_bytes_remaining: Arc<AtomicUsize>,
    /// Monotonic counter for unique hyperref-safe duplicate element IDs.
    next_dup_id: usize,
}

/// Scoped `#set` heading numbering state tracked by `MiniEval`.
#[derive(Debug, Clone, PartialEq, Default)]
enum HeadingNumbering {
    #[default]
    Default,
    Numbered(Value),
    Unnumbered,
}

/// Scoped `#set` style properties tracked by `MiniEval`.
#[derive(Debug, Clone, Default)]
struct EvalStyles {
    list_marker: Option<Value>,
    enum_start: Option<i64>,
    enum_numbering: Option<Value>,
    heading_numbering: HeadingNumbering,
    figure_numbering: Option<Value>,
}

/// Whether `eval_seq_exprs` starts in markup/content mode or pure code mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeqMode {
    Markup,
    Code,
}

type TrackedOrigin = (usize, ContentNode, Rc<Cell<usize>>, Option<usize>);

/// Recursion & rule-revocation state during content realization.
#[derive(Debug, Clone, Default)]
struct RealizeState {
    revoked_text_rules: Vec<usize>,
    element_guards: Vec<(usize, ContentNode)>,
    tracked_origins: Vec<TrackedOrigin>,
    locked_set_fields: Vec<(String, String)>,
    depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainerKind {
    List,
    Enum,
    Terms,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ContainerGroupMask {
    list: bool,
    enum_: bool,
    terms: bool,
}

impl ContainerGroupMask {
    const ALL: Self = Self {
        list: true,
        enum_: true,
        terms: true,
    };
    const NONE: Self = Self {
        list: false,
        enum_: false,
        terms: false,
    };

    fn is_empty(self) -> bool {
        !self.list && !self.enum_ && !self.terms
    }

    fn matches(self, kind: ContainerKind) -> bool {
        match kind {
            ContainerKind::List => self.list,
            ContainerKind::Enum => self.enum_,
            ContainerKind::Terms => self.terms,
        }
    }
}

fn content_node_container_kind(node: &ContentNode) -> Option<ContainerKind> {
    let target = match node.labeled_parts() {
        Some(([single], _, _, _)) => single,
        Some(_) => return None,
        None => node,
    };
    if matches!(target, ContentNode::ListItem(_)) {
        Some(ContainerKind::List)
    } else if matches!(target, ContentNode::EnumItem { .. }) {
        Some(ContainerKind::Enum)
    } else if target.term_item_parts().is_some() {
        Some(ContainerKind::Terms)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RefAnchorSpec {
    counter: String,
    kind: String,
    label: Option<String>,
}

fn heading_level_counter(level: u8) -> &'static str {
    match level {
        1 => "section",
        2 => "subsection",
        3 => "subsubsection",
        4 => "paragraph",
        _ => "subparagraph",
    }
}

fn strip_dup_snapshot(nodes: &mut Vec<ContentNode>, origin_id: usize) {
    nodes.retain(|n| n.dup_snapshot_parts().is_none_or(|(_, id)| id != origin_id));
    for node in nodes.iter_mut() {
        match node {
            ContentNode::Strong(children)
            | ContentNode::Emph(children)
            | ContentNode::ListItem(children)
            | ContentNode::Heading {
                content: children, ..
            }
            | ContentNode::EnumItem {
                content: children, ..
            } => strip_dup_snapshot(children, origin_id),
            ContentNode::Element { fields, .. } => {
                for val in fields.values_mut() {
                    if let Value::Content(inner) = val {
                        strip_dup_snapshot(inner, origin_id);
                    } else if let Value::Array(items) = val {
                        for item in items.iter_mut() {
                            if let Value::Content(inner) = item {
                                strip_dup_snapshot(inner, origin_id);
                            }
                        }
                    }
                }
            }
            ContentNode::FuncCall { args, .. } => {
                for arg in args.iter_mut() {
                    match arg {
                        Arg::Pos(Value::Content(inner))
                        | Arg::Named(_, Value::Content(inner))
                        | Arg::Spread(Value::Content(inner)) => {
                            strip_dup_snapshot(inner, origin_id);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn node_contains_table_or_grid(node: &ContentNode) -> bool {
    if let Some((target, _, _, _)) = node.labeled_parts() {
        return target.iter().any(node_contains_table_or_grid);
    }
    match node {
        ContentNode::FuncCall { name, args } => {
            matches!(name.as_str(), "table" | "grid")
                || args.iter().any(|arg| match arg {
                    super::value::Arg::Pos(Value::Content(nodes))
                    | super::value::Arg::Named(_, Value::Content(nodes))
                    | super::value::Arg::Spread(Value::Content(nodes)) => {
                        nodes.iter().any(node_contains_table_or_grid)
                    }
                    _ => false,
                })
        }
        ContentNode::Element { name, fields } => {
            matches!(name.as_str(), "table" | "grid")
                || fields.values().any(|val| match val {
                    Value::Content(nodes) => nodes.iter().any(node_contains_table_or_grid),
                    Value::Array(items) => items.iter().any(|item| match item {
                        Value::Content(nodes) => nodes.iter().any(node_contains_table_or_grid),
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
            nodes.iter().any(node_contains_table_or_grid)
        }
        ContentNode::RawSource(s) => {
            let trimmed = s.trim_start();
            trimmed.starts_with("#table(")
                || trimmed.starts_with("table(")
                || trimmed.starts_with("#grid(")
                || trimmed.starts_with("grid(")
        }
        _ => false,
    }
}

fn merge_adjacent_text_nodes(nodes: Vec<ContentNode>) -> Vec<ContentNode> {
    let mut merged: Vec<ContentNode> = Vec::with_capacity(nodes.len());
    for node in nodes {
        if let (Some(ContentNode::Text(prev)), ContentNode::Text(next)) = (merged.last_mut(), &node)
        {
            prev.push_str(next);
        } else {
            merged.push(node);
        }
    }
    merged
}

fn content_node_label_parts(node: &ContentNode) -> Option<(&str, bool)> {
    match node {
        ContentNode::Label(l) => Some((l.as_str(), false)),
        ContentNode::LabelDef(l) => Some((l.as_str(), true)),
        _ => None,
    }
}

fn attach_labels_in_nodes(nodes: Vec<ContentNode>) -> Vec<ContentNode> {
    let mut out: Vec<ContentNode> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Some((label_str, is_func)) = content_node_label_parts(&node) else {
            out.push(node);
            continue;
        };
        let label = label_str.to_string();
        let mut scan = out.len();
        while scan > 0 && matches!(out[scan - 1], ContentNode::Space | ContentNode::Parbreak) {
            scan -= 1;
        }
        if scan == 0
            || content_node_label_parts(&out[scan - 1]).is_some()
            || out[scan - 1].ref_anchor_parts().is_some()
        {
            out.push(node);
            continue;
        }

        let target_idx = scan - 1;
        let trivia = out.split_off(target_idx + 1);
        if out[target_idx].labeled_parts().is_some() {
            let prev_labeled = out.pop().expect("target_idx is valid");
            let (orig_target, _, _, _) = prev_labeled
                .into_labeled_parts()
                .expect("checked labeled_parts().is_some()");
            out.push(ContentNode::labeled(orig_target, trivia, label, is_func));
        } else if matches!(out[target_idx], ContentNode::Text(_)) {
            let mut run_start = target_idx;
            loop {
                if run_start >= 1 && matches!(out[run_start - 1], ContentNode::Text(_)) {
                    run_start -= 1;
                } else if run_start >= 2
                    && matches!(out[run_start - 1], ContentNode::Space)
                    && matches!(out[run_start - 2], ContentNode::Text(_))
                {
                    run_start -= 2;
                } else {
                    break;
                }
            }
            let run = out.split_off(run_start);
            let mut combined = String::new();
            for part in run {
                match part {
                    ContentNode::Text(t) => combined.push_str(&t),
                    ContentNode::Space => combined.push(' '),
                    _ => {}
                }
            }
            out.push(ContentNode::labeled(
                vec![ContentNode::Text(combined)],
                trivia,
                label,
                is_func,
            ));
        } else {
            let target = out.pop().expect("target_idx is valid");
            out.push(ContentNode::labeled(vec![target], trivia, label, is_func));
        }
    }
    out
}

fn unwrap_labeled_node(node: ContentNode, out: &mut Vec<ContentNode>) {
    if node.labeled_parts().is_some() {
        let (target, trivia, label, is_func) = node
            .into_labeled_parts()
            .expect("checked labeled_parts().is_some()");
        let label_node = if is_func {
            ContentNode::LabelDef(label)
        } else {
            ContentNode::Label(label)
        };
        let mut unwrapped_target = unwrap_labeled_nodes(target);
        if unwrapped_target.len() == 1 {
            let single = unwrapped_target.pop().expect("len == 1");
            match single {
                ContentNode::ListItem(mut children) => {
                    children.extend(
                        trivia
                            .into_iter()
                            .filter(|n| matches!(n, ContentNode::Space)),
                    );
                    children.push(label_node);
                    out.push(ContentNode::ListItem(children));
                    return;
                }
                ContentNode::EnumItem {
                    number,
                    mut content,
                } => {
                    content.extend(
                        trivia
                            .into_iter()
                            .filter(|n| matches!(n, ContentNode::Space)),
                    );
                    content.push(label_node);
                    out.push(ContentNode::EnumItem { number, content });
                    return;
                }
                other if other.term_item_parts().is_some() => {
                    let (term, mut description) = other
                        .into_term_item_parts()
                        .expect("checked term_item_parts().is_some()");
                    description.extend(
                        trivia
                            .into_iter()
                            .filter(|n| matches!(n, ContentNode::Space)),
                    );
                    description.push(label_node);
                    out.push(ContentNode::term_item(term, description));
                    return;
                }
                other => {
                    out.push(other);
                    out.extend(trivia);
                    out.push(label_node);
                    return;
                }
            }
        }
        out.extend(unwrapped_target);
        out.extend(trivia);
        out.push(label_node);
        return;
    }

    let unwrapped = match node {
        ContentNode::Strong(children) => ContentNode::Strong(unwrap_labeled_nodes(children)),
        ContentNode::Emph(children) => ContentNode::Emph(unwrap_labeled_nodes(children)),
        ContentNode::Heading { level, content } => ContentNode::Heading {
            level,
            content: unwrap_labeled_nodes(content),
        },
        ContentNode::ListItem(children) => ContentNode::ListItem(unwrap_labeled_nodes(children)),
        ContentNode::EnumItem { number, content } => ContentNode::EnumItem {
            number,
            content: unwrap_labeled_nodes(content),
        },
        ContentNode::Element { name, mut fields } => {
            for (_, val) in fields.iter_mut() {
                match val {
                    Value::Content(nodes) => {
                        *nodes = unwrap_labeled_nodes(std::mem::take(nodes));
                    }
                    Value::Array(items) => {
                        for item in items.iter_mut() {
                            if let Value::Content(nodes) = item {
                                *nodes = unwrap_labeled_nodes(std::mem::take(nodes));
                            }
                        }
                    }
                    _ => {}
                }
            }
            ContentNode::Element { name, fields }
        }
        ContentNode::FuncCall { name, mut args } => {
            for arg in &mut args {
                match arg {
                    super::value::Arg::Pos(Value::Content(nodes))
                    | super::value::Arg::Named(_, Value::Content(nodes))
                    | super::value::Arg::Spread(Value::Content(nodes)) => {
                        *nodes = unwrap_labeled_nodes(std::mem::take(nodes));
                    }
                    _ => {}
                }
            }
            ContentNode::FuncCall { name, args }
        }
        other => other,
    };
    out.push(unwrapped);
}

fn unwrap_labeled_nodes(nodes: Vec<ContentNode>) -> Vec<ContentNode> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        unwrap_labeled_node(node, &mut out);
    }
    out
}

fn push_text_fragment(fragment: &str, out: &mut Vec<ContentNode>) {
    if fragment.is_empty() {
        return;
    }
    let trimmed_start = fragment.trim_start_matches(' ');
    let leading = fragment.len() - trimmed_start.len();
    for _ in 0..leading {
        out.push(ContentNode::Space);
    }
    let core = trimmed_start.trim_end_matches(' ');
    let trailing = trimmed_start.len() - core.len();
    if !core.is_empty() {
        out.push(ContentNode::Text(core.to_string()));
    }
    for _ in 0..trailing {
        out.push(ContentNode::Space);
    }
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
        Self::with_vfs_and_config(Arc::new(NoopVfs), EvalConfig::default())
    }

    /// Create a new MiniEval interpreter with a custom VFS.
    pub fn with_vfs(vfs: Arc<dyn VirtualFileSystem>) -> Self {
        Self::with_vfs_and_config(vfs, EvalConfig::default())
    }

    /// Create a new MiniEval interpreter with custom config.
    pub fn with_config(config: EvalConfig) -> Self {
        Self::with_vfs_and_config(Arc::new(NoopVfs), config)
    }

    /// Create a new MiniEval interpreter with custom VFS and config.
    pub fn with_vfs_and_config(vfs: Arc<dyn VirtualFileSystem>, config: EvalConfig) -> Self {
        Self {
            scopes: Scopes::new(),
            flow: None,
            vfs,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
            current_file: None,
            root_file: None,
            state_store: HashMap::new(),
            counter_store: HashMap::new(),
            config,
            warnings: Vec::new(),
            show_rules: Vec::new(),
            styles: EvalStyles::default(),
            current_depth: 0,
            shadowed_context_marker: None,
            import_stack: Vec::new(),
            import_bytes_remaining: Arc::new(AtomicUsize::new(MAX_TOTAL_IMPORT_BYTES)),
            next_dup_id: 1,
        }
    }

    /// Set the current file path (for relative imports).
    pub fn set_current_file(&mut self, path: impl Into<String>) {
        let path = path.into();
        if self.current_depth == 0 {
            self.root_file = Some(path.clone());
        }
        self.current_file = Some(path);
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
    /// This traverses the content tree using Typst realization semantics:
    /// textual rules operate on contiguous text runs with per-rule revocation,
    /// item rules run before loose items are grouped into `list`/`enum`/`terms`
    /// containers, and later rules take precedence when tied at the same start.
    pub fn apply_show_rules(&mut self, content: Vec<ContentNode>) -> EvalResult<Vec<ContentNode>> {
        let realized = self.realize_content_range(
            content,
            0,
            self.show_rules.len(),
            &mut RealizeState::default(),
        )?;
        Ok(unwrap_labeled_nodes(realized))
    }

    fn has_container_rule_in(&self, start: usize, end: usize, target: &str) -> bool {
        let end = end.min(self.show_rules.len());
        (start..end).any(|idx| self.show_rules[idx].selector.may_match_container(target))
    }

    fn apply_single_show_rule(
        &mut self,
        rule: &ShowRule,
        matched_node: ContentNode,
    ) -> EvalResult<Vec<ContentNode>> {
        if let Some(ref replacement) = rule.replacement {
            return Ok(replacement.clone().into_content());
        }

        let node_value = Value::Content(vec![matched_node]);
        let body_result = self.with_closure_scope(&rule.transform, |this| {
            this.scopes.define("it".to_string(), node_value.clone());
            if let Some(first_param) = rule.transform.params.first() {
                this.scopes.define(first_param.clone(), node_value);
            }
            let res = this.eval_body_source(&rule.transform.body_source);
            if let Some(FlowEvent::Return(value)) = this.flow.take() {
                return Ok(value.unwrap_or(Value::None));
            }
            res
        });

        body_result.map(Value::into_content)
    }

    fn realize_content_range(
        &mut self,
        content: Vec<ContentNode>,
        start: usize,
        end: usize,
        state: &mut RealizeState,
    ) -> EvalResult<Vec<ContentNode>> {
        self.realize_content_range_with_mask(content, start, end, state, ContainerGroupMask::ALL)
    }

    fn realize_content_range_with_mask(
        &mut self,
        content: Vec<ContentNode>,
        start: usize,
        end: usize,
        state: &mut RealizeState,
        group_mask: ContainerGroupMask,
    ) -> EvalResult<Vec<ContentNode>> {
        let pass_a = self.realize_nodes_without_grouping(content, start, end, state, group_mask)?;
        self.group_and_realize_containers(pass_a, start, end, state, group_mask)
    }

    fn check_show_depth(&mut self, state: &RealizeState) -> EvalResult<bool> {
        if state.depth < MAX_SHOW_DEPTH {
            return Ok(true);
        }
        if self.config.strict {
            Err(EvalError::new(EvalErrorKind::RecursionLimitExceeded {
                max_depth: MAX_SHOW_DEPTH,
            }))
        } else {
            self.warn("maximum show rule depth exceeded");
            Ok(false)
        }
    }

    fn extract_replaced_anchor(&self, node: &ContentNode) -> Option<RefAnchorSpec> {
        let (inner, label) = match node.labeled_parts() {
            Some(([single], _, lbl, _)) => (single, Some(lbl.to_string())),
            Some(_) => return None,
            None => (node, None),
        };
        match inner {
            ContentNode::Element { name, fields } if name == "heading" => {
                let is_numbered = match fields.get("numbering") {
                    Some(Value::None) => false,
                    Some(_) => true,
                    None => matches!(self.styles.heading_numbering, HeadingNumbering::Numbered(_)),
                };
                if !is_numbered {
                    return None;
                }
                let level = fields
                    .get("level")
                    .or_else(|| fields.get("depth"))
                    .and_then(|v| v.as_int().ok())
                    .unwrap_or(1)
                    .clamp(1, 6) as u8;
                Some(RefAnchorSpec {
                    counter: heading_level_counter(level).to_string(),
                    kind: "section".to_string(),
                    label,
                })
            }
            ContentNode::Heading { level, .. } => {
                if !matches!(self.styles.heading_numbering, HeadingNumbering::Numbered(_)) {
                    return None;
                }
                Some(RefAnchorSpec {
                    counter: heading_level_counter(*level).to_string(),
                    kind: "section".to_string(),
                    label,
                })
            }
            ContentNode::FuncCall { name, args } if name == "figure" => {
                let effective_numbering = args.iter().rev().find_map(|arg| match arg {
                    Arg::Named(k, v) if k == "numbering" => Some(v),
                    _ => None,
                });
                if effective_numbering.is_some_and(|v| matches!(v, Value::None)) {
                    return None;
                }
                let explicit_kind = args.iter().rev().find_map(|arg| match arg {
                    Arg::Named(k, v) if k == "kind" => Some(v),
                    _ => None,
                });
                let explicit_supp = args
                    .iter()
                    .rev()
                    .find_map(|arg| match arg {
                        Arg::Named(k, v) if k == "supplement" => Some(v),
                        _ => None,
                    })
                    .and_then(|v| match v {
                        Value::Auto => None,
                        Value::None => Some(String::new()),
                        Value::Str(s) => Some(s.clone()),
                        Value::Content(nodes) => {
                            Some(super::super::supplement::plain_text_from_eval_nodes(nodes))
                        }
                        other => Some(other.display().trim().to_string()),
                    });

                enum FigClass {
                    Image,
                    Table,
                    Custom(String),
                }
                let fig_class = match explicit_kind {
                    None | Some(Value::Auto) => {
                        let is_table = args.iter().any(|arg| match arg {
                            super::value::Arg::Pos(Value::Content(nodes)) => {
                                nodes.iter().any(node_contains_table_or_grid)
                            }
                            _ => false,
                        });
                        if is_table {
                            FigClass::Table
                        } else {
                            FigClass::Image
                        }
                    }
                    Some(Value::Str(s)) => match s.as_str() {
                        "table" => FigClass::Table,
                        "image" | "figure" => FigClass::Image,
                        other => FigClass::Custom(other.to_string()),
                    },
                    Some(Value::Func(f)) => match f.builtin_name() {
                        Some("table") => FigClass::Table,
                        Some("image" | "figure") | None => FigClass::Image,
                        Some(other) => FigClass::Custom(other.to_string()),
                    },
                    Some(_) => FigClass::Image,
                };
                let (counter, default_kind) = match fig_class {
                    FigClass::Table => ("table".to_string(), "table".to_string()),
                    FigClass::Image => ("figure".to_string(), "figure".to_string()),
                    FigClass::Custom(raw_kind) => (
                        super::super::supplement::custom_figure_counter_name(&raw_kind),
                        super::super::supplement::default_custom_figure_supplement(&raw_kind),
                    ),
                };
                Some(RefAnchorSpec {
                    counter,
                    kind: explicit_supp.unwrap_or(default_kind),
                    label,
                })
            }
            _ => None,
        }
    }

    fn extract_replaced_anchor_with_show_sets(
        &self,
        node: &ContentNode,
        start: usize,
        end: usize,
        state: &RealizeState,
    ) -> Option<RefAnchorSpec> {
        let end = end.min(self.show_rules.len());
        let has_set_style = (start..end).any(|idx| self.show_rules[idx].set_style.is_some());
        if !has_set_style {
            return self.extract_replaced_anchor(node);
        }
        let mut effective = node.clone();
        let mut locked = state.locked_set_fields.clone();
        for idx in (start..end).rev() {
            let rule = &self.show_rules[idx];
            let Some(ref set_style) = rule.set_style else {
                continue;
            };
            if state
                .element_guards
                .iter()
                .any(|(id, prev)| *id == rule.id && (prev == node || prev == &effective))
            {
                continue;
            }
            if rule.selector.matches(&effective) {
                effective =
                    match Self::apply_show_set_style_to_node(effective, set_style, &mut locked) {
                        Ok(updated) | Err(updated) => updated,
                    };
            }
        }
        self.extract_replaced_anchor(&effective)
    }

    fn apply_show_set_style_to_inner(
        node: ContentNode,
        style: &ShowSetStyle,
        locked: &mut Vec<(String, String)>,
    ) -> Result<ContentNode, ContentNode> {
        let is_locked = |locked: &[(String, String)], field: &str| {
            locked
                .iter()
                .any(|(target, key)| target == style.target.as_str() && key == field)
        };
        match (style.target.as_str(), node) {
            ("list", ContentNode::Element { name, mut fields }) if name == LIST_CONTAINER_NAME => {
                if let Some(marker) = style.args.get("marker") {
                    if !is_locked(locked, "marker") && !has_explicit_field(&fields, "marker") {
                        if matches!(marker, Value::Auto) {
                            fields.shift_remove("marker");
                        } else {
                            fields.insert("marker".to_string(), marker.clone());
                        }
                        locked.push(("list".to_string(), "marker".to_string()));
                    }
                }
                Ok(ContentNode::Element { name, fields })
            }
            ("enum", ContentNode::Element { name, mut fields }) if name == ENUM_CONTAINER_NAME => {
                if let Some(start) = style.args.get("start") {
                    if !is_locked(locked, "start") && !has_explicit_field(&fields, "start") {
                        match start {
                            Value::Auto | Value::None => {
                                fields.insert("start".to_string(), Value::Auto);
                            }
                            other => {
                                if let Ok(n) = other.as_int() {
                                    fields.insert("start".to_string(), Value::Int(n));
                                }
                            }
                        }
                        locked.push(("enum".to_string(), "start".to_string()));
                    }
                }
                if let Some(numbering) = style.args.get("numbering") {
                    if !is_locked(locked, "numbering") && !has_explicit_field(&fields, "numbering")
                    {
                        if matches!(numbering, Value::Auto) {
                            fields.shift_remove("numbering");
                        } else {
                            fields.insert("numbering".to_string(), numbering.clone());
                        }
                        locked.push(("enum".to_string(), "numbering".to_string()));
                    }
                }
                Ok(ContentNode::Element { name, fields })
            }
            ("heading", ContentNode::Heading { level, content }) => {
                if let Some(numbering) = style.args.get("numbering") {
                    if !is_locked(locked, "numbering") {
                        locked.push(("heading".to_string(), "numbering".to_string()));
                        if matches!(numbering, Value::Auto) {
                            return Ok(ContentNode::Heading { level, content });
                        }
                        let mut fields = IndexMap::new();
                        fields.insert("level".to_string(), Value::Int(level as i64));
                        fields.insert("numbering".to_string(), numbering.clone());
                        fields.insert("body".to_string(), Value::Content(content));
                        return Ok(ContentNode::Element {
                            name: "heading".to_string(),
                            fields,
                        });
                    }
                }
                Ok(ContentNode::Heading { level, content })
            }
            ("heading", ContentNode::Element { name, mut fields }) if name == "heading" => {
                if let Some(numbering) = style.args.get("numbering") {
                    if !is_locked(locked, "numbering") && !has_explicit_field(&fields, "numbering")
                    {
                        if matches!(numbering, Value::Auto) {
                            fields.shift_remove("numbering");
                        } else {
                            fields.insert("numbering".to_string(), numbering.clone());
                        }
                        locked.push(("heading".to_string(), "numbering".to_string()));
                    }
                }
                Ok(ContentNode::Element { name, fields })
            }
            ("figure", ContentNode::FuncCall { name, mut args }) if name == "figure" => {
                if let Some(numbering) = style.args.get("numbering") {
                    let is_explicit = args.iter().any(|arg| {
                        matches!(arg, Arg::Named(k, Value::Bool(true)) if k == EXPLICIT_NUMBERING_ARG)
                    });
                    if !is_locked(locked, "numbering") && !is_explicit {
                        args.retain(|arg| !matches!(arg, Arg::Named(k, _) if k == "numbering"));
                        if !matches!(numbering, Value::Auto) {
                            args.push(Arg::Named("numbering".to_string(), numbering.clone()));
                        }
                        locked.push(("figure".to_string(), "numbering".to_string()));
                    }
                }
                Ok(ContentNode::FuncCall { name, args })
            }
            (_, other) => Err(other),
        }
    }

    fn apply_show_set_style_to_node(
        node: ContentNode,
        style: &ShowSetStyle,
        locked: &mut Vec<(String, String)>,
    ) -> Result<ContentNode, ContentNode> {
        if node.labeled_parts().is_some() {
            let (mut target, trivia, label, is_func) = node
                .into_labeled_parts()
                .expect("checked labeled_parts().is_some()");
            if target.len() == 1 {
                let single = target.pop().expect("target.len() == 1");
                match Self::apply_show_set_style_to_inner(single, style, locked) {
                    Ok(updated) => {
                        return Ok(ContentNode::labeled(vec![updated], trivia, label, is_func));
                    }
                    Err(orig) => {
                        return Err(ContentNode::labeled(vec![orig], trivia, label, is_func));
                    }
                }
            }
            return Err(ContentNode::labeled(target, trivia, label, is_func));
        }
        Self::apply_show_set_style_to_inner(node, style, locked)
    }

    fn apply_set_style_to_eval_styles(&mut self, style: &ShowSetStyle) {
        match style.target.as_str() {
            "list" => {
                if let Some(marker) = style.args.get("marker") {
                    self.styles.list_marker = if matches!(marker, Value::Auto) {
                        None
                    } else {
                        Some(marker.clone())
                    };
                }
            }
            "enum" => {
                if let Some(start) = style.args.get("start") {
                    self.styles.enum_start = match start {
                        Value::Auto | Value::None => None,
                        other => other.as_int().ok(),
                    };
                }
                if let Some(numbering) = style.args.get("numbering") {
                    self.styles.enum_numbering =
                        (!matches!(numbering, Value::Auto)).then(|| numbering.clone());
                }
            }
            "heading" => {
                if let Some(numbering) = style.args.get("numbering") {
                    self.styles.heading_numbering = if matches!(numbering, Value::None) {
                        HeadingNumbering::Unnumbered
                    } else if matches!(numbering, Value::Auto) {
                        HeadingNumbering::Default
                    } else {
                        HeadingNumbering::Numbered(numbering.clone())
                    };
                }
            }
            "figure" => {
                if let Some(numbering) = style.args.get("numbering") {
                    self.styles.figure_numbering =
                        (!matches!(numbering, Value::Auto)).then(|| numbering.clone());
                }
            }
            _ => {}
        }
    }

    fn carry_element_guards(
        guards: &mut Vec<(usize, ContentNode)>,
        from: &ContentNode,
        to: &ContentNode,
    ) {
        let ids: Vec<usize> = guards
            .iter()
            .filter(|(_, prev)| prev == from)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if !guards.iter().any(|(gid, prev)| *gid == id && prev == to) {
                guards.push((id, to.clone()));
            }
        }
    }

    fn realize_single_non_textual_node(
        &mut self,
        node: ContentNode,
        start: usize,
        end: usize,
        state: &mut RealizeState,
        recurse_children: bool,
        group_mask: ContainerGroupMask,
    ) -> EvalResult<Vec<ContentNode>> {
        let mut is_dup_origin = false;
        let mut first_origin_snapshot: Option<usize> = None;
        let mut matched_origin_id: Option<usize> = None;
        let mut tracked_guard_ids = Vec::new();
        for (guard_rule_id, orig_node, seen_count, origin_id) in &state.tracked_origins {
            if orig_node == &node
                && state
                    .element_guards
                    .iter()
                    .any(|(id, prev)| id == guard_rule_id && prev == &node)
            {
                let prev = seen_count.get();
                seen_count.set(prev + 1);
                if prev == 0 {
                    if first_origin_snapshot.is_none() {
                        first_origin_snapshot = *origin_id;
                    }
                } else {
                    is_dup_origin = true;
                    if matched_origin_id.is_none() {
                        matched_origin_id = *origin_id;
                    }
                }
                tracked_guard_ids.push(*guard_rule_id);
            }
        }

        if is_dup_origin {
            let dup_counter = self
                .extract_replaced_anchor_with_show_sets(&node, start, end, state)
                .map(|spec| spec.counter);
            let orig_for_retain = node.clone();
            let had_label = node.labeled_parts().is_some();
            let stripped = match node.into_labeled_parts() {
                Ok((mut target, _, _, _)) if target.len() == 1 => {
                    target.pop().expect("target.len() == 1")
                }
                Ok((target, _, _, _)) => return Ok(target),
                Err(original) => original,
            };
            let mut dup_state = state.clone();
            dup_state
                .tracked_origins
                .retain(|(_, orig, _, _)| orig != &orig_for_retain);
            if had_label {
                Self::carry_element_guards(
                    &mut dup_state.element_guards,
                    &orig_for_retain,
                    &stripped,
                );
            }
            let realized = self.realize_single_non_textual_node(
                stripped,
                start,
                end,
                &mut dup_state,
                recurse_children,
                group_mask,
            )?;
            if let Some(counter) = dup_counter {
                let dup_id = self.next_dup_id;
                self.next_dup_id += 1;
                return Ok(vec![ContentNode::dup_element(
                    &counter,
                    dup_id,
                    matched_origin_id.unwrap_or(0),
                    realized,
                )]);
            }
            return Ok(realized);
        }

        let first_snapshot = first_origin_snapshot.and_then(|oid| {
            self.extract_replaced_anchor_with_show_sets(&node, start, end, state)
                .map(|spec| (spec.counter, oid))
        });
        let orig_for_first = (!tracked_guard_ids.is_empty()).then(|| node.clone());
        let drop_first_tracked = |target_state: &mut RealizeState| {
            if let Some(ref orig_first) = orig_for_first {
                target_state.tracked_origins.retain(|(gid, orig, _, _)| {
                    !(tracked_guard_ids.contains(gid) && orig == orig_first)
                });
            }
        };

        let end = end.min(self.show_rules.len());
        let matching_idx = (start..end).rev().find(|&idx| {
            let rule = &self.show_rules[idx];
            !rule.selector.is_textual()
                && !state
                    .element_guards
                    .iter()
                    .any(|(id, prev)| *id == rule.id && prev == &node)
                && rule.selector.matches(&node)
        });

        let mut out = if let Some(rule_idx) = matching_idx {
            if !self.check_show_depth(state)? {
                return Ok(vec![node]);
            }
            let rule = self.show_rules[rule_idx].clone();
            if let Some(ref set_style) = rule.set_style {
                let mut child_state = state.clone();
                drop_first_tracked(&mut child_state);
                child_state.depth += 1;
                let node_before_set = node.clone();
                child_state
                    .element_guards
                    .push((rule.id, node_before_set.clone()));
                match Self::apply_show_set_style_to_node(
                    node,
                    set_style,
                    &mut child_state.locked_set_fields,
                ) {
                    Ok(updated) => {
                        Self::carry_element_guards(
                            &mut child_state.element_guards,
                            &node_before_set,
                            &updated,
                        );
                        child_state.element_guards.push((rule.id, updated.clone()));
                        self.realize_single_non_textual_node(
                            updated,
                            start,
                            end,
                            &mut child_state,
                            recurse_children,
                            group_mask,
                        )?
                    }
                    Err(orig_node) => {
                        let saved_styles = self.styles.clone();
                        self.apply_set_style_to_eval_styles(set_style);
                        let mut child_mask = group_mask;
                        if set_style.target == "list" {
                            child_mask.list = true;
                        } else if set_style.target == "enum" {
                            child_mask.enum_ = true;
                        }
                        let res = self.realize_single_non_textual_node(
                            orig_node,
                            start,
                            end,
                            &mut child_state,
                            recurse_children,
                            child_mask,
                        );
                        self.styles = saved_styles;
                        res?
                    }
                }
            } else {
                let anchor_spec =
                    self.extract_replaced_anchor_with_show_sets(&node, start, end, state);
                let mut child_state = state.clone();
                drop_first_tracked(&mut child_state);
                child_state.depth += 1;
                child_state.element_guards.push((rule.id, node.clone()));
                let origin_id = if anchor_spec.is_some() && rule.replacement.is_none() {
                    let oid = self.next_dup_id;
                    self.next_dup_id += 1;
                    Some(oid)
                } else {
                    None
                };
                let orig_seen = if (anchor_spec.is_some() || node.labeled_parts().is_some())
                    && rule.replacement.is_none()
                {
                    let seen = Rc::new(Cell::new(0usize));
                    child_state.tracked_origins.push((
                        rule.id,
                        node.clone(),
                        Rc::clone(&seen),
                        origin_id,
                    ));
                    Some(seen)
                } else {
                    None
                };
                match self.apply_single_show_rule(&rule, node.clone()) {
                    Ok(produced) => {
                        let mut realized = self.realize_content_range_with_mask(
                            produced,
                            start,
                            end,
                            &mut child_state,
                            group_mask,
                        )?;
                        let seen_count = orig_seen.as_ref().map_or(0, |seen| seen.get());
                        if seen_count <= 1 {
                            if let Some(oid) = origin_id {
                                strip_dup_snapshot(&mut realized, oid);
                            }
                        }
                        if let Some(spec) = anchor_spec {
                            if seen_count == 0 {
                                realized.insert(
                                    0,
                                    ContentNode::ref_anchor(
                                        &spec.counter,
                                        &spec.kind,
                                        spec.label,
                                        true,
                                    ),
                                );
                            }
                        }
                        realized
                    }
                    Err(e) => {
                        if self.config.strict {
                            return Err(e);
                        }
                        vec![node]
                    }
                }
            }
        } else if recurse_children {
            if orig_for_first.is_some() {
                let mut child_state = state.clone();
                drop_first_tracked(&mut child_state);
                vec![self.process_content_children_range(
                    node,
                    start,
                    end,
                    &mut child_state,
                    group_mask,
                )?]
            } else {
                vec![self.process_content_children_range(node, start, end, state, group_mask)?]
            }
        } else {
            vec![node]
        };

        if let Some((counter, oid)) = first_snapshot {
            out.push(ContentNode::dup_snapshot(&counter, oid));
        }
        Ok(out)
    }

    fn realize_nodes_without_grouping(
        &mut self,
        content: Vec<ContentNode>,
        start: usize,
        end: usize,
        state: &mut RealizeState,
        group_mask: ContainerGroupMask,
    ) -> EvalResult<Vec<ContentNode>> {
        let end = end.min(self.show_rules.len());
        if content.is_empty() {
            return Ok(content);
        }

        let nodes = attach_labels_in_nodes(merge_adjacent_text_nodes(content));
        let has_active_textual = (start..end).any(|idx| {
            let rule = &self.show_rules[idx];
            rule.selector.is_textual() && !state.revoked_text_rules.contains(&rule.id)
        });

        let mut result = Vec::with_capacity(nodes.len());
        let mut i = 0;
        while i < nodes.len() {
            if has_active_textual && matches!(nodes[i], ContentNode::Text(_) | ContentNode::Space) {
                let mut j = i;
                let mut combined = String::new();
                while j < nodes.len() {
                    match &nodes[j] {
                        ContentNode::Text(t) => combined.push_str(t),
                        ContentNode::Space => combined.push(' '),
                        _ => break,
                    }
                    j += 1;
                }

                let mut cursor = 0usize;
                let mut matched_any = false;
                while cursor < combined.len() {
                    let slice = &combined[cursor..];
                    let mut best: Option<(usize, usize, usize)> = None;
                    for idx in (start..end).rev() {
                        let rule = &self.show_rules[idx];
                        if !rule.selector.is_textual()
                            || state.revoked_text_rules.contains(&rule.id)
                        {
                            continue;
                        }
                        if let Some((m_start, m_end)) = rule.selector.find_text_match(slice) {
                            // Reverse iteration keeps higher `rule_idx` on identical `m_start` ties.
                            if best.is_none_or(|(best_start, _, _)| m_start < best_start) {
                                best = Some((m_start, m_end, idx));
                            }
                        }
                    }

                    let Some((rel_start, rel_end, rule_idx)) = best else {
                        break;
                    };
                    if rel_end <= rel_start || !self.check_show_depth(state)? {
                        break;
                    }

                    matched_any = true;
                    let m_start = cursor + rel_start;
                    let m_end = cursor + rel_end;
                    if m_start > cursor {
                        let mut prefix_nodes = Vec::new();
                        push_text_fragment(&combined[cursor..m_start], &mut prefix_nodes);
                        for pnode in prefix_nodes {
                            result.extend(self.realize_single_non_textual_node(
                                pnode, start, end, state, false, group_mask,
                            )?);
                        }
                    }

                    let matched_text = combined[m_start..m_end].to_string();
                    let rule = self.show_rules[rule_idx].clone();
                    let mut child_state = state.clone();
                    child_state.depth += 1;
                    child_state.revoked_text_rules.push(rule.id);
                    match self
                        .apply_single_show_rule(&rule, ContentNode::Text(matched_text.clone()))
                    {
                        Ok(produced) => {
                            result.extend(self.realize_content_range_with_mask(
                                produced,
                                start,
                                end,
                                &mut child_state,
                                group_mask,
                            )?);
                        }
                        Err(e) => {
                            if self.config.strict {
                                return Err(e);
                            }
                            result.push(ContentNode::Text(matched_text));
                        }
                    }
                    cursor = m_end;
                }

                if matched_any {
                    if cursor < combined.len() {
                        let mut suffix_nodes = Vec::new();
                        push_text_fragment(&combined[cursor..], &mut suffix_nodes);
                        for snode in suffix_nodes {
                            result.extend(self.realize_single_non_textual_node(
                                snode, start, end, state, false, group_mask,
                            )?);
                        }
                    }
                } else {
                    for orig in &nodes[i..j] {
                        result.extend(self.realize_single_non_textual_node(
                            orig.clone(),
                            start,
                            end,
                            state,
                            false,
                            group_mask,
                        )?);
                    }
                }

                i = j;
                continue;
            }

            result.extend(self.realize_single_non_textual_node(
                nodes[i].clone(),
                start,
                end,
                state,
                true,
                group_mask,
            )?);
            i += 1;
        }

        Ok(result)
    }

    fn group_and_realize_containers(
        &mut self,
        nodes: Vec<ContentNode>,
        start: usize,
        end: usize,
        state: &mut RealizeState,
        group_mask: ContainerGroupMask,
    ) -> EvalResult<Vec<ContentNode>> {
        if group_mask.is_empty() {
            return Ok(nodes);
        }

        let mut out = Vec::with_capacity(nodes.len());
        let mut i = 0;
        while i < nodes.len() {
            let Some(kind) = content_node_container_kind(&nodes[i]) else {
                out.push(nodes[i].clone());
                i += 1;
                continue;
            };
            if !group_mask.matches(kind) {
                out.push(nodes[i].clone());
                i += 1;
                continue;
            }

            let mut items = vec![nodes[i].clone()];
            let mut tight = true;
            i += 1;
            while i < nodes.len() {
                let mut scan = i;
                let mut saw_parbreak = false;
                while let Some(trivia) = nodes.get(scan) {
                    match trivia {
                        ContentNode::Space => scan += 1,
                        ContentNode::Parbreak => {
                            saw_parbreak = true;
                            scan += 1;
                        }
                        _ => break,
                    }
                }
                if nodes.get(scan).and_then(content_node_container_kind) == Some(kind) {
                    if saw_parbreak {
                        tight = false;
                    }
                    items.push(nodes[scan].clone());
                    i = scan + 1;
                } else {
                    break;
                }
            }

            let container = match kind {
                ContainerKind::List => {
                    ContentNode::list_container(items, tight, self.styles.list_marker.clone())
                }
                ContainerKind::Enum => ContentNode::enum_container(
                    items,
                    tight,
                    self.styles.enum_start,
                    self.styles.enum_numbering.clone(),
                ),
                ContainerKind::Terms => ContentNode::terms_container(items, tight),
            };
            out.extend(self.realize_single_non_textual_node(
                container, start, end, state, false, group_mask,
            )?);
        }
        Ok(out)
    }

    /// Recursively process children of a content node for rules in `start..end`.
    fn process_content_children_range(
        &mut self,
        node: ContentNode,
        start: usize,
        end: usize,
        state: &mut RealizeState,
        group_mask: ContainerGroupMask,
    ) -> EvalResult<ContentNode> {
        match node {
            ContentNode::Strong(children) => {
                let processed =
                    self.realize_content_range_with_mask(children, start, end, state, group_mask)?;
                Ok(ContentNode::Strong(processed))
            }
            ContentNode::Emph(children) => {
                let processed =
                    self.realize_content_range_with_mask(children, start, end, state, group_mask)?;
                Ok(ContentNode::Emph(processed))
            }
            ContentNode::Heading { level, content } => {
                let processed =
                    self.realize_content_range_with_mask(content, start, end, state, group_mask)?;
                Ok(ContentNode::Heading {
                    level,
                    content: processed,
                })
            }
            ContentNode::ListItem(children) => {
                let processed =
                    self.realize_content_range_with_mask(children, start, end, state, group_mask)?;
                Ok(ContentNode::ListItem(processed))
            }
            ContentNode::EnumItem { number, content } => {
                let processed =
                    self.realize_content_range_with_mask(content, start, end, state, group_mask)?;
                Ok(ContentNode::EnumItem {
                    number,
                    content: processed,
                })
            }
            ContentNode::Element { name, mut fields } => {
                if name == LABELED_ELEMENT_NAME {
                    if let Some(Value::Content(target)) = fields.get_mut("__target") {
                        if target.len() == 1 {
                            if matches!(target[0], ContentNode::Text(_)) {
                                *target = self.realize_nodes_without_grouping(
                                    std::mem::take(target),
                                    start,
                                    end,
                                    state,
                                    group_mask,
                                )?;
                            } else {
                                let inner = target.pop().expect("target.len() == 1");
                                *target = vec![self.process_content_children_range(
                                    inner, start, end, state, group_mask,
                                )?];
                            }
                        } else {
                            *target = self.realize_content_range_with_mask(
                                std::mem::take(target),
                                start,
                                end,
                                state,
                                group_mask,
                            )?;
                        }
                    }
                    return Ok(ContentNode::Element { name, fields });
                }
                let is_list_container = matches!(
                    name.as_str(),
                    LIST_CONTAINER_NAME | ENUM_CONTAINER_NAME | TERMS_CONTAINER_NAME
                );
                if is_list_container {
                    if let Some(Value::Array(items)) = fields.get_mut("children") {
                        for item in items.iter_mut() {
                            if let Value::Content(nodes) = item {
                                *nodes = self.realize_nodes_without_grouping(
                                    std::mem::take(nodes),
                                    start,
                                    end,
                                    state,
                                    group_mask,
                                )?;
                            }
                        }
                    }
                } else {
                    for (_, value) in fields.iter_mut() {
                        if let Value::Content(nodes) = value {
                            *nodes = self.realize_content_range_with_mask(
                                std::mem::take(nodes),
                                start,
                                end,
                                state,
                                group_mask,
                            )?;
                        }
                    }
                }
                Ok(ContentNode::Element { name, fields })
            }
            ContentNode::FuncCall { name, mut args } => {
                for arg in &mut args {
                    match arg {
                        super::value::Arg::Pos(Value::Content(nodes))
                        | super::value::Arg::Named(_, Value::Content(nodes))
                        | super::value::Arg::Spread(Value::Content(nodes)) => {
                            *nodes = self.realize_content_range_with_mask(
                                std::mem::take(nodes),
                                start,
                                end,
                                state,
                                group_mask,
                            )?;
                        }
                        _ => {}
                    }
                }
                Ok(ContentNode::FuncCall { name, args })
            }
            // Other nodes don't have children to process
            other => Ok(other),
        }
    }

    fn resolve_builtin_element_name(&self, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::Ident(ident) => {
                let name = ident.get().as_str();
                match self.scopes.get(name) {
                    None => Some(name.to_string()),
                    Some(Value::Func(f)) => f.builtin_name().map(str::to_string),
                    _ => None,
                }
            }
            ast::Expr::FieldAccess(access) => {
                if let ast::Expr::Ident(target) = access.target() {
                    if self.scopes.get(target.get().as_str()).is_none() {
                        let t = target.get().as_str();
                        let f = access.field().get().as_str();
                        if matches!(
                            (t, f),
                            ("math", "equation")
                                | ("terms", "item")
                                | ("list", "item")
                                | ("enum", "item")
                                | ("table", "header" | "cell" | "hline" | "vline")
                        ) {
                            return Some(format!("{t}.{f}"));
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn resolve_list_marker_value(&mut self, val: &Value) -> EvalResult<Option<Value>> {
        match val {
            Value::Auto => Ok(None),
            Value::Func(closure) => {
                let mut markers = Vec::with_capacity(4);
                for depth in 0..4 {
                    markers.push(self.apply_closure(closure, vec![Value::Int(depth)])?);
                }
                Ok(Some(Value::Array(markers)))
            }
            other => Ok(Some(other.clone())),
        }
    }

    fn eval_set_rule(&mut self, rule: ast::SetRule) -> EvalResult<Value> {
        self.eval_set_rule_with_target(rule)?;
        Ok(Value::Styles)
    }

    fn eval_set_rule_with_target(
        &mut self,
        rule: ast::SetRule,
    ) -> EvalResult<Option<ContainerKind>> {
        if let Some(cond) = rule.condition() {
            let cond_ok = match self.eval_expr(cond) {
                Ok(v) => v.as_bool()?,
                Err(e) if self.config.strict => return Err(e),
                Err(_) => return Ok(None),
            };
            if !cond_ok {
                return Ok(None);
            }
        }

        let target_name = self.resolve_builtin_element_name(&rule.target());
        let args_res = self.eval_args(rule.args());
        let (_, named_args) = match args_res {
            Ok(pair) => pair,
            Err(e) if self.config.strict => return Err(e),
            Err(_) => return Ok(None),
        };

        let container_target = match target_name.as_deref() {
            Some("list") => {
                if let Some(marker) = named_args.get("marker") {
                    self.styles.list_marker = match self.resolve_list_marker_value(marker) {
                        Ok(resolved) => resolved,
                        Err(e) if self.config.strict => return Err(e),
                        Err(_) => self.styles.list_marker.clone(),
                    };
                }
                Some(ContainerKind::List)
            }
            Some("enum") => {
                if let Some(start) = named_args.get("start") {
                    self.styles.enum_start = match start {
                        Value::Auto | Value::None => None,
                        other => other.as_int().ok(),
                    };
                }
                if let Some(numbering) = named_args.get("numbering") {
                    self.styles.enum_numbering =
                        (!matches!(numbering, Value::Auto)).then(|| numbering.clone());
                }
                Some(ContainerKind::Enum)
            }
            Some("heading") => {
                if let Some(numbering) = named_args.get("numbering") {
                    self.styles.heading_numbering = if matches!(numbering, Value::None) {
                        HeadingNumbering::Unnumbered
                    } else if matches!(numbering, Value::Auto) {
                        HeadingNumbering::Default
                    } else {
                        HeadingNumbering::Numbered(numbering.clone())
                    };
                }
                None
            }
            Some("figure") => {
                if let Some(numbering) = named_args.get("numbering") {
                    self.styles.figure_numbering =
                        (!matches!(numbering, Value::Auto)).then(|| numbering.clone());
                }
                None
            }
            _ => None,
        };

        Ok(container_target)
    }

    fn eval_show_selector(&mut self, expr: ast::Expr) -> Option<Selector> {
        if let ast::Expr::Ident(ident) = &expr {
            let name = ident.get().as_str();
            if let Some(val) = self.scopes.get(name) {
                return Selector::from_value(val);
            }
        }
        if let Some(elem_name) = self.resolve_builtin_element_name(&expr) {
            return Some(Selector::element(elem_name));
        }

        self.eval_expr(expr)
            .ok()
            .and_then(|val| Selector::from_value(&val))
    }

    fn validate_show_selector(&mut self, selector: &Selector) -> EvalResult<bool> {
        let invalid_msg = match selector {
            Selector::String(s) if s.is_empty() => Some("text selector is empty"),
            Selector::Regex(re) if re.is_match("") => Some("regex matches empty text"),
            sel if !sel.is_textual() && sel.contains_textual() => {
                Some("this selector cannot be used with show")
            }
            _ => None,
        };
        if let Some(msg) = invalid_msg {
            if self.config.strict {
                return Err(EvalError::invalid_op(msg.to_string()));
            }
            self.warn(msg);
            return Ok(false);
        }
        Ok(true)
    }

    fn eval_show_rule(&mut self, rule: ast::ShowRule) -> EvalResult<Value> {
        self.eval_show_rule_with_set_target(rule)?;
        Ok(Value::Styles)
    }

    fn eval_show_rule_with_set_target(
        &mut self,
        rule: ast::ShowRule,
    ) -> EvalResult<Option<ContainerKind>> {
        let Some(selector_expr) = rule.selector() else {
            return Ok(None);
        };
        let Some(selector) = self.eval_show_selector(selector_expr) else {
            return Ok(None);
        };
        if !self.validate_show_selector(&selector)? {
            return Ok(None);
        }

        if let ast::Expr::SetRule(set_rule) = rule.transform() {
            let target_name = self.resolve_builtin_element_name(&set_rule.target());
            if let Selector::Element(sel_name, filters) = &selector {
                if filters.is_empty()
                    && matches!(sel_name.as_str(), "list" | "enum" | "heading" | "figure")
                    && target_name.as_deref() == Some(sel_name.as_str())
                {
                    return self.eval_set_rule_with_target(set_rule);
                }
            }
            if let Some(cond) = set_rule.condition() {
                let cond_ok = match self.eval_expr(cond) {
                    Ok(v) => v.as_bool()?,
                    Err(e) if self.config.strict => return Err(e),
                    Err(_) => return Ok(None),
                };
                if !cond_ok {
                    return Ok(None);
                }
            }
            let Some(target) = target_name
                .filter(|t| matches!(t.as_str(), "list" | "enum" | "heading" | "figure"))
            else {
                return Ok(None);
            };
            let (_, mut named_args) = match self.eval_args(set_rule.args()) {
                Ok(pair) => pair,
                Err(e) if self.config.strict => return Err(e),
                Err(_) => return Ok(None),
            };
            if target == "list" {
                if let Some(marker) = named_args.get("marker").cloned() {
                    match self.resolve_list_marker_value(&marker) {
                        Ok(Some(resolved)) => {
                            named_args.insert("marker".to_string(), resolved);
                        }
                        Ok(None) => {
                            named_args.insert("marker".to_string(), Value::Auto);
                        }
                        Err(e) if self.config.strict => return Err(e),
                        Err(_) => {
                            named_args.shift_remove("marker");
                        }
                    }
                }
            }
            if named_args.is_empty() {
                return Ok(None);
            }
            let priority = self.show_rules.len();
            self.show_rules.push(ShowRule::from_set(
                selector,
                ShowSetStyle {
                    target: target.clone(),
                    args: named_args,
                },
                priority,
            ));
            return Ok(match target.as_str() {
                "list" => Some(ContainerKind::List),
                "enum" => Some(ContainerKind::Enum),
                _ => None,
            });
        }

        match self.eval_expr(rule.transform()) {
            Ok(Value::Func(closure)) => {
                self.add_show_rule(selector, closure);
                Ok(None)
            }
            Ok(replacement) => {
                let priority = self.show_rules.len();
                self.show_rules
                    .push(ShowRule::from_value(selector, replacement, priority));
                Ok(None)
            }
            Err(e) => {
                if self.config.strict {
                    Err(e)
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn eval_seq_exprs<'a>(
        &mut self,
        exprs: &mut dyn Iterator<Item = ast::Expr<'a>>,
        rule_base: usize,
        mode: SeqMode,
        group_mask: ContainerGroupMask,
    ) -> EvalResult<Value> {
        let mut in_content_mode = mode == SeqMode::Markup;
        let mut pure_value = Value::None;
        let mut committed_nodes: Vec<ContentNode> = Vec::new();
        let mut segment_nodes: Vec<ContentNode> = Vec::new();
        let mut last_content_group: Option<std::ops::Range<usize>> = None;
        let mut local_list_set = false;
        let mut local_enum_set = false;

        macro_rules! effective_mask {
            ($end:expr) => {
                ContainerGroupMask {
                    list: group_mask.list
                        || local_list_set
                        || self.has_container_rule_in(rule_base, $end, "list"),
                    enum_: group_mask.enum_
                        || local_enum_set
                        || self.has_container_rule_in(rule_base, $end, "enum"),
                    terms: group_mask.terms || self.has_container_rule_in(rule_base, $end, "terms"),
                }
            };
        }

        macro_rules! flush_segment {
            () => {
                if !segment_nodes.is_empty() {
                    let end = self.show_rules.len();
                    let mask = effective_mask!(end);
                    if rule_base < end || !mask.is_empty() {
                        let batch = std::mem::take(&mut segment_nodes);
                        let realized = self.realize_content_range_with_mask(
                            batch,
                            rule_base,
                            end,
                            &mut RealizeState::default(),
                            mask,
                        )?;
                        committed_nodes.extend(realized);
                    } else {
                        committed_nodes.append(&mut segment_nodes);
                    }
                }
            };
        }

        macro_rules! ensure_content_mode {
            () => {
                if !in_content_mode {
                    let prev = std::mem::take(&mut pure_value);
                    segment_nodes.extend(prev.into_content());
                    in_content_mode = true;
                }
            };
        }

        while let Some(expr) = exprs.next() {
            match expr {
                ast::Expr::SetRule(set_rule) => {
                    ensure_content_mode!();
                    flush_segment!();
                    last_content_group = None;
                    match self.eval_set_rule_with_target(set_rule)? {
                        Some(ContainerKind::List) => local_list_set = true,
                        Some(ContainerKind::Enum) => local_enum_set = true,
                        _ => {}
                    }
                }
                ast::Expr::ShowRule(rule) => {
                    ensure_content_mode!();
                    flush_segment!();
                    if rule.selector().is_none() {
                        if let ast::Expr::SetRule(set_rule) = rule.transform() {
                            match self.eval_set_rule_with_target(set_rule)? {
                                Some(ContainerKind::List) => local_list_set = true,
                                Some(ContainerKind::Enum) => local_enum_set = true,
                                _ => {}
                            }
                            last_content_group = None;
                            continue;
                        }
                        let transform_res = self.eval_expr(rule.transform());
                        let transform_val = match transform_res {
                            Ok(val) => val,
                            Err(e) if self.config.strict => return Err(e),
                            Err(_) => {
                                last_content_group = None;
                                continue;
                            }
                        };
                        let inner_base = self.show_rules.len();
                        let saved_styles = self.styles.clone();
                        let rest_res = self.eval_seq_exprs(
                            exprs,
                            inner_base,
                            SeqMode::Markup,
                            ContainerGroupMask::NONE,
                        );
                        self.styles = saved_styles;
                        self.show_rules.truncate(inner_base);
                        let rest_val = rest_res?;

                        let transformed = match transform_val {
                            Value::Func(closure) => self.apply_closure(
                                &closure,
                                vec![Value::Content(rest_val.into_content())],
                            )?,
                            other => other,
                        };
                        let end = self.show_rules.len();
                        let mask = effective_mask!(end);
                        let realized = if rule_base < end || !mask.is_empty() {
                            self.realize_content_range_with_mask(
                                transformed.into_content(),
                                rule_base,
                                end,
                                &mut RealizeState::default(),
                                mask,
                            )?
                        } else {
                            transformed.into_content()
                        };
                        committed_nodes.extend(realized);
                        break;
                    } else {
                        match self.eval_show_rule_with_set_target(rule)? {
                            Some(ContainerKind::List) => local_list_set = true,
                            Some(ContainerKind::Enum) => local_enum_set = true,
                            _ => {}
                        }
                        last_content_group = None;
                    }
                }
                other => {
                    let val = self.eval_expr(other)?;
                    if matches!(val, Value::Styles) {
                        ensure_content_mode!();
                        last_content_group = None;
                        continue;
                    }
                    if in_content_mode {
                        if !matches!(val, Value::None) {
                            let nodes = val.into_content();
                            if !nodes.is_empty()
                                && nodes.iter().all(|n| {
                                    matches!(n, ContentNode::Space | ContentNode::Parbreak)
                                })
                            {
                                segment_nodes.extend(nodes);
                            } else if let [single] = nodes.as_slice() {
                                if let Some((label, is_func)) = content_node_label_parts(single) {
                                    if let Some(range) =
                                        last_content_group.clone().filter(|r| r.len() > 1)
                                    {
                                        let label = label.to_string();
                                        let trivia = segment_nodes.split_off(range.end);
                                        let target = segment_nodes.split_off(range.start);
                                        segment_nodes.push(ContentNode::labeled(
                                            target, trivia, label, is_func,
                                        ));
                                        last_content_group = Some(range.start..range.start + 1);
                                        continue;
                                    }
                                }
                                let start_idx = segment_nodes.len();
                                segment_nodes.extend(nodes);
                                last_content_group = Some(start_idx..segment_nodes.len());
                            } else if nodes.is_empty() {
                                last_content_group = None;
                            } else {
                                let start_idx = segment_nodes.len();
                                segment_nodes.extend(nodes);
                                last_content_group = Some(start_idx..segment_nodes.len());
                            }
                        }
                    } else {
                        pure_value = ops::join(pure_value, val)?;
                    }
                }
            }

            if self.flow.is_some() {
                break;
            }
        }

        if in_content_mode {
            flush_segment!();
            Ok(Value::Content(committed_nodes))
        } else {
            Ok(pure_value)
        }
    }

    fn eval_markup_scoped(&mut self, markup: ast::Markup) -> EvalResult<Value> {
        let base = self.show_rules.len();
        let saved_styles = self.styles.clone();
        let res = self.eval_seq_exprs(
            &mut markup.exprs(),
            base,
            SeqMode::Markup,
            ContainerGroupMask::NONE,
        );
        self.styles = saved_styles;
        self.show_rules.truncate(base);
        res
    }

    /// Evaluate markup (the top-level content).
    pub fn eval_markup(&mut self, markup: ast::Markup) -> EvalResult<Value> {
        match self.eval_seq_exprs(
            &mut markup.exprs(),
            0,
            SeqMode::Markup,
            ContainerGroupMask::ALL,
        )? {
            Value::Content(nodes) => Ok(Value::Content(unwrap_labeled_nodes(nodes))),
            other => Ok(other),
        }
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
            ast::Expr::Numeric(numeric) => Ok(eval_numeric(numeric)),

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
            | ast::Expr::TermItem(_)
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
            // Style / Show / Passthrough
            // ================================================================
            ast::Expr::SetRule(rule) => self.eval_set_rule(rule),
            ast::Expr::ShowRule(rule) => self.eval_show_rule(rule),
            ast::Expr::Contextual(_) | ast::Expr::DestructAssignment(_) => {
                self.passthrough_expr(&expr)
            }

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
        let source = expr.to_untyped().clone().into_text().to_string();
        let typst_source = if source.starts_with('#') || source.starts_with('@') {
            source
        } else {
            format!("#{source}")
        };
        Ok(Value::Content(vec![ContentNode::RawSource(typst_source)]))
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
                let body = self.eval_markup_scoped(strong.body())?;
                Ok(Value::Content(vec![ContentNode::Strong(
                    body.into_content(),
                )]))
            }
            ast::Expr::Emph(emph) => {
                let body = self.eval_markup_scoped(emph.body())?;
                Ok(Value::Content(vec![ContentNode::Emph(body.into_content())]))
            }
            ast::Expr::Heading(heading) => {
                let level = heading.depth().get() as u8;
                let body = self.eval_markup_scoped(heading.body())?.into_content();
                match &self.styles.heading_numbering {
                    HeadingNumbering::Unnumbered => {
                        let mut fields = IndexMap::new();
                        fields.insert("level".to_string(), Value::Int(level as i64));
                        fields.insert("numbering".to_string(), Value::None);
                        fields.insert("body".to_string(), Value::Content(body));
                        Ok(Value::Content(vec![ContentNode::Element {
                            name: "heading".to_string(),
                            fields,
                        }]))
                    }
                    HeadingNumbering::Numbered(numbering) => {
                        let mut fields = IndexMap::new();
                        fields.insert("level".to_string(), Value::Int(level as i64));
                        fields.insert("numbering".to_string(), numbering.clone());
                        fields.insert("body".to_string(), Value::Content(body));
                        Ok(Value::Content(vec![ContentNode::Element {
                            name: "heading".to_string(),
                            fields,
                        }]))
                    }
                    HeadingNumbering::Default => Ok(Value::Content(vec![ContentNode::Heading {
                        level,
                        content: body,
                    }])),
                }
            }
            ast::Expr::ListItem(item) => {
                let body = self.eval_markup_scoped(item.body())?;
                Ok(Value::Content(vec![ContentNode::ListItem(
                    body.into_content(),
                )]))
            }
            ast::Expr::EnumItem(item) => {
                let number = item
                    .number()
                    .map(|n| {
                        i64::try_from(n).map_err(|_| {
                            EvalError::invalid_op(format!(
                                "enum item number {n} exceeds supported integer range"
                            ))
                        })
                    })
                    .transpose()?;
                let body = self.eval_markup_scoped(item.body())?;
                Ok(Value::Content(vec![ContentNode::EnumItem {
                    number,
                    content: body.into_content(),
                }]))
            }
            ast::Expr::TermItem(item) => {
                let term = self.eval_markup_scoped(item.term())?;
                let description = self.eval_markup_scoped(item.description())?;
                Ok(Value::Content(vec![ContentNode::term_item(
                    term.into_content(),
                    description.into_content(),
                )]))
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
                let segments = self.eval_math_content(eq.body().to_untyped())?;
                let block = eq.block();
                Ok(Value::Content(vec![ContentNode::Math { segments, block }]))
            }
            ast::Expr::Math(math) => {
                // Evaluate hash expressions inside math content
                let segments = self.eval_math_content(math.to_untyped())?;
                Ok(Value::Content(vec![ContentNode::Math {
                    segments,
                    block: false,
                }]))
            }
            ast::Expr::Label(label) => Ok(Value::Content(vec![ContentNode::Label(
                label.get().to_string(),
            )])),
            // Bare `@target` needs the document's complete label and
            // bibliography declarations to decide between `\\ref` and `\\cite`.
            // `ContentNode::Reference` is reserved for explicit `#ref(...)`.
            // Evaluated content is written back out as TYPST SOURCE for a second
            // pass, so the reference is reproduced as written. A marker carrying
            // already-converted LaTeX was re-read as text and escaped, turning
            // `@x[*y*]` into `textbf\{y\}`.
            ast::Expr::Ref(reference) => Ok(Value::Content(vec![ContentNode::RawSource(
                reference.to_untyped().clone().into_text().to_string(),
            )])),
            ast::Expr::Escape(esc) => Ok(Value::Content(vec![ContentNode::Text(
                esc.get().to_string(),
            )])),
            ast::Expr::Shorthand(sh) => Ok(Value::Content(vec![ContentNode::Text(
                sh.get().to_string(),
            )])),
            _ => unreachable!("eval_markup_element called with non-markup"),
        }
    }

    fn prepare_vfs_target(&self, raw_path: &str, kind: &str) -> EvalResult<String> {
        let base = self.current_file.as_deref().unwrap_or("");
        let resolved_path = self
            .vfs
            .resolve(base, raw_path)
            .map_err(|e| EvalError::other(e.to_string()))?;

        if self.root_file.as_deref() == Some(resolved_path.as_str())
            || self.current_file.as_deref() == Some(resolved_path.as_str())
            || self.import_stack.iter().any(|p| p == &resolved_path)
        {
            return Err(EvalError::other(format!(
                "circular {kind}: {resolved_path}"
            )));
        }
        if self.import_stack.len() >= MAX_IMPORT_DEPTH {
            return Err(EvalError::other(format!("maximum {kind} depth exceeded")));
        }

        Ok(resolved_path)
    }

    fn load_vfs_source(&self, resolved_path: &str, kind: &str) -> EvalResult<String> {
        let content = self
            .vfs
            .read_text(resolved_path)
            .map_err(|e| EvalError::other(e.to_string()))?;
        if self
            .import_bytes_remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |rem| {
                rem.checked_sub(content.len())
            })
            .is_err()
        {
            return Err(EvalError::other(format!(
                "maximum {kind} byte limit exceeded"
            )));
        }
        Ok(content)
    }

    fn spawn_child_evaluator(&self, resolved_path: String) -> Self {
        let mut sub_stack = self.import_stack.clone();
        sub_stack.push(resolved_path.clone());
        Self {
            scopes: Scopes::new(),
            flow: None,
            vfs: self.vfs.clone(),
            module_cache: self.module_cache.clone(),
            current_file: Some(resolved_path),
            root_file: self.root_file.clone(),
            state_store: self.state_store.clone(),
            counter_store: self.counter_store.clone(),
            config: self.config.clone(),
            warnings: Vec::new(),
            show_rules: Vec::new(),
            styles: EvalStyles::default(),
            current_depth: self.current_depth + 1,
            shadowed_context_marker: self.shadowed_context_marker.clone(),
            import_stack: sub_stack,
            import_bytes_remaining: self.import_bytes_remaining.clone(),
            next_dup_id: 1,
        }
    }

    fn apply_module_import(
        &mut self,
        import: &ast::ModuleImport,
        raw_source_path: &str,
        exports: &IndexMap<String, Value>,
    ) -> EvalResult<()> {
        if let Some(new_name) = import.new_name() {
            let mod_name = new_name.get().to_string();
            self.scopes
                .define(mod_name.clone(), Value::Module(mod_name, exports.clone()));
        }
        if let Some(imports) = import.imports() {
            self.bind_imports(Some(imports), exports)?;
        } else if import.new_name().is_none() {
            let default_name = super::vfs::default_import_binding_name(raw_source_path)
                .ok_or_else(|| {
                    EvalError::other(format!(
                        "cannot derive module name from import path '{raw_source_path}'"
                    ))
                })?;
            self.scopes.define(
                default_name.to_string(),
                Value::Module(default_name.to_string(), exports.clone()),
            );
        }
        Ok(())
    }

    /// Evaluate a module import.
    fn eval_import(&mut self, import: ast::ModuleImport) -> EvalResult<Value> {
        let source_expr = import.source();
        let source_val = self.eval_expr(source_expr)?;
        if let Value::Module(mod_name, exports) = source_val {
            self.apply_module_import(&import, &mod_name, &exports)?;
            return Ok(Value::None);
        }
        let path = source_val.as_str()?;

        let resolved_path = self.prepare_vfs_target(path, "module import")?;

        // Check cache - clone the module to avoid holding the lock
        let cached_module = {
            if let Ok(cache) = self.module_cache.read() {
                cache.get(&resolved_path).cloned()
            } else {
                None
            }
        };

        if let Some(module) = cached_module {
            self.apply_module_import(&import, path, &module)?;
            return Ok(Value::None);
        }

        // Load and evaluate
        let content = self.load_vfs_source(&resolved_path, "module import")?;
        let mut sub_eval = self.spawn_child_evaluator(resolved_path.clone());

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
        self.warnings.extend(sub_eval.take_warnings());

        let exports = sub_eval.scopes.top_bindings();

        // Update cache
        if let Ok(mut cache) = self.module_cache.write() {
            cache.insert(resolved_path, exports.clone());
        }

        self.apply_module_import(&import, path, &exports)?;
        Ok(Value::None)
    }

    /// Evaluate a module include.
    fn eval_include(&mut self, include: ast::ModuleInclude) -> EvalResult<Value> {
        let source_expr = include.source();
        let source_val = self.eval_expr(source_expr)?;
        let path = source_val.as_str()?;

        let resolved_path = self.prepare_vfs_target(path, "module include")?;
        let content = self.load_vfs_source(&resolved_path, "module include")?;
        let mut sub_eval = self.spawn_child_evaluator(resolved_path);
        sub_eval.styles = self.styles.clone();

        let root = parse(&content);
        if !root.errors().is_empty() {
            return Err(EvalError::syntax(format!(
                "In included file {}: parse error",
                path
            )));
        }

        if let Some(markup) = root.cast::<ast::Markup>() {
            let value = sub_eval.eval_markup_scoped(markup)?;
            self.warnings.extend(sub_eval.take_warnings());
            Ok(value)
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
                        let mut segments = item.path().iter();
                        let Some(first) = segments.next() else {
                            continue;
                        };
                        let first_name = first.get().as_str();
                        let mut current = module.get(first_name).cloned().ok_or_else(|| {
                            EvalError::undefined(format!("Module does not export '{}'", first_name))
                        })?;
                        for seg in segments {
                            let seg_name = seg.get().as_str();
                            match current {
                                Value::Module(mod_name, sub_exports) => {
                                    current =
                                        sub_exports.get(seg_name).cloned().ok_or_else(|| {
                                            EvalError::undefined(format!(
                                                "Module '{}' does not export '{}'",
                                                mod_name, seg_name
                                            ))
                                        })?;
                                }
                                other => {
                                    return Err(EvalError::type_mismatch(
                                        "module",
                                        other.type_name(),
                                    ));
                                }
                            }
                        }
                        let bound_name = item.bound_name().get().as_str();
                        self.scopes.define(bound_name.to_string(), current);
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
            "function" => Ok(Value::Type(super::value::ValType::Function)),

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
        let base = self.show_rules.len();
        let saved_styles = self.styles.clone();
        self.scopes.enter();
        let result = self.eval_seq_exprs(
            &mut block.body().exprs(),
            base,
            SeqMode::Code,
            ContainerGroupMask::NONE,
        );
        self.scopes.exit();
        self.styles = saved_styles;
        self.show_rules.truncate(base);
        result
    }

    /// Evaluate code (a sequence of expressions).
    fn eval_code(&mut self, code: ast::Code) -> EvalResult<Value> {
        let base = self.show_rules.len();
        let saved_styles = self.styles.clone();
        let result = self.eval_seq_exprs(
            &mut code.exprs(),
            base,
            SeqMode::Code,
            ContainerGroupMask::NONE,
        );
        self.styles = saved_styles;
        self.show_rules.truncate(base);
        result
    }

    /// Evaluate a content block.
    fn eval_content_block(&mut self, block: ast::ContentBlock) -> EvalResult<Value> {
        self.scopes.enter();
        let result = self.eval_markup_scoped(block.body())?;
        self.scopes.exit();
        // `[]` is an empty content value, not `none`. The distinction is
        // structural for calls such as `table(...)`, where an empty content
        // block still occupies one positional cell.
        Ok(match result {
            Value::None => Value::Content(Vec::new()),
            value => value,
        })
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
            Value::Str(s) => s
                .graphemes(true)
                .map(|g| Value::Str(g.to_string()))
                .collect(),
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
        let mut value = match binding.init() {
            Some(expr) => self.eval_expr(expr)?,
            None => Value::None,
        };

        match binding.kind() {
            ast::LetBindingKind::Normal(pattern) => {
                self.destructure(pattern, value)?;
            }
            ast::LetBindingKind::Closure(ident) => {
                // This is a function definition: #let f(x) = ...
                let fn_name = ident.get().to_string();
                if let Value::Func(ref mut arc_closure) = value {
                    Arc::make_mut(arc_closure).name = Some(fn_name.clone());
                }
                self.scopes.define(fn_name, value);
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
        let mut captures = self.scopes.capture_all();
        if let Some(ref current_file) = self.current_file {
            captures.insert(
                CLOSURE_FILE_CAPTURE_KEY.to_string(),
                Value::Str(current_file.clone()),
            );
        }

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
            Value::Content(nodes) => nodes
                .iter()
                .map(|n| match n {
                    ContentNode::Label(label) | ContentNode::LabelDef(label) => label.clone(),
                    ContentNode::Text(text) => text.clone(),
                    _ => n.to_typst(),
                })
                .collect::<String>(),
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

    #[inline(never)]
    fn eval_builtin_element_call(
        &mut self,
        name: &str,
        pos_args: Vec<Value>,
        named_args: IndexMap<String, Value>,
    ) -> EvalResult<Value> {
        match name {
            "list" => {
                let has_explicit_marker = named_args
                    .get("marker")
                    .is_some_and(|v| !matches!(v, Value::Auto));
                let marker = match named_args.get("marker") {
                    Some(m) => self.resolve_list_marker_value(m)?,
                    None => self.styles.list_marker.clone(),
                };
                let tight = named_args
                    .get("tight")
                    .and_then(|v| v.as_bool().ok())
                    .unwrap_or(true);
                let items = pos_args
                    .into_iter()
                    .map(|arg| match arg {
                        Value::Content(mut nodes)
                            if nodes.len() == 1
                                && content_node_container_kind(&nodes[0])
                                    == Some(ContainerKind::List) =>
                        {
                            nodes.pop().unwrap()
                        }
                        other => ContentNode::ListItem(other.into_content()),
                    })
                    .collect();
                let mut container = ContentNode::list_container(items, tight, marker);
                if has_explicit_marker {
                    if let ContentNode::Element { ref mut fields, .. } = container {
                        mark_explicit_fields(fields, &["marker"]);
                    }
                }
                Ok(Value::Content(vec![container]))
            }
            "enum" => {
                let has_explicit_start = named_args
                    .get("start")
                    .is_some_and(|v| !matches!(v, Value::Auto));
                let has_explicit_numbering = named_args
                    .get("numbering")
                    .is_some_and(|v| !matches!(v, Value::Auto));
                let start = match named_args.get("start") {
                    Some(Value::Auto | Value::None) => None,
                    Some(other) => Some(other.as_int()?),
                    None => self.styles.enum_start,
                };
                let numbering = match named_args.get("numbering") {
                    Some(Value::Auto) => None,
                    Some(other) => Some(other.clone()),
                    None => self.styles.enum_numbering.clone(),
                };
                let tight = named_args
                    .get("tight")
                    .and_then(|v| v.as_bool().ok())
                    .unwrap_or(true);
                let items = pos_args
                    .into_iter()
                    .map(|arg| match arg {
                        Value::Content(mut nodes)
                            if nodes.len() == 1
                                && content_node_container_kind(&nodes[0])
                                    == Some(ContainerKind::Enum) =>
                        {
                            nodes.pop().unwrap()
                        }
                        other => ContentNode::EnumItem {
                            number: None,
                            content: other.into_content(),
                        },
                    })
                    .collect();
                let mut container = ContentNode::enum_container(items, tight, start, numbering);
                let mut explicit = Vec::new();
                if has_explicit_start {
                    explicit.push("start");
                }
                if has_explicit_numbering {
                    explicit.push("numbering");
                }
                if !explicit.is_empty() {
                    if let ContentNode::Element { ref mut fields, .. } = container {
                        mark_explicit_fields(fields, &explicit);
                    }
                }
                Ok(Value::Content(vec![container]))
            }
            "terms" => {
                let tight = named_args
                    .get("tight")
                    .and_then(|v| v.as_bool().ok())
                    .unwrap_or(true);
                let mut items = Vec::with_capacity(pos_args.len());
                for arg in pos_args {
                    match arg {
                        Value::Content(mut nodes)
                            if nodes.len() == 1
                                && content_node_container_kind(&nodes[0])
                                    == Some(ContainerKind::Terms) =>
                        {
                            items.push(nodes.pop().unwrap());
                        }
                        Value::Array(mut pair) if pair.len() == 2 => {
                            let desc = pair.pop().unwrap().into_content();
                            let term = pair.pop().unwrap().into_content();
                            items.push(ContentNode::term_item(term, desc));
                        }
                        _ => return Err(EvalError::argument(
                            "terms expects (term, description) arrays or terms.item(...) arguments"
                                .to_string(),
                        )),
                    }
                }
                Ok(Value::Content(vec![ContentNode::terms_container(
                    items, tight,
                )]))
            }
            "heading" => {
                let body = pos_args
                    .into_iter()
                    .next()
                    .or_else(|| named_args.get("body").cloned())
                    .map(Value::into_content)
                    .unwrap_or_default();
                let level = named_args
                    .get("level")
                    .or_else(|| named_args.get("depth"))
                    .map(|v| v.as_int())
                    .transpose()?
                    .unwrap_or(1)
                    .clamp(1, 6) as u8;
                let has_explicit_numbering = named_args
                    .get("numbering")
                    .is_some_and(|v| !matches!(v, Value::Auto));
                let numbering = match named_args.get("numbering") {
                    Some(Value::None) => HeadingNumbering::Unnumbered,
                    Some(Value::Auto) | None => self.styles.heading_numbering.clone(),
                    Some(other) => HeadingNumbering::Numbered(other.clone()),
                };
                match numbering {
                    HeadingNumbering::Unnumbered => {
                        let mut fields = IndexMap::new();
                        fields.insert("level".to_string(), Value::Int(level as i64));
                        fields.insert("numbering".to_string(), Value::None);
                        fields.insert("body".to_string(), Value::Content(body));
                        if has_explicit_numbering {
                            mark_explicit_fields(&mut fields, &["numbering"]);
                        }
                        Ok(Value::Content(vec![ContentNode::Element {
                            name: "heading".to_string(),
                            fields,
                        }]))
                    }
                    HeadingNumbering::Numbered(val) => {
                        let mut fields = IndexMap::new();
                        fields.insert("level".to_string(), Value::Int(level as i64));
                        fields.insert("numbering".to_string(), val);
                        fields.insert("body".to_string(), Value::Content(body));
                        if has_explicit_numbering {
                            mark_explicit_fields(&mut fields, &["numbering"]);
                        }
                        Ok(Value::Content(vec![ContentNode::Element {
                            name: "heading".to_string(),
                            fields,
                        }]))
                    }
                    HeadingNumbering::Default => Ok(Value::Content(vec![ContentNode::Heading {
                        level,
                        content: body,
                    }])),
                }
            }
            "figure" => {
                let has_explicit_numbering = named_args
                    .get("numbering")
                    .is_some_and(|v| !matches!(v, Value::Auto));
                let mut func_args: Vec<Arg> = pos_args.into_iter().map(Arg::Pos).collect();
                func_args.extend(named_args.into_iter().map(|(k, v)| Arg::Named(k, v)));
                if has_explicit_numbering {
                    func_args.push(Arg::Named(
                        EXPLICIT_NUMBERING_ARG.to_string(),
                        Value::Bool(true),
                    ));
                } else if matches!(self.styles.figure_numbering, Some(Value::None)) {
                    func_args.retain(|arg| !matches!(arg, Arg::Named(k, _) if k == "numbering"));
                    func_args.push(Arg::Named("numbering".to_string(), Value::None));
                }
                Ok(Value::Content(vec![ContentNode::FuncCall {
                    name: "figure".to_string(),
                    args: func_args,
                }]))
            }
            _ => unreachable!("eval_builtin_element_call called for non-element"),
        }
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

            // Keep oversized tables unevaluated until the emitter reports them.
            if matches!(name, "table" | "grid") && super::super::table::exceeds_column_limit(args) {
                let source = call.to_untyped().clone().into_text();
                return Ok(Value::Content(vec![ContentNode::RawSource(format!(
                    "#{source}"
                ))]));
            }

            // Try built-in functions
            let (mut pos_args, named_args) = self.eval_args(args)?;
            if matches!(name, "list" | "enum" | "terms" | "heading" | "figure") {
                return self.eval_builtin_element_call(name, pos_args, named_args);
            }
            if matches!(name, "read" | "json" | "csv" | "yaml" | "toml") {
                if let Some(Value::Str(raw_path)) = pos_args.first_mut() {
                    let base = self.current_file.as_deref().unwrap_or("");
                    let resolved = self
                        .vfs
                        .resolve(base, raw_path)
                        .map_err(|e| EvalError::other(e.to_string()))?;
                    *raw_path = resolved;
                }
            } else if name == "image" {
                if let Some(ref current_file) = self.current_file {
                    if let Some(Value::Str(raw_path)) = pos_args.first_mut() {
                        if self.root_file.as_deref() != Some(current_file.as_str())
                            || raw_path.starts_with('/')
                        {
                            let resolved = self
                                .vfs
                                .resolve(current_file, raw_path)
                                .map_err(|e| EvalError::other(e.to_string()))?;
                            *raw_path = Self::rebase_vfs_path_to_root_dir(
                                &resolved,
                                self.root_file.as_deref(),
                            );
                        }
                    }
                }
            }
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
            if ident.get().as_str() == "calc" && self.scopes.get("calc").is_none() {
                let (pos_args, _) = self.eval_args(args)?;
                return call_calc(&field, pos_args);
            }
            if ident.get().as_str() == "table"
                && self.scopes.get("table").is_none()
                && matches!(field.as_str(), "header" | "cell" | "hline" | "vline")
            {
                let (pos_args, named_args) = self.eval_args(args)?;
                return match call_builtin(
                    &format!("table.{field}"),
                    pos_args,
                    named_args,
                    &self.vfs,
                ) {
                    BuiltinResult::Ok(value) => Ok(value),
                    BuiltinResult::Err(error) => Err(error),
                    BuiltinResult::NotFound => unreachable!("registered table helper"),
                };
            }
            if matches!(ident.get().as_str(), "terms" | "list" | "enum")
                && field == "item"
                && self.scopes.get(ident.get().as_str()).is_none()
            {
                let (pos_args, named_args) = self.eval_args(args)?;
                return match call_builtin(
                    &format!("{}.{field}", ident.get().as_str()),
                    pos_args,
                    named_args,
                    &self.vfs,
                ) {
                    BuiltinResult::Ok(value) => Ok(value),
                    BuiltinResult::Err(error) => Err(error),
                    BuiltinResult::NotFound => unreachable!("registered list/enum/terms helper"),
                };
            }
        }

        if field == "where" {
            if let Some(elem_name) = self.resolve_builtin_element_name(&target) {
                let (_, named_args) = self.eval_args(args)?;
                return Ok(Value::Selector(Selector::element_with_filters(
                    elem_name,
                    named_args.into_iter().collect(),
                )));
            }
        }

        // Evaluate the target
        let target_value = self.eval_expr(target)?;

        if let Value::Module(ref mod_name, ref exports) = target_value {
            return match exports.get(&field).cloned() {
                Some(Value::Func(closure)) => self.call_closure(&closure, args),
                Some(other) => Err(EvalError::invalid_op(format!(
                    "cannot call field '{}' of module '{}' ({} is not a function)",
                    field,
                    mod_name,
                    other.type_name()
                ))),
                None => Err(EvalError::undefined(format!(
                    "module '{}' does not export '{}'",
                    mod_name, field
                ))),
            };
        }

        if let Value::Dict(ref dict) = target_value {
            if !matches!(
                field.as_str(),
                "len" | "at" | "keys" | "values" | "pairs" | "contains"
            ) {
                if let Some(Value::Func(closure)) = dict.get(&field).cloned() {
                    return self.call_closure(&closure, args);
                }
            }
        }

        // Evaluate arguments
        let (pos_args, named_args) = self.eval_args(args)?;

        if field == "where" {
            if let Value::Func(ref f) = target_value {
                if let Some(elem_name) = f.builtin_name() {
                    return Ok(Value::Selector(Selector::element_with_filters(
                        elem_name,
                        named_args.into_iter().collect(),
                    )));
                }
            }
        }

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

    fn rebase_vfs_path_to_root_dir(resolved_vfs_path: &str, root_file: Option<&str>) -> String {
        let root_dir_parts: Vec<&str> = root_file
            .and_then(|rf| std::path::Path::new(rf).parent())
            .and_then(|p| p.to_str())
            .map(|s| {
                s.split('/')
                    .filter(|seg| !seg.is_empty() && *seg != ".")
                    .collect()
            })
            .unwrap_or_default();
        let target_parts: Vec<&str> = resolved_vfs_path
            .split('/')
            .filter(|seg| !seg.is_empty() && *seg != ".")
            .collect();
        let common = root_dir_parts
            .iter()
            .zip(target_parts.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let mut out: Vec<&str> = vec![".."; root_dir_parts.len() - common];
        out.extend_from_slice(&target_parts[common..]);
        out.join("/")
    }

    fn prepare_closure_captures(
        &self,
        closure: &Closure,
    ) -> (IndexMap<String, Value>, Option<String>) {
        let mut captures = closure.captures.clone();
        let defining_file = match captures.shift_remove(CLOSURE_FILE_CAPTURE_KEY) {
            Some(Value::Str(path)) => Some(path),
            _ => None,
        };
        (captures, defining_file)
    }

    fn is_builtin_or_module_name(&self, name: &str) -> bool {
        self.is_builtin_function(name)
            || matches!(
                name,
                "read"
                    | "json"
                    | "csv"
                    | "yaml"
                    | "toml"
                    | "image"
                    | "bibliography"
                    | "cite"
                    | "ref"
                    | "calc"
                    | "table"
                    | "terms"
                    | "list"
                    | "enum"
                    | "figure"
                    | "heading"
                    | "strong"
                    | "emph"
                    | "raw"
                    | "text"
                    | "link"
                    | "footnote"
                    | "align"
                    | "pad"
                    | "page"
                    | "pagebreak"
                    | "parbreak"
                    | "linebreak"
                    | "h"
                    | "v"
                    | "line"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "polygon"
            )
    }

    fn with_closure_scope<T>(&mut self, closure: &Closure, f: impl FnOnce(&mut Self) -> T) -> T {
        let (mut captures, defining_file) = self.prepare_closure_captures(closure);
        if defining_file.as_deref() == self.current_file.as_deref() {
            if let Some(root_bindings) = self.scopes.root_bindings() {
                for (k, v) in root_bindings {
                    if matches!(v, Value::Func(_))
                        && !captures.contains_key(k)
                        && !self.is_builtin_or_module_name(k)
                    {
                        captures.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        if let Some(ref fn_name) = closure.name {
            if !fn_name.starts_with("<builtin:") {
                captures.insert(fn_name.clone(), Value::Func(Arc::new(closure.clone())));
            }
        }
        let mut lexical_scopes = Scopes::with_stdlib(captures);
        lexical_scopes.enter();
        let prev_scopes = std::mem::replace(&mut self.scopes, lexical_scopes);
        let prev_file = self.current_file.clone();
        if let Some(file) = defining_file {
            self.current_file = Some(file);
        }
        let result = f(self);
        self.scopes = prev_scopes;
        self.current_file = prev_file;
        result
    }

    /// Call a user-defined closure.
    fn call_closure(&mut self, closure: &Closure, args: ast::Args) -> EvalResult<Value> {
        // Note: Recursion depth is already checked by eval_func_call
        let (pos_args, named_args) = self.eval_args(args)?;

        let result = self.with_closure_scope(closure, |this| {
            // Track which named args are consumed by regular parameters
            let mut consumed_named: std::collections::HashSet<&str> =
                std::collections::HashSet::new();

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
                    this.eval_body_source(default_source)?
                } else {
                    Value::None
                };
                this.scopes.define(param.clone(), value);
            }

            // Bind sink argument as Arguments (extra positional AND extra named)
            if let Some(ref sink_name) = closure.sink {
                let extra_pos: Vec<Value> = pos_args.into_iter().skip(num_regular_params).collect();
                let extra_named: IndexMap<String, Value> = named_args
                    .into_iter()
                    .filter(|(k, _)| !consumed_named.contains(k.as_str()))
                    .collect();

                // Bind as Arguments type, not just Array
                this.scopes.define(
                    sink_name.clone(),
                    Value::Arguments(Arguments {
                        positional: extra_pos,
                        named: extra_named,
                    }),
                );
            }

            // Parse and evaluate the body
            this.eval_body_source(&closure.body_source)
        });

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
            self.eval_markup_scoped(markup)
        } else {
            Err(EvalError::syntax(format!(
                "unexpected node type for: {}",
                source
            )))
        }
    }

    fn content_node_field(node: &ContentNode, field: &str) -> Option<Value> {
        if let Some((target, _, label, _)) = node.labeled_parts() {
            if field == "label" {
                return Some(Value::Label(label.to_string()));
            }
            if let [single] = target {
                return Self::content_node_field(single, field);
            }
            return None;
        }
        match node {
            ContentNode::Strong(c) | ContentNode::Emph(c) | ContentNode::ListItem(c)
                if field == "body" =>
            {
                Some(Value::Content(c.clone()))
            }
            ContentNode::Heading { level, content } => match field {
                "body" => Some(Value::Content(content.clone())),
                "level" | "depth" => Some(Value::Int(*level as i64)),
                _ => None,
            },
            ContentNode::EnumItem { number, content } => match field {
                "body" => Some(Value::Content(content.clone())),
                "number" => Some(number.map(Value::Int).unwrap_or(Value::Auto)),
                _ => None,
            },
            ContentNode::Raw { text, lang, block } => match field {
                "text" => Some(Value::Str(text.clone())),
                "lang" => Some(lang.clone().map(Value::Str).unwrap_or(Value::None)),
                "block" => Some(Value::Bool(*block)),
                _ => None,
            },
            ContentNode::Text(t) if field == "text" => Some(Value::Str(t.clone())),
            ContentNode::Element { name, fields } => {
                if name == "heading" && field == "depth" {
                    fields.get("depth").or_else(|| fields.get("level")).cloned()
                } else {
                    fields.get(field).cloned()
                }
            }
            _ => None,
        }
    }

    /// Evaluate field access.
    fn eval_field_access(&mut self, access: ast::FieldAccess) -> EvalResult<Value> {
        let field = access.field().get().as_str();
        if let ast::Expr::Ident(ident) = access.target() {
            if ident.get().as_str() == "math"
                && field == "equation"
                && self.scopes.get("math").is_none()
            {
                return Ok(Value::Func(Arc::new(Closure::builtin("math.equation"))));
            }
        }

        let target = self.eval_expr(access.target())?;

        match target {
            Value::Module(mod_name, exports) => exports.get(field).cloned().ok_or_else(|| {
                EvalError::undefined(format!("module '{}' does not export '{}'", mod_name, field))
            }),
            Value::Dict(dict) => dict
                .get(field)
                .cloned()
                .ok_or(EvalError::key_not_found(field.to_string())),
            Value::Func(ref f) => {
                if let Some(base) = f.builtin_name() {
                    if matches!(
                        (base, field),
                        ("terms" | "list" | "enum", "item")
                            | ("table", "header" | "cell" | "hline" | "vline")
                    ) {
                        return Ok(Value::Func(Arc::new(Closure::builtin(&format!(
                            "{base}.{field}"
                        )))));
                    }
                }
                Err(EvalError::invalid_op(format!(
                    "cannot access field '{}' on function",
                    field
                )))
            }
            Value::Content(nodes) => {
                if let [single] = nodes.as_slice() {
                    if let Some(val) = Self::content_node_field(single, field) {
                        return Ok(val);
                    }
                }
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
                | "table"
                | "grid"
                | "stack"
                | "strong"
                | "emph"
                | "raw"
                | "heading"
                | "list"
                | "enum"
                | "terms"
                | "selector"
                | "figure"
                | "image"
                | "text"
                | "cite"
                | "ref"
                | "bibliography"
                | "quote"
                | "link"
                | "footnote"
                | "align"
                | "pad"
                | "rect"
                | "circle"
                | "ellipse"
                | "line"
                | "polygon"
                | "par"
                | "parbreak"
                | "linebreak"
                | "pagebreak"
                | "colbreak"
                | "h"
                | "v"
                | "underline"
                | "strike"
                | "smallcaps"
                | "sub"
                | "super"
                | "highlight"
                | "overline"
        )
    }

    /// Create a wrapper closure for a builtin function.
    /// This allows builtin functions to be passed as arguments to higher-order functions.
    fn create_builtin_wrapper(&self, name: &str) -> Closure {
        Closure::builtin(name)
    }

    /// Apply a closure to arguments.
    fn apply_closure(&mut self, closure: &Closure, args: Vec<Value>) -> EvalResult<Value> {
        let result = self.with_closure_scope(closure, |this| {
            for (i, param) in closure.params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(Value::None);
                this.scopes.define(param.clone(), value);
            }

            this.eval_body_source(&closure.body_source)
        });

        if let Some(FlowEvent::Return(value)) = self.flow.take() {
            return Ok(value.unwrap_or(Value::None));
        }

        result
    }

    /// Evaluate math content, replacing hash expressions with their values.
    ///
    /// This function traverses a Math AST node and evaluates any embedded
    /// hash expressions (like `#n` in `$F_#n$`), returning the expanded math string.
    fn eval_math_content(&mut self, node: &SyntaxNode) -> EvalResult<Vec<MathSegment>> {
        use SyntaxKind::*;

        let mut result = Vec::new();
        let children: Vec<_> = node.children().collect();
        let mut i = 0;

        let push_source = |segments: &mut Vec<MathSegment>, text: &str| {
            if text.is_empty() {
                return;
            }

            if let Some(MathSegment::Source(existing)) = segments.last_mut() {
                existing.push_str(text);
            } else {
                segments.push(MathSegment::Source(text.to_string()));
            }
        };

        let push_value = |segments: &mut Vec<MathSegment>, value: Value| {
            if node.kind() == Named && !matches!(value, Value::Str(_) | Value::Content(_)) {
                // Named math arguments retain code types, such as augment: #(1).
                push_source(
                    segments,
                    &format!("#{}", super::value::value_to_typst_arg(&value)),
                );
            } else {
                segments.push(MathSegment::Evaluated(value));
            }
        };

        while i < children.len() {
            let child = &children[i];
            match child.kind() {
                // Hash marks the start of a code expression
                Hash => {
                    // Check for expression after hash
                    if i + 1 < children.len() {
                        let expr_node = &children[i + 1];
                        let source = || format!("#{}", SyntaxNode::clone(expr_node).into_text());
                        match expr_node.kind() {
                            // Simple identifier: #n
                            Ident => {
                                let name = expr_node.text().to_string();
                                if let Some(value) = self.scopes.get(&name) {
                                    push_value(&mut result, value.clone());
                                } else {
                                    push_source(&mut result, &format!("#{}", name));
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
                                                push_value(&mut result, value);
                                            }
                                            Err(_) => push_source(&mut result, &source()),
                                        }
                                        break;
                                    }
                                }
                                if !matches!(result.last(), Some(MathSegment::Evaluated(_))) {
                                    // No inner expression found; preserve the original source.
                                    if !expr_node
                                        .children()
                                        .any(|inner| inner.cast::<ast::Expr>().is_some())
                                    {
                                        push_source(&mut result, &source());
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
                                            push_value(&mut result, value);
                                        }
                                        Err(_) => push_source(&mut result, &source()),
                                    }
                                } else {
                                    push_source(&mut result, &source());
                                }
                                i += 2;
                                continue;
                            }
                            _ => {
                                let marked = self.shadowed_context_marker.as_deref().and_then(|marker| {
                                    if matches!(
                                        expr_node.kind(),
                                        Contextual | ContentBlock | CodeBlock
                                    ) {
                                        let outer_shadowed =
                                            super::super::scope_analysis::collect_shadowed_builtins(
                                                |name| self.scopes.contains(name),
                                            );
                                        super::super::scope_analysis::annotate_shadowed_builtins_in_node(
                                            expr_node,
                                            &outer_shadowed,
                                            marker,
                                            self.vfs.as_ref(),
                                            self.current_file.as_deref(),
                                        )
                                    } else {
                                        Option::None
                                    }
                                });
                                // Unknown expression type after hash: keep its
                                // source whole, as `#context { … }`.
                                if let Some(text) = marked {
                                    push_source(&mut result, &text);
                                } else {
                                    push_source(&mut result, &source());
                                }
                                i += 2;
                                continue;
                            }
                        }
                    } else {
                        // Hash at end with nothing after
                        push_source(&mut result, "#");
                    }
                }
                // Default strategy: if node has children, recurse; otherwise use raw text.
                // This ensures we don't miss any container nodes that might contain #expr.
                _ => {
                    if child.children().next().is_some() {
                        // Container node - recurse to find any nested #expr
                        result.extend(self.eval_math_content(child)?);
                    } else {
                        // Leaf node (no children) - use raw text
                        push_source(&mut result, child.text().as_str());
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
            _ if node.is_block_list_like() => {
                // Remove space before list/container items to ensure proper indentation
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
    expand_macros_with_vfs(source, Arc::new(NoopVfs), None, None)
}

pub(crate) fn expand_macros_with_vfs(
    source: &str,
    vfs: Arc<dyn VirtualFileSystem>,
    current_file: Option<&str>,
    shadowed_context_marker: Option<&str>,
) -> EvalResult<ExpandResult> {
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

    let mut eval = MiniEval::with_vfs(vfs);
    if let Some(file) = current_file {
        eval.set_current_file(file);
    }
    eval.shadowed_context_marker = shadowed_context_marker.map(str::to_string);
    let result = eval.eval_markup(markup)?;

    // Convert result to content nodes (show rules have already been applied sequentially)
    let transformed_nodes = match result {
        Value::Content(nodes) => nodes,
        Value::None => Vec::new(),
        other => vec![ContentNode::Text(other.display())],
    };

    // Normalize whitespace: remove consecutive Space nodes to prevent
    // indentation accumulation in loops (e.g., #for x in arr [- #x])
    let normalized_nodes = normalize_content_whitespace(transformed_nodes);

    // Convert transformed nodes to string
    let output = super::value::render_content_nodes(&normalized_nodes);

    // Collect warnings
    let warnings = eval.take_warnings();

    Ok(ExpandResult {
        output,
        nodes: normalized_nodes,
        warnings,
    })
}

/// A numeric literal (`2cm`, `-1em` after negation, `50%`, `45deg`, `1fr`) as a
/// value, so arithmetic and unary minus work on it.
fn eval_numeric(numeric: ast::Numeric) -> Value {
    let (value, unit) = numeric.get();
    match unit {
        ast::Unit::Pt => Value::Length(Length::exact(value, LengthUnit::Pt)),
        ast::Unit::Mm => Value::Length(Length::exact(value, LengthUnit::Mm)),
        ast::Unit::Cm => Value::Length(Length::exact(value, LengthUnit::Cm)),
        ast::Unit::In => Value::Length(Length::exact(value, LengthUnit::In)),
        ast::Unit::Em => Value::Length(Length::exact(value, LengthUnit::Em)),
        ast::Unit::Deg => Value::Angle(value),
        ast::Unit::Rad => Value::Angle(value.to_degrees()),
        ast::Unit::Fr => Value::Fraction(value),
        ast::Unit::Percent => Value::Ratio(value / 100.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_content_preserves_h_func_call_as_structured_segments() {
        let result = expand_macros_with_warnings("$a #h(1cm) b$").unwrap();

        assert!(
            !result.output.contains("[1cm]"),
            "structured math output should not regress to h([1cm]), got: {}",
            result.output
        );

        let ContentNode::Math { segments, block } = &result.nodes[0] else {
            panic!("expected a math content node, got: {:?}", result.nodes);
        };

        assert!(!block, "inline math should not be marked as block math");
        assert_eq!(
            segments.len(),
            3,
            "unexpected math segments: {:?}",
            segments
        );
        assert_eq!(segments[0], MathSegment::Source("a ".to_string()));
        assert_eq!(segments[2], MathSegment::Source(" b".to_string()));

        match &segments[1] {
            MathSegment::Evaluated(Value::Content(nodes)) => {
                assert!(
                    matches!(
                        nodes.first(),
                        Some(ContentNode::FuncCall { name, .. }) if name == "h"
                    ),
                    "expected the evaluated segment to preserve h(...) as a func call, got: {:?}",
                    nodes
                );
            }
            other => panic!("expected an evaluated h(...) segment, got: {:?}", other),
        }
    }

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

    #[test]
    fn test_show_regex_partial_replacement_preserves_surrounding_text() {
        let res = expand_macros_with_warnings("#show regex(\"bc\"): [X]\nabcde").unwrap();
        assert!(res.warnings.is_empty(), "{:?}", res.warnings);
        assert_eq!(res.output, "aXde");

        let multi =
            expand_macros_with_warnings("#show regex(\"b+\"): it => [(#it)]\nabbbcbb").unwrap();
        assert!(multi.warnings.is_empty(), "{:?}", multi.warnings);
        assert_eq!(multi.output, "a(bbb)c(bb)");

        let empty_re = expand_macros_with_warnings("#show regex(\"b*\"): [X]\nac").unwrap();
        assert!(!empty_re.warnings.is_empty());
        assert_eq!(empty_re.output, "ac");

        let empty_str = expand_macros_with_warnings("#show \"\": [X]\nab").unwrap();
        assert!(!empty_str.warnings.is_empty());
        assert_eq!(empty_str.output, "ab");
    }

    #[test]
    fn test_show_rule_chaining_dynamic_text_and_tie_breaking() {
        assert_eq!(
            expand_macros("#show \"cat\": [dog]\n#show \"dog\": [bird]\ncat").unwrap(),
            "bird"
        );
        assert_eq!(
            expand_macros("#show \"dog\": [bird]\n#show \"cat\": [dog]\ncat").unwrap(),
            "bird"
        );
        assert_eq!(
            expand_macros("#show \"cat\": [dog]\n#(\"cat\")").unwrap(),
            "dog"
        );
        assert_eq!(
            expand_macros("#let s = \"cat\"\n#show \"cat\": [dog]\n#(s.len()): #s").unwrap(),
            "3: dog"
        );
        assert_eq!(
            expand_macros("#show \"cat\": [dog]\nca#(\"t\")").unwrap(),
            "dog"
        );
        assert_eq!(
            expand_macros("#{ show \"cat\": [dog]; \"cat\" }").unwrap(),
            "dog"
        );
        assert_eq!(
            expand_macros("#show \"x\": [y]\n#show \"y\": [x]\nx").unwrap(),
            "x"
        );
        assert_eq!(
            expand_macros("#show \"ab\": [X]\n#show \"bc\": [Y]\nabc").unwrap(),
            "Xc"
        );
        assert_eq!(
            expand_macros("#show \"abc\": [LONG]\n#show \"ab\": [SHORT]\nabc").unwrap(),
            "SHORTc"
        );
        assert_eq!(
            expand_macros("#show \"ab\": [SHORT]\n#show \"abc\": [LONG]\nabc").unwrap(),
            "LONG"
        );
        assert_eq!(
            expand_macros("#show strong: it => emph(it.body)\n#show strong: it => [S:#it]\n*hi*")
                .unwrap(),
            "S:_hi_"
        );
        assert_eq!(
            expand_macros(
                "#show heading: it => if it.level == 1 { heading(level: 2, it.body) } else { strong(it.body) }\n= Hi"
            )
            .unwrap(),
            "*Hi*"
        );
    }

    #[test]
    fn test_show_container_selectors_separate_from_items() {
        assert_eq!(
            expand_macros("#show terms: [GLOSSARY]\n/ A: 1\n/ B: 2").unwrap(),
            "GLOSSARY"
        );
        assert_eq!(
            expand_macros("#show terms: [GLOSSARY]\n/ A: 1\n\n/ B: 2").unwrap(),
            "GLOSSARY"
        );
        assert_eq!(
            expand_macros("#show terms.item: it => [#it.term=#it.description;]\n/ A: 1\n/ B: 2")
                .unwrap(),
            "A=1 B=2"
        );
        assert_eq!(
            expand_macros("#show list: [ONELIST]\n- a\n- b").unwrap(),
            "ONELIST"
        );
        assert_eq!(
            expand_macros("#show list: [ONELIST]\n#for x in (\"a\", \"b\") [- #x]").unwrap(),
            "ONELIST"
        );
        assert_eq!(
            expand_macros("#show list: [ONELIST]\n#[#show strong: [S]\n- *a*]\n- b").unwrap(),
            "ONELIST"
        );
        assert_eq!(
            expand_macros("#let xs = [- a]\n#show list: [AFTER]\n#xs\n- b").unwrap(),
            "AFTER"
        );
        let inner_list = expand_macros("#[#show list: [INNER]\n- a]\n- b").unwrap();
        assert!(
            inner_list.contains("INNER") && inner_list.contains("- b"),
            "{inner_list}"
        );
        let split_set =
            expand_macros("#show list: [OUTER]\n#[#set list(marker: [Q])\n- a]\n- b").unwrap();
        assert_eq!(split_set, "OUTER OUTER");
        assert_eq!(
            expand_macros("#show list.item: it => [(#it.body)]\n- a\n- b").unwrap(),
            "(a) (b)"
        );
        assert_eq!(
            expand_macros("#show enum: [ONEENUM]\n+ a\n+ b").unwrap(),
            "ONEENUM"
        );
        assert_eq!(
            expand_macros("#show enum.item: it => [(#it.body)]\n+ a\n+ b").unwrap(),
            "(a) (b)"
        );
    }

    #[test]
    fn test_set_list_enum_heading_scoping_in_eval() {
        let list_out = expand_macros("#set list(marker: [Q])\n- one\n- two").unwrap();
        assert_eq!(list_out, "#list(marker: [Q], [one], [two])");

        let closure_list =
            expand_macros("#set list(marker: n => [D#n])\n- outer\n  - inner").unwrap();
        assert!(
            closure_list.contains("#list(marker: ([D0], [D1], [D2], [D3])"),
            "{closure_list}"
        );

        let enum_out = expand_macros("#set enum(start: 3)\n+ one\n+ two").unwrap();
        assert_eq!(enum_out, "#enum(start: 3, [one], [two])");

        let head_out = expand_macros("#set heading(numbering: none)\n= Title").unwrap();
        assert_eq!(head_out, "#heading(level: 1, numbering: none, [Title])");
    }

    #[test]
    fn test_show_label_selectors_and_attachment() {
        let basic = expand_macros("= Original <topic>\n#show <next>: [REPLACEMENT]\n= Next <next>")
            .unwrap();
        assert!(basic.contains("= Original"), "{basic}");
        assert!(basic.contains("<topic>"), "{basic}");
        assert!(basic.contains("REPLACEMENT"), "{basic}");
        assert!(
            !basic.contains("Next") && !basic.contains("<next>"),
            "{basic}"
        );

        assert_eq!(
            expand_macros("#let lbl = <v>\n#show lbl: [VARLBL]\n= VarHeading <v>").unwrap(),
            "VARLBL"
        );
        assert_eq!(
            expand_macros("#show label(\"fn1\"): [FNLBL]\n= FnHeading <fn1>").unwrap(),
            "FNLBL"
        );
        assert_eq!(
            expand_macros("#show label(\"s~1\"): [FNATTACH]\n= A\n#label(\"s~1\")").unwrap(),
            "FNATTACH"
        );
        assert_eq!(
            expand_macros("#show <h1-it>: it => [L#it.level:#it.body]\n= SubTitle <h1-it>")
                .unwrap(),
            "L1:SubTitle"
        );
        let lbl_field = expand_macros(
            "#show <h1-lbl>: it => [HAS=#it.has(\"label\"):LBL=#str(it.label)]\n= Title <h1-lbl>",
        )
        .unwrap();
        assert_eq!(lbl_field, r"HAS=true:LBL=\<h1-lbl\>");

        let self_wrap_label = expand_macros("#show <h>: it => [WRAP(#it)]\n= Head <h>").unwrap();
        assert!(
            self_wrap_label.contains("WRAP(")
                && self_wrap_label.contains("= Head")
                && self_wrap_label.contains("<h>"),
            "{self_wrap_label}"
        );
        let self_wrap_heading = expand_macros("#show heading: it => [H(#it)]\n= Head <h>").unwrap();
        assert!(
            self_wrap_heading.contains("H(")
                && self_wrap_heading.contains("= Head")
                && self_wrap_heading.contains("<h>"),
            "{self_wrap_heading}"
        );

        assert_eq!(
            expand_macros("#show <both>: [LBL]\n#show heading: [HEAD]\n= Both <both>").unwrap(),
            "HEAD"
        );
        assert_eq!(
            expand_macros("#show heading: [HEAD]\n#show <both>: [LBL]\n= Both <both>").unwrap(),
            "LBL"
        );
        assert_eq!(
            expand_macros(
                "#show heading: it => [H(#it.body)]\n#show <both3>: it => [L(#it)]\n= Both3 <both3>"
            )
            .unwrap(),
            "L(H(Both3))"
        );

        assert_eq!(
            expand_macros("#show selector(<or1>).or(<or2>): [OR]\n= First <or1>\n= Second <or2>")
                .unwrap()
                .trim(),
            "OR OR"
        );
        let and_out = expand_macros(
            "#show heading.where(level: 1).and(<match>): [AND]\n= One <match>\n== Two <match>",
        )
        .unwrap();
        assert!(
            and_out.contains("AND") && and_out.contains("== Two") && !and_out.contains("= One"),
            "{and_out}"
        );

        assert_eq!(
            expand_macros("#show <t1>: [TEXTREPL]\nhello world <t1>").unwrap(),
            "TEXTREPL"
        );
        assert_eq!(
            expand_macros("#show <t>: [T]\nfoo *bar* baz <t>").unwrap(),
            "foo *bar* T"
        );
        let unmatched_text =
            expand_macros("#show \"world\": [EARTH]\nhello world <unmatched>").unwrap();
        assert_eq!(unmatched_text, "hello EARTH <unmatched>");

        assert_eq!(
            expand_macros("#show <seq>: [SEQ]\n#[hello *world*] <seq>").unwrap(),
            "SEQ"
        );
        assert_eq!(
            expand_macros("#show <eq1>: [EQREPL]\n$x + 1$ <eq1>").unwrap(),
            "EQREPL"
        );
        assert_eq!(
            expand_macros("#show <fig>: [FIGREPL]\n#figure([body], caption: [cap]) <fig>").unwrap(),
            "FIGREPL"
        );
        assert_eq!(
            expand_macros("#show <x>: [X]\n#show <y>: [Y]\n= Multi <x> <y>").unwrap(),
            "Y"
        );
        assert_eq!(
            expand_macros("#show <standalone>: [ALONE]\n<standalone>").unwrap(),
            "<standalone>"
        );
    }

    #[test]
    fn test_variadic_selector_or_and_methods() {
        let or3 = expand_macros(
            "#show selector(<o1>).or(<o2>, <o3>): [MATCH]\n= A <o1>\n= B <o2>\n= C <o3>\n= D <o4>",
        )
        .unwrap();
        assert_eq!(or3.matches("MATCH").count(), 3, "{or3}");
        assert!(or3.contains("= D") && or3.contains("<o4>"), "{or3}");

        let or0 = expand_macros("#show selector(<o1>).or(): [ZERO]\n= A <o1>\n= B <o2>").unwrap();
        assert_eq!(or0.matches("ZERO").count(), 1, "{or0}");
        assert!(or0.contains("= B") && or0.contains("<o2>"), "{or0}");

        let and3 = expand_macros(
            "#show selector(heading).and(heading.where(level: 1), <a1>): [AND3]\n= E <a1>\n== F <a1>\n= G <g1>",
        )
        .unwrap();
        assert_eq!(and3.matches("AND3").count(), 1, "{and3}");
        assert!(and3.contains("== F") && and3.contains("= G"), "{and3}");

        let and0 = expand_macros("#show selector(<a1>).and(): [ZEROA]\n= E <a1>").unwrap();
        assert_eq!(and0.trim(), "ZEROA");
    }

    #[test]
    fn test_labeled_items_stay_grouped_in_containers() {
        assert_eq!(
            expand_macros("#show list: [LGROUP]\n#list.item[One] <lx1>\n#list.item[Two]").unwrap(),
            "LGROUP"
        );
        assert_eq!(
            expand_macros("#show enum: [EGROUP]\n#enum.item[One] <ex1>\n#enum.item[Two]").unwrap(),
            "EGROUP"
        );
        assert_eq!(
            expand_macros("#show terms: [TGROUP]\n#terms.item[A][B] <tx1>\n#terms.item[C][D]")
                .unwrap(),
            "TGROUP"
        );
        assert_eq!(
            expand_macros(
                "#show <lx3>: [ITEMREPL]\n#show list: [LGROUP]\n#list.item[One] <lx3>\n#list.item[Two]"
            )
            .unwrap(),
            "ITEMREPL LGROUP"
        );
        assert_eq!(
            expand_macros(
                "#show list: [LGROUP]\n#show <lx4>: [ITEMREPL]\n#list.item[One] <lx4>\n#list.item[Two]"
            )
            .unwrap(),
            "ITEMREPL LGROUP"
        );

        let surviving_list =
            expand_macros("#set list(marker: [Q])\n#list.item[One] <lx2>\n#list.item[Two]")
                .unwrap();
        assert_eq!(surviving_list, "#list(marker: [Q], [One <lx2>], [Two])");

        let surviving_enum =
            expand_macros("#set enum(start: 3)\n#enum.item[One] <ex2>\n#enum.item[Two]").unwrap();
        assert_eq!(surviving_enum, "#enum(start: 3, [One <ex2>], [Two])");
    }

    #[test]
    fn test_element_func_introspection_equality_and_display() {
        let checks = expand_macros(
            r#"
#show heading: it => if it.func() == heading and it.func() != strong and type(it.func()) == function [H-OK] else [H-BAD]
#show strong: it => if it.func() == strong [S-OK] else [S-BAD]
#show emph: it => if it.func() == emph [E-OK] else [E-BAD]
#show raw: it => if it.func() == raw [R-OK] else [R-BAD]
#show list: it => if it.func() == list [L-OK] else [L-BAD]
#show enum: it => if it.func() == enum [EN-OK] else [EN-BAD]
#show terms: it => if it.func() == terms [T-OK] else [T-BAD]
#show figure: it => if it.func() == figure [F-OK] else [F-BAD]
#show math.equation: it => if it.func() == math.equation [M-OK] else [M-BAD]
= Hi
*B* _I_ `c`
- A
+ B
/ K: V
#figure([X])
$x$
"#,
        )
        .unwrap();
        for expected in [
            "H-OK", "S-OK", "E-OK", "R-OK", "L-OK", "EN-OK", "T-OK", "F-OK", "M-OK",
        ] {
            assert!(
                checks.contains(expected),
                "missing {expected} in:\n{checks}"
            );
        }
        assert!(!checks.contains("-BAD"), "unexpected -BAD in:\n{checks}");

        let item_checks = expand_macros(
            r#"
#show list.item: it => if it.func() == list.item [LI-OK] else [LI-BAD]
#show enum.item: it => if it.func() == enum.item [EI-OK] else [EI-BAD]
#show terms.item: it => if it.func() == terms.item [TI-OK] else [TI-BAD]
- A
+ B
/ K: V
"#,
        )
        .unwrap();
        for expected in ["LI-OK", "EI-OK", "TI-OK"] {
            assert!(
                item_checks.contains(expected),
                "missing {expected} in:\n{item_checks}"
            );
        }
        assert!(!item_checks.contains("-BAD"), "{item_checks}");

        assert_eq!(expand_macros("#repr(heading)").unwrap(), "heading");
        assert_eq!(
            expand_macros("#show heading: it => [#(it.func())]\n= Hi").unwrap(),
            "heading"
        );
    }

    #[test]
    fn test_replaced_numbered_elements_preserve_hidden_ref_anchors() {
        let heading_res = expand_macros_with_warnings(
            "#set heading(numbering: \"1.\")\n#show <x>: [REPL]\n= First <x>\n= Second",
        )
        .unwrap();
        assert!(!heading_res.output.contains("__tylax_ref_anchor"));
        assert!(heading_res.output.contains("REPL") && heading_res.output.contains("= Second"));
        assert!(
            heading_res
                .nodes
                .iter()
                .any(|n| n.ref_anchor_parts() == Some(("section", "section", Some("x"), true))),
            "missing section anchor in {:?}",
            heading_res.nodes
        );

        let fig_res = expand_macros_with_warnings(
            "#show <fig>: [FIGREPL]\n#figure([body], caption: [cap]) <fig>",
        )
        .unwrap();
        assert_eq!(fig_res.output, "FIGREPL");
        assert!(
            fig_res
                .nodes
                .iter()
                .any(|n| n.ref_anchor_parts() == Some(("figure", "figure", Some("fig"), true))),
            "missing figure anchor in {:?}",
            fig_res.nodes
        );

        let replaced_with_heading = expand_macros_with_warnings(
            "#set heading(numbering: \"1.\")\n#show <x>: [= New <y>]\n= Orig <x>\n= After",
        )
        .unwrap();
        let x_anchor_pos = replaced_with_heading
            .nodes
            .iter()
            .position(|n| n.ref_anchor_parts() == Some(("section", "section", Some("x"), true)));
        let y_heading_pos = replaced_with_heading
            .nodes
            .iter()
            .position(|n| n.is_heading());
        assert!(
            matches!((x_anchor_pos, y_heading_pos), (Some(a), Some(h)) if a < h),
            "expected anchor before new heading in {:?}",
            replaced_with_heading.nodes
        );

        let closure_new_heading = expand_macros_with_warnings(
            "#set heading(numbering: \"1.\")\n#show <x>: it => [= New (#it.body) <y>]\n= Orig <x>\n= After",
        )
        .unwrap();
        assert!(
            closure_new_heading
                .nodes
                .iter()
                .any(|n| n.ref_anchor_parts() == Some(("section", "section", Some("x"), true))),
            "missing anchor for closure replacing heading with new heading: {:?}",
            closure_new_heading.nodes
        );

        let kept_it_and_new_heading = expand_macros_with_warnings(
            "#set heading(numbering: \"1.\")\n#show <x>: it => [#it\n= Extra <y>]\n= Orig <x>\n= After",
        )
        .unwrap();
        assert!(
            !kept_it_and_new_heading
                .nodes
                .iter()
                .any(|n| n.ref_anchor_parts().is_some()),
            "should not emit extra anchor when #it is preserved: {:?}",
            kept_it_and_new_heading.nodes
        );

        let fig_replaced_by_table = expand_macros_with_warnings(
            "#show <f1>: [#figure(table(columns: 2)[A][B], caption: [Tab]) <t1>]\n#figure([Img], caption: [Cap]) <f1>",
        )
        .unwrap();
        assert!(
            fig_replaced_by_table
                .nodes
                .iter()
                .any(|n| n.ref_anchor_parts() == Some(("figure", "figure", Some("f1"), true))),
            "missing figure anchor before table replacement: {:?}",
            fig_replaced_by_table.nodes
        );

        let explicit_image_kind = expand_macros_with_warnings(
            "#show <f1>: [REPL]\n#figure(table(columns: 2)[A][B], kind: image, caption: [Cap]) <f1>",
        )
        .unwrap();
        assert!(
            explicit_image_kind
                .nodes
                .iter()
                .any(|n| n.ref_anchor_parts() == Some(("figure", "figure", Some("f1"), true))),
            "kind: image should override table inference: {:?}",
            explicit_image_kind.nodes
        );

        for kind_src in [
            "#show <t1>: [REPL]\n#figure([NotATable], kind: table, caption: [Cap]) <t1>",
            "#show <t1>: [REPL]\n#figure(table(columns: 2)[A][B], kind: auto, caption: [Cap]) <t1>",
        ] {
            let kind_res = expand_macros_with_warnings(kind_src).unwrap();
            assert!(
                kind_res
                    .nodes
                    .iter()
                    .any(|n| n.ref_anchor_parts() == Some(("table", "table", Some("t1"), true))),
                "expected table anchor for `{kind_src}`: {:?}",
                kind_res.nodes
            );
        }

        let diagram_res = expand_macros_with_warnings(
            "#show <d1>: [DREPL]\n#figure([Diag], kind: \"diagram\", supplement: [Diagram], caption: [D1]) <d1>",
        )
        .unwrap();
        assert!(
            diagram_res
                .nodes
                .iter()
                .any(|n| n.ref_anchor_parts()
                    == Some(("tylaxfigdiagram", "Diagram", Some("d1"), true))),
            "expected custom diagram anchor in {:?}",
            diagram_res.nodes
        );
    }

    #[test]
    fn test_repeated_text_show_matches_are_iterative() {
        let input = format!("#show \"a\": [b]\n{}", "a".repeat(600));
        let out = expand_macros(&input).unwrap();
        assert_eq!(out, "b".repeat(600));

        let self_ref = format!("#show \"a\": it => [#it+]\n{}", "a".repeat(600));
        let self_out = expand_macros(&self_ref).unwrap();
        assert_eq!(self_out, "a+".repeat(600));
    }

    #[test]
    fn test_duplicate_origin_show_rules_wrap_without_duplicate_labels() {
        let dup_label = expand_macros_with_warnings(
            "#set heading(numbering: \"1.\")\n#show <x>: it => [\n#it\n#it\n]\n= First <x>\n= Second",
        )
        .unwrap();
        assert!(
            dup_label.nodes.iter().any(|n| n
                .dup_element_parts()
                .is_some_and(|(c, _, _, _)| c == "section")),
            "expected dup_element wrapper in {:?}",
            dup_label.nodes
        );
        assert_eq!(
            dup_label.output.matches("<x>").count(),
            1,
            "duplicate label leaked in {:?}",
            dup_label.output
        );

        let dup_elem = expand_macros_with_warnings(
            "#set heading(numbering: \"1.\")\n#show heading: it => [\n#it\n#it\n]\n= First <x>\n= Second",
        )
        .unwrap();
        let dup_count = dup_elem
            .nodes
            .iter()
            .filter(|n| {
                n.dup_element_parts()
                    .is_some_and(|(c, _, _, _)| c == "section")
            })
            .count();
        assert_eq!(
            dup_count, 2,
            "expected both duplicated headings to wrap: {:?}",
            dup_elem.nodes
        );
        assert_eq!(
            dup_elem.output.matches("<x>").count(),
            1,
            "{}",
            dup_elem.output
        );
    }

    #[test]
    fn test_conditional_show_set_where_rules_and_utf8_len() {
        let tight_list = expand_macros(
            "#show list.where(tight: true): set list(marker: [Q])\n- a\n- b\n\nPara\n\n- c\n\n- d",
        )
        .unwrap();
        assert!(
            tight_list.contains("#list(marker: [Q], [a], [b])"),
            "{tight_list}"
        );
        assert!(
            !tight_list.contains("marker: [Q], [c]"),
            "loose list should not receive tight marker: {tight_list}"
        );

        let heading_where = expand_macros(
            "#set heading(numbering: \"1.\")\n#show heading.where(level: 2): set heading(numbering: none)\n= One\n== Two\n= Three",
        )
        .unwrap();
        assert!(
            heading_where.contains("#heading(level: 2, numbering: none, [Two])"),
            "{heading_where}"
        );
        assert!(
            heading_where.contains("= One") && heading_where.contains("= Three"),
            "{heading_where}"
        );

        let enum_explicit =
            expand_macros("#set enum(numbering: \"I.\")\n3. Three\n4. Four").unwrap();
        assert!(
            enum_explicit.contains("numbering: \"I.\"")
                && enum_explicit.contains("enum.item(3, [Three])"),
            "{enum_explicit}"
        );

        let utf8_lens =
            expand_macros("#(\"é\".len()) / #(\"😀\".len()) / #(len(\"é\")) / #(len(\"😀\"))")
                .unwrap();
        assert_eq!(utf8_lens, r"2 \/ 4 \/ 2 \/ 4");

        // Interleaved duplicate emits snapshot before middle heading & duplicate references origin_id
        let interleaved = expand_macros_with_warnings(
            "#set heading(numbering: \"1.1\")\n#show <x>: it => [\n#it\n= Middle <m>\n#it\n]\n= First <x>\n= After <a>",
        )
        .unwrap();
        assert!(
            interleaved.nodes.iter().any(|n| n
                .dup_snapshot_parts()
                .is_some_and(|(c, id)| c == "section" && id > 0)),
            "expected dup_snapshot in {:?}",
            interleaved.nodes
        );
        assert!(
            interleaved.nodes.iter().any(|n| n
                .dup_element_parts()
                .is_some_and(|(c, _, oid, _)| c == "section" && oid > 0)),
            "expected dup_element with non-zero origin_id in {:?}",
            interleaved.nodes
        );

        // Single non-duplicated `#show <x>: it => [Wrap: #it]` strips its snapshot cleanly
        let non_dup = expand_macros_with_warnings(
            "#set heading(numbering: \"1.\")\n#show <x>: it => [Wrap: #it]\n= First <x>",
        )
        .unwrap();
        assert!(
            !non_dup
                .nodes
                .iter()
                .any(|n| n.dup_snapshot_parts().is_some()),
            "unexpected dup_snapshot on non-duplicated heading: {:?}",
            non_dup.nodes
        );

        // Explicit element arguments survive `show ...: set ...`
        let explicit_pres = expand_macros(
            "#show list.where(tight: true): set list(marker: [Q])\n#list(marker: [X], [one], [two])\n- a\n- b",
        )
        .unwrap();
        assert!(
            explicit_pres.contains("#list(marker: [X], [one], [two])")
                && explicit_pres.contains("#list(marker: [Q], [a], [b])"),
            "{explicit_pres}"
        );

        let explicit_heading_pres = expand_macros(
            "#set heading(numbering: \"1.\")\n#show heading.where(level: 2): set heading(numbering: none)\n#heading(level: 2, numbering: \"A.\")[ExplicitNum]\n== ImplicitSub",
        )
        .unwrap();
        assert!(
            explicit_heading_pres.contains("== ExplicitNum")
                && explicit_heading_pres
                    .contains("#heading(level: 2, numbering: none, [ImplicitSub])"),
            "{explicit_heading_pres}"
        );

        // `#set figure(numbering: none)` and `#show figure.where(kind: table): set figure(numbering: none)`
        let fig_set = expand_macros(
            "#set figure(numbering: none)\n#figure([Img1], caption: [Unnum])\n#figure([Img2], caption: [Exp], numbering: \"1\")\n#set figure(numbering: \"1\")\n#figure([Img3], caption: [Num])",
        )
        .unwrap();
        assert!(
            fig_set.contains("numbering: none")
                && !fig_set.contains("__tylax_")
                && fig_set.matches("numbering: none").count() == 1,
            "{fig_set}"
        );

        let fig_where = expand_macros(
            "#show figure.where(kind: table): set figure(numbering: none)\n#figure(table(columns: 2)[A][B], caption: [Tab])\n#figure([Img], caption: [Fig])",
        )
        .unwrap();
        assert_eq!(
            fig_where.matches("numbering: none").count(),
            1,
            "{fig_where}"
        );
    }
}
