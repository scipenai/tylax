//! Which `(`, `[`, `{` delimiters of LaTeX math stay unpaired in Typst.
//!
//! Typst closes an opening bracket with any closing one, and a call argument
//! must not hold an unpaired one. The converter escapes the delimiters this
//! pass returns.

use std::collections::HashSet;

use mitex_parser::syntax::{CmdItem, SyntaxElement, SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;

use crate::data::symbols::is_big_delimiter_command;

use super::context::is_command_named;

/// The delimiters in `root` that do not pair within their own sequence of
/// siblings, by start offset.
///
/// A Typst call argument must not hold an unpaired `(`, `[` or `{`, and every
/// TeX group becomes one. So delimiters are paired within each sequence, with
/// nested groups opaque, and any opener matches any closer, as in Typst. The
/// leftovers are escaped: in `\det(e^{-i\theta^0)}` both parentheses are,
/// since one sits in the superscript and the other outside it.
pub(super) fn unpaired_delimiters(root: &SyntaxNode) -> HashSet<u32> {
    let mut unpaired = HashSet::new();
    let mut pending = vec![root.clone()];
    while let Some(node) = pending.pop() {
        let mut sequence = Vec::new();
        let mut fixed = Vec::new();
        flatten_delimiter_sequence(&node, &mut sequence, &mut pending, &mut unpaired);
        if node.kind() == SyntaxKind::ItemLR {
            // The body of `\left. \ln ( x \right)` pairs with the delimiter.
            let clauses: Vec<_> = node
                .children()
                .filter(|child| child.kind() == SyntaxKind::ClauseLR)
                .collect();
            let delimiter = |clause: &SyntaxNode| {
                clause.children_with_tokens().find_map(|e| {
                    let t = e.into_token()?;
                    matches!(
                        t.kind(),
                        SyntaxKind::TokenLParen
                            | SyntaxKind::TokenRParen
                            | SyntaxKind::TokenLBracket
                            | SyntaxKind::TokenRBracket
                    )
                    .then(|| u32::from(t.text_range().start()))
                })
            };
            // Two bracket delimiters pair with each other, so `\left( a)\right)`
            // escapes the body's `)`. One alone may pair with the body.
            if let [left, right] = clauses.as_slice() {
                match (delimiter(left), delimiter(right)) {
                    (Some(offset), None) => {
                        sequence.insert(0, Some((offset, true)));
                        fixed.push(offset);
                    }
                    (None, Some(offset)) => {
                        sequence.push(Some((offset, false)));
                        fixed.push(offset);
                    }
                    _ => {}
                }
            }
        }
        let mut open = Vec::new();
        for item in sequence {
            match item {
                Some((offset, true)) => open.push(offset),
                Some((offset, false)) => {
                    if open.pop().is_none() {
                        unpaired.insert(offset);
                    }
                }
                None => unpaired.extend(open.drain(..)),
            }
        }
        unpaired.extend(open);
        // `\left`/`\right` delimiters pair, but the `lr` code writes them.
        for offset in fixed {
            unpaired.remove(&offset);
        }
    }
    unpaired
}

/// The delimiters of one sequence: `Some((offset, is_opener))`, or `None` at
/// a boundary (`&`, `\\`, or a style declaration whose rest becomes a call).
/// Nested groups go to `pending`, to be paired on their own. The nucleus of
/// an attachment is written unwrapped, so it stays in the sequence.
fn flatten_delimiter_sequence(
    node: &SyntaxNode,
    sequence: &mut Vec<Option<(u32, bool)>>,
    pending: &mut Vec<SyntaxNode>,
    unpaired: &mut HashSet<u32>,
) {
    let delimiter = |elem: &SyntaxElement| -> Option<bool> {
        match elem {
            SyntaxElement::Token(t) => match t.kind() {
                SyntaxKind::TokenLParen | SyntaxKind::TokenLBracket => Some(true),
                SyntaxKind::TokenRParen | SyntaxKind::TokenRBracket => Some(false),
                SyntaxKind::TokenCommandSym if t.text() == "\\{" => Some(true),
                SyntaxKind::TokenCommandSym if t.text() == "\\}" => Some(false),
                _ => None,
            },
            SyntaxElement::Node(_) if is_command_named(elem, "{") => Some(true),
            SyntaxElement::Node(_) if is_command_named(elem, "}") => Some(false),
            SyntaxElement::Node(_) => None,
        }
    };
    let offset = |elem: &SyntaxElement| u32::from(elem.text_range().start());
    let mut after_script = false;
    for (index, child) in node.children_with_tokens().enumerate() {
        let kind = child.kind();
        if node.kind() == SyntaxKind::ItemAttachComponent {
            match kind {
                SyntaxKind::TokenUnderscore | SyntaxKind::TokenCaret => {
                    after_script = true;
                    continue;
                }
                SyntaxKind::TokenWhiteSpace | SyntaxKind::TokenLineBreak => continue,
                _ if after_script => {
                    // A script is a call argument of its own: `T^)`.
                    after_script = false;
                    match &child {
                        SyntaxElement::Node(n) => pending.push(n.clone()),
                        SyntaxElement::Token(_) if delimiter(&child).is_some() => {
                            unpaired.insert(offset(&child));
                        }
                        SyntaxElement::Token(_) => {}
                    }
                    continue;
                }
                SyntaxKind::ClauseArgument if index == 0 => {
                    let SyntaxElement::Node(n) = &child else {
                        continue;
                    };
                    // Typst attaches nothing to an opening delimiter: `[^(x)`
                    // opens a group that starts with `^`.
                    let tokens: Vec<_> = n
                        .children_with_tokens()
                        .filter(|e| e.kind() != SyntaxKind::TokenWhiteSpace)
                        .collect();
                    if let [only] = tokens.as_slice() {
                        if delimiter(only) == Some(true) {
                            unpaired.insert(offset(only));
                            continue;
                        }
                    }
                    flatten_delimiter_sequence(n, sequence, pending, unpaired);
                    continue;
                }
                _ => {}
            }
        }
        if let Some(is_opener) = delimiter(&child) {
            sequence.push(Some((offset(&child), is_opener)));
            continue;
        }
        let SyntaxElement::Node(n) = &child else {
            // `\\` is a token in an environment body.
            if matches!(kind, SyntaxKind::TokenAmpersand | SyntaxKind::ItemNewLine) {
                sequence.push(None);
            }
            continue;
        };
        match kind {
            SyntaxKind::ItemText | SyntaxKind::ItemAttachComponent => {
                flatten_delimiter_sequence(n, sequence, pending, unpaired)
            }
            SyntaxKind::ItemNewLine => sequence.push(None),
            // `\left(` is paired by the `lr` code.
            SyntaxKind::ClauseLR => {}
            // `\bigl(` pairs like a bare `(`. `\middle` sits inside an `lr`.
            SyntaxKind::ItemCmd if is_sized_delimiter_command(&child) => {
                let token = CmdItem::cast(n.clone())
                    .filter(|cmd| cmd.name_tok().is_some_and(|name| name.text() != "\\middle"))
                    .and_then(|cmd| super::markup::sized_delimiter_token(&cmd));
                if let Some(token) = token {
                    if let Some(is_opener) = delimiter(&token) {
                        sequence.push(Some((offset(&token), is_opener)));
                    }
                }
            }
            _ if ["displaystyle", "textstyle", "cal"]
                .iter()
                .any(|name| is_command_named(&child, name)) =>
            {
                sequence.push(None)
            }
            // KaTeX writes `3 {\bigl(} a {\bigr)}`: the group adds nothing.
            SyntaxKind::ItemCurly if holds_only_sized_delimiter(n) => {
                flatten_delimiter_sequence(n, sequence, pending, unpaired)
            }
            _ => pending.push(n.clone()),
        }
    }
}

fn holds_only_sized_delimiter(group: &SyntaxNode) -> bool {
    let mut significant = group.children_with_tokens().filter(|child| {
        !matches!(
            child.kind(),
            SyntaxKind::TokenLBrace
                | SyntaxKind::TokenRBrace
                | SyntaxKind::TokenWhiteSpace
                | SyntaxKind::TokenLineBreak
        )
    });
    matches!(
        (significant.next(), significant.next()),
        (Some(only), None) if is_sized_delimiter_command(&only)
    )
}

fn is_sized_delimiter_command(elem: &SyntaxElement) -> bool {
    let Some(name) = elem
        .as_node()
        .and_then(|node| CmdItem::cast(node.clone()))
        .and_then(|cmd| cmd.name_tok())
    else {
        return false;
    };
    let name = name.text().trim_start_matches('\\');
    is_big_delimiter_command(name) || name == "middle"
}
