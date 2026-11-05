//! Shared byte-preserving lexical boundaries for parsing and diagnostics.

/// Literal environments; exclude `alltt` because it still expands commands.
const LITERAL_ENVS: &[&str] = &[
    "verbatim",
    "verbatim*",
    "Verbatim",
    "Verbatim*",
    "lstlisting",
    "minted",
];

/// Index existing markers once; restore generated markers in one forward pass.
/// Restored literal text is never searched again, even if it resembles a marker.
pub(crate) struct LiteralStore {
    prefix: String,
    occupied: std::collections::HashSet<usize>,
    values: std::collections::HashMap<usize, String>,
    next: usize,
}

impl LiteralStore {
    pub fn new(input: &str, prefix: &str) -> Self {
        let prefix = format!("\u{E010}{prefix}");
        let occupied = input
            .match_indices(&prefix)
            .filter_map(|(at, _)| Self::marker_id(&input[at + prefix.len()..]).map(|(id, _)| id))
            .collect();
        Self {
            prefix,
            occupied,
            values: Default::default(),
            next: 0,
        }
    }

    fn marker_id(rest: &str) -> Option<(usize, usize)> {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        rest.get(digits..)?.strip_prefix("X\u{E011}")?;
        Some((rest[..digits].parse().ok()?, digits + "X\u{E011}".len()))
    }

    pub fn protect(&mut self, literal: String) -> String {
        while self.occupied.contains(&self.next) {
            self.next += 1;
        }
        let id = self.next;
        self.next += 1;
        self.values.insert(id, literal);
        format!("{}{id}X\u{E011}", self.prefix)
    }

    pub fn restore(&self, input: String) -> String {
        if self.values.is_empty() {
            return input;
        }
        let mut output = String::with_capacity(input.len());
        let mut copied = 0;
        for (at, _) in input.match_indices(&self.prefix) {
            if let Some((id, length)) = Self::marker_id(&input[at + self.prefix.len()..]) {
                if let Some(value) = self.values.get(&id) {
                    output.push_str(&input[copied..at]);
                    output.push_str(value);
                    copied = at + self.prefix.len() + length;
                }
            }
        }
        output.push_str(&input[copied..]);
        output
    }
}

/// Remove only the line ending adjacent to each tag, keeping spaces and tabs.
pub(crate) fn literal_content(body: &str) -> &str {
    let body = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body);
    body.strip_suffix("\r\n")
        .or_else(|| body.strip_suffix('\n'))
        .unwrap_or(body)
}

pub(crate) struct InlineVerb<'a> {
    pub content: &'a str,
    pub end: usize,
}

/// Parse a closed inline `\verb`/`\verb*` span.
pub(crate) fn inline_verb(input: &str, start: usize) -> Option<InlineVerb<'_>> {
    let after = input.get(start..)?.strip_prefix("\\verb")?;
    let mut chars = after.char_indices();
    let (_, first) = chars.next()?;
    // A real inline `\verb` is never followed by a letter.
    if first.is_ascii_alphabetic() {
        return None;
    }
    let (delim_offset, delim) = if first == '*' {
        chars.next()?
    } else {
        (0, first)
    };
    let content_start = start + "\\verb".len() + delim_offset + delim.len_utf8();
    let close = input.get(content_start..)?.find(delim)?;
    Some(InlineVerb {
        content: &input[content_start..content_start + close],
        end: content_start + close + delim.len_utf8(),
    })
}

/// Environment name and exclusive end of its tag.
pub(crate) struct EnvTag<'a> {
    pub(crate) name: &'a str,
    pub(crate) end: usize,
}

/// Skip TeX whitespace and comments before an argument.
pub(crate) fn skip_spaces_and_comments(input: &str, start: usize) -> usize {
    let bytes = input.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                index = input[index..]
                    .find('\n')
                    .map_or(bytes.len(), |p| index + p + 1);
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            _ => break,
        }
    }
    index
}

/// Parse a `\begin`/`\end` tag.
pub(crate) fn parse_env_tag<'a>(input: &'a str, start: usize, keyword: &str) -> Option<EnvTag<'a>> {
    let after_keyword = input.get(start..)?.strip_prefix(keyword)?;
    // A control word is not a prefix of a longer one: `\beginning`, `\endinput`.
    if after_keyword
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
    {
        return None;
    }
    let argument = skip_spaces_and_comments(input, start + keyword.len());
    let braced = input.get(argument..)?.strip_prefix('{')?;
    let close = braced.find('}')?;
    Some(EnvTag {
        name: &braced[..close],
        end: argument + '{'.len_utf8() + close + '}'.len_utf8(),
    })
}

