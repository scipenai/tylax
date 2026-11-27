use super::context::T2LOptions;
use super::math::build_lr_ir;
use super::utils::{
    escape_latex_text, extract_length_value, get_simple_text, is_content_node,
    normalize_typst_color_expr, parse_spacing_spec, FuncArgs, SpacingSpec, UNICODE_TO_LATEX,
};
use crate::data::maps::{DELIMITER_MAP, TYPST_TO_TEX};
use crate::data::typst_compat::{MathHandler, TYPST_MATH_HANDLERS};
use std::cell::RefCell;
use typst_syntax::ast::{self, AstNode, Escape};
use typst_syntax::{SyntaxKind, SyntaxNode};

#[derive(Debug, Clone, Default)]
pub(crate) struct MathBuildEnv {
    pub(crate) shadowed_context_marker: Option<String>,
}

impl MathBuildEnv {
    pub(crate) fn is_shadowed_context(&self, node: &SyntaxNode) -> bool {
        let Some(marker) = self.shadowed_context_marker.as_deref() else {
            return false;
        };
        node.children()
            .any(|child| child.kind() == SyntaxKind::BlockComment && child.text() == marker)
    }

    pub(crate) fn clean_context_source(&self, node: &SyntaxNode) -> String {
        let raw = format!("#{}", SyntaxNode::clone(node).into_text());
        match self.shadowed_context_marker.as_deref() {
            Some(marker) if raw.contains(marker) => raw.replace(marker, ""),
            _ => raw,
        }
    }
}

// A block comment marks shadowed `#context` through
// preprocessing and reparsing. The conversion-scoped environment identifies
// that marker. The IR builder strips it from retained source, and the
// conversion entry points remove any marker left in the output.
thread_local! {
    static MATH_BUILD_ENV: RefCell<Vec<MathBuildEnv>> = const { RefCell::new(Vec::new()) };
}

struct MathBuildEnvGuard;

