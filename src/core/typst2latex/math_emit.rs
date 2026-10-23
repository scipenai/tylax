use super::context::{ConvertContext, EnvironmentContext, T2LOptions, TokenType};
use super::math_ir::{
    normalize_math_ir, ArrayLayout, MathCommand, MathEnvironment, MathIr, MathSpacing,
    MathStyleMode,
};

pub fn emit_math_ir(ir: &MathIr, ctx: &mut ConvertContext) {
    match ir {
        MathIr::Seq(items) => {
            for item in items {
                emit_math_ir(item, ctx);
            }
        }
        MathIr::Linebreak => emit_linebreak(ctx),
        MathIr::Symbol(symbol) => emit_symbol(symbol, ctx),
        MathIr::Ident(ident) => ctx.push_with_spacing(ident, TokenType::Letter),
        MathIr::Number(number) => ctx.push_with_spacing(number, TokenType::Number),
        MathIr::Operator(operator) => ctx.push_with_spacing(operator, TokenType::Operator),
        MathIr::Punctuation(ch) => emit_punctuation(*ch, ctx),
        MathIr::Spacing(spacing) => emit_spacing(spacing, ctx),
        MathIr::Delimited {
            open,
            content,
            close,
        } => emit_delimited(open, content, close, ctx),
        MathIr::Apply { callee, args } => emit_apply(callee, args, ctx),
        MathIr::Limits(content) => emit_limits(content, ctx),
        MathIr::Style { mode, content } => emit_style(mode, content, ctx),
        MathIr::Attachment { .. } => emit_attachment(ir, ctx),
        MathIr::Script {
            base,
            sub,
            sup,
            primes,
            sup_first,
        } => emit_script(
            base,
            *primes,
            sub.as_deref(),
            sup.as_deref(),
            *sup_first,
            ctx,
        ),
        MathIr::Command(command) => emit_command(command, ctx),
        MathIr::Environment(environment) => emit_environment(environment, ctx),
        MathIr::RawLiteral(text) => emit_raw_literal(text, ctx),
    }
}

fn emit_apply(callee: &MathIr, args: &[MathIr], ctx: &mut ConvertContext) {
    emit_math_ir(callee, ctx);
    ctx.push("(");
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            ctx.push(", ");
        }
        ctx.push(&emit_math_ir_to_string(arg, ctx));
    }
    ctx.push(")");
    ctx.last_token = TokenType::CloseParen;
}

/// Render math IR in a context of its own, such as an equation inside a text
/// argument, which is built outside an emitting context.
pub fn render_math_ir(ir: &MathIr, options: &T2LOptions) -> String {
    let mut nested = ConvertContext::new();
    nested.options = options.clone();
    nested.in_math = true;
    emit_math_ir(ir, &mut nested);
    nested.finalize()
}

pub fn emit_math_ir_to_string(ir: &MathIr, template: &ConvertContext) -> String {
    render_math_ir(ir, &template.options)
}

fn emit_symbol(symbol: &str, ctx: &mut ConvertContext) {
    if symbol.starts_with('\\') {
        ctx.push(symbol);
        ctx.last_token = TokenType::Command;
    } else {
        ctx.push_with_spacing(symbol, TokenType::Letter);
    }
}

fn emit_punctuation(ch: char, ctx: &mut ConvertContext) {
    ctx.trim_trailing_space();
    match ch {
        ',' | ';' => ctx.push(&format!("{} ", ch)),
        ':' => ctx.push(":"),
        other => ctx.push(&other.to_string()),
    }
    ctx.last_token = TokenType::Punctuation;
}

fn emit_spacing(spacing: &MathSpacing, ctx: &mut ConvertContext) {
    match spacing {
        MathSpacing::Soft => {
            if !ctx.output.ends_with(' ') && !ctx.output.ends_with('{') {
                ctx.push(" ");
            }
            ctx.last_token = TokenType::None;
        }
        MathSpacing::Space => {
            ctx.push("\\ ");
            ctx.last_token = TokenType::Command;
        }
        MathSpacing::Thin => {
            ctx.push("\\,");
            ctx.last_token = TokenType::Command;
        }
        MathSpacing::Med => {
            ctx.push("\\:");
            ctx.last_token = TokenType::Command;
        }
        MathSpacing::Thick => {
            ctx.push("\\;");
            ctx.last_token = TokenType::Command;
        }
        MathSpacing::Quad => {
            ctx.push("\\quad");
            ctx.last_token = TokenType::Command;
        }
        MathSpacing::Qquad | MathSpacing::Wide => {
            ctx.push("\\qquad");
            ctx.last_token = TokenType::Command;
        }
    }
}

