//! TeX Primitive Commands for Macro Definitions
//!
//! This module handles parsing of definition commands:
//! - `\newcommand`, `\renewcommand`, `\providecommand`
//! - `\def`
//! - `\let`

use super::token::{TexToken, TokenList};
use super::utils::{self, TokenSource};

/// A single part of a macro parameter pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternPart {
    /// A parameter placeholder (e.g., #1)
    Argument(u8),
    /// Exact tokens that must match in the input stream.
    Literal(Vec<TexToken>),
}

/// Describes how a macro's arguments should be parsed.
#[derive(Debug, Clone, PartialEq)]
pub enum MacroSignature {
    /// Standard LaTeX style: simple positional arguments.
    /// Optimized path for 90% of macros (\newcommand, etc.)
    Simple(u8),
    /// TeX primitive style: pattern-based matching with delimiters.
    /// e.g., \def\foo#1=#2. produces Pattern([Arg(1), Literal(=), Arg(2), Literal(.)])
    Pattern(Vec<PatternPart>),
}

impl MacroSignature {
    /// Get the number of arguments
    pub fn num_args(&self) -> u8 {
        match self {
            MacroSignature::Simple(n) => *n,
            MacroSignature::Pattern(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    PatternPart::Argument(n) => Some(*n),
                    _ => None,
                })
                .max()
                .unwrap_or(0),
        }
    }
}

/// Represents a parsed definition
#[derive(Debug, Clone)]
pub enum DefinitionKind {
    /// \newcommand{\name}[n][default]{body}
    NewCommand {
        name: String,
        num_args: u8,
        default: Option<TokenList>,
        body: TokenList,
    },
    /// \renewcommand (same structure as newcommand)
    RenewCommand {
        name: String,
        num_args: u8,
        default: Option<TokenList>,
        body: TokenList,
    },
    /// \providecommand (same structure)
    ProvideCommand {
        name: String,
        num_args: u8,
        default: Option<TokenList>,
        body: TokenList,
    },
    /// \def\name#1#2{body}
    Def {
        name: String,
        signature: MacroSignature,
        body: TokenList,
    },
    /// \edef\name#1#2{body} - expanded at definition time
    Edef {
        name: String,
        signature: MacroSignature,
        body: TokenList,
    },
    /// \let\name=\target or \let\name\target
    Let { name: String, target: String },
    /// \newenvironment{name}[n][default]{begin}{end}
    /// Creates two macros: \name and \endname
    NewEnvironment {
        name: String,
        num_args: u8,
        default: Option<TokenList>,
        begin_body: TokenList,
        end_body: TokenList,
    },
    /// \renewenvironment (same structure)
    RenewEnvironment {
        name: String,
        num_args: u8,
        default: Option<TokenList>,
        begin_body: TokenList,
        end_body: TokenList,
    },
    /// \newif\iffoo creates:
    /// - \iffoo (initially \iffalse)
    /// - \footrue (sets \iffoo to \iftrue)
    /// - \foofalse (sets \iffoo to \iffalse)
    NewIf {
        /// The base name (e.g., "foo" from \iffoo)
        base_name: String,
    },
    /// \DeclareMathOperator{\name}{text}
    DeclareMathOperator {
        name: String,
        body: TokenList,
        is_starred: bool,
    },
}

/// Check if a control sequence name is a definition command
pub fn is_definition_command(name: &str) -> bool {
    matches!(
        name,
        "newcommand"
            | "renewcommand"
            | "providecommand"
            | "NewDocumentCommand"
            | "RenewDocumentCommand"
            | "DeclareMathOperator"
            | "DeclareRobustCommand"
            | "newenvironment"
            | "renewenvironment"
            | "def"
            | "edef"
            | "gdef"
            | "xdef"
            | "let"
            | "newif"
    )
}

