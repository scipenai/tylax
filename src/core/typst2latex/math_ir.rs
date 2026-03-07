use super::context::T2LOptions;
use super::math::render_lr_to_latex_string;
use super::utils::{
    get_simple_text, is_content_node, normalize_typst_color_expr, FuncArgs, UNICODE_TO_LATEX,
};
use crate::data::maps::{DELIMITER_MAP, TYPST_TO_TEX};
use crate::data::typst_compat::{MathHandler, TYPST_MATH_HANDLERS};
use typst_syntax::ast::{AstNode, Escape};
use typst_syntax::{SyntaxKind, SyntaxNode};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathCaseRow {
    pub value: MathIr,
    pub condition: Option<MathIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathStyleMode {
    Display,
    Inline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathEnvironment {
    Matrix {
        name: String,
        rows: Vec<Vec<MathIr>>,
    },
    Cases {
        rows: Vec<MathCaseRow>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathIr {
    Seq(Vec<MathIr>),
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
    },
    Command(MathCommand),
    Environment(MathEnvironment),
    RawLiteral(String),
}

impl MathIr {
    pub fn empty() -> Self {
        MathIr::Seq(vec![])
    }

    pub fn is_empty(&self) -> bool {
        match self {
            MathIr::Seq(items) => items.is_empty(),
            MathIr::RawLiteral(text) => text.is_empty(),
            _ => false,
        }
    }
}

pub fn build_math_ir(node: &SyntaxNode, options: &T2LOptions) -> MathIr {
    match node.kind() {
        SyntaxKind::MathIdent => build_math_ident(node),
        SyntaxKind::FieldAccess => build_field_access(node),
        SyntaxKind::Space => MathIr::Spacing(MathSpacing::Soft),
        SyntaxKind::Escape => build_escape(node),
        SyntaxKind::Linebreak => MathIr::RawLiteral(" \\\\\n".to_string()),
        SyntaxKind::MathAttach => build_math_attach(node, options),
        SyntaxKind::FuncCall => build_func_call(node, options),
        SyntaxKind::MathFrac => build_math_frac(node, options),
        SyntaxKind::MathRoot => build_math_root(node, options),
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
        SyntaxKind::Str => MathIr::Command(MathCommand {
            latex: "\\text".to_string(),
            args: vec![MathIr::RawLiteral(
                node.text().trim_matches('"').to_string(),
            )],
            optional_arg: None,
        }),
        SyntaxKind::MathDelimited => build_math_delimited(node, options),
        SyntaxKind::MathPrimes => {
            let count = node.text().chars().filter(|&c| c == '\'').count();
            MathIr::RawLiteral("'".repeat(count))
        }
        _ => build_generic(node, options),
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
        MathIr::Script { base, sub, sup } => MathIr::Script {
            base: Box::new(normalize_math_ir(*base)),
            sub: sub.map(|item| Box::new(normalize_math_ir(*item))),
            sup: sup.map(|item| Box::new(normalize_math_ir(*item))),
        },
        MathIr::Command(mut command) => {
            command.args = command
                .args
                .into_iter()
                .map(normalize_math_ir)
                .filter(|arg| !arg.is_empty())
                .collect();
            command.optional_arg = command
                .optional_arg
                .map(|arg| Box::new(normalize_math_ir(*arg)));
            MathIr::Command(command)
        }
        MathIr::Environment(environment) => match environment {
            MathEnvironment::Matrix { name, rows } => {
                MathIr::Environment(MathEnvironment::Matrix {
                    name,
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
                    .map(|row| MathCaseRow {
                        value: normalize_math_ir(row.value),
                        condition: row.condition.map(normalize_math_ir),
                    })
                    .filter(|row| !row.value.is_empty())
                    .collect(),
            }),
        },
        other => other,
    }
}

fn build_math_ident(node: &SyntaxNode) -> MathIr {
    let text = node.text();
    let text_str = text.as_str();

    if matches!(text_str, "zws" | "zwsp" | "nbsp" | "wj" | "shy") {
        return MathIr::empty();
    }

    match text_str {
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
        return MathIr::Symbol(with_leading_backslash(tex));
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
        return MathIr::Symbol(with_leading_backslash(tex));
    }

    if full_text_str == "square.stroked" || full_text_str == "square.filled" {
        return MathIr::Symbol("\\blacksquare".to_string());
    }

    if full_text_str == "bar.v.double" || full_text_str.ends_with(".v.double") {
        return MathIr::Symbol("\\|".to_string());
    }

    if full_text_str == "bar.v" {
        return MathIr::RawLiteral("|".to_string());
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
        ch => {
            if let Some(latex) = UNICODE_TO_LATEX.get(&ch) {
                MathIr::Symbol((*latex).to_string())
            } else {
                MathIr::RawLiteral(ch.to_string())
            }
        }
    }
}

fn build_math_attach(node: &SyntaxNode, options: &T2LOptions) -> MathIr {
    let children: Vec<&SyntaxNode> = node
        .children()
        .filter(|child| child.kind() != SyntaxKind::Space)
        .collect();

    let base_idx = children.iter().position(|child| {
        child.kind() != SyntaxKind::Hat && child.kind() != SyntaxKind::Underscore
    });

    let Some(base_idx) = base_idx else {
        return MathIr::empty();
    };

    let base = Box::new(build_math_ir(children[base_idx], options));
    let mut sub = None;
    let mut sup = None;

    let mut index = 0;
    while index < children.len() {
        if index == base_idx {
            index += 1;
            continue;
        }

        match children[index].kind() {
            SyntaxKind::Hat => {
                if index + 1 < children.len()
                    && children[index + 1].kind() != SyntaxKind::Hat
                    && children[index + 1].kind() != SyntaxKind::Underscore
                {
                    sup = Some(Box::new(build_math_ir(children[index + 1], options)));
                    index += 2;
                    continue;
                }
            }
            SyntaxKind::Underscore => {
                if index + 1 < children.len()
                    && children[index + 1].kind() != SyntaxKind::Hat
                    && children[index + 1].kind() != SyntaxKind::Underscore
                {
                    sub = Some(Box::new(build_math_ir(children[index + 1], options)));
                    index += 2;
                    continue;
                }
            }
            _ => {}
        }

        index += 1;
    }

    MathIr::Script { base, sub, sup }
}

fn build_func_call(node: &SyntaxNode, options: &T2LOptions) -> MathIr {
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

    if let Some(handler) = TYPST_MATH_HANDLERS.get(func_str.as_str()) {
        match handler {
            MathHandler::Command { latex_cmd } => {
                let args = build_args(args_node, options);
                return MathIr::Command(MathCommand {
                    latex: (*latex_cmd).to_string(),
                    args,
                    optional_arg: None,
                });
            }
            MathHandler::CommandWithOpt { latex_cmd } => {
                let args = build_args(args_node, options);
                if args.is_empty() {
                    return MathIr::Command(MathCommand {
                        latex: (*latex_cmd).to_string(),
                        args: vec![],
                        optional_arg: None,
                    });
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
                });
            }
            MathHandler::Delimiters { open, close } => {
                return MathIr::Delimited {
                    open: (*open).to_string(),
                    content: Box::new(seq_or_single(build_args(args_node, options))),
                    close: (*close).to_string(),
                };
            }
            MathHandler::BigOperator { latex_cmd } => {
                return MathIr::Apply {
                    callee: Box::new(MathIr::Symbol((*latex_cmd).to_string())),
                    args: build_args(args_node, options),
                };
            }
            MathHandler::Environment { name } => {
                return build_environment(args_node, name, options);
            }
            MathHandler::Special => {
                if let Some(ir) = build_special_func_call(func_str.as_str(), args_node, options) {
                    return ir;
                }
            }
        }
    }

    build_callable_ir(&func_str, build_args(args_node, options))
}

fn build_callable_ir(func_str: &str, args: Vec<MathIr>) -> MathIr {
    let callee = if let Some(tex) = TYPST_TO_TEX.get(func_str) {
        MathIr::Symbol(with_leading_backslash(tex))
    } else {
        MathIr::Command(MathCommand {
            latex: r"\operatorname".to_string(),
            args: vec![MathIr::RawLiteral(func_str.to_string())],
            optional_arg: None,
        })
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
) -> Option<MathIr> {
    let args: Vec<&SyntaxNode> = args_node
        .children()
        .filter(|child| is_content_node(child))
        .collect();

    match func_str {
        "math.vec" => Some(build_math_vec_ir(args_node, options)),
        "lr" => Some(MathIr::RawLiteral(render_lr_to_latex_string(
            args_node, options,
        ))),
        "attach" => Some(build_attach_ir(args_node, options)),
        "scripts" => Some(MathIr::Style {
            mode: MathStyleMode::Display,
            content: Box::new(seq_or_single(
                args.iter().map(|arg| build_math_ir(arg, options)).collect(),
            )),
        }),
        "primes" => Some(build_primes_ir(args_node)),
        "stretch" => Some(build_stretch_ir(args_node, options)),
        "mid" => Some(MathIr::Symbol(r"\mid".to_string())),
        "circle" => args.first().map(|arg| MathIr::Delimited {
            open: r"\mathring{".to_string(),
            content: Box::new(build_math_ir(arg, options)),
            close: "}".to_string(),
        }),
        "divergence" => Some(MathIr::Seq(vec![
            MathIr::Symbol(r"\nabla".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            MathIr::Symbol(r"\cdot".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            seq_or_single(args.iter().map(|arg| build_math_ir(arg, options)).collect()),
        ])),
        "curl" => Some(MathIr::Seq(vec![
            MathIr::Symbol(r"\nabla".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            MathIr::Symbol(r"\times".to_string()),
            MathIr::Spacing(MathSpacing::Soft),
            seq_or_single(args.iter().map(|arg| build_math_ir(arg, options)).collect()),
        ])),
        "limits" => args
            .first()
            .map(|arg| MathIr::Limits(Box::new(build_math_ir(arg, options)))),
        "display" => Some(MathIr::Style {
            mode: MathStyleMode::Display,
            content: Box::new(seq_or_single(
                args.iter().map(|arg| build_math_ir(arg, options)).collect(),
            )),
        }),
        "inline" => Some(MathIr::Style {
            mode: MathStyleMode::Inline,
            content: Box::new(seq_or_single(
                args.iter().map(|arg| build_math_ir(arg, options)).collect(),
            )),
        }),
        "op" => {
            let name = args
                .first()
                .map(|arg| get_simple_text(arg).trim_matches('"').to_string())
                .unwrap_or_default();
            Some(MathIr::Command(MathCommand {
                latex: r"\operatorname".to_string(),
                args: vec![MathIr::RawLiteral(name)],
                optional_arg: None,
            }))
        }
        "class" => build_class_ir(&args, options),
        "set" | "Set" => Some(MathIr::Delimited {
            open: r"\left\{".to_string(),
            content: Box::new(seq_or_single(
                args.iter().map(|arg| build_math_ir(arg, options)).collect(),
            )),
            close: r"\right\}".to_string(),
        }),
        "arrow" => args.first().map(|arg| MathIr::Delimited {
            open: r"\overrightarrow{".to_string(),
            content: Box::new(build_math_ir(arg, options)),
            close: "}".to_string(),
        }),
        "accent" => build_accent_ir(&args, options),
        "color" => build_color_ir(&args, options),
        _ => None,
    }
}

fn build_math_vec_ir(args_node: &SyntaxNode, options: &T2LOptions) -> MathIr {
    let rows = args_node
        .children()
        .filter(|child| is_content_node(child))
        .map(|child| vec![build_math_ir(child, options)])
        .collect();

    MathIr::Environment(MathEnvironment::Matrix {
        name: "pmatrix".to_string(),
        rows,
    })
}

fn build_attach_ir(args_node: &SyntaxNode, options: &T2LOptions) -> MathIr {
    let args = FuncArgs::from_args_node(args_node);
    let base = args
        .positional_node(0)
        .map(|node| build_math_ir(node, options))
        .unwrap_or_else(MathIr::empty);

    let named = |key: &str| {
        args.named_node(key)
            .map(|node| Box::new(build_math_ir(node, options)))
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

fn build_stretch_ir(args_node: &SyntaxNode, options: &T2LOptions) -> MathIr {
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
        _ => build_math_ir(first_arg, options),
    }
}

fn build_class_ir(args: &[&SyntaxNode], options: &T2LOptions) -> Option<MathIr> {
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

    let content = build_math_ir(args[1], options);
    Some(match latex_cmd {
        Some(cmd) => MathIr::Delimited {
            open: format!("{}{{", cmd),
            content: Box::new(content),
            close: "}".to_string(),
        },
        None => content,
    })
}

fn build_accent_ir(args: &[&SyntaxNode], options: &T2LOptions) -> Option<MathIr> {
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
        content: Box::new(build_math_ir(args[0], options)),
        close: "}".to_string(),
    })
}

fn build_color_ir(args: &[&SyntaxNode], options: &T2LOptions) -> Option<MathIr> {
    if args.len() < 2 {
        return None;
    }

    let raw_color = get_simple_text(args[0]);
    let normalized_color = normalize_typst_color_expr(&raw_color).unwrap_or(raw_color);

    Some(MathIr::Delimited {
        open: format!(r"{{\color{{{}}}", normalized_color),
        content: Box::new(build_math_ir(args[1], options)),
        close: "}".to_string(),
    })
}

fn build_math_frac(node: &SyntaxNode, options: &T2LOptions) -> MathIr {
    let children: Vec<&SyntaxNode> = node.children().collect();
    let slash_pos = children
        .iter()
        .position(|child| child.kind() == SyntaxKind::Slash);

    let (numerator, denominator) = if let Some(pos) = slash_pos {
        (
            seq_or_single(
                children[..pos]
                    .iter()
                    .map(|child| build_math_ir(child, options))
                    .collect(),
            ),
            seq_or_single(
                children[pos + 1..]
                    .iter()
                    .map(|child| build_math_ir(child, options))
                    .collect(),
            ),
        )
    } else if children.len() >= 2 {
        let mid = children.len() / 2;
        (
            seq_or_single(
                children[..mid]
                    .iter()
                    .map(|child| build_math_ir(child, options))
                    .collect(),
            ),
            seq_or_single(
                children[mid..]
                    .iter()
                    .map(|child| build_math_ir(child, options))
                    .collect(),
            ),
        )
    } else if let Some(child) = children.first() {
        (build_math_ir(child, options), MathIr::empty())
    } else {
        (MathIr::empty(), MathIr::empty())
    };

    MathIr::Command(MathCommand {
        latex: "\\frac".to_string(),
        args: vec![numerator, denominator],
        optional_arg: None,
    })
}

fn build_math_root(node: &SyntaxNode, options: &T2LOptions) -> MathIr {
    let content_children: Vec<&SyntaxNode> = node
        .children()
        .filter(|child| is_content_node(child))
        .collect();

    match content_children.len() {
        0 => MathIr::Command(MathCommand {
            latex: "\\sqrt".to_string(),
            args: vec![MathIr::empty()],
            optional_arg: None,
        }),
        1 => MathIr::Command(MathCommand {
            latex: "\\sqrt".to_string(),
            args: vec![build_math_ir(content_children[0], options)],
            optional_arg: None,
        }),
        _ => MathIr::Command(MathCommand {
            latex: "\\sqrt".to_string(),
            args: vec![seq_or_single(
                content_children[1..]
                    .iter()
                    .map(|child| build_math_ir(child, options))
                    .collect(),
            )],
            optional_arg: Some(Box::new(build_math_ir(content_children[0], options))),
        }),
    }
}

fn build_math_delimited(node: &SyntaxNode, options: &T2LOptions) -> MathIr {
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
                    .map(|child| build_math_ir(child, options))
                    .collect(),
            )),
            close: format!("\\right{}", get_latex_delimiter(&last_text, false)),
        };
    }

    seq_or_single(
        children
            .into_iter()
            .map(|child| build_math_ir(child, options))
            .collect(),
    )
}

fn build_generic(node: &SyntaxNode, options: &T2LOptions) -> MathIr {
    let children: Vec<&SyntaxNode> = node.children().collect();
    if !children.is_empty() {
        return seq_or_single(
            children
                .into_iter()
                .map(|child| build_math_ir(child, options))
                .collect(),
        );
    }

    let text = node.text();
    let text_str = text.as_str();
    if matches!(text_str, "zws" | "zwsp" | "nbsp" | "wj" | "shy") || text_str.trim().is_empty() {
        return MathIr::empty();
    }

    if let Some(tex) = TYPST_TO_TEX.get(text_str) {
        return MathIr::Symbol(with_leading_backslash(tex));
    }

    MathIr::Ident(convert_unicode_in_text(text_str))
}

fn build_environment(args_node: &SyntaxNode, name: &str, options: &T2LOptions) -> MathIr {
    if name == "cases" {
        let items: Vec<MathIr> = args_node
            .children()
            .filter(|child| is_content_node(child))
            .map(|child| build_math_ir(child, options))
            .collect();

        let mut rows = Vec::new();
        let mut index = 0;
        while index < items.len() {
            let value = items[index].clone();
            let mut condition = None;
            if index + 1 < items.len() && is_cases_condition(&items[index + 1]) {
                condition = Some(strip_cases_condition_prefix(items[index + 1].clone()));
                index += 2;
            } else {
                index += 1;
            }
            rows.push(MathCaseRow { value, condition });
        }

        return MathIr::Environment(MathEnvironment::Cases { rows });
    }

    let args = FuncArgs::from_args_node(args_node);
    let actual_name = if let Some(delim) = args
        .named_text("delim")
        .map(|value| value.trim().trim_matches('"').trim_matches('\''))
    {
        match delim {
            "[" => "bmatrix",
            "(" => "pmatrix",
            "{" => "Bmatrix",
            "|" => "vmatrix",
            "||" => "Vmatrix",
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
            SyntaxKind::Semicolon => {
                if !current_row.is_empty() {
                    rows.push(std::mem::take(&mut current_row));
                }
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
                            current_row.push(build_math_ir(arr_child, options))
                        }
                        _ => {}
                    }
                }
            }
            _ if is_content_node(child) => current_row.push(build_math_ir(child, options)),
            _ => {}
        }
    }

    if !current_row.is_empty() {
        rows.push(current_row);
    }

    MathIr::Environment(MathEnvironment::Matrix {
        name: actual_name.to_string(),
        rows,
    })
}

