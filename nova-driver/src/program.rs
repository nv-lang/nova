//! Multi-module program discovery for the Carina build path (274.16 Ш.4,
//! registry №1889).
//!
//! The pinned Carina B1 `emit` contract takes ONE entry file plus a program
//! root (`NOVAC_SELF_PATH`) that it reads recursively, grouping every
//! directory of `.nv` files into one unit. Handing the seed the compiler's
//! own tree (`novac/src`) is right only for building Carina itself: for a
//! user program the entry lies outside that root, so the entry is compiled
//! as a lone unit and every call into the program's own modules fails
//! (measured: `E_NOVAC_SUBSET`, "`greet` is not a callable novac knows").
//! Pointing the root at the whole package instead is not viable either: the
//! directory-unit grouping would merge every sibling file of a crowded
//! corpus directory into one unit (measured: `spec_tests` as the root does
//! not terminate in ten minutes, and `conformance/standalone` would be one
//! unit of 404 `main`s).
//!
//! So the driver — the thin CLI/graph owner per the Ш.4 boundary — discovers
//! the program's own module graph with a deliberately shallow textual scan
//! (import lines and module declarations only; parsing, checking and
//! emitting Nova stay with the pinned Carina B1) and stages exactly the
//! reachable files into a fresh per-build tree that preserves the
//! package-relative layout, so each staged directory is exactly one module
//! of the program. A program with no program modules at all keeps the
//! historical single-file invocation untouched.
//!
//! Known boundaries of the shallow scan (all failures are loud at the seed,
//! never silent): import paths split across lines are not seen; `embed`
//! (D412) assets beside the sources are not staged; package-qualified
//! imports of the OWN package are staged but the pinned B1 links only
//! relative-import edges between units (registry caveat of №1889).

use anyhow::{Context, Result};
use nova_driver_support::manifest::Manifest;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// The program's own source files, discovered from the entry: the entry's
/// folder-module peers plus the transitive closure of its relative and
/// own-package imports. The entry itself is NOT part of the returned set.
pub(crate) fn discover_program_files(entry: &Path, manifest: &Manifest) -> Result<Vec<PathBuf>> {
    let mut known: BTreeSet<PathBuf> = BTreeSet::new();
    known.insert(normalize_path(entry));
    let mut queue: Vec<PathBuf> = vec![entry.to_path_buf()];

    // Peers of the entry's own folder-module share its namespace without an
    // import (D78), so no scan of imports can find them: they are the files
    // of the entry's directory declaring the same `module` line.
    if let Some(module) = module_line(&read_text(entry)?) {
        if let Some(dir) = entry.parent() {
            for peer in module_peer_files(dir, &module)? {
                let peer = normalize_path(&peer);
                if known.insert(peer.clone()) {
                    queue.push(peer);
                }
            }
        }
    }

    while let Some(file) = queue.pop() {
        let text = read_text(&file)?;
        let file_dir = file
            .parent()
            .ok_or_else(|| anyhow::anyhow!("program file has no parent: {}", file.display()))?;
        for import in scan_import_paths(&text) {
            for candidate in resolve_import(&import, file_dir, manifest) {
                for member in module_files(&candidate) {
                    let member = normalize_path(&member);
                    if known.insert(member.clone()) {
                        queue.push(member);
                    }
                }
            }
        }
    }
    known.remove(&normalize_path(entry));
    Ok(known.into_iter().collect())
}

/// Stage the entry and the discovered program files under `stage_root`,
/// keeping each file's path relative to `source_base` so the staged tree
/// has the same module layout as the package. Returns the staged entry.
pub(crate) fn stage_program(
    stage_root: &Path,
    source_base: &Path,
    entry: &Path,
    program_files: &[PathBuf],
) -> Result<PathBuf> {
    if stage_root.exists() {
        std::fs::remove_dir_all(stage_root).with_context(|| {
            format!("cannot clear stale program stage {}", stage_root.display())
        })?;
    }
    let mut staged_entry = PathBuf::new();
    for source in std::iter::once(entry).chain(program_files.iter().map(PathBuf::as_path)) {
        let relative = source.strip_prefix(source_base).unwrap_or_else(|_| {
            // A file outside the source base (an import walking above the
            // package root) keeps its file name only; Carina B1 reports any
            // module it then cannot reach.
            Path::new(source.file_name().unwrap_or_default())
        });
        let target = stage_root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("cannot create program stage directory {}", parent.display())
            })?;
        }
        std::fs::copy(source, &target).with_context(|| {
            format!(
                "cannot stage program file {} -> {}",
                source.display(),
                target.display()
            )
        })?;
        if normalize_path(source) == normalize_path(entry) {
            staged_entry = target;
        }
    }
    Ok(staged_entry)
}

