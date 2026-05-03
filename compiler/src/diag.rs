//! Диагностика — `Span` и `Diagnostic`.
//!
//! Без структурированных ошибок (R5.3) — это bootstrap. Текстовые
//! сообщения с указанием места.

use std::fmt;

/// Диапазон позиций в исходнике (byte offset, end-exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn dummy() -> Self {
        Self::default()
    }

    /// Объединяет два span'а в покрывающий оба.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
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
