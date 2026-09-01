// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 177 Ф.3 / D325 — **conformance guard** над стабильной `std/**.nv`
//! (§8.2 плана 177). Source-of-truth: `spec/decisions/04-effects.md` D325.
//!
//! Три инварианта, каждый — отдельный `#[test]`:
//!
//! 1. `guard_no_own_fail_in_public_std_signatures` (R1/R5) — публичная std-
//!    сигнатура НЕ несёт `Fail[E]` для СОБСТВЕННОЙ ошибки (→ обязан Result).
//!    Прозрачный проброс `Fail[E]` из `fn(...) … Fail[E]`-параметра (R5
//!    effect-polymorphic forwarding) — ЛЕГАЛЕН. Явный exempt-list (§2 D325):
//!    `Option@unwrap`/`Result@unwrap` (мост D85 `!!`), `on_exit` (R5 protocol-
//!    member), весь `testing/property.nv` (Q5 — assert/test-DSL). Пути здесь
//!    и далее — std-source-root-relative (манифест `[lib] src`, Plan 195),
//!    без "std/" префикса.
//!
//! 2. `naming_lint_no_opt_suffix_or_orphan_try_prefix` (A2 = R3/R4 negative) —
//!    ни один публичный API не имеет суффикса `_opt` (fallibility ≠ absence,
//!    R4); префикс `try_` — только при наличии одноимённого INFALLIBLE-сиблинга
//!    (R3). Established-идиома `try_start`/`try_start_won` (Once non-blocking
//!    init — genuine absence, blocking-`start` by-design нет) — явный exempt.
//!
//! 3. `net_family_has_zero_fail` — `std/net/**` (эталон Result-everywhere) не
//!    содержит `Fail[` НИ в одной публичной сигнатуре (§8.2, 3-й bullet).
//!
//! Скан — по ИСХОДНИКУ (hermetic, без вызова компилятора): триггер только на
//! РЕАЛЬНЫЕ декларации (`export fn` / `export extern … fn` / `extern … fn`),
//! поэтому `Fail[` в doc-комментах (`///`) и в теле — НЕ ложно-срабатывают.
//! `std/_experimental/**` исключён (§9 Q3 — отложенный TODO, вне stable).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

// ───────────────────────────── file discovery ──────────────────────────────

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/compiler-codegen → parent = <repo>.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler-codegen has a parent (repo root)")
        .to_path_buf()
}

/// std SOURCE root — манифест-derived (`resolve_std_path` уважает `[lib]
/// src` из `std/nova.toml`; Plan 195). НЕ хардкодим `std/` или `std/src/`
/// напрямую — источник истины — манифест, как для любого другого пакета.
fn std_root() -> PathBuf {
    nova_codegen::manifest::resolve_std_path(&repo_root())
}

fn collect_nv(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_nv(&p, out);
        } else if p.extension().map_or(false, |x| x == "nv") {
            out.push(p);
        }
    }
}

/// Все `.nv` под std source root'ом, КРОМЕ `_experimental/**` (отложенный
/// TODO, §9 Q3).
fn stable_std_files() -> Vec<PathBuf> {
    let mut all = Vec::new();
    collect_nv(&std_root(), &mut all);
    all.retain(|p| {
        !p.components()
            .any(|c| c.as_os_str() == "_experimental")
    });
    all.sort();
    assert!(
        !all.is_empty(),
        "no std/*.nv found — std_root() = {:?}",
        std_root()
    );
    all
}

