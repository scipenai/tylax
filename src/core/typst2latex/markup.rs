//! Markup mode conversion for Typst to LaTeX
//!
//! Handles document structure, text formatting, and non-math content.

use super::context::{ConvertContext, EnvironmentContext, T2LOptions, TokenType};
use super::engine::{
    render_math_segments_to_typst_source, ContentNode, DUP_ELEMENT_NAME, DUP_SNAPSHOT_NAME,
    REF_ANCHOR_ELEMENT_NAME,
};
use super::math::convert_math_node;
use super::supplement::{
    custom_figure_counter_name, default_custom_figure_supplement, mark_at_reference,
    mark_reference, node_contains_call, unwrap_balanced_parens,
};
use super::table::{
    infer_table_columns, is_empty_spread, is_none_value, table_rule, LatexCell, LatexCellAlign,
    LatexHLine, LatexTableGenerator, MAX_TABLE_COLUMNS,
};
use super::utils::{
    count_heading_markers, escape_latex_text, extract_length_value, format_latex_color_command,
    get_raw_text_with_lang, get_simple_text, get_string_content, is_display_math,
    is_string_or_content, normalize_typst_color_expr, parse_angle_value, parse_spacing_spec,
    preserve_typst_source, ref_supplement_markup, FuncArgs, SpacingSpec,
};
use crate::data::typst_compat::{
    get_heading_command, is_math_func_in_markup, MarkupHandler, TYPST_MARKUP_HANDLERS,
};
use crate::features::refs::{
    citation_mode_from_typst_form, citation_to_latex, label_to_latex, reference_to_latex, Citation,
    CiteGroup, Reference,
};
use crate::tikz::{convert_cetz_to_tikz, is_cetz_code};
use std::cell::RefCell;
use typst_syntax::ast::{self, AstNode};
use typst_syntax::{SyntaxKind, SyntaxNode};

#[derive(Debug, Clone)]
enum MarkupMarkerSpec {
    Single(String),
    Cycle(Vec<String>),
}

#[derive(Debug, Clone, Default)]
struct MarkupStyleFrame {
    list_marker: Option<MarkupMarkerSpec>,
    enum_start: Option<u64>,
    enum_numbering: Option<String>,
    heading_unnumbered: bool,
    figure_unnumbered: bool,
    itemize_depth: usize,
    enum_depth: usize,
}

thread_local! {
    static MARKUP_STYLE_STACK: RefCell<Vec<MarkupStyleFrame>> = const { RefCell::new(Vec::new()) };
}

struct MarkupStyleGuard;

impl Drop for MarkupStyleGuard {
    fn drop(&mut self) {
        MARKUP_STYLE_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub(crate) fn with_fresh_markup_styles<R>(f: impl FnOnce() -> R) -> R {
    MARKUP_STYLE_STACK.with(|stack| {
        stack.borrow_mut().push(MarkupStyleFrame::default());
    });
    let _guard = MarkupStyleGuard;
    f()
}

fn with_scoped_markup_styles<R>(f: impl FnOnce() -> R) -> R {
    MARKUP_STYLE_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let top = stack.last().cloned().unwrap_or_default();
        stack.push(top);
    });
    let _guard = MarkupStyleGuard;
    f()
}

struct ListDepthGuard {
    is_enum: bool,
    tracked: bool,
}

impl Drop for ListDepthGuard {
    fn drop(&mut self) {
        if !self.tracked {
            return;
        }
        MARKUP_STYLE_STACK.with(|stack| {
            if let Some(frame) = stack.borrow_mut().last_mut() {
                if self.is_enum {
                    frame.enum_depth = frame.enum_depth.saturating_sub(1);
                } else {
                    frame.itemize_depth = frame.itemize_depth.saturating_sub(1);
                }
            }
        });
    }
}

fn enter_list_depth(is_enum: bool) -> (usize, ListDepthGuard) {
    let (depth, tracked) = MARKUP_STYLE_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if let Some(frame) = stack.last_mut() {
            let d = if is_enum {
                let d = frame.enum_depth;
                frame.enum_depth += 1;
                d
            } else {
                let d = frame.itemize_depth;
                frame.itemize_depth += 1;
                d
            };
            (d, true)
        } else {
            (0, false)
        }
    });
    (depth, ListDepthGuard { is_enum, tracked })
}

fn resolve_marker_at_depth(spec: &MarkupMarkerSpec, depth: usize) -> String {
    match spec {
        MarkupMarkerSpec::Single(s) => s.clone(),
        MarkupMarkerSpec::Cycle(items) => {
            if items.is_empty() {
                String::new()
            } else {
                items[depth % items.len()].clone()
            }
        }
    }
}

fn active_list_marker(depth: usize) -> Option<String> {
    MARKUP_STYLE_STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .and_then(|f| f.list_marker.as_ref())
            .map(|spec| resolve_marker_at_depth(spec, depth))
    })
}

fn active_enum_start() -> Option<u64> {
    MARKUP_STYLE_STACK.with(|stack| stack.borrow().last().and_then(|f| f.enum_start))
}

fn active_enum_numbering() -> Option<String> {
    MARKUP_STYLE_STACK.with(|stack| stack.borrow().last().and_then(|f| f.enum_numbering.clone()))
}

fn active_heading_unnumbered() -> bool {
    MARKUP_STYLE_STACK.with(|stack| stack.borrow().last().is_some_and(|f| f.heading_unnumbered))
}

fn active_figure_unnumbered() -> bool {
    MARKUP_STYLE_STACK.with(|stack| stack.borrow().last().is_some_and(|f| f.figure_unnumbered))
}

fn format_latex_list_item_prefix(marker: Option<&str>) -> String {
    match marker {
        None => "\\item ".to_string(),
        Some("") => "\\item[] ".to_string(),
        Some(m) if m.contains(']') => format!("\\item[{{{m}}}] "),
        Some(m) => format!("\\item[{m}] "),
    }
}

fn format_table_list_marker(marker: Option<&str>) -> String {
    match marker {
        None => "\\textbullet~".to_string(),
        Some("") => String::new(),
        Some(m) => format!("{m}~"),
    }
}

fn latex_enum_counter_name(depth: usize) -> &'static str {
    match depth {
        0 => "enumi",
        1 => "enumii",
        2 => "enumiii",
        _ => "enumiv",
    }
}

fn emit_enum_start_counter(
    indent: &str,
    depth: usize,
    start: Option<u64>,
    ctx: &mut ConvertContext,
) {
    if let Some(s) = start.filter(|&s| s != 1) {
        if let Some(counter_val) = (s as i128).checked_sub(1) {
            ctx.push(indent);
            ctx.push_line(&format!(
                "\\setcounter{{{}}}{{{}}}",
                latex_enum_counter_name(depth),
                counter_val
            ));
        }
    }
}

fn emit_enum_item_counter_override(
    indent: &str,
    depth: usize,
    explicit: Option<u64>,
    next_expected: &mut Option<u64>,
    ctx: &mut ConvertContext,
) {
    if let Some(n) = explicit {
        if *next_expected != Some(n) {
            if let Some(counter_val) = (n as i128).checked_sub(1) {
                ctx.push(indent);
                ctx.push_line(&format!(
                    "\\setcounter{{{}}}{{{}}}",
                    latex_enum_counter_name(depth),
                    counter_val
                ));
            }
        }
        *next_expected = n.checked_add(1);
    } else {
        *next_expected = next_expected.and_then(|v| v.checked_add(1));
    }
}

fn is_enum_numbering_token(ch: char) -> bool {
    matches!(ch, '1' | 'a' | 'A' | 'i' | 'I')
}

fn typst_enum_token_to_latex_macro(token: char, counter: &str) -> String {
    match token {
        'I' => format!("\\Roman{{{counter}}}"),
        'i' => format!("\\roman{{{counter}}}"),
        'A' => format!("\\Alph{{{counter}}}"),
        'a' => format!("\\alph{{{counter}}}"),
        _ => format!("\\arabic{{{counter}}}"),
    }
}

fn typst_enum_numbering_to_latex_label(pattern: &str, depth: usize) -> Option<String> {
    let token_count = pattern
        .chars()
        .filter(|&c| is_enum_numbering_token(c))
        .count();
    if token_count == 0 {
        return None;
    }

    let mut out = String::new();
    let mut literal = String::new();
    let flush_literal = |out: &mut String, literal: &mut String| {
        if !literal.is_empty() {
            out.push_str(&escape_latex_text(literal));
            literal.clear();
        }
    };

    if token_count == 1 {
        let counter = latex_enum_counter_name(depth);
        for ch in pattern.chars() {
            if is_enum_numbering_token(ch) {
                flush_literal(&mut out, &mut literal);
                out.push_str(&typst_enum_token_to_latex_macro(ch, counter));
            } else {
                literal.push(ch);
            }
        }
        flush_literal(&mut out, &mut literal);
        if depth == 0 && out == "\\arabic{enumi}." {
            return None;
        }
        return Some(out);
    }

    let target_tokens = (depth + 1).min(token_count);
    let mut seen_tokens = 0usize;
    for ch in pattern.chars() {
        if is_enum_numbering_token(ch) {
            if seen_tokens >= target_tokens {
                break;
            }
            flush_literal(&mut out, &mut literal);
            out.push_str(&typst_enum_token_to_latex_macro(
                ch,
                latex_enum_counter_name(seen_tokens),
            ));
            seen_tokens += 1;
        } else {
            literal.push(ch);
        }
    }
    flush_literal(&mut out, &mut literal);
    Some(out)
}

fn emit_enum_numbering_cmd(
    indent: &str,
    depth: usize,
    numbering: Option<&str>,
    ctx: &mut ConvertContext,
) {
    if let Some(label_fmt) =
        numbering.and_then(|pat| typst_enum_numbering_to_latex_label(pat, depth))
    {
        ctx.push(indent);
        ctx.push_line(&format!(
            "\\renewcommand{{\\label{}}}{{{}}}",
            latex_enum_counter_name(depth),
            label_fmt
        ));
    }
}

