//! Plan 45 Ф.5 + Ф.25.2 — markdown utilities для doc-content'а.
//!
//! Без внешних зависимостей (no pulldown-cmark). Production-grade
//! summary-extractor: учитывает inline code, fenced blocks, markdown
//! links, naked URLs, decimal numbers, common abbreviations.

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

/// Plan 45 Ф.5 / D104 / D107: parse markdown body на стандартные секции.
///
/// Распознаваемые секции (по style-guide §11.5 fixed order):
/// - `# Examples` / `# Example`
/// - `# Errors`
/// - `# Panics`
/// - `# Safety`
/// - `# Effects`
/// - `# Contracts`
/// - `# Since`
/// - `# See also` / `# See Also`
/// - `# Deprecated`
///
/// Заголовок распознаётся в первой колонне строки (без отступа).
/// Соответствие имени — case-insensitive, без учёта trailing
/// whitespace. Любые другие `# Heading` сохраняются как часть текущей
/// секции (renderer'у решать что делать).
///
/// Возвращает `Sections` с ключами в lowercase (для JSON output по D107).
/// Возвращаемая `body` — текст до первого распознанного `# Heading`
/// (т.е. общая часть, не относящаяся ни к одной секции).
pub fn split_sections(body: &str) -> ParsedBody {
    let mut sections: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut current_section: Option<String> = None;
    let mut current_buf = String::new();
    let mut intro_buf = String::new();

    fn flush(
        current: &mut Option<String>,
        buf: &mut String,
        sections: &mut std::collections::BTreeMap<String, String>,
    ) {
        if let Some(name) = current.take() {
            let trimmed = buf.trim().to_string();
            if !trimmed.is_empty() {
                sections.insert(name, trimmed);
            }
            buf.clear();
        }
    }

    for raw_line in body.lines() {
        let line = raw_line;
        // Section-heading: ровно один `#`, пробел, имя.
        if let Some(rest) = line.strip_prefix("# ") {
            let heading = rest.trim();
            let canonical = canonical_section_name(heading);
            if let Some(name) = canonical {
                // Flush previous section/intro.
                if current_section.is_some() {
                    flush(&mut current_section, &mut current_buf, &mut sections);
                }
                current_section = Some(name);
                continue;
            }
        }
        if current_section.is_some() {
            if !current_buf.is_empty() {
                current_buf.push('\n');
            }
            current_buf.push_str(line);
        } else {
            if !intro_buf.is_empty() {
                intro_buf.push('\n');
            }
            intro_buf.push_str(line);
        }
    }
    // Flush last section.
    flush(&mut current_section, &mut current_buf, &mut sections);

    ParsedBody {
        intro: {
            let s = intro_buf.trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        },
        sections,
    }
}

/// Результат `split_sections`. `intro` — текст до первого распознанного
/// section heading'а (общая часть description'а). `sections` —
/// distinct-секции, ключеванные lowercase-именем.
#[derive(Debug, Clone, Default)]
pub struct ParsedBody {
    pub intro: Option<String>,
    pub sections: std::collections::BTreeMap<String, String>,
}

/// Канонизировать имя секции в lowercase. Возвращает `None` для
/// неизвестных секций (renderer оставит их в общем тексте).
fn canonical_section_name(heading: &str) -> Option<String> {
    let h = heading.trim().to_ascii_lowercase();
    match h.as_str() {
        "examples" | "example" => Some("examples".to_string()),
        "errors" => Some("errors".to_string()),
        "panics" => Some("panics".to_string()),
        "safety" => Some("safety".to_string()),
        "effects" => Some("effects".to_string()),
        "contracts" => Some("contracts".to_string()),
        "since" => Some("since".to_string()),
        "see also" => Some("see also".to_string()),
        "deprecated" => Some("deprecated".to_string()),
        _ => None,
    }
}