/// std-source-root-relative, forward-slash path (стабильно для
/// exempt-матчинга на Windows И независимо от расположения std-корня —
/// манифест решает, живёт ли std плоско или на `src/`).
fn rel(p: &Path) -> String {
    p.strip_prefix(std_root())
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

// ─────────────────────────── signature scanning ────────────────────────────

/// Одна публичная декларация: собранный (возможно многострочный) заголовок
/// сигнатуры + извлечённые имя/ресивер.
struct Decl {
    rel: String,
    line: usize, // 1-based
    header: String,
    name: String,
    // receiver сохраняется для будущих per-receiver проверок; naming-lint
    // сейчас матчит сиблинга глобально (см. rationale в тесте), поэтому поле
    // не читается — префикс `_` глушит dead-code warning.
    _receiver: Option<String>,
}

/// Триггер декларации: `export fn …`, `export extern … fn …`, `extern … fn …`.
/// (Приватные `fn …` и doc-комменты `/// …` — НЕ публичная поверхность.)
fn is_decl_start(trimmed: &str) -> bool {
    let toks: Vec<&str> = trimmed.split_whitespace().collect();
    match toks.as_slice() {
        ["export", "fn", ..] => true,
        ["export", "extern", ..] => toks.contains(&"fn"),
        ["extern", ..] => toks.contains(&"fn"),
        _ => false,
    }
}

/// Заголовок = от строки-декларации до завершения сигнатуры. Терминатор:
///   • тело-разделитель `{` / `=>` (Nova-body fn), ЛИБО
///   • баланс `()`/`[]` после открытия списка параметров (extern-fn без тела —
///     одна строка `… @lock() -> MutexGuard consume`, `-> bool` и т.п.).
/// Второе критично: extern-декларации не имеют `{`/`=>`, иначе заголовок
/// поглотил бы последующие декларации (теряя их имена из сиблинг-набора).
fn gather_header(lines: &[&str], start: usize) -> (String, usize) {
    let mut header = String::new();
    let mut depth = 0i32; // () + [] вложенность
    let mut j = start;
    while j < lines.len() {
        header.push_str(lines[j]);
        header.push(' ');
        for c in lines[j].chars() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                _ => {}
            }
        }
        if lines[j].contains('{') || lines[j].contains("=>") {
            break; // тело Nova-fn началось
        }
        if depth <= 0 && header.contains('(') {
            break; // список параметров закрыт (extern one-liner завершён)
        }
        j += 1;
    }
    (header, j)
}

/// Имя = последний identifier-run перед главным `(`. Ресивер = первый
/// тип-токен, если декларация метода (`Recv @m` / `Recv.m` / `Recv[T] @m`).
fn parse_name_receiver(header: &str) -> (Option<String>, String) {
    let before_paren = header.split('(').next().unwrap_or(header);
    // Отбрасываем ведущие ключевые слова до и включая `fn`.
    let after_fn = match before_paren.find(" fn ") {
        Some(pos) => &before_paren[pos + 4..],
        None => before_paren,
    };
    let mut after_fn = after_fn.trim();
    // Пропускаем generics ФУНКЦИИ, если заголовок начинается с `[T, E]`.
    if after_fn.starts_with('[') {
        if let Some(close) = after_fn.find(']') {
            after_fn = after_fn[close + 1..].trim();
        }
    }

    // Имя: последний максимальный run [A-Za-z0-9_] в `after_fn`.
    let name = last_ident(after_fn).unwrap_or_default();

    // Ресивер: метод, если есть `@` или `.`.
    let receiver = if after_fn.contains('@') || after_fn.contains('.') {
        let first = after_fn
            .split(|c: char| c == ' ' || c == '.')
            .next()
            .unwrap_or("");
        let recv = first.split('[').next().unwrap_or(first).trim();
        if recv.is_empty() {
            None
        } else {
            Some(recv.to_string())
        }
    } else {
        None
    };

    (receiver, name)
}