/// Parse a definition from the token stream
///
/// Returns the parsed definition and remaining tokens, or Err with original tokens if parsing fails
pub fn parse_definition(
    cmd_name: &str,
    tokens: TokenList,
) -> Result<(DefinitionKind, TokenList), TokenList> {
    match cmd_name {
        "newcommand" | "renewcommand" | "providecommand" | "DeclareRobustCommand" => {
            parse_newcommand_style(cmd_name, tokens)
        }
        "NewDocumentCommand" | "RenewDocumentCommand" => parse_xparse_style(cmd_name, tokens),
        "DeclareMathOperator" => parse_declare_math_operator(tokens),
        "newenvironment" | "renewenvironment" => parse_newenvironment(cmd_name, tokens),
        "def" | "gdef" => parse_def(tokens, false),
        "edef" | "xdef" => parse_def(tokens, true),
        "let" => parse_let(tokens),
        "newif" => parse_newif(tokens),
        _ => Err(tokens),
    }
}

/// Read a definition directly from a token stream, consuming only the tokens
/// that belong to it (name, parameter text and body) and leaving the rest of
/// the stream untouched. Returns `None` if the definition is malformed; the
/// tokens read so far are consumed either way, as with [`parse_definition`].
pub(crate) fn read_definition<S: TokenSource>(
    cmd_name: &str,
    iter: &mut S,
) -> Option<DefinitionKind> {
    match cmd_name {
        "newcommand" | "renewcommand" | "providecommand" | "DeclareRobustCommand" => {
            read_newcommand_style(cmd_name, iter)
        }
        "NewDocumentCommand" | "RenewDocumentCommand" => read_xparse_style(cmd_name, iter),
        "DeclareMathOperator" => read_declare_math_operator(iter),
        "newenvironment" | "renewenvironment" => read_newenvironment(cmd_name, iter),
        "def" | "gdef" => read_def(iter, false),
        "edef" | "xdef" => read_def(iter, true),
        "let" => read_let(iter),
        "newif" => read_newif(iter),
        _ => None,
    }
}

/// Run a stream reader over a token list, returning the definition and the
/// tokens it did not consume (or just those tokens if it failed).
fn parse_from_list<F>(tokens: TokenList, read: F) -> Result<(DefinitionKind, TokenList), TokenList>
where
    F: FnOnce(&mut std::iter::Peekable<std::vec::IntoIter<TexToken>>) -> Option<DefinitionKind>,
{
    let mut iter = tokens.into_inner().into_iter().peekable();
    let def = read(&mut iter);
    let rest = TokenList::from_vec(iter.collect());
    match def {
        Some(def) => Ok((def, rest)),
        None => Err(rest),
    }
}

/// Parse \newcommand, \renewcommand, \providecommand
fn parse_newcommand_style(
    cmd_name: &str,
    tokens: TokenList,
) -> Result<(DefinitionKind, TokenList), TokenList> {
    parse_from_list(tokens, |iter| read_newcommand_style(cmd_name, iter))
}

/// Parse xparse-style commands (simplified)
fn parse_xparse_style(
    cmd_name: &str,
    tokens: TokenList,
) -> Result<(DefinitionKind, TokenList), TokenList> {
    parse_from_list(tokens, |iter| read_xparse_style(cmd_name, iter))
}

/// Parse \def\name#1#2{body} or \edef\name#1#2{body}
fn parse_def(tokens: TokenList, is_edef: bool) -> Result<(DefinitionKind, TokenList), TokenList> {
    parse_from_list(tokens, |iter| read_def(iter, is_edef))
}

/// Parse \let\name=\target or \let\name\target
fn parse_let(tokens: TokenList) -> Result<(DefinitionKind, TokenList), TokenList> {
    parse_from_list(tokens, read_let)
}

/// Parse \newif\iffoo
fn parse_newif(tokens: TokenList) -> Result<(DefinitionKind, TokenList), TokenList> {
    parse_from_list(tokens, read_newif)
}

/// Parse \DeclareMathOperator{\name}{text}
fn parse_declare_math_operator(
    tokens: TokenList,
) -> Result<(DefinitionKind, TokenList), TokenList> {
    parse_from_list(tokens, read_declare_math_operator)
}