fn format_roman_numeral(mut n: u64, upper: bool) -> String {
    if n == 0 || n >= 4000 {
        return n.to_string();
    }
    const TABLE: &[(u64, &str)] = &[
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for &(val, sym) in TABLE {
        while n >= val {
            out.push_str(sym);
            n -= val;
        }
    }
    if upper {
        out.to_uppercase()
    } else {
        out
    }
}

fn format_alpha_numeral(n: u64, upper: bool) -> String {
    if n == 0 || n > 26 {
        return n.to_string();
    }
    let base = if upper { b'A' } else { b'a' };
    ((base + (n as u8 - 1)) as char).to_string()
}

fn format_enum_number_with_pattern(n: u64, pattern: Option<&str>) -> String {
    let Some(pat) = pattern else {
        return format!("{n}.~");
    };
    if !pat.chars().any(is_enum_numbering_token) {
        return format!("{n}.~");
    }
    let mut out = String::new();
    let mut replaced = false;
    let mut literal = String::new();
    let flush_lit = |out: &mut String, literal: &mut String| {
        if !literal.is_empty() {
            out.push_str(&escape_latex_text(literal));
            literal.clear();
        }
    };
    for ch in pat.chars() {
        if is_enum_numbering_token(ch) && !replaced {
            flush_lit(&mut out, &mut literal);
            replaced = true;
            match ch {
                'I' => out.push_str(&format_roman_numeral(n, true)),
                'i' => out.push_str(&format_roman_numeral(n, false)),
                'A' => out.push_str(&format_alpha_numeral(n, true)),
                'a' => out.push_str(&format_alpha_numeral(n, false)),
                _ => out.push_str(&n.to_string()),
            }
        } else if is_enum_numbering_token(ch) {
            break;
        } else {
            literal.push(ch);
        }
    }
    flush_lit(&mut out, &mut literal);
    format!("{out}~")
}

fn render_marker_expr_to_latex(expr: ast::Expr<'_>, ctx: &ConvertContext) -> String {
    let mut inner = ConvertContext::new();
    inner.options = ctx.options.clone();
    if let ast::Expr::Str(s) = expr {
        push_escaped_latex_text(&mut inner, &s.get());
    } else {
        convert_markup_node(expr.to_untyped(), &mut inner);
    }
    collapse_table_text(&inner.finalize())
}

fn ast_expr_to_marker_spec(
    mut expr: ast::Expr<'_>,
    ctx: &ConvertContext,
) -> Option<Option<MarkupMarkerSpec>> {
    while let ast::Expr::Parenthesized(p) = expr {
        expr = p.expr();
    }
    match expr {
        ast::Expr::Auto(_) => Some(None),
        ast::Expr::None(_) => Some(Some(MarkupMarkerSpec::Single(String::new()))),
        ast::Expr::Closure(_) => None,
        ast::Expr::Array(arr) => {
            let mut markers = Vec::new();
            for item in arr.items() {
                if let ast::ArrayItem::Pos(inner) = item {
                    markers.push(render_marker_expr_to_latex(inner, ctx));
                }
            }
            if markers.is_empty() {
                Some(Some(MarkupMarkerSpec::Single(String::new())))
            } else {
                Some(Some(MarkupMarkerSpec::Cycle(markers)))
            }
        }
        other => Some(Some(MarkupMarkerSpec::Single(render_marker_expr_to_latex(
            other, ctx,
        )))),
    }
}

fn valid_named_expr(named: ast::Named<'_>) -> Option<ast::Expr<'_>> {
    if !named.to_untyped().errors().is_empty() {
        return None;
    }
    let mut seen_colon = false;
    for child in named.to_untyped().children() {
        if !seen_colon {
            if child.kind() == SyntaxKind::Colon {
                seen_colon = true;
            }
            continue;
        }
        if let Some(expr) = child.cast::<ast::Expr>() {
            return Some(expr);
        }
    }
    None
}

fn apply_ast_set_rule(node: &SyntaxNode, ctx: &ConvertContext) {
    let Some(rule) = node.cast::<ast::SetRule>() else {
        return;
    };
    if rule.condition().is_some() {
        return;
    }
    let ast::Expr::Ident(target) = rule.target() else {
        return;
    };
    if super::scope_analysis::is_shadowed_node(target.to_untyped()) {
        return;
    }
    let target_name = target.get().as_str();
    for item in rule.args().items() {
        let ast::Arg::Named(named) = item else {
            continue;
        };
        let key = named.name().get().as_str();
        let Some(val_expr) = valid_named_expr(named) else {
            continue;
        };
        match (target_name, key) {
            ("list", "marker") => {
                if let Some(resolved) = ast_expr_to_marker_spec(val_expr, ctx) {
                    MARKUP_STYLE_STACK.with(|stack| {
                        if let Some(frame) = stack.borrow_mut().last_mut() {
                            frame.list_marker = resolved;
                        }
                    });
                }
            }
            ("enum", "start") => {
                let mut inner = val_expr;
                while let ast::Expr::Parenthesized(p) = inner {
                    inner = p.expr();
                }
                let next_start = match inner {
                    ast::Expr::Auto(_) | ast::Expr::None(_) => Some(None),
                    ast::Expr::Int(i) => u64::try_from(i.get()).ok().map(Some),
                    _ => None,
                };
                if let Some(start_val) = next_start {
                    MARKUP_STYLE_STACK.with(|stack| {
                        if let Some(frame) = stack.borrow_mut().last_mut() {
                            frame.enum_start = start_val;
                        }
                    });
                }
            }
            ("enum", "numbering") => {
                let mut inner = val_expr;
                while let ast::Expr::Parenthesized(p) = inner {
                    inner = p.expr();
                }
                let next_numbering = match inner {
                    ast::Expr::Auto(_) => Some(None),
                    ast::Expr::Str(s) => Some(Some(s.get().to_string())),
                    _ => None,
                };
                if let Some(num_val) = next_numbering {
                    MARKUP_STYLE_STACK.with(|stack| {
                        if let Some(frame) = stack.borrow_mut().last_mut() {
                            frame.enum_numbering = num_val;
                        }
                    });
                }
            }
            ("heading", "numbering") => {
                let mut inner = val_expr;
                while let ast::Expr::Parenthesized(p) = inner {
                    inner = p.expr();
                }
                let unnumbered = matches!(inner, ast::Expr::None(_));
                MARKUP_STYLE_STACK.with(|stack| {
                    if let Some(frame) = stack.borrow_mut().last_mut() {
                        frame.heading_unnumbered = unnumbered;
                    }
                });
            }
            ("figure", "numbering") => {
                let mut inner = val_expr;
                while let ast::Expr::Parenthesized(p) = inner {
                    inner = p.expr();
                }
                let unnumbered = matches!(inner, ast::Expr::None(_));
                MARKUP_STYLE_STACK.with(|stack| {
                    if let Some(frame) = stack.borrow_mut().last_mut() {
                        frame.figure_unnumbered = unnumbered;
                    }
                });
            }
            _ => {}
        }
    }
}

/// Languages supported by the listings package (case-insensitive check)
/// This is a subset of commonly used languages that listings supports by default
const LISTINGS_SUPPORTED_LANGUAGES: &[&str] = &[
    "abap",
    "acsl",
    "ada",
    "algol",
    "ant",
    "assembler",
    "awk",
    "bash",
    "basic",
    "c",
    "caml",
    "cil",
    "clean",
    "cobol",
    "comsol",
    "csh",
    "delphi",
    "eiffel",
    "erlang",
    "euphoria",
    "fortran",
    "gcl",
    "gnuplot",
    "haskell",
    "html",
    "idl",
    "inform",
    "java",
    "jvmis",
    "ksh",
    "lisp",
    "logo",
    "lua",
    "make",
    "mathematica",
    "matlab",
    "mercury",
    "metapost",
    "miranda",
    "mizar",
    "ml",
    "modula-2",
    "mupad",
    "nastran",
    "oberon-2",
    "ocl",
    "octave",
    "oz",
    "pascal",
    "perl",
    "php",
    "pl/i",
    "plasm",
    "postscript",
    "pov",
    "prolog",
    "promela",
    "pstricks",
    "python",
    "r",
    "reduce",
    "rexx",
    "rsl",
    "ruby",
    "s",
    "sas",
    "scala",
    "scilab",
    "sh",
    "shelxl",
    "simula",
    "sparql",
    "sql",
    "tcl",
    "tex",
    "vbscript",
    "verilog",
    "vhdl",
    "vrml",
    "xml",
    "xslt",
    // Additional common aliases
    "c++",
    "cpp",
    "objective-c",
    "objc",
    "javascript",
    "js",
    "typescript",
    "ts",
];

fn flush_typst_chunk(buffer: &mut String, ctx: &mut ConvertContext) {
    if buffer.trim().is_empty() {
        buffer.clear();
        return;
    }
    let root = typst_syntax::parse(buffer);
    convert_markup_node(&root, ctx);
    buffer.clear();
}

fn emit_rendered_math(ctx: &mut ConvertContext, math_content: &str, is_block: bool) {
    let in_table = ctx.is_in_env(&EnvironmentContext::Table);

    if math_content.trim().is_empty() {
        return;
    }

    let has_alignment = has_unescaped_alignment(math_content);

    if has_alignment {
        if in_table {
            ctx.push("$\\begin{aligned}");
            if math_content.trim_start().starts_with('[') {
                ctx.push("[c] ");
            } else {
                ctx.push(" ");
            }
            ctx.push(math_content);
            ctx.push(" \\end{aligned}$");
        } else {
            ctx.push("\\begin{align}\n");
            ctx.push(math_content);
            ctx.push("\n\\end{align}");
        }
    } else if is_block && !in_table {
        ctx.push("\\[\n");
        ctx.push(math_content);
        ctx.push("\n\\]");
    } else {
        ctx.push("$");
        ctx.push(math_content);
        ctx.push("$");
    }
    ctx.last_token = TokenType::Command;
}

fn emit_evaluated_term_items<'a>(
    ctx: &mut ConvertContext,
    pairs: impl IntoIterator<Item = (&'a [ContentNode], &'a [ContentNode])>,
) {
    let pairs: Vec<_> = pairs.into_iter().collect();
    if pairs.is_empty() {
        return;
    }
    if ctx.is_in_env(&EnvironmentContext::Table) {
        let items: Vec<String> = pairs
            .into_iter()
            .map(|(t, d)| {
                render_table_term_entry(
                    ctx,
                    |c| convert_content_nodes_to_latex(t, c),
                    |c| convert_content_nodes_to_latex(d, c),
                )
            })
            .collect();
        emit_table_cell_lines(items, ctx);
    } else {
        begin_description(ctx);
        for (t, d) in pairs {
            emit_description_item(
                ctx,
                |c| convert_content_nodes_to_latex(t, c),
                |c| convert_content_nodes_to_latex(d, c),
            );
        }
        end_description(ctx);
    }
}

fn emit_refstepcounter(counter: &str, ctx: &mut ConvertContext) {
    if counter.starts_with("tylaxfig") {
        ctx.push(&format!(
            "\\ifcsname c@{counter}\\endcsname\\else\\newcounter{{{counter}}}\\fi"
        ));
    }
    ctx.push(&format!("\\refstepcounter{{{counter}}}"));
}

const HEADING_COUNTERS: [&str; 5] = [
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "subparagraph",
];

fn emit_dup_snapshot(counter: &str, origin_id: usize, ctx: &mut ConvertContext) {
    if HEADING_COUNTERS.contains(&counter) {
        for c in HEADING_COUNTERS {
            ctx.push_line(&format!(
                "\\expandafter\\xdef\\csname tylaxorig:{origin_id}:{c}\\endcsname{{\\number\\value{{{c}}}}}%"
            ));
        }
    } else if !counter.is_empty() {
        ctx.push_line(&format!(
            "\\ifcsname c@{counter}\\endcsname\\expandafter\\xdef\\csname tylaxorig:{origin_id}:{counter}\\endcsname{{\\number\\value{{{counter}}}}}\\fi%"
        ));
    }
}

fn emit_dup_counter_prefix(counter: &str, id: usize, origin_id: usize, ctx: &mut ConvertContext) {
    ctx.ensure_paragraph_break();
    ctx.push_line("\\begingroup%");
    if let Some(level_idx) = HEADING_COUNTERS.iter().position(|&c| c == counter) {
        for c in HEADING_COUNTERS {
            ctx.push_line(&format!("\\edef\\tylaxsaved{c}{{\\number\\value{{{c}}}}}%"));
        }
        if origin_id > 0 {
            ctx.push(&format!(
                "\\ifcsname tylaxorig:{origin_id}:{counter}\\endcsname"
            ));
            for parent in &HEADING_COUNTERS[..level_idx] {
                ctx.push(&format!(
                    "\\setcounter{{{parent}}}{{\\csname tylaxorig:{origin_id}:{parent}\\endcsname}}"
                ));
            }
            ctx.push_line(&format!(
                "\\setcounter{{{counter}}}{{\\numexpr\\csname tylaxorig:{origin_id}:{counter}\\endcsname-1\\relax}}\\else\\addtocounter{{{counter}}}{{-1}}\\fi%"
            ));
        } else {
            ctx.push_line(&format!("\\addtocounter{{{counter}}}{{-1}}%"));
        }
        ctx.push_line(&format!(
            "\\ifcsname theH{counter}\\endcsname\\renewcommand{{\\theH{counter}}}{{tylaxdup{id}.\\arabic{{{counter}}}}}\\fi%"
        ));
    } else if !counter.is_empty() {
        ctx.push_line(&format!(
            "\\ifcsname c@{counter}\\endcsname\\edef\\tylaxsavedctr{{\\number\\value{{{counter}}}}}\\fi%"
        ));
        if origin_id > 0 {
            ctx.push_line(&format!(
                "\\ifcsname c@{counter}\\endcsname\\ifcsname tylaxorig:{origin_id}:{counter}\\endcsname\\setcounter{{{counter}}}{{\\numexpr\\csname tylaxorig:{origin_id}:{counter}\\endcsname-1\\relax}}\\else\\addtocounter{{{counter}}}{{-1}}\\fi\\fi%"
            ));
        } else {
            ctx.push_line(&format!(
                "\\ifcsname c@{counter}\\endcsname\\addtocounter{{{counter}}}{{-1}}\\fi%"
            ));
        }
        ctx.push_line(&format!(
            "\\ifcsname theH{counter}\\endcsname\\renewcommand{{\\theH{counter}}}{{tylaxdup{id}.\\arabic{{{counter}}}}}\\fi%"
        ));
    }
}

fn emit_dup_counter_suffix(counter: &str, ctx: &mut ConvertContext) {
    if HEADING_COUNTERS.contains(&counter) {
        for c in HEADING_COUNTERS {
            ctx.push_line(&format!("\\setcounter{{{c}}}{{\\tylaxsaved{c}}}%"));
        }
    } else if !counter.is_empty() {
        ctx.push_line(&format!(
            "\\ifcsname c@{counter}\\endcsname\\ifdefined\\tylaxsavedctr\\setcounter{{{counter}}}{{\\tylaxsavedctr}}\\fi\\fi%"
        ));
    }
    ctx.push_line("\\endgroup");
}

pub fn convert_content_nodes_to_latex(nodes: &[ContentNode], ctx: &mut ConvertContext) {
    let mut buffer = String::new();

    let mut i = 0;
    let mut after_list_item = false;
    while i < nodes.len() {
        let node = &nodes[i];
        i += 1;
        // Nested evaluated markup can retain the newline as a Space. Keep
        // adjacent ordinary list items together until the chunk is parsed.
        if after_list_item
            && matches!(node, ContentNode::Space)
            && nodes
                .get(i)
                .is_some_and(|next| next.is_ordinary_list_item())
        {
            continue;
        }
        after_list_item = node.is_ordinary_list_item();

        if let Some((counter, _kind, label, step)) = node.ref_anchor_parts() {
            flush_typst_chunk(&mut buffer, ctx);
            if step && !counter.is_empty() {
                emit_refstepcounter(counter, ctx);
            }
            if let Some(lbl) = label.filter(|l| !l.is_empty()) {
                ctx.push(&label_to_latex(lbl));
            }
            ctx.last_token = TokenType::Command;
            continue;
        }

        if let Some((counter, origin_id)) = node.dup_snapshot_parts() {
            flush_typst_chunk(&mut buffer, ctx);
            emit_dup_snapshot(counter, origin_id, ctx);
            ctx.last_token = TokenType::Command;
            continue;
        }

        if let Some((counter, id, origin_id, body)) = node.dup_element_parts() {
            flush_typst_chunk(&mut buffer, ctx);
            emit_dup_counter_prefix(counter, id, origin_id, ctx);
            convert_content_nodes_to_latex(body, ctx);
            emit_dup_counter_suffix(counter, ctx);
            ctx.last_token = TokenType::Command;
            continue;
        }

        if let Some(items) = node.terms_container_items() {
            flush_typst_chunk(&mut buffer, ctx);
            let pairs: Vec<_> = items
                .iter()
                .filter_map(ContentNode::term_item_parts)
                .collect();
            if pairs.len() == items.len() {
                emit_evaluated_term_items(ctx, pairs);
            } else {
                convert_content_nodes_to_latex(&items, ctx);
            }
            continue;
        }

        if let Some((term, description)) = node.term_item_parts() {
            flush_typst_chunk(&mut buffer, ctx);
            let mut pairs = vec![(term, description)];
            collect_following_evaluated_term_items(nodes, &mut i, |t, d| {
                pairs.push((t, d));
            });
            emit_evaluated_term_items(ctx, pairs);
            continue;
        }
        match node {
            ContentNode::Space => {
                flush_typst_chunk(&mut buffer, ctx);
                if !ctx.output.is_empty()
                    && !ctx.output.ends_with(' ')
                    && !ctx.output.ends_with('\n')
                {
                    ctx.push(" ");
                }
            }
            ContentNode::Parbreak => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.ensure_paragraph_break();
                ctx.last_token = TokenType::Newline;
            }
            ContentNode::Text(s) => {
                flush_typst_chunk(&mut buffer, ctx);
                if !s.is_empty() {
                    if !is_collapsible_ascii_whitespace(s) {
                        push_escaped_latex_text(ctx, s);
                        ctx.last_token = TokenType::Text;
                    } else if ctx.last_token != TokenType::Newline && !ctx.output.ends_with(' ') {
                        ctx.push(" ");
                    }
                }
            }
            ContentNode::Linebreak => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.push("\\\n");
                ctx.last_token = TokenType::Newline;
            }
            ContentNode::Citation {
                keys,
                mode,
                supplement,
            } => {
                flush_typst_chunk(&mut buffer, ctx);
                let mut group = CiteGroup::new();
                group.suffix = supplement.clone();
                for key in keys {
                    group.push(Citation::with_mode(key.clone(), *mode));
                }
                ctx.push(&citation_to_latex(&group));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::Reference { target, ref_type } => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.push(&mark_reference(&reference_to_latex(&Reference {
                    target: target.clone(),
                    ref_type: *ref_type,
                })));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::LabelDef(label) => {
                flush_typst_chunk(&mut buffer, ctx);
                ctx.push(&label_to_latex(label));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::Bibliography { file, style } => {
                flush_typst_chunk(&mut buffer, ctx);
                let bib_name = file
                    .trim_end_matches(".yml")
                    .trim_end_matches(".yaml")
                    .trim_end_matches(".bib");
                ctx.ensure_paragraph_break();
                ctx.push_line(&format!(
                    r"\bibliographystyle{{{}}}",
                    style.clone().unwrap_or_else(|| "plain".to_string())
                ));
                ctx.push_line(&format!(r"\bibliography{{{}}}", bib_name));
                ctx.last_token = TokenType::Command;
            }
            ContentNode::Math { segments, block } => {
                flush_typst_chunk(&mut buffer, ctx);
                let math_source = render_math_segments_to_typst_source(segments);
                let math_content = convert_math_source_to_latex(
                    &math_source,
                    &ctx.options,
                    false,
                    &mut ctx.warnings,
                );
                let math_content = rerender_math_with_row_linebreaks_if_aligned(
                    &math_source,
                    &ctx.options,
                    math_content,
                );
                emit_rendered_math(ctx, &math_content, *block);
            }
            other => {
                if other.is_block_list_like() || other.is_heading() {
                    buffer.truncate(buffer.trim_end_matches([' ', '\t']).len());
                    if !buffer.is_empty() && !buffer.ends_with('\n') {
                        buffer.push('\n');
                    }
                }
                buffer.push_str(&other.to_typst_for_latex());
                if matches!(other, ContentNode::FuncCall { name, .. } if name == "figure") {
                    let mut scan = i;
                    while matches!(nodes.get(scan), Some(ContentNode::Space)) {
                        scan += 1;
                    }
                    if let Some(ContentNode::Label(lbl) | ContentNode::LabelDef(lbl)) =
                        nodes.get(scan)
                    {
                        buffer.push_str(&format!(" <{lbl}>"));
                        i = scan + 1;
                    }
                }
            }
        }
    }

    flush_typst_chunk(&mut buffer, ctx);
}

fn convert_math_source_to_latex(
    math_source: &str,
    options: &T2LOptions,
    linebreak_as_row: bool,
    warnings: &mut Vec<String>,
) -> String {
    let wrapped = format!("${}$", math_source);
    let root = typst_syntax::parse(&wrapped);
    let mut math_ctx = ConvertContext::new();
    math_ctx.options = options.clone();
    math_ctx.in_math = true;
    if linebreak_as_row {
        math_ctx.push_env(EnvironmentContext::Align);
    }
    convert_first_math_child(&root, &mut math_ctx);
    warnings.append(&mut math_ctx.warnings);
    super::context::trim_latex(&math_ctx.finalize()).to_string()
}

// In both math paths, only the first render reports warnings.
fn rerender_math_with_row_linebreaks_if_aligned(
    math_source: &str,
    options: &T2LOptions,
    rendered: String,
) -> String {
    if rendered.contains(" \\\n") && has_unescaped_alignment(&rendered) {
        convert_math_source_to_latex(math_source, options, true, &mut Vec::new())
    } else {
        rendered
    }
}

fn convert_equation_math_to_latex(
    node: &SyntaxNode,
    options: &T2LOptions,
    linebreak_as_row: bool,
    warnings: &mut Vec<String>,
) -> String {
    let mut math_ctx = ConvertContext::new();
    math_ctx.options = options.clone();
    math_ctx.in_math = true;
    if linebreak_as_row {
        math_ctx.push_env(EnvironmentContext::Align);
    }
    for child in node.children() {
        if child.kind() == SyntaxKind::Math {
            convert_math_node(child, &mut math_ctx);
        }
    }
    warnings.append(&mut math_ctx.warnings);
    super::context::trim_latex(&math_ctx.finalize()).to_string()
}

fn rerender_equation_with_row_linebreaks_if_aligned(
    node: &SyntaxNode,
    options: &T2LOptions,
    rendered: String,
) -> String {
    if rendered.contains(" \\\n") && has_unescaped_alignment(&rendered) {
        convert_equation_math_to_latex(node, options, true, &mut Vec::new())
    } else {
        rendered
    }
}

fn convert_first_math_child(node: &SyntaxNode, ctx: &mut ConvertContext) -> bool {
    if node.kind() == SyntaxKind::Math {
        convert_math_node(node, ctx);
        return true;
    }

    for child in node.children() {
        if convert_first_math_child(child, ctx) {
            return true;
        }
    }

    false
}

/// Check if a language is supported by the listings package
fn is_listings_supported(lang: &str) -> bool {
    let lang_lower = lang.to_lowercase();
    LISTINGS_SUPPORTED_LANGUAGES
        .iter()
        .any(|&supported| supported == lang_lower)
}

/// Check if math content has alignment markers (&) that are not inside cases/matrix environments
/// These need to be wrapped in an align environment
fn has_unescaped_alignment(content: &str) -> bool {
    // Count nesting level of cases/matrix environments
    let mut depth = 0;
    let mut brace_depth = 0;
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            // Retained Typst source is a comment, not part of the math layout.
            for ch in chars.by_ref() {
                if matches!(ch, '\n' | '\r') {
                    break;
                }
            }
        } else if ch == '\\' {
            // Check for \begin or \end before treating the next character as an
            // escaped literal (for example `\&`, which is not an alignment
            // marker).
            let rest: String = chars.clone().take(5).collect();
            if rest.starts_with("begin") {
                depth += 1;
            } else if rest.starts_with("end") && depth > 0 {
                depth -= 1;
            } else {
                let _ = chars.next();
            }
        } else if ch == '{' {
            brace_depth += 1;
        } else if ch == '}' && brace_depth > 0 {
            brace_depth -= 1;
        } else if ch == '&' && depth == 0 && brace_depth == 0 {
            // Found an alignment marker outside of any environment
            return true;
        }
    }

    false
}

