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
            // D412: `x"…"` hex-blob literal — MUST be checked before the
            // generic ident-start branch (`x` alone is a valid identifier;
            // ident+string juxtaposition with no operator between them is
            // not otherwise legal Nova syntax, so hijacking this exact
            // two-byte sequence is non-breaking — mirrors D/Rust `b"…"`).
            b'x' if self.peek_at(1) == Some(b'"') => return self.lex_hex_blob(start),
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
            b'%' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::PercentEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Percent
                }
            },
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
            // Plan 234 Ф.2 (D46-амендмент): `~` — побитовое дополнение,
            // отдельный от `!` (логического) оператор/токен. Нет compound-
            // формы (`~=` не существует — `~` унарный).
            b'~' => self.single(TokenKind::Tilde),
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
                    // Plan 234 Ф.2а: `<<=` (3 байта) имеет приоритет над `<<` (2 байта).
                    if self.peek_at(2) == Some(b'=') {
                        self.pos += 3;
                        TokenKind::ShlEq
                    } else {
                        self.pos += 2;
                        TokenKind::Shl
                    }
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
                    // Plan 234 Ф.2а: `>>=` (3 байта) имеет приоритет над `>>` (2 байта).
                    if self.peek_at(2) == Some(b'=') {
                        self.pos += 3;
                        TokenKind::ShrEq
                    } else {
                        self.pos += 2;
                        TokenKind::Shr
                    }
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
                // Plan 234 Ф.2а: `&=` compound bitwise-and-assign.
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::AmpEq
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
                // Plan 234 Ф.2а: `|=` compound bitwise-or-assign.
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::PipeEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Pipe
                }
            },
            // Plan 234 Ф.2а: `^=` compound bitwise-xor-assign.
            b'^' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    TokenKind::CaretEq
                }
                _ => {
                    self.pos += 1;
                    TokenKind::Caret
                }
            },
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
            // Plan 172.1-K4: u64-fallback СИММЕТРИЧНО lex_radix_int (hex/bin/oct). Десятичный
            // литерал в (i64::MAX, u64::MAX] (значение `uint`/`u64`) парсится как u64 и кладётся
            // в i64-носитель wrapping — биты тождественны (важно для bitwise/hash; D130
            // uint-литералы). Контекст int↔uint не у лексера (TokenKind::Int(i64)); диапазон-чек
            // — забота чекера.
            let v: i64 = match cleaned.parse::<i64>() {
                Ok(v) => v,
                Err(_) => cleaned
                    .parse::<u64>()
                    .map_err(|e| Diagnostic::new(format!("invalid int: {e}"), span))?
                    as i64,
            };
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
            // Plan 239 (D443): `use` — контекстуальный keyword (был hard
            // keyword `KwUse`). В lexer'е — обычный identifier (иначе
            // ломает identifier'ы пользователя с таким именем — поле/
            // переменная/функция `use`). Парсер distinguishes по контексту
            // ровно так же, как `bench`/`measure`/`apply`: import-synonym
            // (`use path.to.mod` на месте `import`), record-field embed
            // (`use alias Type` внутри `type { ... }`, D39), protocol embed
            // (`use TypeName` в начале `protocol { ... }` тела, D145 §
            // Protocol composition) — все три позиции проверяют lookahead,
            // иначе `use` падает через в обычный Ident.
            "export" => TokenKind::KwExport,
            "external" => TokenKind::KwExternal,
            // Plan 91.12 Ф.-1 (D282): `extern "nova" fn` / `extern "C" fn`.
            "extern" => TokenKind::KwExtern,
            "fn" => TokenKind::KwFn,
            "type" => TokenKind::KwType,
            "effect" => TokenKind::KwEffect,
            // Plan 97 Ф.3 (D142): keyword `handler` снят (clean break).
            // Литерал handler'а пишется через `effect X { ... }`
            // (тот же keyword, что и в declaration `type X effect { ... }`).
            // Дисамбигуация — позиция (см. parser/parse_atom).
            // `handler` теперь обычный идентификатор.
            "alias" => TokenKind::KwAlias,
            "let" => TokenKind::KwLet,
            "const" => TokenKind::KwConst,
            "mut" => TokenKind::KwMut,
            "consume" => TokenKind::KwConsume,
            // Plan 172.5 (D326): `ref` — parameter passing-mode marker (safe
            // in-out / borrow), NOT a type. Global keyword: `ref` is unused as
            // an identifier anywhere in std/examples/tests (verified), so
            // reserving it is non-breaking.
            "ref" => TokenKind::KwRef,
            // Plan 114 (D184): `ro` — canonical short keyword.
            "ro" => TokenKind::KwRo,
            // Plan 114 (D184): retracted; lexer still recognizes the lexeme
            // so parser can emit `E_KW_REMOVED_READONLY` with a clear hint.
            "readonly" => TokenKind::KwReadonly,
            // Plan 118 (D216 §8, D2 amend); §10a rename (Plan 174.5, 2026-07-11):
            // `unsafe` keyword. Used в:
            //   - `unsafe { ... }` block (Ф.3),
            //   - `#unsafe` attribute / `unsafe fn` declaration (Ф.3),
            //   - legacy fn-pointer composition `*unsafe fn(...)` /
            //     `*extern "C" unsafe fn(...)` (D216 §10, NOT renamed —
            //     encodes call-requires-unsafe, not possibly-uninit data).
            // The possibly-uninit type-modifier (`*unsafe T` / `unsafe T`,
            // T non-Func) is RENAMED to `uninit` (below) — `unsafe` there is
            // now `E_UNSAFE_TYPE_MODIFIER_RENAMED` (parser/mod.rs).
            "unsafe" => TokenKind::KwUnsafe,
            // §10a rename (Plan 174.5, 2026-07-11): `uninit` — possibly-uninit
            // type-modifier, split off `unsafe`. `*uninit T` / `uninit T`.
            "uninit" => TokenKind::KwUninit,
            // Plan 118.7 (D216 §4 amend): `raw` остаётся идентификатором
            // (контекстное ключевое слово, аналог `bench`/`measure`).
            // Парсер распознаёт `raw &expr` контекстно в parse_unary().
            // Plan 118.5 V3 §V3.4 → RETIRED in Plan 138.5 (2026-06-11):
            // `safe` was a type-position propagation-stopper (`unsafe * safe
            // T`). With prefix `unsafe *` now forbidden there is nothing to
            // stop, so the parser rejects `safe` in type position with
            // `E_SAFE_RETIRED`. The keyword is still tokenized (so the
            // diagnostic is precise rather than "expected type").
            "safe" => TokenKind::KwSafe,
            // Plan 124 (D220): per-field private visibility modifier +
            // type-level default flip (`type X priv { ... }`). Compile-time
            // enforcement of `priv` field access — выбрасывает
            // E_PRIV_FIELD_{READ,WRITE,INIT,PATTERN} вне type-methods scope.
            "priv" => TokenKind::KwPriv,
            // Plan 124 (D220): explicit public override marker. Required для
            // override type-level priv default; redundant без него.
            "pub" => TokenKind::KwPub,
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
            "blocking" => TokenKind::KwBlocking,
            "protocol" => TokenKind::KwProtocol,
            "interrupt" => TokenKind::KwInterrupt,
            "forbid" => TokenKind::KwForbid,
            "realtime" => TokenKind::KwRealtime,
            "defer" => TokenKind::KwDefer,
            "errdefer" => TokenKind::KwErrDefer,
            "okdefer" => TokenKind::KwOkDefer,
            "select" => TokenKind::KwSelect,
            "lemma" => TokenKind::KwLemma,
            // "apply" — контекстуальный keyword (не резервируем глобально, чтобы не ломать идентификаторы)
            // Plan 115 D214: `null` тоже контекстуально recognized (только в
            // expression-position в комбинации `null ptr`). НЕ резервируем
            // глобально чтобы не ломать `JsonValue.null()` и подобные method
            // names (Plan 207: бывший пример `AtomicPtr.null()` — тип снят
            // 2026-07-16, до generic AtomicPtr[T] Plan 103.7).
            _ => TokenKind::Ident(text.to_string()),
        };
        Ok(Token::new(kind, span))
    }

    fn lex_string(&mut self, start: usize) -> Result<Token, Diagnostic> {
        // "..." — строка. Поддерживает \n, \t, \r, \\, \", \0, \x.., \u{..}
        // и `${...}` интерполяцию (Plan 102, D258-амендмент, PEP 701/JS-путь).
        //
        // Ключевой момент: `${...}` — это НЕ строковое содержимое, а Nova-
        // выражение. Внутри него могут легально встречаться вложенные строки
        // (`"${m["key"]}"`, `"${req.param("name")}"`) и вложенные
        // интерполяции (`"${f("x ${y} z")}"`) — их кавычки/скобки НЕ должны
        // закрывать ЭТУ строку раньше времени (старый баг: одномерный скан
        // "до первой неэкранированной `"`" слепо натыкался на внутреннюю
        // `"` и обрывал строку). Когда встречаем неэкранированный `${`,
        // `scan_interpolation_body` со своим string/brace-aware стеком
        // находит ИСТИННУЙ конец интерполяции (её `}`), и мы копируем этот
        // диапазон СЫРЫМ (без escape-декодирования — это исходный Nova-код,
        // его decode/re-lex делает `desugar_string_interpolation` в
        // parser/mod.rs через собственный sub-lex), затем продолжаем
        // обычный посимвольный скан строки после закрывающей `}`.
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
                        // D467 §3 (план 277 Ф.1): `\` перед переводом строки —
                        // продолжение строки. Съедает и сам перенос (`\n` или
                        // `\r\n`), и ведущие пробелы/табы следующей строки, так
                        // что длинный текст без переводов строк пишется в
                        // несколько строк исходника, оставаясь ОДНИМ литералом.
                        // Перенос ЯВНЫЙ, поэтому забытая кавычка по-прежнему
                        // обрывается на конце строки — семантика rustc/Roslyn.
                        b'\r' | b'\n' => {
                            if esc == b'\r' && self.bytes.get(self.pos + 1) == Some(&b'\n') {
                                self.pos += 2;
                            } else {
                                self.pos += 1;
                            }
                            while matches!(self.bytes.get(self.pos), Some(&b' ') | Some(&b'\t')) {
                                self.pos += 1;
                            }
                        }
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
                            // `other as char` — это ОДИН БАЙТ, прочитанный как
                            // Latin-1: для многобайтной буквы он называет
                            // символ, которого в исходнике нет вовсе (`\я`
                            // сообщался как `\Ñ`). Тот же корень, что у падения
                            // в `lex_backtick`, но здесь он только врал в
                            // сообщении, а не ронял компилятор — реестр №853.
                            // `self.pos` стоит НА экранируемом байте и это
                            // граница символа (перед ним ASCII-слэш), поэтому
                            // срез безопасен.
                            let ch = self.src[self.pos..]
                                .chars()
                                .next()
                                .unwrap_or(other as char);
                            let ch_end = self.pos + ch.len_utf8();
                            return Err(Diagnostic::new(
                                format!("unknown escape: \\{}", ch),
                                self.span(self.pos - 1, ch_end),
                            ));
                        }
                    }
                }
                b'$' if self.peek_at(1) == Some(b'{') => {
                    // Plan 102 (D258-амендмент): неэкранированный `${` —
                    // начало интерполяции. `\$` (literal-escape) обработан
                    // ВЫШЕ (arm `b'\\'`, sentinel-механика) и сюда не
                    // попадает — здесь только настоящий triggers.
                    let interp_start = self.pos;
                    let brace_pos = self.pos + 1;
                    match scan_interpolation_body(self.bytes, brace_pos) {
                        Some(close_pos) => {
                            // Сырая копия ВКЛЮЧИТЕЛЬНО `${` .. `}` — без
                            // escape-декодирования: это Nova-исходник,
                            // desugar_string_interpolation() re-lex'ит его
                            // сама (parser/mod.rs).
                            s.push_str(&self.src[interp_start..=close_pos]);
                            self.pos = close_pos + 1;
                        }
                        None => {
                            return Err(Diagnostic::new(
                                "unterminated interpolation (started here): `${` has no \
                                 matching `}` before end of string/file",
                                self.span(interp_start, interp_start + 2),
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

    /// D412: `x"48 69"` — hex-blob literal. Hex digits + separators (`_`,
    /// space, `\n`, tolerated `\r` for CRLF sources) between the quotes;
    /// separators are pure visual grouping and carry no value. Odd digit
    /// count → `E_HEX_BLOB_ODD`; non-hex/non-separator byte → `E_HEX_BLOB_CHAR`.
    /// Empty `x""` is legal → empty `[]u8`. NOT a numeric literal: leading
    /// zero bytes are significant, byte order = written order (no
    /// endianness reinterpretation).
    fn lex_hex_blob(&mut self, start: usize) -> Result<Token, Diagnostic> {
        self.pos += 2; // `x"`
        let mut digits: Vec<u8> = Vec::new(); // ASCII hex-digit bytes, separators stripped
        loop {
            let Some(&b) = self.bytes.get(self.pos) else {
                return Err(Diagnostic::new(
                    "unterminated hex-blob literal",
                    self.span(start, self.pos),
                ));
            };
            match b {
                b'"' => {
                    self.pos += 1;
                    break;
                }
                b'_' | b' ' | b'\n' | b'\r' | b'\t' => {
                    self.pos += 1;
                }
                _ if b.is_ascii_hexdigit() => {
                    digits.push(b);
                    self.pos += 1;
                }
                other => {
                    return Err(Diagnostic::new(
                        format!(
                            "[E_HEX_BLOB_CHAR] invalid character {:?} in hex-blob literal \
                             (D412): only hex digits and `_`/space/newline separators are \
                             allowed inside `x\"…\"`",
                            other as char
                        ),
                        self.span(self.pos, self.pos + 1),
                    ));
                }
            }
        }
        let span = self.span(start, self.pos);
        if digits.len() % 2 != 0 {
            return Err(Diagnostic::new(
                format!(
                    "[E_HEX_BLOB_ODD] hex-blob literal has an odd number of hex digits \
                     ({}) — D412 requires an even count (each byte = 2 digits)",
                    digits.len()
                ),
                span,
            ));
        }
        let mut bytes = Vec::with_capacity(digits.len() / 2);
        for pair in digits.chunks(2) {
            // `is_ascii_hexdigit` guaranteed above — parse cannot fail.
            let s = std::str::from_utf8(pair).expect("ascii hexdigit bytes are valid utf8");
            let v = u8::from_str_radix(s, 16).expect("validated hex-digit pair");
            bytes.push(v);
        }
        Ok(Token::new(TokenKind::HexBlob(bytes), span))
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
        // `...` — backtick-строка для tagged templates (D48). Лексер выдаёт
        // её как один TokenKind::Backtick(s) с СЫРЫМ текстом (включая
        // escape-последовательности и `${…}`) — interpolation-split и
        // разворачивание escape'ов делает ПАРСЕР при D48-desugar'е (там
        // точные byte-offset'ы для span'ов sub-выражений). Здесь только
        // правило терминации: экранированный `\`` НЕ закрывает литерал.
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
            // D48 (2026-07-02): escape сохраняется сырым (backslash + символ).
            //
            // ЭКРАНИРУЕМЫЙ СИМВОЛ БЫВАЕТ МНОГОБАЙТНЫМ, и здесь стояло
            // `self.pos + 2`, то есть «слэш плюс РОВНО ОДИН БАЙТ». По D467 §6
            // набор escape в backtick закрыт тремя (`` \` ``, `\\`, `\$`), а
            // ЛЮБОЙ другой символ после слэша проходит насквозь как два
            // символа — в том числе кириллическая буква в два байта. Срез
            // `&self.src[self.pos..esc_end]` тогда резал символ ПОСРЕДИНЕ, и
            // Rust падал: «byte index is not a char boundary». Это ICE на
            // законном исходнике: падал не только `nova build`, но и языковой
            // сервер — редактор перезапускал его на каждой правке такого файла.
            // Реестр 221.1 №853.
            //
            // Строкой ниже, в обычной ветке, тот же лексер уже делает верно —
            // через `utf8_char_len`; здесь ровно то же правило.
            if b == b'\\' {
                let esc_len = self
                    .bytes
                    .get(self.pos + 1)
                    .map_or(0, |&nb| utf8_char_len(nb));
                let esc_end = (self.pos + 1 + esc_len).min(self.bytes.len());
                s.push_str(&self.src[self.pos..esc_end]);
                self.pos = esc_end;
                continue;
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

/// Plan 102 (D258-амендмент): единственный string/brace-aware сканер тела
/// `${...}`-интерполяции — используется И лексером (`lex_string`, чтобы
/// корректно найти истинный конец строкового литерала, содержащего
/// вложенные строки/скобки внутри интерполяции), И парсером
/// (`desugar_string_interpolation` в `parser/mod.rs`, чтобы разбить уже
/// вырезанную строку на литерал/expr-части). Один алгоритм, одно место —
/// не два независимо поддерживаемых дубля (196-консолидация).
///
/// `bytes[brace_pos]` ДОЛЖЕН быть байтом `{` из `${`-триггера. Скан вперёд
/// с небольшим стеком режимов (`false` = expr-режим, `true` = string-режим):
///
/// - **expr-режим**: `{`/`}` считаются как обычная вложенность (record-
///   литералы/блоки — включая вложенную интерполяцию: её `${` не
///   спец-обрабатывается ЗДЕСЬ на уровне `{`, спец-обработка — при входе В
///   string-режим ниже, а закрывающая `}` вложенной интерполяции просто
///   балансируется как обычная закрывающая скобка); `"` открывает вложенную
///   строку (push string-режим).
/// - **string-режим**: `\` целиком пропускает следующий байт (чтобы `\"` не
///   закрыл вложенную строку раньше времени); `"` закрывает вложенную
///   строку (pop); неэкранированный `${` внутри вложенной строки — ещё одна
///   (рекурсивная) интерполяция — push нового expr-режим-фрейма; тот же
///   стек унифицированно обрабатывает произвольную глубину вложенности.
///
/// Возвращает `Some(idx)` — индекс закрывающей `}`, парной исходному `${` —
/// или `None`, если вход кончился раньше, чем интерполяция закрылась
/// (незакрытая интерполяция).
pub(crate) fn scan_interpolation_body(bytes: &[u8], brace_pos: usize) -> Option<usize> {
    // `false` = expr-mode фрейм (стартуем в нём — `brace_pos` это `{`
    // самого `${`, который мы уже "вошли"), `true` = string-mode фрейм.
    let mut stack: Vec<bool> = vec![false];
    let mut i = brace_pos + 1;
    loop {
        let b = *bytes.get(i)?;
        let in_string = *stack.last().expect("стек не пустеет без return");
        if in_string {
            match b {
                b'\\' => i += 2, // весь escape целиком (\" \\ \n \$ ...)
                b'"' => {
                    stack.pop();
                    i += 1;
                }
                b'$' if bytes.get(i + 1) == Some(&b'{') => {
                    // Вложенная интерполяция внутри вложенной строки.
                    stack.push(false);
                    i += 2;
                }
                _ => i += 1,
            }
        } else {
            match b {
                b'"' => {
                    stack.push(true);
                    i += 1;
                }
                b'{' => {
                    stack.push(false);
                    i += 1;
                }
                b'}' => {
                    stack.pop();
                    if stack.is_empty() {
                        return Some(i);
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
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

/// Plan 104.10 Ф.5 ([M-104.10-hardcode-lists]): single-source keyword predicate
/// for external tooling (nova-lsp rename validation). A word is a *reserved*
/// keyword iff the lexer classifies it as a non-identifier keyword token — i.e.
/// exactly one significant token whose kind is NOT `Ident`. This makes the
/// lexer's own `match` (above, `lex_ident`) the single source of truth so tools
/// never drift with a stale hand-maintained keyword list.
///
/// Contextual keywords that the lexer intentionally keeps as identifiers
/// (`bench`, `measure`, `apply`, `raw`, `null`, `use` — Plan 233/D443) are —
/// correctly — reported as NOT reserved: they are valid identifiers and thus
/// valid rename targets.
///
/// Retracted-but-still-lexed lexemes (`let`, `readonly`) are reported as
/// reserved: they tokenize to `KwLet` / `KwReadonly` (so the parser can emit a
/// precise "removed keyword" diagnostic), and a tool must not let a user rename
/// a symbol *to* them.
pub fn is_reserved_keyword(word: &str) -> bool {
    // Only alphabetic/underscore words can be keywords; reject anything that
    // would not lex as a single identifier-shaped token (operators, digits,
    // whitespace, empty).
    if word.is_empty()
        || !word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || word.chars().next().map_or(true, |c| c.is_ascii_digit())
    {
        return false;
    }
    match lex(word) {
        Ok(tokens) => {
            // Skip trailing Newline/Eof; expect exactly one significant token.
            let mut sig = tokens
                .iter()
                .filter(|t| !matches!(t.kind, TokenKind::Newline | TokenKind::Eof));
            match (sig.next(), sig.next()) {
                (Some(tok), None) => !matches!(tok.kind, TokenKind::Ident(_)),
                _ => false,
            }
        }
        Err(_) => false,
    }
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

#[cfg(test)]
mod interp_nested_string_tests {
    //! Plan 102 (D258-амендмент, реестр 221.1 №102): unit-тесты на
    //! `scan_interpolation_body` и `lex_string`'s string/brace-aware
    //! termination scan — PEP 701/JS-путь для вложенных строк/кавычек
    //! внутри `${...}`. Гран-кейсы: nested string in a call arg, nested
    //! string in index syntax, nested interpolation inside a nested
    //! string, `{}`-nesting (record/block) inside the expr, and genuinely
    //! unterminated `${` (EOF before the matching `}`).
    use super::*;

    fn lex_str_content(src: &str) -> String {
        match Lexer::new(src).lex() {
            Ok(toks) => match &toks[0].kind {
                TokenKind::Str(s) => s.clone(),
                other => panic!("expected Str token, got {:?}", other),
            },
            Err(e) => panic!("lex failed: {}", e.message),
        }
    }

    #[test]
    fn nested_quote_in_method_call_arg_does_not_terminate_outer_string() {
        // The old one-dimensional scan used to stop at the `"` before
        // `name` — treating it as the outer string's terminator.
        let content = lex_str_content(r#""hello, ${req.param("name")}""#);
        assert_eq!(content, r#"hello, ${req.param("name")}"#);
    }

    #[test]
    fn nested_quote_in_index_key_does_not_terminate_outer_string() {
        // The typical `m["key"]` case named explicitly in the plan.
        let content = lex_str_content(r#""${m["key"]}""#);
        assert_eq!(content, r#"${m["key"]}"#);
    }

    #[test]
    fn nested_interpolation_inside_nested_string() {
        let content = lex_str_content(r#""${f("x ${y} z")}""#);
        assert_eq!(content, r#"${f("x ${y} z")}"#);
    }

    #[test]
    fn record_literal_braces_inside_interpolation_balance_correctly() {
        // `{}` that belong to the expression (not a nested string) must
        // nest correctly — the interpolation's own closing `}` is the
        // one that brings brace-depth back to zero.
        let content = lex_str_content(r#""${ if c { a } else { b } }""#);
        assert_eq!(content, "${ if c { a } else { b } }");
    }

    #[test]
    fn escaped_dollar_brace_is_untouched_by_the_fix() {
        // `\$` sentinel mechanics — delta-0, unrelated to `${` scanning.
        let content = lex_str_content(r#""literal \${x}""#);
        assert_eq!(content, "literal \u{0001}${x}");
    }

    #[test]
    fn unterminated_interpolation_reports_precise_span_at_the_dollar_brace() {
        // Exact repro from the plan: `"abc ${x` running off EOF with no
        // matching `}`.
        let src = "\"abc ${x";
        let err = Lexer::new(src).lex().expect_err("must fail to lex");
        assert!(
            err.message.contains("unterminated interpolation (started here)"),
            "unexpected message: {}",
            err.message
        );
        // Span must point at the `${` (byte offset 5..7 in `"abc ${x`),
        // not at the whole string / whole file.
        assert_eq!(err.span.start, 5);
        assert_eq!(err.span.end, 7);
    }

    #[test]
    fn scan_interpolation_body_finds_matching_brace_directly() {
        // `${m["key"]}` — brace_pos is the index of `{` (1).
        let bytes = b"${m[\"key\"]}";
        let close = scan_interpolation_body(bytes, 1).expect("must find matching }");
        assert_eq!(bytes[close], b'}');
        assert_eq!(close, bytes.len() - 1);
    }

    #[test]
    fn scan_interpolation_body_none_when_unterminated() {
        let bytes = b"${x";
        assert_eq!(scan_interpolation_body(bytes, 1), None);
    }
}
