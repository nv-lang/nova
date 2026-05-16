//! Диагностика — `Span`, `Diagnostic`, `SourceMap`.
//!
//! Без структурированных ошибок (R5.3) — это bootstrap. Текстовые
//! сообщения с указанием места.
//!
//! **Plan 35 Ф.0 (AD6/N11):** `Span` имеет `file_id: u32` для
//! cross-file diagnostics. `SourceMap` — registry всех файлов
//! compilation unit. `file_id=0` зарезервирован для main module
//! (legacy single-file mode без cross-file deps работает с этим
//! значением по умолчанию — backward compat).

use std::fmt;
use std::path::PathBuf;

/// Идентификатор файла в `SourceMap`. `0` = main module / legacy.
pub type FileId = u32;

pub const MAIN_FILE_ID: FileId = 0;

/// Диапазон позиций в исходнике (byte offset, end-exclusive)
/// + identifier файла для cross-file diagnostics.
///
/// Backward-compat: `Span::new(start, end)` создаёт span с
/// `file_id = MAIN_FILE_ID`. Cross-file resolver вызывает
/// `Span::with_file(start, end, file_id)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    /// File identifier for cross-file diagnostics (Plan 35 Ф.0).
    /// `0` = main module / legacy single-file mode.
    pub file_id: FileId,
}

impl Span {
    /// Создаёт span с `file_id = MAIN_FILE_ID` (backward compat).
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end, file_id: MAIN_FILE_ID }
    }

    /// Создаёт span с explicit file_id (Plan 35 cross-file).
    pub fn with_file(start: usize, end: usize, file_id: FileId) -> Self {
        Self { start, end, file_id }
    }

    pub fn dummy() -> Self {
        Self::default()
    }

    /// Объединяет два span'а в покрывающий оба. file_id берётся из
    /// `self` (предполагается что merge'аются span'ы одного файла).
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            file_id: self.file_id,
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.file_id == MAIN_FILE_ID {
            write!(f, "{}..{}", self.start, self.end)
        } else {
            write!(f, "[file#{}] {}..{}", self.file_id, self.start, self.end)
        }
    }
}

/// Plan 35 Ф.0 (AD6): registry всех файлов текущей compilation unit.
/// `FileId` — index в `files`. `0` = main module по convention.
///
/// `Diagnostic::render_with_map(&source_map)` использует registry для
/// resolution `file_id → (path, source)`, что необходимо для
/// cross-file diagnostic rendering.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

/// Один файл в `SourceMap` — путь + source content (для line/col
/// resolution в diagnostics).
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует main module (file_id = MAIN_FILE_ID = 0).
    /// Должен быть вызван первым.
    pub fn register_main(&mut self, path: PathBuf, source: String) -> FileId {
        debug_assert!(self.files.is_empty(), "register_main must be called first");
        self.files.push(SourceFile { path, source });
        MAIN_FILE_ID
    }

    /// Регистрирует imported file. Возвращает новый `FileId`.
    pub fn register(&mut self, path: PathBuf, source: String) -> FileId {
        let id = self.files.len() as FileId;
        self.files.push(SourceFile { path, source });
        id
    }

    /// Возвращает file по id, или None если id invalid.
    pub fn get(&self, file_id: FileId) -> Option<&SourceFile> {
        self.files.get(file_id as usize)
    }

    /// Возвращает path по id, или fallback `"<unknown>"`.
    pub fn path_for(&self, file_id: FileId) -> &str {
        self.files.get(file_id as usize)
            .map(|f| f.path.to_str().unwrap_or("<bad-utf8>"))
            .unwrap_or("<unknown>")
    }

    /// Возвращает source по id, или fallback пустой строкой.
    pub fn source_for(&self, file_id: FileId) -> &str {
        self.files.get(file_id as usize)
            .map(|f| f.source.as_str())
            .unwrap_or("")
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Насколько безопасно инструменту (`nova fix` / LSP code-action)
/// авто-применять `Suggestion` без участия человека.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applicability {
    /// Замена корректна и может быть применена автоматически.
    MachineApplicable,
    /// Замена правдоподобна, но может быть неверна — требует ревью.
    MaybeIncorrect,
    /// Замена содержит плейсхолдеры — не применять без правки.
    HasPlaceholders,
}

