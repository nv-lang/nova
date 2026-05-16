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

pub use token::{DocCommentKind, Token, TokenKind};

use crate::diag::{Diagnostic, FileId, Span, MAIN_FILE_ID};

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    /// Plan 42 Sub-plan 42.4 шаг 2 (2026-05-14): FileId присваивается
    /// каждому Span создаваемому лексером. Default = MAIN_FILE_ID для
    /// entry/single-file (backward compat). imports.rs передаёт unique
    /// FileId для каждого imported peer-файла через
    /// `new_with_file_id`/`lex_with_file_id`.
    file_id: FileId,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self::new_with_file_id(src, MAIN_FILE_ID)
    }

    /// Plan 42 Sub-plan 42.4 шаг 2: lexer с explicit FileId.
    /// Все Span'ы (tokens + EOF) получат этот file_id.
    pub fn new_with_file_id(src: &'a str, file_id: FileId) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            file_id,
        }
    }

    /// Helper — construct Span с lexer's file_id.
    #[inline]
    fn span(&self, start: usize, end: usize) -> Span {
        Span::with_file(start, end, self.file_id)
    }

    /// Лексирует весь вход, возвращает Vec<Token>. EOF добавляется в конец.
    pub fn lex(&mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut out = Vec::new();
        loop {
            // Plan 45 / D104: пропускаем whitespace + не-doc комментарии.
            // Если встретили doc-comment (`///` или `//!`) — собираем
            // подряд идущие строки того же kind'а в один токен и
            // возвращаем; основной цикл продолжается со следующего
            // символа.
            if let Some(doc) = self.scan_trivia_and_doc()? {
                out.push(doc);
                continue;
            }
            if self.pos >= self.bytes.len() {
                let span = self.span(self.pos, self.pos);
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
            b'\'' => return self.lex_char(start),
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
            // Plan 33.1: `#` — attribute prefix (`#realtime`, `#pure`, etc.).
            // Не комментарий (комментарии только `//`). См. D-NN attribute syntax.
            b'#' => self.single(TokenKind::Hash),
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
                    // Plan 33.1 (D24): `==>` — импликация (3 байта),
                    // имеет приоритет над `==` (2 байта).
                    if self.peek_at(2) == Some(b'>') {
                        self.pos += 3;
                        TokenKind::Implies
                    } else {
                        self.pos += 2;
                        TokenKind::EqEq
                    }
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
                    // Plan 33.1 (D24): `<==>` — эквивалентность (4 байта),
                    // имеет приоритет над `<=` (2 байта).
                    if self.peek_at(2) == Some(b'=') && self.peek_at(3) == Some(b'>') {
                        self.pos += 4;
                        TokenKind::Iff
                    } else {
                        self.pos += 2;
                        TokenKind::Le
                    }
                }
                Some(b'<') => {
                    self.pos += 2;
                    TokenKind::Shl
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
                Some(b'>') => {
                    self.pos += 2;
                    TokenKind::Shr
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
                    self.pos += 1;
                    TokenKind::Amp
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
            b'^' => self.single(TokenKind::Caret),
            other => {
                return Err(Diagnostic::new(
                    format!("unexpected byte: {:?}", other as char),
                    self.span(start, start + 1),
                ));
            }
        };
        let span = self.span(start, self.pos);
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
    /// Plan 45 / D104: пропускает whitespace и non-doc line-комментарии;
    /// при встрече doc-comment (`///` или `//!`) собирает все подряд
    /// идущие строки того же kind'а в один токен.
    ///
    /// Возвращает:
    /// - `Ok(Some(doc_token))` — если был обнаружен doc-comment;
    /// - `Ok(None)` — если только пропустил trivia и упёрся в обычный
    ///   токен или EOF.
    ///
    /// Классификация (после `//`):
    /// - `//!` → Inner doc-comment.
    /// - `///` (ровно 3 слэша, четвёртый не слэш) → Outer doc-comment.
    /// - `////` (4+ слэша) → обычный line-комментарий (mirrors rustdoc,
    ///   предотвращает случайное doc-promotion для idiomatic
    ///   `//// SECTION` разделителей).
    /// - `//` + что угодно ещё → обычный line-комментарий.
    fn scan_trivia_and_doc(&mut self) -> Result<Option<Token>, Diagnostic> {
        loop {
            let Some(&b) = self.bytes.get(self.pos) else {
                return Ok(None);
            };
            match b {
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'/' if self.peek_at(1) == Some(b'/') => {
                    // Классифицируем форму после `//`.
                    match (self.peek_at(2), self.peek_at(3)) {
                        (Some(b'!'), _) => {
                            return Ok(Some(self.lex_doc_comment(DocCommentKind::Inner)?));
                        }
                        (Some(b'/'), Some(b'/')) => {
                            // `////` (4+) — обычный line-комментарий.
                            self.skip_line_comment();
                        }
                        (Some(b'/'), _) => {
                            // `///` ровно 3 — Outer doc-comment.
                            return Ok(Some(self.lex_doc_comment(DocCommentKind::Outer)?));
                        }
                        _ => {
                            // `//` + что угодно ещё — обычный комментарий.
                            self.skip_line_comment();
                        }
                    }
                }
                _ => return Ok(None),
            }
        }
    }

    /// Пропускает остаток строки (от текущей позиции до `\n` или EOF).
    /// Символ `\n` НЕ потребляется — он будет токенизирован как Newline.
    fn skip_line_comment(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if b == b'\n' {
                break;
            }
            self.pos += 1;
        }
    }

    /// Plan 45 / D104: лексирует одну или несколько подряд идущих
    /// doc-line того же `kind`'а в один `TokenKind::DocComment`.
    ///
    /// Каждая строка:
    /// 1. Потребляется префикс (`///` или `//!`).
    /// 2. Снимается одна опциональная ведущая пробел-позиция (rustdoc-
    ///    convention: `/// text` → `text`).
    /// 3. Захватывается остаток строки до `\n` (или EOF).
    /// 4. Если следующая строка (после `\n`) начинается с того же
    ///    префикса (с возможным leading whitespace) — продолжаем
    ///    собирать. Иначе — block завершён.
    ///
    /// Для Outer (`///`): продолжение `///` валидно, `////` — нет
    /// (это уже обычный комментарий, doc-block прерывается).
    /// Для Inner (`//!`): продолжение только `//!`; `////` коллизии нет.
    ///
    /// После сбора всех строк применяется indentation stripping:
    /// находим common leading whitespace по всем НЕ-пустым строкам и
    /// убираем его единообразно. Это нормализует индентацию markdown.
    ///
    /// Возвращаемый span покрывает все строки doc-блока от начала
    /// первого `///`/`//!` до конца последнего line content (без
    /// trailing `\n`).
    fn lex_doc_comment(&mut self, kind: DocCommentKind) -> Result<Token, Diagnostic> {
        let block_start = self.pos;
        let prefix_bytes: &[u8] = match kind {
            DocCommentKind::Outer => b"///",
            DocCommentKind::Inner => b"//!",
        };
        let mut lines: Vec<String> = Vec::new();
        // Конец span'а — позиция конца последней захваченной строки.
        // Цикл гарантированно выполнит ≥ 1 итерацию (нас вызвали
        // когда позиция стоит на префиксе).
        let mut block_end: usize;

        loop {
            // Инвариант: self.pos указывает на первый байт префикса.
            debug_assert_eq!(
                &self.bytes[self.pos..self.pos + 3],
                prefix_bytes,
                "lex_doc_comment должна вызываться только когда позиция на префиксе"
            );
            self.pos += 3;

            // Опциональный одиночный ведущий пробел.
            if self.peek_at(0) == Some(b' ') {
                self.pos += 1;
            }

            // Захватываем содержимое строки до \n или EOF.
            let line_start = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b == b'\n' {
                    break;
                }
                self.pos += 1;
            }
            let line_end_pos = self.pos;
            block_end = line_end_pos;

            // Извлекаем текст. Конвертация: исходник UTF-8 (Lexer'ом
            // гарантировано), но защитимся явной проверкой.
            let raw = std::str::from_utf8(&self.bytes[line_start..line_end_pos])
                .map_err(|_| {
                    Diagnostic::new(
                        "non-UTF-8 byte sequence inside doc-comment",
                        self.span(line_start, line_end_pos),
                    )
                })?;
            // CRLF-tolerance: уже на уровне skip_trivia мы съели `\r`
            // как часть whitespace, но внутри line content `\r` могут
            // остаться (если файл с CRLF). Снимаем trailing `\r`.
            let line = raw.trim_end_matches('\r').to_string();
            lines.push(line);

            // Потребляем `\n`, если есть.
            if self.peek_at(0) == Some(b'\n') {
                self.pos += 1;
            }

            // Проверяем, есть ли продолжение того же kind'а. На следующей
            // строке допустим leading whitespace (пробелы/табы), затем
            // должен идти ровно тот же префикс (для Outer — `///` но не
            // `////`; для Inner — `//!`).
            let mut peek_pos = self.pos;
            while let Some(&b) = self.bytes.get(peek_pos) {
                if b == b' ' || b == b'\t' {
                    peek_pos += 1;
                } else {
                    break;
                }
            }
            let has_prefix = self.bytes.get(peek_pos..peek_pos + 3) == Some(prefix_bytes);
            // Для Outer: после `///` следующий байт не должен быть `/`
            // (иначе это `////` — не doc).
            let is_overrun = match kind {
                DocCommentKind::Outer => self.bytes.get(peek_pos + 3) == Some(&b'/'),
                // Для Inner `//!` коллизии с `////` нет; `//!` уникальный.
                DocCommentKind::Inner => false,
            };
            if has_prefix && !is_overrun {
                self.pos = peek_pos;
                continue;
            }
            break;
        }

        // Indentation stripping: общий leading whitespace по всем
        // непустым строкам — снимается единообразно.
        let common_indent = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.bytes().take_while(|&b| b == b' ').count())
            .min()
            .unwrap_or(0);
        let content = if common_indent > 0 {
            lines
                .iter()
                .map(|l| {
                    let leading = l.bytes().take_while(|&b| b == b' ').count();
                    let cut = leading.min(common_indent);
                    l[cut..].to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            lines.join("\n")
        };

        let span = self.span(block_start, block_end);
        Ok(Token::new(
            TokenKind::DocComment { kind, content },
            span,
        ))
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
        let span = self.span(start, self.pos);
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
        let span = self.span(start, self.pos);
        if self.pos == digits_start {
            return Err(Diagnostic::new(
                format!("expected digits after radix prefix (base {radix})"),
                span,
            ));
        }
        let text = &self.src[digits_start..self.pos];
        let cleaned: String = text.chars().filter(|c| *c != '_').collect();
        // Сначала пробуем i64. Если не лезет (e.g. 0xCBF29CE484222325 в FNV-64 prime),
        // парсим как u64 и приводим к i64 wrapping — биты тождественны, что важно для
        // bitwise/hash операций. Это spec'оподобное поведение u64-литералов в i64-типе.
        let v = match i64::from_str_radix(&cleaned, radix) {
            Ok(v) => v,
            Err(_) => {
                let u = u64::from_str_radix(&cleaned, radix)
                    .map_err(|e| Diagnostic::new(format!("invalid int: {e}"), span))?;
                u as i64
            }
        };
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
        let span = self.span(start, self.pos);
        let kind = match text {
            "module" => TokenKind::KwModule,
            "import" => TokenKind::KwImport,
            "use" => TokenKind::KwUse,
            "export" => TokenKind::KwExport,
            "external" => TokenKind::KwExternal,
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
            // Plan 57: `bench` и `measure` — контекстуальные keyword'ы.
            // В lexer'е остаются обычными identifier'ами (иначе ломают
            // `module bench.X` paths, `bench.opaque(v)` namespace dispatch,
            // и identifier'ы пользователя с таким именем). Парсер distinguishes
            // по контексту: top-level `bench "<string>"` parses bench decl,
            // `bench.X` parses identifier expr. Аналог `apply` keyword.
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
            "protocol" => TokenKind::KwProtocol,
            "interrupt" => TokenKind::KwInterrupt,
            "forbid" => TokenKind::KwForbid,
            "realtime" => TokenKind::KwRealtime,
            "defer" => TokenKind::KwDefer,
            "errdefer" => TokenKind::KwErrDefer,
            "select" => TokenKind::KwSelect,
            "lemma" => TokenKind::KwLemma,
            // "apply" — контекстуальный keyword (не резервируем глобально, чтобы не ломать идентификаторы)
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
                    self.span(start, self.pos),
                ));
            };
            match b {
                b'"' => {
                    self.pos += 1;
                    let span = self.span(start, self.pos);
                    return Ok(Token::new(TokenKind::Str(s), span));
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(&esc) = self.bytes.get(self.pos) else {
                        return Err(Diagnostic::new(
                            "unterminated escape",
                            self.span(start, self.pos),
                        ));
                    };
                    match esc {
                        b'n' => { s.push('\n'); self.pos += 1; }
                        b't' => { s.push('\t'); self.pos += 1; }
                        b'r' => { s.push('\r'); self.pos += 1; }
                        b'\\' => { s.push('\\'); self.pos += 1; }
                        b'"' => { s.push('"'); self.pos += 1; }
                        b'0' => { s.push('\0'); self.pos += 1; }
                        b'$' => {
                            // \$ — escape для буквального ${ в interpolated string.
                            // Сохраняем sentinel-байт U+0001 (SOH) перед `$`, чтобы
                            // parser отличил literal-${ от interpolation-${.
                            // SOH в обычном Nova-коде не встречается (control char).
                            s.push('\u{0001}');
                            s.push('$');
                            self.pos += 1;
                        }
                        b'x' => {
                            // \xNN — ровно 2 hex digit'а, byte value 0..255.
                            // Для бинарных байтов в string (тест-кейсы, протоколы).
                            self.pos += 1; // 'x'
                            let hex_start = self.pos;
                            for _ in 0..2 {
                                match self.bytes.get(self.pos) {
                                    Some(&c) if c.is_ascii_hexdigit() => self.pos += 1,
                                    _ => return Err(Diagnostic::new(
                                        "expected 2 hex digits after \\x",
                                        self.span(hex_start.saturating_sub(2), self.pos + 1),
                                    )),
                                }
                            }
                            let hex_str = &self.src[hex_start..self.pos];
                            let byte_val = u8::from_str_radix(hex_str, 16).map_err(|_| {
                                Diagnostic::new(
                                    format!("invalid hex in \\x: {}", hex_str),
                                    self.span(hex_start, self.pos),
                                )
                            })?;
                            // Для байтов 0..127 — push as ASCII char (ровно 1 byte UTF-8).
                            // Для байтов 128..255 — push as Latin-1 codepoint (2 bytes UTF-8).
                            // Если нужны raw bytes для протокола — использовать Buffer/[]byte.
                            s.push(byte_val as char);
                        }
                        b'u' => {
                            // \u{HEX} — Unicode codepoint, encoded as UTF-8 в string.
                            self.pos += 1; // 'u'
                            if self.bytes.get(self.pos) != Some(&b'{') {
                                return Err(Diagnostic::new(
                                    "expected '{' after \\u in string literal",
                                    self.span(self.pos, self.pos + 1),
                                ));
                            }
                            self.pos += 1;
                            let hex_start = self.pos;
                            while let Some(&c) = self.bytes.get(self.pos) {
                                if c.is_ascii_hexdigit() { self.pos += 1; } else { break; }
                            }
                            let hex_end = self.pos;
                            if hex_end == hex_start {
                                return Err(Diagnostic::new(
                                    "expected hex digits in \\u{...}",
                                    self.span(hex_start, hex_end),
                                ));
                            }
                            let hex_str = &self.src[hex_start..hex_end];
                            let cp = u32::from_str_radix(hex_str, 16).map_err(|_| {
                                Diagnostic::new(
                                    format!("invalid hex in \\u{{...}}: {}", hex_str),
                                    self.span(hex_start, hex_end),
                                )
                            })?;
                            if cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF) {
                                return Err(Diagnostic::new(
                                    format!("invalid Unicode codepoint: U+{:X}", cp),
                                    self.span(hex_start, hex_end),
                                ));
                            }
                            if self.bytes.get(self.pos) != Some(&b'}') {
                                return Err(Diagnostic::new(
                                    "expected '}' to close \\u{...}",
                                    self.span(self.pos, self.pos + 1),
                                ));
                            }
                            self.pos += 1;
                            if let Some(c) = char::from_u32(cp) {
                                s.push(c);
                            } else {
                                return Err(Diagnostic::new(
                                    format!("invalid char codepoint: U+{:X}", cp),
                                    self.span(hex_start, hex_end),
                                ));
                            }
                        }
                        other => {
                            return Err(Diagnostic::new(
                                format!("unknown escape: \\{}", other as char),
                                self.span(self.pos - 1, self.pos + 1),
                            ));
                        }
                    }
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

    /// Q-char-literals: `'a'` / `'\n'` / `'\\'` / `'\''` / `'\u{1F600}'`.
    /// Возвращает TokenKind::Char(u32) с Unicode codepoint'ом.
    fn lex_char(&mut self, start: usize) -> Result<Token, Diagnostic> {
        self.pos += 1; // consume opening '
        let Some(&b) = self.bytes.get(self.pos) else {
            return Err(Diagnostic::new(
                "unterminated char literal",
                self.span(start, self.pos),
            ));
        };
        let cp: u32 = if b == b'\\' {
            self.pos += 1;
            let Some(&esc) = self.bytes.get(self.pos) else {
                return Err(Diagnostic::new(
                    "unterminated char escape",
                    self.span(start, self.pos),
                ));
            };
            match esc {
                b'n' => { self.pos += 1; '\n' as u32 }
                b't' => { self.pos += 1; '\t' as u32 }
                b'r' => { self.pos += 1; '\r' as u32 }
                b'\\' => { self.pos += 1; '\\' as u32 }
                b'\'' => { self.pos += 1; '\'' as u32 }
                b'"' => { self.pos += 1; '"' as u32 }
                b'0' => { self.pos += 1; 0 }
                b'u' => {
                    // \u{HEX}
                    self.pos += 1;
                    if self.bytes.get(self.pos) != Some(&b'{') {
                        return Err(Diagnostic::new(
                            "expected '{' after \\u in char literal",
                            self.span(self.pos, self.pos + 1),
                        ));
                    }
                    self.pos += 1;
                    let hex_start = self.pos;
                    while let Some(&c) = self.bytes.get(self.pos) {
                        if c.is_ascii_hexdigit() { self.pos += 1; } else { break; }
                    }
                    let hex_end = self.pos;
                    if hex_end == hex_start {
                        return Err(Diagnostic::new(
                            "expected hex digits in \\u{...}",
                            self.span(hex_start, hex_end),
                        ));
                    }
                    let hex_str = &self.src[hex_start..hex_end];
                    let cp = u32::from_str_radix(hex_str, 16).map_err(|_| {
                        Diagnostic::new(
                            format!("invalid hex in \\u{{...}}: {}", hex_str),
                            self.span(hex_start, hex_end),
                        )
                    })?;
                    if cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF) {
                        return Err(Diagnostic::new(
                            format!("invalid Unicode codepoint: U+{:X}", cp),
                            self.span(hex_start, hex_end),
                        ));
                    }
                    if self.bytes.get(self.pos) != Some(&b'}') {
                        return Err(Diagnostic::new(
                            "expected '}' to close \\u{...}",
                            self.span(self.pos, self.pos + 1),
                        ));
                    }
                    self.pos += 1;
                    cp
                }
                other => {
                    return Err(Diagnostic::new(
                        format!("unknown char escape: \\{}", other as char),
                        self.span(self.pos - 1, self.pos + 1),
                    ));
                }
            }
        } else {
            // UTF-8 codepoint (1-4 bytes). Decode it.
            let ch_len = utf8_char_len(b);
            let end = self.pos + ch_len;
            if end > self.bytes.len() {
                return Err(Diagnostic::new(
                    "incomplete UTF-8 in char literal",
                    self.span(start, self.pos),
                ));
            }
            let s = &self.src[self.pos..end];
            let cp = s.chars().next().ok_or_else(|| {
                Diagnostic::new("empty char literal", self.span(start, end))
            })? as u32;
            self.pos = end;
            cp
        };
        // Closing '
        if self.bytes.get(self.pos) != Some(&b'\'') {
            return Err(Diagnostic::new(
                "expected closing ' in char literal",
                self.span(self.pos, self.pos + 1),
            ));
        }
        self.pos += 1;
        let span = self.span(start, self.pos);
        Ok(Token::new(TokenKind::Char(cp), span))
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
                    self.span(start, self.pos),
                ));
            };
            if b == b'`' {
                self.pos += 1;
                return Ok(Token::new(
                    TokenKind::Backtick(s),
                    self.span(start, self.pos),
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
/// `file_id = MAIN_FILE_ID` (backward compat).
pub fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(src).lex()
}

/// Plan 42 Sub-plan 42.4 шаг 2: lex с explicit FileId.
/// Все Span'ы tokens получат указанный file_id.
pub fn lex_with_file_id(src: &str, file_id: FileId) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new_with_file_id(src, file_id).lex()
}

#[cfg(test)]
mod doc_comment_tests {
    //! Plan 45 / D104 unit-tests для лексера doc-comment'ов.
    //!
    //! Покрывает: распознавание `///` / `//!` / `////` (последний —
    //! НЕ doc); merging подряд идущих doc-line того же kind'а;
    //! indentation stripping; tolerance CRLF; разделение блоков
    //! не-doc-токеном между ними; tolerance к leading whitespace
    //! перед префиксом на continuation-строках.
    use super::*;
    use crate::diag::MAIN_FILE_ID;
    use crate::lexer::token::DocCommentKind;
    use crate::lexer::TokenKind;
    fn doc_tokens(src: &str) -> Vec<(DocCommentKind, String)> {
        Lexer::new_with_file_id(src, MAIN_FILE_ID)
            .lex()
            .expect("lex must succeed for valid input")
            .into_iter()
            .filter_map(|t| match t.kind {
                TokenKind::DocComment { kind, content } => Some((kind, content)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn outer_single_line() {
        let docs = doc_tokens("/// summary\nfn f() {}\n");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].0, DocCommentKind::Outer);
        assert_eq!(docs[0].1, "summary");
    }

    #[test]
    fn outer_multi_line_merged() {
        let docs = doc_tokens("/// first\n/// second\n/// third\nfn f() {}\n");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].0, DocCommentKind::Outer);
        assert_eq!(docs[0].1, "first\nsecond\nthird");
    }

    #[test]
    fn outer_empty_line_in_middle() {
        // Пустая doc-строка (`///` без содержимого) → пустая строка в content.
        let docs = doc_tokens("/// para1\n///\n/// para2\nfn f() {}\n");
        assert_eq!(docs[0].1, "para1\n\npara2");
    }

    #[test]
    fn inner_single_line() {
        let docs = doc_tokens("//! module summary\nmodule x\n");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].0, DocCommentKind::Inner);
        assert_eq!(docs[0].1, "module summary");
    }

    #[test]
    fn inner_multi_line_merged() {
        let docs = doc_tokens("//! first\n//! second\nmodule x\n");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].0, DocCommentKind::Inner);
        assert_eq!(docs[0].1, "first\nsecond");
    }

    #[test]
    fn four_slashes_is_not_doc() {
        // `////` — обычный комментарий, doc-token не эмитится.
        let docs = doc_tokens("//// section divider\nfn f() {}\n");
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn outer_followed_by_four_slashes_terminates_block() {
        // `///` block + `////` (обычный комментарий) — `///` block содержит
        // только первую строку; `////` пропускается как обычный.
        let docs = doc_tokens("/// real doc\n//// not doc\nfn f() {}\n");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].1, "real doc");
    }

    #[test]
    fn plain_double_slash_is_not_doc() {
        let docs = doc_tokens("// just a comment\nfn f() {}\n");
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn one_optional_leading_space_stripped() {
        // `/// text` → "text"; `///text` → "text"; `///  text` → " text"
        // (только ОДИН ведущий пробел снимается префикс-обработкой).
        let docs = doc_tokens("/// one\n///two\n///  three\nfn f() {}\n");
        assert_eq!(docs[0].1, "one\ntwo\n three");
    }

    #[test]
    fn indentation_stripping_uniform() {
        // Common leading whitespace ПОСЛЕ префикс-strip убирается
        // одинаково. Здесь у всех строк один общий пробельный
        // префикс — он снят, относительная индентация внутреннего
        // содержимого сохраняется.
        // Внутри content до stripping: "    indented\n  middle\n      deep"
        // common_indent по non-empty = 2 (вторая строка) → итог:
        // "  indented\nmiddle\n    deep"
        let docs = doc_tokens("///     indented\n///   middle\n///       deep\nfn f() {}\n");
        assert_eq!(docs[0].1, "  indented\nmiddle\n    deep");
    }

    #[test]
    fn crlf_line_endings_tolerated() {
        // CRLF в исходнике — `\r` снимается перед merging.
        let docs = doc_tokens("/// first\r\n/// second\r\nfn f() {}\r\n");
        assert_eq!(docs[0].1, "first\nsecond");
    }

    #[test]
    fn separate_outer_blocks_by_blank_line() {
        // Blank line между двумя doc-блоками → два отдельных токена.
        let docs = doc_tokens("/// first block\n\n/// second block\nfn f() {}\n");
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].1, "first block");
        assert_eq!(docs[1].1, "second block");
    }

    #[test]
    fn separate_outer_blocks_by_code() {
        // Между двумя doc-блоками — фактическая декларация.
        let docs = doc_tokens("/// for_f\nfn f() {}\n\n/// for_g\nfn g() {}\n");
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].1, "for_f");
        assert_eq!(docs[1].1, "for_g");
    }

    #[test]
    fn outer_then_inner_distinct_kinds() {
        // Outer и Inner не сливаются — это разные kind'ы.
        let docs = doc_tokens("/// outer\n//! inner\nmodule x\n");
        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].0, DocCommentKind::Outer);
        assert_eq!(docs[0].1, "outer");
        assert_eq!(docs[1].0, DocCommentKind::Inner);
        assert_eq!(docs[1].1, "inner");
    }

    #[test]
    fn leading_whitespace_before_continuation_prefix() {
        // На continuation-строке допустим leading whitespace перед `///`.
        // Тест с табом + пробелами.
        let docs = doc_tokens("    /// first\n    /// second\n    fn f() {}\n");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].1, "first\nsecond");
    }

    #[test]
    fn doc_at_eof_without_trailing_newline() {
        // Doc-comment в самом конце файла без `\n` — корректно
        // токенизируется (без panic'а).
        let docs = doc_tokens("/// end-of-file doc");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].1, "end-of-file doc");
    }

    #[test]
    fn empty_doc_line_only() {
        // Пустой `///` без содержимого — content = "".
        let docs = doc_tokens("///\nfn f() {}\n");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].1, "");
    }

    #[test]
    fn nova_code_block_in_doc_content_preserved() {
        // Внутри doc-content — markdown / nova code-block; лексер
        // оставляет содержимое как сырой текст (markdown парсит
        // collector).
        let src = "/// Example:\n///\n/// ```nova\n/// let x = 1\n/// ```\nfn f() {}\n";
        let docs = doc_tokens(src);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].1, "Example:\n\n```nova\nlet x = 1\n```");
    }

    #[test]
    fn outer_attaches_to_next_item_via_token_stream() {
        // Doc-токен идёт перед `KwFn` — проверяем что в стриме идут оба
        // в правильном порядке.
        let toks: Vec<TokenKind> = Lexer::new("/// fn-doc\nfn f() {}\n")
            .lex()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect();
        let doc_idx = toks
            .iter()
            .position(|t| matches!(t, TokenKind::DocComment { .. }))
            .expect("doc must be in stream");
        let fn_idx = toks
            .iter()
            .position(|t| matches!(t, TokenKind::KwFn))
            .expect("fn keyword must be in stream");
        assert!(doc_idx < fn_idx, "doc-comment must precede `fn` in stream");
    }
}
