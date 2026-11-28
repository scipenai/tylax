//! Finalizing bare Typst references after the complete document is known.
//!
//! Typst renders `@sec-one` as "Section 1" — the supplement comes from the kind
//! of element the label is attached to, and the source never writes it. LaTeX's
//! `\ref` renders the bare number, so converting `@sec-one` to `\ref{sec-one}`
//! silently turns "See Section 1" into "See 1" (issue #43).
//!
//! The word therefore has to be recovered from what each label actually points
//! at. A bare `@target` also needs document-level context: it is a local
//! cross-reference when `target` is a document label, and conventionally a
//! citation when `target` is an entry in an external bibliography and has no
//! such label. File-oriented conversion supplies those BibTeX keys explicitly;
//! string-only and WASM APIs conservatively retain bare references because they
//! cannot verify external files. Explicit `#ref(...)` and `#cite(...)` remain
//! authoritative on every path.
//!
//! Both decisions are resolved after the walk with inert sentinels. This only
//! rewrites converter-emitted markers, never text that merely looks like a
//! reference, such as a `\ref` inside a converted raw block.
//!
//! Guessing from the label's name prefix (`sec-`, `fig-`) is deliberately NOT
//! done: names are free-form and a wrong guess would state something false.

use super::engine::{Arg, ContentNode, Value};
use crate::core::typst2latex::utils::FuncArgs;
#[cfg(not(target_arch = "wasm32"))]
use crate::features::bibtex::parse_bibtex;
use crate::features::refs::{reference_to_latex, Reference};
use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs, path::Path};
use typst_syntax::{SyntaxKind, SyntaxNode};

/// Sentinels bracketing an emitted reference command. Private-use characters,
/// so they cannot collide with document text, and always removed by
/// [`resolve_supplements`].
pub(crate) const REF_MARK_START: char = '\u{E012}';
pub(crate) const REF_MARK_END: char = '\u{E013}';

/// Sentinels for bare Typst `@target` references. Unlike `#ref`, their final
/// LaTeX representation depends on whether the target is a local label or a
/// bibliography entry in the completed document.
const AT_MARK_START: char = '\u{E014}';
const AT_MARK_END: char = '\u{E015}';
/// Separates target from supplement. Private use, so neither half contains it.
const AT_SUPPLEMENT_SEP: char = '\u{E016}';

/// The kind of element a label is attached to, and the LaTeX word Typst would
/// have rendered for it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetKind {
    Section,
    Figure,
    Table,
    Custom(String),
}

impl TargetKind {
    fn supplement(&self) -> &str {
        match self {
            TargetKind::Section => "Section",
            TargetKind::Figure => "Figure",
            TargetKind::Table => "Table",
            TargetKind::Custom(s) => s.as_str(),
        }
    }
}