impl Drop for MathBuildEnvGuard {
    fn drop(&mut self) {
        MATH_BUILD_ENV.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub(crate) fn with_math_build_env<R>(env: MathBuildEnv, f: impl FnOnce() -> R) -> R {
    MATH_BUILD_ENV.with(|stack| stack.borrow_mut().push(env));
    let _guard = MathBuildEnvGuard;
    f()
}

pub(crate) fn current_math_build_env() -> MathBuildEnv {
    MATH_BUILD_ENV.with(|stack| stack.borrow().last().cloned().unwrap_or_default())
}

pub(crate) fn is_shadowed_node(node: &SyntaxNode) -> bool {
    MATH_BUILD_ENV.with(|stack| {
        stack
            .borrow()
            .last()
            .is_some_and(|env| env.is_shadowed_context(node))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathSpacing {
    Soft,
    Space,
    Thin,
    Med,
    Thick,
    Quad,
    Qquad,
    Wide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathCommand {
    pub latex: String,
    pub args: Vec<MathIr>,
    pub optional_arg: Option<Box<MathIr>>,
    pub preserve_empty_args: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathStyleMode {
    Display,
    Inline,
    Script,
    ScriptScript,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathEnvironment {
    Matrix {
        name: String,
        rows: Vec<Vec<MathIr>>,
        /// Lines and alignment that need an `array`, from `mat(augment:, align:)`.
        layout: Option<ArrayLayout>,
    },
    Cases {
        rows: Vec<MathIr>,
    },
}

/// The `array` form of a matrix with rules or a non-centred alignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayLayout {
    /// The column alignment letter: `l`, `c` or `r`.
    pub align: char,
    /// Rules after these columns and rows, counted from 1.
    pub vlines: Vec<usize>,
    pub hlines: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathIr {
    Seq(Vec<MathIr>),
    Linebreak,
    Symbol(String),
    Ident(String),
    Number(String),
    Operator(String),
    Punctuation(char),
    Spacing(MathSpacing),
    Delimited {
        open: String,
        content: Box<MathIr>,
        close: String,
    },
    Apply {
        callee: Box<MathIr>,
        args: Vec<MathIr>,
    },
    Limits(Box<MathIr>),
    Style {
        mode: MathStyleMode,
        content: Box<MathIr>,
    },
    Attachment {
        base: Box<MathIr>,
        top: Option<Box<MathIr>>,
        bottom: Option<Box<MathIr>>,
        top_left: Option<Box<MathIr>>,
        top_right: Option<Box<MathIr>>,
        bottom_left: Option<Box<MathIr>>,
        bottom_right: Option<Box<MathIr>>,
    },
    Script {
        base: Box<MathIr>,
        sub: Option<Box<MathIr>>,
        sup: Option<Box<MathIr>>,
        primes: usize,
        /// The superscript came first in the source (`x^a_b`); LaTeX keeps that order.
        sup_first: bool,
    },
    Command(MathCommand),
    Environment(MathEnvironment),
    RawLiteral(String),
    PreservedSource {
        source: String,
        reason: &'static str,
    },
}

fn normalize_package_sensitive_math_tex(tex: &str) -> &str {
    match tex {
        "coloneqq" => r"\mathrel{:=}",
        "eqqcolon" => r"\mathrel{=:}",
        "Coloneqq" => r"\mathrel{::=}",
        other => other,
    }
}

impl MathIr {
    pub fn empty() -> Self {
        MathIr::Seq(vec![])
    }

    /// A LaTeX command with no optional argument.
    pub fn command(latex: &str, args: Vec<MathIr>) -> Self {
        MathIr::Command(MathCommand {
            latex: latex.to_string(),
            args,
            optional_arg: None,
            preserve_empty_args: false,
        })
    }

    /// A LaTeX command whose positional brace groups are mandatory even when empty.
    pub fn required_command(latex: &str, args: Vec<MathIr>) -> Self {
        MathIr::Command(MathCommand {
            latex: latex.to_string(),
            args,
            optional_arg: None,
            preserve_empty_args: true,
        })
    }

    pub fn is_empty(&self) -> bool {
        match self {
            MathIr::Seq(items) => items.is_empty(),
            MathIr::RawLiteral(text) => text.is_empty(),
            _ => false,
        }
    }
}

pub(crate) fn build_math_ir_with_env(
    node: &SyntaxNode,
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> MathIr {
    match node.kind() {
        SyntaxKind::MathIdent => build_math_ident(node),
        SyntaxKind::FieldAccess => build_field_access(node),
        SyntaxKind::Space => MathIr::Spacing(MathSpacing::Soft),
        SyntaxKind::Escape => build_escape(node),
        SyntaxKind::Linebreak => MathIr::Linebreak,
        SyntaxKind::MathAttach => build_math_attach(node, options, env),
        SyntaxKind::FuncCall => build_func_call(node, options, env),
        SyntaxKind::MathFrac => build_math_frac(node, options, env),
        SyntaxKind::MathRoot => build_math_root(node, options, env),
        SyntaxKind::LeftParen => MathIr::RawLiteral("(".to_string()),
        SyntaxKind::RightParen => MathIr::RawLiteral(")".to_string()),
        SyntaxKind::LeftBracket => MathIr::RawLiteral("[".to_string()),
        SyntaxKind::RightBracket => MathIr::RawLiteral("]".to_string()),
        SyntaxKind::LeftBrace => MathIr::RawLiteral("\\{".to_string()),
        SyntaxKind::RightBrace => MathIr::RawLiteral("\\}".to_string()),
        SyntaxKind::Plus => MathIr::Operator("+".to_string()),
        SyntaxKind::Minus => MathIr::Operator("-".to_string()),
        SyntaxKind::Star => MathIr::Operator("*".to_string()),
        SyntaxKind::Slash => MathIr::Operator("/".to_string()),
        SyntaxKind::Eq => MathIr::Operator("=".to_string()),
        SyntaxKind::EqEq => MathIr::Operator("==".to_string()),
        SyntaxKind::Lt => MathIr::Operator("<".to_string()),
        SyntaxKind::Gt => MathIr::Operator(">".to_string()),
        SyntaxKind::LtEq => MathIr::Operator("\\le".to_string()),
        SyntaxKind::GtEq => MathIr::Operator("\\ge".to_string()),
        SyntaxKind::Comma => MathIr::Punctuation(','),
        SyntaxKind::Colon => MathIr::Punctuation(':'),
        SyntaxKind::Semicolon => MathIr::Punctuation(';'),
        SyntaxKind::Dots => MathIr::RawLiteral("\\dots".to_string()),
        SyntaxKind::Int | SyntaxKind::Float => MathIr::Number(node.text().to_string()),
        // Resolve the string's escapes (`\"`), then escape what LaTeX text mode
        // treats specially. An empty string is how L -> T writes an empty base
        // (`""_(a)`), and `\text{}_a` would lose its argument.
        SyntaxKind::Str => {
            let text = str_value(node);
            if text.is_empty() {
                MathIr::RawLiteral("{}".to_string())
            } else {
                MathIr::command(
                    "\\text",
                    vec![MathIr::RawLiteral(escape_latex_math_text(&text))],
                )
            }
        }
        SyntaxKind::Raw => build_raw_math_text(node),
        SyntaxKind::MathDelimited => build_math_delimited(node, options, env),
        SyntaxKind::ContentBlock => build_content_block(node, options, env),
        // An equation passed as an argument, as in `#box(width: 0pt, $x$)`.
        SyntaxKind::Equation => child_of_kind(node, SyntaxKind::Math)
            .map_or_else(MathIr::empty, |math| {
                build_math_ir_with_env(math, options, env)
            }),
        SyntaxKind::MathPrimes => {
            let count = count_math_primes(node);
            MathIr::RawLiteral("'".repeat(count))
        }
        _ => build_generic(node, options, env),
    }
}

pub fn normalize_math_ir(ir: MathIr) -> MathIr {
    match ir {
        MathIr::Seq(items) => {
            let mut normalized = Vec::new();
            for item in items {
                let item = normalize_math_ir(item);
                match item {
                    MathIr::Seq(children) => {
                        normalized.extend(children.into_iter().filter(|child| !child.is_empty()))
                    }
                    other if !other.is_empty() => normalized.push(other),
                    _ => {}
                }
            }
            if normalized.is_empty() {
                MathIr::empty()
            } else if normalized.len() == 1 {
                normalized.into_iter().next().unwrap_or_else(MathIr::empty)
            } else {
                MathIr::Seq(normalized)
            }
        }
        MathIr::Delimited {
            open,
            content,
            close,
        } => MathIr::Delimited {
            open,
            content: Box::new(normalize_math_ir(*content)),
            close,
        },
        MathIr::Apply { callee, args } => MathIr::Apply {
            callee: Box::new(normalize_math_ir(*callee)),
            args: args
                .into_iter()
                .map(normalize_math_ir)
                .filter(|arg| !arg.is_empty())
                .collect(),
        },
        MathIr::Limits(content) => MathIr::Limits(Box::new(normalize_math_ir(*content))),
        MathIr::Style { mode, content } => MathIr::Style {
            mode,
            content: Box::new(normalize_math_ir(*content)),
        },
        MathIr::Attachment {
            base,
            top,
            bottom,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        } => MathIr::Attachment {
            base: Box::new(normalize_math_ir(*base)),
            top: top.map(|item| Box::new(normalize_math_ir(*item))),
            bottom: bottom.map(|item| Box::new(normalize_math_ir(*item))),
            top_left: top_left.map(|item| Box::new(normalize_math_ir(*item))),
            top_right: top_right.map(|item| Box::new(normalize_math_ir(*item))),
            bottom_left: bottom_left.map(|item| Box::new(normalize_math_ir(*item))),
            bottom_right: bottom_right.map(|item| Box::new(normalize_math_ir(*item))),
        },
        MathIr::Script {
            base,
            sub,
            sup,
            primes,
            sup_first,
        } => MathIr::Script {
            base: Box::new(normalize_math_ir(*base)),
            sub: sub.map(|item| Box::new(normalize_math_ir(*item))),
            sup: sup.map(|item| Box::new(normalize_math_ir(*item))),
            primes,
            sup_first,
        },
        MathIr::Command(mut command) => {
            let preserve_empty = command.preserve_empty_args;
            command.args = command
                .args
                .into_iter()
                .map(normalize_math_ir)
                .filter(|arg| preserve_empty || !arg.is_empty())
                .collect();
            command.optional_arg = command
                .optional_arg
                .map(|arg| Box::new(normalize_math_ir(*arg)));
            MathIr::Command(command)
        }
        MathIr::Environment(environment) => match environment {
            MathEnvironment::Matrix { name, rows, layout } => {
                MathIr::Environment(MathEnvironment::Matrix {
                    name,
                    layout,
                    rows: rows
                        .into_iter()
                        .map(|row| {
                            row.into_iter()
                                .map(normalize_math_ir)
                                .filter(|cell| !cell.is_empty())
                                .collect()
                        })
                        .filter(|row: &Vec<MathIr>| !row.is_empty())
                        .collect(),
                })
            }
            MathEnvironment::Cases { rows } => MathIr::Environment(MathEnvironment::Cases {
                rows: rows
                    .into_iter()
                    .map(normalize_math_ir)
                    .filter(|row| !row.is_empty())
                    .collect(),
            }),
        },
        other => other,
    }
}

fn build_math_ident(node: &SyntaxNode) -> MathIr {
    let text = node.text();
    let text_str = text.as_str();

    match text_str {
        // `zws` is how L -> T writes an empty group (`{}_a`). Dropping it let the
        // next token take its place as a base, script or matrix cell.
        "zws" | "zwsp" => return MathIr::RawLiteral("{}".to_string()),
        "nbsp" => return MathIr::RawLiteral("~".to_string()),
        "wj" => return MathIr::RawLiteral(r"\nobreak".to_string()),
        "shy" => return MathIr::RawLiteral(r"\-".to_string()),
        "quad" => return MathIr::Spacing(MathSpacing::Quad),
        "qquad" => return MathIr::Spacing(MathSpacing::Qquad),
        "space" | "sp" => return MathIr::Spacing(MathSpacing::Space),
        "thin" => return MathIr::Spacing(MathSpacing::Thin),
        "med" => return MathIr::Spacing(MathSpacing::Med),
        "thick" => return MathIr::Spacing(MathSpacing::Thick),
        "wide" => return MathIr::Spacing(MathSpacing::Wide),
        "plus" => return MathIr::Operator("+".to_string()),
        "minus" => return MathIr::Operator("-".to_string()),
        "eq" => return MathIr::Operator("=".to_string()),
        "lt" => return MathIr::Operator("<".to_string()),
        "gt" => return MathIr::Operator(">".to_string()),
        _ => {}
    }

    if let Some(tex) = TYPST_TO_TEX.get(text_str) {
        return MathIr::Symbol(with_leading_backslash(
            normalize_package_sensitive_math_tex(tex),
        ));
    }

    if text_str.len() == 1 {
        if let Some(ch) = text_str.chars().next() {
            if let Some(latex) = UNICODE_TO_LATEX.get(&ch) {
                return MathIr::Symbol((*latex).to_string());
            }
        }
    }

    MathIr::Ident(convert_unicode_in_text(text_str))
}

fn build_field_access(node: &SyntaxNode) -> MathIr {
    let full_text = collect_field_access_text(node);
    let full_text_str = full_text.as_str();

    if let Some(tex) = TYPST_TO_TEX.get(full_text_str) {
        return MathIr::Symbol(with_leading_backslash(
            normalize_package_sensitive_math_tex(tex),
        ));
    }

    if full_text_str == "square.stroked" || full_text_str == "square.filled" {
        return MathIr::Symbol("\\blacksquare".to_string());
    }

    if full_text_str == "bar.v.double" || full_text_str.ends_with(".v.double") {
        return MathIr::Symbol("\\|".to_string());
    }

    MathIr::Ident(full_text)
}

fn build_escape(node: &SyntaxNode) -> MathIr {
    let escaped = Escape::from_untyped(node)
        .map(|escape| escape.get())
        .unwrap_or_default();

    match escaped {
        ',' | ':' | ';' => MathIr::Punctuation(escaped),
        '+' | '-' | '=' | '<' | '>' | '|' | '/' | '*' => MathIr::Operator(escaped.to_string()),
        ch if ch.is_ascii_digit() => MathIr::Number(ch.to_string()),
        ch if ch.is_alphabetic() => MathIr::Ident(ch.to_string()),
        // Escape LaTeX-active characters so math escapes stay literal.
        '{' | '}' | '$' | '%' | '#' | '&' | '_' => MathIr::RawLiteral(format!("\\{escaped}")),
        '\\' => MathIr::Symbol(r"\backslash".to_string()),
        '^' => MathIr::RawLiteral(r"\text{\textasciicircum{}}".to_string()),
        '~' => MathIr::RawLiteral(r"\text{\textasciitilde{}}".to_string()),
        ch => {
            if let Some(latex) = UNICODE_TO_LATEX.get(&ch) {
                MathIr::Symbol((*latex).to_string())
            } else {
                MathIr::RawLiteral(ch.to_string())
            }
        }
    }
}

/// A script operand. The emitter strips grouping parentheses from the
/// rendered script, which cannot tell an escaped `\)` from a real one, so a
/// group holding an escaped delimiter (`x^(a\))`) is unwrapped here.
fn build_script(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    fn holds_escaped_delimiter(node: &SyntaxNode) -> bool {
        node.children().any(|child| {
            (child.kind() == SyntaxKind::Escape
                && matches!(child.text().as_str(), "\\(" | "\\)" | "\\[" | "\\]"))
                || holds_escaped_delimiter(child)
        })
    }
    if holds_escaped_delimiter(node) {
        let only = [node];
        let inner = strip_grouping_parens(&only);
        if inner.len() != 1 || !std::ptr::eq(inner[0], node) {
            let inner: Vec<&SyntaxNode> = inner
                .into_iter()
                .filter(|child| child.kind() != SyntaxKind::Space)
                .collect();
            let content = seq_or_single(
                inner
                    .iter()
                    .map(|child| build_math_ir_with_env(child, options, env))
                    .collect(),
            );
            // `x^(\(a\))`: braces stop the emitter stripping `(a)` again.
            let first_text = inner.first().map(|child| first_leaf_text(child));
            if matches!(first_text.as_deref(), Some("\\(" | "(")) {
                return MathIr::Delimited {
                    open: "{".to_string(),
                    content: Box::new(content),
                    close: "}".to_string(),
                };
            }
            return content;
        }
    }
    build_math_ir_with_env(node, options, env)
}

fn first_leaf_text(node: &SyntaxNode) -> String {
    match node
        .children()
        .find(|child| child.kind() != SyntaxKind::Space)
    {
        Some(child) => first_leaf_text(child),
        None => node.text().to_string(),
    }
}

fn build_math_attach(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let children: Vec<&SyntaxNode> = node
        .children()
        .filter(|child| child.kind() != SyntaxKind::Space)
        .collect();

    // The base is always the first child. Searching further picked a script operand
    // as the base too, building it twice per level: `^'^'^'…` took exponential time.
    // A leading `MathPrimes` (`'^2`) is primes on an empty base, since LaTeX rejects `'^2`.
    let base_idx = match children.first() {
        None => return MathIr::empty(),
        Some(first)
            if matches!(
                first.kind(),
                SyntaxKind::MathPrimes | SyntaxKind::Hat | SyntaxKind::Underscore
            ) =>
        {
            None
        }
        Some(_) => Some(0),
    };

    let base = Box::new(
        base_idx
            .map(|index| build_math_ir_with_env(children[index], options, env))
            .unwrap_or_else(MathIr::empty),
    );
    let mut sub = None;
    let mut sup = None;
    let mut primes = 0usize;
    let mut sup_first = false;

    let mut index = 0;
    while index < children.len() {
        if Some(index) == base_idx {
            index += 1;
            continue;
        }

        match children[index].kind() {
            SyntaxKind::Hat
                if index + 1 < children.len()
                    && children[index + 1].kind() != SyntaxKind::Hat
                    && children[index + 1].kind() != SyntaxKind::Underscore =>
            {
                sup = Some(Box::new(build_script(children[index + 1], options, env)));
                sup_first |= sub.is_none();
                index += 2;
                continue;
            }
            SyntaxKind::Underscore
                if index + 1 < children.len()
                    && children[index + 1].kind() != SyntaxKind::Hat
                    && children[index + 1].kind() != SyntaxKind::Underscore =>
            {
                sub = Some(Box::new(build_script(children[index + 1], options, env)));
                index += 2;
                continue;
            }
            SyntaxKind::MathPrimes => {
                primes += count_math_primes(children[index]);
                index += 1;
                continue;
            }
            _ => {}
        }

        index += 1;
    }

    MathIr::Script {
        base,
        sub,
        sup,
        primes,
        sup_first,
    }
}

fn count_math_primes(node: &SyntaxNode) -> usize {
    node.cast::<typst_syntax::ast::MathPrimes>()
        .map(|primes| primes.count())
        .unwrap_or_else(|| node.text().chars().filter(|&c| c == '\'').count())
}

fn build_func_call(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let children: Vec<&SyntaxNode> = node.children().collect();
    if children.is_empty() {
        return MathIr::empty();
    }

    let func_str = get_math_func_name(children[0]);
    let args_node = children
        .iter()
        .copied()
        .find(|child| child.kind() == SyntaxKind::Args);

    let Some(args_node) = args_node else {
        return build_callable_ir(&func_str, vec![]);
    };

    // `upright(bold(x))` is how L -> T writes `\mathbf{x}`. Converting each wrapper
    // gives `\mathrm{\boldsymbol{x}}`, which reads back one `upright` deeper.
    // `bold(upright(x))` is how it writes `\boldsymbol{\mathrm{x}}`, so it stays.
    if let Some(inner_args) = (func_str == "upright")
        .then(|| sole_call_args(args_node, "bold"))
        .flatten()
    {
        return MathIr::command(r"\mathbf", build_args(inner_args, options, env));
    }

    if let Some(handler) = TYPST_MATH_HANDLERS.get(func_str.as_str()) {
        match handler {
            MathHandler::Command { latex_cmd } => {
                return MathIr::command(latex_cmd, build_positional_args(args_node, options, env));
            }
            MathHandler::CommandWithOpt { latex_cmd } => {
                let args = build_positional_args(args_node, options, env);
                if args.is_empty() {
                    return MathIr::command(latex_cmd, vec![]);
                }

                let (optional_arg, regular_args) = if args.len() >= 2 {
                    let mut iter = args.into_iter();
                    let first = iter.next().map(Box::new);
                    let rest: Vec<MathIr> = iter.collect();
                    (first, vec![seq_or_single(rest)])
                } else {
                    (None, args)
                };

                return MathIr::Command(MathCommand {
                    latex: (*latex_cmd).to_string(),
                    args: regular_args,
                    optional_arg,
                    preserve_empty_args: false,
                });
            }
            MathHandler::Delimiters { open, close } => {
                return MathIr::Delimited {
                    open: (*open).to_string(),
                    content: Box::new(seq_or_single(build_args(args_node, options, env))),
                    close: (*close).to_string(),
                };
            }
            MathHandler::BigOperator { latex_cmd } => {
                return MathIr::Apply {
                    callee: Box::new(MathIr::Symbol((*latex_cmd).to_string())),
                    args: build_args(args_node, options, env),
                };
            }
            MathHandler::Environment { name } => {
                return build_environment(args_node, name, options, env);
            }
            MathHandler::Special => {
                if let Some(ir) = build_special_func_call(
                    func_str.as_str(),
                    args_node,
                    options,
                    children[0].kind() == SyntaxKind::MathIdent,
                    env,
                ) {
                    return ir;
                }
            }
        }
    }

    build_callable_ir(&func_str, build_args(args_node, options, env))
}

/// The argument list of the call `name(…)` when it is the only argument in `args_node`.
fn sole_call_args<'a>(args_node: &'a SyntaxNode, name: &str) -> Option<&'a SyntaxNode> {
    let mut args = args_node.children().filter(|child| is_content_node(child));
    let mut arg = args.next()?;
    if args.next().is_some() {
        return None;
    }
    while arg.kind() == SyntaxKind::Math {
        let mut children = arg
            .children()
            .filter(|child| child.kind() != SyntaxKind::Space);
        arg = children.next()?;
        if children.next().is_some() {
            return None;
        }
    }
    if arg.kind() != SyntaxKind::FuncCall {
        return None;
    }
    let mut children = arg.children();
    if get_math_func_name(children.next()?) != name {
        return None;
    }
    children.find(|child| child.kind() == SyntaxKind::Args)
}

fn build_callable_ir(func_str: &str, args: Vec<MathIr>) -> MathIr {
    let callee = if let Some(tex) = TYPST_TO_TEX.get(func_str) {
        MathIr::Symbol(with_leading_backslash(
            normalize_package_sensitive_math_tex(tex),
        ))
    } else {
        MathIr::command(
            r"\operatorname",
            vec![MathIr::RawLiteral(func_str.to_string())],
        )
    };

    if args.is_empty() {
        callee
    } else {
        MathIr::Apply {
            callee: Box::new(callee),
            args,
        }
    }
}

fn build_special_func_call(
    func_str: &str,
    args_node: &SyntaxNode,
    options: &T2LOptions,
    math_args: bool,
    env: &MathBuildEnv,
) -> Option<MathIr> {
    let args: Vec<&SyntaxNode> = args_node
        .children()
        .filter(|child| is_content_node(child))
        .collect();
    let all_args = || {
        seq_or_single(
            args.iter()
                .map(|arg| build_math_ir_with_env(arg, options, env))
                .collect(),
        )
    };

    match func_str {
        "math.vec" => Some(build_math_vec_ir(args_node, options, env)),
        "lr" => Some(build_lr_ir(args_node, options, env)),
        "attach" => Some(build_attach_ir(args_node, options, env)),
        "scripts" | "display" | "inline" | "script" | "sscript" => Some(MathIr::Style {
            mode: match func_str {
                "inline" => MathStyleMode::Inline,
                "script" => MathStyleMode::Script,
                "sscript" => MathStyleMode::ScriptScript,
                _ => MathStyleMode::Display,
            },
            content: Box::new(all_args()),
        }),
        "primes" => Some(build_primes_ir(args_node)),
        "stretch" => Some(build_stretch_ir(args_node, options, env)),
        "mid" => Some(MathIr::Symbol(r"\mid".to_string())),
        "circle" => args.first().map(|arg| MathIr::Delimited {
            open: r"\mathring{".to_string(),
            content: Box::new(build_math_ir_with_env(arg, options, env)),
            close: "}".to_string(),
        }),
        "divergence" => Some(MathIr::Seq(vec![
            MathIr::Symbol(r"\nabla".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            MathIr::Symbol(r"\cdot".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            all_args(),
        ])),
        "curl" => Some(MathIr::Seq(vec![
            MathIr::Symbol(r"\nabla".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            MathIr::Symbol(r"\times".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            all_args(),
        ])),
        "limits" => args
            .first()
            .map(|arg| MathIr::Limits(Box::new(build_math_ir_with_env(arg, options, env)))),
        "box" => Some(build_box_ir(args_node, options, math_args, env)),
        "text" => Some(build_text_markup_ir(
            r"\text", args_node, options, math_args, env,
        )),
        "emph" => Some(build_text_markup_ir(
            r"\textit", args_node, options, math_args, env,
        )),
        "strong" => Some(build_text_markup_ir(
            r"\textbf", args_node, options, math_args, env,
        )),
        // A sum such as `1em + 2pt` becomes one `\hspace` per term.
        "h" => args.first().and_then(|arg| {
            let arg_text = get_simple_text(arg);
            let terms = arg_text
                .split('+')
                .map(|term| match parse_spacing_spec(term) {
                    Some(SpacingSpec::Fixed(value)) => {
                        Some(MathIr::command(r"\hspace", vec![MathIr::RawLiteral(value)]))
                    }
                    Some(SpacingSpec::Flex(_)) | None => None,
                })
                .collect::<Option<Vec<_>>>()?;
            Some(seq_or_single(terms))
        }),
        "op" => {
            // A string is the operator's name; other content is math. The
            // Unicode spaces that L → T keeps for `\,`, `\:` and `\;` turn back.
            let name = match args.first() {
                Some(arg) if arg.kind() == SyntaxKind::Str => {
                    let name = str_value(arg);
                    let mut latex = String::with_capacity(name.len());
                    for ch in name.chars() {
                        match ch {
                            '\u{2009}' => latex.push_str(r"\,"),
                            '\u{205F}' => latex.push_str(r"\:"),
                            '\u{2004}' => latex.push_str(r"\;"),
                            // `\operatorname{a_b}` would read `b` as a subscript.
                            '_' | '&' | '%' | '#' | '$' | '{' | '}' => {
                                latex.push('\\');
                                latex.push(ch);
                            }
                            _ => latex.push(ch),
                        }
                    }
                    MathIr::RawLiteral(latex)
                }
                Some(arg) => build_math_ir_with_env(arg, options, env),
                None => MathIr::empty(),
            };
            Some(MathIr::command(r"\operatorname", vec![name]))
        }
        "class" => build_class_ir(&args, options, env),
        "set" | "Set" => Some(MathIr::Delimited {
            open: r"\left\{".to_string(),
            content: Box::new(all_args()),
            close: r"\right\}".to_string(),
        }),
        "arrow" => args.first().map(|arg| MathIr::Delimited {
            open: r"\overrightarrow{".to_string(),
            content: Box::new(build_math_ir_with_env(arg, options, env)),
            close: "}".to_string(),
        }),
        "accent" => build_accent_ir(&args, options, env),
        "color" => build_color_ir(&args, options, env),
        _ => None,
    }
}

fn build_math_vec_ir(args_node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let rows = args_node
        .children()
        .filter(|child| is_content_node(child))
        .map(|child| vec![build_math_ir_with_env(child, options, env)])
        .collect();

    MathIr::Environment(MathEnvironment::Matrix {
        name: "pmatrix".to_string(),
        rows,
        layout: None,
    })
}

fn build_attach_ir(args_node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let args = FuncArgs::from_args_node(args_node);
    let base = args
        .positional_node(0)
        .map(|node| build_math_ir_with_env(node, options, env))
        .unwrap_or_else(MathIr::empty);

    let named = |key: &str| {
        args.named_node(key)
            .map(|node| Box::new(build_math_ir_with_env(node, options, env)))
    };

    MathIr::Attachment {
        base: Box::new(base),
        top: named("t").or_else(|| named("top")),
        bottom: named("b").or_else(|| named("bottom")),
        top_left: named("tl"),
        top_right: named("tr"),
        bottom_left: named("bl"),
        bottom_right: named("br"),
    }
}

fn build_primes_ir(args_node: &SyntaxNode) -> MathIr {
    let count = args_node
        .children()
        .find(|child| is_content_node(child))
        .map(get_simple_text)
        .and_then(|text| text.trim().parse::<usize>().ok())
        .unwrap_or(0);
    MathIr::RawLiteral("'".repeat(count))
}

fn build_stretch_ir(args_node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let Some(first_arg) = args_node.children().find(|child| is_content_node(child)) else {
        return MathIr::empty();
    };

    let text = get_simple_text(first_arg);
    match text.as_str() {
        "arrow.r" | "->" => MathIr::RawLiteral(r"\xrightarrow{}".to_string()),
        "arrow.l" | "<-" => MathIr::RawLiteral(r"\xleftarrow{}".to_string()),
        "arrow.l.r" | "<->" => MathIr::RawLiteral(r"\xleftrightarrow{}".to_string()),
        "brace.t" => MathIr::RawLiteral(r"\overbrace{}".to_string()),
        "brace.b" => MathIr::RawLiteral(r"\underbrace{}".to_string()),
        _ => build_math_ir_with_env(first_arg, options, env),
    }
}

fn build_class_ir(
    args: &[&SyntaxNode],
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> Option<MathIr> {
    if args.len() < 2 {
        return None;
    }

    let class_type = get_simple_text(args[0]);
    let latex_cmd = match class_type.trim_matches('"') {
        "relation" => Some(r"\mathrel"),
        "binary" => Some(r"\mathbin"),
        "large" => Some(r"\mathop"),
        "opening" => Some(r"\mathopen"),
        "closing" => Some(r"\mathclose"),
        "punctuation" => Some(r"\mathpunct"),
        _ => None,
    };

    let content = build_math_ir_with_env(args[1], options, env);
    Some(match latex_cmd {
        Some(cmd) => MathIr::Delimited {
            open: format!("{}{{", cmd),
            content: Box::new(content),
            close: "}".to_string(),
        },
        None => content,
    })
}

fn build_accent_ir(
    args: &[&SyntaxNode],
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> Option<MathIr> {
    if args.len() < 2 {
        return None;
    }

    let accent_text = get_simple_text(args[1]);
    let accent_name = accent_text.trim_matches('"');
    let latex_cmd = match accent_name {
        "arrow.l" => r"\overleftarrow",
        "arrow.r" => r"\overrightarrow",
        "arrow.l.r" => r"\overleftrightarrow",
        "hat" | "widehat" => r"\hat",
        "tilde" | "widetilde" => r"\tilde",
        "dot" => r"\dot",
        "ddot" => r"\ddot",
        "bar" | "macron" => r"\bar",
        "overline" => r"\overline",
        "grave" => r"\grave",
        "acute" => r"\acute",
        "breve" => r"\breve",
        "check" | "caron" => r"\check",
        _ => r"\hat",
    };

    Some(MathIr::Delimited {
        open: format!("{}{{", latex_cmd),
        content: Box::new(build_math_ir_with_env(args[0], options, env)),
        close: "}".to_string(),
    })
}

fn build_color_ir(
    args: &[&SyntaxNode],
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> Option<MathIr> {
    if args.len() < 2 {
        return None;
    }

    let raw_color = get_simple_text(args[0]);
    let normalized_color = normalize_typst_color_expr(&raw_color).unwrap_or(raw_color);

    Some(MathIr::Delimited {
        open: format!(r"{{\color{{{}}}", normalized_color),
        content: Box::new(build_math_ir_with_env(args[1], options, env)),
        close: "}".to_string(),
    })
}

fn build_math_frac(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let children: Vec<&SyntaxNode> = node.children().collect();
    let slash_pos = children
        .iter()
        .position(|child| child.kind() == SyntaxKind::Slash);

    let build_side = |side: &[&SyntaxNode]| -> MathIr {
        let stripped = strip_grouping_parens(side);
        let items: Vec<MathIr> = stripped
            .iter()
            .map(|child| build_math_ir_with_env(child, options, env))
            .collect();
        seq_or_single(items)
    };

    let (numerator, denominator) = if let Some(pos) = slash_pos {
        (
            build_side(&children[..pos]),
            build_side(&children[pos + 1..]),
        )
    } else if children.len() >= 2 {
        let mid = children.len() / 2;
        (build_side(&children[..mid]), build_side(&children[mid..]))
    } else if !children.is_empty() {
        (build_side(&children[..]), MathIr::empty())
    } else {
        (MathIr::empty(), MathIr::empty())
    };

    MathIr::command("\\frac", vec![numerator, denominator])
}

/// When a fraction's numerator or denominator is a single parenthesized
/// group, the parens act as implicit grouping (the `\frac` braces already
/// group the content) and should be absorbed.
///
/// Typst parses `(...)` in fraction operands two ways depending on context:
///   - as a `MathDelimited` whose first/last children are `LeftParen`/`RightParen`, or
///   - as a `Math` block whose first/last children are `LeftParen`/`RightParen`.
///
/// Both shapes are unwrapped here. Only round parens — `[...]`, `{...}`, `|...|`
/// are visible delimiters and stay.
fn strip_grouping_parens<'a>(children: &'a [&'a SyntaxNode]) -> Vec<&'a SyntaxNode> {
    let meaningful: Vec<&SyntaxNode> = children
        .iter()
        .filter(|c| c.kind() != SyntaxKind::Space)
        .copied()
        .collect();
    if meaningful.len() != 1 {
        return children.to_vec();
    }
    let only = meaningful[0];
    if !matches!(only.kind(), SyntaxKind::Math | SyntaxKind::MathDelimited) {
        return children.to_vec();
    }
    let inner: Vec<&SyntaxNode> = only.children().collect();
    if inner.len() >= 2
        && inner.first().map(|c| c.kind()) == Some(SyntaxKind::LeftParen)
        && inner.last().map(|c| c.kind()) == Some(SyntaxKind::RightParen)
    {
        return inner[1..inner.len() - 1].to_vec();
    }
    children.to_vec()
}

fn build_math_root(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let content_children: Vec<&SyntaxNode> = node
        .children()
        .filter(|child| is_content_node(child))
        .collect();

    match content_children.len() {
        0 => MathIr::command("\\sqrt", vec![MathIr::empty()]),
        1 => MathIr::command(
            "\\sqrt",
            vec![build_math_ir_with_env(content_children[0], options, env)],
        ),
        _ => MathIr::Command(MathCommand {
            latex: "\\sqrt".to_string(),
            args: vec![seq_or_single(
                content_children[1..]
                    .iter()
                    .map(|child| build_math_ir_with_env(child, options, env))
                    .collect(),
            )],
            optional_arg: Some(Box::new(build_math_ir_with_env(
                content_children[0],
                options,
                env,
            ))),
            preserve_empty_args: false,
        }),
    }
}

fn build_math_delimited(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let children: Vec<&SyntaxNode> = node
        .children()
        .filter(|child| child.kind() != SyntaxKind::Space)
        .collect();
    if children.is_empty() {
        return MathIr::empty();
    }

    let first = children[0];
    let last = children.last().unwrap_or(&first);

    let first_text = get_node_delimiter_text(first);
    let last_text = get_node_delimiter_text(last);

    if is_delimiter(&first_text) && is_delimiter(&last_text) && children.len() >= 2 {
        return MathIr::Delimited {
            open: format!("\\left{}", get_latex_delimiter(&first_text, true)),
            content: Box::new(seq_or_single(
                children[1..children.len() - 1]
                    .iter()
                    .map(|child| build_math_ir_with_env(child, options, env))
                    .collect(),
            )),
            close: format!("\\right{}", get_latex_delimiter(&last_text, false)),
        };
    }

    seq_or_single(
        children
            .into_iter()
            .map(|child| build_math_ir_with_env(child, options, env))
            .collect(),
    )
}

fn build_generic(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let children: Vec<&SyntaxNode> = node.children().collect();
    if !children.is_empty() {
        let mut items = Vec::new();
        let mut index = 0;

        while index < children.len() {
            let child = children[index];
            if child.kind() == SyntaxKind::Hash && index + 1 < children.len() {
                let next = children[index + 1];
                if next.kind() == SyntaxKind::FuncCall {
                    items.push(build_math_ir_with_env(next, options, env));
                    index += 2;
                    continue;
                }
                if next.kind() == SyntaxKind::Contextual {
                    items.push(build_contextual(next, options, env));
                    index += 2;
                    continue;
                }
                if next.kind() == SyntaxKind::Ident {
                    items.push(MathIr::Ident(next.text().to_string()));
                    index += 2;
                    continue;
                }
            }

            items.push(build_math_ir_with_env(child, options, env));
            index += 1;
        }

        return seq_or_single(items);
    }

    let text = node.text();
    let text_str = text.as_str();
    if text_str.trim().is_empty() {
        return MathIr::empty();
    }
    // A lone brace is a literal one: an `lr` edge group split into its brackets
    // (`lr(a { b })`) leaves one inside the content, where `{` would open a group.
    // Bare `%`, `$`, and literal `#` text also need escaping before reaching LaTeX.
    match text_str {
        "{" => return MathIr::RawLiteral(r"\{".to_string()),
        "}" => return MathIr::RawLiteral(r"\}".to_string()),
        "%" => return MathIr::RawLiteral(r"\%".to_string()),
        "$" => return MathIr::RawLiteral(r"\$".to_string()),
        "#" if node.kind() == SyntaxKind::MathText => return MathIr::RawLiteral(r"\#".to_string()),
        _ => {}
    }

    if node.kind() == SyntaxKind::Ident {
        return MathIr::Ident(text_str.to_string());
    }

    if let Some(tex) = TYPST_TO_TEX.get(text_str) {
        return MathIr::Symbol(with_leading_backslash(
            normalize_package_sensitive_math_tex(tex),
        ));
    }

    MathIr::Ident(convert_unicode_in_text(text_str))
}

fn build_environment(
    args_node: &SyntaxNode,
    name: &str,
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> MathIr {
    if name == "cases" {
        // Every argument is a row, as in Typst: `cases(a, & b)` has two rows,
        // the second with an empty first column.
        let rows = build_args(args_node, options, env);
        return MathIr::Environment(MathEnvironment::Cases { rows });
    }

    let args = FuncArgs::from_args_node(args_node);
    let delimiter = named_expr(&args, "delim").map(|expr| match expr {
        ast::Expr::Str(string) => string.get().to_string(),
        expr => get_simple_text(expr.to_untyped()),
    });
    let actual_name = if let Some(delim) = delimiter.as_deref() {
        match delim {
            "[" => "bmatrix",
            "(" => "pmatrix",
            "{" => "Bmatrix",
            "|" | "bar.v" => "vmatrix",
            "||" | "‖" | "bar.v.double" => "Vmatrix",
            _ => name,
        }
    } else {
        name
    };

    let actual_name = match actual_name {
        "matrix" => "matrix",
        "pmatrix" => "pmatrix",
        "bmatrix" => "bmatrix",
        "Bmatrix" => "Bmatrix",
        "vmatrix" => "vmatrix",
        "Vmatrix" => "Vmatrix",
        _ => name,
    };

    let mut rows: Vec<Vec<MathIr>> = vec![];
    let mut current_row: Vec<MathIr> = vec![];

    for child in args_node.children() {
        match child.kind() {
            SyntaxKind::Named => {}
            SyntaxKind::Semicolon if !current_row.is_empty() => {
                rows.push(std::mem::take(&mut current_row));
            }
            SyntaxKind::Comma
            | SyntaxKind::Space
            | SyntaxKind::LeftParen
            | SyntaxKind::RightParen => {}
            SyntaxKind::Array => {
                for arr_child in child.children() {
                    match arr_child.kind() {
                        SyntaxKind::Comma | SyntaxKind::Space => {}
                        _ if is_content_node(arr_child) => {
                            current_row.push(build_math_ir_with_env(arr_child, options, env))
                        }
                        _ => {}
                    }
                }
            }
            _ if is_content_node(child) => {
                current_row.push(build_math_ir_with_env(child, options, env))
            }
            _ => {}
        }
    }

    if !current_row.is_empty() {
        rows.push(current_row);
    }

    let layout = array_layout(&args);
    MathIr::Environment(MathEnvironment::Matrix {
        name: actual_name.to_string(),
        rows,
        layout,
    })
}

/// The `array` layout for `mat(augment: …, align: …)`, if either needs one.
/// Resolve non-negative literal indices; other layouts remain unsupported.
fn array_layout(args: &FuncArgs) -> Option<ArrayLayout> {
    let align = match named_expr(args, "align") {
        Some(ast::Expr::Ident(name)) if matches!(name.get().as_str(), "left" | "start") => 'l',
        Some(ast::Expr::Ident(name)) if matches!(name.get().as_str(), "right" | "end") => 'r',
        _ => 'c',
    };
    let (mut hlines, mut vlines) = (Vec::new(), Vec::new());
    if let Some(augment) = named_expr(args, "augment") {
        if let ast::Expr::Dict(dict) = augment {
            for item in dict.items() {
                let (key, value) = match item {
                    ast::DictItem::Named(named) => (named.name().get().to_string(), named.expr()),
                    ast::DictItem::Keyed(keyed) => match literal_text(keyed.key()) {
                        Some(key) => (key, keyed.expr()),
                        None => continue,
                    },
                    ast::DictItem::Spread(_) => continue,
                };
                match key.as_str() {
                    "hline" => hlines = augmentation_indices(value),
                    "vline" => vlines = augmentation_indices(value),
                    _ => {}
                }
            }
        } else {
            vlines = augmentation_indices(augment);
        }
    }
    if align == 'c' && hlines.is_empty() && vlines.is_empty() {
        return None;
    }
    Some(ArrayLayout {
        align,
        vlines,
        hlines,
    })
}

fn named_expr<'a>(args: &FuncArgs<'a>, key: &str) -> Option<ast::Expr<'a>> {
    args.named_arg(key)?
        .node
        .cast::<ast::Named>()
        .map(|named| ungroup_expr(named.expr()))
}

fn augmentation_indices(expr: ast::Expr<'_>) -> Vec<usize> {
    match ungroup_expr(expr) {
        ast::Expr::Array(array) => array
            .items()
            .filter_map(|item| match item {
                ast::ArrayItem::Pos(expr) => augmentation_index(expr),
                ast::ArrayItem::Spread(_) => None,
            })
            .collect(),
        expr => augmentation_index(expr).into_iter().collect(),
    }
}

fn augmentation_index(expr: ast::Expr<'_>) -> Option<usize> {
    match ungroup_expr(expr) {
        ast::Expr::Int(index) => usize::try_from(index.get()).ok(),
        ast::Expr::Unary(unary) if unary.op() == ast::UnOp::Pos => augmentation_index(unary.expr()),
        _ => None,
    }
}

fn build_args(args_node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> Vec<MathIr> {
    args_node
        .children()
        .filter(|child| is_content_node(child))
        .map(|child| build_math_ir_with_env(child, options, env))
        .collect()
}

/// Recognize the measured negative skip used by L → T for `\llap`.
fn build_contextual(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    if let Some(body) = llap_body(node) {
        if !env.is_shadowed_context(node) {
            return MathIr::required_command(r"\llap", vec![box_math_arg(body, options, env)]);
        }
        return MathIr::PreservedSource {
            source: env.clean_context_source(node),
            reason: "cannot confirm built-in h/measure bindings in context expression; Typst retained as comments below",
        };
    }
    MathIr::PreservedSource {
        source: env.clean_context_source(node),
        reason: "unsupported context expression; Typst retained as comments below",
    }
}

pub(super) fn llap_body(node: &SyntaxNode) -> Option<&SyntaxNode> {
    let contextual = node.cast::<ast::Contextual>()?;
    let ast::Expr::CodeBlock(block) = ungroup_expr(contextual.body()) else {
        return None;
    };
    let mut exprs = block.body().exprs();
    let skip = named_call(exprs.next()?, "h")?;
    let body = ungroup_expr(exprs.next()?);
    let ast::Expr::Equation(equation) = body else {
        return None;
    };
    if exprs.next().is_some() || equation.block() {
        return None;
    }
    let ast::Expr::Unary(negative) = ungroup_expr(single_positional_arg(skip.args())?) else {
        return None;
    };
    if negative.op() != ast::UnOp::Neg {
        return None;
    }
    let ast::Expr::FieldAccess(width) = ungroup_expr(negative.expr()) else {
        return None;
    };
    if width.field().get() != "width" {
        return None;
    }
    let measure = named_call(width.target(), "measure")?;
    let measured = ungroup_expr(single_positional_arg(measure.args())?);
    // Measuring one expression and displaying another is not an llap.
    same_syntax(measured.to_untyped(), body.to_untyped()).then(|| body.to_untyped())
}

fn named_call<'a>(expr: ast::Expr<'a>, name: &str) -> Option<ast::FuncCall<'a>> {
    let ast::Expr::FuncCall(call) = ungroup_expr(expr) else {
        return None;
    };
    matches!(ungroup_expr(call.callee()), ast::Expr::Ident(ident) if ident.get() == name)
        .then_some(call)
}

fn single_positional_arg(args: ast::Args<'_>) -> Option<ast::Expr<'_>> {
    let mut items = args.items();
    let ast::Arg::Pos(expr) = items.next()? else {
        return None;
    };
    items.next().is_none().then_some(expr)
}

fn same_syntax(left: &SyntaxNode, right: &SyntaxNode) -> bool {
    let mut pending = vec![(left, right)];
    while let Some((left, right)) = pending.pop() {
        if left.kind() != right.kind() {
            return false;
        }
        if let (Some(left), Some(right)) =
            (left.cast::<ast::Equation>(), right.cast::<ast::Equation>())
        {
            if left.block() != right.block() {
                return false;
            }
        }
        let mut lhs = left.children().filter(|node| !node.kind().is_trivia());
        let mut rhs = right.children().filter(|node| !node.kind().is_trivia());
        if left.children().next().is_none() && left.text() != right.text() {
            return false;
        }
        loop {
            match (lhs.next(), rhs.next()) {
                (Some(left), Some(right)) => pending.push((left, right)),
                (None, None) => break,
                _ => return false,
            }
        }
    }
    true
}

fn box_math_arg(math: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    MathIr::Delimited {
        open: "$".to_string(),
        content: Box::new(build_math_ir_with_env(math, options, env)),
        close: "$".to_string(),
    }
}

/// `#text[…]`, `#emph[…]` or `#strong[…]` in math as a LaTeX text command.
fn build_text_markup_ir(
    latex: &str,
    args_node: &SyntaxNode,
    options: &T2LOptions,
    math_args: bool,
    env: &MathBuildEnv,
) -> MathIr {
    MathIr::required_command(
        latex,
        vec![text_markup_arg(args_node, options, math_args, env)],
    )
}

/// The body of a text call in math as a LaTeX text argument. It stays text,
/// with each equation in it as `$…$`: `#text[if $x > 0$]` → `if $x > 0$`.
/// Named arguments are dropped.
fn text_markup_arg(
    args_node: &SyntaxNode,
    options: &T2LOptions,
    math_args: bool,
    env: &MathBuildEnv,
) -> MathIr {
    let mut parts = Vec::new();
    for child in args_node.children() {
        match child.kind() {
            SyntaxKind::ContentBlock => {
                if let Some(markup) = child_of_kind(child, SyntaxKind::Markup) {
                    push_markup_as_text(markup, &mut parts, options, env);
                }
            }
            SyntaxKind::Str => parts.push(MathIr::RawLiteral(escape_latex_math_text(&str_value(
                child,
            )))),
            SyntaxKind::MathText => {
                parts.push(MathIr::RawLiteral(escape_latex_math_text(child.text())))
            }
            _ if (math_args || child.kind() == SyntaxKind::Equation)
                && !child.kind().is_trivia()
                && is_content_node(child)
                && child.kind() != SyntaxKind::Named =>
            {
                parts.push(box_math_arg(child, options, env));
            }
            _ if !child.kind().is_trivia()
                && is_content_node(child)
                && child.kind() != SyntaxKind::Named =>
            {
                let converted = child
                    .cast::<ast::Expr>()
                    .and_then(|expr| push_literal_text_expr(expr, &mut parts, options, env));
                if converted.is_none() {
                    parts.push(MathIr::PreservedSource {
                        source: child.clone().into_text().to_string(),
                        reason:
                            "could not convert a text argument; Typst retained as comments below",
                    });
                }
            }
            _ => {}
        }
    }
    MathIr::Seq(parts)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LiteralTextKind {
    Str,
    Content,
}

fn sanitize_math_text_newlines(text: &str) -> String {
    if !text.contains(['\n', '\r']) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for line in normalized.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        if !out.is_empty() && !out.ends_with(' ') && !line.starts_with(' ') {
            out.push(' ');
        }
        out.push_str(line);
    }
    out
}

fn escape_latex_math_text(text: &str) -> String {
    escape_latex_text(&sanitize_math_text_newlines(text))
}

fn build_raw_math_text(node: &SyntaxNode) -> MathIr {
    let raw_text = node
        .cast::<ast::Raw>()
        .map(|raw| {
            raw.lines()
                .map(|line| line.get().as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| SyntaxNode::clone(node).into_text().to_string());
    MathIr::required_command(
        r"\texttt",
        vec![MathIr::RawLiteral(escape_latex_math_text(&raw_text))],
    )
}

fn push_literal_text_expr(
    expr: ast::Expr<'_>,
    parts: &mut Vec<MathIr>,
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> Option<LiteralTextKind> {
    match ungroup_expr(expr) {
        ast::Expr::Str(string) => {
            parts.push(MathIr::RawLiteral(escape_latex_math_text(
                string.get().as_str(),
            )));
            Some(LiteralTextKind::Str)
        }
        ast::Expr::Raw(raw) => {
            parts.push(build_raw_math_text(raw.to_untyped()));
            Some(LiteralTextKind::Content)
        }
        ast::Expr::ContentBlock(block) => {
            push_markup_as_text(block.body().to_untyped(), parts, options, env);
            Some(LiteralTextKind::Content)
        }
        ast::Expr::Equation(equation) => {
            parts.push(box_math_arg(equation.body().to_untyped(), options, env));
            Some(LiteralTextKind::Content)
        }
        ast::Expr::Binary(binary) if binary.op() == ast::BinOp::Add => {
            let mut lhs_parts = Vec::new();
            let mut rhs_parts = Vec::new();
            let lhs_kind = push_literal_text_expr(binary.lhs(), &mut lhs_parts, options, env)?;
            let rhs_kind = push_literal_text_expr(binary.rhs(), &mut rhs_parts, options, env)?;
            if lhs_kind == rhs_kind {
                parts.extend(lhs_parts);
                parts.extend(rhs_parts);
                Some(lhs_kind)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn literal_text(expr: ast::Expr<'_>) -> Option<String> {
    match ungroup_expr(expr) {
        ast::Expr::Str(string) => Some(string.get().to_string()),
        ast::Expr::Binary(binary) if binary.op() == ast::BinOp::Add => {
            Some(literal_text(binary.lhs())? + &literal_text(binary.rhs())?)
        }
        _ => None,
    }
}

fn is_visible_markup_child(node: &SyntaxNode) -> bool {
    !matches!(
        node.kind(),
        SyntaxKind::Parbreak
            | SyntaxKind::Space
            | SyntaxKind::LineComment
            | SyntaxKind::BlockComment
    )
}

/// Markup as the pieces of a LaTeX text argument.
fn push_markup_as_text(
    markup: &SyntaxNode,
    parts: &mut Vec<MathIr>,
    options: &T2LOptions,
    env: &MathBuildEnv,
) {
    let children: Vec<&SyntaxNode> = markup.children().collect();
    for (idx, child) in children.iter().enumerate() {
        match child.kind() {
            SyntaxKind::Equation => {
                if let Some(math) = child_of_kind(child, SyntaxKind::Math) {
                    parts.push(box_math_arg(math, options, env));
                }
            }
            SyntaxKind::Strong | SyntaxKind::Emph => {
                let latex = if child.kind() == SyntaxKind::Strong {
                    r"\textbf"
                } else {
                    r"\textit"
                };
                let mut inner = Vec::new();
                if let Some(body) = child_of_kind(child, SyntaxKind::Markup) {
                    push_markup_as_text(body, &mut inner, options, env);
                }
                parts.push(MathIr::required_command(latex, vec![MathIr::Seq(inner)]));
            }
            SyntaxKind::Parbreak => {
                let has_before = children[..idx].iter().any(|c| is_visible_markup_child(c));
                let has_after = children[idx + 1..]
                    .iter()
                    .any(|c| is_visible_markup_child(c));
                if has_before && has_after {
                    parts.push(MathIr::RawLiteral(" ".to_string()));
                }
            }
            SyntaxKind::Linebreak => parts.push(MathIr::RawLiteral(" ".to_string())),
            SyntaxKind::Space => {
                if child.text().contains(['\n', '\r']) {
                    parts.push(MathIr::RawLiteral(" ".to_string()));
                } else {
                    parts.push(MathIr::RawLiteral(child.text().to_string()));
                }
            }
            SyntaxKind::LineComment | SyntaxKind::BlockComment => {}
            SyntaxKind::Escape => {
                if let Some(ch) = Escape::from_untyped(child).map(|escape| escape.get()) {
                    parts.push(MathIr::RawLiteral(escape_latex_text(&ch.to_string())));
                }
            }
            SyntaxKind::Raw => parts.push(build_raw_math_text(child)),
            _ => parts.push(MathIr::RawLiteral(escape_latex_math_text(
                &SyntaxNode::clone(child).into_text(),
            ))),
        }
    }
}

/// A content block in math, such as the body of `#box[$x$]`: the equations
/// in it become math and any other markup becomes `\text{…}`.
fn build_content_block(node: &SyntaxNode, options: &T2LOptions, env: &MathBuildEnv) -> MathIr {
    let Some(markup) = child_of_kind(node, SyntaxKind::Markup) else {
        return MathIr::empty();
    };
    // Text runs keep their spaces inside `\text{…}`, where LaTeX sets them.
    fn flush(text: &mut String, items: &mut Vec<MathIr>) {
        if text.trim().is_empty() {
            if !text.is_empty() {
                items.push(MathIr::Spacing(MathSpacing::Soft));
            }
        } else {
            items.push(MathIr::command(
                "\\text",
                vec![MathIr::RawLiteral(escape_latex_math_text(text))],
            ));
        }
        text.clear();
    }
    let mut items = Vec::new();
    let mut text = String::new();
    for child in markup.children() {
        match child.kind() {
            SyntaxKind::Equation => {
                flush(&mut text, &mut items);
                if let Some(math) = child_of_kind(child, SyntaxKind::Math) {
                    items.push(build_math_ir_with_env(math, options, env));
                }
            }
            SyntaxKind::Parbreak | SyntaxKind::Linebreak => text.push(' '),
            SyntaxKind::Space => {
                if child.text().contains(['\n', '\r']) {
                    text.push(' ');
                } else {
                    text.push_str(child.text().as_str());
                }
            }
            SyntaxKind::LineComment | SyntaxKind::BlockComment => {}
            SyntaxKind::Escape => {
                if let Some(ch) = Escape::from_untyped(child).map(|escape| escape.get()) {
                    text.push(ch);
                }
            }
            SyntaxKind::Raw => {
                flush(&mut text, &mut items);
                items.push(build_raw_math_text(child));
            }
            _ => text.push_str(&SyntaxNode::clone(child).into_text()),
        }
    }
    flush(&mut text, &mut items);
    seq_or_single(items)
}

/// The positional arguments of a call, for a LaTeX command whose arguments are
/// all positional. A named argument (`stroke: 0.5pt`) would otherwise become
/// one more brace group.
fn build_positional_args(
    args_node: &SyntaxNode,
    options: &T2LOptions,
    env: &MathBuildEnv,
) -> Vec<MathIr> {
    args_node
        .children()
        .filter(|child| is_content_node(child) && child.kind() != SyntaxKind::Named)
        .map(|child| build_math_ir_with_env(child, options, env))
        .collect()
}

/// `box` in math. L → T writes `\boxed` and `\fbox` as a box with a stroke and
/// `\rule{w}{h}` as an empty box with a fill. A box with neither draws only
/// its body, and an empty one is a space of its width.
fn build_box_ir(
    args_node: &SyntaxNode,
    options: &T2LOptions,
    math_args: bool,
    env: &MathBuildEnv,
) -> MathIr {
    let mut named = Vec::new();
    let mut body_nodes = Vec::new();
    for child in args_node.children() {
        if child.kind() == SyntaxKind::Named {
            if let Some((name, value)) = get_simple_text(child).split_once(':') {
                named.push((name.trim().to_string(), value.trim().to_string()));
            }
        } else if is_content_node(child) {
            body_nodes.push(child);
        }
    }
    let get = |key: &str| {
        named
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    // A positive baseline lowers the box; `\raisebox` and `\rule` raise it.
    let raise = get("baseline")
        .and_then(extract_length_value)
        .map(|baseline| match baseline.strip_prefix('-') {
            Some(up) => up.to_string(),
            None => format!("-{baseline}"),
        });
    // A framed box of text, `#box(stroke: …)[6]`, is `\fbox{6}`; one that
    // holds only an equation is `\boxed`.
    let text_body = args_node.children().any(|child| {
        child.kind() == SyntaxKind::ContentBlock
            && child_of_kind(child, SyntaxKind::Markup).is_some_and(|markup| {
                let parts: Vec<_> = markup
                    .children()
                    .filter(|c| !matches!(c.kind(), SyntaxKind::Space))
                    .collect();
                !matches!(parts.as_slice(), [only] if only.kind() == SyntaxKind::Equation)
            })
    });
    if text_body && get("stroke").is_some_and(|stroke| stroke != "none") {
        return build_text_markup_ir(r"\fbox", args_node, options, false, env);
    }
    // L → T writes `\raisebox{h}{…}` as a box with baseline `-h`.
    if let (Some(raise), None, None) = (&raise, get("stroke"), get("width")) {
        if args_node
            .children()
            .any(|c| c.kind() == SyntaxKind::ContentBlock)
        {
            let text = text_markup_arg(args_node, options, false, env);
            return MathIr::required_command(
                r"\raisebox",
                vec![MathIr::RawLiteral(raise.clone()), text],
            );
        }
    }
    if !body_nodes.is_empty() {
        let build_body = || {
            seq_or_single(
                body_nodes
                    .iter()
                    .map(|child| build_math_ir_with_env(child, options, env))
                    .collect(),
            )
        };
        return match (get("stroke"), get("width")) {
            (Some(stroke), _) if stroke != "none" => MathIr::command(r"\boxed", vec![build_body()]),
            // L → T writes `\rlap{x}` as a box of zero width.
            (_, Some("0pt")) => MathIr::required_command(
                r"\rlap",
                vec![text_markup_arg(args_node, options, math_args, env)],
            ),
            _ => build_body(),
        };
    }
    let width = get("width").and_then(extract_length_value);
    let height = get("height").and_then(extract_length_value);
    match (width, height, get("fill")) {
        (Some(width), Some(height), Some(fill)) if fill != "none" => MathIr::Command(MathCommand {
            latex: r"\rule".to_string(),
            args: vec![MathIr::RawLiteral(width), MathIr::RawLiteral(height)],
            optional_arg: raise.map(|raise| Box::new(MathIr::RawLiteral(raise))),
            preserve_empty_args: false,
        }),
        (Some(width), _, _) => MathIr::command(r"\hspace", vec![MathIr::RawLiteral(width)]),
        _ => MathIr::empty(),
    }
}

/// Collect diagnostics before nested fragments are rendered.
pub fn collect_math_warnings(ir: &MathIr, warnings: &mut Vec<String>) {
    let mut visit = |item: &MathIr| collect_math_warnings(item, warnings);
    match ir {
        MathIr::Ident(name) => {
            if name.len() > 1 && name.chars().all(|ch| ch.is_ascii_alphabetic() || ch == '.') {
                warnings.push(format!(
                    "unknown math symbol or variable `{name}` was copied as letters"
                ));
            }
        }
        MathIr::Seq(items) => items.iter().for_each(visit),
        MathIr::Delimited { content, .. }
        | MathIr::Limits(content)
        | MathIr::Style { content, .. } => visit(content),
        MathIr::Apply { callee, args } => {
            visit(callee);
            args.iter().for_each(visit);
        }
        MathIr::Attachment {
            base,
            top,
            bottom,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        } => {
            visit(base);
            [top, bottom, top_left, top_right, bottom_left, bottom_right]
                .into_iter()
                .flatten()
                .for_each(|item| visit(item));
        }
        MathIr::Script { base, sub, sup, .. } => {
            visit(base);
            [sub, sup]
                .into_iter()
                .flatten()
                .for_each(|item| visit(item));
        }
        MathIr::Command(command) => {
            command.args.iter().for_each(&mut visit);
            if let Some(arg) = &command.optional_arg {
                visit(arg);
            }
        }
        MathIr::Environment(MathEnvironment::Matrix { rows, .. }) => {
            rows.iter().flatten().for_each(visit)
        }
        MathIr::Environment(MathEnvironment::Cases { rows }) => rows.iter().for_each(visit),
        MathIr::PreservedSource { reason, .. } => warnings.push((*reason).to_string()),
        MathIr::Linebreak
        | MathIr::Symbol(_)
        | MathIr::Number(_)
        | MathIr::Operator(_)
        | MathIr::Punctuation(_)
        | MathIr::Spacing(_)
        | MathIr::RawLiteral(_) => {}
    }
}

pub(super) fn ungroup_expr(mut expr: ast::Expr<'_>) -> ast::Expr<'_> {
    while let ast::Expr::Parenthesized(group) = expr {
        expr = group.expr();
    }
    expr
}

/// The first child of `node` of the given kind: the `Math` of an `Equation`, or
/// the `Markup` of a `ContentBlock`.
fn child_of_kind(node: &SyntaxNode, kind: SyntaxKind) -> Option<&SyntaxNode> {
    node.children().find(|child| child.kind() == kind)
}

/// The value of a string literal, with its escapes (`\"`) resolved.
fn str_value(node: &SyntaxNode) -> String {
    typst_syntax::ast::Str::from_untyped(node)
        .map(|string| string.get().to_string())
        .unwrap_or_default()
}

fn seq_or_single(items: Vec<MathIr>) -> MathIr {
    if items.is_empty() {
        MathIr::empty()
    } else if items.len() == 1 {
        items.into_iter().next().unwrap_or_else(MathIr::empty)
    } else {
        MathIr::Seq(items)
    }
}

/// `TYPST_TO_TEX` values are a command name without its backslash (`alpha`), full
/// LaTeX (`\mathbb{E}`), or one literal character (`|`, `~`, `A` for `Alpha`). A
/// literal character must not gain a backslash: `\|` is a double bar and `\~` an accent.
fn with_leading_backslash(tex: &str) -> String {
    if tex.starts_with('\\') || tex.chars().count() == 1 {
        tex.to_string()
    } else {
        format!("\\{}", tex)
    }
}

fn get_math_func_name(node: &SyntaxNode) -> String {
    match node.kind() {
        SyntaxKind::FieldAccess => collect_field_access_text(node),
        _ => node.text().to_string(),
    }
}

fn is_delimiter(text: &str) -> bool {
    text == "." || DELIMITER_MAP.contains_key(text)
}

fn get_latex_delimiter(text: &str, is_left: bool) -> &'static str {
    if text == "." {
        return ".";
    }

    DELIMITER_MAP
        .get(text)
        .copied()
        .unwrap_or(if is_left { "(" } else { ")" })
}

fn get_node_delimiter_text(node: &SyntaxNode) -> String {
    match node.kind() {
        SyntaxKind::FieldAccess => collect_field_access_text(node),
        SyntaxKind::LeftParen => "(".to_string(),
        SyntaxKind::RightParen => ")".to_string(),
        SyntaxKind::LeftBracket => "[".to_string(),
        SyntaxKind::RightBracket => "]".to_string(),
        SyntaxKind::LeftBrace => "{".to_string(),
        SyntaxKind::RightBrace => "}".to_string(),
        _ => node.text().to_string(),
    }
}

fn collect_field_access_text(node: &SyntaxNode) -> String {
    fn collect_recursive(node: &SyntaxNode, parts: &mut Vec<String>) {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::FieldAccess => collect_recursive(child, parts),
                SyntaxKind::MathIdent | SyntaxKind::Ident => parts.push(child.text().to_string()),
                SyntaxKind::Dot => {}
                _ => {
                    let text = child.text().to_string();
                    if !text.is_empty() && text != "." {
                        parts.push(text);
                    }
                }
            }
        }
    }

    let mut parts = Vec::new();
    collect_recursive(node, &mut parts);
    parts.join(".")
}

fn convert_unicode_in_text(text: &str) -> String {
    let mut result = String::new();

    for (index, ch) in text.chars().enumerate() {
        if let Some(latex) = UNICODE_TO_LATEX.get(&ch) {
            result.push_str(latex);
            if text
                .chars()
                .nth(index + 1)
                .is_some_and(|next| next.is_alphanumeric())
            {
                result.push(' ');
            }
        } else {
            result.push(ch);
        }
    }

    result
}