fn last_ident(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = chars.len();
    // пропустить хвостовые не-ident-символы
    while i > 0 && !is_ident(chars[i - 1]) {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    let hi = i;
    while i > 0 && is_ident(chars[i - 1]) {
        i -= 1;
    }
    Some(chars[i..hi].iter().collect())
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn all_decls() -> Vec<Decl> {
    let mut out = Vec::new();
    for f in stable_std_files() {
        let Ok(src) = fs::read_to_string(&f) else { continue };
        let lines: Vec<&str> = src.lines().collect();
        let r = rel(&f);
        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim_start();
            if is_decl_start(trimmed) {
                let (header, endj) = gather_header(&lines, i);
                let (receiver, name) = parse_name_receiver(&header);
                out.push(Decl {
                    rel: r.clone(),
                    line: i + 1,
                    header,
                    name,
                    _receiver: receiver,
                });
                i = endj + 1;
            } else {
                i += 1;
            }
        }
    }
    out
}

// ────────────────────────── Fail[...] extraction ───────────────────────────

/// Все имена ошибок внутри `Fail[<X>]` в срезе (`[` balanced).
fn fail_error_names(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = s.chars().collect();
    let needle: Vec<char> = "Fail[".chars().collect();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..i + needle.len()] == needle[..] {
            // читаем до balanced ']'
            let mut depth = 1;
            let mut j = i + needle.len();
            let start = j;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    break;
                }
                j += 1;
            }
            let inner: String = bytes[start..j].iter().collect();
            out.push(inner.trim().to_string());
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Делит заголовок на (param-region, return-effect-region) по главному
/// списку параметров (первый `(` и его balanced `)`).
fn split_param_return(header: &str) -> (String, String) {
    let chars: Vec<char> = header.chars().collect();
    let Some(open) = chars.iter().position(|&c| c == '(') else {
        return (String::new(), header.to_string());
    };
    let mut depth = 0i32;
    let mut close = open;
    for (k, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = k;
                    break;
                }
            }
            _ => {}
        }
    }
    let params: String = chars[open + 1..close].iter().collect();
    let ret: String = chars[close + 1..].iter().collect();
    (params, ret)
}

// ─────────────────────────────── exempt-list ───────────────────────────────

/// Явный exempt-list из D325 §2 (иначе false-positive) — по (rel-path, name).
fn is_fail_exempt(d: &Decl) -> bool {
    // Q5: весь testing/property — assert/test-DSL семантика. `d.rel` —
    // std-source-root-relative (манифест-derived, Plan 195), без "std/"
    // префикса и независимо от src/-раскладки.
    if d.rel == "testing/property.nv" {
        return true;
    }
    // Мост D85 `!!`: Option@unwrap / Result@unwrap (core.nv).
    if d.rel == "prelude/core.nv" && d.name == "unwrap" {
        return true;
    }
    // Cleanup-ПРОТОКОЛ — exempt-список D325 §2, пункт 2 (Amend-пакет,
    // Ред. 2, sign-off владельца 2026-07-03): protocol-member
    // `@cleanup(...) Fail[E]` легален by-design.
    //
    // ВТОРОЙ ИСТОЧНИК, И ОН НОРМАТИВНЫЙ: D158 (failable cleanup bodies)
    // прямо разрешает эффект `Fail` в теле cleanup. Вернуть `Result`
    // такой метод НЕ МОЖЕТ ПО ПОСТРОЕНИЮ: его зовёт ВЫХОД ИЗ
    // ОБЛАСТИ, принимающего значение нет вовсе. Сам `fs/fs.nv` говорит
    // это заранее: «ошибка закрытия НЕ глотается: `@cleanup` прокидывает
    // её через `Fs Fail[IoError]`». Ссылка на D158 стоит ЗДЕСЬ по требованию
    // приёмки №852 (пункт 1): исключение без нормы, на которую оно
    // опирается, через год читается как послабление «чтобы прошло».
    //
    // ЗДЕСЬ СТОЯЛО `on_exit` — ИМЯ, КОТОРОГО В std НЕТ НИГДЕ
    // (греп 2026-08-31: ноль вхождений). Член протокола давно
    // зовётся `cleanup` (`prelude/protocols.nv:384` —
    // `consume @cleanup(outcome ScopeOutcome) Fail[E] -> ()`), а исключение
    // осталось на прежнем имени — то есть было МЁРТВЫМ, а две честные
    // реализации протокола (`fs/fs.nv`, `io/buffered.nv`) числились
    // нарушениями. Не заметили потому, что этот тест не гоняет ни
    // гейт, ни CI (оба шли с `--lib`) — реестр №852, охват — №723.
    //
    // Судим НЕ по одному имени: `cleanup` мог бы называться и обычный
    // метод. Признак именно ПРОТОКОЛЬНОЙ реализации — параметр
    // `ScopeOutcome`, который задаёт сам протокол: реализация не вольна
    // отклониться от его сигнатуры, а значит и от `Fail[E]` в ней.
    if d.name == "cleanup" && d.header.contains("ScopeOutcome") {
        return true;
    }
    false
}

