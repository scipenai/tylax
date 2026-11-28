use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use typst_syntax::ast::{self, AstNode};
use typst_syntax::{LinkedNode, SyntaxKind, SyntaxNode};
use unicode_segmentation::UnicodeSegmentation;

use super::engine::{default_import_binding_name, VirtualFileSystem};
use super::math_ir::{llap_body, ungroup_expr};

const MAX_STATIC_LOOP_ITERS: usize = 64;
const MAX_STATIC_LOOP_STEPS: usize = 512;
const MAX_STATIC_CALL_DEPTH: usize = 4;
const MAX_STATIC_VALUE_BYTES: usize = 64 * 1024;
const MAX_STATIC_TOTAL_VALUE_BYTES: usize = 256 * 1024;
const MAX_STATIC_IMPORT_DEPTH: usize = 8;
const MAX_STATIC_IMPORT_BYTES: usize = 1024 * 1024;
const MAX_STATIC_TOTAL_IMPORT_BYTES: usize = 4 * 1024 * 1024;
const MAX_STATIC_TOTAL_EXPORT_ENTRIES: usize = 8 * 1024;
const MAX_IMPORTED_SETS_PER_SCOPE: usize = 64;

thread_local! {
    static ACTIVE_SHADOW_MARKERS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct ShadowSessionGuard;

impl Drop for ShadowSessionGuard {
    fn drop(&mut self) {
        ACTIVE_SHADOW_MARKERS.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub(crate) fn with_shadow_session<R>(marker: &str, f: impl FnOnce() -> R) -> R {
    ACTIVE_SHADOW_MARKERS.with(|stack| stack.borrow_mut().push(marker.to_string()));
    let _guard = ShadowSessionGuard;
    f()
}

pub(crate) fn is_shadowed_node(node: &SyntaxNode) -> bool {
    ACTIVE_SHADOW_MARKERS.with(|stack| {
        let stack = stack.borrow();
        let Some(marker) = stack.last().map(String::as_str) else {
            return false;
        };
        node.children()
            .any(|child| child.kind() == SyntaxKind::BlockComment && child.text() == marker)
    })
}

pub(crate) fn strip_active_shadow_marker<'a>(text: &'a str) -> Cow<'a, str> {
    ACTIVE_SHADOW_MARKERS.with(|stack| {
        let stack = stack.borrow();
        match stack.last().map(String::as_str) {
            Some(marker) if text.contains(marker) => Cow::Owned(text.replace(marker, "")),
            _ => Cow::Borrowed(text),
        }
    })
}

pub(crate) fn clean_context_source(node: &SyntaxNode) -> String {
    let raw = format!("#{}", SyntaxNode::clone(node).into_text());
    strip_active_shadow_marker(&raw).into_owned()
}

pub(crate) fn strip_shadow_marker(output: &mut String, marker: &str) {
    if output.contains(marker) {
        *output = output.replace(marker, "");
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShadowPolicy {
    /// Require the built-in to remain provably unshadowed; any unresolved wildcard marks it shadowed.
    StrictConfirm,
    /// Preserve built-in lowering unless shadowed explicitly or by an unknown dynamic import source.
    ConservativeDynamic,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum WildcardImportState {
    #[default]
    None,
    UnloadedModule,
    UnknownSource,
}

struct ShadowRule {
    syntax_kind: SyntaxKind,
    builtins: &'static [&'static str],
    policy: ShadowPolicy,
    marker_offset: fn(&LinkedNode<'_>) -> Option<usize>,
}

const SHADOW_RULES: &[ShadowRule] = &[
    ShadowRule {
        syntax_kind: SyntaxKind::Contextual,
        builtins: &["h", "measure"],
        policy: ShadowPolicy::StrictConfirm,
        marker_offset: shadowed_contextual_llap_offset,
    },
    ShadowRule {
        syntax_kind: SyntaxKind::FuncCall,
        builtins: &["terms"],
        policy: ShadowPolicy::ConservativeDynamic,
        marker_offset: shadowed_terms_item_marker_offset,
    },
];

const STATIC_EVAL_BUILTINS: &[&str] = &["range"];

pub(crate) fn collect_shadowed_builtins(mut is_bound: impl FnMut(&str) -> bool) -> HashSet<String> {
    let mut out = HashSet::new();
    for rule in SHADOW_RULES {
        for &builtin in rule.builtins {
            if is_bound(builtin) {
                out.insert(builtin.to_string());
            }
        }
    }
    for &builtin in STATIC_EVAL_BUILTINS {
        if is_bound(builtin) {
            out.insert(builtin.to_string());
        }
    }
    out
}

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

#[derive(Clone)]
struct StaticModule(Rc<ModuleExports>);

impl PartialEq for StaticModule {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for StaticModule {}

#[derive(Clone, PartialEq, Eq)]
enum StaticValue {
    Str(Rc<str>),
    Bool(bool),
    Int(i64),
    Array(StaticArray),
    Closure(Rc<StaticClosure>),
    Module(StaticModule),
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
            Self::Bool(_) | Self::Int(_) | Self::Closure(_) | Self::Module(_) | Self::Unknown => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlFlow {
    Normal,
    Continue,
    Break,
}

#[derive(Clone, Default)]
struct ScopeBindings {
    bound_names: HashSet<String>,
    imported_sets: Vec<Rc<HashSet<String>>>,
    wildcard_state: WildcardImportState,
    values: HashMap<String, StaticValue>,
}

impl ScopeBindings {
    fn binds_name(&self, name: &str) -> bool {
        self.bound_names.contains(name) || self.imported_sets.iter().any(|set| set.contains(name))
    }

    fn shadows(&self, name: &str, policy: ShadowPolicy) -> bool {
        if self.binds_name(name) {
            return true;
        }
        match policy {
            ShadowPolicy::StrictConfirm => self.wildcard_state != WildcardImportState::None,
            ShadowPolicy::ConservativeDynamic => {
                self.wildcard_state == WildcardImportState::UnknownSource
            }
        }
    }

    fn add_imported_set(&mut self, set: &Rc<HashSet<String>>) {
        if set.is_empty()
            || self
                .imported_sets
                .iter()
                .any(|existing| Rc::ptr_eq(existing, set))
        {
            return;
        }
        if self.imported_sets.len() < MAX_IMPORTED_SETS_PER_SCOPE {
            self.imported_sets.push(Rc::clone(set));
        } else {
            self.wildcard_state = WildcardImportState::UnknownSource;
        }
    }

    fn to_module_exports(&self) -> ModuleExports {
        let mut exported_sets = self.imported_sets.clone();
        let mut wildcard_state = self.wildcard_state;
        if !self.bound_names.is_empty() {
            if exported_sets.len() < MAX_IMPORTED_SETS_PER_SCOPE {
                exported_sets.push(Rc::new(self.bound_names.clone()));
            } else {
                let mut merged = self.bound_names.clone();
                let mut flattened = true;
                for existing in &exported_sets {
                    if merged.len().saturating_add(existing.len()) > MAX_STATIC_TOTAL_EXPORT_ENTRIES
                    {
                        flattened = false;
                        break;
                    }
                    merged.extend(existing.iter().cloned());
                }
                if flattened {
                    exported_sets = vec![Rc::new(merged)];
                } else {
                    wildcard_state = WildcardImportState::UnknownSource;
                }
            }
        }
        ModuleExports {
            exported_sets,
            wildcard_state,
            values: self.values.clone(),
        }
    }
}

#[derive(Clone, Default)]
struct ModuleExports {
    exported_sets: Vec<Rc<HashSet<String>>>,
    wildcard_state: WildcardImportState,
    values: HashMap<String, StaticValue>,
}

impl ModuleExports {
    fn exports_name(&self, name: &str) -> bool {
        self.exported_sets.iter().any(|set| set.contains(name))
    }
}

#[derive(Clone, Copy)]
struct BudgetPair<T> {
    live: T,
    dead: T,
}

impl<T> BudgetPair<T> {
    fn new(live: T, dead: T) -> Self {
        Self { live, dead }
    }

    fn get(&self, in_dead_branch: bool) -> &T {
        if in_dead_branch {
            &self.dead
        } else {
            &self.live
        }
    }

    fn get_mut(&mut self, in_dead_branch: bool) -> &mut T {
        if in_dead_branch {
            &mut self.dead
        } else {
            &mut self.live
        }
    }
}

#[derive(Clone, Copy)]
struct LocalBudgets {
    loop_steps: usize,
    value_bytes: usize,
}

impl LocalBudgets {
    fn new() -> Self {
        Self {
            loop_steps: MAX_STATIC_LOOP_STEPS,
            value_bytes: MAX_STATIC_TOTAL_VALUE_BYTES,
        }
    }
}

struct SharedImportBudgets {
    import_bytes: BudgetPair<usize>,
    export_entries: BudgetPair<usize>,
}

impl SharedImportBudgets {
    fn new() -> Self {
        Self {
            import_bytes: BudgetPair::new(
                MAX_STATIC_TOTAL_IMPORT_BYTES,
                MAX_STATIC_TOTAL_IMPORT_BYTES,
            ),
            export_entries: BudgetPair::new(
                MAX_STATIC_TOTAL_EXPORT_ENTRIES,
                MAX_STATIC_TOTAL_EXPORT_ENTRIES,
            ),
        }
    }
}

struct WalkState<'a> {
    offsets: Option<&'a mut Vec<usize>>,
    vfs: &'a dyn VirtualFileSystem,
    current_file: Option<&'a str>,
    visited: &'a mut HashSet<String>,
    module_cache: &'a mut HashMap<String, Option<Rc<ModuleExports>>>,
    shared_budgets: &'a mut SharedImportBudgets,
    local_budgets: BudgetPair<LocalBudgets>,
    import_depth: usize,
    in_dead_branch: bool,
    export_incomplete: bool,
}

impl WalkState<'_> {
    fn active_value_budget(&mut self) -> &mut usize {
        &mut self.local_budgets.get_mut(self.in_dead_branch).value_bytes
    }

    fn remaining_loop_steps(&self) -> usize {
        self.local_budgets.get(self.in_dead_branch).loop_steps
    }

    fn consume_loop_step(&mut self) -> bool {
        let budget = &mut self.local_budgets.get_mut(self.in_dead_branch).loop_steps;
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        true
    }
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

pub(crate) fn annotate_shadowed_builtins(
    input: &str,
    math_only: bool,
    marker: &str,
    vfs: &dyn VirtualFileSystem,
    current_file: Option<&str>,
) -> String {
    let root = if math_only {
        typst_syntax::parse_math(input)
    } else {
        typst_syntax::parse(input)
    };
    let linked = LinkedNode::new(&root);
    annotate_shadowed_builtins_in_linked(&linked, input, &HashSet::new(), marker, vfs, current_file)
        .unwrap_or_else(|| input.to_string())
}

pub(crate) fn annotate_shadowed_builtins_in_node(
    node: &SyntaxNode,
    initial_shadowed: &HashSet<String>,
    marker: &str,
    vfs: &dyn VirtualFileSystem,
    current_file: Option<&str>,
) -> Option<String> {
    let source = SyntaxNode::clone(node).into_text().to_string();
    let linked = LinkedNode::new(node);
    annotate_shadowed_builtins_in_linked(
        &linked,
        &source,
        initial_shadowed,
        marker,
        vfs,
        current_file,
    )
    .map(|annotated| format!("#{annotated}"))
}

fn annotate_shadowed_builtins_in_linked(
    root: &LinkedNode<'_>,
    source: &str,
    initial_shadowed: &HashSet<String>,
    marker: &str,
    vfs: &dyn VirtualFileSystem,
    current_file: Option<&str>,
) -> Option<String> {
    let mut scopes = vec![Rc::new(ScopeBindings {
        bound_names: initial_shadowed.clone(),
        ..Default::default()
    })];
    let mut offsets = Vec::new();
    let mut visited = HashSet::new();
    if let Some(file) = current_file {
        visited.insert(file.to_string());
    }
    let mut module_cache = HashMap::new();
    let mut shared_budgets = SharedImportBudgets::new();
    let mut state = WalkState {
        offsets: Some(&mut offsets),
        vfs,
        current_file,
        visited: &mut visited,
        module_cache: &mut module_cache,
        shared_budgets: &mut shared_budgets,
        local_budgets: BudgetPair::new(LocalBudgets::new(), LocalBudgets::new()),
        import_depth: 0,
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

fn is_builtin_shadowed(scopes: &[Rc<ScopeBindings>], name: &str, policy: ShadowPolicy) -> bool {
    scopes.iter().any(|scope| scope.shadows(name, policy))
}

fn apply_shadow_rules(
    node: &LinkedNode<'_>,
    scopes: &[Rc<ScopeBindings>],
    offsets: &mut Vec<usize>,
) {
    for rule in SHADOW_RULES {
        if rule.syntax_kind == node.kind()
            && rule
                .builtins
                .iter()
                .any(|builtin| is_builtin_shadowed(scopes, builtin, rule.policy))
        {
            if let Some(offset) = (rule.marker_offset)(node) {
                offsets.push(offset);
            }
        }
    }
}

fn mark_binding(name: &str, scope: &mut ScopeBindings) {
    scope.bound_names.insert(name.to_string());
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
        wildcard_state: scopes
            .iter()
            .map(|scope| scope.wildcard_state)
            .max()
            .unwrap_or_default(),
        values: HashMap::with_capacity(referenced.len()),
        ..Default::default()
    };

    for name in referenced {
        if scopes.iter().any(|scope| scope.binds_name(&name)) {
            captured.bound_names.insert(name.clone());
        }
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
            ) && !is_builtin_shadowed(scopes, "range", ShadowPolicy::StrictConfirm)
            {
                eval_builtin_range_call(call, scopes, depth, budget)
            } else {
                StaticValue::Unknown
            }
        }
        ast::Expr::FieldAccess(access) => {
            match eval_static_value_with_depth(access.target(), scopes, depth + 1, budget) {
                StaticValue::Module(m) => {
                    m.0.values
                        .get(access.field().get().as_str())
                        .cloned()
                        .unwrap_or(StaticValue::Unknown)
                }
                _ => StaticValue::Unknown,
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

        let mut merged_bound_names = left[i].bound_names.clone();
        merged_bound_names.extend(right[i].bound_names.iter().cloned());

        let mut merged_scope = ScopeBindings {
            bound_names: merged_bound_names,
            imported_sets: left[i].imported_sets.clone(),
            wildcard_state: left[i].wildcard_state.max(right[i].wildcard_state),
            values: merged_values,
        };
        for set in &right[i].imported_sets {
            merged_scope.add_imported_set(set);
        }

        base[i] = Rc::new(merged_scope);
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

fn read_vfs_bounded(
    vfs: &dyn VirtualFileSystem,
    path: &str,
    byte_budget: &mut usize,
) -> (Option<String>, bool) {
    if *byte_budget == 0 {
        return (None, true);
    }
    let Ok(source) = vfs.read_text(path) else {
        return (None, false);
    };
    if source.len() > MAX_STATIC_IMPORT_BYTES {
        return (None, false);
    }
    if source.len() > *byte_budget {
        return (None, true);
    }
    *byte_budget -= source.len();
    (Some(source), false)
}

fn resolve_module_exports(
    resolved_source: Option<&str>,
    state: &mut WalkState<'_>,
) -> Option<Rc<ModuleExports>> {
    let source = resolved_source?;
    if state.import_depth >= MAX_STATIC_IMPORT_DEPTH {
        state.export_incomplete = true;
        return None;
    }
    let base = state.current_file.unwrap_or("");
    let resolved_path = state.vfs.resolve(base, source).ok()?;
    if state.visited.contains(&resolved_path) {
        state.export_incomplete = true;
        return None;
    }
    if let Some(cached) = state.module_cache.get(&resolved_path) {
        return cached.as_ref().map(Rc::clone);
    }
    let (maybe_source, budget_exhausted) = read_vfs_bounded(
        state.vfs,
        &resolved_path,
        state
            .shared_budgets
            .import_bytes
            .get_mut(state.in_dead_branch),
    );
    if budget_exhausted {
        state.export_incomplete = true;
        return None;
    }
    let Some(module_source) = maybe_source else {
        state.module_cache.insert(resolved_path, None);
        return None;
    };

    state.visited.insert(resolved_path.clone());
    let (result, subtree_incomplete) = {
        let root = typst_syntax::parse(&module_source);
        let linked = LinkedNode::new(&root);
        let mut scopes = vec![Rc::new(ScopeBindings::default())];
        let mut module_state = WalkState {
            offsets: None,
            vfs: state.vfs,
            current_file: Some(&resolved_path),
            visited: state.visited,
            module_cache: state.module_cache,
            shared_budgets: state.shared_budgets,
            local_budgets: BudgetPair::new(LocalBudgets::new(), LocalBudgets::new()),
            import_depth: state.import_depth + 1,
            in_dead_branch: state.in_dead_branch,
            export_incomplete: false,
        };
        let _ = walk_scopes(&linked, &mut scopes, &mut module_state);
        let incomplete = module_state.export_incomplete;
        let exports = scopes
            .first()
            .map(|scope| Rc::new(scope.to_module_exports()));
        (exports, incomplete)
    };
    state.visited.remove(&resolved_path);
    state.export_incomplete |= subtree_incomplete;
    if !subtree_incomplete {
        if let Some(exports) = result.as_ref() {
            if charge_value_budget(
                exports.values.len(),
                state
                    .shared_budgets
                    .export_entries
                    .get_mut(state.in_dead_branch),
            ) {
                state
                    .module_cache
                    .insert(resolved_path, Some(Rc::clone(exports)));
            } else {
                state.export_incomplete = true;
            }
        } else {
            state.module_cache.insert(resolved_path, None);
        }
    }
    result
}

fn record_import_bindings(
    import: ast::ModuleImport<'_>,
    scopes: &mut [Rc<ScopeBindings>],
    state: &mut WalkState<'_>,
) {
    let mut source_was_static_module = false;
    let (resolved_source, exports) =
        match eval_static_value(import.source(), scopes, state.active_value_budget()) {
            StaticValue::Module(m) => {
                source_was_static_module = true;
                (None, Some(m.0))
            }
            StaticValue::Str(s) => {
                let s = s.to_string();
                let exp = resolve_module_exports(Some(&s), state);
                (Some(s), exp)
            }
            _ => (None, None),
        };
    let is_wildcard = matches!(import.imports(), Some(ast::Imports::Wildcard));
    let copy_exports = is_wildcard
        && exports.as_ref().is_some_and(|exp| {
            charge_value_budget(
                exp.values.len(),
                state
                    .shared_budgets
                    .export_entries
                    .get_mut(state.in_dead_branch),
            )
        });
    if is_wildcard && exports.is_some() && !copy_exports {
        state.export_incomplete = true;
    }

    let need_visible_names = is_wildcard
        && (!copy_exports
            || exports
                .as_ref()
                .is_none_or(|exp| exp.wildcard_state != WildcardImportState::None));
    let mut visible_names = Vec::new();
    if need_visible_names {
        for s in scopes.iter() {
            visible_names.extend(s.values.keys().cloned());
        }
    }

    let fallback_name = resolved_source
        .as_deref()
        .and_then(default_import_binding_name)
        .map(str::to_string)
        .or_else(|| {
            if source_was_static_module {
                if let ast::Expr::Ident(ident) = ungroup_expr(import.source()) {
                    return Some(ident.get().to_string());
                }
            }
            None
        });

    let Some(scope_rc) = scopes.last_mut() else {
        return;
    };
    let scope = Rc::make_mut(scope_rc);

    if let Some(alias) = import.new_name() {
        let alias_name = alias.get().as_str();
        mark_binding(alias_name, scope);
        let module_val = exports.as_ref().map_or(StaticValue::Unknown, |exp| {
            StaticValue::Module(StaticModule(Rc::clone(exp)))
        });
        scope.values.insert(alias_name.to_string(), module_val);
    }

    if let Some(imports) = import.imports() {
        match imports {
            ast::Imports::Wildcard => {
                if let Some(exports) = exports {
                    for set in &exports.exported_sets {
                        scope.add_imported_set(set);
                    }
                    scope.wildcard_state = scope.wildcard_state.max(exports.wildcard_state);
                    if copy_exports {
                        for name in visible_names {
                            if (exports.exports_name(&name)
                                || exports.wildcard_state != WildcardImportState::None)
                                && !exports.values.contains_key(&name)
                            {
                                scope.values.insert(name, StaticValue::Unknown);
                            }
                        }
                        for (name, val) in &exports.values {
                            scope.values.insert(name.clone(), val.clone());
                        }
                    } else {
                        for name in visible_names {
                            scope.values.insert(name, StaticValue::Unknown);
                        }
                        scope.wildcard_state = scope
                            .wildcard_state
                            .max(WildcardImportState::UnloadedModule);
                    }
                } else {
                    for name in visible_names {
                        scope.values.insert(name, StaticValue::Unknown);
                    }
                    let fallback_state = if resolved_source.is_some() || source_was_static_module {
                        WildcardImportState::UnloadedModule
                    } else {
                        WildcardImportState::UnknownSource
                    };
                    scope.wildcard_state = scope.wildcard_state.max(fallback_state);
                }
            }
            ast::Imports::Items(items) => {
                for item in items.iter() {
                    let bound_name = item.bound_name().get().as_str();
                    mark_binding(bound_name, scope);
                    let segments: Vec<&str> =
                        item.path().iter().map(|id| id.get().as_str()).collect();
                    let resolved_val = match (exports.as_ref(), segments.split_first()) {
                        (Some(root_exp), Some((&first, rest))) => {
                            let mut current = root_exp.values.get(first).cloned();
                            for &seg in rest {
                                current = match current {
                                    Some(StaticValue::Module(sub)) => {
                                        sub.0.values.get(seg).cloned()
                                    }
                                    _ => None,
                                };
                            }
                            current.unwrap_or(StaticValue::Unknown)
                        }
                        _ => StaticValue::Unknown,
                    };
                    scope.values.insert(bound_name.to_string(), resolved_val);
                }
            }
        }
    } else if import.new_name().is_none() {
        if let Some(name) = fallback_name {
            mark_binding(&name, scope);
            let module_val = exports.as_ref().map_or(StaticValue::Unknown, |exp| {
                StaticValue::Module(StaticModule(Rc::clone(exp)))
            });
            scope.values.insert(name, module_val);
        } else {
            scope.wildcard_state = WildcardImportState::UnknownSource;
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
                apply_shadow_rules(node, scopes, offsets);
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
                apply_shadow_rules(node, scopes, offsets);
            }
            walk_children_with_flow(node, scopes, state)
        }
        _ => walk_children_with_flow(node, scopes, state),
    }
}

fn shadowed_contextual_llap_offset(node: &LinkedNode<'_>) -> Option<usize> {
    llap_body(node.get())?;
    node.children()
        .find(|child| child.kind() == SyntaxKind::Context)
        .map(|kw| kw.range().end)
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