/// Структурированное предложение правки — машинно-применимый edit.
///
/// `span` — регион замены; **нулевая ширина** (`start == end`) означает
/// чистую вставку в точке `start`. `replacement` — текст, который туда
/// подставляется. `message` — человекочитаемая подсказка (producer
/// формирует её без доступа к source — поэтому suggestion
/// source-независим и применим для cross-file диагностик).
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub message: String,
    pub span: Span,
    pub replacement: String,
    pub applicability: Applicability,
}

/// Дополнительный контекст к диагностике — `note: ...`. `span = Some`
/// привязывает note к месту (например `parameter declared here`),
/// `None` — общая ремарка.
#[derive(Debug, Clone)]
pub struct Note {
    pub message: String,
    pub span: Option<Span>,
}

/// Диагностическое сообщение об ошибке. Несёт основной `message` + span,
/// опциональные `notes` (доп. контекст, в т.ч. с привязкой к месту) и
/// `suggestion` (машинно-применимая правка — готова для `nova fix` /
/// LSP code-action).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub notes: Vec<Note>,
    pub suggestion: Option<Suggestion>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            notes: Vec::new(),
            suggestion: None,
        }
    }

    /// Добавить `note` с привязкой к месту (`note: ... --> file:line`).
    pub fn with_note_at(mut self, message: impl Into<String>, span: Span) -> Self {
        self.notes.push(Note { message: message.into(), span: Some(span) });
        self
    }

    /// Добавить общий `note` без привязки к месту.
    pub fn with_note(mut self, message: impl Into<String>) -> Self {
        self.notes.push(Note { message: message.into(), span: None });
        self
    }

    /// Прикрепить структурированное предложение правки.
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestion = Some(suggestion);
        self
    }

    /// Красиво отрендерить диагностику с фрагментом исходника.
    /// (line, col) восстанавливается по тексту. Notes и suggestion
    /// рендерятся под основным сообщением.
    pub fn render(&self, source: &str, file: &str) -> String {
        let resolver = SrcResolver::Single { source, file };
        let (src, path) = resolver.resolve(&self.span);
        let (line, col) = byte_to_line_col(src, self.span.start);
        let mut out = format!("{}:{}:{}: error: {}", path, line, col, self.message);
        // Plan 45 Ф.23.18: source-snippet with caret highlighting.
        append_snippet(&mut out, src, &self.span, line, col);
        self.render_extras(&mut out, &resolver);
        out
    }

    /// Plan 35 Ф.0: render через SourceMap — берёт source + path
    /// автоматически по `span.file_id`. Используется в cross-file
    /// diagnostics. Notes/suggestion резолвят свой `file_id` через map —
    /// поэтому cross-file note показывает правильный файл.
    ///
    /// Если `file_id` не найден в map — fallback на "<unknown>".
    pub fn render_with_map(&self, map: &SourceMap) -> String {
        let resolver = SrcResolver::Map(map);
        let (src, path) = resolver.resolve(&self.span);
        let (line, col) = byte_to_line_col(src, self.span.start);
        let mut out = format!("{}:{}:{}: error: {}", path, line, col, self.message);
        self.render_extras(&mut out, &resolver);
        out
    }

    /// Общий рендер `notes` + `suggestion`. `resolver` отдаёт
    /// `(source, path)` для произвольного span'а — единая точка для
    /// single-file и cross-file путей.
    fn render_extras(&self, out: &mut String, resolver: &SrcResolver<'_>) {
        for note in &self.notes {
            match &note.span {
                Some(s) => {
                    let (src, path) = resolver.resolve(s);
                    let (l, c) = byte_to_line_col(src, s.start);
                    out.push_str(&format!("\n  note: {} --> {}:{}:{}", note.message, path, l, c));
                }
                None => out.push_str(&format!("\n  note: {}", note.message)),
            }
        }
        if let Some(sg) = &self.suggestion {
            let (src, path) = resolver.resolve(&sg.span);
            let (l, c) = byte_to_line_col(src, sg.span.start);
            out.push_str(&format!(
                "\n  help: {} --> {}:{}:{} [{}]",
                sg.message, path, l, c, applicability_tag(sg.applicability),
            ));
        }
    }
}

