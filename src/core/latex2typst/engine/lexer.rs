//! TeX Lexer/Tokenizer
//!
//! Converts a LaTeX source string into a stream of TeX tokens.
//! This follows standard TeX tokenization rules including:
//! - Control sequence recognition
//! - Comment handling
//! - Space normalization after control sequences
//! - Parameter token parsing

use super::input::BOUNDARY;
use super::token::{TexToken, TokenList};

/// The TeX Lexer that converts source text to tokens
pub struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    /// Track if we just emitted a control sequence (for space swallowing)
    after_cs: bool,
    /// Mark where swallowed spaces separated a control word from an `@`
    /// (see [`tokenize_marked`]).
    mark_boundaries: bool,
    /// Phantom data to hold lifetime
    _marker: std::marker::PhantomData<&'a str>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given input
    pub fn new(input: &'a str) -> Self {
        Lexer {
            chars: input.char_indices().peekable(),
            after_cs: false,
            mark_boundaries: false,
            _marker: std::marker::PhantomData,
        }
    }

    /// Peek at the next character without consuming it
    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    /// Consume and return the next character
    fn next_char(&mut self) -> Option<char> {
        self.chars.next().map(|(_, c)| c)
    }

    /// Skip whitespace, returns true if any was skipped
    fn skip_whitespace(&mut self) -> bool {
        let mut skipped = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_whitespace() {
                self.next_char();
                skipped = true;
            } else {
                break;
            }
        }
        skipped
    }

    /// Read a control sequence name (letters only, or single non-letter)
    fn read_control_seq(&mut self) -> String {
        let mut name = String::new();

        // Check first character
        if let Some(c) = self.peek_char() {
            if c.is_ascii_alphabetic() {
                // Multi-letter control sequence
                while let Some(c) = self.peek_char() {
                    if c.is_ascii_alphabetic() {
                        name.push(c);
                        self.next_char();
                    } else {
                        break;
                    }
                }
                // TeX swallows spaces after alphabetic control sequences
                self.after_cs = true;
            } else {
                // Single non-letter control sequence like \% \{ \}
                name.push(c);
                self.next_char();
                self.after_cs = false;
            }
        }

        name
    }

    /// Read a comment (everything until end of line)
    fn read_comment(&mut self) -> String {
        let mut comment = String::new();
        while let Some(c) = self.peek_char() {
            if c == '\n' || c == '\r' {
                break;
            }
            comment.push(c);
            self.next_char();
        }
        // Consume the newline
        if let Some(c) = self.peek_char() {
            if c == '\r' {
                self.next_char();
            }
        }
        if let Some(c) = self.peek_char() {
            if c == '\n' {
                self.next_char();
            }
        }
        comment
    }

    /// Read the next token
    fn next_token(&mut self) -> Option<TexToken> {
        // Handle space swallowing after control sequences
        if self.after_cs {
            let skipped = self.skip_whitespace();
            self.after_cs = false;
            if skipped && self.mark_boundaries && self.peek_char() == Some('@') {
                return Some(BOUNDARY);
            }
        }

        let c = self.next_char()?;

        match c {
            // Escape character - start of control sequence
            '\\' => {
                let name = self.read_control_seq();
                if name.is_empty() {
                    // Lone backslash at end of input
                    Some(TexToken::Char('\\'))
                } else {
                    Some(TexToken::ControlSeq(name))
                }
            }

            // Begin group
            '{' => Some(TexToken::BeginGroup),

            // End group
            '}' => Some(TexToken::EndGroup),

            // Parameter token
            '#' => {
                if let Some(next) = self.peek_char() {
                    if next.is_ascii_digit() && next != '0' {
                        self.next_char();
                        Some(TexToken::Param(next.to_digit(10).unwrap() as u8))
                    } else if next == '#' {
                        // ## - check if followed by digit for DeferredParam
                        self.next_char();
                        if let Some(digit) = self.peek_char() {
                            if digit.is_ascii_digit() && digit != '0' {
                                self.next_char();
                                Some(TexToken::DeferredParam(digit.to_digit(10).unwrap() as u8))
                            } else {
                                // ## not followed by digit produces a single #
                                Some(TexToken::Char('#'))
                            }
                        } else {
                            // ## at end of input produces a single #
                            Some(TexToken::Char('#'))
                        }
                    } else {
                        // Invalid parameter, treat as char
                        Some(TexToken::Char('#'))
                    }
                } else {
                    Some(TexToken::Char('#'))
                }
            }

            // Comment
            '%' => {
                let comment = self.read_comment();
                Some(TexToken::Comment(comment))
            }

            // Math shift
            '$' => Some(TexToken::MathShift),

            // Alignment tab
            '&' => Some(TexToken::AlignTab),

            // Superscript
            '^' => Some(TexToken::Superscript),

            // Subscript
            '_' => Some(TexToken::Subscript),

            // Active character (tilde for non-breaking space)
            '~' => Some(TexToken::ActiveChar('~')),

            // Whitespace - normalize to single space
            ' ' | '\t' => {
                // Skip any additional whitespace
                while let Some(next) = self.peek_char() {
                    if next == ' ' || next == '\t' {
                        self.next_char();
                    } else {
                        break;
                    }
                }
                Some(TexToken::Space)
            }

            // Newlines - can be significant in TeX
            '\n' | '\r' => {
                // Check for paragraph break (blank line)
                let mut blank_line = false;
                while let Some(next) = self.peek_char() {
                    if next == ' ' || next == '\t' {
                        self.next_char();
                    } else if next == '\n' || next == '\r' {
                        self.next_char();
                        blank_line = true;
                    } else {
                        break;
                    }
                }
                if blank_line {
                    // Paragraph break becomes \par
                    Some(TexToken::ControlSeq("par".into()))
                } else {
                    // Single newline becomes space
                    Some(TexToken::Space)
                }
            }

            // Regular character
            _ => Some(TexToken::Char(c)),
        }
    }

    /// Tokenize the entire input
    pub fn tokenize(self) -> TokenList {
        let tokens: Vec<TexToken> = self.collect();
        TokenList::from_vec(tokens)
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = TexToken;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

/// Convenience function to tokenize a string
pub fn tokenize(input: &str) -> TokenList {
    Lexer::new(input).tokenize()
}

/// Tokenize text for the expansion engine: as [`tokenize`], with a
/// [`BOUNDARY`] wherever spaces after a control word come before an `@`, so
/// that `\foo @bar` is never read as `\foo@bar` under `\makeatletter`. The
/// engine drops the marks.
pub(crate) fn tokenize_marked(input: &str) -> TokenList {
    let mut lexer = Lexer::new(input);
    lexer.mark_boundaries = true;
    lexer.tokenize()
}

/// Convert a token list back to a string (detokenize)
pub fn detokenize(tokens: &TokenList) -> String {
    let mut result = String::new();
    let slice = tokens.as_slice();

    for (i, token) in slice.iter().enumerate() {
        match token {
            TexToken::ControlSeq(name) => {
                result.push('\\');
                result.push_str(name);

                // Add space after alphabetic control sequences if next token
                // is a letter or another control sequence
                if name.chars().all(|c| c.is_ascii_alphabetic()) {
                    if let Some(next) = slice.get(i + 1) {
                        match next {
                            TexToken::Char(c) if c.is_ascii_alphabetic() => {
                                result.push(' ');
                            }
                            TexToken::ControlSeq(_) => {
                                result.push(' ');
                            }
                            _ => {}
                        }
                    }
                }
            }
            TexToken::BeginGroup => {
                result.push('{');
            }
            TexToken::EndGroup => {
                result.push('}');
            }
            TexToken::Param(n) => {
                result.push('#');
                result.push(char::from_digit(*n as u32, 10).unwrap());
            }
            TexToken::DeferredParam(n) => {
                result.push('#');
                result.push('#');
                result.push(char::from_digit(*n as u32, 10).unwrap());
            }
            TexToken::Char(c) => {
                result.push(*c);
            }
            TexToken::Space => {
                result.push(' ');
            }
            TexToken::Comment(text) => {
                result.push('%');
                result.push_str(text);
                result.push('\n');
            }
            TexToken::MathShift => {
                result.push('$');
            }
            TexToken::AlignTab => {
                result.push('&');
            }
            TexToken::Superscript => {
                result.push('^');
            }
            TexToken::Subscript => {
                result.push('_');
            }
            TexToken::ActiveChar(c) => {
                result.push(*c);
            }
            TexToken::EndOfInput => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tokenize() {
        let tokens = tokenize("hello");
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens.as_slice()[0], TexToken::Char('h'));
    }

    #[test]
    fn test_control_sequence() {
        let tokens = tokenize("\\frac{a}{b}");
        assert_eq!(tokens.as_slice()[0], TexToken::ControlSeq("frac".into()));
        assert_eq!(tokens.as_slice()[1], TexToken::BeginGroup);
        assert_eq!(tokens.as_slice()[2], TexToken::Char('a'));
        assert_eq!(tokens.as_slice()[3], TexToken::EndGroup);
    }

    #[test]
    fn test_space_swallowing() {
        // Space after \frac should be swallowed
        let tokens = tokenize("\\frac  {a}");
        assert_eq!(tokens.as_slice()[0], TexToken::ControlSeq("frac".into()));
        assert_eq!(tokens.as_slice()[1], TexToken::BeginGroup);
    }

    #[test]
    fn test_parameter_tokens() {
        let tokens = tokenize("#1 #2");
        assert_eq!(tokens.as_slice()[0], TexToken::Param(1));
        assert_eq!(tokens.as_slice()[1], TexToken::Space);
        assert_eq!(tokens.as_slice()[2], TexToken::Param(2));
    }

    #[test]
    fn test_comment() {
        let tokens = tokenize("a%comment\nb");
        assert_eq!(tokens.as_slice()[0], TexToken::Char('a'));
        assert_eq!(tokens.as_slice()[1], TexToken::Comment("comment".into()));
        assert_eq!(tokens.as_slice()[2], TexToken::Char('b'));
    }

    #[test]
    fn test_escaped_chars() {
        let tokens = tokenize("\\% \\{");
        assert_eq!(tokens.as_slice()[0], TexToken::ControlSeq("%".into()));
        assert_eq!(tokens.as_slice()[1], TexToken::Space);
        assert_eq!(tokens.as_slice()[2], TexToken::ControlSeq("{".into()));
    }

    #[test]
    fn test_double_hash() {
        let tokens = tokenize("##");
        assert_eq!(tokens.as_slice()[0], TexToken::Char('#'));
    }

    #[test]
    fn test_deferred_param() {
        let tokens = tokenize("##1 ##2");
        assert_eq!(tokens.as_slice()[0], TexToken::DeferredParam(1));
        assert_eq!(tokens.as_slice()[1], TexToken::Space);
        assert_eq!(tokens.as_slice()[2], TexToken::DeferredParam(2));
    }

    #[test]
    fn test_roundtrip() {
        let input = "\\frac{a^2}{b_1}";
        let tokens = tokenize(input);
        let output = detokenize(&tokens);
        assert_eq!(output, input);
    }

    #[test]
    fn test_newcommand_body() {
        let tokens = tokenize("\\langle #1, #2\\rangle");
        assert_eq!(tokens.as_slice()[0], TexToken::ControlSeq("langle".into()));
        assert_eq!(tokens.as_slice()[1], TexToken::Param(1));
        assert_eq!(tokens.as_slice()[2], TexToken::Char(','));
        assert_eq!(tokens.as_slice()[3], TexToken::Space);
        assert_eq!(tokens.as_slice()[4], TexToken::Param(2));
        assert_eq!(tokens.as_slice()[5], TexToken::ControlSeq("rangle".into()));
    }
}