/// Parse \newenvironment{name}[n][default]{begin}{end}
fn parse_newenvironment(
    cmd_name: &str,
    tokens: TokenList,
) -> Result<(DefinitionKind, TokenList), TokenList> {
    parse_from_list(tokens, |iter| read_newenvironment(cmd_name, iter))
}

/// Parse definitions from a token list and return remaining tokens
pub fn parse_definitions(tokens: TokenList) -> (Vec<DefinitionKind>, TokenList) {
    let mut definitions = Vec::new();
    let mut result = Vec::new();
    let mut iter = tokens.into_inner().into_iter().peekable();

    while let Some(token) = iter.next() {
        match &token {
            TexToken::ControlSeq(name) if is_definition_command(name) => {
                let remaining: Vec<TexToken> = iter.collect();
                let remaining_list = TokenList::from_vec(remaining);

                match parse_definition(name, remaining_list) {
                    Ok((def, rest)) => {
                        definitions.push(def);
                        iter = rest.into_inner().into_iter().peekable();
                    }
                    Err(rest) => {
                        result.push(token);
                        iter = rest.into_inner().into_iter().peekable();
                    }
                }
            }
            _ => {
                result.push(token);
            }
        }
    }

    (definitions, TokenList::from_vec(result))
}

/// Read \newcommand, \renewcommand, \providecommand from a token stream
fn read_newcommand_style<S: TokenSource>(cmd_name: &str, iter: &mut S) -> Option<DefinitionKind> {
    // Skip spaces
    utils::skip_spaces_in(iter);

    // Check for star variant (ignored for now)
    if matches!(iter.peek_token(), Some(TexToken::Char('*'))) {
        iter.next();
        utils::skip_spaces_in(iter);
    }

    // Parse command name: either {\cmd} or \cmd
    let macro_name = match iter.peek_token() {
        Some(TexToken::BeginGroup) => {
            iter.next();
            let name = match utils::read_control_seq_name_in(iter) {
                Ok(n) => n,
                Err(_) => return None,
            };
            // Consume closing brace
            match iter.next() {
                Some(TexToken::EndGroup) => {}
                _ => return None,
            }
            name
        }
        Some(TexToken::ControlSeq(_)) => {
            if let Some(TexToken::ControlSeq(name)) = iter.next() {
                name
            } else {
                return None;
            }
        }
        _ => return None,
    };

    utils::skip_spaces_in(iter);

    // Parse optional argument count [n]
    let num_args = if matches!(iter.peek_token(), Some(TexToken::Char('['))) {
        iter.next();
        let num = utils::read_number_in(iter).unwrap_or_default();
        // Consume closing bracket
        utils::skip_until_char(iter, ']');
        num
    } else {
        0
    };

    utils::skip_spaces_in(iter);

    // Parse optional default value [default]
    let default = if matches!(iter.peek_token(), Some(TexToken::Char('['))) {
        iter.next();
        let def_tokens = utils::read_until_char_in(iter, ']');
        Some(def_tokens)
    } else {
        None
    };

    utils::skip_spaces_in(iter);

    // Parse body {body}
    let body = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        utils::read_balanced_group_in(iter)
    } else {
        return None;
    };

    let def = match cmd_name {
        "newcommand" => DefinitionKind::NewCommand {
            name: macro_name,
            num_args,
            default,
            body,
        },
        "renewcommand" => DefinitionKind::RenewCommand {
            name: macro_name,
            num_args,
            default,
            body,
        },
        "providecommand" => DefinitionKind::ProvideCommand {
            name: macro_name,
            num_args,
            default,
            body,
        },
        "DeclareRobustCommand" => DefinitionKind::NewCommand {
            name: macro_name,
            num_args,
            default,
            body,
        },
        _ => unreachable!(),
    };

    Some(def)
}

