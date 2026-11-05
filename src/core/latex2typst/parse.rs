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

/// Recover mitex-rs/mitex#225 without rewriting literal source.
pub(crate) fn parse_latex(input: &str, spec: CommandSpec) -> ParsedLatex {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut literals = LiteralStore::new(input, "TylaxParserLiteral");
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
                    let wraps = spec.get_cmd(&input[start..end]).is_some_and(|cmd| {
                        matches!(cmd.args, ArgShape::Left1 | ArgShape::InfixGreedy)
                    });
                    if follows_script && wraps {
                        edits.push((i..i, "{".into()));
                        edits.push((end..end, "}".into()));
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
