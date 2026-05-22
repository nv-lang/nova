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

use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone)]
pub struct Manifest {
    pub package_name: String,
    pub source_root: PathBuf,
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
}

/// Plan 03.1: quote-aware разбор тела inline-таблицы TOML
/// (`key = "v", key2 = "v2"`) — запятая внутри `"..."` не разделяет.
fn parse_inline_table(body: &str) -> Vec<(String, String)> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in body.chars() {
        match ch {
            '"' => { in_str = !in_str; cur.push(ch); }
            ',' if !in_str => { parts.push(std::mem::take(&mut cur)); }
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

/// Plan 03.1 Ф.4: директория ближайшего вверх по дереву `nova.toml` —
/// корень пакета, которому принадлежит `file`. `None` — файл не входит
/// ни в один пакет.
pub fn find_package_dir(file: &Path) -> Option<PathBuf> {
    let abs = std::fs::canonicalize(file).ok()?;
    let mut dir = abs.parent()?.to_path_buf();
    loop {
        if dir.join("nova.toml").is_file() {
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
    let abs = std::fs::canonicalize(file).ok()?;
    let mut dir = abs.parent()?.to_path_buf();
    loop {
        let toml = dir.join("nova.toml");
        if toml.is_file() {
            return parse_manifest(&toml, &dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Parse a `nova.toml` directly from `toml_path`, with `dir` as the
/// manifest-relative source-root anchor. Public for use from
/// `nova-cli::build_lint_config_for` fallback path и в integration
/// tests (Plan 71 Ф.1 / Ф.5).
pub fn parse_manifest(toml_path: &Path, dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(toml_path).ok()?;
    let mut package_name: Option<String> = None;
    let mut lib_src: Option<String> = None;
    let mut edition: Option<String> = None;
    let mut enforce_stability: bool = false;
    let mut dependencies: Vec<Dependency> = Vec::new();
    let mut section: &str = "";

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            // [section] or [[section]]
            let inner = line.trim_start_matches('[').trim_end_matches(']');
            section = match inner {
                "package"      => "package",
                "lib"          => "lib",
                "dependencies" => "dependencies",
                _              => "",  // ignore other sections
            };
            // Note: `&'static str` can't be assigned from a String slice;
            // we work around with hardcoded match arms. Section names we
            // care about are fixed ("package", "lib"), so this works.
            // Suppress unused warning on `section` reassignment.
            let _ = section;
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
                });
                continue;
            }
            match (section, key) {
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
    Some(Manifest {
        package_name: pkg,
        source_root,
        edition,
        enforce_stability,
        dependencies,
    })
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
    if let Some(internal_idx) = parts.iter().position(|s| s == "internal") {
        // owner = parts[internal_idx - 1]; если internal на root level
        // (parts[0] == "internal") — owner = package name.
        let owner = if internal_idx == 0 {
            m.package_name.clone()
        } else {
            parts[internal_idx - 1].clone()
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
        // Если target == "internal" → `internal/` сама folder-module,
        // declaration = owner.internal (2 segments, без дублирования).
        if target == "internal" {
            return Some(vec![owner, "internal".to_string()]);
        }
        return Some(vec![owner, "internal".to_string(), target]);
    }

    if is_folder_module {
        // peer of folder `X/` — declaration = "<parent_of_X>.<X>".
        // rel = "encoding/json/parse" — но target = json (folder),
        // parent = encoding.
        // Так что мы берём parts[..parts.len()-1] и last из этого.
        if parts.len() < 2 {
            // peer на root level (например `src/main/foo.nv` — folder
            // module `main` под `src`): parent = root folder name.
            // Fall back to using package name as parent.
            if parts.len() == 1 {
                // folder = parts[0]
                return Some(vec![m.package_name.clone(), parts[0].clone()]);
            }
            return None;
        }
        let folder = parts[parts.len() - 2].clone();
        let parent = if parts.len() == 2 {
            // folder прямо под source_root → parent = package name
            m.package_name.clone()
        } else {
            parts[parts.len() - 3].clone()
        };
        return Some(vec![parent, folder]);
    }

    // single-file: target = filename, parent = parent folder.
    if parts.is_empty() {
        return None;
    }
    let target = parts[parts.len() - 1].clone();
    let parent = if parts.len() == 1 {
        // file прямо под source_root → parent = package name
        m.package_name.clone()
    } else {
        parts[parts.len() - 2].clone()
    };
    Some(vec![parent, target])
}

/// Проверить module declaration vs file path по D78. Returns Err с
/// человекочитаемым сообщением, если mismatch. None manifest →
/// enforcement skipped (не часть пакета).
///
/// **Plan 42 (2026-05-13) compatibility mode:** declaration валидно если
/// matches **либо** rev-1 (legacy full path) **либо** rev-3 (parent.X).
/// Это позволяет постепенную миграцию std/* без big-bang breaking change.
/// После полной миграции rev-1 branch будет removed.
pub fn check_module_path(
    file: &Path,
    declared: &[String],
) -> Result<(), String> {
    // Plan 81 Ф.10: auto-detect whether `file` is a peer of a folder-module
    // so a folder-module *entry* (`nova check` / `nova build` pointed at one
    // of its peers) is validated against the folder-module D29 rule, not the
    // single-file rule. For every single-file entry the detector returns
    // false → identical to the pre-Ф.10 behaviour.
    let is_folder_module = crate::imports::is_folder_module_peer(file);
    check_module_path_with_kind(file, declared, is_folder_module)
}

pub fn check_module_path_with_kind(
    file: &Path,
    declared: &[String],
    is_folder_module: bool,
) -> Result<(), String> {
    let Some(manifest) = find_manifest(file) else {
        return Ok(());
    };
    // Plan 81 Ф.10: a folder-module peer's legacy (rev-1) declaration is the
    // path to the FOLDER — every peer of the folder shares one declaration,
    // so the file-stem segment is dropped. This matches the universal
    // folder-module convention (peer_recur, std/prelude/, …) and the
    // `import` path that addresses the folder.
    let expected_legacy = {
        let base = expected_module_path(file, &manifest);
        if is_folder_module {
            base.map(|mut v| {
                v.pop();
                v
            })
        } else {
            base
        }
    };
    let expected_rev3 = expected_module_path_rev3(file, &manifest, is_folder_module);

    // rev-3 first (preferred); fallback rev-1 (legacy compatibility).
    if let Some(exp) = &expected_rev3 {
        if declared == exp.as_slice() {
            return Ok(());
        }
    }
    if let Some(exp) = &expected_legacy {
        if declared == exp.as_slice() {
            return Ok(());
        }
    }

    let exp_legacy_str = expected_legacy
        .as_ref()
        .map(|e| e.join("."))
        .unwrap_or_else(|| "<n/a>".into());
    let exp_rev3_str = expected_rev3
        .as_ref()
        .map(|e| e.join("."))
        .unwrap_or_else(|| "<n/a>".into());
    Err(format!(
        "module declaration does not match file path (D29 rev-3 + legacy)\n  \
         in {}\n  \
         declares `{}`\n  \
         expected (rev-3 parent.X): `{}`\n  \
         expected (rev-1 legacy):    `{}`",
        file.display(),
        declared.join("."),
        exp_rev3_str,
        exp_legacy_str,
    ))
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
/// Compat mode остаётся после Sub-plan 42.6 migration для случая user
/// package с `name = "std"` (overlap с stdlib namespace).
pub fn is_stdlib_runtime_module(name: &[String]) -> bool {
    (name.len() >= 2 && name[0] == "std" && name[1] == "runtime")
        || (name.len() == 2 && name[0] == "runtime")
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
}