/// Read xparse-style commands (simplified) from a token stream
fn read_xparse_style<S: TokenSource>(cmd_name: &str, iter: &mut S) -> Option<DefinitionKind> {
    utils::skip_spaces_in(iter);

    // Parse command name
    let macro_name = match iter.peek_token() {
        Some(TexToken::BeginGroup) => {
            iter.next();
            let name = match utils::read_control_seq_name_in(iter) {
                Ok(n) => n,
                Err(_) => return None,
            };
            match iter.next() {
                Some(TexToken::EndGroup) => {}
                _ => return None,
            }
            name
        }
        Some(TexToken::ControlSeq(_)) => {
            if let Some(TexToken::ControlSeq(name)) = iter.next() {
                name
            } else {
                return None;
            }
        }
        _ => return None,
    };

    utils::skip_spaces_in(iter);

    // Parse argument specification {mmm} - count 'm' for mandatory args
    let num_args = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        let spec = utils::read_balanced_group_in(iter);
        count_mandatory_args(&spec)
    } else {
        0
    };

    utils::skip_spaces_in(iter);

    // Parse body
    let body = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        utils::read_balanced_group_in(iter)
    } else {
        return None;
    };

    let def = match cmd_name {
        "NewDocumentCommand" => DefinitionKind::NewCommand {
            name: macro_name,
            num_args,
            default: None,
            body,
        },
        "RenewDocumentCommand" => DefinitionKind::RenewCommand {
            name: macro_name,
            num_args,
            default: None,
            body,
        },
        _ => unreachable!(),
    };

    Some(def)
}

/// Read \def\name#1#2{body} or \edef\name#1#2{body} from a token stream
fn read_def<S: TokenSource>(iter: &mut S, is_edef: bool) -> Option<DefinitionKind> {
    utils::skip_spaces_in(iter);

    // Parse macro name
    let macro_name = match iter.next() {
        Some(TexToken::ControlSeq(name)) => name,
        _ => return None,
    };

    // Parse parameter text until '{'
    // Build a pattern of arguments and literals
    let mut parts: Vec<PatternPart> = Vec::new();
    let mut literal_acc: Vec<TexToken> = Vec::new();

    while let Some(token) = iter.peek_token() {
        match token {
            TexToken::BeginGroup => break,
            TexToken::Param(n) => {
                // Flush accumulated literals
                if !literal_acc.is_empty() {
                    parts.push(PatternPart::Literal(std::mem::take(&mut literal_acc)));
                }
                parts.push(PatternPart::Argument(*n));
                iter.next();
            }
            _ => {
                // Accumulate as literal (including spaces - we keep them EXACTLY as defined)
                // SAFETY: iter.next() is guaranteed to return Some because we're inside
                // `while let Some(_) = iter.peek_token()` - the loop condition ensures the iterator
                // is non-empty when we reach this point.
                literal_acc.push(
                    iter.next()
                        .expect("iterator guaranteed non-empty by loop condition"),
                );
            }
        }
    }

    // Flush trailing literals
    if !literal_acc.is_empty() {
        parts.push(PatternPart::Literal(literal_acc));
    }

    // Determine if we can use the optimized Simple signature
    // Simple requires: only Argument parts, in order #1, #2, ..., #N
    let is_simple_sequence = parts
        .iter()
        .enumerate()
        .all(|(i, p)| matches!(p, PatternPart::Argument(n) if *n as usize == i + 1));

    let signature = if is_simple_sequence {
        MacroSignature::Simple(parts.len() as u8)
    } else {
        MacroSignature::Pattern(parts)
    };

    // Parse body
    let body = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        utils::read_balanced_group_in(iter)
    } else {
        return None;
    };

    let def = if is_edef {
        DefinitionKind::Edef {
            name: macro_name,
            signature,
            body,
        }
    } else {
        DefinitionKind::Def {
            name: macro_name,
            signature,
            body,
        }
    };

    Some(def)
}

