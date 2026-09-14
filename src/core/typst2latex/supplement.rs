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

use crate::features::refs::{reference_to_latex, Reference};
#[cfg(not(target_arch = "wasm32"))]
use crate::{core::typst2latex::utils::FuncArgs, features::bibtex::parse_bibtex};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Section,
    Figure,
    Table,
}

impl TargetKind {
    fn supplement(self) -> &'static str {
        match self {
            TargetKind::Section => "Section",
            TargetKind::Figure => "Figure",
            TargetKind::Table => "Table",
        }
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
pub(crate) fn resolve_references(
    root: &SyntaxNode,
    latex: &str,
    bibliography_keys: Option<&HashSet<String>>,
) -> String {
    let index = index_document_targets(root);
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
                out.push_str(kind.supplement());
                out.push('~');
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

/// Which supplement Typst renders for the element this label follows.
fn classify(node: &SyntaxNode) -> Option<TargetKind> {
    match node.kind() {
        SyntaxKind::Heading => Some(TargetKind::Section),
        SyntaxKind::FuncCall => match callee(node)?.as_str() {
            // A `#figure` is a table when its body is one; Typst picks the
            // supplement from the figure's `kind` the same way.
            "figure" => Some(if contains_call(node, &["table", "grid"]) {
                TargetKind::Table
            } else {
                TargetKind::Figure
            }),
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

/// Whether the call's arguments contain a call to one of `names`.
fn contains_call(node: &SyntaxNode, names: &[&str]) -> bool {
    node.children().any(|child| {
        (child.kind() == SyntaxKind::FuncCall
            && callee(child).is_some_and(|name| names.contains(&name.as_str())))
            || contains_call(child, names)
    })
}