/// Strip matching balanced outer parentheses from a Typst argument expression.
pub(crate) fn unwrap_balanced_parens(mut s: &str) -> &str {
    loop {
        let trimmed = s.trim();
        let Some(inner) = trimmed.strip_prefix('(').and_then(|t| t.strip_suffix(')')) else {
            return trimmed;
        };
        let mut depth = 0usize;
        let mut in_quote = false;
        let mut escaped = false;
        let mut wraps_all = true;
        for ch in inner.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            if in_quote {
                if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_quote = false;
                }
                continue;
            }
            match ch {
                '"' => in_quote = true,
                '(' => depth += 1,
                ')' => {
                    if depth == 0 {
                        wraps_all = false;
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        if wraps_all && depth == 0 && !in_quote {
            s = inner;
        } else {
            return trimmed;
        }
    }
}

/// Pure-ASCII counter name for a custom `figure(kind: ...)` so TeX macros remain valid
/// while keeping case sensitivity (e.g. `"Example"` vs `"example"`).
pub(crate) fn custom_figure_counter_name(kind: &str) -> String {
    let normalized = kind.trim();
    if !normalized.is_empty()
        && normalized.bytes().all(|b| b.is_ascii_alphabetic())
        && !normalized.starts_with("hex")
    {
        return format!("tylaxfig{normalized}");
    }
    let mut out = String::from("tylaxfighex");
    for &b in normalized.as_bytes() {
        out.push((b'a' + (b >> 4)) as char);
        out.push((b'a' + (b & 0x0f)) as char);
    }
    out
}

/// Default cross-reference / caption supplement word for a custom `figure(kind: ...)`.
pub(crate) fn default_custom_figure_supplement(kind: &str) -> String {
    let trimmed = kind.trim();
    if trimmed == "raw" {
        return "Listing".to_string();
    }
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => {
            let mut out = first.to_uppercase().collect::<String>();
            out.push_str(chars.as_str());
            out
        }
        None => "Figure".to_string(),
    }
}

/// Wrap a rendered reference command so the finalizer can find it.
pub(crate) fn mark_reference(rendered: &str) -> String {
    format!("{REF_MARK_START}{rendered}{REF_MARK_END}")
}

/// Defer a bare Typst `@target` until document-level target resolution.
///
/// The supplement travels with the target: `\cite`'s postnote for a
/// bibliography entry, the word before `\ref` for a label.
pub(crate) fn mark_at_reference(target: &str, supplement: Option<&str>) -> String {
    match supplement {
        Some(supplement) => {
            format!("{AT_MARK_START}{target}{AT_SUPPLEMENT_SEP}{supplement}{AT_MARK_END}")
        }
        None => format!("{AT_MARK_START}{target}{AT_MARK_END}"),
    }
}

/// Strip every reference sentinel without adding supplements. Used when the
/// document AST is unavailable, so markers can never leak into the output.
pub(crate) fn strip_marks(latex: &str) -> String {
    let latex = resolve_at_references_without_document(latex);
    latex.replace([REF_MARK_START, REF_MARK_END], "")
}

/// Resolve both deferred bare references and reference supplements.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn resolve_references(
    root: &SyntaxNode,
    latex: &str,
    bibliography_keys: Option<&HashSet<String>>,
) -> String {
    resolve_references_with_nodes(root, None, latex, bibliography_keys)
}

/// Resolve deferred bare references and reference supplements using both the
/// parsed document AST and evaluated content-node anchors.
pub(crate) fn resolve_references_with_nodes(
    root: &SyntaxNode,
    nodes: Option<&[ContentNode]>,
    latex: &str,
    bibliography_keys: Option<&HashSet<String>>,
) -> String {
    let mut index = index_document_targets(root);
    if let Some(nodes) = nodes {
        index_content_node_anchors(nodes, &mut index);
    }
    let latex = resolve_at_references(latex, &index, bibliography_keys);
    resolve_supplements_with_index(&index, &latex)
}

/// Read the keys declared by literal `#bibliography("...")` calls relative to
/// a source document. This belongs to the file-conversion boundary: generic
/// string and WASM conversion intentionally have no implicit filesystem access.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn load_bibliography_keys(input: &str, source_path: &Path) -> HashSet<String> {
    let root = typst_syntax::parse(input);
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));

    bibliography_paths(&root)
        .into_iter()
        .filter_map(|path| fs::read_to_string(base_dir.join(path)).ok())
        .flat_map(|contents| parse_bibtex(&contents).into_iter().map(|entry| entry.key))
        .collect()
}

/// Rewrite each marked reference to carry the supplement word that Typst would
/// render for its target, then remove the sentinels.
fn resolve_supplements_with_index(index: &DocumentTargets, latex: &str) -> String {
    if index.supplements.is_empty() {
        return latex.replace([REF_MARK_START, REF_MARK_END], "");
    }

    let mut out = String::with_capacity(latex.len());
    let mut rest = latex;

    while let Some(start) = rest.find(REF_MARK_START) {
        let Some(end) = rest[start..].find(REF_MARK_END).map(|p| start + p) else {
            break;
        };
        out.push_str(&rest[..start]);

        let command = &rest[start + REF_MARK_START.len_utf8()..end];
        match referenced_label(command).and_then(|label| index.supplements.get(label)) {
            // A tie (`~`) matches the usual LaTeX spelling and keeps the word
            // and its number on one line.
            Some(kind) => {
                let supp = kind.supplement();
                if !supp.is_empty() {
                    out.push_str(supp);
                    out.push('~');
                }
                out.push_str(command);
            }
            // Unknown target: emit the reference unchanged rather than invent a
            // word that might be wrong.
            None => out.push_str(command),
        }

        rest = &rest[end + REF_MARK_END.len_utf8()..];
    }

    out.push_str(rest);
    out.replace([REF_MARK_START, REF_MARK_END], "")
}