/// Read \let\name=\target or \let\name\target from a token stream
fn read_let<S: TokenSource>(iter: &mut S) -> Option<DefinitionKind> {
    utils::skip_spaces_in(iter);

    // Parse the new name
    let new_name = match iter.next() {
        Some(TexToken::ControlSeq(name)) => name,
        _ => return None,
    };

    utils::skip_spaces_in(iter);

    // Optional equals sign
    if matches!(iter.peek_token(), Some(TexToken::Char('='))) {
        iter.next();
        utils::skip_spaces_in(iter);
    }

    // Parse the target
    let target = match iter.next() {
        Some(TexToken::ControlSeq(name)) => name,
        _ => return None,
    };

    Some(DefinitionKind::Let {
        name: new_name,
        target,
    })
}

/// Read \newif\iffoo from a token stream
/// Creates three macros:
/// - \iffoo -> \iffalse (initial state)
/// - \footrue -> \def\iffoo{\iftrue}
/// - \foofalse -> \def\iffoo{\iffalse}
fn read_newif<S: TokenSource>(iter: &mut S) -> Option<DefinitionKind> {
    utils::skip_spaces_in(iter);

    // Parse \iffoo - the name must start with "if"
    let full_name = match iter.next() {
        Some(TexToken::ControlSeq(name)) => name,
        _ => return None,
    };

    // Validate that name starts with "if"
    if !full_name.starts_with("if") {
        return None;
    }

    // Extract base name (e.g., "foo" from "iffoo")
    let base_name = full_name
        .strip_prefix("if")
        .unwrap_or(&full_name)
        .to_string();

    Some(DefinitionKind::NewIf { base_name })
}

/// Read \DeclareMathOperator{\name}{text} from a token stream
fn read_declare_math_operator<S: TokenSource>(iter: &mut S) -> Option<DefinitionKind> {
    utils::skip_spaces_in(iter);

    // Check for star variant
    let is_starred = if matches!(iter.peek_token(), Some(TexToken::Char('*'))) {
        iter.next();
        utils::skip_spaces_in(iter);
        true
    } else {
        false
    };

    // Parse command name: {\cmd} or \cmd
    let macro_name = match iter.peek_token() {
        Some(TexToken::BeginGroup) => {
            iter.next();
            let name = match utils::read_control_seq_name_in(iter) {
                Ok(n) => n,
                Err(_) => return None,
            };
            match iter.next() {
                Some(TexToken::EndGroup) => {}
                _ => return None,
            }
            name
        }
        Some(TexToken::ControlSeq(_)) => {
            if let Some(TexToken::ControlSeq(name)) = iter.next() {
                name
            } else {
                return None;
            }
        }
        _ => return None,
    };

    utils::skip_spaces_in(iter);

    // Parse body {text}
    let body = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        utils::read_balanced_group_in(iter)
    } else {
        return None;
    };

    Some(DefinitionKind::DeclareMathOperator {
        name: macro_name,
        body,
        is_starred,
    })
}

/// Read \newenvironment{name}[n][default]{begin}{end} from a token stream
fn read_newenvironment<S: TokenSource>(cmd_name: &str, iter: &mut S) -> Option<DefinitionKind> {
    utils::skip_spaces_in(iter);

    // Check for star variant (ignored)
    if matches!(iter.peek_token(), Some(TexToken::Char('*'))) {
        iter.next();
        utils::skip_spaces_in(iter);
    }

    // Parse environment name: {envname}
    let env_name = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        let name_tokens = utils::read_balanced_group_in(iter);
        // Extract name from tokens (should be plain text)
        name_tokens
            .as_slice()
            .iter()
            .filter_map(|t| match t {
                TexToken::Char(c) => Some(*c),
                _ => None,
            })
            .collect::<String>()
    } else {
        return None;
    };

    if env_name.is_empty() {
        return None;
    }

    utils::skip_spaces_in(iter);

    // Parse optional argument count [n]
    let num_args = if matches!(iter.peek_token(), Some(TexToken::Char('['))) {
        iter.next();
        let num = utils::read_number_in(iter).unwrap_or_default();
        utils::skip_until_char(iter, ']');
        num
    } else {
        0
    };

    utils::skip_spaces_in(iter);

    // Parse optional default value [default]
    let default = if matches!(iter.peek_token(), Some(TexToken::Char('['))) {
        iter.next();
        let def_tokens = utils::read_until_char_in(iter, ']');
        Some(def_tokens)
    } else {
        None
    };

    utils::skip_spaces_in(iter);

    // Parse begin body {begin}
    let begin_body = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        utils::read_balanced_group_in(iter)
    } else {
        return None;
    };

    utils::skip_spaces_in(iter);

    // Parse end body {end}
    let end_body = if matches!(iter.peek_token(), Some(TexToken::BeginGroup)) {
        iter.next();
        utils::read_balanced_group_in(iter)
    } else {
        return None;
    };

    let def = match cmd_name {
        "newenvironment" => DefinitionKind::NewEnvironment {
            name: env_name,
            num_args,
            default,
            begin_body,
            end_body,
        },
        "renewenvironment" => DefinitionKind::RenewEnvironment {
            name: env_name,
            num_args,
            default,
            begin_body,
            end_body,
        },
        _ => unreachable!(),
    };

    Some(def)
}