/// Найти первое sentence-terminating предложение в строке. Игнорирует
/// `.`/`?`/`!` внутри inline-кода (backtick'ов) и markdown-link'ов.
/// Plan 45 Ф.25.2: markdown-aware first-sentence extractor.
///
/// Игнорирует терминаторы (`.`/`!`/`?`) внутри:
/// - inline ``code`` (одинарные backticks)
/// - ```fenced code blocks``` (тройные backticks)
/// - markdown links `[text](url)` — URL может содержать `.`
/// - common abbreviations: `e.g.`, `i.e.`, `etc.`, `Mr.`, `Mrs.`, `Dr.`,
///   `vs.`, `cf.`, `Fig.`, `No.`, `St.`, `Ave.`, `vol.`, `pp.`
/// - decimal numbers: `3.14`, `1.0.0` (терминатор следует за digit, до digit)
/// - URLs без markdown wrapping: `https://x.com/path.`
///
/// Возвращает первое предложение включая terminator. Если терминатор
/// не найден — весь text.
fn first_sentence(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Fenced code block ```...```.
        if i + 2 < bytes.len() && &bytes[i..i + 3] == b"```" {
            // Skip until closing fence.
            i += 3;
            while i + 2 < bytes.len() && &bytes[i..i + 3] != b"```" {
                i += 1;
            }
            i = (i + 3).min(bytes.len());
            continue;
        }
        // Inline code `...`.
        if b == b'`' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'`' {
                if bytes[i] == b'\n' { break; }
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            continue;
        }
        // Markdown link [text](url) — пропускаем URL целиком.
        if b == b'[' {
            // Найти `]` затем `(` затем закрывающую `)`.
            let mut j = i + 1;
            let mut depth = 1;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if depth == 0 && j < bytes.len() && bytes[j] == b'(' {
                // [text](url) — skip до `)`.
                let mut k = j + 1;
                let mut paren_depth = 1;
                while k < bytes.len() && paren_depth > 0 {
                    match bytes[k] {
                        b'(' => paren_depth += 1,
                        b')' => paren_depth -= 1,
                        _ => {}
                    }
                    k += 1;
                }
                i = k;
                continue;
            }
            // Просто [...] без URL — продолжаем дальше после `]`.
            i = j;
            continue;
        }
        // Naked URL: `https://...` / `http://...` — skip whitespace.
        if b == b'h' && (text[i..].starts_with("http://") || text[i..].starts_with("https://")) {
            // Skip до whitespace или конца строки.
            while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\n') {
                i += 1;
            }
            continue;
        }
        if b == b'.' || b == b'!' || b == b'?' {
            // Decimal: digit . digit — не терминатор.
            let prev = if i > 0 { bytes[i - 1] } else { 0 };
            let next = bytes.get(i + 1).copied();
            if b == b'.'
                && prev.is_ascii_digit()
                && matches!(next, Some(n) if n.is_ascii_digit())
            {
                i += 1;
                continue;
            }
            // Abbreviation check: ищем word до точки.
            if b == b'.' && is_after_abbreviation(text, i) {
                i += 1;
                continue;
            }
            // Terminator: следующий символ — whitespace или конец.
            let is_terminator = match next {
                None => true,
                Some(b' ') | Some(b'\t') | Some(b'\n') => true,
                _ => false,
            };
            if is_terminator {
                return text[..=i].to_string();
            }
        }
        i += 1;
    }
    text.to_string()
}