/// Резолвер `span → (source, path)` для рендера диагностик. Единый
/// интерфейс для single-file (`render`) и cross-file (`render_with_map`)
/// путей — span'ы notes/suggestion резолвятся по своему `file_id`.
enum SrcResolver<'a> {
    /// Single-file: все span'ы из одного файла.
    Single { source: &'a str, file: &'a str },
    /// Cross-file: span резолвится по `file_id` через `SourceMap`.
    Map(&'a SourceMap),
}

impl<'a> SrcResolver<'a> {
    fn resolve(&self, span: &Span) -> (&str, &str) {
        match self {
            SrcResolver::Single { source, file } => (source, file),
            SrcResolver::Map(m) => (m.source_for(span.file_id), m.path_for(span.file_id)),
        }
    }
}

fn applicability_tag(a: Applicability) -> &'static str {
    match a {
        Applicability::MachineApplicable => "machine-applicable",
        Applicability::MaybeIncorrect => "maybe-incorrect",
        Applicability::HasPlaceholders => "has-placeholders",
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error at {}: {}", self.span, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// Преобразует byte offset в (line, col), 1-based. Используется
/// рендерингом диагностики.
/// Plan 45 Ф.23.18: append source-line + caret (`^^^^`) to diagnostic output.
fn append_snippet(out: &mut String, source: &str, span: &Span, line: usize, col: usize) {
    let lines: Vec<&str> = source.lines().collect();
    let start_line = line.saturating_sub(1); // 0-indexed
    // Compute end line/col from span.end.
    let (end_line_1, end_col_1) = byte_to_line_col(source, span.end.saturating_sub(1).max(span.start));
    let end_line = end_line_1.saturating_sub(1); // 0-indexed

    if end_line <= start_line {
        // Single-line snippet.
        let line_text = lines.get(start_line).copied().unwrap_or("");
        let span_len = if span.end > span.start { span.end - span.start } else { 1 };
        let caret_width = span_len.min(line_text.len().saturating_sub(col.saturating_sub(1))).max(1);
        let indent = col.saturating_sub(1);
        out.push('\n');
        out.push_str(&format!("{} | {}", line, line_text));
        out.push('\n');
        out.push_str(&format!("  | {}{}", " ".repeat(indent), "^".repeat(caret_width)));
    } else {
        // Plan 45 Ф.24.7: multi-line snippet — first line with ^^^^, last line with ~~~~.
        let first_text = lines.get(start_line).copied().unwrap_or("");
        let indent = col.saturating_sub(1);
        let first_caret = first_text.len().saturating_sub(indent).max(1);
        out.push('\n');
        out.push_str(&format!("{} | {}", line, first_text));
        out.push('\n');
        out.push_str(&format!("  | {}{}", " ".repeat(indent), "^".repeat(first_caret)));
        // Intermediate lines (skipped if adjacent).
        if end_line > start_line + 1 {
            out.push_str(&format!("\n  | ... ({} more lines)", end_line - start_line - 1));
        }
        // Last line with ~~~~.
        let last_text = lines.get(end_line).copied().unwrap_or("");
        let tilde_width = end_col_1.saturating_sub(1).max(1);
        out.push('\n');
        out.push_str(&format!("{} | {}", end_line_1, last_text));
        out.push('\n');
        out.push_str(&format!("  | {}", "~".repeat(tilde_width)));
    }
}

pub fn byte_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_merge() {
        let a = Span::new(0, 5);
        let b = Span::new(3, 10);
        assert_eq!(a.merge(b), Span::new(0, 10));
    }

    #[test]
    fn line_col_basic() {
        let src = "abc\ndef\nghi";
        assert_eq!(byte_to_line_col(src, 0), (1, 1));
        assert_eq!(byte_to_line_col(src, 4), (2, 1));
        assert_eq!(byte_to_line_col(src, 5), (2, 2));
        assert_eq!(byte_to_line_col(src, 8), (3, 1));
    }
}