// Helper functions removed - using utils module

fn count_mandatory_args(spec: &TokenList) -> u8 {
    spec.as_slice()
        .iter()
        .filter(|t| matches!(t, TexToken::Char('m') | TexToken::Char('M')))
        .count() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::latex2typst::engine::lexer::tokenize;

    #[test]
    fn test_parse_newcommand_simple() {
        let tokens = tokenize("{\\foo}{bar}");
        let result = parse_newcommand_style("newcommand", tokens);
        assert!(result.is_ok());

        let (def, _rest) = result.unwrap();
        match def {
            DefinitionKind::NewCommand {
                name,
                num_args,
                body,
                ..
            } => {
                assert_eq!(name, "foo");
                assert_eq!(num_args, 0);
                assert_eq!(format!("{}", body), "bar");
            }
            _ => panic!("Wrong definition kind"),
        }
    }

    #[test]
    fn test_parse_newcommand_with_args() {
        let tokens = tokenize("{\\pair}[2]{\\langle #1, #2\\rangle}");
        let result = parse_newcommand_style("newcommand", tokens);
        assert!(result.is_ok());

        let (def, _rest) = result.unwrap();
        match def {
            DefinitionKind::NewCommand {
                name,
                num_args,
                body,
                ..
            } => {
                assert_eq!(name, "pair");
                assert_eq!(num_args, 2);
                assert!(format!("{}", body).contains("#1"));
                assert!(format!("{}", body).contains("#2"));
            }
            _ => panic!("Wrong definition kind"),
        }
    }

    #[test]
    fn test_parse_def() {
        let tokens = tokenize("\\foo#1#2{#1 and #2}");
        let result = parse_def(tokens, false);
        assert!(result.is_ok());

        let (def, _rest) = result.unwrap();
        match def {
            DefinitionKind::Def {
                name,
                signature,
                body,
            } => {
                assert_eq!(name, "foo");
                assert_eq!(signature.num_args(), 2);
                assert!(format!("{}", body).contains("#1"));
            }
            _ => panic!("Wrong definition kind"),
        }
    }

    #[test]
    fn test_parse_edef() {
        let tokens = tokenize("\\bar{expanded content}");
        let result = parse_def(tokens, true);
        assert!(result.is_ok());

        let (def, _rest) = result.unwrap();
        match def {
            DefinitionKind::Edef {
                name,
                signature,
                body,
            } => {
                assert_eq!(name, "bar");
                assert_eq!(signature.num_args(), 0);
                assert!(format!("{}", body).contains("expanded"));
            }
            _ => panic!("Wrong definition kind: {:?}", def),
        }
    }

    #[test]
    fn test_parse_let() {
        let tokens = tokenize("\\foo=\\bar");
        let result = parse_let(tokens);
        assert!(result.is_ok());

        let (def, _rest) = result.unwrap();
        match def {
            DefinitionKind::Let { name, target } => {
                assert_eq!(name, "foo");
                assert_eq!(target, "bar");
            }
            _ => panic!("Wrong definition kind"),
        }
    }
}
