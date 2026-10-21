//! Driver-owned manifest metadata, dependency parsing and lock preconditions.
use crate::semver::VersionReq;
use crate::{FfiConfig, RuntimeConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitPin {
    Rev(String),
    Tag(String),
    Branch(String),
    Version(VersionReq),
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepSource {
    Path(String),
    Git { url: String, pin: GitPin },
    Registry(String),
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub source: DepSource,
    pub forbid: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub package_name: String,
    pub source_root: PathBuf,
    /// Directory containing `nova.toml` itself (the package root). Usually
    /// identical to `source_root` — they diverge for the canonical
    /// `src/` layout, `[lib] src = "<subdir>"` (D78 + the Plan 195 amendment,
    /// standard for new packages, NOT back-compat; reference: `nova-tls`'s
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

impl crate::ManifestFfiView for Manifest {
    fn manifest_dir(&self) -> &Path {
        &self.manifest_dir
    }

    fn ffi_config(&self) -> Option<&FfiConfig> {
        self.ffi.as_ref()
    }
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
            '"' => {
                in_str = !in_str;
                cur.push(ch);
            }
            '[' if !in_str => {
                depth += 1;
                cur.push(ch);
            }
            ']' if !in_str => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if !in_str && depth == 0 => {
                parts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    parts
        .iter()
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            let k = k.trim();
            if k.is_empty() {
                return None;
            }
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
        let get = |k: &str| {
            fields
                .iter()
                .find(|(fk, _)| fk == k)
                .map(|(_, fv)| fv.clone())
        };
        if let Some(p) = get("path") {
            DepSource::Path(p)
        } else if let Some(url) = get("git") {
            // [INV-PROPERTY] GitPin is a single enum variant; pin_count rejects multiple inputs.
            let pin_count = ["rev", "tag", "branch", "version"]
                .iter()
                .filter(|k| get(k).is_some())
                .count();
            if pin_count > 1 {
                return DepSource::Invalid(format!(
                    "git-зависимость: пины rev/tag/branch/version \
                     взаимоисключающи (указано {})", // [INV-PROPERTY] pin_count > 1 returns Invalid.
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
                            "git-зависимость: некорректный version `{}`: {:#}",
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
/// машино-локальных оверрайдов. В этой волне поддержана ТОЛЬКО секция [INV-PROPERTY]
/// `[replace]` (тот же формат записи, что и `[replace]` в `nova.toml`).
/// The section == "replace" branch is the only insertion into replace_map;
/// every other parsed assignment is returned as an unsupported label.
/// Прочие секции/ключи не отклоняют парсинг (forward-compat — будущие
/// волны могут добавить поддержанные ключи без breaking change) — но
/// собираются как `unsupported`-метки для `W_OVERRIDE_TOML_UNSUPPORTED_KEY`.
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
            section = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
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
            '"' => {
                in_str = !in_str;
                cur.push(ch);
            }
            ',' if !in_str => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    parts
        .iter()
        .map(|p| p.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a driver manifest from the exact driver-owned subset of `nova.toml`.
pub fn parse_manifest(toml_path: &Path, dir: &Path) -> Option<Manifest> {
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
    let mut ffi_cxx = false;
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
                "package" => "package",
                "lib" => "lib",
                "dependencies" => "dependencies",
                "replace" => "replace",
                "exports.consume_types" => "exports.consume_types",
                "ffi" => {
                    ffi_section_seen = true;
                    "ffi"
                }
                "runtime" => {
                    runtime_section_seen = true;
                    "runtime"
                }
                _ => "", // ignore other sections
            }
            .to_string();
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
                    "c_shims" => ffi_c_shims = parse_toml_string_array(raw_val),
                    "include_dirs" => ffi_include_dirs = parse_toml_string_array(raw_val),
                    "lib_dirs" => ffi_lib_dirs = parse_toml_string_array(raw_val),
                    "libs" => ffi_libs = parse_toml_string_array(raw_val),
                    "vendor_src_dirs" => ffi_vendor_src_dirs = parse_toml_string_array(raw_val),
                    // Bool, not an array: it never meets parse_toml_string_array,
                    // and so never meets the silent-empty defect of #1055.
                    "cxx" => ffi_cxx = raw_val.trim_matches('"') == "true",
                    _ => {} // ignore unknown keys для forward-compat
                }
                continue;
            }
            // Plan 149 D233: [runtime] section — fiber arena tuning.
            if section == "runtime" {
                match key {
                    "fiber_stack" => runtime_fiber_stack = Some(str_val),
                    "fibers_per_worker" => runtime_fibers_per_worker = Some(str_val),
                    _ => {} // ignore unknown keys для forward-compat
                }
                continue;
            }
            match (section.as_str(), key) {
                ("package", "name") => package_name = Some(str_val),
                // Plan 62.F.bis Ф.1: `[package].edition = "2026.05"` pin
                // для prelude content. Опционально — отсутствие → rolling.
                ("package", "edition") => edition = Some(str_val),
                ("lib", "src") => lib_src = Some(str_val),
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
    // D78 + амендмент Plan 195 (2026-07-10): source root задаётся ключом
    // `[lib] src`; для новых пакетов канон — `src = "src"`, плоская
    // раскладка `.` легальна и служит умолчанием. Строка ниже даёт
    // ровно это и НЕ менялась; неверным был только комментарий (№1026).
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
            cxx: ffi_cxx,
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
    let effective_override_path: Option<PathBuf> = if override_toml_path.is_file() {
        Some(override_toml_path)
    } else if legacy_override_toml_path.is_file() {
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
        self.replace
            .get(&dep.name)
            .cloned()
            .unwrap_or_else(|| dep.source.clone())
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
    // Registry 221.1 #1656: a RELATIVE dir is resolved against the current
    // directory first. `nova check src` run inside a package hands in the
    // manifest dir of `nova.toml` -- the EMPTY path `""`: it does not
    // `exists()`, `pop()` on it fails at once, and the walk answered `None`
    // ("the manifest is not under git") while the dependency `../..`, which
    // does exist, found the repo -- a false E_DEP_PATH_OUTSIDE_REPO for a
    // path dependency inside the same repository.
    let mut d = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(dir),
            Err(_) => dir.to_path_buf(),
        }
    };
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
        (None, Some(_)) => "манифест не под git — подтвердить, что цель придёт вместе с ним, нечем",
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
                    d.name,
                    rel,
                    toml_path.display(),
                    cross_repo_reason(&m.manifest_dir, &dep_dir),
                    d.name,
                    d.name,
                    rel,
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
                        d.name,
                        toml_path.display(),
                        cross_repo_reason(&m.manifest_dir, &dep_dir),
                        d.name,
                        d.name,
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
                    name,
                    toml_path.display(),
                ),
            });
        }
    }
    // Plan 233 §2а: имя override-файла, ФАКТИЧЕСКИ использованного при
    // резолве (для точных путей в diagnostic message) — legacy, если
    // `override_legacy_name_used`, иначе новое каноническое имя.
    let override_file_name = if m.override_legacy_name_used {
        "nova.local.toml"
    } else {
        "nova.override.toml"
    };
    if m.override_legacy_name_used {
        let legacy_path = toml_path
            .parent()
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
        let override_path = toml_path
            .parent()
            .map(|d| d.join(override_file_name))
            .unwrap_or_else(|| PathBuf::from(override_file_name));
        for label in &m.override_toml_unsupported {
            out.push(ManifestWarning {
                code: "W_OVERRIDE_TOML_UNSUPPORTED_KEY",
                message: format!(
                    "{}: неподдерживаемый ключ/секция `{}` — {} \
                     поддерживает в этой волне ТОЛЬКО [replace]",
                    override_path.display(),
                    label,
                    override_file_name,
                ),
            });
        }
    }
    out
}

#[cfg(test)]
mod extraction_property_tests {
    use super::{parse_dep_source, parse_override_toml, DepSource};

    #[test]
    fn every_pair_of_git_pins_is_rejected() {
        let pins = ["rev", "tag", "branch", "version"];
        for (index, first) in pins.iter().enumerate() {
            for second in &pins[index + 1..] {
                let text = format!("{{ git = \"https://example.invalid/repo\", {first} = \"1\", {second} = \"1\" }}");
                assert!(matches!(parse_dep_source(&text), DepSource::Invalid(_)), "{text}");
            }
        }
    }

    #[test]
    fn only_replace_assignments_become_overrides() {
        let path = std::env::temp_dir().join(format!("nova-driver-override-property-{}.toml", std::process::id()));
        std::fs::write(&path, "top = \"value\"\n[replace]\nmath = { path = \"../math\" }\n[other]\nmath = { path = \"../wrong\" }\n").unwrap();
        let (replacements, unsupported) = parse_override_toml(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(replacements.len(), 1);
        assert!(matches!(replacements.get("math"), Some(DepSource::Path(path)) if path == "../math"));
        assert_eq!(unsupported, ["<top-level>.top", "other.math"]);
    }
}