/// The raw import path tokens of one file: `./geo`, `../source`, `std.fs`,
/// `nova_examples.tour.greeter`. Shallow by contract (module docs above):
/// a line whose first token is `import` (or `export import`) contributes the
/// dotted/slashed token right after the keyword, before the brace list.
fn scan_import_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with("//") {
            continue;
        }
        let rest = if let Some(rest) = line.strip_prefix("import ") {
            rest
        } else if let Some(rest) = line.strip_prefix("export import ") {
            rest
        } else {
            continue;
        };
        let token: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '\\'))
            .collect();
        let token = token.trim_end_matches('.');
        if !token.is_empty() {
            out.push(token.to_owned());
        }
    }
    out
}

/// The declared module of one file: the first line `module <dotted>`.
fn module_line(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        return line
            .strip_prefix("module ")
            .map(|rest| rest.trim().to_owned());
    }
    None
}

/// Resolve one raw import token to candidate module paths (a directory for
/// a folder-module, or a `.nv` file), still unverified.
fn resolve_import(token: &str, file_dir: &Path, manifest: &Manifest) -> Vec<PathBuf> {
    let mut rest = token;
    let base: PathBuf;
    if rest.starts_with("./") || rest.starts_with("../") {
        let mut dir = file_dir.to_path_buf();
        loop {
            if let Some(tail) = rest.strip_prefix("./") {
                rest = tail;
            } else if let Some(tail) = rest.strip_prefix("../") {
                dir.pop();
                rest = tail;
            } else {
                break;
            }
        }
        base = dir;
    } else {
        let first = rest.split(['.', '/']).next().unwrap_or("");
        if first == manifest.package_name {
            rest = rest
                .strip_prefix(first)
                .unwrap_or(rest)
                .trim_start_matches(['.', '/']);
            base = manifest.source_root.clone();
        } else {
            // std is handed to the seed through NOVA_STD_PATH; external
            // packages are dependency roots the pinned B1 program walk does
            // not link (module docs above) — neither is staged here.
            return Vec::new();
        }
    }
    let mut target = base;
    for segment in rest.split(['.', '/', '\\']) {
        if !segment.is_empty() {
            target.push(segment);
        }
    }
    let mut out = vec![target.clone()];
    if target.extension().is_none() {
        out.push(target.with_extension("nv"));
    }
    out
}

/// The files of one module candidate: every direct `.nv` of a folder-module
/// (tests stay out, as the seed's own reader excludes them), or the single
/// file itself. Both forms may answer one import (D78 Rule E); whichever
/// exists is handed over, and the seed remains the judge of ambiguity.
fn module_files(candidate: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if candidate.is_dir() {
        if let Ok(entries) = std::fs::read_dir(candidate) {
            let mut names: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|path| {
                    path.is_file()
                        && path.extension().is_some_and(|e| e == "nv")
                        && !path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.ends_with("_test.nv"))
                })
                .collect();
            names.sort();
            out.extend(names);
        }
    }
    if candidate.is_file() {
        out.push(candidate.to_path_buf());
    }
    out
}

/// The peer files of a folder-module directory declaring `module` — used for
/// the entry's own directory, where every file is its own module unless it
/// declares the entry's module (D78: peers declare one shared name).
fn module_peer_files(dir: &Path, module: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("cannot list the entry's directory {}", dir.display()))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || !path.extension().is_some_and(|e| e == "nv") {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_test.nv"))
        {
            continue;
        }
        if module_line(&read_text(&path)?).as_deref() == Some(module) {
            out.push(path);
        }
    }
    Ok(out)
}

