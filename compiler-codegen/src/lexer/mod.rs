//! Лексер Nova.
//!
//! Преобразует исходный текст в поток токенов. Без скобочных интерполяций
//! `${...}` в строках (можно добавить позже — для bootstrap'а компилятора
//! строковая интерполяция не критична: компилятор может склеивать строки
//! через `+` или `format!`-эквивалент).
//!
//! Соответствует:
//! - [D27](../../../spec/decisions/03-syntax.md#d27): `[]T`/`[N]T`-массивы
//! - [D44](../../../spec/decisions/03-syntax.md#d44): числовые литералы
//! - [D49](../../../spec/decisions/03-syntax.md#d49): newlines как
//!   разделители statement'ов внутри `{}`

mod token;

pub use token::{Token, TokenKind};

use crate::diag::{Diagnostic, Span};

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    /// Лексирует весь вход, возвращает Vec<Token>. EOF добавляется в конец.
    pub fn lex(&mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            if self.pos >= self.bytes.len() {
                let span = Span::new(self.pos, self.pos);
                out.push(Token::new(TokenKind::Eof, span));
                return Ok(out);
            }
            let tok = self.next_token()?;
            out.push(tok);
        }
    }

    fn next_token(&mut self) -> Result<Token, Diagnostic> {
        let start = self.pos;
        let b = self.bytes[self.pos];
        let kind = match b {
            b'\n' => {
                self.pos += 1;
                TokenKind::Newline
            }
            b if b.is_ascii_digit() => return self.lex_number(start),
            b if is_ident_start(b) => return self.lex_ident_or_keyword(start),
            b'"' => return self.lex_string(start),
            b'`' => return self.lex_backtick(start),
            b'(' => self.single(TokenKind::LParen),
            b')' => self.single(TokenKind::RParen),
            b'[' => self.single(TokenKind::LBracket),
            b']' => self.single(TokenKind::RBracket),
            b'{' => self.single(TokenKind::LBrace),
            b'}' => self.single(TokenKind::RBrace),
            b',' => self.single(TokenKind::Comma),
            b';' => self.single(TokenKind::Semicolon),
            b':' => self.single(TokenKind::Colon),
            b'@' => self.single(TokenKind::At),
            b'?' => match self.peek_at(1) {
                Some(b'?') => {
                    self.pos += 2;
                    TokenKind::Question2
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Question
                }
            },
            b'.' => match (self.peek_at(1), self.peek_at(2)) {
                (Some(b'.'), Some(b'.')) => {
                    self.pos += 3;
                    TokenKind::DotDotDot
                }
                (Some(b'.'), Some(b'=')) => {
                    self.pos += 3;
                    TokenKind::DotDotEq
                }
                (Some(b'.'), _) => {
                    self.pos += 2;
                    TokenKind::DotDot
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Dot
                }
            },
            b'-' => match self.peek_at(1) {
                Some(b'>') => {
                    self.pos += 2;
                    TokenKind::Arrow
                }
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::MinusEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Minus
                }
            },
            b'=' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::EqEq
                }
                Some(b'>') => {
                    self.pos += 2;
                    TokenKind::FatArrow
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Eq
                }
            },
            b'+' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::PlusEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Plus
                }
            },
            b'*' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::StarEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Star
                }
            },
            b'/' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::SlashEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Slash
                }
            },
            b'%' => {
                self.pos += 1;
                TokenKind::Percent
            }
            b'!' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::BangEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Bang
                }
            },
            b'<' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::Le
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Lt
                }
            },
            b'>' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::Ge
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Gt
                }
            },
            b'&' => match self.peek_at(1) {
                Some(b'&') => {
                    self.pos += 2;
                    TokenKind::AmpAmp
                }
                _ => {
                    return Err(Diagnostic::new(
                        "single `&` is not used in Nova; did you mean `&&`?",
                        Span::new(start, start + 1),
                    ));
                }
            },
            b'|' => match self.peek_at(1) {
                Some(b'|') => {
                    self.pos += 2;
                    TokenKind::PipePipe
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Pipe
                }
            },
            other => {
                return Err(Diagnostic::new(
                    format!("unexpected byte: {:?}", other as char),
                    Span::new(start, start + 1),
                ));
            }
        };
        let span = Span::new(start, self.pos);
        Ok(Token::new(kind, span))
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.pos += 1;
        kind
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    /// Пропускает пробелы (но НЕ newline — он значимый, D49) и комментарии.
    fn skip_trivia(&mut self) {
        loop {
            let Some(&b) = self.bytes.get(self.pos) else {
                return;
            };
            match b {
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'/' if self.peek_at(1) == Some(b'/') => {
                    // line comment
                    while let Some(&b) = self.bytes.get(self.pos) {
                        if b == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => return,
            }
        }
    }

    fn lex_number(&mut self, start: usize) -> Result<Token, Diagnostic> {
        // Поддерживаем 0x.., 0b.., 0o.., десятичные с _, числа с плавающей
        // точкой (с точкой и/или экспонентой). D44.
        let mut is_float = false;

        if self.bytes[self.pos] == b'0' && self.pos + 1 < self.bytes.len() {
            match self.bytes[self.pos + 1] {
                b'x' | b'X' => return self.lex_radix_int(start, 16),
                b'b' | b'B' => return self.lex_radix_int(start, 2),
                b'o' | b'O' => return self.lex_radix_int(start, 8),
                _ => {}
            }
        }

        while let Some(&b) = self.bytes.get(self.pos) {
            if b.is_ascii_digit() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        // Дробная часть (только если за точкой идёт цифра — иначе это `..`,
        // `.field` или member-access).
        if self.peek_at(0) == Some(b'.')
            && self.peek_at(1).map(|b| b.is_ascii_digit()).unwrap_or(false)
        {
            is_float = true;
            self.pos += 1; // .
            while let Some(&b) = self.bytes.get(self.pos) {
                if b.is_ascii_digit() || b == b'_' {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }
        // Экспонента
        if matches!(self.peek_at(0), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek_at(0), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while let Some(&b) = self.bytes.get(self.pos) {
                if b.is_ascii_digit() || b == b'_' {
                    self.pos += 1;
                } else {
                    break;
                }
            }
        }

        let text = &self.src[start..self.pos];
        let span = Span::new(start, self.pos);
        if is_float {
            let cleaned: String = text.chars().filter(|c| *c != '_').collect();
            let v: f64 = cleaned
                .parse()
                .map_err(|e| Diagnostic::new(format!("invalid float: {e}"), span))?;
            Ok(Token::new(TokenKind::Float(v), span))
        } else {
            let cleaned: String = text.chars().filter(|c| *c != '_').collect();
            let v: i64 = cleaned
                .parse()
                .map_err(|e| Diagnostic::new(format!("invalid int: {e}"), span))?;
            Ok(Token::new(TokenKind::Int(v), span))
        }
    }

    fn lex_radix_int(&mut self, start: usize, radix: u32) -> Result<Token, Diagnostic> {
        self.pos += 2; // 0x / 0b / 0o
        let digits_start = self.pos;
        while let Some(&b) = self.bytes.get(self.pos) {
            if (b as char).is_digit(radix) || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let span = Span::new(start, self.pos);
        if self.pos == digits_start {
            return Err(Diagnostic::new(
                format!("expected digits after radix prefix (base {radix})"),
                span,
            ));
        }
        let text = &self.src[digits_start..self.pos];
        let cleaned: String = text.chars().filter(|c| *c != '_').collect();
        let v = i64::from_str_radix(&cleaned, radix)
            .map_err(|e| Diagnostic::new(format!("invalid int: {e}"), span))?;
        Ok(Token::new(TokenKind::Int(v), span))
    }

    fn lex_ident_or_keyword(&mut self, start: usize) -> Result<Token, Diagnostic> {
        while let Some(&b) = self.bytes.get(self.pos) {
            if is_ident_continue(b) {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.src[start..self.pos];
        let span = Span::new(start, self.pos);
        let kind = match text {
            "module" => TokenKind::KwModule,
            "import" => TokenKind::KwImport,
            "use" => TokenKind::KwUse,
            "export" => TokenKind::KwExport,
            "fn" => TokenKind::KwFn,
            "type" => TokenKind::KwType,
            "effect" => TokenKind::KwEffect,
            "handler" => TokenKind::KwHandler,
            "alias" => TokenKind::KwAlias,
            "let" => TokenKind::KwLet,
            "const" => TokenKind::KwConst,
            "mut" => TokenKind::KwMut,
            "readonly" => TokenKind::KwReadonly,
            "if" => TokenKind::KwIf,
            "else" => TokenKind::KwElse,
            "match" => TokenKind::KwMatch,
            "for" => TokenKind::KwFor,
            "while" => TokenKind::KwWhile,
            "loop" => TokenKind::KwLoop,
            "in" => TokenKind::KwIn,
            "return" => TokenKind::KwReturn,
            "break" => TokenKind::KwBreak,
            "continue" => TokenKind::KwContinue,
            "test" => TokenKind::KwTest,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "with" => TokenKind::KwWith,
            "throw" => TokenKind::KwThrow,
            "as" => TokenKind::KwAs,
            "is" => TokenKind::KwIs,
            "spawn" => TokenKind::KwSpawn,
            "supervised" => TokenKind::KwSupervised,
            "parallel" => TokenKind::KwParallel,
            "detach" => TokenKind::KwDetach,
            "cancel_scope" => TokenKind::KwCancelScope,
            "protocol" => TokenKind::KwProtocol,
            "interrupt" => TokenKind::KwInterrupt,
            "forbid" => TokenKind::KwForbid,
            "realtime" => TokenKind::KwRealtime,
            "and" => TokenKind::KwAnd,
            "or" => TokenKind::KwOr,
            "not" => TokenKind::KwNot,
            _ => TokenKind::Ident(text.to_string()),
        };
        Ok(Token::new(kind, span))
    }

    fn lex_string(&mut self, start: usize) -> Result<Token, Diagnostic> {
        // "..." — обычная строка. Без интерполяции в bootstrap'е.
        // Поддерживает \n, \t, \r, \\, \", \0.
        self.pos += 1; // "
        let mut s = String::new();
        loop {
            let Some(&b) = self.bytes.get(self.pos) else {
                return Err(Diagnostic::new(
                    "unterminated string literal",
                    Span::new(start, self.pos),
                ));
            };
            match b {
                b'"' => {
                    self.pos += 1;
                    let span = Span::new(start, self.pos);
                    return Ok(Token::new(TokenKind::Str(s), span));
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(&esc) = self.bytes.get(self.pos) else {
                        return Err(Diagnostic::new(
                            "unterminated escape",
                            Span::new(start, self.pos),
                        ));
                    };
                    let ch = match esc {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'\\' => '\\',
                        b'"' => '"',
                        b'0' => '\0',
                        other => {
                            return Err(Diagnostic::new(
                                format!("unknown escape: \\{}", other as char),
                                Span::new(self.pos - 1, self.pos + 1),
                            ));
                        }
                    };
                    s.push(ch);
                    self.pos += 1;
                }
                _ => {
                    // Берём всю utf-8 кодовую точку.
                    let ch_start = self.pos;
                    let ch_len = utf8_char_len(b);
                    let end = (ch_start + ch_len).min(self.bytes.len());
                    s.push_str(&self.src[ch_start..end]);
                    self.pos = end;
                }
            }
        }
    }

    fn lex_backtick(&mut self, start: usize) -> Result<Token, Diagnostic> {
        // `...` — backtick-строка для tagged templates (D48). В bootstrap
        // лексер выдаёт её как один TokenKind::Backtick(s) — сама
        // интерполяция и tag-функция в bootstrap не разворачиваются.
        // Компилятор Nova-on-Nova не использует sql`...` напрямую.
        self.pos += 1;
        let mut s = String::new();
        loop {
            let Some(&b) = self.bytes.get(self.pos) else {
                return Err(Diagnostic::new(
                    "unterminated backtick string",
                    Span::new(start, self.pos),
                ));
            };
            if b == b'`' {
                self.pos += 1;
                return Ok(Token::new(
                    TokenKind::Backtick(s),
                    Span::new(start, self.pos),
                ));
            }
            let ch_start = self.pos;
            let ch_len = utf8_char_len(b);
            let end = (ch_start + ch_len).min(self.bytes.len());
            s.push_str(&self.src[ch_start..end]);
            self.pos = end;
        }
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        b if b < 0x80 => 1,
        b if b < 0xC0 => 1, // некорректный продолжающий байт — продвигаем на 1
        b if b < 0xE0 => 2,
        b if b < 0xF0 => 3,
        _ => 4,
    }
}

/// Удобная обёртка: лексирует строку, возвращая Vec<Token>.
pub fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(src).lex()
}