/// Convert a markup node to LaTeX
/// Convert an explicit `@target[supplement]` body to LaTeX, if it was written.
///
/// A supplement is CONTENT, not a string: `@sec-one[*Custom*]` renders bold, so
/// it goes through the normal markup conversion rather than being flattened to
/// its source text. See [`ref_supplement_markup`] for why `Some("")` matters.
pub fn convert_ref_supplement(node: &SyntaxNode, ctx: &mut ConvertContext) -> Option<String> {
    let markup = ref_supplement_markup(node)?;
    let mut inner = ConvertContext::new();
    convert_markup_node(&markup, &mut inner);
    Some(inner.finish_into(ctx))
}

pub fn convert_markup_node(node: &SyntaxNode, ctx: &mut ConvertContext) {
    match node.kind() {
        SyntaxKind::Markup => {
            // Process children, but group consecutive list items into list environments
            let children: Vec<_> = node.children().collect();
            convert_markup_children(&children, ctx);
        }

        SyntaxKind::Text => {
            let text = node.text().to_string();
            // Strip Typst code block braces if they wrap the entire text
            let cleaned = if text.starts_with('{') && text.ends_with('}') && text.len() > 2 {
                &text[1..text.len() - 1]
            } else {
                &text
            };
            if !is_collapsible_ascii_whitespace(cleaned) {
                push_escaped_latex_text(ctx, cleaned);
                ctx.last_token = TokenType::Text;
            } else if !cleaned.is_empty() && ctx.last_token != TokenType::Newline {
                ctx.push(" ");
            }
        }

        SyntaxKind::Space => {
            if ctx.last_token != TokenType::Newline && !ctx.output.ends_with(' ') {
                ctx.push(" ");
            }
        }

        SyntaxKind::Shorthand => {
            if let Some(sh) = node.cast::<ast::Shorthand>() {
                let mut buf = [0u8; 4];
                push_escaped_latex_text(ctx, sh.get().encode_utf8(&mut buf));
                ctx.last_token = TokenType::Text;
            }
        }

        // Escape sequences: \$, \#, \%, \u{...}, etc.
        SyntaxKind::Escape => {
            let text = node.text().to_string();
            let decoded = node
                .cast::<ast::Escape>()
                .map(|escape| escape.get().to_string());
            let escaped_char = decoded
                .as_deref()
                .unwrap_or_else(|| text.strip_prefix('\\').unwrap_or(&text));
            push_escaped_latex_text(ctx, escaped_char);
            ctx.last_token = TokenType::Text;
        }

        SyntaxKind::Parbreak => {
            ctx.ensure_paragraph_break();
        }

        SyntaxKind::Linebreak => {
            // In table cells, use \newline instead of \\ to avoid LR mode errors
            if ctx.is_in_env(&EnvironmentContext::Table) {
                ctx.push("\\newline ");
            } else {
                ctx.push("\\\\\n");
            }
            ctx.last_token = TokenType::Newline;
        }

        // Headings
        SyntaxKind::Heading => {
            if ctx.is_in_env(&EnvironmentContext::Table) {
                let rendered = render_in_table_cell(ctx, |inner| {
                    for child in node.children() {
                        if child.kind() != SyntaxKind::HeadingMarker {
                            convert_markup_node(child, inner);
                        }
                    }
                });
                let title = collapse_table_text(&rendered);
                if !title.is_empty() {
                    emit_table_cell_lines(vec![format!("\\textbf{{{title}}}")], ctx);
                }
                return;
            }
            let level = count_heading_markers(node);
            let section_cmd = get_heading_command(level);

            ctx.ensure_paragraph_break();
            ctx.push(section_cmd);
            if active_heading_unnumbered() {
                ctx.push("*");
            }
            ctx.push("{");

            // Get heading content
            for child in node.children() {
                if child.kind() != SyntaxKind::HeadingMarker {
                    convert_markup_node(child, ctx);
                }
            }

            ctx.push("}\n");
            ctx.last_token = TokenType::Newline;
        }

        SyntaxKind::HeadingMarker => {
            // Skip, handled by Heading
        }

        // Strong (bold)
        SyntaxKind::Strong => {
            ctx.push("\\textbf{");
            for child in node.children() {
                if child.kind() != SyntaxKind::Star {
                    convert_markup_node(child, ctx);
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // Emphasis (italic)
        SyntaxKind::Emph => {
            ctx.push("\\textit{");
            for child in node.children() {
                if child.kind() != SyntaxKind::Underscore {
                    convert_markup_node(child, ctx);
                }
            }
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // Raw/Code
        SyntaxKind::Raw => {
            let (text, lang) = get_raw_text_with_lang(node);
            // Determine if this is a block code or inline code
            // Block code: starts with ```, or contains newlines, or has language tag
            let raw_text = node.text().to_string();
            let is_block = raw_text.starts_with("```") || text.contains('\n') || lang.is_some();

            if is_block {
                if ctx.is_in_env(&EnvironmentContext::Table) {
                    let table_text = node
                        .cast::<ast::Raw>()
                        .map(|raw| {
                            raw.lines()
                                .map(|l| l.get().to_string())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_else(|| text.clone());
                    emit_table_raw_code(&table_text, ctx);
                    ctx.last_token = TokenType::Command;
                    return;
                }
                ctx.ensure_paragraph_break();
                // Only use lstlisting if the language is supported, otherwise use verbatim
                if let Some(ref language) = lang {
                    if is_listings_supported(language) {
                        ctx.push_line(&format!("\\begin{{lstlisting}}[language={}]", language));
                        ctx.push(&text);
                        ctx.newline();
                        ctx.push_line("\\end{lstlisting}");
                    } else {
                        // Unsupported language - use verbatim with a comment
                        ctx.push_line(&format!("% Code block (language: {})", language));
                        ctx.push_line("\\begin{verbatim}");
                        ctx.push(&text);
                        ctx.newline();
                        ctx.push_line("\\end{verbatim}");
                    }
                } else {
                    ctx.push_line("\\begin{verbatim}");
                    ctx.push(&text);
                    ctx.newline();
                    ctx.push_line("\\end{verbatim}");
                }
            } else {
                ctx.push("\\texttt{");
                ctx.push(&escape_latex_text(&text));
                ctx.push("}");
            }
            ctx.last_token = TokenType::Command;
        }

        // Links
        SyntaxKind::Link => {
            let url = node.text().to_string();
            ctx.push("\\url{");
            ctx.push(&url);
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }

        // Lists - these are now handled by the Markup case above
        // If we encounter them directly, we're likely in a nested context
        SyntaxKind::ListItem => {
            let (depth, _depth_guard) = enter_list_depth(false);
            let marker = active_list_marker(depth);
            if ctx.is_in_env(&EnvironmentContext::Table) {
                let table_marker = format_table_list_marker(marker.as_deref());
                let item = render_table_list_item(node, &table_marker, ctx);
                emit_table_cell_lines(vec![item], ctx);
            } else if ctx.in_list() {
                // Check if we're already in an itemize environment
                convert_list_item(node, ctx, marker.as_deref());
            } else {
                // Standalone list item - wrap it in an environment
                ctx.push_line("\\begin{itemize}");
                convert_list_item(node, ctx, marker.as_deref());
                ctx.push_line("\\end{itemize}");
            }
        }

        SyntaxKind::EnumItem => {
            let (depth, _depth_guard) = enter_list_depth(true);
            let start = active_enum_start();
            let numbering = active_enum_numbering();
            let explicit = node.cast::<ast::EnumItem>().and_then(|e| e.number());
            if ctx.is_in_env(&EnvironmentContext::Table) {
                let num = explicit.or(start).unwrap_or(1);
                let marker = format_enum_number_with_pattern(num, numbering.as_deref());
                let item = render_table_list_item(node, &marker, ctx);
                emit_table_cell_lines(vec![item], ctx);
            } else if ctx.in_list() {
                // Check if we're already in an enumerate environment
                if let Some(n) = explicit {
                    if let Some(counter_val) = (n as i128).checked_sub(1) {
                        ctx.push_indent();
                        ctx.push_line(&format!(
                            "\\setcounter{{{}}}{{{}}}",
                            latex_enum_counter_name(depth),
                            counter_val
                        ));
                    }
                }
                convert_enum_item(node, ctx);
            } else {
                // Standalone enum item - wrap it in an environment
                ctx.push_line("\\begin{enumerate}");
                emit_enum_numbering_cmd("", depth, numbering.as_deref(), ctx);
                if explicit.is_none() {
                    emit_enum_start_counter("", depth, start, ctx);
                } else if let Some(n) = explicit.filter(|&n| n != 1) {
                    if let Some(counter_val) = (n as i128).checked_sub(1) {
                        ctx.push_line(&format!(
                            "\\setcounter{{{}}}{{{}}}",
                            latex_enum_counter_name(depth),
                            counter_val
                        ));
                    }
                }
                convert_enum_item(node, ctx);
                ctx.push_line("\\end{enumerate}");
            }
        }

        SyntaxKind::TermItem => {
            if let Some((term, description)) = term_item_parts(node) {
                emit_single_ast_term_item(term, description, ctx);
            }
        }

        // Math (inline or display)
        SyntaxKind::Equation => {
            let in_table = ctx.is_in_env(&EnvironmentContext::Table);
            let is_block = !in_table && is_display_math(node);

            // Convert math content to a temporary buffer first. Start with
            // inline-fragment linebreaks, then re-render with LaTeX row
            // separators only when the expression is actually aligned.
            let math_content =
                convert_equation_math_to_latex(node, &ctx.options, false, &mut ctx.warnings);
            let math_content =
                rerender_equation_with_row_linebreaks_if_aligned(node, &ctx.options, math_content);

            emit_rendered_math(ctx, &math_content, is_block);
        }

        SyntaxKind::Math => {
            // Delegate to math converter
            convert_math_node(node, ctx);
        }

        // Function calls (like #image, #table, etc.)
        SyntaxKind::FuncCall => {
            convert_func_call_markup(node, ctx);
        }

        // Content blocks
        SyntaxKind::ContentBlock => {
            with_scoped_markup_styles(|| {
                for child in node.children() {
                    convert_markup_node(child, ctx);
                }
            });
        }

        // Code mode - process content, skip braces
        SyntaxKind::Code => {
            for child in node.children() {
                convert_markup_node(child, ctx);
            }
        }

        // Code block {expr} - skip braces, process inner content
        SyntaxKind::CodeBlock => {
            with_scoped_markup_styles(|| {
                for child in node.children() {
                    // Skip the braces themselves
                    if child.kind() != SyntaxKind::LeftBrace
                        && child.kind() != SyntaxKind::RightBrace
                    {
                        convert_markup_node(child, ctx);
                    }
                }
            });
        }

        // Track set rules for list/enum/heading and skip show rules
        SyntaxKind::SetRule => {
            apply_ast_set_rule(node, ctx);
        }
        SyntaxKind::ShowRule => {
            // Do nothing
        }

        // Handle identifiers that might be content
        SyntaxKind::Ident => {
            let text = node.text().to_string();
            // Ignore common keywords that might appear as identifiers in some contexts
            if !matches!(text.as_str(), "set" | "show" | "let") {
                ctx.push(&text);
                ctx.last_token = TokenType::Letter;
            }
        }

        // Bare `@target` is ambiguous until the whole document is known: label or bib entry.
        SyntaxKind::Ref => {
            // The target is the `RefMarker` alone: reading the node's whole text
            // swallowed a `[supplement]` body into the label.
            let target = node
                .children()
                .find(|c| c.kind() == SyntaxKind::RefMarker)
                .map(|marker| marker.text().to_string())
                .unwrap_or_else(|| get_simple_text(node));
            let label = target.trim_start_matches('@').trim();

            let supplement = convert_ref_supplement(node, ctx);

            if !label.is_empty() {
                ctx.push(&mark_at_reference(label, supplement.as_deref()));
                ctx.last_token = TokenType::Command;
            }
        }

        // Labels: <label> -> \label{label}
        SyntaxKind::Label => {
            let text = node.text().to_string();
            let label = normalize_label_like_text(&text);
            if !label.is_empty() {
                ctx.push(&label_to_latex(&label));
                ctx.last_token = TokenType::Command;
            }
        }

        // Primitive types - output their text representation
        // These may appear when MiniEval evaluates expressions to primitives
        SyntaxKind::Int => {
            let text = node.text().to_string();
            ctx.push(&text);
            ctx.last_token = TokenType::Number;
        }

        SyntaxKind::Float => {
            let text = node.text().to_string();
            ctx.push(&text);
            ctx.last_token = TokenType::Number;
        }

        SyntaxKind::Numeric => {
            // Numeric includes lengths like "10pt", "2em", etc.
            let text = node.text().to_string();
            ctx.push(&escape_latex_text(&text));
            ctx.last_token = TokenType::Number;
        }

        SyntaxKind::Str => {
            // String literals - output without quotes (content only)
            let text = node.text().to_string();
            // Remove surrounding quotes if present
            let content = text
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'');
            push_escaped_latex_text(ctx, content);
            ctx.last_token = TokenType::Text;
        }

        SyntaxKind::Bool => {
            // Boolean values
            let text = node.text().to_string();
            ctx.push(&text);
            ctx.last_token = TokenType::Letter;
        }

        SyntaxKind::None => {
            // None value - don't output anything
        }

        // `#include "sections/child.typ"` -> `\subimport{sections/}{child}`.
        //
        // This preserves the multi-file structure of a project instead of
        // inlining the included content, which matters for batch/directory
        // conversion where each `.typ` becomes a sibling `.tex`.
        //
        // `\subimport{dir/}{file}` (from the `import` package) is used rather
        // than `\input` because Typst resolves `#include` paths relative to the
        // *current* file, whereas LaTeX `\input` resolves relative to the main
        // document. `\subimport` maintains a per-file directory stack, so a
        // nested include inside `child` that references its own siblings still
        // resolves correctly. The `.typ` extension is dropped (LaTeX appends
        // `.tex`); a same-directory include uses `./` as its base.
        SyntaxKind::ModuleInclude => {
            if let Some(path) = module_include_path(node) {
                let stem = path.strip_suffix(".typ").unwrap_or(&path);
                if !stem.is_empty() {
                    let (dir, file) = match stem.rfind('/') {
                        Some(i) => (&stem[..=i], &stem[i + 1..]),
                        None => ("./", stem),
                    };
                    ctx.push(&format!("\\subimport{{{}}}{{{}}}", dir, file));
                    ctx.last_token = TokenType::Command;
                }
            }
        }

        _ => {
            // Recursively process children
            let child_count = node.children().count();
            if child_count > 0 {
                for child in node.children() {
                    convert_markup_node(child, ctx);
                }
            }
        }
    }
}

/// Literal path of a `#include "..."` (`ModuleInclude`). `None` for a
/// non-literal target (e.g. a variable), which is skipped rather than emitted
/// as garbage text.
fn module_include_path(node: &SyntaxNode) -> Option<String> {
    let raw = node
        .children()
        .find(|c| c.kind() == SyntaxKind::Str)?
        .text()
        .to_string();
    let path = raw
        .trim_start_matches('"')
        .trim_end_matches('"')
        .trim_start_matches('\'')
        .trim_end_matches('\'')
        .trim();
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// Convert Typst function calls to LaTeX (in markup mode)
pub fn convert_func_call_markup(node: &SyntaxNode, ctx: &mut ConvertContext) {
    let children: Vec<_> = node.children().collect();
    if children.is_empty() {
        return;
    }

    if let Some((term, description)) = term_item_parts(node) {
        emit_single_ast_term_item(term, description, ctx);
        return;
    }

    let func_name = children[0].text().to_string();

    if func_name == REF_ANCHOR_ELEMENT_NAME {
        let args = FuncArgs::from_func_call(&children);
        let counter = args
            .named_text("counter")
            .map(|s| s.trim().trim_matches('"'))
            .unwrap_or("");
        let step = args.named_bool("step").unwrap_or(true);
        let label = args
            .named("label")
            .map(normalize_label_like_text)
            .filter(|l| !l.is_empty());
        if step && !counter.is_empty() {
            emit_refstepcounter(counter, ctx);
        }
        if let Some(lbl) = label {
            ctx.push(&label_to_latex(&lbl));
        }
        ctx.last_token = TokenType::Command;
        return;
    }

    if func_name == DUP_SNAPSHOT_NAME {
        let args = FuncArgs::from_func_call(&children);
        let counter = args
            .named_text("counter")
            .map(|s| s.trim().trim_matches('"'))
            .unwrap_or("");
        let origin_id = args.named_usize("id").unwrap_or(0);
        emit_dup_snapshot(counter, origin_id, ctx);
        ctx.last_token = TokenType::Command;
        return;
    }

    if func_name == DUP_ELEMENT_NAME {
        let args = FuncArgs::from_func_call(&children);
        let counter = args
            .named_text("counter")
            .map(|s| s.trim().trim_matches('"'))
            .unwrap_or("");
        let id = args.named_usize("id").unwrap_or(1);
        let origin_id = args.named_usize("origin").unwrap_or(0);
        emit_dup_counter_prefix(counter, id, origin_id, ctx);
        for arg in args.iter() {
            if arg.is_positional {
                if let Some(body) = arg.value_node {
                    convert_markup_node(body, ctx);
                }
            }
        }
        for child in children.iter().skip(2) {
            if child.kind() == SyntaxKind::ContentBlock {
                convert_markup_node(child, ctx);
            }
        }
        emit_dup_counter_suffix(counter, ctx);
        ctx.last_token = TokenType::Command;
        return;
    }

    // Check if this is a math function that needs $ wrapping
    if is_math_func_in_markup(&func_name) {
        ctx.in_math = true;
        ctx.push("$");
        super::math::convert_func_call(node, ctx);
        ctx.push("$");
        ctx.in_math = false;
        ctx.last_token = TokenType::Command;
        return;
    }

    // Try to use the handler map
    if let Some(handler) = TYPST_MARKUP_HANDLERS.get(func_name.as_str()) {
        match handler {
            MarkupHandler::Wrap { prefix, suffix } => {
                if func_name == "heading" && ctx.is_in_env(&EnvironmentContext::Table) {
                    let rendered =
                        render_in_table_cell(ctx, |inner| convert_func_args_text(&children, inner));
                    let title = collapse_table_text(&rendered);
                    if !title.is_empty() {
                        emit_table_cell_lines(vec![format!("\\textbf{{{title}}}")], ctx);
                    }
                } else {
                    ctx.push(prefix);
                    convert_func_args_text(&children, ctx);
                    ctx.push(suffix);
                }
            }
            MarkupHandler::Environment { name } => {
                if ctx.is_in_env(&EnvironmentContext::Table) {
                    convert_func_args_text(&children, ctx);
                } else {
                    ctx.ensure_paragraph_break();
                    ctx.push_line(&format!("\\begin{{{}}}", name));
                    ctx.indent_level += 1;
                    convert_func_args_text(&children, ctx);
                    ctx.indent_level -= 1;
                    ctx.push_line(&format!("\\end{{{}}}", name));
                }
            }
            MarkupHandler::PassThrough => {
                convert_func_args_text(&children, ctx);
            }
            MarkupHandler::Special => {
                // Handle special cases
                handle_special_markup_func(&func_name, &children, ctx);
            }
        }
    } else {
        // Unknown function, try to output content
        convert_func_args_text(&children, ctx);
    }

    ctx.last_token = TokenType::Command;
}

/// Handle special markup functions that need custom logic
fn handle_special_markup_func(func_name: &str, children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    match func_name {
        "image" => {
            convert_image_to_latex(children, ctx);
        }

        "table" => {
            convert_table_to_latex(children, ctx, true);
        }

        // `#grid` is the same cell model as `#table`, only unstroked. Treating
        // it as flowing content concatenated the cells into one blob.
        "grid" => {
            convert_table_to_latex(children, ctx, false);
        }

        "figure" => {
            convert_figure_to_latex(children, ctx);
        }

        "link" => {
            convert_link_to_latex(children, ctx);
        }

        "cite" => {
            convert_cite_to_latex(children, ctx);
        }

        "ref" => {
            convert_ref_to_latex(children, ctx);
        }

        "label" => {
            convert_label_to_latex(children, ctx);
        }

        "bibliography" => {
            convert_bibliography_to_latex(children, ctx);
        }

        "footnote" => {
            ctx.push("\\footnote{");
            convert_func_args_text(children, ctx);
            ctx.push("}");
        }

        "caption" => {
            ctx.push("\\caption{");
            convert_func_args_text(children, ctx);
            ctx.push("}");
        }

        // Theorem-like environments
        "theorem" | "lemma" | "proposition" | "corollary" | "definition" | "example" | "remark"
        | "proof" => {
            convert_theorem_to_latex(func_name, children, ctx);
        }

        // Block quote with attribution
        "blockquote" => {
            ctx.ensure_paragraph_break();
            ctx.push_line("\\begin{quote}");
            convert_func_args_text(children, ctx);
            ctx.push_line("\\end{quote}");
        }

        // Text formatting
        "text" => {
            convert_text_func(children, ctx);
        }

        // Layout
        "pad" | "block" => {
            // For now, just output content without the box/padding wrapper to avoid "width inset" text
            // In the future, this could map to \parbox or minipage
            convert_func_args_text(children, ctx);
        }

        // Rotation
        "rotate" => {
            convert_rotate_func(children, ctx);
        }

        // Alignment
        "center" | "align" => {
            // Check arguments to see if it's align(center, ...) or just center(...)
            let mut alignment = "center";

            if func_name == "align" {
                if let Some(args) = children.get(1) {
                    let arg_children: Vec<_> = args.children().collect();
                    for child in arg_children.iter() {
                        if child.kind() == SyntaxKind::Ident {
                            let text = child.text().to_string();
                            if text == "left" || text == "start" {
                                alignment = "flushleft";
                            } else if text == "right" || text == "end" {
                                alignment = "flushright";
                            } else if text == "center" {
                                alignment = "center";
                            }
                        }
                    }
                }
            }

            let in_table = ctx.is_in_env(&EnvironmentContext::Table);
            if !in_table {
                ctx.ensure_paragraph_break();
                ctx.push_line(&format!("\\begin{{{}}}", alignment));
            }

            // Custom args processing to skip the alignment parameter
            if let Some(args) = children.get(1) {
                let arg_children: Vec<_> = args.children().collect();
                for child in arg_children.iter() {
                    // Better approach: filter out the alignment keyword if we found one
                    let is_alignment_keyword = child.kind() == SyntaxKind::Ident
                        && (child.text() == "center"
                            || child.text() == "left"
                            || child.text() == "right"
                            || child.text() == "start"
                            || child.text() == "end");

                    if func_name == "align" && is_alignment_keyword {
                        continue;
                    }

                    if child.kind() != SyntaxKind::Comma
                        && child.kind() != SyntaxKind::Colon
                        && child.kind() != SyntaxKind::LeftParen
                        && child.kind() != SyntaxKind::RightParen
                    {
                        convert_markup_node(child, ctx);
                    }
                }
            }

            if !in_table {
                ctx.push_line(&format!("\\end{{{}}}", alignment));
            }
        }

        // Box/Frame
        "box" => {
            ctx.push("\\fbox{");
            convert_func_args_text(children, ctx);
            ctx.push("}");
        }

        // Rectangle
        "rect" => {
            convert_rect_func(children, ctx);
        }

        // `#columns(n)[..]` is a multi-column page layout, not a cell grid.
        "columns" => {
            convert_columns_to_latex(children, ctx);
        }

        // CeTZ graphics (canvas)
        "canvas" => {
            convert_cetz_to_latex(children, ctx);
        }

        // Headings
        "heading" => {
            convert_heading_func(children, ctx);
        }

        // Lists
        "list" => {
            convert_list_to_latex(children, ctx, false);
        }

        "enum" => {
            convert_list_to_latex(children, ctx, true);
        }

        "terms" => {
            convert_terms_func(children, ctx);
        }

        // Raw/verbatim blocks
        "raw" => {
            convert_raw_to_latex(children, ctx);
        }

        // Horizontal spacing: h(1fr) -> \hfill, h(1em) -> \hspace{1em}
        "h" => {
            if let Some(args) = children.get(1) {
                let arg_text = args.text().to_string();
                match parse_spacing_spec(&arg_text) {
                    Some(SpacingSpec::Flex(_)) => ctx.push("\\hfill"),
                    Some(SpacingSpec::Fixed(value)) => {
                        ctx.push(&format!("\\hspace{{{}}}", value));
                    }
                    None => ctx.push("\\hfill"),
                }
            }
        }

        // Vertical spacing: v(1em) -> \vspace{1em}
        "v" => {
            if let Some(args) = children.get(1) {
                let arg_text = args.text().to_string();
                if arg_text.contains("fr") {
                    ctx.push("\\vfill");
                } else if let Some(value) = extract_length_value(&arg_text) {
                    ctx.push(&format!("\\vspace{{{}}}", value));
                } else {
                    ctx.push("\\vspace{1em}");
                }
            }
        }

        _ => {
            // Fallback: just output content
            convert_func_args_text(children, ctx);
        }
    }
}

// ============================================================================
// Text Function Conversion
// ============================================================================

fn convert_text_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let weight = args.named_text("weight").map(str::to_string);
    let size = args.named_text("size").map(str::to_string);
    let style = args.named_text("style").map(str::to_string);
    let mut color = args
        .named_color("fill")
        .and_then(normalize_typst_color_expr);
    let mut content_nodes = Vec::new();

    for arg in args.iter().filter(|arg| arg.is_positional) {
        match arg.node.kind() {
            SyntaxKind::ContentBlock | SyntaxKind::Markup | SyntaxKind::Str => {
                content_nodes.push(arg.node);
            }
            SyntaxKind::Ident => {
                if let Some(normalized) = normalize_typst_color_expr(&arg.value) {
                    color = Some(normalized);
                }
            }
            SyntaxKind::FuncCall => {
                if let Some(normalized) = normalize_typst_color_expr(&arg.value) {
                    color = Some(normalized);
                } else {
                    content_nodes.push(arg.node);
                }
            }
            _ => content_nodes.push(arg.node),
        }
    }

    // Apply styling wrappers
    let mut suffix_count = 0;

    // Apply color first (outermost wrapper)
    if let Some(c) = &color {
        ctx.push(&format_latex_color_command("textcolor", c));
        ctx.push("{");
        suffix_count += 1;
    }

    if let Some(w) = weight {
        if w == "\"bold\"" || w == "bold" || w == "extrabold" || w == "black" {
            ctx.push("\\textbf{");
            suffix_count += 1;
        }
    }

    if let Some(s) = style {
        if s == "\"italic\"" || s == "italic" {
            ctx.push("\\textit{");
            suffix_count += 1;
        }
    }

    if let Some(s) = size {
        // Simple heuristic for size mapping
        if s.contains("pt") {
            if let Ok(pt) = s.trim_end_matches("pt").trim().parse::<f64>() {
                if pt >= 20.0 {
                    ctx.push("{\\Huge ");
                } else if pt >= 17.0 {
                    ctx.push("{\\huge ");
                } else if pt >= 14.0 {
                    ctx.push("{\\Large ");
                } else if pt >= 12.0 {
                    ctx.push("{\\large ");
                } else if pt <= 8.0 {
                    ctx.push("{\\small ");
                } else {
                    ctx.push("{");
                }
                suffix_count += 1;
            }
        }
    }

    // Output content
    if content_nodes.is_empty() {
        // Might be just setting style without content, or content is in a later block
        // For now, do nothing
    } else {
        for node in content_nodes {
            convert_markup_node(node, ctx);
        }
    }

    // Close wrappers
    for _ in 0..suffix_count {
        ctx.push("}");
    }
}

/// Convert #rotate(angle)[content] to \rotatebox{angle}{content}
fn convert_rotate_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let mut angle = args.named_angle("angle");
    let mut content_nodes = Vec::new();

    for arg in args.iter().filter(|arg| arg.is_positional) {
        match arg.node.kind() {
            SyntaxKind::ContentBlock | SyntaxKind::Markup => content_nodes.push(arg.node),
            SyntaxKind::Unary => {
                angle = parse_angle_value(&arg.value);
            }
            _ => {
                if arg.value.contains("deg")
                    || arg.value.contains("rad")
                    || arg.value.parse::<f64>().is_ok()
                {
                    angle = parse_angle_value(&arg.value);
                }
            }
        }
    }

    // Output \rotatebox{angle}{content}
    let angle_deg = angle.unwrap_or(0.0);
    ctx.push(&format!("\\rotatebox{{{}}}", angle_deg));
    ctx.push("{");

    for node in content_nodes {
        convert_markup_node(node, ctx);
    }

    ctx.push("}");
}

/// Convert #rect(...)[content] to appropriate LaTeX
/// - If has content with fill: use \colorbox (preserves content)
/// - If no content with fill and height: use \rule (solid rectangle)
/// - Otherwise, use \fbox
fn convert_rect_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);

    let width = args.named("width");
    let height = args.named("height");
    let fill = args.named("fill").and_then(normalize_typst_color_expr);

    // Get content nodes
    let mut content_nodes = Vec::new();
    if let Some(args_node) = children.get(1) {
        for child in args_node.children() {
            if matches!(child.kind(), SyntaxKind::ContentBlock | SyntaxKind::Markup) {
                content_nodes.push(child);
            }
        }
    }

    let has_content = !content_nodes.is_empty();

    // Determine the best LaTeX representation
    if let Some(f) = fill.as_deref() {
        let colorbox = format_latex_color_command("colorbox", f);
        let color_cmd = format_latex_color_command("color", f);

        if has_content {
            // Has content - must use \colorbox to preserve it
            // Optionally wrap in minipage if width specified
            if let Some(w) = width {
                let latex_width = convert_dimension_to_latex(w);
                ctx.push(&colorbox);
                ctx.push(&format!("{{\\begin{{minipage}}{{{}}}", latex_width));
                for node in &content_nodes {
                    convert_markup_node(node, ctx);
                }
                ctx.push("\\end{minipage}}");
            } else {
                ctx.push(&colorbox);
                ctx.push("{");
                for node in &content_nodes {
                    convert_markup_node(node, ctx);
                }
                ctx.push("}");
            }
        } else if let Some(h) = height {
            // No content, has height - use \rule for solid rectangle
            let latex_height = convert_dimension_to_latex(h);
            let latex_width = width
                .map(convert_dimension_to_latex)
                .unwrap_or_else(|| "\\linewidth".to_string());
            ctx.push(&format!(
                "{{{}\\rule{{{}}}{{{}}}}}",
                color_cmd, latex_width, latex_height
            ));
        } else {
            // Has fill but no height and no content - just colorbox with empty
            ctx.push(&colorbox);
            ctx.push("{}");
        }
    } else if let Some(h) = height {
        // No fill, but has height - black rule
        let latex_height = convert_dimension_to_latex(h);
        let latex_width = width
            .map(convert_dimension_to_latex)
            .unwrap_or_else(|| "\\linewidth".to_string());
        ctx.push(&format!("\\rule{{{}}}{{{}}}", latex_width, latex_height));
    } else {
        // Default - use \fbox
        ctx.push("\\fbox{");
        for node in content_nodes {
            convert_markup_node(node, ctx);
        }
        ctx.push("}");
    }
}

fn convert_image_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    // Use FuncArgs for unified argument parsing
    let args = FuncArgs::from_func_call(children);

    // Get path from first positional argument
    let path = args.first().unwrap_or("").trim_matches('"').to_string();

    // Get optional width and height
    let width = args.named("width").map(convert_dimension_to_latex);
    let height = args.named("height").map(convert_dimension_to_latex);

    // Build \includegraphics command
    ctx.push("\\includegraphics");

    // Add options if present
    let mut options = Vec::new();
    if let Some(w) = width {
        options.push(format!("width={}", w));
    }
    if let Some(h) = height {
        options.push(format!("height={}", h));
    }

    if !options.is_empty() {
        ctx.push("[");
        ctx.push(&options.join(", "));
        ctx.push("]");
    }

    ctx.push("{");
    ctx.push(&path);
    ctx.push("}");
}

/// Convert Typst dimension to LaTeX dimension
fn convert_dimension_to_latex(dim: &str) -> String {
    let dim = dim.trim();
    if dim.ends_with('%') {
        // Convert percentage to \textwidth
        let percent = dim.trim_end_matches('%');
        if let Ok(p) = percent.parse::<f64>() {
            return format!("{:.2}\\textwidth", p / 100.0);
        }
    }
    // Pass through other dimensions (pt, cm, mm, in, em)
    dim.to_string()
}

// ============================================================================
// Enhanced Table Conversion
// ============================================================================

/// Get the full function name from a FuncCall's first child, handling FieldAccess
fn get_func_call_name(node: &SyntaxNode) -> String {
    match node.kind() {
        SyntaxKind::FieldAccess => {
            // Collect parts from FieldAccess: table.header -> "table.header"
            let mut parts = Vec::new();
            for child in node.children() {
                match child.kind() {
                    SyntaxKind::FieldAccess => {
                        parts.push(get_func_call_name(child));
                    }
                    SyntaxKind::Ident | SyntaxKind::MathIdent => {
                        parts.push(child.text().to_string());
                    }
                    SyntaxKind::Dot => {
                        // Skip dots, we'll join with them
                    }
                    _ => {
                        let text = child.text().to_string();
                        if !text.is_empty() && text != "." {
                            parts.push(text);
                        }
                    }
                }
            }
            parts.join(".")
        }
        _ => node.text().to_string(),
    }
}

/// Convert a Typst table to LaTeX using the state-aware table generator
fn convert_table_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext, ruled: bool) {
    let args = FuncArgs::from_func_call(children);
    let column_value = args.named_text("columns");
    let inferred_columns = column_value.and_then(infer_table_columns);
    let unresolved_columns = column_value.is_some() && inferred_columns.is_none();
    let mut columns = inferred_columns.unwrap_or(0);
    let mut col_aligns: Vec<LatexCellAlign> = Vec::new();
    let mut cells: Vec<LatexCell> = Vec::new();
    let mut hlines: Vec<(usize, LatexHLine)> = Vec::new(); // (cell_index, hline)
    let mut in_header = false;
    let mut header_end_idx: Option<usize> = None;
    let mut cells_complete = true;
    // Cell warnings apply only if their table is actually rendered.
    let warnings_before_cells = ctx.warnings.len();

    if let Some(args_node) = children.get(1) {
        cells_complete = !args_node.erroneous();
        for child in args_node.children() {
            if let Some((rule, call)) = table_rule(child) {
                if rule == "hline" {
                    hlines.push((cells.len(), LatexHLine::from_typst_ast(call)));
                }
                continue;
            }
            match child.kind() {
                SyntaxKind::Named => {
                    if let Some(arg) = args.arg_for_node(child) {
                        if let Some("align") = arg.name.as_deref() {
                            col_aligns = parse_typst_align(&arg.value);
                        }
                    }
                }
                // Each occupies one positional cell even when empty (`[]`);
                // dropping one shifts every later cell left.
                SyntaxKind::ContentBlock | SyntaxKind::Markup | SyntaxKind::Equation => {
                    let mut cell_ctx = ConvertContext::new();
                    cell_ctx.push_env(EnvironmentContext::Table);
                    convert_markup_node(child, &mut cell_ctx);
                    let content = cell_ctx.finish_into(ctx);
                    let mut cell = LatexCell::new(content);
                    cell.is_header = in_header;
                    cells.push(cell);
                }
                SyntaxKind::FuncCall => {
                    let func_children: Vec<_> = child.children().collect();
                    if !func_children.is_empty() {
                        let func_name = get_func_call_name(func_children[0]);

                        if func_name.contains("header") {
                            // Process header cells - can contain table.cell() or plain content
                            if let Some(func_args) = func_children.get(1) {
                                for header_child in func_args.children() {
                                    if table_rule(header_child).is_some() {
                                        continue;
                                    }
                                    match header_child.kind() {
                                        SyntaxKind::ContentBlock
                                        | SyntaxKind::Markup
                                        | SyntaxKind::Equation => {
                                            // Plain content like [Type A] or $x$
                                            let mut cell_ctx = ConvertContext::new();
                                            cell_ctx.push_env(EnvironmentContext::Table);
                                            convert_markup_node(header_child, &mut cell_ctx);
                                            let content = cell_ctx.finish_into(ctx);
                                            let mut cell = LatexCell::new(content);
                                            cell.is_header = true;
                                            cells.push(cell);
                                        }
                                        SyntaxKind::FuncCall => {
                                            // table.cell(...) inside header
                                            let inner_func_children: Vec<_> =
                                                header_child.children().collect();
                                            if !inner_func_children.is_empty() {
                                                let inner_func_name =
                                                    get_func_call_name(inner_func_children[0]);
                                                if inner_func_name.contains("cell") {
                                                    let (cell, complete) =
                                                        LatexCell::from_typst_cell_ast(
                                                            header_child,
                                                            ctx,
                                                        );
                                                    cells_complete &= complete;
                                                    if let Some(mut cell) = cell {
                                                        cell.is_header = true;
                                                        cells.push(cell);
                                                    }
                                                } else {
                                                    // Other function call (e.g., text(...))
                                                    let mut cell_ctx = ConvertContext::new();
                                                    cell_ctx.push_env(EnvironmentContext::Table);
                                                    convert_markup_node(
                                                        header_child,
                                                        &mut cell_ctx,
                                                    );
                                                    let content = cell_ctx.finish_into(ctx);
                                                    if !content.is_empty() {
                                                        let mut cell = LatexCell::new(content);
                                                        cell.is_header = true;
                                                        cells.push(cell);
                                                    } else {
                                                        cells_complete = false;
                                                    }
                                                }
                                            } else {
                                                cells_complete = false;
                                            }
                                        }
                                        _ if is_none_value(header_child) => {
                                            let mut cell = LatexCell::new(String::new());
                                            cell.is_header = true;
                                            cells.push(cell);
                                        }
                                        _ if is_empty_spread(header_child) => {}
                                        _ if matches!(
                                            header_child.cast::<ast::Arg>(),
                                            Some(ast::Arg::Pos(_) | ast::Arg::Spread(_))
                                        ) =>
                                        {
                                            cells_complete = false;
                                        }
                                        _ => {}
                                    }
                                }
                            } else {
                                cells_complete = false;
                            }
                            header_end_idx = Some(cells.len());
                            in_header = false;
                        } else if func_name.contains("cell") {
                            let (cell, complete) = LatexCell::from_typst_cell_ast(child, ctx);
                            cells_complete &= complete;
                            if let Some(cell) = cell {
                                cells.push(cell);
                            }
                        } else if func_name.contains("hline") {
                            let hline = LatexHLine::from_typst_ast(child);
                            hlines.push((cells.len(), hline));
                        } else if func_name == "align" {
                            // Special case: align() inside table should extract alignment property
                            // instead of creating \begin{center} environment
                            if let Some(args) =
                                child.children().find(|c| c.kind() == SyntaxKind::Args)
                            {
                                // Default alignment
                                let mut alignment = LatexCellAlign::Center;
                                let mut content_node = None;

                                for arg in args.children() {
                                    if arg.kind() == SyntaxKind::ContentBlock {
                                        content_node = Some(arg.clone());
                                    } else if arg.kind() != SyntaxKind::Comma
                                        && arg.kind() != SyntaxKind::LeftParen
                                        && arg.kind() != SyntaxKind::RightParen
                                    {
                                        // This is likely the alignment argument
                                        // Simplified check for "left", "right" in the text
                                        let text = arg.text().to_string();
                                        if text.contains("left") || text.contains("start") {
                                            alignment = LatexCellAlign::Left;
                                        } else if text.contains("right") || text.contains("end") {
                                            alignment = LatexCellAlign::Right;
                                        } else if text.contains("center") {
                                            alignment = LatexCellAlign::Center;
                                        }
                                    }
                                }

                                if let Some(content_arg) = content_node {
                                    let mut cell_ctx = ConvertContext::new();
                                    cell_ctx.push_env(EnvironmentContext::Table);
                                    convert_markup_node(&content_arg, &mut cell_ctx);
                                    let content = cell_ctx.finish_into(ctx);

                                    let mut cell = LatexCell::new(content);
                                    cell.align = Some(alignment);
                                    cells.push(cell);
                                } else {
                                    cells_complete = false;
                                }
                            } else {
                                cells_complete = false;
                            }
                        } else {
                            // Regular cell (other function call)
                            let mut cell_ctx = ConvertContext::new();
                            cell_ctx.push_env(EnvironmentContext::Table);
                            convert_markup_node(child, &mut cell_ctx);
                            let content = cell_ctx.finish_into(ctx);
                            if !content.is_empty() {
                                cells.push(LatexCell::new(content));
                            } else {
                                cells_complete = false;
                            }
                        }
                    } else {
                        cells_complete = false;
                    }
                }
                // Handle string arguments as table cells (from MiniEval expansion)
                SyntaxKind::Str => {
                    let text = child.text().to_string();
                    // Remove quotes from string literal
                    let content = text.trim_matches('"').to_string();
                    cells.push(LatexCell::new(content));
                }
                // Handle other expression types as table cells
                SyntaxKind::Int | SyntaxKind::Float | SyntaxKind::Bool => {
                    let content = child.text().to_string();
                    if !content.is_empty() {
                        cells.push(LatexCell::new(content));
                    }
                }
                _ if is_none_value(child) => cells.push(LatexCell::new(String::new())),
                _ if is_empty_spread(child) => {}
                _ if child.cast::<ast::Arg>().is_some() => {
                    cells_complete = false;
                }
                _ => {
                    // Ignore punctuation / structural tokens
                }
            }
        }
    }

    // Reuse the existing fallback only when no positional cells were dropped.
    if columns == 0 && cells_complete {
        columns = if cells.len() >= 4 {
            (cells.len() as f64).sqrt().ceil() as usize
        } else {
            cells.len().max(1)
        };
    }

    if columns > MAX_TABLE_COLUMNS {
        ctx.warnings.truncate(warnings_before_cells);
        preserve_unconverted_layout(
            &format!("column count {columns} exceeds the supported limit of {MAX_TABLE_COLUMNS}"),
            children,
            ctx,
        );
        return;
    }

    if !cells_complete && (inferred_columns.is_none() || cells.is_empty()) {
        ctx.warnings.truncate(warnings_before_cells);
        preserve_unconverted_layout("could not determine all table cells", children, ctx);
        return;
    }
    if !cells_complete {
        report_layout_warning(
            "some table cells could not be converted; output may be incomplete or misaligned",
            children,
            ctx,
        );
    } else if unresolved_columns {
        report_layout_warning(
            &format!(
                "could not determine column count; using an approximate {columns}-column layout"
            ),
            children,
            ctx,
        );
    }

    // Pad col_aligns to match column count
    while col_aligns.len() < columns {
        col_aligns.push(LatexCellAlign::Center);
    }

    // Create the table generator
    let mut generator = LatexTableGenerator::new(columns, col_aligns);
    if !ruled {
        generator = generator.without_rules();
    }

    // Process cells row by row
    let mut current_row: Vec<LatexCell> = Vec::new();
    let mut cell_idx = 0;
    let mut col_in_row = 0;

    for cell in cells {
        // Check if there's an hline before this cell
        for (hline_idx, hline) in &hlines {
            if *hline_idx == cell_idx {
                generator.add_hline(hline.clone());
            }
        }

        // Check if this starts the header
        if cell_idx == 0 && cell.is_header {
            generator.begin_header();
        }

        current_row.push(cell.clone());
        col_in_row += cell.colspan;

        // Calculate effective columns needed for this row
        // (accounting for columns covered by previous rowspans)
        let covered_cols = generator.get_covered_columns();
        let effective_cols_needed = columns.saturating_sub(covered_cols);

        // If we've filled a row, process it
        if col_in_row >= effective_cols_needed {
            generator.process_row(current_row);
            current_row = Vec::new();
            col_in_row = 0;

            // Check if header ended
            if let Some(end_idx) = header_end_idx {
                if cell_idx + 1 >= end_idx {
                    generator.end_header();
                }
            }
        }

        cell_idx += 1;
    }

    // Process any remaining cells
    if !current_row.is_empty() {
        generator.process_row(current_row);
    }

    // Check for trailing hlines
    for (hline_idx, hline) in &hlines {
        if *hline_idx >= cell_idx {
            generator.add_hline(hline.clone());
        }
    }

    ctx.ensure_paragraph_break();
    ctx.push(&generator.generate_latex());
}

