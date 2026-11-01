//! Addressed byte-embed resources for the compiler-free program stage.
//! This is deliberately not a Nova parser: only the seed's plain literal
//! `embed("path")` door is recognized. Other forms remain the seed's concern.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
struct Token<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

// Skip strings and comments as units, so examples in documentation, ordinary
// strings and member calls cannot cause filesystem reads. Keep literal spans
// for the few assets that cannot keep their spelling after relocation.
fn tokens(text: &str) -> Vec<Token<'_>> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if b[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        let start = i;
        if b[i] == b'"' || b[i] == b'\'' {
            let quote = b[i];
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 128 {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 128) {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push(Token {
            text: &text[start..i],
            start,
            end: i,
        });
    }
    out
}

fn literals(text: &str) -> Vec<Token<'_>> {
    let ts = tokens(text);
    let mut out = Vec::new();
    for k in 0..ts.len().saturating_sub(3) {
        if ts[k].text != "embed"
            || (k > 0 && ts[k - 1].text == ".")
            || ts[k + 1].text != "("
            || ts[k + 3].text != ")"
        {
            continue;
        }
        let lit = &ts[k + 2];
        if lit.text.starts_with('"') && lit.text.ends_with('"') && lit.text.len() >= 2 {
            let path = &lit.text[1..lit.text.len() - 1];
            // The seed refuses escapes/interpolation; do not interpret them.
            if !path.is_empty() && !path.contains('\\') && !path.contains("${") {
                out.push(Token {
                    text: path,
                    start: lit.start,
                    end: lit.end,
                });
            }
        }
    }
    out
}

fn normalized(path: &Path) -> PathBuf {
    // Canonical Windows paths carry a verbatim prefix; a valid absolute
    // literal usually does not. Compare the same spelling on both sides.
    let plain;
    let path = if cfg!(windows) {
        let text = path.to_string_lossy();
        plain = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{rest}"))
        } else if let Some(rest) = text.strip_prefix(r"\\?\") {
            PathBuf::from(rest)
        } else {
            path.to_path_buf()
        };
        plain.as_path()
    } else {
        path
    };
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(part.as_os_str()),
        }
    }
    out
}

fn package_root(source: &Path) -> Result<PathBuf> {
    source
        .ancestors()
        .skip(1)
        .find(|p| p.join("nova.toml").is_file())
        .unwrap_or_else(|| source.parent().unwrap())
        .canonicalize()
        .context("cannot resolve embed package root")
}

/// Each destination has one original-file owner (one value per map key).
/// [INV-PROPERTY] An asset must never overwrite an import-rewritten .nv:
/// `stage_embeds` remaps every .nv destination to .bin before its only copy
/// site, separating data and source destinations by construction. Other
/// destination collisions are rejected by `stage_embeds`' owner-map lookup.
/// Coverage in program.rs::tests:
/// `staging_preserves_explicit_embed_assets_without_sweeping_directories` and
/// `staging_embed_rejects_dependency_destination_collisions`.
pub(crate) type Destinations = HashMap<PathBuf, PathBuf>;

pub(crate) fn destination_key(path: &Path) -> PathBuf {
    let path = normalized(path);
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path
    }
}

pub(crate) fn stage_embeds(
    source: &Path,
    target: &Path,
    stage_root: &Path,
    text: &str,
    destinations: &mut Destinations,
) -> Result<String> {
    let lits = literals(text);
    if lits.is_empty() {
        return Ok(text.to_owned());
    }
    let root = package_root(source)?;
    let stage = normalized(stage_root);
    let mut out = String::new();
    let mut at = 0;
    for lit in lits {
        let written = source.parent().unwrap().canonicalize()?.join(lit.text);
        // Check both lexical traversal and canonical symlink traversal BEFORE
        // any resource bytes are read. Each dependency owns its own boundary.
        let lexical = normalized(&written);
        if !lexical.starts_with(normalized(&root)) {
            bail!(
                "cannot stage embed outside package root: {} (source {})",
                written.display(),
                source.display()
            );
        }
        let full = match written.canonicalize() {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Leave real missing resources to E_EMBED_NOT_FOUND, but use a
                // private absent destination so a staged module cannot mask it.
                let missing = target.parent().unwrap().join(format!(
                    "__nova_missing_embed_{}_{}.bin",
                    target.file_name().unwrap().to_string_lossy(),
                    lit.start
                ));
                let key = destination_key(&missing);
                if missing.exists() || destinations.contains_key(&key) {
                    bail!("embed missing-path collision: {}", missing.display());
                }
                destinations.insert(key, written.clone());
                out.push_str(&text[at..lit.start]);
                out.push_str(&format!(
                    "\"{}\"",
                    missing.file_name().unwrap().to_string_lossy()
                ));
                at = lit.end;
                continue;
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("cannot resolve embed {}", written.display()))
            }
        };
        if !full.starts_with(&root) {
            bail!(
                "cannot stage embed outside package root: {} (source {})",
                written.display(),
                source.display()
            );
        }
        let mut dest = normalized(&target.parent().unwrap().join(lit.text));
        // Absolute paths, upward paths that leave the stage and .nv data
        // cannot be placed literally: the seed would read outside the stage
        // or compile the bytes as a source unit. Keep them beside this copy
        // under a non-source name and rewrite ONLY the staged path literal.
        let relocated = dest == stage.join("nova.toml")
            || !dest.starts_with(&stage)
            || Path::new(lit.text).is_absolute()
            || dest
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("nv"));
        if relocated {
            dest = target.parent().unwrap().join(format!(
                "__nova_embed_{}_{}.bin",
                target.file_name().unwrap().to_string_lossy(),
                lit.start
            ));
        }
        let key = destination_key(&dest);
        if let Some(prior) = destinations.get(&key) {
            if prior.canonicalize()? != full {
                bail!("embed stage destination collision: {}", dest.display());
            }
            // A .nv source with import rewrites cannot double as raw bytes.
            if dest
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("nv"))
            {
                bail!("embed stage source collision: {}", dest.display());
            }
        } else {
            if dest.exists() {
                bail!(
                    "untracked embed stage destination collision: {}",
                    dest.display()
                );
            }
            destinations.insert(key, full.clone());
            std::fs::create_dir_all(dest.parent().unwrap())?;
            if full.is_dir() {
                // Preserve the seed's E_EMBED_IS_A_DIR; never sweep its files.
                std::fs::create_dir_all(&dest)?;
            } else {
                std::fs::copy(&full, &dest).with_context(|| {
                    format!(
                        "cannot stage embed {} -> {}",
                        full.display(),
                        dest.display()
                    )
                })?;
            }
        }
        if relocated {
            out.push_str(&text[at..lit.start]);
            out.push_str(&format!(
                "\"{}\"",
                dest.file_name().unwrap().to_string_lossy()
            ));
            at = lit.end;
        }
    }
    out.push_str(&text[at..]);
    Ok(out)
}
