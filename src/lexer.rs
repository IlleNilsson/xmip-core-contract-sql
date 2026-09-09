//! SQL tokens with their positions.
//!
//! The lexer knows exactly as much SQL as well-formedness needs: single-quoted
//! string literals with `''` as the escape, double-quoted identifiers with
//! `""`, `--` line comments, `/* */` block comments, bare words, numbers and
//! single punctuation characters. Comments are read and dropped; everything
//! else becomes a [`Token`] carrying the line and column it began on. Reading
//! stops at the first quote or comment that never closes, and what was read up
//! to there is still returned so the caller can say which statement it was in.

/// What a token is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// A bare word: a keyword or an unquoted identifier.
    Word,
    /// A double-quoted identifier, quotes stripped and `""` unescaped.
    Identifier,
    /// A single-quoted string literal, quotes stripped and `''` unescaped.
    Text,
    /// A numeric literal.
    Number,
    /// Any other single character: `;`, `(`, `)`, `,`, an operator.
    Punct,
}

/// One token and where it began, line and column both counted from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: usize,
    pub column: usize,
}

impl Token {
    /// Whether this is the punctuation character `c`.
    #[must_use]
    pub fn is_punct(&self, c: char) -> bool {
        self.kind == TokenKind::Punct && self.text.starts_with(c)
    }

    /// The bare word in upper case, or `None` for any other token.
    #[must_use]
    pub fn word(&self) -> Option<String> {
        (self.kind == TokenKind::Word).then(|| self.text.to_ascii_uppercase())
    }
}

/// Where reading stopped: a quote or comment that never closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    /// `unterminated-string`, `unterminated-identifier` or `unterminated-comment`.
    pub code: &'static str,
    pub message: String,
    /// The line the unterminated construct began on.
    pub line: usize,
}

/// The tokens read, and the error that stopped reading if one did.
#[derive(Debug, Default)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub error: Option<LexError>,
}

/// Reads `text` into tokens.
#[must_use]
pub fn tokenize(text: &str) -> Lexed {
    let mut reader = Reader::new(text);
    let mut lexed = Lexed::default();
    while let Some(c) = reader.peek() {
        if c.is_whitespace() {
            reader.bump();
            continue;
        }
        let (line, column) = (reader.line, reader.column);
        if c == '-' && reader.peek_next() == Some('-') {
            reader.skip_line();
            continue;
        }
        if c == '/' && reader.peek_next() == Some('*') {
            if !reader.skip_block_comment() {
                lexed.error = Some(unterminated("comment", "unterminated-comment", line));
                return lexed;
            }
            continue;
        }
        let (kind, text) = match c {
            '\'' | '"' => {
                let (kind, what, code) = if c == '\'' {
                    (TokenKind::Text, "string", "unterminated-string")
                } else {
                    (
                        TokenKind::Identifier,
                        "identifier",
                        "unterminated-identifier",
                    )
                };
                let Some(text) = reader.quoted(c) else {
                    lexed.error = Some(unterminated(what, code, line));
                    return lexed;
                };
                (kind, text)
            }
            _ if c.is_alphabetic() || c == '_' => (TokenKind::Word, reader.word()),
            _ if c.is_ascii_digit() => (TokenKind::Number, reader.number()),
            _ => {
                reader.bump();
                (TokenKind::Punct, c.to_string())
            }
        };
        lexed.tokens.push(Token {
            kind,
            text,
            line,
            column,
        });
    }
    lexed
}

fn unterminated(what: &str, code: &'static str, line: usize) -> LexError {
    LexError {
        code,
        message: format!("the {what} opened at line {line} never closes"),
        line,
    }
}

/// A cursor over the text's characters that keeps the line and column.
struct Reader {
    chars: Vec<char>,
    at: usize,
    line: usize,
    column: usize,
}

impl Reader {
    fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            at: 0,
            line: 1,
            column: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.at + 1).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    /// Reads through the end of the line, or the end of the text.
    fn skip_line(&mut self) {
        while let Some(c) = self.bump() {
            if c == '\n' {
                return;
            }
        }
    }

    /// Reads a `/* */` comment; `false` when the text ends inside it.
    fn skip_block_comment(&mut self) -> bool {
        self.bump();
        self.bump();
        while let Some(c) = self.bump() {
            if c == '*' && self.peek() == Some('/') {
                self.bump();
                return true;
            }
        }
        false
    }

    /// Reads a quoted run, a doubled quote standing for one; `None` when the
    /// text ends inside it.
    fn quoted(&mut self, quote: char) -> Option<String> {
        self.bump();
        let mut text = String::new();
        loop {
            let c = self.bump()?;
            if c == quote {
                if self.peek() == Some(quote) {
                    self.bump();
                    text.push(quote);
                } else {
                    return Some(text);
                }
            } else {
                text.push(c);
            }
        }
    }

    fn word(&mut self) -> String {
        self.take_while(|c| c.is_alphanumeric() || c == '_' || c == '$')
    }

    fn number(&mut self) -> String {
        self.take_while(|c| c.is_ascii_alphanumeric() || c == '.')
    }

    fn take_while(&mut self, keep: impl Fn(char) -> bool) -> String {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if !keep(c) {
                break;
            }
            self.bump();
            text.push(c);
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(TokenKind, String)> {
        let lexed = tokenize(text);
        assert!(lexed.error.is_none(), "{:?}", lexed.error);
        lexed.tokens.into_iter().map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn words_numbers_and_punctuation_read_with_positions() {
        let lexed = tokenize("SELECT a,\n  b2 FROM t WHERE x >= 1.5;");
        assert!(lexed.error.is_none());
        let t = &lexed.tokens;
        assert_eq!(t[0].word().as_deref(), Some("SELECT"));
        assert_eq!((t[1].line, t[1].column), (1, 8));
        assert!(t[2].is_punct(','));
        assert_eq!((t[3].line, t[3].column, t[3].text.as_str()), (2, 3, "b2"));
        assert_eq!(t[8].kind, TokenKind::Punct);
        assert_eq!(t[10].text, "1.5");
        assert!(t[11].is_punct(';'));
        assert_eq!(t.len(), 12);
    }

    #[test]
    fn strings_and_identifiers_unescape_and_hide_separators() {
        let read = kinds(r#"SELECT 'it''s; -- not' AS "a ""b""" /* ; */ -- ;"#);
        assert_eq!(read[1], (TokenKind::Text, "it's; -- not".to_string()));
        assert_eq!(read[3], (TokenKind::Identifier, "a \"b\"".to_string()));
        assert_eq!(read.len(), 4);
    }

    #[test]
    fn an_unterminated_construct_names_its_line_and_keeps_what_was_read() {
        let lexed = tokenize("SELECT 1;\n/* open\nstill open");
        let error = lexed.error.expect("error");
        assert_eq!(error.code, "unterminated-comment");
        assert_eq!(error.line, 2);
        assert_eq!(lexed.tokens.len(), 3);

        let error = tokenize("SELECT 'a''b").error.expect("error");
        assert_eq!(error.code, "unterminated-string");
        assert_eq!(error.message, "the string opened at line 1 never closes");
        let error = tokenize("SELECT \"a").error.expect("error");
        assert_eq!(error.code, "unterminated-identifier");
        assert!(tokenize("SELECT 1 -- trailing").error.is_none());
    }
}