/// Parse Typst align specification to LaTeX column alignments
fn parse_typst_align(value: &str) -> Vec<LatexCellAlign> {
    // If it's a function (contains "=>"), we can't parse it easily, so return empty
    // The generator will fallback to default alignments
    if value.contains("=>") {
        return Vec::new();
    }

    let inner = value
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    if inner.is_empty() {
        return Vec::new();
    }

    inner
        .split(',')
        .map(|s| LatexCellAlign::from_typst(s.trim()))
        .collect()
}

/// Report to structured diagnostics and plain-string callers.
fn report_layout_warning(reason: &str, children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let name = children
        .first()
        .map(|node| get_func_call_name(node))
        .unwrap_or_default();
    let warning = format!("{name}: {reason}");
    ctx.add_warning(warning.clone());
    ctx.ensure_paragraph_break();
    ctx.push_line(&format!("% Tylax: {warning}"));
}

/// Preserve source when a table cannot be converted safely.
fn preserve_unconverted_layout(reason: &str, children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    report_layout_warning(
        &format!("{reason}; layout not converted, Typst retained as comments below"),
        children,
        ctx,
    );
    let source: String = children
        .iter()
        .map(|node| (**node).clone().into_text().to_string())
        .collect();
    preserve_typst_source(&format!("#{source}"), ctx);
    if ctx.is_in_env(&EnvironmentContext::Table) {
        ctx.push("{}");
    }
}