fn emit_delimited(open: &str, content: &MathIr, close: &str, ctx: &mut ConvertContext) {
    ctx.push(open);
    ctx.last_token = delimiter_token_type(open, true);
    emit_math_ir(content, ctx);
    ctx.push(close);
    ctx.last_token = delimiter_token_type(close, false);
}

fn emit_style(mode: &MathStyleMode, content: &MathIr, ctx: &mut ConvertContext) {
    match mode {
        MathStyleMode::Display => {
            ctx.push(r"\displaystyle ");
            emit_math_ir(content, ctx);
            if !ctx.options.block_math_mode {
                ctx.push(r" \textstyle ");
                ctx.last_token = TokenType::Command;
            }
        }
        MathStyleMode::Inline => {
            ctx.push(r"\textstyle ");
            emit_math_ir(content, ctx);
            if ctx.options.block_math_mode {
                ctx.push(r" \displaystyle ");
                ctx.last_token = TokenType::Command;
            }
        }
        // A group ends the switch, so no restoring switch is needed.
        MathStyleMode::Script | MathStyleMode::ScriptScript => {
            ctx.push(if *mode == MathStyleMode::Script {
                r"{\scriptstyle "
            } else {
                r"{\scriptscriptstyle "
            });
            emit_math_ir(content, ctx);
            ctx.push("}");
        }
    }
    ctx.last_token = TokenType::Command;
}

fn emit_attachment(attachment: &MathIr, ctx: &mut ConvertContext) {
    let MathIr::Attachment {
        base,
        top,
        bottom,
        top_left,
        top_right,
        bottom_left,
        bottom_right,
    } = attachment
    else {
        return;
    };

    if top_left.is_some() || bottom_left.is_some() {
        ctx.push("{}");
        if let Some(bottom_left) = bottom_left.as_deref() {
            ctx.push("_");
            emit_script_content(bottom_left, ctx);
        }
        if let Some(top_left) = top_left.as_deref() {
            ctx.push("^");
            emit_script_content(top_left, ctx);
        }
        ctx.last_token = TokenType::None;
    }

    emit_math_ir(base, ctx);

    if let Some(bottom) = bottom.as_deref() {
        ctx.push("_");
        emit_script_content_for_base(bottom, Some(base), ctx);
    }
    if let Some(top) = top.as_deref() {
        ctx.push("^");
        emit_script_content_for_base(top, Some(base), ctx);
    }

    if top_right.is_some() || bottom_right.is_some() {
        ctx.push("{}");
        if let Some(bottom_right) = bottom_right.as_deref() {
            ctx.push("_");
            emit_script_content(bottom_right, ctx);
        }
        if let Some(top_right) = top_right.as_deref() {
            ctx.push("^");
            emit_script_content(top_right, ctx);
        }
    }

    ctx.last_token = TokenType::Command;
}

/// Large operators that take `\limits` in LaTeX.
const MATHOP_SYMBOLS: &[&str] = &[
    r"\sum",
    r"\prod",
    r"\coprod",
    r"\int",
    r"\iint",
    r"\iiint",
    r"\oint",
    r"\bigcup",
    r"\bigcap",
    r"\bigsqcup",
    r"\biguplus",
    r"\bigvee",
    r"\bigwedge",
    r"\bigoplus",
    r"\bigotimes",
    r"\bigodot",
    r"\lim",
    r"\limsup",
    r"\liminf",
    r"\max",
    r"\min",
    r"\sup",
    r"\inf",
    r"\det",
    r"\gcd",
    r"\Pr",
];

fn is_mathop(ir: &MathIr) -> bool {
    match ir {
        MathIr::Symbol(symbol) => MATHOP_SYMBOLS.contains(&symbol.as_str()),
        MathIr::Command(command) => command.latex == r"\operatorname",
        MathIr::Delimited { open, .. } => open.starts_with(r"\mathop{"),
        _ => false,
    }
}