/// Plan 45 Ф.25.2: проверка что точка на позиции `dot_idx` — часть
/// known abbreviation (e.g., `e.g.`, `Mr.`). Берём слово до точки
/// (буквы/цифры) и сравниваем case-insensitive с whitelist.
fn is_after_abbreviation(text: &str, dot_idx: usize) -> bool {
    const ABBREVIATIONS: &[&str] = &[
        "e.g", "i.e", "etc", "vs", "cf", "fig", "no", "st", "ave",
        "vol", "pp", "mr", "mrs", "ms", "dr", "prof", "jr", "sr",
        "inc", "ltd", "co", "ca", "approx",
    ];
    let bytes = text.as_bytes();
    let mut word_start = dot_idx;
    while word_start > 0 {
        let b = bytes[word_start - 1];
        if b.is_ascii_alphanumeric() || b == b'.' {
            word_start -= 1;
        } else {
            break;
        }
    }
    if word_start >= dot_idx {
        return false;
    }
    let word = &text[word_start..dot_idx].to_ascii_lowercase();
    ABBREVIATIONS.iter().any(|abbr| word == abbr || word.ends_with(&format!(".{}", abbr)))
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

    // Plan 45 Ф.25.2 — markdown-aware summary extraction edge cases.

    #[test]
    fn ignores_dot_inside_markdown_link_url() {
        // URL contains `.` — должно ignore.
        let src = "See [docs](https://example.com/api.html) for more. Second sentence.";
        let (s, _d) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("See [docs](https://example.com/api.html) for more."));
    }

    #[test]
    fn ignores_dot_inside_fenced_code_block() {
        // Fenced code с `.` внутри — терминатор НЕ должен срабатывать.
        let src = "Uses `format!` with template.\n```\nlet x = a.b.c;\n```\nTrailing.";
        let (s, _) = extract_summary(src);
        // Должно остановиться на первой точке после `template`.
        assert_eq!(s.as_deref(), Some("Uses `format!` with template."));
    }

    #[test]
    fn ignores_decimal_numbers() {
        let src = "Computes 3.14 * radius squared. End.";
        let (s, _) = extract_summary(src);
        // `3.14` — не терминатор.
        assert_eq!(s.as_deref(), Some("Computes 3.14 * radius squared."));
    }

    #[test]
    fn ignores_version_strings() {
        let src = "Available since 1.0.0 of the library. After.";
        let (s, _) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("Available since 1.0.0 of the library."));
    }

    #[test]
    fn ignores_common_abbreviations_eg() {
        let src = "Useful for collections, e.g. arrays and maps. Second.";
        let (s, _) = extract_summary(src);
        // `e.g.` — не терминатор.
        assert_eq!(s.as_deref(), Some("Useful for collections, e.g. arrays and maps."));
    }

    #[test]
    fn ignores_common_abbreviations_ie() {
        let src = "Returns a copy, i.e. an independent value. After.";
        let (s, _) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("Returns a copy, i.e. an independent value."));
    }

    #[test]
    fn ignores_naked_url() {
        let src = "Source: https://example.com/path.html and more. After.";
        let (s, _) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("Source: https://example.com/path.html and more."));
    }

    #[test]
    fn handles_exclamation_in_macro() {
        // `format!` — НЕ терминатор, потому что после `!` идёт `(` или word char.
        let src = "Uses `format!` for output. After.";
        let (s, _) = extract_summary(src);
        // `format!` внутри backticks → defended by inline-code handling.
        assert_eq!(s.as_deref(), Some("Uses `format!` for output."));
    }

    #[test]
    fn question_mark_terminator_works() {
        let src = "Is this safe? Yes, it is.";
        let (s, _) = extract_summary(src);
        assert_eq!(s.as_deref(), Some("Is this safe?"));
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

    #[test]
    fn split_sections_basic_examples() {
        let body = "Some intro text.\n\n# Examples\n\n```nova\nlet x = 1\n```";
        let parsed = split_sections(body);
        assert_eq!(parsed.intro.as_deref(), Some("Some intro text."));
        assert_eq!(
            parsed.sections.get("examples").map(|s| s.as_str()),
            Some("```nova\nlet x = 1\n```")
        );
    }

    #[test]
    fn split_sections_multiple() {
        let body = "Intro.\n\n# Examples\n\nE1.\n\n# Errors\n\nNotFound.";
        let parsed = split_sections(body);
        assert_eq!(parsed.intro.as_deref(), Some("Intro."));
        assert_eq!(parsed.sections.len(), 2);
        assert_eq!(parsed.sections.get("examples").map(|s| s.as_str()), Some("E1."));
        assert_eq!(parsed.sections.get("errors").map(|s| s.as_str()), Some("NotFound."));
    }

    #[test]
    fn split_sections_case_insensitive() {
        let body = "# EXAMPLES\n\nx";
        let parsed = split_sections(body);
        assert!(parsed.sections.contains_key("examples"));
    }

    #[test]
    fn split_sections_unknown_heading_kept_in_intro_or_section() {
        // Unknown `# Foo` остаётся как часть текущего блока, не создаёт
        // новой секции.
        let body = "Intro.\n# Foo\nFoo body.\n\n# Examples\n\nE1.";
        let parsed = split_sections(body);
        // intro содержит "# Foo\nFoo body."
        assert!(parsed.intro.as_deref().unwrap().contains("# Foo"));
        assert_eq!(parsed.sections.get("examples").map(|s| s.as_str()), Some("E1."));
    }

    #[test]
    fn split_sections_empty_body() {
        let parsed = split_sections("");
        assert!(parsed.intro.is_none());
        assert!(parsed.sections.is_empty());
    }

    #[test]
    fn split_sections_no_sections_just_intro() {
        let parsed = split_sections("Plain description with no sections.");
        assert_eq!(parsed.intro.as_deref(), Some("Plain description with no sections."));
        assert!(parsed.sections.is_empty());
    }

    #[test]
    fn split_sections_with_see_also() {
        let body = "Intro.\n\n# See also\n\n- [foo]\n- [bar]";
        let parsed = split_sections(body);
        assert_eq!(parsed.sections.get("see also").map(|s| s.as_str()), Some("- [foo]\n- [bar]"));
    }
}