// ============================================================================
// Enhanced Figure Conversion
// ============================================================================

fn unwrap_paren_syntax_node(mut node: &SyntaxNode) -> &SyntaxNode {
    while node.kind() == SyntaxKind::Parenthesized {
        let Some(inner) = node.children().find(|c| {
            !matches!(
                c.kind(),
                SyntaxKind::LeftParen
                    | SyntaxKind::RightParen
                    | SyntaxKind::Space
                    | SyntaxKind::Linebreak
                    | SyntaxKind::Parbreak
                    | SyntaxKind::LineComment
                    | SyntaxKind::BlockComment
            )
        }) else {
            break;
        };
        node = inner;
    }
    node
}

fn process_figure_positional_body(
    child: &SyntaxNode,
    ctx: &mut ConvertContext,
    inferred_is_table: &mut bool,
    content: &mut String,
) {
    let inner = unwrap_paren_syntax_node(child);
    if node_contains_call(inner, &["table", "grid"]) {
        *inferred_is_table = true;
    }
    let mut content_ctx = ConvertContext::new();
    content_ctx.options = ctx.options.clone();
    convert_markup_node(inner, &mut content_ctx);
    let c = content_ctx.finish_into(ctx);
    if !c.is_empty() {
        *content = c;
    }
}

fn convert_figure_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut caption: Option<String> = None;
    let mut label: Option<String> = None;
    let mut explicit_supplement: Option<String> = None;
    let mut content = String::new();
    let mut inferred_is_table = false;

    let func_args = FuncArgs::from_func_call(children);
    #[derive(Clone, PartialEq, Eq)]
    enum ExplicitFigureKind {
        Auto,
        Image,
        Table,
        Custom(String),
    }
    let explicit_kind = func_args
        .iter()
        .filter(|arg| arg.name.as_deref() == Some("kind"))
        .last()
        .map(|arg| {
            let cleaned = unwrap_balanced_parens(arg.value.trim())
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            match cleaned {
                "" | "auto" => ExplicitFigureKind::Auto,
                "table" => ExplicitFigureKind::Table,
                "image" | "figure" => ExplicitFigureKind::Image,
                other => ExplicitFigureKind::Custom(other.to_string()),
            }
        });
    let unnumbered = match func_args
        .iter()
        .filter(|arg| arg.name.as_deref() == Some("numbering"))
        .last()
    {
        Some(arg) => {
            let trimmed = unwrap_balanced_parens(arg.value.trim());
            if trimmed == "auto" {
                active_figure_unnumbered()
            } else {
                trimmed == "none"
            }
        }
        None => active_figure_unnumbered(),
    };

    if let Some(args) = children.get(1) {
        for child in args.children() {
            match child.kind() {
                SyntaxKind::Named => {
                    let named_children: Vec<_> = child.children().collect();
                    if !named_children.is_empty() {
                        let key = named_children[0].text().to_string();
                        let value_node = named_children.iter().find(|n| {
                            n.kind() != SyntaxKind::Ident
                                && n.kind() != SyntaxKind::Colon
                                && n.kind() != SyntaxKind::Space
                        });
                        match key.as_str() {
                            "caption" => {
                                if let Some(value_node) = value_node {
                                    let unwrapped = unwrap_paren_syntax_node(value_node);
                                    if unwrapped.kind() == SyntaxKind::None
                                        || matches!(
                                            unwrap_balanced_parens(unwrapped.text().trim()),
                                            "none" | "auto"
                                        )
                                    {
                                        caption = None;
                                    } else {
                                        let mut cap_ctx = ConvertContext::new();
                                        cap_ctx.options = ctx.options.clone();
                                        convert_markup_node(unwrapped, &mut cap_ctx);
                                        caption = Some(cap_ctx.finish_into(ctx));
                                    }
                                }
                            }
                            "label" => {
                                if let Some(value_node) = value_node {
                                    label = Some(get_simple_text(value_node));
                                }
                            }
                            "supplement" => {
                                if let Some(value_node) = value_node {
                                    let unwrapped = unwrap_paren_syntax_node(value_node);
                                    let raw_trimmed =
                                        unwrap_balanced_parens(unwrapped.text().trim());
                                    if unwrapped.kind() == SyntaxKind::Auto || raw_trimmed == "auto"
                                    {
                                        explicit_supplement = None;
                                    } else if unwrapped.kind() == SyntaxKind::None
                                        || raw_trimmed == "none"
                                    {
                                        explicit_supplement = Some(String::new());
                                    } else {
                                        let mut supp_ctx = ConvertContext::new();
                                        supp_ctx.options = ctx.options.clone();
                                        convert_markup_node(unwrapped, &mut supp_ctx);
                                        explicit_supplement =
                                            Some(supp_ctx.finish_into(ctx).trim().to_string());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                SyntaxKind::LeftParen
                | SyntaxKind::RightParen
                | SyntaxKind::Comma
                | SyntaxKind::Space
                | SyntaxKind::Linebreak
                | SyntaxKind::Parbreak
                | SyntaxKind::LineComment
                | SyntaxKind::BlockComment => {}
                _ => {
                    process_figure_positional_body(
                        child,
                        ctx,
                        &mut inferred_is_table,
                        &mut content,
                    );
                }
            }
        }
    }
    for child in children.iter().skip(2) {
        if child.kind() == SyntaxKind::ContentBlock {
            process_figure_positional_body(child, ctx, &mut inferred_is_table, &mut content);
        }
    }

    enum RenderedFigureKind {
        Image,
        Table,
        Custom { counter: String, supplement: String },
    }
    let rendered_kind = match explicit_kind {
        None | Some(ExplicitFigureKind::Auto) => {
            if inferred_is_table {
                RenderedFigureKind::Table
            } else {
                RenderedFigureKind::Image
            }
        }
        Some(ExplicitFigureKind::Table) => RenderedFigureKind::Table,
        Some(ExplicitFigureKind::Image) => RenderedFigureKind::Image,
        Some(ExplicitFigureKind::Custom(raw_kind)) => RenderedFigureKind::Custom {
            counter: custom_figure_counter_name(&raw_kind),
            supplement: default_custom_figure_supplement(&raw_kind),
        },
    };

    ctx.ensure_paragraph_break();
    let env_name = match rendered_kind {
        RenderedFigureKind::Table => "table",
        RenderedFigureKind::Image | RenderedFigureKind::Custom { .. } => "figure",
    };
    ctx.push_line(&format!("\\begin{{{}}}[htbp]", env_name));
    ctx.push_line("\\centering");

    if !content.is_empty() {
        ctx.push("  ");
        ctx.push(&content);
        ctx.newline();
    }

    let clean_caption = caption.as_deref().map(|cap| {
        cap.trim()
            .trim_start_matches("\\{")
            .trim_end_matches("\\}")
            .trim()
            .to_string()
    });

    match rendered_kind {
        RenderedFigureKind::Custom {
            counter,
            supplement,
        } => {
            let supp_label = explicit_supplement.unwrap_or(supplement);
            if !unnumbered {
                ctx.push_line(&format!(
                    "  \\ifcsname c@{counter}\\endcsname\\else\\newcounter{{{counter}}}\\fi"
                ));
                ctx.push_line(&format!("  \\refstepcounter{{{counter}}}"));
                if let Some(ref clean_cap) = clean_caption {
                    if !clean_cap.is_empty() {
                        if supp_label.is_empty() {
                            ctx.push_line(&format!(
                                "  \\par\\smallskip \\the{counter}: {clean_cap}"
                            ));
                        } else {
                            ctx.push_line(&format!(
                                "  \\par\\smallskip {supp_label}~\\the{counter}: {clean_cap}"
                            ));
                        }
                    }
                }
            } else if let Some(ref clean_cap) = clean_caption {
                if !clean_cap.is_empty() {
                    ctx.push_line(&format!("  \\par\\smallskip {clean_cap}"));
                }
            }
        }
        RenderedFigureKind::Image | RenderedFigureKind::Table => {
            if let Some(ref clean_cap) = clean_caption {
                if unnumbered {
                    if !clean_cap.is_empty() {
                        ctx.push_line(&format!("  \\par\\smallskip {clean_cap}"));
                    }
                } else {
                    ctx.push("  \\caption{");
                    ctx.push(clean_cap);
                    ctx.push("}\n");
                }
            } else if !unnumbered {
                ctx.push_line(&format!("  \\refstepcounter{{{env_name}}}"));
            }
        }
    }

    // Use label from argument, or from pending_label (set by parent when processing figure + <label>)
    let final_label = label.or_else(|| ctx.pending_label.clone());
    if let Some(lbl) = final_label {
        ctx.push("  \\label{");
        ctx.push(lbl.trim_start_matches('<').trim_end_matches('>'));
        ctx.push("}\n");
    }

    ctx.push_line(&format!("\\end{{{}}}", env_name));
}

// ============================================================================
// Link Conversion
// ============================================================================

fn convert_link_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut url = String::new();
    let mut text = String::new();

    if let Some(args) = children.get(1) {
        let mut first_str = true;
        for child in args.children() {
            if is_string_or_content(child.kind()) {
                let content = get_string_content(child);
                if first_str {
                    url = content;
                    first_str = false;
                } else {
                    text = content;
                }
            } else if child.kind() == SyntaxKind::ContentBlock {
                let mut text_ctx = ConvertContext::new();
                convert_markup_node(child, &mut text_ctx);
                text = text_ctx.finish_into(ctx);
            }
        }
    }

    if text.is_empty() {
        ctx.push("\\url{");
        ctx.push(&url);
        ctx.push("}");
    } else {
        ctx.push("\\href{");
        ctx.push(&url);
        ctx.push("}{");
        ctx.push(&text);
        ctx.push("}");
    }
}

// ============================================================================
// Citation and Reference Conversion
// ============================================================================

fn normalize_label_like_text(text: &str) -> String {
    text.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn normalize_citation_note_text(text: &str) -> String {
    text.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn extract_label_like_from_node(node: &SyntaxNode) -> Option<String> {
    match node.kind() {
        SyntaxKind::Label => Some(normalize_label_like_text(node.text().as_ref())),
        SyntaxKind::Str => Some(get_string_content(node)),
        _ => {
            let text = get_simple_text(node);
            let normalized = normalize_label_like_text(&text);
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        }
    }
}

fn convert_cite_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let mut group = CiteGroup::new();
    let mode = citation_mode_from_typst_form(args.named_text("form"));
    group.suffix = args
        .named_text("supplement")
        .map(normalize_citation_note_text)
        .filter(|value| !value.is_empty());

    for index in 0..args.positional_count() {
        if let Some(node) = args.positional_node(index) {
            if let Some(key) = extract_label_like_from_node(node) {
                group.push(Citation::with_mode(key, mode));
            }
        }
    }

    if group.citations.is_empty() {
        return;
    }

    ctx.push(&citation_to_latex(&group));
}

fn convert_ref_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    if let Some(node) = args.first_node() {
        if let Some(target) = extract_label_like_from_node(node) {
            ctx.push(&mark_reference(&reference_to_latex(&Reference::new(
                target,
            ))));
        }
    }
}

fn convert_label_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    if let Some(node) = args.first_node() {
        if let Some(label) = extract_label_like_from_node(node) {
            ctx.push(&label_to_latex(&label));
        }
    }
}

fn convert_bibliography_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let mut bib_file = String::new();
    let style = args.named_text("style").unwrap_or("plain").to_string();

    if let Some(first_node) = args.first_node() {
        if first_node.kind() == SyntaxKind::Str {
            bib_file = get_string_content(first_node);
        }
    }

    // Remove .yml or .bib extension if present
    let bib_name = bib_file
        .trim_end_matches(".yml")
        .trim_end_matches(".yaml")
        .trim_end_matches(".bib");

    ctx.ensure_paragraph_break();
    ctx.push_line(&format!("\\bibliographystyle{{{}}}", style));
    ctx.push_line(&format!("\\bibliography{{{}}}", bib_name));
}

// ============================================================================
// Theorem Environment Conversion
// ============================================================================

fn convert_theorem_to_latex(env_name: &str, children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let latex_env = env_name;

    ctx.ensure_paragraph_break();
    ctx.push_line(&format!("\\begin{{{}}}", latex_env));
    ctx.indent_level += 1;
    convert_func_args_text(children, ctx);
    ctx.indent_level -= 1;
    ctx.push_line(&format!("\\end{{{}}}", latex_env));
}

// ============================================================================
// Grid/Columns Conversion
// ============================================================================

fn convert_columns_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let num_cols = args.named_usize("columns").map(|n| n.max(1)).unwrap_or(2);

    // Calculate column width
    let col_width = if num_cols > 0 {
        0.95 / num_cols as f64
    } else {
        0.48
    };

    ctx.ensure_paragraph_break();
    ctx.push_line(&format!(
        "\\begin{{minipage}}[t]{{{:.2}\\textwidth}}",
        col_width
    ));
    convert_func_args_text(children, ctx);
    ctx.push_line("\\end{minipage}");
}

/// Convert function arguments as text content, ignoring named argument keys
pub fn convert_func_args_text(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);

    for arg in args.iter() {
        if arg.is_positional {
            if let Some(node) = arg.value_node {
                convert_markup_node(node, ctx);
            }
            continue;
        }

        if let Some(node) = arg.value_node {
            if node.kind() == SyntaxKind::ContentBlock || node.kind() == SyntaxKind::Markup {
                convert_markup_node(node, ctx);
            }
        }
    }

    for child in children.iter().skip(1) {
        if child.kind() == SyntaxKind::ContentBlock {
            for arg_child in child.children() {
                convert_markup_node(arg_child, ctx);
            }
        }
    }
}