/// `limits(x)`: `\operatorname*` for a named operator, `\limits` after any
/// other LaTeX operator. Other content is emitted as is, and `emit_script`
/// stacks its scripts with `\overset` and `\underset`.
fn emit_limits(content: &MathIr, ctx: &mut ConvertContext) {
    match content {
        MathIr::Command(command) if command.latex == r"\operatorname" => {
            emit_command(
                &MathCommand {
                    latex: r"\operatorname*".to_string(),
                    ..command.clone()
                },
                ctx,
            );
        }
        _ if is_mathop(content) => {
            emit_math_ir(content, ctx);
            ctx.push(r"\limits");
            ctx.last_token = TokenType::Command;
        }
        _ => emit_math_ir(content, ctx),
    }
}

/// Scripts set above and below a base that is not a LaTeX operator, where
/// `\limits` is an error: `limits(F)^(a)` → `\overset{a}{F}`.
fn emit_stacked_scripts(
    base: &MathIr,
    sub: Option<&MathIr>,
    sup: Option<&MathIr>,
    ctx: &mut ConvertContext,
) {
    let mut stacked = emit_math_ir_to_string(base, ctx).trim().to_string();
    if let Some(sup) = sup {
        stacked = format!(
            r"\overset{{{}}}{{{}}}",
            render_script_fragment(sup, ctx),
            stacked
        );
    }
    if let Some(sub) = sub {
        stacked = format!(
            r"\underset{{{}}}{{{}}}",
            render_script_fragment(sub, ctx),
            stacked
        );
    }
    ctx.push(&stacked);
    ctx.last_token = TokenType::Command;
}

fn emit_script(
    base: &MathIr,
    primes: usize,
    sub: Option<&MathIr>,
    sup: Option<&MathIr>,
    sup_first: bool,
    ctx: &mut ConvertContext,
) {
    if let MathIr::Limits(content) = base {
        if primes == 0 && !is_mathop(content) && (sub.is_some() || sup.is_some()) {
            emit_stacked_scripts(content, sub, sup, ctx);
            return;
        }
    }

    emit_math_ir(base, ctx);

    if primes > 0 {
        // LaTeX rejects `'^2` as a double superscript; `{}'^2` is valid.
        if base.is_empty() {
            ctx.push("{}");
        }
        ctx.push(&"'".repeat(primes));
        ctx.last_token = TokenType::Punctuation;
    }

    // Keep the source order, so that `x^a_b` round-trips to the same Typst.
    let scripts = if sup_first {
        [("^", sup), ("_", sub)]
    } else {
        [("_", sub), ("^", sup)]
    };
    for (marker, script) in scripts {
        if let Some(script) = script {
            ctx.push(marker);
            ctx.last_token = TokenType::Operator;
            emit_script_content_for_base(script, Some(base), ctx);
        }
    }
}

fn strip_grouping_parentheses_for_script(ir: &MathIr) -> &MathIr {
    match ir {
        MathIr::Delimited {
            open,
            content,
            close,
        } if (open == "(" && close == ")") || (open == r"\left(" && close == r"\right)") => {
            content.as_ref()
        }
        _ => ir,
    }
}

fn strip_grouping_parentheses_from_rendered_script(rendered: String) -> String {
    if can_strip_wrapping_left_right_parens(&rendered) {
        return rendered
            .trim_start_matches(r"\left(")
            .trim_end_matches(r"\right)")
            .trim()
            .to_string();
    }

    if can_strip_wrapping_plain_parens(&rendered) {
        return rendered[1..rendered.len() - 1].trim().to_string();
    }

    rendered
}

fn can_strip_wrapping_plain_parens(rendered: &str) -> bool {
    if !rendered.starts_with('(') || !rendered.ends_with(')') {
        return false;
    }

    let mut depth = 0i32;
    for (index, ch) in rendered.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && index != rendered.len() - 1 {
                    return false;
                }
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }

    depth == 0
}

fn can_strip_wrapping_left_right_parens(rendered: &str) -> bool {
    if !rendered.starts_with(r"\left(") || !rendered.ends_with(r"\right)") {
        return false;
    }

    let mut depth = 0i32;
    let mut index = 0usize;
    while index < rendered.len() {
        let rest = &rendered[index..];
        if rest.starts_with(r"\left(") {
            depth += 1;
            index += r"\left(".len();
            continue;
        }
        if rest.starts_with(r"\right)") {
            depth -= 1;
            if depth == 0 && index + r"\right)".len() != rendered.len() {
                return false;
            }
            if depth < 0 {
                return false;
            }
            index += r"\right)".len();
            continue;
        }
        index += rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }

    depth == 0
}

