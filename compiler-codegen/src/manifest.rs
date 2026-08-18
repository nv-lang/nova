//! D78 path/module enforcement + `[dependencies]` (Plan 03.1).
//!
//! Walk parent dirs от файла, ищем `nova.toml`. Из него извлекаем
//! `[package].name`, `[package].edition`, `[lib].enforce-stability` и
//! `[dependencies]`.
//!
//! **Source root = корень пакета** (директория `nova.toml`). D78
//! (2026-05-22): отдельной `src/` и настройки `[lib] src` больше нет;
//! `[lib] src`, если задан в legacy-манифесте, ещё уважается.
//! Expected module = `<package>.<rel-path-from-package-root-without-ext>`.
//!
//! Если файл лежит **вне** source root — пропускаем enforcement (это
//! может быть test, example, scratch — не часть пакета).
//!
//! Минимальный TOML-парсер (без full TOML crate ради bootstrap'а):
//! `key = "..."` по секциям + array-of-tables не нужен (`[dependencies]`
//! — плоская секция `name = <spec>`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Plan 03.1: git-пин зависимости.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitPin {
    Rev(String),
    Tag(String),
    Branch(String),
    /// Plan 03.2: semver-диапазон — версия выбирается среди тегов
    /// репозитория (наибольший подходящий semver-тег).
    Version(crate::semver::VersionReq),
    /// Пин не указан — резолвится в default-ветку (lockfile фиксирует commit).
    Default,
}

/// Plan 03.1: источник внешней зависимости.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepSource {
    /// Локальная path-зависимость: директория другого пакета.
    Path(String),
    /// Git-зависимость; pin — rev/tag/branch.
    Git { url: String, pin: GitPin },
    /// Версия из registry (registry — Plan 03.3; пока не резолвится).
    Registry(String),
    /// Некорректная запись (ни `path`, ни `git`, ни версия) — хранит
    /// сырое значение для диагностики на этапе резолва.
    Invalid(String),
}

/// Plan 03.1: одна запись `[dependencies]`.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub source: DepSource,
    /// Plan 03.4 Ф.3: capability-confined dep — `forbid = ["Net", "Fs"]`.
    /// Запрещённые эффекты: компилятор проверяет, что effect-surface
    /// зависимости их не содержит. Пусто — ограничений нет.
    pub forbid: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub package_name: String,
    pub source_root: PathBuf,
    /// Directory containing `nova.toml` itself (the package root). Usually
    /// identical to `source_root` — they diverge only for a legacy `[lib]
    /// src = "<subdir>"` manifest (D78 back-compat; e.g. `nova-tls`'s
    /// `src = "src"`). `[ffi]` paths are documented (see [`FfiConfig`]) as
    /// relative to **this** directory, not `source_root` — found 2026-07-12
    /// while fixing the `nova-tls` standalone-package D133 regression:
    /// `ResolvedFfiConfig::from_manifest` previously joined against
    /// `source_root`, so `c_shims = ["native/tls_c_shim.c"]` resolved to
    /// `<pkg>/src/native/...` instead of `<pkg>/native/...` for any package
    /// using a non-trivial `[lib] src`.
    pub manifest_dir: PathBuf,
    /// **Plan 62.F.bis Ф.1 (edition versioning, 2026-05-18):**
    /// `[package].edition = "2026.05"` — pin для prelude content. None →
    /// rolling (uses `std/prelude.nv` facade). Some("X.Y") → resolver
    /// проверяет наличие `std/prelude/<sanitized>.nv` (где `.` → `_`)
    /// перед fallback'ом на rolling facade.
    ///
    /// Mirrors Rust's `edition = "2021"` и Go's `go 1.21` — stability
    /// через explicit pin. Безопасно extends prelude content без
    /// breaking existing packages.
    pub edition: Option<String>,
    /// Plan 71 / D127: opt-in строгий enforcement правила
    /// `public-missing-stability` (Plan 45 §11.5 №7).
    ///
    /// Source: `[lib] enforce-stability = true` в `nova.toml`.
    /// Default (если flag не задан) — `false`: lint emit Warning, не
    /// блокирует `nova doc --check`. `true` — Error, exit 1.
    ///
    /// Test/example/bench paths игнорируют этот flag (см.
    /// `doc::lints::LintConfig::fixture_dirs`) — там lint всегда skip'ается.
    pub enforce_stability: bool,
    /// Plan 03.1: внешние зависимости из `[dependencies]`. Пусто, если
    /// секция отсутствует.
    pub dependencies: Vec<Dependency>,
    /// Plan 100.6 (D164 §6): `[exports.consume_types]` — пакетный контракт
    /// на consume-статус типов. Ключ = имя типа, значение = версия контракта
    /// (semver major-string, напр. `"1.0"`). Пусто, если секция отсутствует.
    ///
    /// Семантика: потребители могут полагаться на неизменность consume-статуса
    /// типа в рамках указанной major-версии. Изменение consume-статуса без
    /// major-bump — ABI-break (D164 §2).
    ///
    /// Пример в nova.toml:
    /// ```toml
    /// [exports.consume_types]
    /// Transaction = "1.0"
    /// Resource    = "1.0"
    /// ```
    pub exports_consume_types: HashMap<String, String>,
    /// Plan 115 D214 [M-115-ffi-build-pipeline]: `[ffi]` section — user FFI
    /// build pipeline. Объявляет C shim header'ы, include-каталоги и
    /// system libraries которые передаются clang при сборке тестов и
    /// бинарей этого пакета.
    ///
    /// Все paths относительные к директории `nova.toml`.
    ///
    /// Пример в nova.toml:
    /// ```toml
    /// [ffi]
    /// c_shims      = ["src/sqlite3_shim.c", "src/libpng_shim.c"]
    /// include_dirs = ["src/", "third_party/sqlite3/"]
    /// lib_dirs     = ["third_party/sqlite3/lib/"]
    /// libs         = ["sqlite3", "png"]
    /// ```
    ///
    /// Семантика: `c_shims` — дополнительные `.c` или `.h` файлы для
    /// compilation (header-only inline shims OK); `include_dirs` →
    /// clang `-I` flags; `lib_dirs` (Plan 193 Ф.2 gap-1) → linker
    /// search-directory flags (`-L`/MSVC `/LIBPATH:`) для non-default-path
    /// native libs; `libs` → clang `-l<name>` / MSVC `<name>.lib` flags для
    /// linking. `lib_dirs` пустой (не задан) → только default toolchain
    /// search path (как раньше).
    ///
    /// Detect-and-degrade (Plan 193 Ф.2 gap-1): когда `lib_dirs` задан явно,
    /// но объявленный `libs`-файл не найден ни в одной из директорий —
    /// test_runner деградирует ЭТОТ пакет к SKIP («lib not found»), а не
    /// hard CC/link-FAIL (мирроринг retired built-in
    /// MbedtlsConfig/BrotliConfig graceful-degrade контракта, обобщённый
    /// для generic `[ffi] libs`).
    ///
    /// Пусто (None), если секция отсутствует.
    pub ffi: Option<FfiConfig>,
    /// Plan 149 D233: `[runtime]` section — fiber arena tuning baked as
    /// compile-time defaults (-DNOVA_FIBER_STACK_DEFAULT / -DNOVA_FIBERS_PER_WORKER_DEFAULT).
    /// Precedence env > nova.toml(-D) > builtin #define. None если секция
    /// отсутствует.
    pub runtime: Option<RuntimeConfig>,
    /// Plan 204: `[replace]` section — dev-override источника зависимости,
    /// объявленной в `[dependencies]` (school Go `replace`-директивы /
    /// Cargo `[patch]`). Ключ — имя зависимости (должно совпадать с
    /// записью в `[dependencies]`); значение — источник, который
    /// РЕАЛЬНО резолвится вместо декларированного (обычно `path = "../..."`
    /// поверх релизной `{ git = "...", version = "..." }` формы).
    ///
    /// Разделение «что требуется» (`[dependencies]`) / «откуда взять
    /// СЕЙЧАС» (`[replace]`) — go-школа (D-block Q-dependency-versioning).
    /// Пусто, если секция отсутствует. См. [`Manifest::effective_source`].
    ///
    /// **Plan 204 дофикс №2 / Plan 233 §2а (переименование):** объединяет
    /// ДВА источника — `[replace]` из самого `nova.toml` (закоммиченный,
    /// см. `replace_in_committed_manifest`) и `[replace]` из необязательного
    /// соседнего override-файла (машино-локальный, не коммитится — новое
    /// имя `nova.override.toml`, legacy `nova.local.toml` читается тоже, с
    /// deprecation warning, см. `override_legacy_name_used`). Override-файл
    /// побеждает при совпадении ключа (более специфичный, машино-локальный
    /// оверрайд). Само по себе это поле НЕ учитывает go-scope (корень vs
    /// зависимость) — это делает `Manifest::effective_source`'s caller
    /// (`imports::lookup_dependency`), консультируя его ТОЛЬКО когда
    /// текущий манифест — корень собираемого дерева.
    pub replace: HashMap<String, DepSource>,
    /// **Plan 204 дофикс №2:** `true`, если `[replace]` объявлен
    /// непосредственно в ЭТОМ `nova.toml` (закоммиченном файле) — а не
    /// только в соседнем override-файле. Триггерит `W_REPLACE_IN_MANIFEST`
    /// (`manifest_warnings`): закоммиченный `[replace]` ломает чистый клон
    /// (путь, валидный на машине автора, отсутствует у клонирующего).
    pub replace_in_committed_manifest: bool,
    /// **Plan 204 дофикс №2 / Plan 233 §2а:** секции/ключи override-файла,
    /// отличные от `[replace]` — эта волна поддерживает в нём ТОЛЬКО
    /// `[replace]`. Каждая запись — метка вида `"section"` (секция целиком)
    /// или `"section.key"` (конкретный ключ) для diagnostic message.
    /// Пусто, если override-файл отсутствует либо полностью валиден.
    /// Триггерит `W_OVERRIDE_TOML_UNSUPPORTED_KEY`.
    pub override_toml_unsupported: Vec<String>,
    /// **Plan 233 §2а:** `true`, если override-данные (`[replace]` и/или
    /// `override_toml_unsupported`) взяты из LEGACY-имени `nova.local.toml`
    /// — новое имя `nova.override.toml` в той же директории ОТСУТСТВОВАЛО.
    /// Триггерит `W_OVERRIDE_TOML_DEPRECATED` (`manifest_warnings`). `false`,
    /// если override-файла нет вовсе, либо присутствует новое имя.
    pub override_legacy_name_used: bool,
}

/// Plan 149 D233: `[runtime]` section config — fiber arena tuning.
///
/// `fiber_stack` — per-fiber stack slot size (human-friendly `"4MB"` или bare
/// bytes `"4194304"`). `fibers_per_worker` — max concurrent fibers per worker
/// (`"16384"`). Both baked as compile-time `-D...DEFAULT` flags; the
/// corresponding env var (NOVA_FIBER_STACK / NOVA_FIBERS_PER_WORKER) overrides at
/// runtime. Stored as raw strings; `parse_size_to_bytes` converts to the raw
/// integer the C `#define` consumes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// "4MB" | "4194304" — per-fiber stack slot size.
    pub fiber_stack: Option<String>,
    /// "16384" — max concurrent fibers per worker.
    pub fibers_per_worker: Option<String>,
}