// ============================================================================
// CeTZ to TikZ Conversion
// ============================================================================

fn convert_cetz_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    // Extract the CeTZ code from the content block
    let mut cetz_code = String::new();

    if let Some(args) = children.get(1) {
        for child in args.children() {
            if child.kind() == SyntaxKind::ContentBlock {
                // Get the raw text content
                cetz_code = child.text().to_string();
                break;
            }
        }
    }

    if cetz_code.is_empty() {
        // Fallback: try to get all text
        for child in children.iter().skip(1) {
            cetz_code.push_str(child.text().as_ref());
        }
    }

    // Check if it looks like CeTZ code
    if is_cetz_code(&cetz_code) {
        ctx.ensure_paragraph_break();
        let tikz_code = convert_cetz_to_tikz(&cetz_code);
        ctx.push(&tikz_code);
        ctx.newline();
    } else {
        // Not CeTZ, just output as-is
        ctx.add_warning("Canvas content not recognized as CeTZ");
        convert_func_args_text(children, ctx);
    }
}

// ============================================================================
// Enhanced List Conversion with Nesting Support
// ============================================================================

struct TableEnumCounter {
    next: Option<u64>,
    numbering: Option<String>,
}

impl TableEnumCounter {
    fn new(start: u64, numbering: Option<String>) -> Self {
        Self {
            next: Some(start),
            numbering,
        }
    }

    fn next_marker(&mut self, explicit: Option<u64>) -> String {
        if let Some(n) = explicit {
            self.next = n.checked_add(1);
            format_enum_number_with_pattern(n, self.numbering.as_deref())
        } else if let Some(n) = self.next {
            self.next = n.checked_add(1);
            format_enum_number_with_pattern(n, self.numbering.as_deref())
        } else {
            // Explicit item hit u64::MAX; avoid wrapping a subsequent implicit item to 0.
            "\\textbullet~".to_string()
        }
    }
}

fn expand_list_spread_expr<'a>(
    mut expr: ast::Expr<'a>,
    out: &mut Vec<&'a SyntaxNode>,
    depth: usize,
) -> bool {
    if depth >= 8 {
        return false;
    }
    while let ast::Expr::Parenthesized(paren) = expr {
        expr = paren.expr();
    }
    let ast::Expr::Array(array) = expr else {
        return false;
    };
    for item in array.items() {
        match item {
            ast::ArrayItem::Pos(inner) => out.push(inner.to_untyped()),
            ast::ArrayItem::Spread(spread) => {
                if !expand_list_spread_expr(spread.expr(), out, depth + 1) {
                    return false;
                }
            }
        }
    }
    true
}

