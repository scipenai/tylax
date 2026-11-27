use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Component;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use typst_syntax::ast::{self, AstNode};
use typst_syntax::{LinkedNode, SyntaxKind, SyntaxNode};
use unicode_segmentation::UnicodeSegmentation;

use super::math_ir::{llap_body, ungroup_expr};

const MAX_STATIC_LOOP_ITERS: usize = 64;
const MAX_STATIC_LOOP_STEPS: usize = 512;
const MAX_STATIC_CALL_DEPTH: usize = 4;
const MAX_STATIC_VALUE_BYTES: usize = 64 * 1024;
const MAX_STATIC_TOTAL_VALUE_BYTES: usize = 256 * 1024;
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const MAX_STATIC_IMPORT_DEPTH: usize = 8;
#[cfg(not(target_arch = "wasm32"))]
const MAX_STATIC_IMPORT_BYTES: usize = 1024 * 1024;
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const MAX_STATIC_TOTAL_IMPORT_BYTES: usize = 4 * 1024 * 1024;
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const MAX_STATIC_TOTAL_EXPORT_ENTRIES: usize = 8 * 1024;

fn charge_value_budget(cost: usize, budget: &mut usize) -> bool {
    let Some(remaining) = budget.checked_sub(cost) else {
        return false;
    };
    *budget = remaining;
    true
}

#[derive(Clone)]
struct StaticClosure {
    node: SyntaxNode,
    captured: Rc<ScopeBindings>,
}

impl PartialEq for StaticClosure {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && Rc::ptr_eq(&self.captured, &other.captured)
    }
}

impl Eq for StaticClosure {}

#[derive(Clone)]
struct StaticArray {
    items: Rc<[StaticValue]>,
    weight: usize,
}

impl PartialEq for StaticArray {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.items, &other.items)
            || (self.weight == other.weight && self.items == other.items)
    }
}

impl Eq for StaticArray {}

#[derive(Clone, PartialEq, Eq)]
enum StaticValue {
    Str(Rc<str>),
    Bool(bool),
    Int(i64),
    Array(StaticArray),
    Closure(Rc<StaticClosure>),
    Unknown,
}

impl StaticValue {
    fn from_string(s: String, budget: &mut usize) -> Self {
        if s.len() > MAX_STATIC_VALUE_BYTES || !charge_value_budget(s.len(), budget) {
            Self::Unknown
        } else {
            Self::Str(Rc::from(s))
        }
    }

    fn concat_strs(lhs: &str, rhs: &str, budget: &mut usize) -> Self {
        let total = lhs.len().saturating_add(rhs.len());
        if total > MAX_STATIC_VALUE_BYTES || !charge_value_budget(total, budget) {
            return Self::Unknown;
        }
        let mut out = String::with_capacity(total);
        out.push_str(lhs);
        out.push_str(rhs);
        Self::Str(Rc::from(out))
    }

    fn from_vec(items: Vec<StaticValue>, budget: &mut usize) -> Self {
        let mut weight = 1usize;
        for item in &items {
            weight = weight.saturating_add(item.weight());
            if weight > MAX_STATIC_VALUE_BYTES {
                return Self::Unknown;
            }
        }
        let alloc_cost = items
            .len()
            .saturating_mul(std::mem::size_of::<StaticValue>())
            .max(1);
        if !charge_value_budget(alloc_cost, budget) {
            return Self::Unknown;
        }
        Self::Array(StaticArray {
            items: Rc::from(items),
            weight,
        })
    }

