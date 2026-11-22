//! Temporary MiTeX recovery with literal shielding and original-source offsets.

use std::borrow::Cow;
use std::ops::Range;

use mitex_parser::{
    syntax::{SyntaxKind, SyntaxNode},
    CommandSpec,
};
use mitex_spec::ArgShape;

use super::lexical::{
    inline_verb, literal_content, literal_env, parse_env_tag, skip_spaces_and_comments,
    LiteralStore,
};

pub(crate) struct ParsedLatex {
    pub tree: SyntaxNode,
    pub source_map: SourceMap,
    literals: LiteralStore,
}

impl ParsedLatex {
    /// Restore literals after conversion and cleanup.
    pub fn restore_literals(&self, output: String) -> String {
        self.literals.restore(output)
    }
}

#[derive(Default)]
pub(crate) struct SourceMap {
    // Prepared-source offsets -> original input.
    edits: Vec<(Range<usize>, Range<usize>)>,
    // MiTeX token offsets -> prepared source.
    parser_edits: Vec<(Range<usize>, Range<usize>)>,
}

impl SourceMap {
    pub fn original_offset(&self, offset: usize) -> usize {
        let prepared = Self::translate(&self.parser_edits, offset);
        Self::translate(&self.edits, prepared)
    }

    fn translate(edits: &[(Range<usize>, Range<usize>)], offset: usize) -> usize {
        let index = edits.partition_point(|(generated, _)| generated.start <= offset);
        let Some((generated, original)) = index.checked_sub(1).map(|i| &edits[i]) else {
            return offset;
        };
        if offset < generated.end {
            original.start + (offset - generated.start).min(original.len())
        } else {
            original.end + (offset - generated.end)
        }
    }

    /// Map contracted tags and discarded argument trivia.
    fn map_parser_tokens(&mut self, input: &str, tree: &SyntaxNode) {
        let mut cursor = 0;
        for token in tree
            .descendants_with_tokens()
            .filter_map(|node| node.into_token())
        {
            let keyword = match token.parent().map(|node| node.kind()) {
                Some(SyntaxKind::ItemBegin) => Some("\\begin"),
                Some(SyntaxKind::ItemEnd) => Some("\\end"),
                _ => None,
            };
            let environment_at = |at| {
                keyword
                    .and_then(|word| parse_env_tag(input, at, word))
                    .filter(|tag| tag.name == token.text())
            };
            let mut tag = environment_at(cursor);
            if tag.is_none()
                && !input
                    .get(cursor..)
                    .is_some_and(|rest| rest.starts_with(token.text()))
            {
                let next = skip_spaces_and_comments(input, cursor);
                if next == cursor
                    || (!input[next..].starts_with(token.text()) && environment_at(next).is_none())
                {
                    // Upstream macro expansion has no source spans; do not guess.
                    break;
                }
                let at = usize::from(token.text_range().start());
                self.parser_edits.push((at..at, cursor..next));
                cursor = next;
                tag = environment_at(cursor);
            }
            if let Some(tag) = tag {
                let range = token.text_range();
                self.parser_edits.push((
                    usize::from(range.start())..usize::from(range.end()),
                    cursor..tag.end,
                ));
                cursor = tag.end;
            } else {
                cursor += token.text().len();
            }
        }
    }
}

/// Skip a balanced `[...]` optional argument starting at `start`, returning the
/// byte offset right after the closing `]`.
fn skip_optional_bracket_arg(input: &str, start: usize) -> Option<usize> {
    if !input.get(start..)?.starts_with('[') {
        return None;
    }
    let mut braces = 0usize;
    let mut i = start + 1;
    while i < input.len() {
        let rest = &input[i..];
        let ch = rest.chars().next().unwrap();
        match ch {
            '%' => i += rest.find('\n').unwrap_or(rest.len()),
            '\\' => {
                if let Some(verb) = inline_verb(input, i) {
                    i = verb.end;
                    continue;
                }
                i += 1 + input[i + 1..].chars().next().map_or(0, char::len_utf8);
            }
            '{' => {
                braces += 1;
                i += 1;
            }
            '}' => {
                braces = braces.saturating_sub(1);
                i += 1;
            }
            ']' if braces == 0 => return Some(i + 1),
            _ => i += ch.len_utf8(),
        }
    }
    None
}