fn script_seq_or_single(items: Vec<MathIr>) -> MathIr {
    if items.is_empty() {
        MathIr::empty()
    } else if items.len() == 1 {
        items.into_iter().next().unwrap_or_else(MathIr::empty)
    } else {
        MathIr::Seq(items)
    }
}

fn split_top_level_script_lines(ir: &MathIr) -> Option<Vec<MathIr>> {
    let ir = strip_grouping_parentheses_for_script(ir);
    let MathIr::Seq(items) = ir else {
        return None;
    };

    let mut saw_linebreak = false;
    let mut current = Vec::new();
    let mut lines = Vec::new();

    for item in items {
        match item {
            MathIr::Linebreak => {
                saw_linebreak = true;
                let line = normalize_math_ir(script_seq_or_single(std::mem::take(&mut current)));
                if !line.is_empty() {
                    lines.push(line);
                }
            }
            other => current.push(other.clone()),
        }
    }

    let line = normalize_math_ir(script_seq_or_single(current));
    if !line.is_empty() {
        lines.push(line);
    }

    if saw_linebreak {
        Some(lines)
    } else {
        None
    }
}

fn render_script_fragment(ir: &MathIr, ctx: &ConvertContext) -> String {
    let ir = strip_grouping_parentheses_for_script(ir);
    let rendered = emit_math_ir_to_string(ir, ctx).trim().to_string();
    strip_grouping_parentheses_from_rendered_script(rendered)
}

const LIMITS_LIKE_SYMBOLS: &[&str] = &[
    r"\sum", r"\prod", r"\int", r"\oint", r"\iint", r"\iiint", r"\lim", r"\limsup", r"\liminf",
    r"\max", r"\min", r"\sup", r"\inf",
];

fn is_limits_like_symbol(symbol: &str) -> bool {
    LIMITS_LIKE_SYMBOLS.contains(&symbol)
}

fn is_limits_like_base(base: &MathIr) -> bool {
    match base {
        MathIr::Limits(_) => true,
        MathIr::Symbol(symbol) => is_limits_like_symbol(symbol),
        MathIr::Apply { callee, .. } => is_limits_like_base(callee),
        MathIr::Style { content, .. } => is_limits_like_base(content),
        MathIr::Delimited { open, .. } => open.starts_with(r"\mathop{"),
        _ => false,
    }
}

fn emit_rendered_script_content(rendered: &str, ctx: &mut ConvertContext) {
    if rendered.is_empty() {
        return;
    }

    if rendered.len() == 1
        && rendered
            .chars()
            .next()
            .map(|ch| ch.is_alphanumeric())
            .unwrap_or(false)
    {
        ctx.push(rendered);
    } else {
        ctx.push("{");
        ctx.push(rendered);
        ctx.push("}");
    }
    ctx.last_token = TokenType::Command;
}

fn emit_script_content_for_base(ir: &MathIr, base: Option<&MathIr>, ctx: &mut ConvertContext) {
    if let Some(base) = base {
        if is_limits_like_base(base) {
            if let Some(lines) = split_top_level_script_lines(ir) {
                let rendered_lines: Vec<String> = lines
                    .iter()
                    .map(|line| render_script_fragment(line, ctx))
                    .filter(|line| !line.is_empty())
                    .collect();

                if rendered_lines.len() > 1 {
                    let joined = strip_grouping_parentheses_from_rendered_script(
                        rendered_lines.join(r" \\ "),
                    );
                    ctx.push(r"{\substack{");
                    ctx.push(&joined);
                    ctx.push("}}");
                    ctx.last_token = TokenType::Command;
                    return;
                }
            }
        }
    }

    emit_script_content(ir, ctx);
}

fn emit_script_content(ir: &MathIr, ctx: &mut ConvertContext) {
    let rendered = render_script_fragment(ir, ctx);
    emit_rendered_script_content(&rendered, ctx);
}