/// Established `try_`-идиомы БЕЗ одноимённого infallible-сиблинга, но
/// конформные D325: Once non-blocking init («выиграл гонку инициализации» —
/// genuine absence, R4; блокирующего `start` by-design нет).
///
/// `try_exists` — сиблинг `exists` НЕВОЗМОЖЕН ПО ПОСТРОЕНИЮ ЯЗЫКА,
/// а не по чьему-то выбору: `exists` и `forall` — кванторы контрактов (D.1.3),
/// и парсер разбирает их в позиции выражения КАК КВАНТОР
/// (`parser/mod.rs:9614`: `Ident(kw) if kw == "forall" || kw == "exists"`).
/// Функцию с таким именем нельзя было бы даже ВЫЗВАТЬ — сверено чтением
/// парсера, а не принято на веру из комментария.
///
/// Сам исходник это УЖЕ ГОВОРИЛ: над `fs/fs.nv:528` стоит
/// `// nova:allow W_TRY_WITHOUT_SIBLING -- … reserved quantifier keywords …`,
/// но этот тест — сканер по тексту и пометок `nova:allow` не читает,
/// поэтому требовал переименования, НЕВОЗМОЖНОГО в этом языке. Страж,
/// требующий невозможного, снимается первым же окном — вместе со всеми
/// настоящими находками. Реестр 221.1 №852.
const TRY_EXEMPT: &[&str] = &["try_start", "try_start_won", "try_exists"];

// ───────────────────────────────── tests ───────────────────────────────────

