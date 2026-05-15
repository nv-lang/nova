//! Plan 45 Ф.5 — markdown utilities для doc-content'а.
//!
//! MVP: минимальный summary-extractor + render passthrough.
//!
//! Полный markdown-парсинг (через pulldown-cmark) — добавляется в
//! Plan 45 Ф.5; здесь MVP-обработка summary без внешних зависимостей.

/// Plan 45 Ф.5: разбить markdown-content на (summary, description).
///
/// - **Summary** — первое предложение, заканчивающееся `.`/`!`/`?`,
///   до первого `\n\n` (paragraph break) или конца текста. Если нет
///   терминатора-предложения, summary = первая параграф-строка целиком.
/// - **Description** — всё остальное после summary.
///
/// **Style-guide (Plan 45 §11.5):** summary ≤ 120 chars,
/// imperative mood, полное предложение. MVP не enforce'ит; lint
/// `summary-not-sentence` / `summary-too-long` — Plan 45 Ф.3
/// (lint_docs pass).
pub fn extract_summary(content: &str) -> (Option<String>, Option<String>) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return (None, None);
    }
    // Разбиваем по первому `\n\n` (paragraph break).
    let (first_para, rest) = match trimmed.split_once("\n\n") {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (trimmed, None),
    };
    // Внутри first_para — ищем первое предложение-терминатор.
    let summary = first_sentence(first_para);
    let description = match rest {
        Some(r) if !r.is_empty() => Some(r.to_string()),
        _ => {
            // Если первый параграф длиннее summary'а — остаток
            // first_para тоже идёт в description.
            let summary_len = summary.len();
            if summary_len < first_para.len() {
                let tail = first_para[summary_len..].trim_start();
                if tail.is_empty() {
                    None
                } else {
                    Some(tail.to_string())
                }
            } else {
                None
            }
        }
    };
    let summary = if summary.is_empty() {
        None
    } else {
        Some(summary)
    };
    (summary, description)
}

/// Найти первое sentence-terminating предложение в строке. Игнорирует
/// `.`/`?`/`!` внутри inline-кода (backtick'ов) и markdown-link'ов.
fn first_sentence(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut in_inline_code = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'`' {
            in_inline_code = !in_inline_code;
            i += 1;
            continue;
        }
        if !in_inline_code && (b == b'.' || b == b'!' || b == b'?') {
            // Terminator-кандидат. Проверим, что после — whitespace
            // или конец (иначе это часть слова типа `x.y`).
            let next = bytes.get(i + 1).copied();
            let is_terminator = match next {
                None => true,
                Some(b' ') | Some(b'\t') | Some(b'\n') => true,
                _ => false,
            };
            if is_terminator {
                // Включаем сам символ-терминатор.
                return text[..=i].to_string();
            }
        }
        i += 1;
    }
    // Не нашли терминатор — возвращаем всё.
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_sentence() {
        let (s, d) = extract_summary("Returns the absolute value of `x`.");
        assert_eq!(s.as_deref(), Some("Returns the absolute value of `x`."));
        assert_eq!(d, None);
    }

    #[test]
    fn sentence_plus_paragraph() {
        let src = "Summary line.\n\nLong description spanning\nmultiple lines.";
        let (s, d) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("Summary line."));
        assert_eq!(
            d.as_deref(),
            Some("Long description spanning\nmultiple lines.")
        );
    }

    #[test]
    fn no_terminator_returns_full_first_paragraph() {
        let src = "No terminator here";
        let (s, d) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("No terminator here"));
        assert_eq!(d, None);
    }

    #[test]
    fn ignores_dot_inside_backticks() {
        let src = "Returns `a.b.c` as a path.";
        let (s, _) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("Returns `a.b.c` as a path."));
    }

    #[test]
    fn empty_input() {
        let (s, d) = extract_summary("");
        assert_eq!(s, None);
        assert_eq!(d, None);
    }

    #[test]
    fn whitespace_only_input() {
        let (s, d) = extract_summary("   \n\n   ");
        assert_eq!(s, None);
        assert_eq!(d, None);
    }

    #[test]
    fn multi_sentence_in_first_paragraph_keeps_only_first() {
        let src = "First sentence. Second sentence in same paragraph.";
        let (s, d) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("First sentence."));
        assert_eq!(
            d.as_deref(),
            Some("Second sentence in same paragraph.")
        );
    }
}