/// Plan 115 D214 [M-115-ffi-build-pipeline]: `[ffi]` section config.
///
/// Все пути относительные к директории `nova.toml`. Test_runner +
/// build pipeline резолвят их в absolute paths перед передачей clang.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FfiConfig {
    /// Список C / header файлов для compilation. Header-only inline shim'ы
    /// (как `nova_rt/sqlite_mini_ffi.h`) включаются через `#include`,
    /// .c файлы compilation units компилируются и линкуются.
    pub c_shims: Vec<String>,
    /// Include directories для clang `-I`. Дают доступ к user shim header'ам
    /// и third-party C library headers.
    pub include_dirs: Vec<String>,
    /// Plan 193 Ф.2 gap-1: library search directories для linker `-L`
    /// (Clang/GCC) / `/LIBPATH:` (MSVC) — non-default-path native libs
    /// (напр. vcpkg install без system-wide регистрации). Пусто → только
    /// toolchain default search path (Windows: нет системного аналога
    /// `/usr/lib` — без `lib_dirs` non-default `.lib` не найдётся).
    pub lib_dirs: Vec<String>,
    /// System library names для clang `-l<name>` linking. Например
    /// `libs = ["sqlite3", "png"]` → `-lsqlite3 -lpng`.
    pub libs: Vec<String>,
    /// Plan 193 Ф.2 gate-3 (mbedtls-vendored, 2026-07-12): vendored C source
    /// directories to build-and-cache when a declared `libs` entry is
    /// missing from `lib_dirs` — generic "195-pattern" extension of the
    /// monorepo's libuv one-time-build-and-cache precedent
    /// (`detect_or_build_libuv` in `test_runner.rs`), so ANY native module
    /// can vendor upstream C sources instead of requiring a prebuilt
    /// system/vcpkg lib. All `.c` files directly under each declared dir
    /// (non-recursive — matches typical upstream `library/`-style flat
    /// layouts, e.g. mbedTLS) are compiled + archived into `lib_dirs[0]`
    /// under EVERY name declared in `libs` (see
    /// `test_runner::build_missing_vendor_ffi_libs`). Empty (default) — no
    /// vendor build attempted, unchanged legacy behaviour (falls through to
    /// the existing `first_missing_ffi_lib` detect-and-degrade probe).
    pub vendor_src_dirs: Vec<String>,
}

/// Plan 03.1 / 03.4: quote- и bracket-aware разбор тела inline-таблицы
/// TOML (`key = "v", key2 = ["a", "b"]`) — запятая внутри `"..."` либо
/// `[...]` не разделяет поля.
fn parse_inline_table(body: &str) -> Vec<(String, String)> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut depth: i32 = 0; // вложенность `[ ]` (массив-значение)
    for ch in body.chars() {
        match ch {
            '"' => { in_str = !in_str; cur.push(ch); }
            '[' if !in_str => { depth += 1; cur.push(ch); }
            ']' if !in_str => { depth -= 1; cur.push(ch); }
            ',' if !in_str && depth == 0 => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    parts.iter()
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            let k = k.trim();
            if k.is_empty() { return None; }
            Some((k.to_string(), v.trim().trim_matches('"').to_string()))
        })
        .collect()
}

/// Plan 03.1: разобрать значение записи `[dependencies]`.
/// `"1.2"` → Registry; `{ path = "..." }` → Path; `{ git = "...", tag/rev/branch }`
/// → Git; иначе → Invalid (диагностируется при резолве).
fn parse_dep_source(raw_val: &str) -> DepSource {
    let v = raw_val.trim();
    if let Some(inner) = v.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let fields = parse_inline_table(inner.trim());
        let get = |k: &str| fields.iter().find(|(fk, _)| fk == k).map(|(_, fv)| fv.clone());
        if let Some(p) = get("path") {
            DepSource::Path(p)
        } else if let Some(url) = get("git") {
            // Plan 03.2: пины rev/tag/branch/version взаимоисключающи.
            let pin_count = ["rev", "tag", "branch", "version"]
                .iter()
                .filter(|k| get(k).is_some())
                .count();
            if pin_count > 1 {
                return DepSource::Invalid(format!(
                    "git-зависимость: пины rev/tag/branch/version \
                     взаимоисключающи (указано {})",
                    pin_count,
                ));
            }
            let pin = if let Some(r) = get("rev") {
                GitPin::Rev(r)
            } else if let Some(t) = get("tag") {
                GitPin::Tag(t)
            } else if let Some(b) = get("branch") {
                GitPin::Branch(b)
            } else if let Some(vr) = get("version") {
                // Plan 03.2: semver-диапазон по тегам репозитория.
                match crate::semver::VersionReq::parse(&vr) {
                    Ok(req) => GitPin::Version(req),
                    Err(e) => {
                        return DepSource::Invalid(format!(
                            "git-зависимость: некорректный version `{}`: {}",
                            vr, e,
                        ))
                    }
                }
            } else {
                GitPin::Default
            };
            DepSource::Git { url, pin }
        } else {
            DepSource::Invalid(v.to_string())
        }
    } else {
        let ver = v.trim_matches('"').to_string();
        if ver.is_empty() {
            DepSource::Invalid(v.to_string())
        } else {
            DepSource::Registry(ver)
        }
    }
}

/// Plan 03.4 Ф.3: разобрать `forbid = ["Net", "Fs"]` из inline-таблицы
/// зависимости. Пусто, если поля нет либо запись — не inline-таблица.
fn parse_dep_forbid(raw_val: &str) -> Vec<String> {
    let v = raw_val.trim();
    let Some(inner) = v.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
        return Vec::new();
    };
    let fields = parse_inline_table(inner.trim());
    let Some((_, arr)) = fields.iter().find(|(k, _)| k == "forbid") else {
        return Vec::new();
    };
    let arr = arr.trim();
    let Some(items) = arr.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return Vec::new();
    };
    items
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Plan 204 дофикс №2 / Plan 233 §2а: разобрать override-файл
/// (`nova.override.toml` — новое имя, либо legacy `nova.local.toml`) —
/// необязательный, НЕ коммитящийся файл рядом с `nova.toml` для
/// машино-локальных оверрайдов. В этой волне поддержана ТОЛЬКО секция
/// `[replace]` (тот же формат записи, что и `[replace]` в `nova.toml`).
/// Прочие секции/ключи не отклоняют парсинг (forward-compat — будущие
/// волны могут добавить поддержанные ключи без breaking change) — но
/// собираются как `unsupported`-метки для `W_OVERRIDE_TOML_UNSUPPORTED_KEY`.
///
/// Returns `(replace_map, unsupported_labels)`. Файл отсутствует/пуст/не
/// читается → `(HashMap::new(), Vec::new())`.
fn parse_override_toml(path: &Path) -> (HashMap<String, DepSource>, Vec<String>) {
    let mut replace: HashMap<String, DepSource> = HashMap::new();
    let mut unsupported: Vec<String> = Vec::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return (replace, unsupported);
    };
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_start_matches('[').trim_end_matches(']').trim().to_string();
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let raw_val = val.trim();
            let raw_val = raw_val.split('#').next().unwrap_or("").trim();
            if section == "replace" {
                replace.insert(key.to_string(), parse_dep_source(raw_val));
            } else {
                // Key outside `[replace]` — either no section header seen
                // yet (top-level key) or an already-flagged unsupported
                // section. Record `section.key` (or `<top-level>.key`) so
                // the diagnostic can point at the exact offending line.
                let label = if section.is_empty() {
                    format!("<top-level>.{}", key)
                } else {
                    format!("{}.{}", section, key)
                };
                unsupported.push(label);
            }
        }
    }
    (replace, unsupported)
}