/// Recover mitex-rs/mitex#225 without rewriting literal source.
pub(crate) fn parse_latex(input: &str, spec: CommandSpec) -> ParsedLatex {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut literals = LiteralStore::new(input, "TylaxParserLiteral");
    let mut last_rbracket: Option<Option<usize>> = None;
    let mut i = 0;
    let mut follows_script = false;

    while i < input.len() {
        let rest = &input[i..];
        let ch = rest.chars().next().unwrap();
        match ch {
            '%' => {
                // Escaped percent signs are consumed with their control symbol below.
                i += rest.find('\n').unwrap_or(rest.len());
            }
            '\\' => {
                if let Some(verb) = inline_verb(input, i) {
                    i = verb.end;
                    follows_script = false;
                    continue;
                }
                if let Some(env) = literal_env(input, i) {
                    let body = env.body;
                    if !body.is_empty() {
                        let marker =
                            literals.protect(literal_content(&input[body.clone()]).to_string());
                        edits.push((body.clone(), marker));
                    }
                    i = body.end;
                    follows_script = false;
                    continue;
                }
                let start = i + 1;
                let mut end = start;
                for c in input[start..].chars() {
                    if !c.is_ascii_alphabetic() {
                        break;
                    }
                    end += c.len_utf8();
                }
                if end == start {
                    // Consume an entire escaped Unicode character, not one of its bytes.
                    i = start + input[start..].chars().next().map_or(0, char::len_utf8);
                } else {
                    let name = &input[start..end];
                    if PRIMITIVE_SKIPS.contains(&name) {
                        if let Some((dimension, used)) = parse_glue(&input[end..]) {
                            edits.push((i..end + used, format!("\\hspace{{{dimension}}}")));
                            i = end + used;
                            follows_script = false;
                            continue;
                        }
                    }
                    let wraps = spec.get_cmd(name).is_some_and(|cmd| {
                        matches!(cmd.args, ArgShape::Left1 | ArgShape::InfixGreedy)
                    });
                    if follows_script && wraps {
                        edits.push((i..i, "{".into()));
                        edits.push((end..end, "}".into()));
                    }
                    // TeX takes one token as `\sqrt`'s radicand, so `\sqrt(2)` and
                    // `\sqrt[3](2)` take `(` as the radicand, whereas mitex binds a
                    // symbol to nothing.
                    if name == "sqrt" {
                        let mut sym_pos = skip_spaces_and_comments(input, end);
                        if input.as_bytes().get(sym_pos) == Some(&b'[')
                            && last_rbracket
                                .get_or_insert_with(|| input.rfind(']'))
                                .is_some_and(|last| last > sym_pos)
                        {
                            if let Some(after_opt) = skip_optional_bracket_arg(input, sym_pos) {
                                sym_pos = skip_spaces_and_comments(input, after_opt);
                            }
                        }
                        if let Some(symbol) = input[sym_pos..]
                            .chars()
                            .next()
                            .filter(|&c| "()|<>.,;:!?+-=*/".contains(c))
                        {
                            let sym_end = sym_pos + symbol.len_utf8();
                            edits.push((sym_pos..sym_pos, "{".into()));
                            edits.push((sym_end..sym_end, "}".into()));
                        }
                    }
                    i = end;
                }
                follows_script = false;
            }
            c if c.is_whitespace() => i += c.len_utf8(),
            c => {
                follows_script = matches!(c, '_' | '^');
                i += c.len_utf8();
            }
        }
    }

    let mut source_map = SourceMap::default();
    let prepared = if edits.is_empty() {
        Cow::Borrowed(input)
    } else {
        edits.sort_by_key(|(range, _)| range.start);
        let mut output = String::with_capacity(input.len());
        let mut copied = 0;
        for (original, replacement) in edits {
            output.push_str(&input[copied..original.start]);
            let generated_start = output.len();
            output.push_str(&replacement);
            copied = original.end;
            source_map
                .edits
                .push((generated_start..output.len(), original));
        }
        output.push_str(&input[copied..]);
        Cow::Owned(output)
    };
    let tree = mitex_parser::parse(&prepared, spec);
    source_map.map_parser_tokens(&prepared, &tree);
    ParsedLatex {
        tree,
        source_map,
        literals,
    }
}

/// TeX primitives whose dimension follows them unbraced.
const PRIMITIVE_SKIPS: [&str; 4] = ["hskip", "kern", "mkern", "mskip"];

/// Parse a TeX dimension, then any `plus`/`minus` glue, at the start of `s`.
/// Returns the dimension with spaces removed and the bytes consumed.
pub(super) fn parse_glue(s: &str) -> Option<(String, usize)> {
    let (dimension, mut used) = parse_tex_dimension(s, false)?;
    for keyword in ["plus", "minus"] {
        let rest = &s[used..];
        let trimmed = rest.trim_start();
        if let Some(after) = trimmed.strip_prefix(keyword) {
            let skipped = rest.len() - after.len();
            if let Some((_, glue_used)) = parse_tex_dimension(after, true) {
                used += skipped + glue_used;
            }
        }
    }
    Some((dimension, used))
}

/// Parse `[sign] number unit` at the start of `s`, allowing TeX's spaces.
/// `fil` units (`1fill`) are accepted only for glue.
fn parse_tex_dimension(s: &str, allow_fil: bool) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let skip_spaces = |i: &mut usize| {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
    };
    let mut dimension = String::new();
    skip_spaces(&mut i);
    while i < bytes.len() && matches!(bytes[i], b'+' | b'-') {
        if bytes[i] == b'-' {
            dimension.push('-');
        }
        i += 1;
        skip_spaces(&mut i);
    }
    let number_start = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || matches!(bytes[i], b'.' | b',')) {
        i += 1;
    }
    let number = s[number_start..i].replace(',', ".");
    if !number.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    if number.starts_with('.') {
        dimension.push('0');
    }
    dimension.push_str(&number);
    skip_spaces(&mut i);
    // A unit is two letters, so `1cmv` is `1cm` then `v`; glue also takes
    // `fil`, `fill` and `filll`.
    let rest = &s[i..];
    let fil = ["filll", "fill", "fil"]
        .into_iter()
        .find(|unit| allow_fil && rest.starts_with(unit));
    let unit = fil.or_else(|| {
        [
            "pt", "mm", "cm", "in", "em", "ex", "pc", "bp", "px", "mu", "dd", "cc", "sp",
        ]
        .into_iter()
        .find(|unit| rest.starts_with(unit))
    })?;
    dimension.push_str(unit);
    Some((dimension, i + unit.len()))
}