fn resolve_at_references(
    latex: &str,
    index: &DocumentTargets,
    bibliography_keys: Option<&HashSet<String>>,
) -> String {
    resolve_at_markers(latex, |target, supplement| {
        if index.labels.contains(target)
            || !bibliography_keys.is_some_and(|keys| keys.contains(target))
        {
            render_at_label(target, supplement)
        } else {
            match supplement {
                // `@key[]` asks for no supplement, which for a citation is
                // simply no postnote -- not an empty `\cite[]{..}`.
                Some(note) if !note.is_empty() => format!("\\cite[{note}]{{{target}}}"),
                _ => format!("\\cite{{{target}}}"),
            }
        }
    })
}

fn resolve_at_references_without_document(latex: &str) -> String {
    resolve_at_markers(latex, render_at_label)
}

/// Render `@label` / `@label[supplement]` as a cross-reference.
///
/// Either explicit form is emitted UNMARKED, since the supplement replaces the
/// automatic word: [`resolve_supplements_with_index`] rewrites only marked
/// references, and marking these would prepend a second word
/// ("Section p. 5 1"). An empty one therefore leaves a bare `\ref{..}`.
fn render_at_label(target: &str, supplement: Option<&str>) -> String {
    let rendered = reference_to_latex(&Reference::new(target.to_string()));
    match supplement {
        Some("") => rendered,
        Some(word) => format!("{word}~{rendered}"),
        None => mark_reference(&rendered),
    }
}

fn resolve_at_markers(latex: &str, resolve: impl Fn(&str, Option<&str>) -> String) -> String {
    let mut out = String::with_capacity(latex.len());
    let mut rest = latex;

    while let Some(start) = rest.find(AT_MARK_START) {
        let Some(end) = rest[start..].find(AT_MARK_END).map(|offset| start + offset) else {
            break;
        };
        out.push_str(&rest[..start]);
        let body = &rest[start + AT_MARK_START.len_utf8()..end];
        let (target, supplement) = match body.split_once(AT_SUPPLEMENT_SEP) {
            Some((target, supplement)) => (target, Some(supplement)),
            None => (body, None),
        };
        out.push_str(&resolve(target, supplement));
        rest = &rest[end + AT_MARK_END.len_utf8()..];
    }

    out.push_str(rest);
    out.replace([AT_MARK_START, AT_MARK_END, AT_SUPPLEMENT_SEP], "")
}

/// The label a rendered `\ref{..}`-style command points at.
fn referenced_label(command: &str) -> Option<&str> {
    let open = command.find('{')?;
    let close = command.rfind('}')?;
    if close <= open {
        return None;
    }
    Some(command[open + 1..close].trim())
}

/// Map every label in the document to the kind of element it is attached to.
#[derive(Default)]
struct DocumentTargets {
    labels: HashSet<String>,
    supplements: HashMap<String, TargetKind>,
}

fn index_document_targets(root: &SyntaxNode) -> DocumentTargets {
    let mut targets = DocumentTargets::default();
    visit(root, &mut targets);
    targets
}

fn target_kind_from_anchor(kind: &str) -> Option<TargetKind> {
    match kind {
        "section" => Some(TargetKind::Section),
        "figure" => Some(TargetKind::Figure),
        "table" => Some(TargetKind::Table),
        other => Some(TargetKind::Custom(other.to_string())),
    }
}