#[test]
fn guard_no_own_fail_in_public_std_signatures() {
    let mut violations: Vec<String> = Vec::new();
    let mut exempted_own_fail = 0usize; // non-vacuity счётчик

    for d in all_decls() {
        let (params, ret) = split_param_return(&d.header);
        let ret_fail = fail_error_names(&ret);
        if ret_fail.is_empty() {
            continue; // нет Fail в return/effect-позиции → нечего проверять
        }
        // R5: forwarded, если КАЖДАЯ ошибка из return-региона также несётся
        // Fail[...]-параметром (closure-param). Иначе — СОБСТВЕННАЯ ошибка.
        let param_fail: BTreeSet<String> =
            fail_error_names(&params).into_iter().collect();
        let own: Vec<&String> = ret_fail
            .iter()
            .filter(|e| !param_fail.contains(*e))
            .collect();
        if own.is_empty() {
            continue; // чистый R5-forwarding — легально
        }
        if is_fail_exempt(&d) {
            exempted_own_fail += 1;
            continue; // явный exempt-list §2
        }
        violations.push(format!(
            "{}:{}: public signature carries OWN Fail{:?} (D325 R1: must be \
             Result). name=`{}` header=`{}`",
            d.rel,
            d.line,
            own,
            d.name,
            d.header.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }

    assert!(
        violations.is_empty(),
        "D325 R1 guard: {} public std signature(s) throw their OWN error \
         (must return Result[T, E] instead):\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
    // Non-vacuity: детектор own-Fail ДОЛЖЕН реально срабатывать — известные
    // exempt-сигнатуры (Option@unwrap, Result@unwrap, property.assert_prop*)
    // несут own-Fail и обязаны попасть в exempt-ветку. Иначе скан «молча
    // пуст» (регресс парсинга) — тоже провал.
    assert!(
        exempted_own_fail >= 3,
        "non-vacuity: expected >=3 exempted own-Fail signatures (2×unwrap + \
         property asserts), got {} — scanner likely stopped seeing decls",
        exempted_own_fail
    );
}

#[test]
fn naming_lint_no_opt_suffix_or_orphan_try_prefix() {
    let decls = all_decls();
    // Глобальный набор всех публичных имён (база для сиблинг-матчинга).
    // Глобально (а не per-receiver): infallible `from`/`into` — builtin/
    // protocol-конвертеры (D73/D77), у char/u8/JsonValue нет собственного
    // `.from`-декла; per-receiver дал бы false-positive на `try_from`.
    let names: BTreeSet<String> =
        decls.iter().map(|d| d.name.clone()).collect();

    let mut opt_violations: Vec<String> = Vec::new();
    let mut try_violations: Vec<String> = Vec::new();
    let mut try_seen = 0usize; // non-vacuity счётчик

    for d in &decls {
        if d.name.is_empty() {
            continue;
        }
        // R4: `_opt`-суффикс запрещён (fallibility ≠ absence; Option через .ok()).
        if d.name.ends_with("_opt") {
            opt_violations.push(format!("{}:{}: `{}`", d.rel, d.line, d.name));
        }
        // R3: `try_` — только при одноимённом infallible-сиблинге.
        if let Some(stripped) = d.name.strip_prefix("try_") {
            try_seen += 1;
            if TRY_EXEMPT.contains(&d.name.as_str()) {
                continue;
            }
            // progressive base-strip: `lock_for` → `lock`, `from_codepoint` → `from`.
            let mut base = stripped.to_string();
            let mut has_sibling = false;
            loop {
                if !base.is_empty() && names.contains(&base) {
                    has_sibling = true;
                    break;
                }
                match base.rfind('_') {
                    Some(pos) => base.truncate(pos),
                    None => break,
                }
            }
            if !has_sibling {
                try_violations.push(format!(
                    "{}:{}: `{}` (no infallible sibling `{}` — R3)",
                    d.rel, d.line, d.name, stripped
                ));
            }
        }
    }

    assert!(
        opt_violations.is_empty(),
        "D325 R4 naming-lint: {} public API(s) use `_opt` suffix (use Result + \
         `.ok()`, no `_opt` twin):\n  {}",
        opt_violations.len(),
        opt_violations.join("\n  ")
    );
    assert!(
        try_violations.is_empty(),
        "D325 R3 naming-lint: {} public API(s) use `try_` prefix without an \
         infallible sibling (add exempt only for sanctioned idioms):\n  {}",
        try_violations.len(),
        try_violations.join("\n  ")
    );
    // Non-vacuity: в stable std заведомо есть `try_`-семейство (try_from,
    // try_lock(_for), try_read/write, try_acquire, try_await, try_start…) —
    // если 0, скан «молча пуст» и линт бессмыслен.
    assert!(
        try_seen >= 8,
        "non-vacuity: expected >=8 `try_`-prefixed public APIs, got {} — \
         scanner likely stopped seeing decls",
        try_seen
    );
}

#[test]
fn net_family_has_zero_fail() {
    // std/net — эталон Result-everywhere (0 Fail[ в публичных сигнатурах).
    // `d.rel` — std-source-root-relative (манифест-derived), поэтому
    // префикс — просто `net/`, без "std/" и без src/-раскладки.
    let mut offenders: Vec<String> = Vec::new();
    for d in all_decls() {
        if !d.rel.starts_with("net/") {
            continue;
        }
        let (_params, ret) = split_param_return(&d.header);
        if !fail_error_names(&ret).is_empty() {
            offenders.push(format!("{}:{}: `{}`", d.rel, d.line, d.name));
        }
    }
    assert!(
        offenders.is_empty(),
        "std/net must stay Result-everywhere (0 Fail[ in public signatures), \
         found:\n  {}",
        offenders.join("\n  ")
    );
}