fn convert_heading_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let level = args
        .named_usize("level")
        .or_else(|| args.named_usize("depth"))
        .map(|l| l.clamp(1, 6))
        .unwrap_or(1);
    let unnumbered = match args.named("numbering") {
        Some(v) => {
            let trimmed = unwrap_balanced_parens(v.trim());
            if trimmed == "auto" {
                active_heading_unnumbered()
            } else {
                trimmed == "none"
            }
        }
        None => active_heading_unnumbered(),
    };

    let render_body = |target_ctx: &mut ConvertContext| {
        for arg in args.iter() {
            if arg.is_positional || arg.name.as_deref() == Some("body") {
                if let Some(node) = arg.value_node {
                    convert_markup_node(node, target_ctx);
                }
            }
        }
    };

    if ctx.is_in_env(&EnvironmentContext::Table) {
        let rendered = render_in_table_cell(ctx, render_body);
        let title = collapse_table_text(&rendered);
        if !title.is_empty() {
            emit_table_cell_lines(vec![format!("\\textbf{{{title}}}")], ctx);
        }
        return;
    }

    let section_cmd = get_heading_command(level);
    ctx.ensure_paragraph_break();
    ctx.push(section_cmd);
    if unnumbered {
        ctx.push("*");
    }
    ctx.push("{");
    render_body(ctx);
    ctx.push("}\n");
    ctx.last_token = TokenType::Newline;
}

fn convert_terms_func(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut pairs: Vec<(TermPart<'_>, TermPart<'_>)> = Vec::new();
    let mut has_unresolved = false;

    if let Some(args) = children.get(1).and_then(|node| node.cast::<ast::Args>()) {
        for item in args.items() {
            match item {
                ast::Arg::Pos(mut expr) => {
                    while let ast::Expr::Parenthesized(p) = expr {
                        expr = p.expr();
                    }
                    if let ast::Expr::Array(arr) = expr {
                        let mut elems = arr.items().filter_map(|it| match it {
                            ast::ArrayItem::Pos(e) => Some(TermPart::Expr(e)),
                            ast::ArrayItem::Spread(_) => None,
                        });
                        if let (Some(t), Some(d), None) = (elems.next(), elems.next(), elems.next())
                        {
                            pairs.push((t, d));
                        } else {
                            has_unresolved = true;
                        }
                    } else if let Some(td) = term_item_parts(expr.to_untyped()) {
                        pairs.push(td);
                    } else {
                        has_unresolved = true;
                    }
                }
                ast::Arg::Spread(_) => {
                    has_unresolved = true;
                }
                ast::Arg::Named(_) => {}
            }
        }
    }

    if has_unresolved {
        preserve_unconverted_layout("could not expand terms argument", children, ctx);
        return;
    }
    if pairs.is_empty() {
        return;
    }

    if ctx.is_in_env(&EnvironmentContext::Table) {
        let items: Vec<String> = pairs
            .into_iter()
            .map(|(t, d)| {
                render_table_term_entry(
                    ctx,
                    |c| convert_term_part(t, c),
                    |c| convert_term_part(d, c),
                )
            })
            .collect();
        emit_table_cell_lines(items, ctx);
    } else {
        begin_description(ctx);
        for (t, d) in pairs {
            emit_description_item(
                ctx,
                |c| convert_term_part(t, c),
                |c| convert_term_part(d, c),
            );
        }
        end_description(ctx);
    }
}