pub(crate) fn plain_text_from_eval_nodes(nodes: &[ContentNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            ContentNode::Text(t) | ContentNode::RawSource(t) => out.push_str(t),
            ContentNode::Space => out.push(' '),
            ContentNode::Strong(inner) | ContentNode::Emph(inner) => {
                out.push_str(&plain_text_from_eval_nodes(inner));
            }
            _ => out.push_str(&node.to_typst()),
        }
    }
    out.trim().to_string()
}

fn content_node_has_table_or_grid(node: &ContentNode) -> bool {
    if let Some((target, _, _, _)) = node.labeled_parts() {
        return target.iter().any(content_node_has_table_or_grid);
    }
    match node {
        ContentNode::FuncCall { name, args } => {
            matches!(name.as_str(), "table" | "grid")
                || args.iter().any(|arg| match arg {
                    Arg::Pos(Value::Content(nodes))
                    | Arg::Named(_, Value::Content(nodes))
                    | Arg::Spread(Value::Content(nodes)) => {
                        nodes.iter().any(content_node_has_table_or_grid)
                    }
                    _ => false,
                })
        }
        ContentNode::Element { name, fields } => {
            matches!(name.as_str(), "table" | "grid")
                || fields.values().any(|val| match val {
                    Value::Content(nodes) => nodes.iter().any(content_node_has_table_or_grid),
                    Value::Array(items) => items.iter().any(|item| match item {
                        Value::Content(nodes) => nodes.iter().any(content_node_has_table_or_grid),
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
            nodes.iter().any(content_node_has_table_or_grid)
        }
        _ => false,
    }
}

fn classify_evaluated_figure(args: &[Arg]) -> TargetKind {
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
            Value::Content(nodes) => Some(plain_text_from_eval_nodes(nodes)),
            other => Some(other.display().trim().to_string()),
        });

    let default_kind = match explicit_kind {
        None | Some(Value::Auto) => {
            let is_table = args.iter().any(|arg| match arg {
                Arg::Pos(Value::Content(nodes)) => nodes.iter().any(content_node_has_table_or_grid),
                _ => false,
            });
            if is_table {
                TargetKind::Table
            } else {
                TargetKind::Figure
            }
        }
        Some(Value::Str(s)) => match s.as_str() {
            "table" => TargetKind::Table,
            "image" | "figure" => TargetKind::Figure,
            other => TargetKind::Custom(default_custom_figure_supplement(other)),
        },
        Some(Value::Func(f)) => match f.builtin_name() {
            Some("table") => TargetKind::Table,
            Some("image" | "figure") | None => TargetKind::Figure,
            Some(other) => TargetKind::Custom(default_custom_figure_supplement(other)),
        },
        Some(other) => {
            let raw = other.display();
            let cleaned = unwrap_balanced_parens(&raw)
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            match cleaned {
                "" | "auto" | "image" | "figure" => TargetKind::Figure,
                "table" => TargetKind::Table,
                kind_name => TargetKind::Custom(default_custom_figure_supplement(kind_name)),
            }
        }
    };
    match explicit_supp {
        Some(s) => TargetKind::Custom(s),
        None => default_kind,
    }
}

fn classify_content_node(node: &ContentNode) -> Option<TargetKind> {
    match node {
        ContentNode::Heading { .. } => Some(TargetKind::Section),
        ContentNode::Element { name, .. } if name == "heading" => Some(TargetKind::Section),
        ContentNode::FuncCall { name, args } => match name.as_str() {
            "heading" => Some(TargetKind::Section),
            "figure" => Some(classify_evaluated_figure(args)),
            "table" | "grid" => Some(TargetKind::Table),
            "image" => Some(TargetKind::Figure),
            _ => None,
        },
        _ => None,
    }
}

fn index_value_anchors(val: &Value, targets: &mut DocumentTargets) {
    match val {
        Value::Content(nodes) => index_content_node_anchors(nodes, targets),
        Value::Array(items) => {
            for item in items {
                index_value_anchors(item, targets);
            }
        }
        _ => {}
    }
}

fn index_content_node_anchors(nodes: &[ContentNode], targets: &mut DocumentTargets) {
    for (idx, node) in nodes.iter().enumerate() {
        if let Some((_, kind, Some(label), _)) = node.ref_anchor_parts() {
            targets.labels.insert(label.to_string());
            if let Some(target_kind) = target_kind_from_anchor(kind) {
                targets.supplements.insert(label.to_string(), target_kind);
            }
            continue;
        }
        if let Some((target, _, label, _)) = node.labeled_parts() {
            targets.labels.insert(label.to_string());
            if let [single] = target {
                if let Some(kind) = classify_content_node(single) {
                    targets.supplements.insert(label.to_string(), kind);
                }
            }
            index_content_node_anchors(target, targets);
            continue;
        }
        if matches!(node, ContentNode::Label(_) | ContentNode::LabelDef(_)) {
            let lbl = match node {
                ContentNode::Label(l) | ContentNode::LabelDef(l) => l,
                _ => unreachable!(),
            };
            targets.labels.insert(lbl.clone());
            if let Some(prev) = nodes[..idx]
                .iter()
                .rev()
                .find(|n| !matches!(n, ContentNode::Space | ContentNode::Linebreak))
            {
                if let Some(kind) = classify_content_node(prev) {
                    targets.supplements.insert(lbl.clone(), kind);
                }
            }
            continue;
        }
        match node {
            ContentNode::Strong(children)
            | ContentNode::Emph(children)
            | ContentNode::ListItem(children)
            | ContentNode::Heading {
                content: children, ..
            }
            | ContentNode::EnumItem {
                content: children, ..
            } => index_content_node_anchors(children, targets),
            ContentNode::Element { fields, .. } => {
                for val in fields.values() {
                    index_value_anchors(val, targets);
                }
            }
            ContentNode::FuncCall { args, .. } => {
                for arg in args {
                    match arg {
                        Arg::Pos(val) | Arg::Named(_, val) | Arg::Spread(val) => {
                            index_value_anchors(val, targets);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn visit(node: &SyntaxNode, targets: &mut DocumentTargets) {
    let children: Vec<&SyntaxNode> = node.children().collect();

    for (index, child) in children.iter().enumerate() {
        if child.kind() == SyntaxKind::Label {
            // A heading carries its label as a child (`= One <sec-one>`);
            // everything else is labelled by a following sibling.
            let kind = if node.kind() == SyntaxKind::Heading {
                Some(TargetKind::Section)
            } else {
                children[..index]
                    .iter()
                    .rev()
                    .find(|sibling| !is_trivia(sibling.kind()))
                    .and_then(|sibling| classify(sibling))
            };

            if let Some(name) = label_name(child) {
                targets.labels.insert(name.clone());
                if let Some(kind) = kind {
                    targets.supplements.insert(name, kind);
                }
            }
        }

        visit(child, targets);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn bibliography_paths(root: &SyntaxNode) -> Vec<String> {
    let mut paths = Vec::new();
    collect_bibliography_paths(root, &mut paths);
    paths
}

#[cfg(not(target_arch = "wasm32"))]
fn collect_bibliography_paths(node: &SyntaxNode, paths: &mut Vec<String>) {
    if node.kind() == SyntaxKind::FuncCall && callee(node).as_deref() == Some("bibliography") {
        let children: Vec<_> = node.children().collect();
        let args = FuncArgs::from_func_call(&children);
        if let Some(path) = args
            .first_node()
            .filter(|arg| arg.kind() == SyntaxKind::Str)
            .map(string_content)
        {
            paths.push(path);
        }
    }

    for child in node.children() {
        collect_bibliography_paths(child, paths);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn string_content(node: &SyntaxNode) -> String {
    node.text()
        .trim()
        .trim_start_matches('"')
        .trim_end_matches('"')
        .to_string()
}

fn is_trivia(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Space | SyntaxKind::Parbreak | SyntaxKind::Linebreak | SyntaxKind::LineComment
    )
}

/// `<sec-one>` -> `sec-one`.
fn label_name(node: &SyntaxNode) -> Option<String> {
    let text = node.text().trim();
    let name = text.trim_start_matches('<').trim_end_matches('>').trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn ast_supplement_text(raw: &str) -> Option<String> {
    let inner = unwrap_balanced_parens(raw);
    match inner {
        "" | "auto" => None,
        "none" => Some(String::new()),
        _ if inner.starts_with('[') && inner.ends_with(']') && inner.len() >= 2 => {
            Some(inner[1..inner.len() - 1].trim().to_string())
        }
        _ if ((inner.starts_with('"') && inner.ends_with('"'))
            || (inner.starts_with('\'') && inner.ends_with('\'')))
            && inner.len() >= 2 =>
        {
            Some(inner[1..inner.len() - 1].to_string())
        }
        _ => Some(inner.to_string()),
    }
}

/// Which supplement Typst renders for the element this label follows.
fn classify(node: &SyntaxNode) -> Option<TargetKind> {
    match node.kind() {
        SyntaxKind::Heading => Some(TargetKind::Section),
        SyntaxKind::FuncCall => match callee(node)?.as_str() {
            "heading" => Some(TargetKind::Section),
            // A `#figure` is a table when its body is one; Typst picks the
            // supplement from the figure's `kind` the same way.
            "figure" => {
                let children: Vec<_> = node.children().collect();
                let args = FuncArgs::from_func_call(&children);
                let explicit_supp = args
                    .iter()
                    .filter(|arg| arg.name.as_deref() == Some("supplement"))
                    .last()
                    .and_then(|arg| ast_supplement_text(&arg.value));
                let kind_val = args
                    .iter()
                    .filter(|arg| arg.name.as_deref() == Some("kind"))
                    .last()
                    .map(|arg| {
                        unwrap_balanced_parens(arg.value.trim())
                            .trim_matches('"')
                            .trim_matches('\'')
                            .trim()
                            .to_string()
                    });
                let base_kind = match kind_val.as_deref() {
                    None | Some("" | "auto") => {
                        if figure_ast_has_table_or_grid(node) {
                            TargetKind::Table
                        } else {
                            TargetKind::Figure
                        }
                    }
                    Some("table") => TargetKind::Table,
                    Some("image" | "figure") => TargetKind::Figure,
                    Some(other) => {
                        return Some(TargetKind::Custom(
                            explicit_supp
                                .unwrap_or_else(|| default_custom_figure_supplement(other)),
                        ));
                    }
                };
                Some(match explicit_supp {
                    Some(supp) => TargetKind::Custom(supp),
                    None => base_kind,
                })
            }
            "table" | "grid" => Some(TargetKind::Table),
            "image" => Some(TargetKind::Figure),
            _ => None,
        },
        _ => None,
    }
}

/// Name of the function a `FuncCall` invokes.
fn callee(node: &SyntaxNode) -> Option<String> {
    node.children()
        .next()
        .map(|callee| callee.text().trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Whether `node` itself or any descendant is a call to one of `names`.
pub(crate) fn node_contains_call(node: &SyntaxNode, names: &[&str]) -> bool {
    (node.kind() == SyntaxKind::FuncCall
        && callee(node).is_some_and(|name| names.contains(&name.as_str())))
        || node
            .children()
            .any(|child| node_contains_call(child, names))
}

/// Whether a `#figure(...)` AST node's positional body arguments contain a `table` or `grid`.
pub(crate) fn figure_ast_has_table_or_grid(figure_node: &SyntaxNode) -> bool {
    let children: Vec<_> = figure_node.children().collect();
    if let Some(args) = children.get(1) {
        for child in args.children() {
            if matches!(
                child.kind(),
                SyntaxKind::Named
                    | SyntaxKind::LeftParen
                    | SyntaxKind::RightParen
                    | SyntaxKind::Comma
                    | SyntaxKind::Space
                    | SyntaxKind::Linebreak
                    | SyntaxKind::Parbreak
                    | SyntaxKind::LineComment
                    | SyntaxKind::BlockComment
            ) {
                continue;
            }
            if node_contains_call(child, &["table", "grid"]) {
                return true;
            }
        }
    }
    children
        .iter()
        .skip(2)
        .filter(|child| child.kind() == SyntaxKind::ContentBlock)
        .any(|child| node_contains_call(child, &["table", "grid"]))
}