/// Find the next tag, skipping longer command names.
pub(crate) fn find_env_tag<'a>(
    input: &'a str,
    from: usize,
    keyword: &str,
) -> Option<(usize, EnvTag<'a>)> {
    let mut search = from;
    while let Some(relative) = input.get(search..)?.find(keyword) {
        let at = search + relative;
        if let Some(tag) = parse_env_tag(input, at, keyword) {
            return Some((at, tag));
        }
        search = at + keyword.len();
    }
    None
}

/// Keep malformed headers visible to normal diagnostics.
fn literal_env_header_end(input: &str, env: &str, after_tag: usize) -> Option<usize> {
    // A bare newline starts code. A comment consumes its newline, so a header
    // argument may continue on the next physical line.
    fn skip_header_trivia(input: &str, start: usize) -> (usize, usize) {
        let bytes = input.as_bytes();
        let mut index = start;
        let mut after_comments = start;
        loop {
            while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
                index += 1;
            }
            if bytes.get(index) != Some(&b'%') {
                return (index, after_comments);
            }
            index = input[index..]
                .find('\n')
                .map_or(input.len(), |newline| index + newline + 1);
            after_comments = index;
        }
    }

    // Delimited group that must be closed if it is opened at all.
    fn skip_group(
        input: &str,
        env: &str,
        start: usize,
        open: char,
        close: char,
        required: bool,
    ) -> Option<usize> {
        let (at, after_comments) = skip_header_trivia(input, start);
        match input.get(at..)?.strip_prefix(open) {
            Some(rest) => {
                let mut depth = 1usize;
                let mut braces = 0usize;
                let mut chars = rest.char_indices();
                while let Some((offset, ch)) = chars.next() {
                    match ch {
                        '\\' => {
                            let index = at + open.len_utf8() + offset;
                            if parse_env_tag(input, index, "\\end")
                                .is_some_and(|tag| tag.name == env)
                            {
                                return None;
                            }
                            chars.next();
                        }
                        '%' => {
                            for (_, c) in chars.by_ref() {
                                if c == '\n' {
                                    break;
                                }
                            }
                        }
                        '{' if open != '{' => braces += 1,
                        '}' if open != '{' => braces = braces.saturating_sub(1),
                        c if c == open && braces == 0 => depth += 1,
                        c if c == close && braces == 0 => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(at + open.len_utf8() + offset + ch.len_utf8());
                            }
                        }
                        _ => {}
                    }
                }
                None
            }
            None if required => None,
            // Preserve indentation after header comments.
            None => Some(after_comments),
        }
    }

    let mut position = after_tag;
    // fancyvrb, listings and minted all take an optional `[...]` option list.
    if matches!(env, "Verbatim" | "Verbatim*" | "lstlisting" | "minted") {
        position = skip_group(input, env, position, '[', ']', false)?;
    }
    // minted additionally requires `{<language>}`.
    if env == "minted" {
        position = skip_group(input, env, position, '{', '}', true)?;
    }
    Some(position)
}

pub(crate) struct LiteralEnv<'a> {
    pub name: &'a str,
    pub header: std::ops::Range<usize>,
    pub body: std::ops::Range<usize>,
    pub end: Option<usize>,
}

/// Parse the header, then locate the end in literal source.
pub(crate) fn literal_env(input: &str, start: usize) -> Option<LiteralEnv<'_>> {
    let tag = parse_env_tag(input, start, "\\begin")?;
    if !LITERAL_ENVS.contains(&tag.name) {
        return None;
    }

    // Locate the matching `\end`, tolerating the same separators as the opener.
    let body_start = literal_env_header_end(input, tag.name, tag.end)?;
    let mut search = body_start;
    let (body_end, end) = loop {
        match find_env_tag(input, search, "\\end") {
            Some((at, end_tag)) => {
                if end_tag.name == tag.name {
                    break (at, Some(end_tag.end));
                }
                search = end_tag.end;
            }
            // Keep the opening tag visible for unterminated environments.
            None => break (input.len(), None),
        }
    };
    Some(LiteralEnv {
        name: tag.name,
        header: tag.end..body_start,
        body: body_start..body_end,
        end,
    })
}
