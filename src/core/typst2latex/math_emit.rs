use super::context::{ConvertContext, TokenType};
use super::math_ir::{
    normalize_math_ir, MathCaseRow, MathCommand, MathEnvironment, MathIr, MathSpacing,
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
        MathIr::Limits(content) => emit_math_ir(content, ctx),
        MathIr::Style { mode, content } => emit_style(mode, content, ctx),
        MathIr::Attachment { .. } => emit_attachment(ir, ctx),
        MathIr::Script { base, sub, sup } => emit_script(base, sub.as_deref(), sup.as_deref(), ctx),
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

pub fn emit_math_ir_to_string(ir: &MathIr, template: &ConvertContext) -> String {
    let mut nested = ConvertContext::new();
    nested.options = template.options.clone();
    nested.in_math = true;
    emit_math_ir(ir, &mut nested);
    nested.finalize()
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

fn emit_script(
    base: &MathIr,
    sub: Option<&MathIr>,
    sup: Option<&MathIr>,
    ctx: &mut ConvertContext,
) {
    emit_math_ir(base, ctx);

    if let Some(sub) = sub {
        ctx.push("_");
        ctx.last_token = TokenType::Operator;
        emit_script_content_for_base(sub, Some(base), ctx);
    }

    if let Some(sup) = sup {
        ctx.push("^");
        ctx.last_token = TokenType::Operator;
        emit_script_content_for_base(sup, Some(base), ctx);
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

fn emit_command(command: &MathCommand, ctx: &mut ConvertContext) {
    ctx.push(&command.latex);
    if let Some(optional_arg) = &command.optional_arg {
        ctx.push("[");
        ctx.push(&emit_math_ir_to_string(optional_arg, ctx));
        ctx.push("]");
    }

    for arg in &command.args {
        ctx.push("{");
        ctx.push(&emit_math_ir_to_string(arg, ctx));
        ctx.push("}");
    }

    ctx.last_token = TokenType::Command;
}

fn emit_environment(environment: &MathEnvironment, ctx: &mut ConvertContext) {
    match environment {
        MathEnvironment::Matrix { name, rows } => {
            ctx.push("\\begin{");
            ctx.push(name);
            ctx.push("}\n");
            for (row_index, row) in rows.iter().enumerate() {
                ctx.push("  ");
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
            ctx.push("\\end{");
            ctx.push(name);
            ctx.push("}");
            ctx.last_token = TokenType::Command;
        }
        MathEnvironment::Cases { rows } => {
            ctx.push("\\begin{cases}\n");
            for (index, row) in rows.iter().enumerate() {
                emit_case_row(row, ctx);
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

fn emit_case_row(row: &MathCaseRow, ctx: &mut ConvertContext) {
    ctx.push("  ");
    ctx.push(&emit_math_ir_to_string(&row.value, ctx));
    if let Some(condition) = &row.condition {
        ctx.push(" & ");
        ctx.push(&emit_math_ir_to_string(condition, ctx));
    }
}

fn emit_linebreak(ctx: &mut ConvertContext) {
    emit_raw_literal(" \\\n", ctx);
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
        can_strip_wrapping_left_right_parens, can_strip_wrapping_plain_parens,
        emit_math_ir_to_string, ConvertContext, MathIr, MathSpacing,
    };

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
            "linebreak IR should still emit a raw LaTeX linebreak in plain math, got: {}",
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