/// A matrix with rules or a non-centred alignment, as an `array` inside the
/// delimiters its environment name implies.
fn emit_array(name: &str, rows: &[Vec<MathIr>], layout: &ArrayLayout, ctx: &mut ConvertContext) {
    let (open, close) = match name {
        "pmatrix" => ("(", ")"),
        "bmatrix" => ("[", "]"),
        "Bmatrix" => ("\\{", "\\}"),
        "vmatrix" => ("|", "|"),
        "Vmatrix" => ("\\|", "\\|"),
        _ => ("", ""),
    };
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let mut spec = String::new();
    for column in 1..=columns {
        spec.push(layout.align);
        if column < columns && layout.vlines.contains(&column) {
            spec.push('|');
        }
    }
    if !open.is_empty() {
        ctx.push("\\left");
        ctx.push(open);
    }
    ctx.push("\\begin{array}{");
    ctx.push(&spec);
    ctx.push("}\n");
    emit_rows(rows, &layout.hlines, ctx);
    ctx.push("\\end{array}");
    if !close.is_empty() {
        ctx.push("\\right");
        ctx.push(close);
    }
    ctx.last_token = TokenType::Command;
}

/// The rows of a matrix, with `\hline` before the rows counted in `hlines`.
fn emit_rows(rows: &[Vec<MathIr>], hlines: &[usize], ctx: &mut ConvertContext) {
    for (row_index, row) in rows.iter().enumerate() {
        ctx.push("  ");
        if row_index > 0 && hlines.contains(&row_index) {
            ctx.push("\\hline ");
        }
        for (cell_index, cell) in row.iter().enumerate() {
            if cell_index > 0 {
                ctx.push(" & ");
            }
            ctx.push(&emit_math_ir_to_string(cell, ctx));
        }
        if row_index + 1 < rows.len() {
            ctx.push(" \\\\\n");
        } else {
            ctx.push("\n");
        }
    }
}

fn emit_command(command: &MathCommand, ctx: &mut ConvertContext) {
    ctx.push(&command.latex);
    if let Some(optional_arg) = &command.optional_arg {
        ctx.push("[");
        ctx.push(&emit_math_ir_to_string(optional_arg, ctx));
        ctx.push("]");
    }

    for arg in &command.args {
        ctx.push("{");
        match arg {
            // Text is typeset as written, so its edge spaces stay:
            // `x " and " y` → `x \text{ and } y`.
            MathIr::RawLiteral(text) if command.latex == "\\text" => ctx.push(text),
            _ => ctx.push(&emit_math_ir_to_string(arg, ctx)),
        }
        ctx.push("}");
    }

    ctx.last_token = TokenType::Command;
}

fn emit_environment(environment: &MathEnvironment, ctx: &mut ConvertContext) {
    match environment {
        MathEnvironment::Matrix {
            name,
            rows,
            layout: Some(layout),
        } => emit_array(name, rows, layout, ctx),
        MathEnvironment::Matrix { name, rows, .. } => {
            ctx.push("\\begin{");
            ctx.push(name);
            ctx.push("}\n");
            emit_rows(rows, &[], ctx);
            ctx.push("\\end{");
            ctx.push(name);
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }
        MathEnvironment::Cases { rows } => {
            ctx.push("\\begin{cases}\n");
            for (index, row) in rows.iter().enumerate() {
                ctx.push("  ");
                ctx.push(&emit_math_ir_to_string(row, ctx));
                if index + 1 < rows.len() {
                    ctx.push(" \\\\\n");
                } else {
                    ctx.push("\n");
                }
            }
            ctx.push("\\end{cases}");
            ctx.last_token = TokenType::Command;
        }
    }
}

fn emit_linebreak(ctx: &mut ConvertContext) {
    // A Typst math line break (`\`) maps to LaTeX's `\\` row separator in a row
    // context (`align`, top-level display). Inside an inline fragment (script,
    // matrix cell, argument) a `\\` would be invalid, so it degrades to a bare
    // `\` there.
    if ctx.is_in_env(&EnvironmentContext::Align) || ctx.linebreak_as_row {
        emit_raw_literal(" \\\\\n", ctx);
    } else {
        emit_raw_literal(" \\\n", ctx);
    }
}

fn emit_raw_literal(text: &str, ctx: &mut ConvertContext) {
    ctx.push(text);
    let last_non_space = text.chars().rev().find(|ch| !ch.is_whitespace());
    ctx.last_token = match last_non_space {
        Some('}') | Some(']') => TokenType::Command,
        Some(')') => TokenType::CloseParen,
        Some('(') | Some('[') => TokenType::OpenParen,
        Some(',') | Some(';') | Some(':') => TokenType::Punctuation,
        Some(ch) if ch.is_ascii_digit() => TokenType::Number,
        Some(ch) if ch.is_alphabetic() => TokenType::Letter,
        Some(_) => TokenType::Operator,
        None => TokenType::None,
    };
}