/// Plan 03.1 Ф.4: директория ближайшего вверх по дереву `nova.toml` —
/// корень пакета, которому принадлежит `file`. `None` — файл не входит
/// ни в один пакет.
pub fn find_package_dir(file: &Path) -> Option<PathBuf> {
    let abs = crate::source_index::canonicalize(file)?;
    let mut dir = abs.parent()?.to_path_buf();
    loop {
        if crate::source_index::is_file(&dir.join("nova.toml")) {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Найти nova.toml в parent dirs и извлечь package_name + source_root.
/// Возвращает None если nova.toml не найден ни в одной parent dir
/// (значит файл не часть пакета — без enforcement).
pub fn find_manifest(file: &Path) -> Option<Manifest> {
    // План 252 Ф.2 шаг 4: манифест ищется ОДИН раз и лежит рядом с индексом.
    // `find_manifest` звался на каждый peer каждого импорта
    // (`imports-prelude-compute` = 124 с на корпусе `neg`); ответ зависит
    // только от каталога, в котором лежит файл.
    let abs = crate::source_index::canonicalize(file)?;
    let dir0 = abs.parent()?.to_path_buf();
    crate::source_index::derived_for_path(&dir0, "find-manifest", || {
        let mut dir = dir0.clone();
        loop {
            let toml = dir.join("nova.toml");
            if crate::source_index::is_file(&toml) {
                return parse_manifest(&toml, &dir);
            }
            if !dir.pop() {
                return None;
            }
        }
    })
    .as_ref()
    .clone()
}

/// Parse a `nova.toml` directly from `toml_path`, with `dir` as the
/// manifest-relative source-root anchor. Public for use from
/// `nova-cli::build_lint_config_for` fallback path и в integration
/// tests (Plan 71 Ф.1 / Ф.5).
/// Plan 115 D214 [M-115-ffi-build-pipeline]: parse TOML array of strings
/// `["a.c", "b.c", "c.c"]`. Quote-aware; trims whitespace и outer
/// double-quotes. Returns empty vec для invalid input.
fn parse_toml_string_array(raw_val: &str) -> Vec<String> {
    let v = raw_val.trim();
    let inner = match v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in inner.chars() {
        match ch {
            '"' => { in_str = !in_str; cur.push(ch); }
            ',' if !in_str => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    parts.iter()
        .map(|p| p.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// План 252 Ф.2 шаг 4: разбор `nova.toml` — по разу на манифест за прогон.
/// Ответ зависит только от содержимого файла и каталога-якоря, а дерево в
/// пределах прогона неизменно по построению (см. `source_index`).
pub fn parse_manifest(toml_path: &Path, dir: &Path) -> Option<Manifest> {
    if !crate::source_index::snapshot_enabled() {
        return parse_manifest_uncached(toml_path, dir);
    }
    let key = toml_path.to_path_buf();
    if let Ok(g) = manifest_cache().lock() {
        if let Some(v) = g.get(&key) {
            if v.0 == dir {
                return v.1.as_ref().clone();
            }
        }
    }
    let parsed = parse_manifest_uncached(toml_path, dir);
    if let Ok(mut g) = manifest_cache().lock() {
        g.insert(key, (dir.to_path_buf(), std::sync::Arc::new(parsed.clone())));
    }
    parsed
}

type ManifestCache = HashMap<PathBuf, (PathBuf, std::sync::Arc<Option<Manifest>>)>;

fn manifest_cache() -> &'static std::sync::Mutex<ManifestCache> {
    static C: std::sync::OnceLock<std::sync::Mutex<ManifestCache>> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// Сбросить разобранные манифесты. Симметрично `source_index::reset`.
pub fn reset_manifest_cache() {
    if let Ok(mut g) = manifest_cache().lock() {
        g.clear();
    }
}

fn parse_manifest_uncached(toml_path: &Path, dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(toml_path).ok()?;
    let mut package_name: Option<String> = None;
    let mut lib_src: Option<String> = None;
    let mut edition: Option<String> = None;
    let mut enforce_stability: bool = false;
    let mut dependencies: Vec<Dependency> = Vec::new();
    // Plan 204: [replace] — dev-override источника (см. Manifest::replace).
    let mut replace: HashMap<String, DepSource> = HashMap::new();
    // Plan 100.6 (D164 §6): [exports.consume_types] — type_name → version_contract.
    let mut exports_consume_types: HashMap<String, String> = HashMap::new();
    // Plan 115 D214 [M-115-ffi-build-pipeline]: [ffi] config.
    let mut ffi_c_shims: Vec<String> = Vec::new();
    let mut ffi_include_dirs: Vec<String> = Vec::new();
    let mut ffi_lib_dirs: Vec<String> = Vec::new();
    let mut ffi_libs: Vec<String> = Vec::new();
    // Plan 193 Ф.2 gate-3: [ffi] vendor_src_dirs — vendored C sources for
    // generic build-and-cache (see FfiConfig::vendor_src_dirs doc-comment).
    let mut ffi_vendor_src_dirs: Vec<String> = Vec::new();
    let mut ffi_section_seen: bool = false;
    // Plan 149 D233: [runtime] config.
    let mut runtime_fiber_stack: Option<String> = None;
    let mut runtime_fibers_per_worker: Option<String> = None;
    let mut runtime_section_seen: bool = false;
    // Section tracking: use String to support "exports.consume_types".
    let mut section = String::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            // [section] or [[section]] — strip all leading/trailing `[` `]`.
            let inner = line.trim_start_matches('[').trim_end_matches(']').trim();
            section = match inner {
                "package"              => "package",
                "lib"                  => "lib",
                "dependencies"         => "dependencies",
                "replace"              => "replace",
                "exports.consume_types" => "exports.consume_types",
                "ffi"                  => { ffi_section_seen = true; "ffi" }
                "runtime"              => { runtime_section_seen = true; "runtime" }
                _                      => "",  // ignore other sections
            }.to_string();
            continue;
        }
        // key = "value" or key = bool — minimal parsing.
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let raw_val = val.trim();
            // Strip trailing inline comment ` # ...`. TOML allows `key = true # comment`.
            let raw_val = raw_val.split('#').next().unwrap_or("").trim();
            let str_val = raw_val.trim_matches('"').to_string();
            // Plan 03.1: [dependencies] — key = имя зависимости, val =
            // "version" | { path = "..." } | { git = "...", rev/tag/branch }.
            if section == "dependencies" {
                dependencies.push(Dependency {
                    name: key.to_string(),
                    source: parse_dep_source(raw_val),
                    forbid: parse_dep_forbid(raw_val),
                });
                continue;
            }
            // Plan 204: [replace] — key = имя зависимости, val = источник
            // (обычно `{ path = "..." }`), формат тот же, что и записи
            // `[dependencies]`.
            if section == "replace" {
                replace.insert(key.to_string(), parse_dep_source(raw_val));
                continue;
            }
            // Plan 100.6 (D164 §6): [exports.consume_types] — type_name = "version".
            if section == "exports.consume_types" {
                exports_consume_types.insert(key.to_string(), str_val);
                continue;
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: [ffi] section.
            if section == "ffi" {
                match key {
                    "c_shims"      => ffi_c_shims = parse_toml_string_array(raw_val),
                    "include_dirs" => ffi_include_dirs = parse_toml_string_array(raw_val),
                    "lib_dirs"     => ffi_lib_dirs = parse_toml_string_array(raw_val),
                    "libs"         => ffi_libs = parse_toml_string_array(raw_val),
                    "vendor_src_dirs" => ffi_vendor_src_dirs = parse_toml_string_array(raw_val),
                    _ => {} // ignore unknown keys для forward-compat
                }
                continue;
            }
            // Plan 149 D233: [runtime] section — fiber arena tuning.
            if section == "runtime" {
                match key {
                    "fiber_stack" => runtime_fiber_stack = Some(str_val),
                    "fibers_per_worker"  => runtime_fibers_per_worker = Some(str_val),
                    _ => {} // ignore unknown keys для forward-compat
                }
                continue;
            }
            match (section.as_str(), key) {
                ("package", "name") => package_name = Some(str_val),
                // Plan 62.F.bis Ф.1: `[package].edition = "2026.05"` pin
                // для prelude content. Опционально — отсутствие → rolling.
                ("package", "edition") => edition = Some(str_val),
                ("lib", "src")      => lib_src = Some(str_val),
                // Plan 71 / D127: `[lib] enforce-stability = true|false`.
                // Conservative: anything other than literal `true` → false.
                // Malformed value (e.g. `"garbage"`, `42`) silently → false
                // (acceptance test Ф.1 №3 — `enforce-stability = "garbage"` ignored).
                ("lib", "enforce-stability") => {
                    enforce_stability = raw_val == "true";
                }
                _ => {}
            }
        }
    }

    let pkg = package_name?;
    // D78 (2026-05-22): source root = корень пакета. Отдельной `src/`
    // и настройки `[lib] src` больше нет — default `.`. `[lib] src`,
    // если задан в legacy-манифесте, ещё уважается (back-compat).
    let src_subdir = lib_src.unwrap_or_else(|| ".".to_string());
    let source_root = if src_subdir == "." {
        dir.to_path_buf()
    } else {
        dir.join(src_subdir)
    };
    // Plan 115 D214 [M-115-ffi-build-pipeline]: assemble FfiConfig only если
    // секция [ffi] явно присутствует (даже с пустыми arrays — explicit
    // intent сигнализирует "FFI-aware package но shim'ы ещё не declared").
    let ffi = if ffi_section_seen {
        Some(FfiConfig {
            c_shims: ffi_c_shims,
            include_dirs: ffi_include_dirs,
            lib_dirs: ffi_lib_dirs,
            libs: ffi_libs,
            vendor_src_dirs: ffi_vendor_src_dirs,
        })
    } else {
        None
    };
    // Plan 149 D233: assemble RuntimeConfig only если секция [runtime] явно
    // присутствует. Unknown/garbage values resolved later (build warning +
    // skip -D → builtin fallback).
    let runtime = if runtime_section_seen {
        Some(RuntimeConfig {
            fiber_stack: runtime_fiber_stack,
            fibers_per_worker: runtime_fibers_per_worker,
        })
    } else {
        None
    };
    // Plan 204 дофикс №2: `[replace]` объявленный ПРЯМО в этом (закоммиченном)
    // nova.toml — до слияния с override-файлом, для W_REPLACE_IN_MANIFEST.
    let replace_in_committed_manifest = !replace.is_empty();
    // Plan 204 дофикс №2 / Plan 233 §2а: соседний override-файл (та же
    // директория, что и toml_path) — необязательный, машино-локальный, НЕ
    // коммитится. Новое имя `nova.override.toml` проверяется первым; если
    // отсутствует — legacy `nova.local.toml` (deprecation warning через
    // `override_legacy_name_used` → `manifest_warnings`). `[replace]` из
    // него сливается поверх committed [replace] (побеждает при совпадении
    // ключа — более специфичный, machine-local override).
    let override_toml_path = dir.join("nova.override.toml");
    let legacy_override_toml_path = dir.join("nova.local.toml");
    let mut override_toml_unsupported: Vec<String> = Vec::new();
    let mut override_legacy_name_used = false;
    let effective_override_path: Option<PathBuf> = if crate::source_index::is_file(&override_toml_path) {
        Some(override_toml_path)
    } else if crate::source_index::is_file(&legacy_override_toml_path) {
        override_legacy_name_used = true;
        Some(legacy_override_toml_path)
    } else {
        None
    };
    if let Some(override_path) = &effective_override_path {
        let (override_replace, unsupported) = parse_override_toml(override_path);
        override_toml_unsupported = unsupported;
        for (k, v) in override_replace {
            replace.insert(k, v);
        }
    }
    Some(Manifest {
        package_name: pkg,
        source_root,
        manifest_dir: dir.to_path_buf(),
        edition,
        enforce_stability,
        dependencies,
        exports_consume_types,
        ffi,
        runtime,
        replace,
        replace_in_committed_manifest,
        override_toml_unsupported,
        override_legacy_name_used,
    })
}

impl Manifest {
    /// Plan 204: эффективный источник зависимости `dep` — `[replace]`
    /// override, если объявлен под её именем, иначе декларированный
    /// `dep.source` без изменений. Единая точка резолва для всех
    /// потребителей (`imports.rs` module-path resolution, `lockfile.rs`
    /// dep-graph walk) — добавление/снятие override не требует правок
    /// на call-сайтах.
    pub fn effective_source(&self, dep: &Dependency) -> DepSource {
        self.replace.get(&dep.name).cloned().unwrap_or_else(|| dep.source.clone())
    }
}

/// Plan 204: диагностики манифеста по dependency-versioning схеме.
/// Не фатальны (warning) — публикуемая форма (`git` + `version`) станет
/// обязательной отдельным будущим ужесточением (после появления `nova
/// publish`), сейчас существующий corpus (path-only deps, Plan 202/203)
/// не должен ломаться.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestWarning {
    pub code: &'static str,
    pub message: String,
}

/// Plan 204 дофикс №2 (owner correction): найти корень git-репозитория —
/// ближайший вверх по дереву каталог с `.git` (файл ИЛИ директория —
/// покрывает и обычный репозиторий, и git-worktree, где `.git` внутри
/// worktree-директории — ФАЙЛ с указателем на реальный gitdir). Работает и
/// для ещё НЕСУЩЕСТВУЮЩЕГО `dir` (path-зависимость может указывать на
/// каталог, которого ещё нет) — поднимается сперва до ближайшего
/// существующего предка.
///
/// Используется, чтобы отличить path-зависимость, остающуюся ВНУТРИ той же
/// git-репы (workspace-член, вложенный тест-пакет — clone-safe, `git clone`
/// подтягивает её вместе с остальным деревом), от path-зависимости,
/// выходящей ЗА границу репозитория (сосед-репозиторий типа `../nova-tls` —
/// НЕ материализуется чистым клоном, нужна релизная git+version форма).
pub fn git_repo_root(dir: &Path) -> Option<PathBuf> {
    let mut d = dir.to_path_buf();
    while !d.exists() {
        if !d.pop() {
            return None;
        }
    }
    let mut d = d.canonicalize().unwrap_or(d);
    loop {
        if d.join(".git").exists() {
            return Some(d);
        }
        if !d.pop() {
            return None;
        }
    }
}

/// Plan 204 дофикс №2 (owner correction) / Plan 233 §2а (переименование):
/// `[replace]` объявленный ПРЯМО в закоммиченном `nova.toml` — ЖЁСТКАЯ
/// ОШИБКА (не warning), без периода депрекейшна: закоммиченный `[replace]`
/// ломает чистый клон, если override-путь существует только на машине
/// автора манифеста. `[replace]` разрешён ИСКЛЮЧИТЕЛЬНО в соседнем
/// override-файле — новое имя `nova.override.toml` (legacy `nova.local.toml`
/// тоже читается, см. `parse_override_toml`).
pub fn check_no_committed_replace(m: &Manifest, toml_path: &Path) -> Result<(), String> {
    if m.replace_in_committed_manifest {
        return Err(format!(
            "[E_REPLACE_IN_MANIFEST] [replace] объявлен прямо в {} \
             (закоммиченный файл) — запрещено\n  \
             fix: перенеси секцию [replace] в nova.override.toml рядом \
             (не коммитится — добавь nova.override.toml в .gitignore); \
             закоммиченный [replace] ломает чистый клон, если override-путь \
             существует только на твоей машине",
            toml_path.display(),
        ));
    }
    Ok(())
}

/// `E_DEP_PATH_OUTSIDE_REPO` — ЖЁСТКАЯ ошибка: зависимость объявлена голым
/// `path`, ведущим ЗА границу git-репозитория манифеста.
///
/// **Почему ошибка, а не warning (решение владельца 2026-08-08, реестр 221.1
/// №444).** Правило было и раньше — как `W_DEP_PATH_NO_RELEASE` в
/// `manifest_warnings` ниже, с верным условием и точной подсказкой. Оно честно
/// печаталось при КАЖДОЙ сборке… и месяцами пролистывалось в потоке вывода:
/// `examples/nova.toml` держал `http = { path = "../../nova-http" }` и
/// `polaris = { path = "../../nova-polaris" }`, из-за чего шаг «Flagship
/// examples gate» на CI падал сообщением «резолюция зависимостей: зависимость
/// polaris: path ../../nova-polaris», и этот красный маскировал всё остальное.
/// Предупреждение, которое никто не читает, защитой не является.
///
/// Условие то же, что у warning'а, и оно НЕ стилистическое: путь, выходящий за
/// границу репозитория, на чистом клоне не разрешится НИКОГДА — ни на CI, ни у
/// пользователя. Это состояние «собрать невозможно», а не «оформлено не так».
///
/// Путь ВНУТРИ той же репы (workspace-член, вложенный тест-пакет) законен и
/// ошибкой не считается — `git clone` приносит его вместе с манифестом.
/// `path` в `[replace]` (только в НЕкоммитящемся `nova.override.toml`) законен
/// по D420 и сюда не попадает: `[replace]` в коммитящемся манифесте ловит
/// `check_no_committed_replace` выше.
/// №727: почему зависимость сочтена «не из этой репы» — человеческим языком и
/// БЕЗ вранья.
///
/// Предикат ниже схлопывает ТРИ разных случая в один `false`, и до 2026-08-18
/// все три объяснялись одной фразой «путь выходит за границу git-репозитория».
/// Для проекта, который вообще не под git, это утверждение ЛОЖНО: границы,
/// которую якобы пересекли, там не существует. Диагностика, говорящая больше,
/// чем проверила, отправляет читателя искать поломку, которой нет.
///
/// САМО ПРАВИЛО НЕ ТРОГАЕТСЯ, и это проверено, а не предположено: на рабочей
/// области вне git с ВЛОЖЕННОЙ зависимостью `check` выходит нулём с одним
/// предупреждением, а `build` и `test` про зависимости молчат — жёсткий отказ
/// не воспроизводится ни на одной из трёх команд. Значит менять политику D420
/// не за чем; неверна фраза, и меняется она.
fn cross_repo_reason(manifest_dir: &Path, dep_dir: &Path) -> &'static str {
    match (git_repo_root(manifest_dir), git_repo_root(dep_dir)) {
        (Some(_), Some(_)) => {
            "путь ведёт в СОСЕДНИЙ git-репозиторий, на чистом клоне он не разрешится"
        }
        (None, None) => {
            "ни манифест, ни цель не под git — подтвердить, что путь придёт вместе с клоном, нечем"
        }
        (Some(_), None) => {
            "цель не под git — подтвердить, что она придёт вместе с клоном манифеста, нечем"
        }
        (None, Some(_)) => {
            "манифест не под git — подтвердить, что цель придёт вместе с ним, нечем"
        }
    }
}

pub fn check_no_cross_repo_path_deps(m: &Manifest, toml_path: &Path) -> Result<(), String> {
    for d in &m.dependencies {
        if let DepSource::Path(rel) = &d.source {
            let dep_dir = m.manifest_dir.join(rel);
            let same_repo = match (git_repo_root(&m.manifest_dir), git_repo_root(&dep_dir)) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            if !same_repo {
                return Err(format!(
                    "[E_DEP_PATH_OUTSIDE_REPO] зависимость `{}` объявлена голым \
                     `path = \"{}\"` в [dependencies] ({}) — {}\n  \
                     fix: релизная форма — `{} = {{ git = \"...\", version = \
                     \"x.y\" }}` в [dependencies]; локальный путь — в \
                     `[replace] {} = {{ path = \"{}\" }}` внутри nova.override.toml \
                     (не коммитится). См. D420 (spec/decisions/09-tooling.md).",
                    d.name, rel, toml_path.display(),
                    cross_repo_reason(&m.manifest_dir, &dep_dir),
                    d.name, d.name, rel,
                ));
            }
        }
    }
    Ok(())
}

/// Собрать warnings по манифесту `m` (путь к нему — `toml_path`, только
/// для сообщения). Правила:
///   - `W_DEP_PATH_NO_RELEASE`: зависимость объявлена ГОЛЫМ `path = "..."`
///     непосредственно в `[dependencies]` — нет публикуемого источника
///     (git+version). Рекомендация: релизная форма в `[dependencies]` +
///     `path` вынести в `[replace]` для локальной разработки.
///     **Owner correction:** НЕ срабатывает, если целевой путь остаётся
///     ВНУТРИ той же git-репы, что и сам манифест (workspace-член,
///     вложенный тест-пакет — `git clone` уже приносит его; см.
///     `git_repo_root`). Срабатывает только когда путь выходит за границу
///     репозитория (сосед-репозиторий).
///   - `W_REPLACE_UNKNOWN_DEP`: `[replace]` ссылается на имя, которого нет
///     в `[dependencies]` — нечего заменять (typo / забытый dependency-entry).
///   - `W_OVERRIDE_TOML_UNSUPPORTED_KEY` (Plan 204 дофикс №2 / Plan 233
///     §2а): соседний override-файл содержит секцию/ключ, отличные от
///     `[replace]` — эта волна поддерживает в нём ТОЛЬКО `[replace]`.
///   - `W_OVERRIDE_TOML_DEPRECATED` (Plan 233 §2а): override-данные взяты
///     из LEGACY-имени `nova.local.toml` — рекомендация переименовать в
///     `nova.override.toml`.
///
/// **`[replace]` в закоммиченном `nova.toml` — см. `check_no_committed_replace`
/// (жёсткая ошибка, не warning, вызывается отдельно ДО этой функции).**
pub fn manifest_warnings(m: &Manifest, toml_path: &Path) -> Vec<ManifestWarning> {
    let mut out = Vec::new();
    for d in &m.dependencies {
        if let DepSource::Path(rel) = &d.source {
            let dep_dir = m.manifest_dir.join(rel);
            let same_repo = match (git_repo_root(&m.manifest_dir), git_repo_root(&dep_dir)) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            if !same_repo {
                out.push(ManifestWarning {
                    code: "W_DEP_PATH_NO_RELEASE",
                    message: format!(
                        "зависимость `{}` объявлена голым `path` в [dependencies] \
                         ({}) — {}; публикуемого источника (версия/git) нет\n    \
                         подсказка: релизная форма — `{} = {{ git = \"...\", \
                         version = \"x.y\" }}` в [dependencies], а `path` — в \
                         `[replace] {} = {{ path = \"...\" }}` (nova.override.toml) \
                         для локальной разработки",
                        d.name, toml_path.display(),
                        cross_repo_reason(&m.manifest_dir, &dep_dir),
                        d.name, d.name,
                    ),
                });
            }
        }
    }
    for name in m.replace.keys() {
        if !m.dependencies.iter().any(|d| &d.name == name) {
            out.push(ManifestWarning {
                code: "W_REPLACE_UNKNOWN_DEP",
                message: format!(
                    "[replace] `{}` не соответствует ни одной записи \
                     [dependencies] ({}) — нечего заменять",
                    name, toml_path.display(),
                ),
            });
        }
    }
    // Plan 233 §2а: имя override-файла, ФАКТИЧЕСКИ использованного при
    // резолве (для точных путей в diagnostic message) — legacy, если
    // `override_legacy_name_used`, иначе новое каноническое имя.
    let override_file_name = if m.override_legacy_name_used { "nova.local.toml" } else { "nova.override.toml" };
    if m.override_legacy_name_used {
        let legacy_path = toml_path.parent()
            .map(|d| d.join("nova.local.toml"))
            .unwrap_or_else(|| PathBuf::from("nova.local.toml"));
        out.push(ManifestWarning {
            code: "W_OVERRIDE_TOML_DEPRECATED",
            message: format!(
                "{} устарел, переименуйте в nova.override.toml",
                legacy_path.display(),
            ),
        });
    }
    if !m.override_toml_unsupported.is_empty() {
        let override_path = toml_path.parent()
            .map(|d| d.join(override_file_name))
            .unwrap_or_else(|| PathBuf::from(override_file_name));
        for label in &m.override_toml_unsupported {
            out.push(ManifestWarning {
                code: "W_OVERRIDE_TOML_UNSUPPORTED_KEY",
                message: format!(
                    "{}: неподдерживаемый ключ/секция `{}` — {} \
                     поддерживает в этой волне ТОЛЬКО [replace]",
                    override_path.display(), label, override_file_name,
                ),
            });
        }
    }
    out
}

/// Plan 149 D233: parse a human-friendly size/count string into a raw integer
/// (bytes for stack, count for fibers) — the value baked into a C `#define`.
///
/// Mirrors the C `_nova_parse_size_env` parser: bare integer, or KB/K/MB/M/GB/G
/// suffix (case-insensitive, binary: KB=1024, MB=1024², GB=1024³). Returns
/// `None` on garbage (caller emits a build warning and SKIPS the -D, falling
/// back to the builtin #define) so the compiler never receives a malformed
/// `#define X <garbage>`.
pub fn parse_size_to_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Split leading digits from optional suffix.
    let digit_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if digit_end == 0 {
        return None; // no leading digits
    }
    let num: u64 = s[..digit_end].parse().ok()?;
    let suffix = s[digit_end..].trim();
    let mult: u64 = match suffix.to_ascii_uppercase().as_str() {
        "" => 1,
        "K" | "KB" => 1024,
        "M" | "MB" => 1024 * 1024,
        "G" | "GB" => 1024 * 1024 * 1024,
        _ => return None, // unknown suffix
    };
    if num == 0 {
        return None; // zero is not a valid size/count
    }
    num.checked_mul(mult)
}

/// Plan 62.F.bis Ф.1: sanitize edition string для filesystem path + Nova
/// identifier rules.
///
/// Преобразование:
///   - Нон-alphanumeric ASCII символы → `_` (например `2026.05` → `2026_05`).
///   - Если результат начинается с цифры (Nova ident должен начинаться
///     с буквы/`_` per `is_ident_start`) — prefix `e` (от "edition").
///     `2026.05` → `e2026_05`. `core` → `core` (без изменений).
///   - Empty input → empty output (caller отвечает за None-handling).
///
/// Используется resolver'ом для lookup'а `std/prelude/<sanitized>.nv`.
/// Файл `std/prelude/e2026_05.nv` имеет `module std.prelude.e2026_05`
/// (валидный path element).
pub fn sanitize_edition(edition: &str) -> String {
    let raw: String = edition.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if raw.is_empty() {
        return raw;
    }
    let first = raw.as_bytes()[0];
    if first.is_ascii_digit() {
        format!("e{}", raw)
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_edition_year_dot() {
        assert_eq!(sanitize_edition("2026.05"), "e2026_05");
    }

    #[test]
    fn sanitize_edition_word_unchanged() {
        assert_eq!(sanitize_edition("nightly"), "nightly");
    }

    #[test]
    fn sanitize_edition_mixed() {
        assert_eq!(sanitize_edition("v1-beta"), "v1_beta");
    }

    #[test]
    fn sanitize_edition_starts_underscore_no_prefix() {
        assert_eq!(sanitize_edition("_internal"), "_internal");
    }

    #[test]
    fn sanitize_edition_empty() {
        assert_eq!(sanitize_edition(""), "");
    }

    #[test]
    fn sanitize_edition_pure_digits() {
        assert_eq!(sanitize_edition("2026"), "e2026");
    }
}

/// Compute expected module path for a file given its package manifest.
/// Returns None if file is not under source_root (enforcement skipped).
///
/// **Plan 42 rev-1 (legacy):** Full path `package.dir1.dir2.file` для
/// single-file. (Сейчас для всех файлов.)
pub fn expected_module_path(file: &Path, m: &Manifest) -> Option<Vec<String>> {
    let abs_file = std::fs::canonicalize(file).ok()?;
    let abs_root = std::fs::canonicalize(&m.source_root).ok()?;
    let rel = abs_file.strip_prefix(&abs_root).ok()?;
    // rel = "encoding/base64.nv" (например). Drop .nv extension.
    let rel_no_ext = rel.with_extension("");
    let parts: Vec<String> = rel_no_ext
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
        .collect();
    if parts.is_empty() {
        return None;
    }
    // Plan 223 Ф.1: `bare` shadow manifest (empty `package_name`, see
    // `apply_src_transparency`) — this dead (rev-1, never accepted) legacy
    // hint stays cosmetically consistent (no leading-dot artifact in the
    // error message's "expected (rev-1 legacy)" line) rather than gaining
    // special-case logic of its own.
    if m.package_name.is_empty() {
        return Some(parts);
    }
    let mut full = vec![m.package_name.clone()];
    full.extend(parts);
    Some(full)
}

/// Plan 42 rev-3 (2026-05-13, D29 rev-3): compute expected `module
/// parent.target` declaration для файла. Returns None если file не под
/// source_root.
///
/// **Правило:**
/// - **target** = file basename без .nv (для single-file) или folder name
///   (для folder-module peer — определяется через folder_module flag).
/// - **parent** = directory сразу над target.
///
/// **Plan 42.13 (D29 rev-3.1): `internal/` special-case.** Если path
/// содержит сегмент `internal`, declaration = `<owner>.internal.<target>`
/// (3 segments), где owner = directory сразу перед `internal`. Это
/// устраняет naming collision когда у нескольких модулей свои `internal/`.
///
/// Examples (с source_root = `<repo>`):
/// - `src/main.nv` (single) → `["src", "main"]`
/// - `std/encoding/hex.nv` (single) → `["encoding", "hex"]`
/// - `std/encoding/json/parse.nv` (peer of `json/`) → `["encoding", "json"]`
/// - `src/admin/internal/token.nv` (single) → `["admin", "internal", "token"]`
/// - `src/admin/internal/codec/enc.nv` (peer of `codec/`) → `["admin", "internal", "codec"]`
pub fn expected_module_path_rev3(
    file: &Path,
    m: &Manifest,
    is_folder_module: bool,
) -> Option<Vec<String>> {
    let abs_file = std::fs::canonicalize(file).ok()?;
    let abs_root = std::fs::canonicalize(&m.source_root).ok()?;
    let rel = abs_file.strip_prefix(&abs_root).ok()?;
    let rel_no_ext = rel.with_extension("");
    let parts: Vec<String> = rel_no_ext
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
        .collect();

    // Plan 42.13 (D29 rev-3.1): `internal/` special-case.
    // Если path содержит `internal`, declaration = owner.internal.target.
    // owner = сегмент сразу перед `internal`. target = file basename
    // (single-file) или folder name (folder-module peer).
    //
    // Edge case: если `internal/` САМА folder-module (peers прямо в
    // internal/, target == "internal") — declaration = `owner.internal`
    // (2 segments, без дублирования).
    // Plan 223 Ф.1 (D78 rev-5, "src/ невидим всегда"): `m.package_name ==
    // ""` is the BARE/no-prefix sentinel used by the `src/`-shift shadow
    // manifest built in `apply_src_transparency` below — every "prepend
    // package name at root level" branch below OMITS the segment entirely
    // instead of emitting an empty-string segment, collapsing what would
    // otherwise be a 2 (or 3, for `internal/`) segment declaration down to
    // 1 (or 2) when there is no real package name to anchor it to (an
    // entry-mode app rooted at its own `src/`, not a named library
    // package). Ordinary manifests always have a non-empty
    // `package_name` (`nova.toml` requires it), so every EXISTING call
    // site is byte-identical to pre-223 behavior.
    let bare = m.package_name.is_empty();

    if let Some(internal_idx) = parts.iter().position(|s| s == "internal") {
        // owner = parts[internal_idx - 1]; если internal на root level
        // (parts[0] == "internal") — owner = package name (или ничего,
        // если `bare` — см. коммент выше).
        let owner: Option<String> = if internal_idx == 0 {
            if bare { None } else { Some(m.package_name.clone()) }
        } else {
            Some(parts[internal_idx - 1].clone())
        };
        // target = последний сегмент для single-file; для folder-module
        // peer — folder name (предпоследний сегмент).
        let target = if is_folder_module {
            // peer of folder: parts = [..., owner, internal, folder, basename]
            // target = folder = parts[parts.len()-2].
            if parts.len() < 2 {
                return None;
            }
            parts[parts.len() - 2].clone()
        } else {
            parts.last()?.clone()
        };
        let mut decl: Vec<String> = owner.into_iter().collect();
        decl.push("internal".to_string());
        // Если target == "internal" → `internal/` сама folder-module,
        // declaration = owner.internal (без дублирования target).
        if target != "internal" {
            decl.push(target);
        }
        return Some(decl);
    }

    if is_folder_module {
        // peer of folder `X/` — declaration = "<parent_of_X>.<X>".
        // rel = "encoding/json/parse" — но target = json (folder),
        // parent = encoding.
        // Так что мы берём parts[..parts.len()-1] и last из этого.
        if parts.len() < 2 {
            // peer на root level (например `src/main/foo.nv` — folder
            // module `main` под `src`): parent = root folder name.
            // Fall back to using package name as parent (или ничего в
            // `bare`-режиме — folder name один сегмент).
            if parts.len() == 1 {
                // folder = parts[0]
                return Some(if bare {
                    vec![parts[0].clone()]
                } else {
                    vec![m.package_name.clone(), parts[0].clone()]
                });
            }
            return None;
        }
        let folder = parts[parts.len() - 2].clone();
        if parts.len() == 2 {
            // folder прямо под source_root → parent = package name
            // (или ничего в `bare`-режиме).
            return Some(if bare { vec![folder] } else { vec![m.package_name.clone(), folder] });
        }
        let parent = parts[parts.len() - 3].clone();
        return Some(vec![parent, folder]);
    }

    // single-file: target = filename, parent = parent folder.
    if parts.is_empty() {
        return None;
    }
    let target = parts[parts.len() - 1].clone();
    if parts.len() == 1 {
        // file прямо под source_root → parent = package name (или ничего
        // в `bare`-режиме — просто имя файла, один сегмент).
        return Some(if bare { vec![target] } else { vec![m.package_name.clone(), target] });
    }
    let parent = parts[parts.len() - 2].clone();
    Some(vec![parent, target])
}

/// Проверить module declaration vs file path по D78. Returns:
/// - `Ok(ModulePathCheck::Rev3)` — strict rev-3 match.
/// - `Ok(ModulePathCheck::Rev1Deprecated(msg))` — rev-1 legacy match,
///   actionable warning message embedded.
/// - `Err(msg)` — neither match.
/// None manifest → enforcement skipped (не часть пакета) — returns Rev3.
///
/// **Plan 42 (2026-05-13) compatibility mode:** declaration валидно если
/// matches **либо** rev-1 (legacy full path) **либо** rev-3 (parent.X).
/// Это позволяет постепенную миграцию corpus без big-bang breaking change.
/// **Bug fix 2026-06-01:** legacy form теперь emit'ит deprecation warning
/// `W_D78_REV1_DEPRECATED` вместо silent acceptance, чтобы migrate
/// pressure был visible. После полной миграции rev-1 branch будет removed
/// (followup `[M-D78-strict-removal]`).
/// `[M-oot-dash-module-name-e78]` (2026-07-21): true iff `file` resolves to
/// a filesystem path OUTSIDE `repo` (both canonicalized). `repo` is the
/// CWD-resolved project root the CALLING `nova` invocation already computed
/// for import/prelude resolution (`nova-cli::find_repo_root()` /
/// `test_runner::codegen_to_c`'s `repo` param) — the SAME root threaded
/// through `resolve_imports_inline*` per `[M-standalone-out-of-tree-interp-sb-typedef]`.
///
/// Used to gate D78 enforcement at the call sites (`test_runner.rs`,
/// `nova-cli/src/main.rs`): `find_manifest` above walks parent directories
/// looking for **any** `nova.toml`, with no awareness of which project
/// actually invoked `nova`. For a file living outside the invoking
/// project's own tree (a `%TEMP%` probe, a scratch script), that walk can
/// land on a wholly UNRELATED ancestor manifest — e.g. a leftover
/// `nova.toml` several directories up a shared scratch tree from a
/// different earlier task — and enforce ITS `parent.target` rule against a
/// file that was never meant to be part of that package. That manifest is
/// real (not a bug in `find_manifest` itself, whose contract is exactly
/// "nearest ancestor `nova.toml`"), but honoring it for a file the CALLING
/// project doesn't consider its own is wrong: imports/prelude for that file
/// already resolve against `repo`/`stdlib_dir` (the invoking project), not
/// against whatever foreign manifest happens to sit above it — D78
/// enforcement should use the same "which project is this" answer, not a
/// second, inconsistent one.
///
/// A file INSIDE `repo` is unaffected (`false`) — the overwhelming in-tree
/// case (including nested real sub-packages like
/// `spec_tests/conformance/d78_root_peers/`) keeps exact pre-fix behavior.
/// Canonicalization failure (either path doesn't exist / inaccessible) is
/// treated conservatively as "not outside" (`false`) — enforcement still
/// runs, matching behavior before this fix existed.
pub fn is_outside_repo(file: &Path, repo: &Path) -> bool {
    let abs_file = match std::fs::canonicalize(file) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let abs_repo = match std::fs::canonicalize(repo) {
        Ok(p) => p,
        Err(_) => return false,
    };
    !abs_file.starts_with(&abs_repo)
}

pub fn check_module_path(
    file: &Path,
    declared: &[String],
) -> Result<ModulePathCheck, String> {
    // Plan 81 Ф.10: auto-detect whether `file` is a peer of a folder-module
    // so a folder-module *entry* (`nova check` / `nova build` pointed at one
    // of its peers) is validated against the folder-module D29 rule, not the
    // single-file rule. For every single-file entry the detector returns
    // false → identical to the pre-Ф.10 behaviour.
    let is_folder_module = crate::imports::is_folder_module_peer(file);
    check_module_path_with_kind(file, declared, is_folder_module)
}

/// Plan 42 D29 / D78 check result. `Ok(ModulePathCheck::Rev3)` — strict
/// rev-3 match. `Err(msg)` — declaration не соответствует rev-3.
///
/// History:
/// - **2026-05-13 (rev-3):** parent.target made canonical.
/// - **2026-06-01 bug fix:** ранее compiler silently accepted rev-1
///   legacy form. Fix добавил `W_D78_REV1_DEPRECATED` warning + audit/
///   migration script.
/// - **2026-06-01 strict removal `[M-D78-strict-removal]`:** rev-1
///   acceptance removed после full corpus migration (846 files). rev-1
///   form now → `E_D78_MODULE_PATH_MISMATCH` hard error. Rev1Deprecated
///   variant kept в enum для potential per-package opt-in legacy mode
///   (currently never produced — dead variant for ABI stability).
pub enum ModulePathCheck {
    /// Declaration matches strict rev-3 (parent.target).
    Rev3,
    /// **Dead variant (kept для ABI stability).** Rev-1 legacy match —
    /// больше не produces после [M-D78-strict-removal] (2026-06-01).
    /// rev-1 form now → hard error.
    #[allow(dead_code)]
    Rev1Deprecated(String),
}

/// Plan 223 Ф.1 (D78 rev-5 — "src/ невидим ВЕЗДЕ", §«Source root» amendment):
/// if `file` lives under a directory literally named `src` somewhere below
/// `m`'s OWN `source_root` (manifest-mode already resolves a nontrivial
/// `[lib] src` INTO `source_root` at parse time — see `Manifest::source_root`
/// doc — so this only fires for the file's remaining, package-internal path),
/// that `src/` becomes the file's EFFECTIVE module root and is never part of
/// the declared module path — symmetric with manifest-mode's own `[lib] src`.
///
/// Returns:
/// - `Ok(m)` unchanged (cloned) — no `src` directory found on the path;
///   every existing call site is byte-identical to pre-223 behavior (rule 2:
///   "entry вне `src/` не задет").
/// - `Ok(shadow)` — a shadow `Manifest` with `source_root` relocated to
///   (and including) the FIRST `src/` directory encountered walking from
///   `m.source_root` toward `file` ("ближайший предок... на пути ОТ
///   выведенного корня", D78 rev-5 §1), and `package_name` cleared to `""`
///   — the bare/no-prefix sentinel `expected_module_path_rev3` consumes to
///   omit the package-name segment entirely (an entry-mode app has no
///   library package name to prefix its own `src/`-rooted modules with).
/// - `Err(msg)` — `E_MODULE_DIR_SRC_RESERVED` (D78 rev-5 §3): a
///   module-folder literally named `src` sits somewhere INSIDE an already-
///   established source root — either (a) `m.source_root` was ALREADY
///   relocated by an explicit non-trivial `[lib] src` (`m.source_root !=
///   m.manifest_dir`) and `src` appears anywhere in what remains (manifest
///   mode's own `src/src/` case, e.g. a hypothetical `std/src/src/foo.nv`),
///   or (b) the flat/entry-mode root found MORE THAN ONE `src` directory on
///   the path (the first is legally rule-1's shift target; any FURTHER
///   `src/` nested inside it — `.../src/src/...` — would make rule 1
///   ambiguous, per D78 rev-5's own rationale for reserving the name).
fn apply_src_transparency(file: &Path, m: Manifest) -> Result<Manifest, String> {
    let Some(abs_file) = std::fs::canonicalize(file).ok() else { return Ok(m) };
    let Some(abs_root) = std::fs::canonicalize(&m.source_root).ok() else { return Ok(m) };
    let Ok(rel) = abs_file.strip_prefix(&abs_root) else { return Ok(m) };
    let rel_no_ext = rel.with_extension("");
    let parts: Vec<String> = rel_no_ext
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
        .collect();
    // A file directly in source_root has no directory chain to scan.
    if parts.len() < 2 {
        return Ok(m);
    }
    let dirs = &parts[..parts.len() - 1];
    let src_positions: Vec<usize> = dirs
        .iter()
        .enumerate()
        .filter(|(_, d)| d.as_str() == "src")
        .map(|(i, _)| i)
        .collect();
    if src_positions.is_empty() {
        return Ok(m);
    }
    let already_src_rooted = m.source_root != m.manifest_dir;
    if already_src_rooted || src_positions.len() > 1 {
        return Err(format!(
            "[E_MODULE_DIR_SRC_RESERVED] `src` is a reserved module-folder \
             name inside a source root (D78 rev-5 §3, Plan 223) — in {}\n  \
             a directory literally named `src` was found nested inside a \
             source root that already has its own effective `src/` (either \
             this package's manifest `[lib] src`, or an outer `src/` that \
             rule 1 already picked as the module root). `src/src/` would \
             make rule 1 ambiguous (which `src/` is THE root?) — rename the \
             inner directory.",
            file.display(),
        ));
    }
    // Exactly one `src` occurrence on a flat root — legal shift (rule 1).
    let idx = src_positions[0];
    let mut new_root = abs_root;
    for seg in &dirs[..=idx] {
        new_root.push(seg);
    }
    let mut shadow = m;
    shadow.source_root = new_root;
    shadow.package_name = String::new();
    Ok(shadow)
}

pub fn check_module_path_with_kind(
    file: &Path,
    declared: &[String],
    is_folder_module: bool,
) -> Result<ModulePathCheck, String> {
    let Some(manifest) = find_manifest(file) else {
        return Ok(ModulePathCheck::Rev3);
    };
    // Plan 223 Ф.1: `src/` transparency shift (see `apply_src_transparency`
    // doc) — `manifest_for_path` is the (possibly `src/`-shadowed) manifest
    // used for the rev-3/legacy expected-path computation; `manifest`
    // (unshadowed) stays the source of the D78 rev-4 root-peer alternate
    // form and the final error message's package-name mention, both of
    // which are ABOUT the real package, not the entry-mode `src/` shift.
    let manifest_for_path = apply_src_transparency(file, manifest.clone())?;
    // Plan 81 Ф.10: a folder-module peer's legacy (rev-1) declaration is the
    // path to the FOLDER — every peer of the folder shares one declaration,
    // so the file-stem segment is dropped. This matches the universal
    // folder-module convention (peer_recur, std/prelude/, …) and the
    // `import` path that addresses the folder.
    let expected_legacy = {
        let base = expected_module_path(file, &manifest_for_path);
        if is_folder_module {
            base.map(|mut v| {
                v.pop();
                v
            })
        } else {
            base
        }
    };
    let expected_rev3 = expected_module_path_rev3(file, &manifest_for_path, is_folder_module);

    // rev-3 strict match — only acceptable form (Plan 42 rev-3 canonical).
    if let Some(exp) = &expected_rev3 {
        if declared == exp.as_slice() {
            return Ok(ModulePathCheck::Rev3);
        }
    }
    // Plan 202 Ф.2 (D78 rev-4 "root peers"): a `.nv` file directly in the
    // package source_root MAY ADDITIONALLY declare the single-segment
    // `module <package>` form — peer of the root module (aliases Rust's
    // `lib.rs`; research 2026-07-13-module-naming-two-segment-review.md
    // §7). This is legal ALONGSIDE the independent `<package>.<stem>` form
    // checked above — a source root MAY mix root peers and independent
    // single-file modules (owner decision 2026-07-13, "смешанный корень
    // допустим"). Checked as a SEPARATE acceptance path (not folded into
    // `expected_rev3`) so it never changes the rev-3 error message shape
    // for the overwhelming non-root-peer case.
    if let Some(root_peer) = expected_root_peer_decl(file, &manifest) {
        if declared == root_peer.as_slice() {
            return Ok(ModulePathCheck::Rev3);
        }
    }
    // [M-D78-strict-removal] 2026-06-01: rev-1 legacy form больше не
    // accepted (full corpus migration completed; ~846 files migrated to
    // rev-3 via scripts/tools/d78_audit_migrate.py). Declaration в rev-1 form
    // теперь → hard error E_D78_MODULE_PATH_MISMATCH.

    let exp_legacy_str = expected_legacy
        .as_ref()
        .map(|e| e.join("."))
        .unwrap_or_else(|| "<n/a>".into());
    let exp_rev3_str = expected_rev3
        .as_ref()
        .map(|e| e.join("."))
        .unwrap_or_else(|| "<n/a>".into());
    Err(format!(
        "[E_D78_MODULE_PATH_MISMATCH] module declaration does not match file path \
         (D29 rev-3 + legacy)\n  \
         in {}\n  \
         declares `{}`\n  \
         expected (rev-3 parent.X): `{}`\n  \
         expected (rev-1 legacy):    `{}`\n  \
         expected (rev-4 root peer, only if directly in source root): `{}`",
        file.display(),
        declared.join("."),
        exp_rev3_str,
        exp_legacy_str,
        manifest.package_name,
    ))
}

/// Plan 202 Ф.2 (D78 rev-4 "root peers"): expected single-segment
/// `module <package>` declaration for a `.nv` file living DIRECTLY in the
/// package's `source_root` (depth 1, no subfolder). Returns `None` for any
/// file NOT a direct child of `source_root` — root peers are, by design,
/// only the immediate `.nv` files of the source root; a subfolder file
/// keeps the ordinary rev-3 `parent.target` rule unchanged.
///
/// Examples (`source_root` = package root, `package_name` = "tls"):
/// - `<root>/client.nv` → `Some(["tls"])` — legal ALTERNATIVE to the
///   independent form `["tls", "client"]` (both accepted, see caller).
/// - `<root>/x509/cert.nv` → `None` (not a direct child — ordinary rev-3
///   rule `["x509", "cert"]` applies, root peers don't reach subfolders).
pub fn expected_root_peer_decl(file: &Path, m: &Manifest) -> Option<Vec<String>> {
    let abs_file = std::fs::canonicalize(file).ok()?;
    let abs_root = std::fs::canonicalize(&m.source_root).ok()?;
    let parent = abs_file.parent()?;
    if parent != abs_root {
        return None;
    }
    Some(vec![m.package_name.clone()])
}

/// Plan 42 Sub-plan 42.6 (D29 rev-3): identify stdlib runtime module
/// (`std/runtime/*.nv`) под обоих declaration форматов.
///
/// Используется в type-checker'е для разрешения `external fn` keyword'а
/// (whitelisted только в stdlib runtime — D82).
///
/// - rev-1 legacy:  `module std.runtime.X` → `["std", "runtime", X]`
/// - rev-3 default: `module runtime.X`     → `["runtime", X]` (parent=runtime, target=X)
///
/// **Plan 91 Ф.7.1 (2026-05-27):** расширено для дополнительных stdlib
/// модулей, которые легитимно используют `external fn` для wrapping
/// native runtime:
///   - `std.net.*` / `net.*` — Plan 83.12 async net stdlib (libuv TCP/UDP).
///   - `std.bench` / `bench` — Plan 57 benchmark DSL (hard-coded namespace).
///
/// Compat mode остаётся после Sub-plan 42.6 migration для случая user
/// package с `name = "std"` (overlap с stdlib namespace).
pub fn is_stdlib_runtime_module(name: &[String]) -> bool {
    // std.runtime.* / runtime.* (original Plan 42 whitelist)
    if (name.len() >= 2 && name[0] == "std" && name[1] == "runtime")
        || (name.len() == 2 && name[0] == "runtime")
    {
        return true;
    }
    // Plan 91 Ф.7.1: std.net.* / net.* (Plan 83.12 async net stdlib)
    if (name.len() >= 2 && name[0] == "std" && name[1] == "net")
        || (name.len() == 2 && name[0] == "net")
    {
        return true;
    }
    // Plan 91 Ф.7.1: std.bench / bench (Plan 57 benchmark DSL)
    if (name.len() == 2 && name[0] == "std" && name[1] == "bench")
        || (name.len() == 1 && name[0] == "bench")
    {
        return true;
    }
    false
}

/// Plan 42 Sub-plan 42.6: identify `std/prelude.nv` под обоих форматов.
/// Используется в resolver для skip self-import prelude.
///
/// - rev-1 legacy:  `module std.prelude` → `["std", "prelude"]`
/// - rev-3:         `module <package>.prelude` (для stdlib `<package>=std`,
///   так что result совпадает; для user package — `["myproject", "prelude"]`).
///
/// Более permissive — match по `last() == "prelude"` чтобы прикрыть оба.
///
/// **Plan 62.A:** prelude теперь splittable — `std/prelude/<sub>.nv` тоже
/// считаются "prelude self" для целей auto-import. Иначе sub-module
/// получает auto-import `std.prelude`, который re-export'ит sub-module →
/// circular import. Match по prefix:
///   - `std.prelude.<sub>` (stdlib splittable)
///   - `<pkg>.prelude.<sub>` (user-package splittable)
pub fn is_prelude_self_module(name: &[String]) -> bool {
    // Legacy: any module чей last segment == "prelude"
    // (e.g. ["std", "prelude"], ["foo", "prelude"], ["foo", "bar", "prelude"]).
    let is_prelude_root = name.last().map(|s| s == "prelude").unwrap_or(false);
    // Plan 62.A: splittable prelude sub-modules — penultimate == "prelude".
    // E.g. ["std", "prelude", "core"], ["std", "prelude", "runtime"],
    //      ["foo", "prelude", "core"].
    let is_prelude_submodule = name.len() >= 2
        && name.get(name.len() - 2).map(|s| s == "prelude").unwrap_or(false);
    is_prelude_root || is_prelude_submodule
}

/// Plan 172.1 U.1.1 (compiler-conventions §2 «никакой особости std»): резолвит
/// КОРЕНЬ std-пакета как **конфиг**, а не хардкод `repo.join("std")`. std — просто
/// пакет, найденный по search-path (как sysroot в Rust / GOROOT в Go); «ГДЕ искать»
/// — конфиг (разрешено §2), «ЧТО внутри» (файлы/сигнатуры) — остаётся в папке.
///
/// Precedence (highest first):
///   1. env `NOVA_STD_PATH` (абсолютный или относительный к `repo`);
///   2. `nova.toml` ключ `std = "..."` в секции `[workspace]` или `[package]`
///      (относительный — к `repo`);
///   3. дефолт `repo/std` — **байт-идентично** прежнему поведению (0 регрессий).
///
/// CLI `--std-path` (ещё одна config-поверхность поверх env) — followup
/// `[M-172.1-U1-cli-stdpath]`; env+manifest уже делают расположение std
/// настраиваемым, что и требует §2.
///
/// **Plan 195 (2026-07-13):** возвращаемое значение исторически трактовалось
/// вызывающим кодом как каталог, где `.nv`-файлы лежат НЕПОСРЕДСТВЕННО
/// (`stdlib_dir.join("prelude.nv")` и т.п.) — то есть как **source root**, а
/// не просто корень пакета. После перевода `std` на канон `src/`
/// (`std/nova.toml`: `[lib] src = "src"`) корень пакета (`repo/std`) и
/// source root (`repo/std/src`) разошлись. Чтобы не трогать ~20 call-сайтов
/// в compiler-codegen/nova-cli/nova-lsp, шаг (4) читает `[lib] src` из
/// `nova.toml` найденного std-корня (через тот же `parse_manifest`, что и
/// для обычных пакетов) и возвращает `source_root`, если манифест валиден.
/// Std без `nova.toml` (тестовые фикстуры) или без `[lib] src` — не
/// меняется (fallback на сам `std_root`, byte-identical прежнему поведению).
pub fn resolve_std_path(repo: &Path) -> PathBuf {
    // (1) env override.
    let std_root = if let Ok(v) = std::env::var("NOVA_STD_PATH") {
        let v = v.trim();
        if !v.is_empty() {
            let p = PathBuf::from(v);
            if p.is_absolute() { p } else { repo.join(p) }
        } else {
            resolve_std_root_no_env(repo)
        }
    } else {
        resolve_std_root_no_env(repo)
    };
    // (4) уважать `[lib] src` внутри найденного std-пакета — source root,
    // а не просто package root (Plan 195).
    parse_manifest(&std_root.join("nova.toml"), &std_root)
        .map(|m| m.source_root)
        .unwrap_or(std_root)
}

/// Шаги (2)-(3) `resolve_std_path` (без env override) — вынесены в helper,
/// чтобы (4) `[lib] src` применялся единообразно независимо от того, откуда
/// взялся package root.
fn resolve_std_root_no_env(repo: &Path) -> PathBuf {
    // (2) manifest `[workspace]/[package].std`.
    if let Some(rel) = read_std_key(&repo.join("nova.toml")) {
        let p = PathBuf::from(&rel);
        return if p.is_absolute() { p } else { repo.join(p) };
    }
    // (3) default — identical to the previous hardcode.
    repo.join("std")
}

/// Прочитать ключ `std = "..."` из секции `[workspace]` или `[package]` файла
/// `nova.toml`. Минимальный целевой парс (как `edition` в `parse_manifest`),
/// чтобы не тащить полный Manifest в путь-резолва. `None`, если файла/ключа нет.
fn read_std_key(toml_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(toml_path).ok()?;
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            continue;
        }
        if section != "workspace" && section != "package" {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            if key.trim() == "std" {
                // Strip inline comment, quotes, whitespace.
                let v = val.split('#').next().unwrap_or("").trim().trim_matches('"').trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod parse_tests {
    use super::*;
    use std::io::Write;

    /// Helper: записывает text в tempfile под name, возвращает (path, dir).
    /// Использует unique временную директорию, чтобы тесты не интерферировали.
    fn write_toml(name: &str, text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("nova_manifest_test_{}_{}", name,
            std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir tempdir");
        let toml_path = dir.join("nova.toml");
        let mut f = std::fs::File::create(&toml_path).expect("create toml");
        f.write_all(text.as_bytes()).expect("write toml");
        (toml_path, dir)
    }

    /// Plan 71 Ф.1 acceptance №1: `enforce-stability = true` корректно парсится.
    #[test]
    fn enforce_stability_true() {
        let (path, dir) = write_toml("estab_true", "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\nenforce-stability = true\n");
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(m.enforce_stability);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 71 Ф.1 acceptance №2: при отсутствии flag — default false.
    #[test]
    fn enforce_stability_default_false() {
        let (path, dir) = write_toml("estab_default", "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n");
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(!m.enforce_stability);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 71 Ф.1 acceptance №3: `enforce-stability = "garbage"` → ignored (false).
    /// Conservative parsing: anything kроме literal `true` → false.
    #[test]
    fn enforce_stability_garbage_ignored() {
        let (path, dir) = write_toml("estab_garbage", "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\nenforce-stability = \"garbage\"\n");
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(!m.enforce_stability);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Дополнительно: `enforce-stability = false` (explicit) → false.
    #[test]
    fn enforce_stability_explicit_false() {
        let (path, dir) = write_toml("estab_explicit_false", "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\nenforce-stability = false\n");
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(!m.enforce_stability);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Robustness: inline comment после value не ломает парсинг.
    #[test]
    fn enforce_stability_trailing_comment() {
        let (path, dir) = write_toml("estab_trail_cmt", "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\nenforce-stability = true # opt-in строгий режим\n");
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(m.enforce_stability);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Flag в неправильной секции (`[package]`) — не должен распознаваться.
    #[test]
    fn enforce_stability_wrong_section_ignored() {
        let (path, dir) = write_toml("estab_wrong_section",
            "[package]\nname = \"x\"\nenforce-stability = true\n[lib]\nsrc = \".\"\n");
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(!m.enforce_stability, "flag только в [lib], не в [package]");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── Plan 204: [replace] + manifest_warnings ──────────────────────────

    /// `[replace]` перекрывает эффективный источник зависимости, объявленной
    /// в `[dependencies]` как `{ git, version }` — `effective_source` должен
    /// вернуть `path`, не git.
    #[test]
    fn replace_overrides_git_dep() {
        let (path, dir) = write_toml(
            "replace_override",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = { git = \"https://x.org/tls\", version = \"0.1\" }\n\
             [replace]\ntls = { path = \"../nova-tls\" }\n",
        );
        let m = parse_manifest(&path, &dir).expect("parse");
        assert_eq!(m.dependencies.len(), 1);
        let dep = &m.dependencies[0];
        // Declared source остаётся git — replace не мутирует [dependencies].
        assert!(matches!(dep.source, DepSource::Git { .. }));
        // Effective — path из [replace].
        match m.effective_source(dep) {
            DepSource::Path(p) => assert_eq!(p, "../nova-tls"),
            other => panic!("ожидался Path (replace), получено {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Без `[replace]` — effective_source == declared source (no-op).
    #[test]
    fn no_replace_effective_equals_declared() {
        let (path, dir) = write_toml(
            "no_replace",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nfoo = { path = \"../foo\" }\n",
        );
        let m = parse_manifest(&path, &dir).expect("parse");
        let dep = &m.dependencies[0];
        match m.effective_source(dep) {
            DepSource::Path(p) => assert_eq!(p, "../foo"),
            other => panic!("ожидался Path, получено {:?}", other),
        }
        assert!(m.replace.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Голый `path` в [dependencies] (без release-формы) → W_DEP_PATH_NO_RELEASE.
    #[test]
    fn manifest_warning_bare_path_dep() {
        let (path, dir) = write_toml(
            "bare_path_warn",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nfoo = { path = \"../foo\" }\n",
        );
        let m = parse_manifest(&path, &dir).expect("parse");
        let ws = manifest_warnings(&m, &path);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].code, "W_DEP_PATH_NO_RELEASE");
        assert!(ws[0].message.contains("foo"), "msg: {}", ws[0].message);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `path`-зависимость ПОД `[replace]` (override git+version dep для
    /// dev) НЕ warns via `manifest_warnings` — declared-форма
    /// (`[dependencies]`) сама по себе git, публикуемый источник есть;
    /// path — только override. **Owner correction (дофикс №2):** committed
    /// `[replace]` НЕ warning, а жёсткая ошибка через отдельную функцию
    /// `check_no_committed_replace` — см. `committed_replace_is_hard_error`
    /// ниже; `manifest_warnings` больше не эмитит ничего про `[replace]`
    /// в закоммиченном файле (только `W_REPLACE_UNKNOWN_DEP` /
    /// `W_OVERRIDE_TOML_UNSUPPORTED_KEY`).
    #[test]
    fn manifest_no_warning_when_path_is_replace_override() {
        let (path, dir) = write_toml(
            "replace_no_warn",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = { git = \"https://x.org/tls\", version = \"0.1\" }\n\
             [replace]\ntls = { path = \"../nova-tls\" }\n",
        );
        let m = parse_manifest(&path, &dir).expect("parse");
        let ws = manifest_warnings(&m, &path);
        assert!(ws.is_empty(), "warnings: {:?}", ws);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 204 дофикс №2 (owner correction): `[replace]` declared directly
    /// in the COMMITTED `nova.toml` — `check_no_committed_replace` must
    /// hard-Err with `E_REPLACE_IN_MANIFEST`, no deprecation window. Plan
    /// 233 §2а: the hint now points at the NEW name `nova.override.toml`.
    #[test]
    fn committed_replace_is_hard_error() {
        let (path, dir) = write_toml(
            "committed_replace_err",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = { git = \"https://x.org/tls\", version = \"0.1\" }\n\
             [replace]\ntls = { path = \"../nova-tls\" }\n",
        );
        let m = parse_manifest(&path, &dir).expect("parse");
        let err = check_no_committed_replace(&m, &path).expect_err("must hard-error");
        assert!(err.contains("E_REPLACE_IN_MANIFEST"), "err: {}", err);
        assert!(err.contains("nova.override.toml"), "err hints nova.override.toml: {}", err);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[replace]` living ONLY in `nova.override.toml` (nothing in the
    /// committed `nova.toml`) — `check_no_committed_replace` must be Ok,
    /// AND `effective_source` must still honor the override (merged into
    /// `m.replace` by `parse_manifest`).
    #[test]
    fn override_toml_only_replace_is_not_a_hard_error() {
        let (path, dir) = write_toml(
            "override_only_replace_ok",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = { git = \"https://x.org/tls\", version = \"0.1\" }\n",
        );
        std::fs::write(
            dir.join("nova.override.toml"),
            "[replace]\ntls = { path = \"../nova-tls\" }\n",
        ).unwrap();
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(check_no_committed_replace(&m, &path).is_ok());
        assert!(!m.replace_in_committed_manifest);
        assert!(!m.override_legacy_name_used, "new name present — must NOT flag legacy");
        match m.effective_source(&m.dependencies[0]) {
            DepSource::Path(p) => assert_eq!(p, "../nova-tls"),
            other => panic!("nova.override.toml [replace] must still be honored, got {:?}", other),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 233 §2а: `[replace]` living in the LEGACY `nova.local.toml`
    /// name — still read (back-compat) AND still honored by
    /// `effective_source`, but `manifest_warnings` must flag
    /// `W_OVERRIDE_TOML_DEPRECATED` recommending the rename.
    #[test]
    fn legacy_local_toml_replace_still_honored_with_deprecation_warning() {
        let (path, dir) = write_toml(
            "legacy_local_toml_replace",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = { git = \"https://x.org/tls\", version = \"0.1\" }\n",
        );
        std::fs::write(
            dir.join("nova.local.toml"),
            "[replace]\ntls = { path = \"../nova-tls\" }\n",
        ).unwrap();
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(m.override_legacy_name_used, "legacy name used — must flag it");
        match m.effective_source(&m.dependencies[0]) {
            DepSource::Path(p) => assert_eq!(p, "../nova-tls"),
            other => panic!("legacy nova.local.toml [replace] must still be honored, got {:?}", other),
        }
        let ws = manifest_warnings(&m, &path);
        let deprecated: Vec<_> = ws.iter().filter(|w| w.code == "W_OVERRIDE_TOML_DEPRECATED").collect();
        assert_eq!(deprecated.len(), 1, "ws: {:?}", ws);
        assert!(deprecated[0].message.contains("nova.override.toml"), "msg: {}", deprecated[0].message);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 233 §2а: both `nova.override.toml` (new) AND `nova.local.toml`
    /// (legacy) present in the same directory — the NEW name wins, no
    /// deprecation warning (matches `pkg_proxy`'s
    /// `new_override_name_wins_over_legacy_when_both_present`).
    #[test]
    fn new_override_name_wins_over_legacy_when_both_present() {
        let (path, dir) = write_toml(
            "both_override_names",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = { git = \"https://x.org/tls\", version = \"0.1\" }\n",
        );
        std::fs::write(
            dir.join("nova.override.toml"),
            "[replace]\ntls = { path = \"../new-wins\" }\n",
        ).unwrap();
        std::fs::write(
            dir.join("nova.local.toml"),
            "[replace]\ntls = { path = \"../legacy-loses\" }\n",
        ).unwrap();
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(!m.override_legacy_name_used);
        match m.effective_source(&m.dependencies[0]) {
            DepSource::Path(p) => assert_eq!(p, "../new-wins"),
            other => panic!("new nova.override.toml must win, got {:?}", other),
        }
        let ws = manifest_warnings(&m, &path);
        assert!(ws.iter().all(|w| w.code != "W_OVERRIDE_TOML_DEPRECATED"), "ws: {:?}", ws);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `nova.override.toml` with a section OTHER than `[replace]` — the
    /// unsupported key/section is recorded (`W_OVERRIDE_TOML_UNSUPPORTED_KEY`),
    /// but parsing itself is NOT rejected (forward-compat: unknown keys are
    /// soft-flagged, not fatal).
    #[test]
    fn override_toml_unsupported_section_warns() {
        let (path, dir) = write_toml(
            "override_toml_unsupported",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n",
        );
        std::fs::write(
            dir.join("nova.override.toml"),
            "[dependencies]\nfoo = { path = \"../foo\" }\n",
        ).unwrap();
        let m = parse_manifest(&path, &dir).expect("parse");
        let ws = manifest_warnings(&m, &path);
        let unsupported: Vec<_> = ws.iter()
            .filter(|w| w.code == "W_OVERRIDE_TOML_UNSUPPORTED_KEY")
            .collect();
        assert_eq!(unsupported.len(), 1, "ws: {:?}", ws);
        assert!(unsupported[0].message.contains("dependencies.foo"), "msg: {}", unsupported[0].message);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Override-файл (ни новое, ни legacy имя) absent — no unsupported-key
    /// warnings, `replace` unaffected (byte-identical to pre-дофикс
    /// behavior).
    #[test]
    fn no_override_toml_is_a_no_op() {
        let (path, dir) = write_toml(
            "no_override_toml",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n",
        );
        let m = parse_manifest(&path, &dir).expect("parse");
        assert!(m.override_toml_unsupported.is_empty());
        assert!(!m.override_legacy_name_used);
        assert!(m.replace.is_empty());
        assert!(!m.replace_in_committed_manifest);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 204 дофикс №2 (owner correction №2): path-dep staying INSIDE the
    /// same git repo as the manifest (a real `.git` ancestor shared by both)
    /// must NOT trigger `W_DEP_PATH_NO_RELEASE` — clone-safe (workspace
    /// member / nested test package). Uses a REAL temp git repo (via `git
    /// init`) since `git_repo_root` looks for an actual `.git` entry.
    #[test]
    fn in_repo_path_dep_no_warning() {
        let dir = std::env::temp_dir().join(format!("nova_p204_inrepo_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sub_a = dir.join("pkg_a");
        let sub_b = dir.join("pkg_b");
        std::fs::create_dir_all(&sub_a).unwrap();
        std::fs::create_dir_all(&sub_b).unwrap();
        // Fake `.git` at the shared repo root (a directory is enough for
        // `git_repo_root`'s `.exists()` check — no real git needed).
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        let toml_a = sub_a.join("nova.toml");
        std::fs::write(
            &toml_a,
            "[package]\nname = \"a\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nb = { path = \"../pkg_b\" }\n",
        ).unwrap();
        std::fs::write(sub_b.join("nova.toml"), "[package]\nname = \"b\"\n[lib]\nsrc = \".\"\n").unwrap();
        let m = parse_manifest(&toml_a, &sub_a).expect("parse");
        let ws = manifest_warnings(&m, &toml_a);
        assert!(
            ws.iter().all(|w| w.code != "W_DEP_PATH_NO_RELEASE"),
            "in-repo path dep must not warn: {:?}", ws,
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Cross-repo path-dep (target has ITS OWN separate `.git`, not shared
    /// with the manifest's repo) — `W_DEP_PATH_NO_RELEASE` still fires.
    #[test]
    fn cross_repo_path_dep_still_warns() {
        let dir = std::env::temp_dir().join(format!("nova_p204_crossrepo_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo_a = dir.join("repo_a");
        let repo_b = dir.join("repo_b");
        std::fs::create_dir_all(&repo_a).unwrap();
        std::fs::create_dir_all(&repo_b).unwrap();
        std::fs::create_dir_all(repo_a.join(".git")).unwrap();
        std::fs::create_dir_all(repo_b.join(".git")).unwrap();
        let toml_a = repo_a.join("nova.toml");
        std::fs::write(
            &toml_a,
            "[package]\nname = \"a\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nb = { path = \"../repo_b\" }\n",
        ).unwrap();
        std::fs::write(repo_b.join("nova.toml"), "[package]\nname = \"b\"\n[lib]\nsrc = \".\"\n").unwrap();
        let m = parse_manifest(&toml_a, &repo_a).expect("parse");
        let ws = manifest_warnings(&m, &toml_a);
        assert!(
            ws.iter().any(|w| w.code == "W_DEP_PATH_NO_RELEASE"),
            "cross-repo path dep must still warn: {:?}", ws,
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `[replace]` без соответствующей записи в `[dependencies]` →
    /// W_REPLACE_UNKNOWN_DEP.
    #[test]
    fn manifest_warning_replace_unknown_dep() {
        let (path, dir) = write_toml(
            "replace_unknown",
            "[package]\nname = \"x\"\n[lib]\nsrc = \".\"\n\
             [replace]\nghost = { path = \"../ghost\" }\n",
        );
        let m = parse_manifest(&path, &dir).expect("parse");
        let ws = manifest_warnings(&m, &path);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].code, "W_REPLACE_UNKNOWN_DEP");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 03.2: `{ git = "...", version = "^1.2" }` → GitPin::Version.
    #[test]
    fn dep_git_version_pin() {
        let src = parse_dep_source("{ git = \"https://x.org/g.nv\", version = \"^1.2\" }");
        match src {
            DepSource::Git { pin: GitPin::Version(req), .. } => {
                assert!(req.matches(&crate::semver::Version::new(1, 5, 0)));
                assert!(!req.matches(&crate::semver::Version::new(2, 0, 0)));
            }
            other => panic!("ожидался GitPin::Version, получено {:?}", other),
        }
    }

    /// Plan 03.2: пины git взаимоисключающи — tag + version → Invalid.
    #[test]
    fn dep_git_conflicting_pins_invalid() {
        let src = parse_dep_source(
            "{ git = \"https://x.org/g.nv\", tag = \"v1\", version = \"^1.2\" }",
        );
        match src {
            DepSource::Invalid(msg) => assert!(
                msg.contains("взаимоисключ"),
                "msg: {}", msg,
            ),
            other => panic!("ожидался Invalid, получено {:?}", other),
        }
    }

    /// Plan 03.2: некорректный version-диапазон → Invalid.
    #[test]
    fn dep_git_bad_version_invalid() {
        let src = parse_dep_source("{ git = \"https://x.org/g.nv\", version = \"^x.y\" }");
        assert!(matches!(src, DepSource::Invalid(_)), "получено {:?}", src);
    }

    /// Plan 03.4: `forbid = [...]` парсится; bracket-aware split не
    /// ломает соседние поля (`git` резолвится корректно рядом с массивом).
    #[test]
    fn dep_forbid_parsed() {
        let raw = "{ git = \"https://x.org/g.nv\", tag = \"v1\", forbid = [\"Net\", \"Fs\"] }";
        assert_eq!(parse_dep_forbid(raw), vec!["Net".to_string(), "Fs".to_string()]);
        // Запятая внутри [...] не должна разорвать поле git/tag.
        match parse_dep_source(raw) {
            DepSource::Git { url, pin } => {
                assert_eq!(url, "https://x.org/g.nv");
                assert_eq!(pin, GitPin::Tag("v1".to_string()));
            }
            other => panic!("ожидался Git, получено {:?}", other),
        }
    }

    /// Plan 03.4: без `forbid` — пустой список.
    #[test]
    fn dep_forbid_absent_empty() {
        assert!(parse_dep_forbid("{ path = \"../foo\" }").is_empty());
        assert!(parse_dep_forbid("\"1.2\"").is_empty());
    }

    // ── Plan 172.1 U.1.1: resolve_std_path ───────────────────────────────

    /// `read_std_key` достаёт `std = "..."` из `[workspace]` и `[package]`,
    /// игнорирует другие секции, комментарии и иные ключи.
    #[test]
    fn read_std_key_workspace_and_package() {
        let (p, _d) = write_toml(
            "nova.toml",
            "[workspace]\nmembers = [\"std\"]\nstd = \"vendor/std\"  # comment\n",
        );
        assert_eq!(read_std_key(&p).as_deref(), Some("vendor/std"));

        let (p2, _d2) = write_toml(
            "nova.toml",
            "[package]\nname = \"x\"\nstd = \"../mystd\"\n",
        );
        assert_eq!(read_std_key(&p2).as_deref(), Some("../mystd"));
    }

    /// Без ключа `std` в релевантных секциях → None (а `std` в другой секции
    /// или другой ключ не путаются).
    #[test]
    fn read_std_key_absent() {
        let (p, _d) = write_toml(
            "nova.toml",
            "[package]\nname = \"x\"\nedition = \"2026.05\"\n[dependencies]\nstd = \"ignored\"\n",
        );
        assert_eq!(read_std_key(&p), None);
        // несуществующий файл → None
        assert_eq!(read_std_key(std::path::Path::new("/nonexistent/nova.toml")), None);
    }

    /// `resolve_std_path` без env и без manifest → дефолт `repo/std`
    /// (байт-идентично прежнему хардкоду). env здесь НЕ трогаем (флака в
    /// параллельных тестах); env-precedence проверяется интеграционно.
    #[test]
    fn resolve_std_path_default_is_repo_std() {
        // unique temp repo dir WITHOUT a nova.toml → no manifest key.
        let dir = std::env::temp_dir().join(format!("nova_u111_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Guard: only assert the default when NOVA_STD_PATH is unset in this env.
        if std::env::var_os("NOVA_STD_PATH").is_none() {
            assert_eq!(resolve_std_path(&dir), dir.join("std"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// manifest-ключ резолвится относительно `repo`.
    #[test]
    fn resolve_std_path_manifest_relative_to_repo() {
        if std::env::var_os("NOVA_STD_PATH").is_some() {
            return; // env override wins — skip in that environment.
        }
        let dir = std::env::temp_dir().join(format!("nova_u111m_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("nova.toml"), "[workspace]\nstd = \"vendored/std\"\n").unwrap();
        assert_eq!(resolve_std_path(&dir), dir.join("vendored/std"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Plan 195 (2026-07-13): найденный std-корень с собственным `nova.toml`
    /// объявляющим `[lib] src = "src"` — `resolve_std_path` возвращает
    /// SOURCE ROOT (`<std_root>/src`), а не package root, чтобы ~20
    /// call-сайтов, трактующих результат как «где лежат .nv напрямую»,
    /// продолжали работать без изменений.
    #[test]
    fn resolve_std_path_respects_lib_src_in_std_manifest() {
        if std::env::var_os("NOVA_STD_PATH").is_some() {
            return; // env override wins — skip in that environment.
        }
        let dir = std::env::temp_dir().join(format!("nova_p195_libsrc_{}", std::process::id()));
        let std_dir = dir.join("std");
        let _ = std::fs::create_dir_all(&std_dir);
        std::fs::write(
            std_dir.join("nova.toml"),
            "[package]\nname = \"std\"\n[lib]\nsrc = \"src\"\n",
        ).unwrap();
        assert_eq!(resolve_std_path(&dir), std_dir.join("src"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// std-корень с `nova.toml`, но БЕЗ `[lib] src` (или `src = "."`) —
    /// source root == package root, byte-identical прежнему поведению.
    #[test]
    fn resolve_std_path_std_manifest_without_lib_src_is_unchanged() {
        if std::env::var_os("NOVA_STD_PATH").is_some() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("nova_p195_nolibsrc_{}", std::process::id()));
        let std_dir = dir.join("std");
        let _ = std::fs::create_dir_all(&std_dir);
        std::fs::write(std_dir.join("nova.toml"), "[package]\nname = \"std\"\n").unwrap();
        assert_eq!(resolve_std_path(&dir), std_dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