fn read_text(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let text = String::from_utf8(bytes)
        .with_context(|| format!("program source is not UTF-8: {}", path.display()))?;
    Ok(text.trim_start_matches('\u{feff}').to_owned())
}

/// Lexical normalization (`.`/`..` resolved) so the same file reached two
/// ways compares equal; no filesystem access, no symlink resolution.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{discover_program_files, module_line, scan_import_paths, stage_program};
    use nova_driver_support::manifest::Manifest;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn test_dir() -> PathBuf {
        let root = std::env::var_os("NOVA_TEST_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("nova-driver-program-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn manifest(dir: &Path) -> Manifest {
        let text = "[package]\nname = \"probe\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n";
        write(&dir.join("nova.toml"), text);
        nova_driver_support::manifest::parse_manifest(&dir.join("nova.toml"), dir).unwrap()
    }

    fn remove_dir(path: &Path) {
        if path.exists() {
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn scan_finds_relative_qualified_and_export_imports() {
        let text = "module m.main\n\
                    import ./helper.{greet}\n\
                    import ../shared/util.{U}\n\
                    export import ./facade.{F}\n\
                    import std.fs.{read}\n\
                    import probe.net.http.{get}\n\
                    // import ./commented.{out}\n\
                    fn main() {}\n";
        let found = scan_import_paths(text);
        assert_eq!(
            found,
            vec![
                "./helper",
                "../shared/util",
                "./facade",
                "std.fs",
                "probe.net.http"
            ]
        );
    }

    #[test]
    fn module_line_reads_the_first_declaration() {
        assert_eq!(
            module_line("// comment\nmodule probe.helper\n\nfn f() {}\n"),
            Some("probe.helper".to_owned())
        );
        assert_eq!(module_line("fn main() {}\n"), None);
    }

    #[test]
    fn discovery_walks_folder_modules_and_skips_tests_and_aliens() {
        let dir = test_dir();
        let entry = dir.join("main.nv");
        write(
            &entry,
            "module probe.main\nimport ./helper.{greet}\nfn main() { println(greet()) }\n",
        );
        write(
            &dir.join("helper/a.nv"),
            "module probe.helper\nexport fn greet() -> int => 1\n",
        );
        write(
            &dir.join("helper/b.nv"),
            "module probe.helper\nexport fn other() -> int => 2\n",
        );
        write(&dir.join("helper/a_test.nv"), "module probe.helper\nfn t() {}\n");
        // A sibling of the entry with its OWN module is not the program's.
        write(&dir.join("other.nv"), "module probe.other\nfn main() {}\n");
        let found = discover_program_files(&entry, &manifest(&dir)).unwrap();
        assert_eq!(
            found,
            vec![dir.join("helper/a.nv"), dir.join("helper/b.nv")]
        );
        remove_dir(&dir);
    }

    #[test]
    fn discovery_includes_same_module_peers_of_the_entry() {
        let dir = test_dir();
        let entry = dir.join("one.nv");
        write(&entry, "module probe.shared\nfn main() { println(part()) }\n");
        write(
            &dir.join("two.nv"),
            "module probe.shared\nexport fn part() -> int => 1\n",
        );
        write(&dir.join("alien.nv"), "module probe.alien\nfn main() {}\n");
        let found = discover_program_files(&entry, &manifest(&dir)).unwrap();
        assert_eq!(found, vec![dir.join("two.nv")]);
        remove_dir(&dir);
    }

    #[test]
    fn staging_mirrors_the_package_relative_layout() {
        let dir = test_dir();
        let entry = dir.join("src/main.nv");
        let helper = dir.join("src/helper/helper.nv");
        write(&entry, "module probe.main\n");
        write(&helper, "module probe.helper\n");
        let stage = dir.join("stage");
        let staged = stage_program(&stage, &dir.join("src"), &entry, &[helper]).unwrap();
        assert_eq!(staged, stage.join("main.nv"));
        assert!(stage.join("helper/helper.nv").is_file());
        remove_dir(&dir);
    }
}