fn delimiter_token_type(delim: &str, is_open: bool) -> TokenType {
    if delim.starts_with("\\left") || delim.starts_with("\\right") || delim.starts_with('\\') {
        if is_open {
            TokenType::OpenParen
        } else {
            TokenType::CloseParen
        }
    } else if is_open {
        TokenType::OpenParen
    } else {
        TokenType::CloseParen
    }
}

#[cfg(test)]
mod tests {
    use super::{
        can_strip_wrapping_left_right_parens, can_strip_wrapping_plain_parens, emit_math_ir,
        emit_math_ir_to_string, ConvertContext, EnvironmentContext, MathIr, MathSpacing,
    };

    #[test]
    fn test_linebreak_ir_emits_double_backslash_in_row_context() {
        // At the top level (align/display rows) a Typst line break becomes
        // LaTeX's `\\` row separator. emit_math_ir runs on a row-context context.
        let mut ctx = ConvertContext::new();
        ctx.in_math = true;
        ctx.push_env(EnvironmentContext::Align);
        emit_math_ir(
            &MathIr::Seq(vec![
                MathIr::Ident("a".to_string()),
                MathIr::Linebreak,
                MathIr::Ident("b".to_string()),
            ]),
            &mut ctx,
        );
        let result = ctx.finalize();
        assert!(
            result.contains("a \\\\\n") && result.contains("b"),
            "row-context line break should emit `\\\\`, got: {}",
            result
        );
    }

    #[test]
    fn test_linebreak_compatibility_flag_remains_honored() {
        // `linebreak_as_row` is public in the 0.3.x API. Internal conversion
        // now uses EnvironmentContext::Align, but direct callers that set the
        // established compatibility flag must retain the old behavior.
        let mut ctx = ConvertContext::new();
        ctx.in_math = true;
        ctx.linebreak_as_row = true;
        emit_math_ir(
            &MathIr::Seq(vec![
                MathIr::Ident("a".to_string()),
                MathIr::Linebreak,
                MathIr::Ident("b".to_string()),
            ]),
            &mut ctx,
        );
        assert!(
            ctx.finalize().contains("a \\\\\n"),
            "the public compatibility flag must still emit a row break"
        );
    }

    #[test]
    fn test_linebreak_ir_preserves_plain_math_emission() {
        let result = emit_math_ir_to_string(
            &MathIr::Seq(vec![
                MathIr::Ident("i".to_string()),
                MathIr::Linebreak,
                MathIr::Ident("j".to_string()),
            ]),
            &ConvertContext::new(),
        );

        assert!(
            result.contains("i \\\n") && result.contains("j"),
            "linebreak inside an inline fragment (to_string) degrades to a bare `\\`, got: {}",
            result
        );
    }

    #[test]
    fn test_limits_attachment_multiline_bottom_uses_substack() {
        let result = emit_math_ir_to_string(
            &MathIr::Attachment {
                base: Box::new(MathIr::Symbol(r"\sum".to_string())),
                top: Some(Box::new(MathIr::Ident("n".to_string()))),
                bottom: Some(Box::new(MathIr::Seq(vec![
                    MathIr::Ident("i".to_string()),
                    MathIr::Spacing(MathSpacing::Soft),
                    MathIr::Linebreak,
                    MathIr::Ident("j".to_string()),
                ]))),
                top_left: None,
                top_right: None,
                bottom_left: None,
                bottom_right: None,
            },
            &ConvertContext::new(),
        );

        assert!(
            result.contains(r#"\sum_{\substack{i \\ j}}^n"#),
            "limits-like attachment bottom should use substack, got: {}",
            result
        );
    }

    #[test]
    fn test_strip_plain_grouping_parentheses_requires_outer_pair() {
        assert!(can_strip_wrapping_plain_parens("(i = 1)"));
        assert!(!can_strip_wrapping_plain_parens("(a + b)(c + d)"));
        assert!(!can_strip_wrapping_plain_parens("((a + b)"));
    }

    #[test]
    fn test_strip_left_right_grouping_parentheses_requires_outer_pair() {
        assert!(can_strip_wrapping_left_right_parens(r"\left(i = 1\right)"));
        assert!(!can_strip_wrapping_left_right_parens(
            r"\left(a + b\right)\left(c + d\right)"
        ));
    }
}