fn build_args(args_node: &SyntaxNode, options: &T2LOptions) -> Vec<MathIr> {
    args_node
        .children()
        .filter(|child| is_content_node(child))
        .map(|child| build_math_ir(child, options))
        .collect()
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

fn with_leading_backslash(tex: &str) -> String {
    if tex.starts_with('\\') {
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

fn is_cases_condition(ir: &MathIr) -> bool {
    match ir {
        MathIr::Operator(op) => op == "&",
        MathIr::RawLiteral(text) => text.trim_start().starts_with('&'),
        MathIr::Ident(text) => text.trim_start().starts_with('&'),
        MathIr::Seq(items) => items
            .iter()
            .find(|item| !matches!(item, MathIr::Spacing(MathSpacing::Soft)))
            .is_some_and(is_cases_condition),
        _ => false,
    }
}

fn strip_cases_condition_prefix(ir: MathIr) -> MathIr {
    match ir {
        MathIr::Seq(items) => {
            let mut stripped = Vec::new();
            let mut removed = false;
            for item in items {
                if !removed {
                    match item {
                        MathIr::Spacing(MathSpacing::Soft) => continue,
                        MathIr::Operator(op) if op == "&" => {
                            removed = true;
                            continue;
                        }
                        MathIr::RawLiteral(text) if text.trim_start().starts_with('&') => {
                            let rest = text
                                .trim_start()
                                .trim_start_matches('&')
                                .trim_start()
                                .to_string();
                            if !rest.is_empty() {
                                stripped.push(MathIr::RawLiteral(rest));
                            }
                            removed = true;
                            continue;
                        }
                        MathIr::Ident(text) if text.trim_start().starts_with('&') => {
                            let rest = text
                                .trim_start()
                                .trim_start_matches('&')
                                .trim_start()
                                .to_string();
                            if !rest.is_empty() {
                                stripped.push(MathIr::Ident(rest));
                            }
                            removed = true;
                            continue;
                        }
                        other => {
                            removed = true;
                            stripped.push(other);
                            continue;
                        }
                    }
                }
                stripped.push(item);
            }
            seq_or_single(stripped)
        }
        MathIr::Operator(op) if op == "&" => MathIr::empty(),
        MathIr::RawLiteral(text) => {
            let rest = text
                .trim_start()
                .trim_start_matches('&')
                .trim_start()
                .to_string();
            if rest.is_empty() {
                MathIr::empty()
            } else {
                MathIr::RawLiteral(rest)
            }
        }
        MathIr::Ident(text) => {
            let rest = text
                .trim_start()
                .trim_start_matches('&')
                .trim_start()
                .to_string();
            if rest.is_empty() {
                MathIr::empty()
            } else {
                MathIr::Ident(rest)
            }
        }
        other => other,
    }
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