fn enum_item_func_parts<'a>(node: &'a SyntaxNode) -> Option<(Option<u64>, Vec<&'a SyntaxNode>)> {
    if node.kind() != SyntaxKind::FuncCall {
        return None;
    }
    let func_children: Vec<&'a SyntaxNode> = node.children().collect();
    let first = func_children.first()?;
    if get_func_call_name(first) != "enum.item" {
        return None;
    }
    let mut pos_nodes: Vec<&'a SyntaxNode> = Vec::new();
    if let Some(args_node) = func_children.get(1) {
        for arg in args_node.children() {
            if matches!(
                arg.kind(),
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
            pos_nodes.push(arg);
        }
    }
    for extra in func_children.iter().skip(2) {
        if extra.kind() == SyntaxKind::ContentBlock {
            pos_nodes.push(extra);
        }
    }
    match pos_nodes.len() {
        1 => Some((None, pos_nodes)),
        2 => {
            let num_node = unwrap_paren_syntax_node(pos_nodes[0]);
            let explicit = match num_node.cast::<ast::Expr>() {
                Some(ast::Expr::Int(i)) => u64::try_from(i.get()).ok(),
                _ => None,
            };
            Some((explicit, vec![pos_nodes[1]]))
        }
        _ => None,
    }
}

fn convert_list_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext, is_enumerate: bool) {
    let mut item_nodes: Vec<&SyntaxNode> = Vec::new();
    let mut has_unresolved_spread = false;
    let mut explicit_marker: Option<Option<MarkupMarkerSpec>> = None;
    let mut explicit_start: Option<Option<u64>> = None;
    let mut explicit_numbering: Option<Option<String>> = None;
    if let Some(args) = children.get(1).and_then(|node| node.cast::<ast::Args>()) {
        for item in args.items() {
            match item {
                ast::Arg::Pos(expr) => item_nodes.push(expr.to_untyped()),
                ast::Arg::Spread(spread) => {
                    if !expand_list_spread_expr(spread.expr(), &mut item_nodes, 0) {
                        has_unresolved_spread = true;
                    }
                }
                ast::Arg::Named(named) => {
                    let Some(val_expr) = valid_named_expr(named) else {
                        continue;
                    };
                    match (is_enumerate, named.name().get().as_str()) {
                        (false, "marker") => {
                            explicit_marker = ast_expr_to_marker_spec(val_expr, ctx);
                        }
                        (true, "start") => {
                            let mut inner = val_expr;
                            while let ast::Expr::Parenthesized(p) = inner {
                                inner = p.expr();
                            }
                            match inner {
                                ast::Expr::Auto(_) | ast::Expr::None(_) => {
                                    explicit_start = Some(None);
                                }
                                ast::Expr::Int(i) => {
                                    if let Ok(n) = u64::try_from(i.get()) {
                                        explicit_start = Some(Some(n));
                                    }
                                }
                                _ => {}
                            }
                        }
                        (true, "numbering") => {
                            let mut inner = val_expr;
                            while let ast::Expr::Parenthesized(p) = inner {
                                inner = p.expr();
                            }
                            match inner {
                                ast::Expr::Auto(_) => {
                                    explicit_numbering = Some(None);
                                }
                                ast::Expr::Str(s) => {
                                    explicit_numbering = Some(Some(s.get().to_string()));
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    for child in children.iter().skip(2) {
        if child.kind() == SyntaxKind::ContentBlock {
            item_nodes.push(child);
        }
    }

    if has_unresolved_spread {
        preserve_unconverted_layout("could not expand spread argument", children, ctx);
        return;
    }
    if item_nodes.is_empty() {
        return;
    }

    let (depth, _depth_guard) = enter_list_depth(is_enumerate);
    let _style_guard = if !is_enumerate {
        explicit_marker.map(|marker_override| {
            MARKUP_STYLE_STACK.with(|stack| {
                let mut stack = stack.borrow_mut();
                let mut frame = stack.last().cloned().unwrap_or_default();
                frame.list_marker = marker_override;
                stack.push(frame);
            });
            MarkupStyleGuard
        })
    } else if explicit_start.is_some() || explicit_numbering.is_some() {
        MARKUP_STYLE_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            let mut frame = stack.last().cloned().unwrap_or_default();
            if let Some(start_override) = explicit_start {
                frame.enum_start = start_override;
            }
            if let Some(num_override) = explicit_numbering.clone() {
                frame.enum_numbering = num_override;
            }
            stack.push(frame);
        });
        Some(MarkupStyleGuard)
    } else {
        None
    };

    let marker = if is_enumerate {
        None
    } else {
        active_list_marker(depth)
    };
    let enum_start = if is_enumerate {
        explicit_start.unwrap_or_else(active_enum_start)
    } else {
        None
    };
    let enum_numbering = if is_enumerate {
        explicit_numbering.unwrap_or_else(active_enum_numbering)
    } else {
        None
    };

    if ctx.is_in_env(&EnvironmentContext::Table) {
        let mut counter = TableEnumCounter::new(enum_start.unwrap_or(1), enum_numbering);
        let mut items = Vec::new();
        for item in item_nodes {
            if is_enumerate {
                if let Some((explicit, body_nodes)) = enum_item_func_parts(item) {
                    let body = collapse_table_text(&render_in_table_cell(ctx, |item_ctx| {
                        for b in body_nodes {
                            convert_markup_node(b, item_ctx);
                        }
                    }));
                    let table_marker = counter.next_marker(explicit);
                    items.push(format_table_list_entry(&table_marker, &body));
                    continue;
                }
            }
            let body = collapse_table_text(&render_in_table_cell(ctx, |item_ctx| {
                convert_markup_node(item, item_ctx);
            }));
            let table_marker = if is_enumerate {
                counter.next_marker(None)
            } else {
                format_table_list_marker(marker.as_deref())
            };
            items.push(format_table_list_entry(&table_marker, &body));
        }
        emit_table_cell_lines(items, ctx);
        return;
    }

    let env_name = if is_enumerate { "enumerate" } else { "itemize" };
    let env = if is_enumerate {
        EnvironmentContext::Enumerate
    } else {
        EnvironmentContext::Itemize
    };

    let nested_in_list = ctx.is_in_env(&EnvironmentContext::Itemize)
        || ctx.is_in_env(&EnvironmentContext::Enumerate);
    if nested_in_list {
        ctx.newline();
    } else {
        ctx.ensure_paragraph_break();
    }
    ctx.push(&ctx.list_indent());
    ctx.push_line(&format!("\\begin{{{}}}", env_name));

    ctx.push_env(env);
    let first_explicit = if is_enumerate {
        item_nodes
            .first()
            .and_then(|child| enum_item_func_parts(child).and_then(|(n, _)| n))
    } else {
        None
    };
    let mut next_expected = if is_enumerate {
        emit_enum_numbering_cmd(&ctx.list_indent(), depth, enum_numbering.as_deref(), ctx);
        if first_explicit.is_none() {
            emit_enum_start_counter(&ctx.list_indent(), depth, enum_start, ctx);
            Some(enum_start.unwrap_or(1))
        } else {
            Some(1)
        }
    } else {
        None
    };

    // Process list items
    for child in item_nodes {
        if is_enumerate {
            if let Some((explicit, body_nodes)) = enum_item_func_parts(child) {
                emit_enum_item_counter_override(
                    &format!("{}  ", ctx.list_indent()),
                    depth,
                    explicit,
                    &mut next_expected,
                    ctx,
                );
                ctx.push(&ctx.list_indent());
                ctx.push("  \\item ");
                let mut item_ctx = ConvertContext::new();
                item_ctx.options = ctx.options.clone();
                item_ctx.list_depth = ctx.list_depth;
                for b in body_nodes {
                    convert_markup_node(b, &mut item_ctx);
                }
                let item_text = item_ctx.finish_into(ctx);
                ctx.push(&item_text);
                ctx.newline();
                continue;
            }
            next_expected = next_expected.and_then(|v| v.checked_add(1));
        }
        match child.kind() {
            SyntaxKind::FuncCall => {
                // Might be a nested list
                let func_children: Vec<&SyntaxNode> = child.children().collect();
                if !func_children.is_empty() {
                    let func_name = func_children[0].text().to_string();
                    if func_name == "list" || func_name == "enum" {
                        ctx.push(&ctx.list_indent());
                        ctx.push_line(&format!(
                            "  {}",
                            format_latex_list_item_prefix(marker.as_deref()).trim_end()
                        ));
                        convert_list_to_latex(&func_children, ctx, func_name == "enum");
                    } else {
                        ctx.push(&ctx.list_indent());
                        ctx.push("  ");
                        ctx.push(&format_latex_list_item_prefix(marker.as_deref()));
                        convert_markup_node(child, ctx);
                        ctx.newline();
                    }
                }
            }
            _ => {
                ctx.push(&ctx.list_indent());
                ctx.push("  ");
                ctx.push(&format_latex_list_item_prefix(marker.as_deref()));

                let mut item_ctx = ConvertContext::new();
                item_ctx.options = ctx.options.clone();
                item_ctx.list_depth = ctx.list_depth;
                convert_markup_node(child, &mut item_ctx);
                let item_text = item_ctx.finish_into(ctx);
                ctx.push(&item_text);
                ctx.newline();
            }
        }
    }

    ctx.pop_env();
    ctx.push(&ctx.list_indent());
    ctx.push_line(&format!("\\end{{{}}}", env_name));
}

// ============================================================================
// Raw/Verbatim Conversion
// ============================================================================

fn convert_raw_to_latex(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let args = FuncArgs::from_func_call(children);
    let lang = args.named_text("lang").unwrap_or("").to_string();
    let mut content = String::new();
    let mut is_block = args.named_bool("block").unwrap_or(false);

    if let Some(first_node) = args.first_node() {
        match first_node.kind() {
            SyntaxKind::Str => {
                content = get_string_content(first_node);
            }
            SyntaxKind::ContentBlock => {
                content = first_node
                    .text()
                    .to_string()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                is_block = content.contains('\n');
            }
            _ => {}
        }
    }

    if is_block || content.contains('\n') {
        if ctx.is_in_env(&EnvironmentContext::Table) {
            emit_table_raw_code(&content, ctx);
            return;
        }
        ctx.ensure_paragraph_break();
        if !lang.is_empty() && is_listings_supported(&lang) {
            // Use listings package with language (only if supported)
            ctx.push_line(&format!("\\begin{{lstlisting}}[language={}]", lang));
            ctx.push(&content);
            ctx.newline();
            ctx.push_line("\\end{lstlisting}");
        } else {
            // Unsupported or no language - use verbatim
            if !lang.is_empty() {
                ctx.push_line(&format!("% Code block (language: {})", lang));
            }
            ctx.push_line("\\begin{verbatim}");
            ctx.push(&content);
            ctx.newline();
            ctx.push_line("\\end{verbatim}");
        }
    } else {
        // Inline code
        ctx.push("\\texttt{");
        ctx.push(&escape_latex_text(&content));
        ctx.push("}");
    }
}

// ============================================================================
// List Item Helper Functions
// ============================================================================

/// Convert a single list item (unordered)
fn convert_list_item(node: &SyntaxNode, ctx: &mut ConvertContext, marker: Option<&str>) {
    ctx.push_indent();
    ctx.push(&format_latex_list_item_prefix(marker));
    for child in node.children() {
        if child.kind() != SyntaxKind::ListMarker {
            convert_markup_node(child, ctx);
        }
    }
    ctx.newline();
}

enum TermPart<'a> {
    Markup(&'a SyntaxNode),
    Expr(ast::Expr<'a>),
}

fn term_item_arg(arg: Option<ast::Arg<'_>>) -> Option<TermPart<'_>> {
    match arg? {
        ast::Arg::Pos(expr) => Some(TermPart::Expr(expr)),
        _ => None,
    }
}

fn is_list_group_trivia(node: &SyntaxNode) -> bool {
    matches!(
        node.kind(),
        SyntaxKind::Space
            | SyntaxKind::Parbreak
            | SyntaxKind::LineComment
            | SyntaxKind::BlockComment
    )
}

fn term_item_parts(node: &SyntaxNode) -> Option<(TermPart<'_>, TermPart<'_>)> {
    if let Some(item) = node.cast::<ast::TermItem>() {
        return Some((
            TermPart::Markup(item.term().to_untyped()),
            TermPart::Markup(item.description().to_untyped()),
        ));
    }

    // MiniEval emits this explicit form when expanding content to Typst source.
    let call = node.cast::<ast::FuncCall>()?;
    let ast::Expr::FieldAccess(access) = call.callee() else {
        return None;
    };
    if super::scope_analysis::is_shadowed_node(access.to_untyped()) {
        return None;
    }
    let ast::Expr::Ident(target) = access.target() else {
        return None;
    };
    if target.get().as_str() != "terms" || access.field().get().as_str() != "item" {
        return None;
    }
    let mut args = call.args().items();
    match (
        term_item_arg(args.next()),
        term_item_arg(args.next()),
        args.next(),
    ) {
        (Some(term), Some(description), None) => Some((term, description)),
        _ => None,
    }
}

fn collect_following_evaluated_term_items<'a>(
    nodes: &'a [ContentNode],
    i: &mut usize,
    mut on_item: impl FnMut(&'a [ContentNode], &'a [ContentNode]),
) {
    while *i < nodes.len() {
        let mut next_idx = *i;
        while matches!(
            nodes.get(next_idx),
            Some(ContentNode::Space | ContentNode::Parbreak)
        ) {
            next_idx += 1;
        }
        if let Some((term, description)) =
            nodes.get(next_idx).and_then(ContentNode::term_item_parts)
        {
            on_item(term, description);
            *i = next_idx + 1;
        } else {
            break;
        }
    }
}

fn collect_following_ast_term_items<'a>(
    children: &[&'a SyntaxNode],
    i: &mut usize,
    mut on_item: impl FnMut(TermPart<'a>, TermPart<'a>),
) {
    while *i < children.len() {
        let mut next_idx = *i;
        while let Some(next_child) = children.get(next_idx) {
            let trivia = is_list_group_trivia(next_child);
            let item_hash = next_child.kind() == SyntaxKind::Hash
                && children
                    .get(next_idx + 1)
                    .is_some_and(|next| term_item_parts(next).is_some());
            if trivia || item_hash {
                next_idx += 1;
            } else {
                break;
            }
        }
        if let Some((term, description)) = children.get(next_idx).copied().and_then(term_item_parts)
        {
            on_item(term, description);
            *i = next_idx + 1;
        } else {
            break;
        }
    }
}

fn collect_following_kind<'a>(
    children: &[&'a SyntaxNode],
    i: &mut usize,
    kind: SyntaxKind,
    mut on_item: impl FnMut(&'a SyntaxNode),
) {
    while *i < children.len() {
        let mut next_idx = *i;
        while children
            .get(next_idx)
            .is_some_and(|c| is_list_group_trivia(c))
        {
            next_idx += 1;
        }
        if let Some(&item) = children.get(next_idx).filter(|c| c.kind() == kind) {
            on_item(item);
            *i = next_idx + 1;
        } else {
            break;
        }
    }
}

fn format_table_list_entry(marker: &str, body: &str) -> String {
    if body.is_empty() {
        marker.trim_end_matches('~').to_string()
    } else {
        format!("{marker}{body}")
    }
}

fn render_in_table_cell(
    ctx: &mut ConvertContext,
    render: impl FnOnce(&mut ConvertContext),
) -> String {
    let mut inner = ConvertContext::new();
    inner.options = ctx.options.clone();
    inner.push_env(EnvironmentContext::Table);
    render(&mut inner);
    inner.finish_into(ctx)
}

fn render_table_list_item(node: &SyntaxNode, marker: &str, ctx: &mut ConvertContext) -> String {
    let body = collapse_table_text(&render_in_table_cell(ctx, |inner| {
        for child in node.children() {
            if !matches!(
                child.kind(),
                SyntaxKind::ListMarker | SyntaxKind::EnumMarker
            ) {
                convert_markup_node(child, inner);
            }
        }
    }));
    format_table_list_entry(marker, &body)
}

fn escape_latex_table_code_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut plain = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut at_line_start = true;

    for (idx, &ch) in chars.iter().enumerate() {
        match ch {
            ' ' => {
                let prev_space = idx > 0 && chars[idx - 1] == ' ';
                let next_space = chars.get(idx + 1).copied() == Some(' ');
                if at_line_start || prev_space || next_space {
                    if !plain.is_empty() {
                        out.push_str(&escape_latex_text(&plain));
                        plain.clear();
                    }
                    out.push('~');
                } else {
                    plain.push(' ');
                }
            }
            '\t' => {
                if !plain.is_empty() {
                    out.push_str(&escape_latex_text(&plain));
                    plain.clear();
                }
                out.push_str("~~~~");
            }
            other => {
                at_line_start = false;
                plain.push(other);
            }
        }
    }
    if !plain.is_empty() {
        out.push_str(&escape_latex_text(&plain));
    }
    out
}

fn emit_table_raw_code(text: &str, ctx: &mut ConvertContext) {
    let trimmed = text.trim_matches(['\r', '\n']);
    if trimmed.is_empty() {
        emit_table_cell_lines(vec!["\\texttt{}".to_string()], ctx);
        return;
    }
    let lines: Vec<String> = trimmed
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                "\\strut".to_string()
            } else {
                format!("\\texttt{{{}}}", escape_latex_table_code_line(line))
            }
        })
        .collect();
    emit_table_cell_lines(lines, ctx);
}

fn has_unescaped_latex_comment(line: &str) -> bool {
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '%' {
            return true;
        }
    }
    false
}

fn collapse_table_text(s: &str) -> String {
    let mut out = String::new();
    let mut prev_has_comment = false;
    for line in s
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('%'))
    {
        if !out.is_empty() {
            out.push(if prev_has_comment { '\n' } else { ' ' });
        }
        out.push_str(line);
        prev_has_comment = has_unescaped_latex_comment(line);
    }
    if prev_has_comment {
        out.push('\n');
    }
    out
}

fn format_table_term_item(term: &str, description: &str) -> String {
    let term = collapse_table_text(term);
    let description = collapse_table_text(description);
    match (term.is_empty(), description.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("\\textbf{{{term}}}"),
        (true, false) => description,
        (false, false) => format!("\\textbf{{{term}}} {description}"),
    }
}

fn render_table_term_entry(
    ctx: &mut ConvertContext,
    render_term: impl FnOnce(&mut ConvertContext),
    render_description: impl FnOnce(&mut ConvertContext),
) -> String {
    let term = render_in_table_cell(ctx, render_term);
    let description = render_in_table_cell(ctx, render_description);
    format_table_term_item(&term, &description)
}

fn emit_table_cell_lines(lines: Vec<String>, ctx: &mut ConvertContext) {
    let lines: Vec<String> = lines.into_iter().filter(|line| !line.is_empty()).collect();
    if lines.is_empty() {
        return;
    }
    if !ctx.output.is_empty() && !ctx.output.ends_with(' ') && !ctx.output.ends_with('\n') {
        ctx.push(" ");
    }
    if lines.len() == 1 {
        ctx.push(&lines[0]);
        ctx.last_token = TokenType::Text;
    } else {
        ctx.push("\\begin{tabular}[t]{@{}l@{}}");
        ctx.push(&lines.join(" \\\\ "));
        ctx.push("\\end{tabular}");
        ctx.last_token = TokenType::Command;
    }
}

fn emit_single_ast_term_item(
    term: TermPart<'_>,
    description: TermPart<'_>,
    ctx: &mut ConvertContext,
) {
    if ctx.is_in_env(&EnvironmentContext::Table) {
        let item = render_table_term_entry(
            ctx,
            |c| convert_term_part(term, c),
            |c| convert_term_part(description, c),
        );
        emit_table_cell_lines(vec![item], ctx);
    } else {
        begin_description(ctx);
        emit_description_item(
            ctx,
            |c| convert_term_part(term, c),
            |c| convert_term_part(description, c),
        );
        end_description(ctx);
    }
}

fn begin_description(ctx: &mut ConvertContext) {
    ctx.ensure_paragraph_break();
    ctx.push_line("\\begin{description}");
    ctx.indent_level += 1;
}

fn end_description(ctx: &mut ConvertContext) {
    ctx.indent_level -= 1;
    ctx.push_line("\\end{description}");
}

fn is_collapsible_ascii_whitespace(s: &str) -> bool {
    s.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
}

fn push_escaped_latex_text(ctx: &mut ConvertContext, text: &str) {
    let escaped = escape_latex_text(text);
    if ctx.output.ends_with('-') && escaped.starts_with('-') {
        ctx.push("{}");
    }
    ctx.push(&escaped);
}

fn convert_markup_children(children: &[&SyntaxNode], ctx: &mut ConvertContext) {
    let mut i = 0;
    while i < children.len() {
        let child = children[i];
        match child.kind() {
            SyntaxKind::ListItem => {
                let (depth, _depth_guard) = enter_list_depth(false);
                let marker = active_list_marker(depth);
                if ctx.is_in_env(&EnvironmentContext::Table) {
                    let table_marker = format_table_list_marker(marker.as_deref());
                    let mut items = vec![render_table_list_item(child, &table_marker, ctx)];
                    i += 1;
                    collect_following_kind(children, &mut i, SyntaxKind::ListItem, |item| {
                        items.push(render_table_list_item(item, &table_marker, ctx));
                    });
                    emit_table_cell_lines(items, ctx);
                } else {
                    ctx.ensure_paragraph_break();
                    ctx.push_line("\\begin{itemize}");
                    ctx.indent_level += 1;

                    while i < children.len()
                        && (children[i].kind() == SyntaxKind::ListItem
                            || is_list_group_trivia(children[i]))
                    {
                        if children[i].kind() == SyntaxKind::ListItem {
                            convert_list_item(children[i], ctx, marker.as_deref());
                        }
                        i += 1;
                    }

                    ctx.indent_level -= 1;
                    ctx.push_line("\\end{itemize}");
                }
            }
            SyntaxKind::EnumItem => {
                let (depth, _depth_guard) = enter_list_depth(true);
                let start = active_enum_start();
                let numbering = active_enum_numbering();
                if ctx.is_in_env(&EnvironmentContext::Table) {
                    let mut counter = TableEnumCounter::new(start.unwrap_or(1), numbering);
                    let mut items = Vec::new();
                    let mut push_enum = |item: &SyntaxNode| {
                        let explicit = item.cast::<ast::EnumItem>().and_then(|e| e.number());
                        let marker = counter.next_marker(explicit);
                        items.push(render_table_list_item(item, &marker, ctx));
                    };
                    push_enum(child);
                    i += 1;
                    collect_following_kind(children, &mut i, SyntaxKind::EnumItem, push_enum);
                    emit_table_cell_lines(items, ctx);
                } else {
                    ctx.ensure_paragraph_break();
                    ctx.push_line("\\begin{enumerate}");
                    ctx.indent_level += 1;
                    let indent = "  ".repeat(ctx.indent_level);
                    let first_explicit = child.cast::<ast::EnumItem>().and_then(|e| e.number());
                    emit_enum_numbering_cmd(&indent, depth, numbering.as_deref(), ctx);
                    if first_explicit.is_none() {
                        emit_enum_start_counter(&indent, depth, start, ctx);
                    }
                    let mut next_expected = Some(if first_explicit.is_none() {
                        start.unwrap_or(1)
                    } else {
                        1
                    });

                    while i < children.len()
                        && (children[i].kind() == SyntaxKind::EnumItem
                            || is_list_group_trivia(children[i]))
                    {
                        if children[i].kind() == SyntaxKind::EnumItem {
                            let explicit =
                                children[i].cast::<ast::EnumItem>().and_then(|e| e.number());
                            emit_enum_item_counter_override(
                                &indent,
                                depth,
                                explicit,
                                &mut next_expected,
                                ctx,
                            );
                            convert_enum_item(children[i], ctx);
                        }
                        i += 1;
                    }

                    ctx.indent_level -= 1;
                    ctx.push_line("\\end{enumerate}");
                }
            }
            _ if term_item_parts(child).is_some() => {
                if ctx.is_in_env(&EnvironmentContext::Table) {
                    let mut items = Vec::new();
                    if let Some((term, description)) = term_item_parts(child) {
                        items.push(render_table_term_entry(
                            ctx,
                            |c| convert_term_part(term, c),
                            |c| convert_term_part(description, c),
                        ));
                    }
                    i += 1;
                    collect_following_ast_term_items(children, &mut i, |t, d| {
                        items.push(render_table_term_entry(
                            ctx,
                            |c| convert_term_part(t, c),
                            |c| convert_term_part(d, c),
                        ));
                    });
                    emit_table_cell_lines(items, ctx);
                } else {
                    begin_description(ctx);
                    if let Some((term, description)) = term_item_parts(child) {
                        emit_description_item(
                            ctx,
                            |c| convert_term_part(term, c),
                            |c| convert_term_part(description, c),
                        );
                    }
                    i += 1;
                    collect_following_ast_term_items(children, &mut i, |t, d| {
                        emit_description_item(
                            ctx,
                            |c| convert_term_part(t, c),
                            |c| convert_term_part(d, c),
                        );
                    });
                    end_description(ctx);
                }
            }
            SyntaxKind::FuncCall => {
                let func_name = child
                    .children()
                    .next()
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();

                if func_name == "figure" {
                    let mut label_text: Option<String> = None;
                    let mut label_idx: Option<usize> = None;

                    for (j, sibling) in children.iter().enumerate().skip(i + 1) {
                        match sibling.kind() {
                            SyntaxKind::Space | SyntaxKind::Linebreak => continue,
                            SyntaxKind::Label => {
                                let text = sibling.text().to_string();
                                label_text = Some(
                                    text.trim_start_matches('<')
                                        .trim_end_matches('>')
                                        .to_string(),
                                );
                                label_idx = Some(j);
                                break;
                            }
                            _ => break,
                        }
                    }

                    ctx.pending_label = label_text;
                    convert_markup_node(child, ctx);
                    ctx.pending_label = None;

                    if let Some(j) = label_idx {
                        i = j + 1;
                    } else {
                        i += 1;
                    }
                } else {
                    convert_markup_node(child, ctx);
                    i += 1;
                }
            }
            _ => {
                convert_markup_node(child, ctx);
                i += 1;
            }
        }
    }
}

fn collect_term_add_operands<'a>(mut expr: ast::Expr<'a>, out: &mut Vec<ast::Expr<'a>>) {
    while let ast::Expr::Parenthesized(paren) = expr {
        expr = paren.expr();
    }
    if let ast::Expr::Binary(bin) = expr {
        if bin.op() == ast::BinOp::Add {
            collect_term_add_operands(bin.lhs(), out);
            collect_term_add_operands(bin.rhs(), out);
            return;
        }
    }
    out.push(expr);
}

fn convert_term_expr(expr: ast::Expr<'_>, ctx: &mut ConvertContext) {
    let mut operands = Vec::new();
    collect_term_add_operands(expr, &mut operands);

    let mut pending_markup: Vec<&SyntaxNode> = Vec::new();
    for operand in operands {
        if let ast::Expr::ContentBlock(block) = operand {
            pending_markup.extend(block.body().to_untyped().children());
            continue;
        }
        if !pending_markup.is_empty() {
            convert_markup_children(&pending_markup, ctx);
            pending_markup.clear();
        }
        match operand {
            ast::Expr::Str(text) => {
                let decoded = text.get();
                if !decoded.is_empty() {
                    push_escaped_latex_text(ctx, &decoded);
                    ctx.last_token = TokenType::Text;
                }
            }
            other => convert_markup_node(other.to_untyped(), ctx),
        }
    }
    if !pending_markup.is_empty() {
        convert_markup_children(&pending_markup, ctx);
    }
}

fn convert_term_part(part: TermPart<'_>, ctx: &mut ConvertContext) {
    match part {
        TermPart::Markup(node) => convert_markup_node(node, ctx),
        TermPart::Expr(expr) => convert_term_expr(expr, ctx),
    }
}

fn emit_description_item(
    ctx: &mut ConvertContext,
    render_term: impl FnOnce(&mut ConvertContext),
    render_description: impl FnOnce(&mut ConvertContext),
) {
    let mut inner = ConvertContext::new();
    inner.options = ctx.options.clone();
    render_term(&mut inner);
    let term = inner.finish_into(ctx);
    push_term_label(&term, ctx);
    render_description(ctx);
    ctx.newline();
}

fn push_term_label(term: &str, ctx: &mut ConvertContext) {
    let term = term.trim();
    ctx.push_indent();
    if term.is_empty() {
        // `/ : Details` has no term; a bare `\item` is its LaTeX form.
        ctx.push("\\item ");
    } else if term.contains(']') {
        // Brace the label so a `]` in it does not end the optional argument.
        ctx.push(&format!("\\item[{{{}}}] ", term));
    } else {
        ctx.push(&format!("\\item[{}] ", term));
    }
}

/// Convert a single enum item (ordered)
fn convert_enum_item(node: &SyntaxNode, ctx: &mut ConvertContext) {
    ctx.push_indent();
    ctx.push("\\item ");
    for child in node.children() {
        if child.kind() != SyntaxKind::EnumMarker {
            convert_markup_node(child, ctx);
        }
    }
    ctx.newline();
}
