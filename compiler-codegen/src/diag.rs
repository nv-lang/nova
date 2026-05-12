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

/// Диагностическое сообщение об ошибке. Минимальное по форме — для
/// bootstrap'а структурированные ошибки не нужны.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    /// Красиво отрендерить диагностику с фрагментом исходника.
    /// (line, col) восстанавливается по тексту.
    pub fn render(&self, source: &str, file: &str) -> String {
        let (line, col) = byte_to_line_col(source, self.span.start);
        format!("{}:{}:{}: error: {}", file, line, col, self.message)
    }

    /// Plan 35 Ф.0: render через SourceMap — берёт source + path
    /// автоматически по `span.file_id`. Используется в cross-file
    /// diagnostics.
    ///
    /// Если `file_id` не найден в map — fallback на "<unknown>".
    pub fn render_with_map(&self, map: &SourceMap) -> String {
        let source = map.source_for(self.span.file_id);
        let path = map.path_for(self.span.file_id);
        let (line, col) = byte_to_line_col(source, self.span.start);
        format!("{}:{}:{}: error: {}", path, line, col, self.message)
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
