use crate::error::{Diagnostic, Span};

use super::token::{keyword_kind, Token, TokenKind};

pub fn lex(source: &str, filename: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source, filename).tokenize()
}

struct Lexer<'a> {
    chars: Vec<char>,
    filename: &'a str,
    index: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, filename: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            filename,
            index: 0,
            line: 1,
            column: 1,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        loop {
            self.skip_ws_and_comments();
            let line = self.line;
            let column = self.column;
            let ch = self.peek(0);
            if ch == '\0' {
                tokens.push(Token::new(TokenKind::Eof, "", Span::new(line, column)));
                return Ok(tokens);
            }
            if ch.is_ascii_digit() {
                tokens.push(self.read_number());
                continue;
            }
            if ch.is_ascii_alphabetic() || ch == '_' {
                if ch == 'f' && self.peek(1) == '"' {
                    tokens.push(self.read_string(true)?);
                } else {
                    tokens.push(self.read_ident());
                }
                continue;
            }
            if ch == '"' {
                tokens.push(self.read_string(false)?);
                continue;
            }

            let paired = match [ch, self.peek(1)] {
                ['=', '>'] => Some((TokenKind::FatArrow, "=>")),
                ['-', '>'] => Some((TokenKind::Arrow, "->")),
                ['?', '.'] => Some((TokenKind::QDot, "?.")),
                ['?', ':'] => Some((TokenKind::Elvis, "?:")),
                [':', ':'] => Some((TokenKind::ColonColon, "::")),
                ['=', '='] => Some((TokenKind::EqEq, "==")),
                ['!', '='] => Some((TokenKind::Ne, "!=")),
                ['<', '='] => Some((TokenKind::Le, "<=")),
                ['>', '='] => Some((TokenKind::Ge, ">=")),
                _ => None,
            };
            if let Some((kind, text)) = paired {
                self.advance();
                self.advance();
                tokens.push(Token::new(kind, text, Span::new(line, column)));
                continue;
            }

            let single = match ch {
                '(' => Some(TokenKind::LParen),
                ')' => Some(TokenKind::RParen),
                '{' => Some(TokenKind::LBrace),
                '}' => Some(TokenKind::RBrace),
                '[' => Some(TokenKind::LBracket),
                ']' => Some(TokenKind::RBracket),
                ',' => Some(TokenKind::Comma),
                ';' => Some(TokenKind::Semi),
                ':' => Some(TokenKind::Colon),
                '.' => Some(TokenKind::Dot),
                '?' => Some(TokenKind::Question),
                '@' => Some(TokenKind::At),
                '=' => Some(TokenKind::Eq),
                '+' => Some(TokenKind::Plus),
                '-' => Some(TokenKind::Minus),
                '*' => Some(TokenKind::Star),
                '/' => Some(TokenKind::Slash),
                '%' => Some(TokenKind::Percent),
                '<' => Some(TokenKind::Lt),
                '>' => Some(TokenKind::Gt),
                '!' => Some(TokenKind::Bang),
                _ => None,
            };
            if let Some(kind) = single {
                self.advance();
                tokens.push(Token::new(kind, ch.to_string(), Span::new(line, column)));
                continue;
            }

            return Err(self.error(format!("unexpected character `{ch}`"), line, column));
        }
    }

    fn peek(&self, offset: usize) -> char {
        self.chars.get(self.index + offset).copied().unwrap_or('\0')
    }

    fn advance(&mut self) -> char {
        let ch = self.peek(0);
        if ch == '\0' {
            return ch;
        }
        self.index += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            match self.peek(0) {
                ' ' | '\t' | '\r' | '\n' => {
                    self.advance();
                }
                '/' if self.peek(1) == '/' => {
                    while !matches!(self.peek(0), '\n' | '\0') {
                        self.advance();
                    }
                }
                _ => return,
            }
        }
    }

    fn read_number(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        let mut text = String::new();
        while self.peek(0).is_ascii_digit() {
            text.push(self.advance());
        }
        if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
            text.push(self.advance());
            while self.peek(0).is_ascii_digit() {
                text.push(self.advance());
            }
            return Token::new(TokenKind::Float, text, Span::new(line, column));
        }
        Token::new(TokenKind::Int, text, Span::new(line, column))
    }

    fn read_ident(&mut self) -> Token {
        let line = self.line;
        let column = self.column;
        let mut text = String::new();
        while self.peek(0).is_ascii_alphanumeric() || self.peek(0) == '_' {
            text.push(self.advance());
        }
        let kind = keyword_kind(&text);
        Token::new(kind, text, Span::new(line, column))
    }

    fn read_string(&mut self, formatted: bool) -> Result<Token, Diagnostic> {
        let line = self.line;
        let column = self.column;
        if formatted {
            self.advance();
        }
        self.advance();
        let mut value = String::new();
        let mut brace_depth: usize = 0;
        loop {
            match self.peek(0) {
                '\0' => return Err(self.error("unterminated string literal", line, column)),
                '"' if brace_depth == 0 => {
                    self.advance();
                    break;
                }
                '{' if formatted => {
                    brace_depth += 1;
                    value.push('{');
                    self.advance();
                }
                '}' if formatted && brace_depth > 0 => {
                    brace_depth -= 1;
                    value.push('}');
                    self.advance();
                }
                '\\' => {
                    self.advance();
                    let escaped = match self.advance() {
                        'n' => '\n',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    };
                    value.push(escaped);
                }
                other => {
                    value.push(other);
                    self.advance();
                }
            }
        }
        let kind = if formatted { TokenKind::FString } else { TokenKind::String };
        Ok(Token::new(kind, value, Span::new(line, column)))
    }

    fn error(&self, message: impl Into<String>, line: usize, column: usize) -> Diagnostic {
        Diagnostic::error(&message.into(), self.filename, Span::new(line, column), None)
    }
}

