//! Utility functions for token stream parsing

use super::input::BOUNDARY;
use super::token::{TexToken, TokenList};

/// Maximum number of tokens to read in a single argument.
/// This prevents runaway parsing when input contains unclosed braces.
pub(crate) const MAX_ARG_TOKENS: usize = 10000;

/// A token stream with one token of lookahead.
///
/// Implemented by `Peekable` iterators and by the expansion engine's input
/// stack, so the argument and definition readers work on either.
pub(crate) trait TokenSource: Iterator<Item = TexToken> {
    /// Look at the next token without consuming it.
    fn peek_token(&mut self) -> Option<&TexToken>;

    /// Whether an argument may be longer than [`MAX_ARG_TOKENS`]: the source
    /// takes back what was read past them when the input ends before the
    /// argument does (see [`TokenSource::give_back`]).
    fn unbounded(&self) -> bool {
        false
    }

    /// Take back `tokens`, read past the first [`MAX_ARG_TOKENS`] of an
    /// argument that the input ended in; they are read again next.
    fn give_back(&mut self, _tokens: Vec<TexToken>) {}

    /// Whether the token read last was not written right after the one
    /// before it (the expansion engine's input stack, while it reads a
    /// definition). A body read from such a source keeps that boundary
    /// between a control sequence and an `@` (see [`read_bounded`]).
    fn boundary(&self) -> bool {
        false
    }
}

impl<I: Iterator<Item = TexToken>> TokenSource for std::iter::Peekable<I> {
    fn peek_token(&mut self) -> Option<&TexToken> {
        self.peek()
    }
}

