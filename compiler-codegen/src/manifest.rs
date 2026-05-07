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

/// Проверить module declaration vs file path по D78. Returns Err с
/// человекочитаемым сообщением, если mismatch. None manifest →
/// enforcement skipped (не часть пакета).
pub fn check_module_path(
    file: &Path,
    declared: &[String],
) -> Result<(), String> {
    let Some(manifest) = find_manifest(file) else {
        return Ok(());
    };
    let Some(expected) = expected_module_path(file, &manifest) else {
        return Ok(());
    };
    if declared == expected.as_slice() {
        return Ok(());
    }
    Err(format!(
        "module declaration does not match file path\n  \
         in {}\n  \
         declares `{}`\n  \
         expected for this path: `{}`\n  \
         hints:\n  \
         - move file to: {}/{}.nv (preserve module name)\n  \
         - rename module to: {} (match current path)",
        file.display(),
        declared.join("."),
        expected.join("."),
        manifest.source_root.display(),
        declared[1..].join("/"),  // skip package prefix in suggestion path
        expected.join("."),
    ))
}
