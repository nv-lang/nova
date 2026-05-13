//! D78 path/module enforcement.
//!
//! Walk parent dirs от файла, ищем `nova.toml`. Из него извлекаем:
//!   - `[package].name` (default: dir name)
//!   - `[lib].src` (default: "src")
//!
//! Source root = nova.toml dir + (`[lib].src` или `src/`).
//! Expected module = `<package>.<rel-path-from-src-without-ext>`.
//!
//! Если файл лежит **вне** source root — пропускаем enforcement (это
//! может быть test, example, scratch — не часть пакета).
//!
//! Минимальный TOML-парсер: ищем только `name = "..."` в `[package]` и
//! `src = "..."` в `[lib]`. Не подтягиваем full TOML crate ради bootstrap'а.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Manifest {
    pub package_name: String,
    pub source_root: PathBuf,
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

fn parse_manifest(toml_path: &Path, dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(toml_path).ok()?;
    let mut package_name: Option<String> = None;
    let mut lib_src: Option<String> = None;
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
                "package" => "package",
                "lib"     => "lib",
                _         => "",  // ignore other sections
            };
            // Note: `&'static str` can't be assigned from a String slice;
            // we work around with hardcoded match arms. Section names we
            // care about are fixed ("package", "lib"), so this works.
            // Suppress unused warning on `section` reassignment.
            let _ = section;
            continue;
        }
        // key = "value" — minimal parsing
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"').to_string();
            match (section, key) {
                ("package", "name") => package_name = Some(val),
                ("lib", "src")      => lib_src = Some(val),
                _ => {}
            }
        }
    }

    let pkg = package_name?;
    let src_subdir = lib_src.unwrap_or_else(|| "src".to_string());
    let source_root = if src_subdir == "." {
        dir.to_path_buf()
    } else {
        dir.join(src_subdir)
    };
    Some(Manifest {
        package_name: pkg,
        source_root,
    })
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
/// Examples (с source_root = `<repo>`):
/// - `src/main.nv` (single) → `["src", "main"]`
/// - `std/encoding/hex.nv` (single) → `["encoding", "hex"]`
/// - `std/encoding/json/parse.nv` (peer of `json/`) → `["encoding", "json"]`
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
    check_module_path_with_kind(file, declared, false)
}

pub fn check_module_path_with_kind(
    file: &Path,
    declared: &[String],
    is_folder_module: bool,
) -> Result<(), String> {
    let Some(manifest) = find_manifest(file) else {
        return Ok(());
    };
    let expected_legacy = expected_module_path(file, &manifest);
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