    fn weight(&self) -> usize {
        match self {
            Self::Str(s) => 1 + s.len(),
            Self::Array(arr) => arr.weight,
            Self::Bool(_) | Self::Int(_) | Self::Closure(_) | Self::Unknown => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlFlow {
    Normal,
    Continue,
    Break,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct ScopeBindings {
    h_shadowed: bool,
    measure_shadowed: bool,
    terms_shadowed: bool,
    range_shadowed: bool,
    values: HashMap<String, StaticValue>,
}

#[derive(Clone, Default)]
struct ModuleExports {
    terms: bool,
    range: bool,
    values: HashMap<String, StaticValue>,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Clone, Copy)]
struct ImportBaseDirs<'a> {
    project_root: &'a Path,
    /// Logical (virtual) directory path relative to `project_root` (empty at project root).
    current_rel: &'a Path,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct WalkState<'a> {
    offsets: Option<&'a mut Vec<usize>>,
    source_dir: Option<ImportBaseDirs<'a>>,
    visited: &'a mut HashSet<PathBuf>,
    module_cache: &'a mut HashMap<PathBuf, Option<Rc<ModuleExports>>>,
    import_depth: usize,
    import_byte_budget: &'a mut usize,
    dead_import_byte_budget: &'a mut usize,
    export_entry_budget: &'a mut usize,
    dead_export_entry_budget: &'a mut usize,
    step_budget: usize,
    dead_step_budget: usize,
    value_budget: usize,
    dead_value_budget: usize,
    in_dead_branch: bool,
    export_incomplete: bool,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl WalkState<'_> {
    fn active_import_byte_budget(&mut self) -> &mut usize {
        if self.in_dead_branch {
            self.dead_import_byte_budget
        } else {
            self.import_byte_budget
        }
    }

    fn active_export_entry_budget(&mut self) -> &mut usize {
        if self.in_dead_branch {
            self.dead_export_entry_budget
        } else {
            self.export_entry_budget
        }
    }

    fn active_value_budget(&mut self) -> &mut usize {
        if self.in_dead_branch {
            &mut self.dead_value_budget
        } else {
            &mut self.value_budget
        }
    }

    fn remaining_loop_steps(&self) -> usize {
        if self.in_dead_branch {
            self.dead_step_budget
        } else {
            self.step_budget
        }
    }

    fn consume_loop_step(&mut self) -> bool {
        let budget = if self.in_dead_branch {
            &mut self.dead_step_budget
        } else {
            &mut self.step_budget
        };
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        true
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_initial_import_dirs(source_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let effective_source = if source_dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        source_dir
    };
    if let Some(typst_root) = std::env::var_os("TYPST_ROOT") {
        if !typst_root.is_empty() {
            let canon_root = std::fs::canonicalize(typst_root).ok()?;
            if !canon_root.is_dir() {
                return None;
            }
            let canon_source = std::fs::canonicalize(effective_source).ok()?;
            let rel = canon_source.strip_prefix(&canon_root).ok()?.to_path_buf();
            return Some((canon_root, rel));
        }
    }
    let root =
        std::fs::canonicalize(effective_source).unwrap_or_else(|_| effective_source.to_path_buf());
    Some((root, PathBuf::new()))
}

#[cfg(not(target_arch = "wasm32"))]
fn push_relative_components(rel: &mut PathBuf, path: &Path, allow_root_dir: bool) -> Option<()> {
    for component in path.components() {
        match component {
            Component::Normal(part) => rel.push(part),
            Component::CurDir => {}
            Component::RootDir if allow_root_dir => {}
            Component::RootDir | Component::Prefix(_) => return None,
            Component::ParentDir => {
                if !rel.pop() {
                    return None;
                }
            }
        }
    }
    Some(())
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_typst_import_rel_path(dirs: ImportBaseDirs<'_>, import_path: &str) -> Option<PathBuf> {
    if import_path.is_empty() || import_path.starts_with('@') {
        return None;
    }
    let (mut rel, path_to_walk, allow_root_dir) =
        if let Some(root_rel) = import_path.strip_prefix('/') {
            (PathBuf::new(), root_rel, true)
        } else {
            let mut base = PathBuf::new();
            push_relative_components(&mut base, dirs.current_rel, false)?;
            (base, import_path, false)
        };
    push_relative_components(&mut rel, Path::new(path_to_walk), allow_root_dir)?;
    (!rel.as_os_str().is_empty()).then_some(rel)
}

pub(crate) fn unique_shadowed_marker(input: &str) -> String {
    let mut idx = 0usize;
    loop {
        // Keep the `__tylax_shadowed_ctx_N__` block-comment token intact so math
        // and markup markers round-trip through preprocessing without collisions.
        let candidate = format!("/*__tylax_shadowed_ctx_{idx}__*/");
        if !input.contains(&candidate) {
            return candidate;
        }
        idx += 1;
    }
}

pub(crate) fn annotate_shadowed_builtins_in_dir(
    input: &str,
    math_only: bool,
    marker: &str,
    source_dir: Option<&Path>,
) -> String {
    let root = if math_only {
        typst_syntax::parse_math(input)
    } else {
        typst_syntax::parse(input)
    };
    let linked = LinkedNode::new(&root);
    annotate_shadowed_builtins_in_linked(&linked, input, false, false, false, marker, source_dir)
        .unwrap_or_else(|| input.to_string())
}

pub(crate) fn annotate_shadowed_builtins_in_node(
    node: &SyntaxNode,
    h_shadowed: bool,
    measure_shadowed: bool,
    terms_shadowed: bool,
    marker: &str,
) -> Option<String> {
    let source = SyntaxNode::clone(node).into_text().to_string();
    let linked = LinkedNode::new(node);
    annotate_shadowed_builtins_in_linked(
        &linked,
        &source,
        h_shadowed,
        measure_shadowed,
        terms_shadowed,
        marker,
        None,
    )
    .map(|annotated| format!("#{annotated}"))
}

pub(crate) fn is_shadowed_node(node: &SyntaxNode) -> bool {
    super::math_ir::is_shadowed_node(node)
}

fn annotate_shadowed_builtins_in_linked(
    root: &LinkedNode<'_>,
    source: &str,
    h_shadowed: bool,
    measure_shadowed: bool,
    terms_shadowed: bool,
    marker: &str,
    source_dir: Option<&Path>,
) -> Option<String> {
    let mut scopes = vec![Rc::new(ScopeBindings {
        h_shadowed,
        measure_shadowed,
        terms_shadowed,
        range_shadowed: false,
        values: HashMap::new(),
    })];
    let mut offsets = Vec::new();
    let mut visited = HashSet::new();
    let mut module_cache = HashMap::new();
    let mut import_byte_budget = MAX_STATIC_TOTAL_IMPORT_BYTES;
    let mut dead_import_byte_budget = MAX_STATIC_TOTAL_IMPORT_BYTES;
    let mut export_entry_budget = MAX_STATIC_TOTAL_EXPORT_ENTRIES;
    let mut dead_export_entry_budget = MAX_STATIC_TOTAL_EXPORT_ENTRIES;
    #[cfg(not(target_arch = "wasm32"))]
    let resolved_dirs = source_dir.and_then(resolve_initial_import_dirs);
    #[cfg(not(target_arch = "wasm32"))]
    let import_dirs = resolved_dirs
        .as_ref()
        .map(|(project_root, current_rel)| ImportBaseDirs {
            project_root: project_root.as_path(),
            current_rel: current_rel.as_path(),
        });
    #[cfg(target_arch = "wasm32")]
    let import_dirs = source_dir.map(|dir| ImportBaseDirs {
        project_root: dir,
        current_rel: Path::new(""),
    });
    let mut state = WalkState {
        offsets: Some(&mut offsets),
        source_dir: import_dirs,
        visited: &mut visited,
        module_cache: &mut module_cache,
        import_depth: 0,
        import_byte_budget: &mut import_byte_budget,
        dead_import_byte_budget: &mut dead_import_byte_budget,
        export_entry_budget: &mut export_entry_budget,
        dead_export_entry_budget: &mut dead_export_entry_budget,
        step_budget: MAX_STATIC_LOOP_STEPS,
        dead_step_budget: MAX_STATIC_LOOP_STEPS,
        value_budget: MAX_STATIC_TOTAL_VALUE_BYTES,
        dead_value_budget: MAX_STATIC_TOTAL_VALUE_BYTES,
        in_dead_branch: false,
        export_incomplete: false,
    };
    let _ = walk_scopes(root, &mut scopes, &mut state);
    if offsets.is_empty() {
        return None;
    }
    Some(splice_marker_at_offsets(source, &offsets, marker))
}

fn splice_marker_at_offsets(source: &str, offsets: &[usize], marker: &str) -> String {
    let mut sorted = offsets.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out = String::with_capacity(source.len() + sorted.len() * marker.len());
    let mut last = 0usize;
    for offset in sorted {
        if offset >= last && offset <= source.len() && source.is_char_boundary(offset) {
            out.push_str(&source[last..offset]);
            out.push_str(marker);
            last = offset;
        }
    }
    out.push_str(&source[last..]);
    out
}

fn is_llap_shadowed(scopes: &[Rc<ScopeBindings>]) -> bool {
    scopes
        .iter()
        .any(|scope| scope.h_shadowed || scope.measure_shadowed)
}

fn is_terms_shadowed(scopes: &[Rc<ScopeBindings>]) -> bool {
    scopes.iter().any(|scope| scope.terms_shadowed)
}

fn is_range_shadowed(scopes: &[Rc<ScopeBindings>]) -> bool {
    scopes.iter().any(|scope| scope.range_shadowed)
}

fn mark_binding(name: &str, scope: &mut ScopeBindings) {
    match name {
        "h" => scope.h_shadowed = true,
        "measure" => scope.measure_shadowed = true,
        "terms" => scope.terms_shadowed = true,
        "range" => scope.range_shadowed = true,
        _ => {}
    }
    scope.values.insert(name.to_string(), StaticValue::Unknown);
}

fn record_pattern_bindings(pattern: ast::Pattern<'_>, scope: &mut ScopeBindings) {
    match pattern {
        ast::Pattern::Normal(ast::Expr::Ident(ident)) => {
            mark_binding(ident.get().as_str(), scope);
        }
        ast::Pattern::Parenthesized(paren) => {
            record_pattern_bindings(paren.pattern(), scope);
        }
        ast::Pattern::Destructuring(destruct) => {
            for item in destruct.items() {
                match item {
                    ast::DestructuringItem::Pattern(inner) => {
                        record_pattern_bindings(inner, scope);
                    }
                    ast::DestructuringItem::Named(named) => {
                        record_pattern_bindings(named.pattern(), scope);
                    }
                    ast::DestructuringItem::Spread(spread) => {
                        if let Some(ast::Expr::Ident(ident)) = spread.sink_expr() {
                            mark_binding(ident.get().as_str(), scope);
                        }
                    }
                }
            }
        }
        ast::Pattern::Placeholder(_) | ast::Pattern::Normal(_) => {}
    }
}

fn record_params_bindings(params: ast::Params<'_>, scope: &mut ScopeBindings) {
    for param in params.children() {
        match param {
            ast::Param::Pos(pattern) => record_pattern_bindings(pattern, scope),
            ast::Param::Named(named) => mark_binding(named.name().get().as_str(), scope),
            ast::Param::Spread(spread) => {
                if let Some(ident) = spread.sink_ident() {
                    mark_binding(ident.get().as_str(), scope);
                }
            }
        }
    }
}

fn default_import_binding_name(source: &str) -> Option<&str> {
    if source.starts_with('@') {
        let name = source.split('/').nth(1)?.split(':').next()?;
        (!name.is_empty()).then_some(name)
    } else {
        let file = source.rsplit(['/', '\\']).next()?;
        let stem = file.strip_suffix(".typ").unwrap_or(file);
        (!stem.is_empty()).then_some(stem)
    }
}

fn simple_pattern_ident(pattern: ast::Pattern<'_>) -> Option<&str> {
    match pattern {
        ast::Pattern::Normal(ast::Expr::Ident(ident)) => Some(ident.get().as_str()),
        ast::Pattern::Parenthesized(paren) => simple_pattern_ident(paren.pattern()),
        _ => None,
    }
}

fn capture_closure_env(
    closure: ast::Closure<'_>,
    scopes: &[Rc<ScopeBindings>],
) -> Rc<ScopeBindings> {
    let mut referenced = HashSet::new();
    collect_ident_names(closure.to_untyped(), &mut referenced);

    let mut captured = ScopeBindings {
        h_shadowed: is_llap_shadowed(scopes),
        measure_shadowed: false,
        terms_shadowed: is_terms_shadowed(scopes),
        range_shadowed: is_range_shadowed(scopes),
        values: HashMap::with_capacity(referenced.len()),
    };

    for name in referenced {
        if let Some(val) = scopes
            .iter()
            .rev()
            .find_map(|scope| scope.values.get(&name))
        {
            captured.values.insert(name, val.clone());
        }
    }

    Rc::new(captured)
}

fn eval_static_value(
    expr: ast::Expr<'_>,
    scopes: &[Rc<ScopeBindings>],
    budget: &mut usize,
) -> StaticValue {
    eval_static_value_with_depth(expr, scopes, 0, budget)
}

fn eval_static_closure_call(
    closure_val: &StaticClosure,
    call: ast::FuncCall<'_>,
    caller_scopes: &[Rc<ScopeBindings>],
    depth: usize,
    budget: &mut usize,
) -> StaticValue {
    if depth >= MAX_STATIC_CALL_DEPTH {
        return StaticValue::Unknown;
    }
    let Some(closure) = closure_val.node.cast::<ast::Closure>() else {
        return StaticValue::Unknown;
    };

    let mut pos_args = Vec::new();
    let mut named_args = HashMap::new();
    for arg in call.args().items() {
        match arg {
            ast::Arg::Pos(expr) => {
                pos_args.push(eval_static_value_with_depth(
                    expr,
                    caller_scopes,
                    depth + 1,
                    budget,
                ));
            }
            ast::Arg::Named(named) => {
                let val =
                    eval_static_value_with_depth(named.expr(), caller_scopes, depth + 1, budget);
                named_args.insert(named.name().get().to_string(), val);
            }
            ast::Arg::Spread(_) => return StaticValue::Unknown,
        }
    }

    let mut call_scopes = vec![
        Rc::clone(&closure_val.captured),
        Rc::new(ScopeBindings::default()),
    ];
    let mut pos_idx = 0usize;

    for param in closure.params().children() {
        match param {
            ast::Param::Pos(pattern) => {
                let Some(val) = pos_args.get(pos_idx).cloned() else {
                    return StaticValue::Unknown;
                };
                pos_idx += 1;
                if let Some(scope_rc) = call_scopes.last_mut() {
                    let scope = Rc::make_mut(scope_rc);
                    record_pattern_bindings(pattern, scope);
                    if let Some(name) = simple_pattern_ident(pattern) {
                        scope.values.insert(name.to_string(), val);
                    }
                }
            }
            ast::Param::Named(named) => {
                let name = named.name().get().as_str();
                let val = named_args.remove(name).unwrap_or_else(|| {
                    eval_static_value_with_depth(named.expr(), &call_scopes, depth + 1, budget)
                });
                if let Some(scope_rc) = call_scopes.last_mut() {
                    let scope = Rc::make_mut(scope_rc);
                    mark_binding(name, scope);
                    scope.values.insert(name.to_string(), val);
                }
            }
            ast::Param::Spread(_) => return StaticValue::Unknown,
        }
    }

    if pos_idx != pos_args.len() || !named_args.is_empty() {
        return StaticValue::Unknown;
    }

    eval_static_value_with_depth(closure.body(), &call_scopes, depth + 1, budget)
}

fn eval_builtin_range_call(
    call: ast::FuncCall<'_>,
    scopes: &[Rc<ScopeBindings>],
    depth: usize,
    budget: &mut usize,
) -> StaticValue {
    let mut args = Vec::new();
    for item in call.args().items() {
        match item {
            ast::Arg::Pos(arg) => {
                match eval_static_value_with_depth(arg, scopes, depth + 1, budget) {
                    StaticValue::Int(v) => args.push(v),
                    _ => return StaticValue::Unknown,
                }
            }
            _ => return StaticValue::Unknown,
        }
    }
    let (start, end) = match args.as_slice() {
        [end] => (0, *end),
        [start, end] => (*start, *end),
        _ => return StaticValue::Unknown,
    };
    if end <= start {
        return StaticValue::from_vec(Vec::new(), budget);
    }
    if end.saturating_sub(start) > MAX_STATIC_LOOP_ITERS as i64 {
        return StaticValue::Unknown;
    }
    StaticValue::from_vec((start..end).map(StaticValue::Int).collect(), budget)
}

fn eval_static_value_with_depth(
    expr: ast::Expr<'_>,
    scopes: &[Rc<ScopeBindings>],
    depth: usize,
    budget: &mut usize,
) -> StaticValue {
    if depth >= MAX_STATIC_CALL_DEPTH {
        return StaticValue::Unknown;
    }
    match ungroup_expr(expr) {
        ast::Expr::Str(source) => StaticValue::from_string(source.get().to_string(), budget),
        ast::Expr::Bool(b) => StaticValue::Bool(b.get()),
        ast::Expr::Int(i) => StaticValue::Int(i.get()),
        ast::Expr::Ident(ident) => scopes
            .iter()
            .rev()
            .find_map(|scope| scope.values.get(ident.get().as_str()).cloned())
            .unwrap_or(StaticValue::Unknown),
        ast::Expr::Array(arr) => {
            let mut out = Vec::new();
            for item in arr.items() {
                if out.len() >= MAX_STATIC_LOOP_ITERS {
                    return StaticValue::Unknown;
                }
                match item {
                    ast::ArrayItem::Pos(inner) => {
                        out.push(eval_static_value_with_depth(
                            inner,
                            scopes,
                            depth + 1,
                            budget,
                        ));
                    }
                    ast::ArrayItem::Spread(_) => return StaticValue::Unknown,
                }
            }
            StaticValue::from_vec(out, budget)
        }
        ast::Expr::Closure(closure) => StaticValue::Closure(Rc::new(StaticClosure {
            node: closure.to_untyped().clone(),
            captured: capture_closure_env(closure, scopes),
        })),
        ast::Expr::CodeBlock(block) => {
            let exprs: Vec<_> = block.body().exprs().collect();
            if exprs.len() == 1 {
                eval_static_value_with_depth(exprs[0], scopes, depth + 1, budget)
            } else {
                StaticValue::Unknown
            }
        }
        ast::Expr::FuncReturn(ret) => match ret.body() {
            Some(body) => eval_static_value_with_depth(body, scopes, depth + 1, budget),
            None => StaticValue::Unknown,
        },
        ast::Expr::Conditional(cond) => {
            match eval_static_value_with_depth(cond.condition(), scopes, depth + 1, budget) {
                StaticValue::Bool(true) => {
                    eval_static_value_with_depth(cond.if_body(), scopes, depth + 1, budget)
                }
                StaticValue::Bool(false) => match cond.else_body() {
                    Some(else_body) => {
                        eval_static_value_with_depth(else_body, scopes, depth + 1, budget)
                    }
                    None => StaticValue::Unknown,
                },
                _ => StaticValue::Unknown,
            }
        }
        ast::Expr::Unary(unary) => match unary.op() {
            ast::UnOp::Not => {
                match eval_static_value_with_depth(unary.expr(), scopes, depth + 1, budget) {
                    StaticValue::Bool(b) => StaticValue::Bool(!b),
                    _ => StaticValue::Unknown,
                }
            }
            ast::UnOp::Pos => {
                match eval_static_value_with_depth(unary.expr(), scopes, depth + 1, budget) {
                    StaticValue::Int(v) => StaticValue::Int(v),
                    _ => StaticValue::Unknown,
                }
            }
            ast::UnOp::Neg => {
                match eval_static_value_with_depth(unary.expr(), scopes, depth + 1, budget) {
                    StaticValue::Int(v) => v
                        .checked_neg()
                        .map_or(StaticValue::Unknown, StaticValue::Int),
                    _ => StaticValue::Unknown,
                }
            }
        },
        ast::Expr::Binary(bin) => match bin.op() {
            ast::BinOp::Add => match (
                eval_static_value_with_depth(bin.lhs(), scopes, depth + 1, budget),
                eval_static_value_with_depth(bin.rhs(), scopes, depth + 1, budget),
            ) {
                (StaticValue::Str(lhs), StaticValue::Str(rhs)) => {
                    StaticValue::concat_strs(&lhs, &rhs, budget)
                }
                (StaticValue::Int(lhs), StaticValue::Int(rhs)) => lhs
                    .checked_add(rhs)
                    .map_or(StaticValue::Unknown, StaticValue::Int),
                _ => StaticValue::Unknown,
            },
            ast::BinOp::Sub => match (
                eval_static_value_with_depth(bin.lhs(), scopes, depth + 1, budget),
                eval_static_value_with_depth(bin.rhs(), scopes, depth + 1, budget),
            ) {
                (StaticValue::Int(lhs), StaticValue::Int(rhs)) => lhs
                    .checked_sub(rhs)
                    .map_or(StaticValue::Unknown, StaticValue::Int),
                _ => StaticValue::Unknown,
            },
            ast::BinOp::And => {
                match eval_static_value_with_depth(bin.lhs(), scopes, depth + 1, budget) {
                    StaticValue::Bool(false) => StaticValue::Bool(false),
                    StaticValue::Bool(true) => {
                        match eval_static_value_with_depth(bin.rhs(), scopes, depth + 1, budget) {
                            StaticValue::Bool(b) => StaticValue::Bool(b),
                            _ => StaticValue::Unknown,
                        }
                    }
                    _ => StaticValue::Unknown,
                }
            }
            ast::BinOp::Or => {
                match eval_static_value_with_depth(bin.lhs(), scopes, depth + 1, budget) {
                    StaticValue::Bool(true) => StaticValue::Bool(true),
                    StaticValue::Bool(false) => {
                        match eval_static_value_with_depth(bin.rhs(), scopes, depth + 1, budget) {
                            StaticValue::Bool(b) => StaticValue::Bool(b),
                            _ => StaticValue::Unknown,
                        }
                    }
                    _ => StaticValue::Unknown,
                }
            }
            ast::BinOp::Eq | ast::BinOp::Neq => {
                let eq = match (
                    eval_static_value_with_depth(bin.lhs(), scopes, depth + 1, budget),
                    eval_static_value_with_depth(bin.rhs(), scopes, depth + 1, budget),
                ) {
                    (StaticValue::Str(a), StaticValue::Str(b)) => Some(a == b),
                    (StaticValue::Bool(a), StaticValue::Bool(b)) => Some(a == b),
                    (StaticValue::Int(a), StaticValue::Int(b)) => Some(a == b),
                    _ => None,
                };
                match (eq, bin.op()) {
                    (Some(v), ast::BinOp::Eq) => StaticValue::Bool(v),
                    (Some(v), ast::BinOp::Neq) => StaticValue::Bool(!v),
                    _ => StaticValue::Unknown,
                }
            }
            ast::BinOp::Lt | ast::BinOp::Leq | ast::BinOp::Gt | ast::BinOp::Geq => {
                match (
                    eval_static_value_with_depth(bin.lhs(), scopes, depth + 1, budget),
                    eval_static_value_with_depth(bin.rhs(), scopes, depth + 1, budget),
                ) {
                    (StaticValue::Int(a), StaticValue::Int(b)) => {
                        let cmp = match bin.op() {
                            ast::BinOp::Lt => a < b,
                            ast::BinOp::Leq => a <= b,
                            ast::BinOp::Gt => a > b,
                            ast::BinOp::Geq => a >= b,
                            _ => unreachable!(),
                        };
                        StaticValue::Bool(cmp)
                    }
                    _ => StaticValue::Unknown,
                }
            }
            _ => StaticValue::Unknown,
        },
        ast::Expr::FuncCall(call) => {
            if let StaticValue::Closure(closure_val) =
                eval_static_value_with_depth(call.callee(), scopes, depth + 1, budget)
            {
                eval_static_closure_call(&closure_val, call, scopes, depth + 1, budget)
            } else if matches!(
                ungroup_expr(call.callee()),
                ast::Expr::Ident(ident) if ident.get().as_str() == "range"
            ) && !is_range_shadowed(scopes)
            {
                eval_builtin_range_call(call, scopes, depth, budget)
            } else {
                StaticValue::Unknown
            }
        }
        _ => StaticValue::Unknown,
    }
}

fn static_for_iterable_values(
    expr: ast::Expr<'_>,
    scopes: &[Rc<ScopeBindings>],
    budget: &mut usize,
) -> Option<Vec<StaticValue>> {
    match eval_static_value(expr, scopes, budget) {
        StaticValue::Array(arr) if arr.items.len() <= MAX_STATIC_LOOP_ITERS => {
            Some(arr.items.iter().cloned().collect())
        }
        StaticValue::Str(text) => {
            if text.graphemes(true).nth(MAX_STATIC_LOOP_ITERS).is_some() {
                return None;
            }
            Some(
                text.graphemes(true)
                    .map(|g| StaticValue::from_string(g.to_string(), budget))
                    .collect(),
            )
        }
        _ => None,
    }
}

fn resolve_import_source_string(
    expr: ast::Expr<'_>,
    scopes: &[Rc<ScopeBindings>],
    budget: &mut usize,
) -> Option<String> {
    match eval_static_value(expr, scopes, budget) {
        StaticValue::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

fn eval_static_bool(
    expr: ast::Expr<'_>,
    scopes: &[Rc<ScopeBindings>],
    budget: &mut usize,
) -> Option<bool> {
    match eval_static_value(expr, scopes, budget) {
        StaticValue::Bool(b) => Some(b),
        _ => None,
    }
}

fn record_let_normal_binding(
    pattern: ast::Pattern<'_>,
    init: Option<ast::Expr<'_>>,
    scopes: &mut [Rc<ScopeBindings>],
    budget: &mut usize,
) {
    let resolved = init.map_or(StaticValue::Unknown, |expr| {
        eval_static_value(expr, scopes, budget)
    });
    let Some(scope_rc) = scopes.last_mut() else {
        return;
    };
    let scope = Rc::make_mut(scope_rc);
    record_pattern_bindings(pattern, scope);
    if let Some(name) = simple_pattern_ident(pattern) {
        scope.values.insert(name.to_string(), resolved);
    }
}

fn record_assign_binding(
    bin: ast::Binary<'_>,
    scopes: &mut [Rc<ScopeBindings>],
    budget: &mut usize,
) {
    let op = bin.op();
    if !matches!(
        op,
        ast::BinOp::Assign
            | ast::BinOp::AddAssign
            | ast::BinOp::SubAssign
            | ast::BinOp::MulAssign
            | ast::BinOp::DivAssign
    ) {
        return;
    }
    let ast::Expr::Ident(ident) = ungroup_expr(bin.lhs()) else {
        if op == ast::BinOp::Assign {
            let mut targets = HashSet::new();
            collect_ident_names(bin.lhs().to_untyped(), &mut targets);
            for name in targets {
                if let Some(scope_rc) = scopes
                    .iter_mut()
                    .rev()
                    .find(|scope| scope.values.contains_key(&name))
                {
                    Rc::make_mut(scope_rc)
                        .values
                        .insert(name, StaticValue::Unknown);
                } else if let Some(scope_rc) = scopes.last_mut() {
                    Rc::make_mut(scope_rc)
                        .values
                        .insert(name, StaticValue::Unknown);
                }
            }
        }
        return;
    };
    let name = ident.get().as_str();
    let resolved = match op {
        ast::BinOp::Assign => eval_static_value(bin.rhs(), scopes, budget),
        ast::BinOp::AddAssign => {
            let existing = scopes
                .iter()
                .rev()
                .find_map(|scope| scope.values.get(name).cloned())
                .unwrap_or(StaticValue::Unknown);
            match (existing, eval_static_value(bin.rhs(), scopes, budget)) {
                (StaticValue::Str(lhs), StaticValue::Str(rhs)) => {
                    StaticValue::concat_strs(&lhs, &rhs, budget)
                }
                (StaticValue::Int(lhs), StaticValue::Int(rhs)) => lhs
                    .checked_add(rhs)
                    .map_or(StaticValue::Unknown, StaticValue::Int),
                _ => StaticValue::Unknown,
            }
        }
        ast::BinOp::SubAssign => {
            let existing = scopes
                .iter()
                .rev()
                .find_map(|scope| scope.values.get(name).cloned())
                .unwrap_or(StaticValue::Unknown);
            match (existing, eval_static_value(bin.rhs(), scopes, budget)) {
                (StaticValue::Int(lhs), StaticValue::Int(rhs)) => lhs
                    .checked_sub(rhs)
                    .map_or(StaticValue::Unknown, StaticValue::Int),
                _ => StaticValue::Unknown,
            }
        }
        _ => StaticValue::Unknown,
    };
    if let Some(scope_rc) = scopes
        .iter_mut()
        .rev()
        .find(|scope| scope.values.contains_key(name))
    {
        Rc::make_mut(scope_rc)
            .values
            .insert(name.to_string(), resolved);
    } else if let Some(scope_rc) = scopes.last_mut() {
        Rc::make_mut(scope_rc)
            .values
            .insert(name.to_string(), resolved);
    }
}

fn merge_scope_stacks(
    base: &mut [Rc<ScopeBindings>],
    left: &[Rc<ScopeBindings>],
    right: &[Rc<ScopeBindings>],
) {
    let len = base.len().min(left.len()).min(right.len());
    for i in 0..len {
        if Rc::ptr_eq(&left[i], &right[i]) {
            base[i] = Rc::clone(&left[i]);
            continue;
        }
        let mut keys: Vec<String> = base[i]
            .values
            .keys()
            .chain(left[i].values.keys())
            .chain(right[i].values.keys())
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();

        let mut merged_values = HashMap::with_capacity(keys.len());
        for key in keys {
            let merged = match (left[i].values.get(&key), right[i].values.get(&key)) {
                (Some(v1), Some(v2)) if v1 == v2 => v1.clone(),
                _ => StaticValue::Unknown,
            };
            merged_values.insert(key, merged);
        }

        base[i] = Rc::new(ScopeBindings {
            h_shadowed: left[i].h_shadowed || right[i].h_shadowed,
            measure_shadowed: left[i].measure_shadowed || right[i].measure_shadowed,
            terms_shadowed: left[i].terms_shadowed || right[i].terms_shadowed,
            range_shadowed: left[i].range_shadowed || right[i].range_shadowed,
            values: merged_values,
        });
    }
}

fn collect_ident_names(node: &SyntaxNode, out: &mut HashSet<String>) {
    if node.kind() == SyntaxKind::Ident {
        out.insert(node.text().to_string());
    }
    for child in node.children() {
        collect_ident_names(child, out);
    }
}

fn collect_assigned_names(node: &SyntaxNode, out: &mut HashSet<String>) {
    if node.kind() == SyntaxKind::Closure {
        return;
    }
    if node.kind() == SyntaxKind::Binary {
        if let Some(bin) = node.cast::<ast::Binary>() {
            let op = bin.op();
            if matches!(
                op,
                ast::BinOp::Assign
                    | ast::BinOp::AddAssign
                    | ast::BinOp::SubAssign
                    | ast::BinOp::MulAssign
                    | ast::BinOp::DivAssign
            ) {
                if let ast::Expr::Ident(ident) = ungroup_expr(bin.lhs()) {
                    out.insert(ident.get().to_string());
                } else if op == ast::BinOp::Assign {
                    collect_ident_names(bin.lhs().to_untyped(), out);
                }
            }
        }
    }
    for child in node.children() {
        collect_assigned_names(child, out);
    }
}

fn havoc_assigned_vars(node: &SyntaxNode, scopes: &mut [Rc<ScopeBindings>]) {
    let mut assigned = HashSet::new();
    collect_assigned_names(node, &mut assigned);
    for name in assigned {
        if let Some(scope_rc) = scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.values.contains_key(&name))
        {
            Rc::make_mut(scope_rc)
                .values
                .insert(name, StaticValue::Unknown);
        } else if let Some(scope_rc) = scopes.last_mut() {
            Rc::make_mut(scope_rc)
                .values
                .insert(name, StaticValue::Unknown);
        }
    }
}

fn find_linked_child<'a>(node: &LinkedNode<'a>, target: &SyntaxNode) -> Option<LinkedNode<'a>> {
    let target_ptr = target as *const SyntaxNode;
    node.children()
        .find(|child| std::ptr::eq(child.get(), target_ptr))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_bounded_regular_file(path: &Path, byte_budget: &mut usize) -> (Option<String>, bool) {
    use std::io::Read;

    let Ok(metadata) = std::fs::metadata(path) else {
        return (None, false);
    };
    if !metadata.is_file() {
        return (None, false);
    }
    if metadata.len() > MAX_STATIC_IMPORT_BYTES as u64 {
        return (None, false);
    }
    if *byte_budget == 0 || metadata.len() > *byte_budget as u64 {
        return (None, true);
    }
    let per_file_cap = MAX_STATIC_IMPORT_BYTES.min(*byte_budget);
    let Ok(file) = std::fs::File::open(path) else {
        return (None, false);
    };
    let Ok(opened_meta) = file.metadata() else {
        return (None, false);
    };
    if !opened_meta.is_file() {
        return (None, false);
    }
    let mut buf = Vec::new();
    if file
        .take((per_file_cap as u64).saturating_add(1))
        .read_to_end(&mut buf)
        .is_err()
    {
        return (None, false);
    }
    if buf.len() > MAX_STATIC_IMPORT_BYTES {
        return (None, false);
    }
    if buf.len() > *byte_budget {
        return (None, true);
    }
    *byte_budget -= buf.len();
    (String::from_utf8(buf).ok(), false)
}

#[cfg(not(target_arch = "wasm32"))]
fn module_file_exports(
    dirs: ImportBaseDirs<'_>,
    import_path: &str,
    state: &mut WalkState<'_>,
) -> Option<Rc<ModuleExports>> {
    if state.import_depth >= MAX_STATIC_IMPORT_DEPTH {
        state.export_incomplete = true;
        return None;
    }
    let rel_path = resolve_typst_import_rel_path(dirs, import_path)?;
    let canon_root = std::fs::canonicalize(dirs.project_root).ok()?;
    let candidate = canon_root.join(&rel_path);
    let canon_candidate = std::fs::canonicalize(&candidate).ok()?;
    if !canon_candidate.starts_with(&canon_root) {
        return None;
    }
    if state.visited.contains(&rel_path) {
        state.export_incomplete = true;
        return None;
    }
    if let Some(cached) = state.module_cache.get(&rel_path) {
        return cached.as_ref().map(Rc::clone);
    }
    let (maybe_source, budget_exhausted) =
        read_bounded_regular_file(&canon_candidate, state.active_import_byte_budget());
    if budget_exhausted {
        state.export_incomplete = true;
        return None;
    }
    let Some(source) = maybe_source else {
        state.module_cache.insert(rel_path, None);
        return None;
    };

    state.visited.insert(rel_path.clone());
    let (result, subtree_incomplete) = {
        let root = typst_syntax::parse(&source);
        let linked = LinkedNode::new(&root);
        let mut scopes = vec![Rc::new(ScopeBindings::default())];
        let module_rel_dir = rel_path.parent().unwrap_or(Path::new(""));
        let module_dirs = ImportBaseDirs {
            project_root: &canon_root,
            current_rel: module_rel_dir,
        };
        let mut module_state = WalkState {
            offsets: None,
            source_dir: Some(module_dirs),
            visited: state.visited,
            module_cache: state.module_cache,
            import_depth: state.import_depth + 1,
            import_byte_budget: state.import_byte_budget,
            dead_import_byte_budget: state.dead_import_byte_budget,
            export_entry_budget: state.export_entry_budget,
            dead_export_entry_budget: state.dead_export_entry_budget,
            step_budget: MAX_STATIC_LOOP_STEPS,
            dead_step_budget: MAX_STATIC_LOOP_STEPS,
            value_budget: MAX_STATIC_TOTAL_VALUE_BYTES,
            dead_value_budget: MAX_STATIC_TOTAL_VALUE_BYTES,
            in_dead_branch: state.in_dead_branch,
            export_incomplete: false,
        };
        let _ = walk_scopes(&linked, &mut scopes, &mut module_state);
        let incomplete = module_state.export_incomplete;
        let exports = scopes.first().map(|scope| {
            Rc::new(ModuleExports {
                terms: scope.terms_shadowed,
                range: scope.range_shadowed,
                values: scope.values.clone(),
            })
        });
        (exports, incomplete)
    };
    state.visited.remove(&rel_path);
    state.export_incomplete |= subtree_incomplete;
    if !subtree_incomplete {
        if let Some(exports) = result.as_ref() {
            if charge_value_budget(exports.values.len(), state.active_export_entry_budget()) {
                state
                    .module_cache
                    .insert(rel_path, Some(Rc::clone(exports)));
            } else {
                state.export_incomplete = true;
            }
        } else {
            state.module_cache.insert(rel_path, None);
        }
    }
    result
}

fn resolve_module_exports(
    resolved_source: Option<&str>,
    state: &mut WalkState<'_>,
) -> Option<Rc<ModuleExports>> {
    let (dirs, source) = (state.source_dir?, resolved_source?);
    #[cfg(not(target_arch = "wasm32"))]
    {
        module_file_exports(dirs, source, state)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (dirs, source);
        None
    }
}

fn record_import_bindings(
    import: ast::ModuleImport<'_>,
    scopes: &mut [Rc<ScopeBindings>],
    state: &mut WalkState<'_>,
) {
    let resolved_source =
        resolve_import_source_string(import.source(), scopes, state.active_value_budget());
    let exports = match import.imports() {
        Some(ast::Imports::Wildcard) | Some(ast::Imports::Items(_)) => {
            resolve_module_exports(resolved_source.as_deref(), state)
        }
        None => None,
    };
    let is_wildcard = matches!(import.imports(), Some(ast::Imports::Wildcard));
    let copy_exports = is_wildcard
        && exports.as_ref().is_some_and(|exp| {
            charge_value_budget(exp.values.len(), state.active_export_entry_budget())
        });
    if is_wildcard && exports.is_some() && !copy_exports {
        state.export_incomplete = true;
    }

    let mut visible_names = Vec::new();
    if is_wildcard && !copy_exports {
        for s in scopes.iter() {
            visible_names.extend(s.values.keys().cloned());
        }
    }

    let Some(scope_rc) = scopes.last_mut() else {
        return;
    };
    let scope = Rc::make_mut(scope_rc);

    if let Some(alias) = import.new_name() {
        mark_binding(alias.get().as_str(), scope);
    }

    if let Some(imports) = import.imports() {
        match imports {
            ast::Imports::Wildcard => {
                scope.h_shadowed = true;
                scope.measure_shadowed = true;
                if let Some(exports) = exports {
                    if exports.terms {
                        scope.terms_shadowed = true;
                    }
                    if exports.range {
                        scope.range_shadowed = true;
                        scope
                            .values
                            .insert("range".to_string(), StaticValue::Unknown);
                    }
                    if copy_exports {
                        for (name, val) in &exports.values {
                            scope.values.insert(name.clone(), val.clone());
                        }
                    } else {
                        for name in visible_names {
                            scope.values.insert(name, StaticValue::Unknown);
                        }
                        scope.range_shadowed = true;
                    }
                } else {
                    for name in visible_names {
                        scope.values.insert(name, StaticValue::Unknown);
                    }
                    scope.range_shadowed = true;
                    if resolved_source.is_none() {
                        scope.terms_shadowed = true;
                    }
                }
            }
            ast::Imports::Items(items) => {
                for item in items.iter() {
                    let bound_name = item.bound_name().get().as_str();
                    mark_binding(bound_name, scope);
                    let orig_name = item.original_name().get().as_str();
                    if let Some(exports) = exports.as_ref() {
                        if let Some(val) = exports.values.get(orig_name) {
                            scope.values.insert(bound_name.to_string(), val.clone());
                        }
                    }
                }
            }
        }
    } else if import.new_name().is_none() {
        if let Some(name) = resolved_source
            .as_deref()
            .and_then(default_import_binding_name)
        {
            mark_binding(name, scope);
        } else {
            scope.h_shadowed = true;
            scope.measure_shadowed = true;
            scope.terms_shadowed = true;
            scope.range_shadowed = true;
        }
    }
}

fn with_dead_branch(state: &mut WalkState<'_>, f: impl FnOnce(&mut WalkState<'_>)) {
    let was_dead = state.in_dead_branch;
    state.in_dead_branch = true;
    f(state);
    state.in_dead_branch = was_dead;
}

fn walk_dead(node: &LinkedNode<'_>, scopes: &[Rc<ScopeBindings>], state: &mut WalkState<'_>) {
    if state.offsets.is_none() {
        return;
    }
    let mut dead_scopes = scopes.to_vec();
    with_dead_branch(state, |dead_state| {
        let _ = walk_scopes(node, &mut dead_scopes, dead_state);
    });
}

fn walk_dead_scoped(
    node: &LinkedNode<'_>,
    scopes: &[Rc<ScopeBindings>],
    extra_bindings: Option<ast::Pattern<'_>>,
    state: &mut WalkState<'_>,
) {
    if state.offsets.is_none() {
        return;
    }
    let mut dead_scopes = scopes.to_vec();
    dead_scopes.push(Rc::new(ScopeBindings::default()));
    if let (Some(pattern), Some(scope_rc)) = (extra_bindings, dead_scopes.last_mut()) {
        record_pattern_bindings(pattern, Rc::make_mut(scope_rc));
    }
    with_dead_branch(state, |dead_state| {
        let _ = walk_scopes(node, &mut dead_scopes, dead_state);
    });
}

fn walk_dead_slice(
    nodes: &[LinkedNode<'_>],
    scopes: &[Rc<ScopeBindings>],
    extra_bindings: Option<ast::Pattern<'_>>,
    state: &mut WalkState<'_>,
) {
    if state.offsets.is_none() {
        return;
    }
    let mut dead_scopes = scopes.to_vec();
    if extra_bindings.is_some() {
        dead_scopes.push(Rc::new(ScopeBindings::default()));
        if let (Some(pattern), Some(scope_rc)) = (extra_bindings, dead_scopes.last_mut()) {
            record_pattern_bindings(pattern, Rc::make_mut(scope_rc));
        }
    }
    with_dead_branch(state, |dead_state| {
        let _ = walk_slice_with_flow(nodes, &mut dead_scopes, dead_state);
    });
}

fn walk_closure_body<F>(
    node: &LinkedNode<'_>,
    body: &SyntaxNode,
    record_bindings: F,
    scopes: &[Rc<ScopeBindings>],
    state: &mut WalkState<'_>,
) where
    F: FnOnce(&mut ScopeBindings),
{
    let children: Vec<_> = node.children().collect();
    let body_ptr = body as *const SyntaxNode;
    let body_pos = children
        .iter()
        .position(|child| std::ptr::eq(child.get(), body_ptr));
    let (param_nodes, body_nodes) = match body_pos {
        Some(pos) => (&children[..pos], &children[pos..]),
        None => (&[][..], children.as_slice()),
    };
    with_dead_branch(state, |dead_state| {
        if !param_nodes.is_empty() {
            let mut param_scopes = scopes.to_vec();
            for child in param_nodes {
                let _ = walk_scopes(child, &mut param_scopes, dead_state);
            }
        }
        let mut body_scopes = scopes.to_vec();
        body_scopes.push(Rc::new(ScopeBindings::default()));
        if let Some(scope_rc) = body_scopes.last_mut() {
            record_bindings(Rc::make_mut(scope_rc));
        }
        let _ = walk_slice_with_flow(body_nodes, &mut body_scopes, dead_state);
    });
}

fn walk_slice_with_flow(
    nodes: &[LinkedNode<'_>],
    scopes: &mut Vec<Rc<ScopeBindings>>,
    state: &mut WalkState<'_>,
) -> ControlFlow {
    let mut flow = ControlFlow::Normal;
    for child in nodes {
        if flow == ControlFlow::Normal {
            flow = walk_scopes(child, scopes, state);
        } else if state.offsets.is_some() {
            walk_dead(child, scopes, state);
        } else {
            break;
        }
    }
    flow
}

fn walk_children_with_flow(
    node: &LinkedNode<'_>,
    scopes: &mut Vec<Rc<ScopeBindings>>,
    state: &mut WalkState<'_>,
) -> ControlFlow {
    let mut flow = ControlFlow::Normal;
    for child in node.children() {
        if flow == ControlFlow::Normal {
            flow = walk_scopes(&child, scopes, state);
        } else if state.offsets.is_some() {
            walk_dead(&child, scopes, state);
        } else {
            break;
        }
    }
    flow
}

fn walk_scopes(
    node: &LinkedNode<'_>,
    scopes: &mut Vec<Rc<ScopeBindings>>,
    state: &mut WalkState<'_>,
) -> ControlFlow {
    match node.kind() {
        SyntaxKind::LoopBreak => ControlFlow::Break,
        SyntaxKind::LoopContinue => ControlFlow::Continue,
        SyntaxKind::Contextual => {
            if let Some(offsets) = state.offsets.as_deref_mut() {
                if is_llap_shadowed(scopes) && llap_body(node.get()).is_some() {
                    if let Some(kw) = node
                        .children()
                        .find(|child| child.kind() == SyntaxKind::Context)
                    {
                        offsets.push(kw.range().end);
                    }
                }
                walk_children_with_flow(node, scopes, state)
            } else {
                ControlFlow::Normal
            }
        }
        SyntaxKind::LetBinding => {
            if let Some(binding) = node.get().cast::<ast::LetBinding>() {
                match binding.kind() {
                    ast::LetBindingKind::Closure(ident) => {
                        let closure_val = binding.init().map(|init| {
                            eval_static_value(init, scopes, state.active_value_budget())
                        });
                        if let Some(scope_rc) = scopes.last_mut() {
                            mark_binding(ident.get().as_str(), Rc::make_mut(scope_rc));
                        }
                        let flow = walk_children_with_flow(node, scopes, state);
                        if flow == ControlFlow::Normal {
                            if let (Some(scope_rc), Some(val)) = (scopes.last_mut(), closure_val) {
                                Rc::make_mut(scope_rc)
                                    .values
                                    .insert(ident.get().to_string(), val);
                            }
                        }
                        flow
                    }
                    ast::LetBindingKind::Normal(pattern) => {
                        let flow = walk_children_with_flow(node, scopes, state);
                        if flow == ControlFlow::Normal {
                            record_let_normal_binding(
                                pattern,
                                binding.init(),
                                scopes,
                                state.active_value_budget(),
                            );
                        }
                        flow
                    }
                }
            } else {
                ControlFlow::Normal
            }
        }
        SyntaxKind::Binary => {
            if let Some(bin) = node.get().cast::<ast::Binary>() {
                let op = bin.op();
                if matches!(op, ast::BinOp::And | ast::BinOp::Or) {
                    let lhs_flow =
                        if let Some(lhs_node) = find_linked_child(node, bin.lhs().to_untyped()) {
                            walk_scopes(&lhs_node, scopes, state)
                        } else {
                            ControlFlow::Normal
                        };
                    if lhs_flow != ControlFlow::Normal {
                        if let Some(rhs_node) = find_linked_child(node, bin.rhs().to_untyped()) {
                            walk_dead(&rhs_node, scopes, state);
                        }
                        return lhs_flow;
                    }
                    let lhs_val = eval_static_bool(bin.lhs(), scopes, state.active_value_budget());
                    let short_circuits = (op == ast::BinOp::And && lhs_val == Some(false))
                        || (op == ast::BinOp::Or && lhs_val == Some(true));
                    let must_eval_rhs = (op == ast::BinOp::And && lhs_val == Some(true))
                        || (op == ast::BinOp::Or && lhs_val == Some(false));
                    if let Some(rhs_node) = find_linked_child(node, bin.rhs().to_untyped()) {
                        if short_circuits {
                            walk_dead(&rhs_node, scopes, state);
                            ControlFlow::Normal
                        } else if must_eval_rhs {
                            walk_scopes(&rhs_node, scopes, state)
                        } else {
                            let pre_scopes = scopes.clone();
                            let mut rhs_scopes = pre_scopes.clone();
                            let _ = walk_scopes(&rhs_node, &mut rhs_scopes, state);
                            merge_scope_stacks(scopes, &pre_scopes, &rhs_scopes);
                            ControlFlow::Normal
                        }
                    } else {
                        ControlFlow::Normal
                    }
                } else {
                    let flow = walk_children_with_flow(node, scopes, state);
                    if flow == ControlFlow::Normal {
                        record_assign_binding(bin, scopes, state.active_value_budget());
                    }
                    flow
                }
            } else {
                walk_children_with_flow(node, scopes, state)
            }
        }
        SyntaxKind::Conditional => {
            if let Some(cond) = node.get().cast::<ast::Conditional>() {
                let cond_flow = if let Some(cond_node) =
                    find_linked_child(node, cond.condition().to_untyped())
                {
                    walk_scopes(&cond_node, scopes, state)
                } else {
                    ControlFlow::Normal
                };
                if cond_flow != ControlFlow::Normal {
                    if let Some(if_node) = find_linked_child(node, cond.if_body().to_untyped()) {
                        walk_dead(&if_node, scopes, state);
                    }
                    if let Some(else_body) = cond.else_body() {
                        if let Some(else_node) = find_linked_child(node, else_body.to_untyped()) {
                            walk_dead(&else_node, scopes, state);
                        }
                    }
                    return cond_flow;
                }
                match eval_static_bool(cond.condition(), scopes, state.active_value_budget()) {
                    Some(true) => {
                        let pre_scopes = scopes.clone();
                        let flow = if let Some(if_node) =
                            find_linked_child(node, cond.if_body().to_untyped())
                        {
                            walk_scopes(&if_node, scopes, state)
                        } else {
                            ControlFlow::Normal
                        };
                        if let Some(else_body) = cond.else_body() {
                            if let Some(else_node) = find_linked_child(node, else_body.to_untyped())
                            {
                                walk_dead(&else_node, &pre_scopes, state);
                            }
                        }
                        flow
                    }
                    Some(false) => {
                        if let Some(if_node) = find_linked_child(node, cond.if_body().to_untyped())
                        {
                            walk_dead(&if_node, scopes, state);
                        }
                        if let Some(else_body) = cond.else_body() {
                            if let Some(else_node) = find_linked_child(node, else_body.to_untyped())
                            {
                                walk_scopes(&else_node, scopes, state)
                            } else {
                                ControlFlow::Normal
                            }
                        } else {
                            ControlFlow::Normal
                        }
                    }
                    None => {
                        let mut then_scopes = scopes.clone();
                        let then_flow = if let Some(if_node) =
                            find_linked_child(node, cond.if_body().to_untyped())
                        {
                            walk_scopes(&if_node, &mut then_scopes, state)
                        } else {
                            ControlFlow::Normal
                        };
                        let mut else_scopes = scopes.clone();
                        let else_flow = if let Some(else_body) = cond.else_body() {
                            if let Some(else_node) = find_linked_child(node, else_body.to_untyped())
                            {
                                walk_scopes(&else_node, &mut else_scopes, state)
                            } else {
                                ControlFlow::Normal
                            }
                        } else {
                            ControlFlow::Normal
                        };
                        merge_scope_stacks(scopes, &then_scopes, &else_scopes);
                        if then_flow == else_flow {
                            then_flow
                        } else {
                            ControlFlow::Normal
                        }
                    }
                }
            } else {
                ControlFlow::Normal
            }
        }
        SyntaxKind::Closure => {
            if state.offsets.is_some() {
                if let Some(closure) = node.get().cast::<ast::Closure>() {
                    walk_closure_body(
                        node,
                        closure.body().to_untyped(),
                        |scope| record_params_bindings(closure.params(), scope),
                        scopes,
                        state,
                    );
                }
            }
            ControlFlow::Normal
        }
        SyntaxKind::WhileLoop => {
            if let Some(while_loop) = node.get().cast::<ast::WhileLoop>() {
                let cond_flow = if let Some(cond_node) =
                    find_linked_child(node, while_loop.condition().to_untyped())
                {
                    walk_scopes(&cond_node, scopes, state)
                } else {
                    ControlFlow::Normal
                };
                if cond_flow != ControlFlow::Normal {
                    if let Some(body_node) = find_linked_child(node, while_loop.body().to_untyped())
                    {
                        walk_dead_scoped(&body_node, scopes, None, state);
                    }
                    return cond_flow;
                }
                let mut cond_val =
                    eval_static_bool(while_loop.condition(), scopes, state.active_value_budget());
                match cond_val {
                    Some(false) => {
                        if let Some(body_node) =
                            find_linked_child(node, while_loop.body().to_untyped())
                        {
                            walk_dead_scoped(&body_node, scopes, None, state);
                        }
                    }
                    Some(true) => {
                        let mut terminated = false;
                        let mut ran_any_iter = false;
                        for _ in 0..MAX_STATIC_LOOP_ITERS {
                            if !state.consume_loop_step() {
                                break;
                            }
                            ran_any_iter = true;
                            scopes.push(Rc::new(ScopeBindings::default()));
                            let body_flow = if let Some(body_node) =
                                find_linked_child(node, while_loop.body().to_untyped())
                            {
                                walk_scopes(&body_node, scopes, state)
                            } else {
                                ControlFlow::Normal
                            };
                            scopes.pop();
                            if body_flow == ControlFlow::Break {
                                terminated = true;
                                break;
                            }
                            let next_cond_flow = if let Some(cond_node) =
                                find_linked_child(node, while_loop.condition().to_untyped())
                            {
                                walk_scopes(&cond_node, scopes, state)
                            } else {
                                ControlFlow::Normal
                            };
                            if next_cond_flow != ControlFlow::Normal {
                                terminated = true;
                                break;
                            }
                            cond_val = eval_static_bool(
                                while_loop.condition(),
                                scopes,
                                state.active_value_budget(),
                            );
                            if cond_val == Some(false) {
                                terminated = true;
                                break;
                            }
                            if cond_val != Some(true) {
                                break;
                            }
                        }
                        if !terminated {
                            havoc_assigned_vars(while_loop.to_untyped(), scopes);
                            if !ran_any_iter {
                                if let Some(body_node) =
                                    find_linked_child(node, while_loop.body().to_untyped())
                                {
                                    walk_dead_scoped(&body_node, scopes, None, state);
                                }
                            }
                        }
                    }
                    None => {
                        havoc_assigned_vars(while_loop.to_untyped(), scopes);
                        if let Some(body_node) =
                            find_linked_child(node, while_loop.body().to_untyped())
                        {
                            walk_dead_scoped(&body_node, scopes, None, state);
                        }
                    }
                }
            }
            ControlFlow::Normal
        }
        SyntaxKind::ForLoop => {
            if let Some(for_loop) = node.get().cast::<ast::ForLoop>() {
                let children: Vec<_> = node.children().collect();
                let body_ptr = for_loop.body().to_untyped() as *const SyntaxNode;
                let body_pos = children
                    .iter()
                    .position(|child| std::ptr::eq(child.get(), body_ptr));
                if let Some(pos) = body_pos {
                    let iter_flow = walk_slice_with_flow(&children[..pos], scopes, state);
                    if iter_flow != ControlFlow::Normal {
                        walk_dead_slice(&children[pos..], scopes, None, state);
                        return iter_flow;
                    }
                    match static_for_iterable_values(
                        for_loop.iterable(),
                        scopes,
                        state.active_value_budget(),
                    ) {
                        Some(items) if items.is_empty() => {
                            walk_dead_slice(
                                &children[pos..],
                                scopes,
                                Some(for_loop.pattern()),
                                state,
                            );
                        }
                        Some(items) if state.remaining_loop_steps() >= items.len() => {
                            for item in items {
                                if !state.consume_loop_step() {
                                    havoc_assigned_vars(for_loop.body().to_untyped(), scopes);
                                    break;
                                }
                                scopes.push(Rc::new(ScopeBindings::default()));
                                if let Some(scope_rc) = scopes.last_mut() {
                                    let scope = Rc::make_mut(scope_rc);
                                    record_pattern_bindings(for_loop.pattern(), scope);
                                    if let Some(name) = simple_pattern_ident(for_loop.pattern()) {
                                        scope.values.insert(name.to_string(), item);
                                    }
                                }
                                let body_flow =
                                    walk_slice_with_flow(&children[pos..], scopes, state);
                                scopes.pop();
                                if body_flow == ControlFlow::Break {
                                    break;
                                }
                            }
                        }
                        _ => {
                            havoc_assigned_vars(for_loop.body().to_untyped(), scopes);
                            walk_dead_slice(
                                &children[pos..],
                                scopes,
                                Some(for_loop.pattern()),
                                state,
                            );
                        }
                    }
                    ControlFlow::Normal
                } else {
                    walk_children_with_flow(node, scopes, state)
                }
            } else {
                ControlFlow::Normal
            }
        }
        SyntaxKind::ModuleImport => {
            if let Some(import) = node.get().cast::<ast::ModuleImport>() {
                let flow = walk_children_with_flow(node, scopes, state);
                if flow == ControlFlow::Normal {
                    record_import_bindings(import, scopes, state);
                }
                flow
            } else {
                ControlFlow::Normal
            }
        }
        SyntaxKind::CodeBlock | SyntaxKind::ContentBlock => {
            scopes.push(Rc::new(ScopeBindings::default()));
            let flow = walk_children_with_flow(node, scopes, state);
            scopes.pop();
            flow
        }
        SyntaxKind::FuncCall => {
            if let Some(offsets) = state.offsets.as_deref_mut() {
                if is_terms_shadowed(scopes) {
                    if let Some(offset) = shadowed_terms_item_marker_offset(node) {
                        offsets.push(offset);
                    }
                }
            }
            walk_children_with_flow(node, scopes, state)
        }
        _ => walk_children_with_flow(node, scopes, state),
    }
}

fn shadowed_terms_item_marker_offset(node: &LinkedNode<'_>) -> Option<usize> {
    let call = node.get().cast::<ast::FuncCall>()?;
    let ast::Expr::FieldAccess(access) = call.callee() else {
        return None;
    };
    let ast::Expr::Ident(target) = access.target() else {
        return None;
    };
    if target.get().as_str() != "terms" || access.field().get().as_str() != "item" {
        return None;
    }
    let access_linked = node
        .children()
        .find(|child| std::ptr::eq(child.get(), access.to_untyped()))?;
    let target_linked = access_linked
        .children()
        .find(|child| std::ptr::eq(child.get(), target.to_untyped()))?;
    Some(target_linked.range().end)
}