/// Skip space and comment tokens
pub fn skip_spaces<I>(iter: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = TexToken>,
{
    skip_spaces_in(iter)
}

/// Skip space and comment tokens in any [`TokenSource`].
pub(crate) fn skip_spaces_in<S: TokenSource>(iter: &mut S) {
    while matches!(
        iter.peek_token(),
        Some(TexToken::Space) | Some(TexToken::Comment(_))
    ) {
        iter.next();
    }
}

/// Read a control sequence name from the iterator
#[allow(clippy::result_unit_err)]
pub fn read_control_seq_name<I>(iter: &mut std::iter::Peekable<I>) -> Result<String, ()>
where
    I: Iterator<Item = TexToken>,
{
    read_control_seq_name_in(iter)
}

/// Read a control sequence name from any [`TokenSource`].
pub(crate) fn read_control_seq_name_in<S: TokenSource>(iter: &mut S) -> Result<String, ()> {
    skip_spaces_in(iter);
    match iter.next() {
        Some(TexToken::ControlSeq(name)) => Ok(name),
        _ => Err(()),
    }
}

/// Read a number from the iterator
#[allow(clippy::result_unit_err)]
pub fn read_number<I>(iter: &mut std::iter::Peekable<I>) -> Result<u8, ()>
where
    I: Iterator<Item = TexToken>,
{
    read_number_in(iter)
}

/// Read a number from any [`TokenSource`].
pub(crate) fn read_number_in<S: TokenSource>(iter: &mut S) -> Result<u8, ()> {
    skip_spaces_in(iter);
    let mut num_str = String::new();
    while let Some(TexToken::Char(c)) = iter.peek_token() {
        if c.is_ascii_digit() {
            num_str.push(*c);
            iter.next();
        } else {
            break;
        }
    }
    num_str.parse().map_err(|_| ())
}

/// Skip tokens until a specific character is encountered
pub fn skip_until_char<I>(iter: &mut I, end_char: char)
where
    I: Iterator<Item = TexToken>,
{
    for token in iter {
        if matches!(token, TexToken::Char(c) if c == end_char) {
            break;
        }
    }
}

/// Read tokens until a specific character is encountered, respecting braces.
///
/// Safety: Limited to `MAX_ARG_TOKENS` to prevent runaway parsing.
pub fn read_until_char<I>(iter: &mut I, end_char: char) -> TokenList
where
    I: Iterator<Item = TexToken>,
{
    TokenList::from_vec(read_until(iter, Some(end_char), MAX_ARG_TOKENS, |_| false).0)
}

/// Read tokens inside a balanced group { ... }
///
/// Safety: Limited to `MAX_ARG_TOKENS` to prevent runaway parsing
/// when input contains unclosed braces.
pub fn read_balanced_group<I>(iter: &mut I) -> TokenList
where
    I: Iterator<Item = TexToken>,
{
    TokenList::from_vec(read_until(iter, None, MAX_ARG_TOKENS, |_| false).0)
}

/// [`read_until_char`] for any [`TokenSource`] (see [`read_bounded`]).
pub(crate) fn read_until_char_in<S: TokenSource>(iter: &mut S, end_char: char) -> TokenList {
    read_bounded(iter, Some(end_char))
}

/// [`read_balanced_group`] for any [`TokenSource`] (see [`read_bounded`]).
pub(crate) fn read_balanced_group_in<S: TokenSource>(iter: &mut S) -> TokenList {
    read_bounded(iter, None)
}

/// Read up to `end_char`, or to the end of the group, from a source. An
/// unbounded source has no limit on the length of what is read; if its input
/// ends first, everything after the first `MAX_ARG_TOKENS` tokens is given
/// back, which leaves the source as the limited readers would. Where the
/// source reports a [`TokenSource::boundary`] between a control sequence and
/// an `@`, a [`BOUNDARY`] is kept.
fn read_bounded<S: TokenSource>(iter: &mut S, end_char: Option<char>) -> TokenList {
    if !iter.unbounded() {
        return TokenList::from_vec(read_until(iter, end_char, MAX_ARG_TOKENS, S::boundary).0);
    }
    let (mut tokens, found) = read_until(iter, end_char, usize::MAX, S::boundary);
    if !found && tokens.len() > MAX_ARG_TOKENS {
        let mut rest = tokens.split_off(MAX_ARG_TOKENS);
        rest.retain(|t| *t != BOUNDARY);
        iter.give_back(rest);
    }
    TokenList::from_vec(tokens)
}

/// Read tokens up to `end_char` at brace depth 0, or with `None` up to the
/// brace that closes the group already open, neither included; at most
/// `limit` of them. Also tells whether the end was found. A [`BOUNDARY`] is
/// put between a control sequence and an `@` that `boundary` separates.
fn read_until<I>(
    iter: &mut I,
    end_char: Option<char>,
    limit: usize,
    boundary: impl Fn(&I) -> bool,
) -> (Vec<TexToken>, bool)
where
    I: Iterator<Item = TexToken>,
{
    let mut result = Vec::new();
    let mut depth = usize::from(end_char.is_none());

    while let Some(token) = iter.next() {
        if result.len() >= limit {
            // Safety limit reached - return what we have
            break;
        }
        if token == TexToken::Char('@')
            && matches!(result.last(), Some(TexToken::ControlSeq(_)))
            && boundary(iter)
        {
            result.push(BOUNDARY);
        }
        match &token {
            TexToken::BeginGroup => depth += 1,
            TexToken::EndGroup if depth == 1 && end_char.is_none() => return (result, true),
            TexToken::EndGroup => depth = depth.saturating_sub(1),
            TexToken::Char(c) if Some(*c) == end_char && depth == 0 => return (result, true),
            _ => {}
        }
        result.push(token);
    }

    (result, false)
}

/// Read a single macro argument (braced group or single token)
pub fn read_argument<I>(iter: &mut std::iter::Peekable<I>) -> TokenList
where
    I: Iterator<Item = TexToken>,
{
    read_argument_in(iter)
}

/// Read a single macro argument from any [`TokenSource`].
pub(crate) fn read_argument_in<S: TokenSource>(iter: &mut S) -> TokenList {
    skip_spaces_in(iter);

    match iter.peek_token() {
        Some(TexToken::BeginGroup) => {
            iter.next();
            read_balanced_group_in(iter)
        }
        Some(_) => {
            // SAFETY: peek() returned Some, so next() is guaranteed to return Some
            let token = iter.next().expect("peek succeeded");
            TokenList::from_vec(vec![token])
        }
        None => TokenList::new(),
    }
}
