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
//! EXTERNAL PACKAGES (task 129, the class measured by tasks 122/128): the
//! same walk used to STOP at the package boundary, so the seed never saw a
//! dependency's declarations at all (`undeclared type name ... in the
//! declarations handed to novac`). Now the import walk crosses into every
//! dependency of the lockfile graph (the build root's `[replace]` overlay
//! honored for every edge, mirroring the oracle's
//! `imports::lookup_dependency`): a dependency's modules are staged one
//! directory PER MODULE under `_deps/<module path>/`, because the seed's
//! directory-is-one-unit grouping cannot hold a `src/` that mixes several
//! `module` lines. The seed's program graph links only RELATIVE import
//! edges between units (registry №1924's root), so inside the staged copies
//! — never in the user's sources — every import that names a staged
//! dependency module is rewritten to the relative path of that module's
//! staged directory. The rewrite preserves the path's last segment, which
//! is exactly what the seed's import visibility matches on (274.11 E.10's
//! named simplification).
//!
//! Known boundaries of the shallow scan (all failures are loud at the seed,
//! never silent): import paths split across lines are not seen; `embed`
//! (D412) assets beside the sources are not staged; package-qualified
//! imports of the OWN package are staged but the pinned B1 links only
//! relative-import edges between units (registry caveat of №1889); spans in
//! the seed's diagnostics name the staged copies, and a rewritten import
//! line shifts the byte offsets behind it within that copy.

use anyhow::{Context, Result};
use nova_driver_support::manifest::{DepSource, Manifest};
use nova_driver_support::git_cache;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

/// The staged subtree root of every dependency module: `_deps/<module path
/// with '/' separators>/`. No dots in the name on purpose: the seed's
/// relative-import reader splits the path at the first `.`, so a dotted
/// directory name would truncate the edge target.
const DEPS_PREFIX: &str = "_deps";

/// The dependency roots of the build graph: package name -> the dependency's
/// source root on disk (a `path` checkout, a `[replace]` target, or a git
/// cache checkout). Transitive: a dependency's own dependencies resolve the
/// same way, with the BUILD ROOT's `[replace]` consulted for every edge (Go
/// scope), as the oracle's `imports::lookup_dependency` does.
pub(crate) struct DepRoots {
    roots: HashMap<String, PathBuf>,
}

impl DepRoots {
    fn source_root(&self, package_name: &str) -> Option<&Path> {
        self.roots.get(package_name).map(PathBuf::as_path)
    }
}

/// Collect the dependency roots of the package described by
/// `root_manifest`. Unresolvable edges (a missing checkout, a manifest that
/// will not parse, a name mismatch) are skipped here: when the program
/// actually imports such a dependency the seed refuses the unhanded module
/// by name, which is the same honest refusal an unresolved import gives
/// today.
pub(crate) fn collect_dep_roots(root_manifest: &Manifest) -> DepRoots {
    let mut roots: HashMap<String, PathBuf> = HashMap::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(normalize_path(&root_manifest.manifest_dir));
    let mut queue: Vec<Manifest> = vec![root_manifest.clone()];
    while let Some(manifest) = queue.pop() {
        for dep in &manifest.dependencies {
            // The root's `[replace]` (including `nova.override.toml`)
            // overlays every edge of the graph, not only the root's own;
            // its relative paths resolve against the ROOT's directory.
            let (source, base) = match root_manifest.replace.get(&dep.name) {
                Some(src) => (src.clone(), root_manifest.manifest_dir.clone()),
                None => (dep.source.clone(), manifest.manifest_dir.clone()),
            };
            let dir = match &source {
                DepSource::Path(rel) => normalize_path(&base.join(rel)),
                DepSource::Git { url, pin } => match git_cache::resolve_git_dep(url, pin, None) {
                    Ok(resolved) => resolved.checkout,
                    Err(_) => continue,
                },
                DepSource::Registry(_) | DepSource::Invalid(_) => continue,
            };
            let toml = dir.join("nova.toml");
            let Some(dep_manifest) =
                nova_driver_support::manifest::parse_manifest(&toml, &dir)
            else {
                continue;
            };
            if dep_manifest.package_name != dep.name {
                continue;
            }
            roots
                .entry(dep.name.clone())
                .or_insert_with(|| dep_manifest.source_root.clone());
            if seen.insert(normalize_path(&dir)) {
                queue.push(dep_manifest);
            }
        }
    }
    DepRoots { roots }
}

/// One source file of the staged program: where it is read from and where it
/// lands inside the stage tree (relative, `/`-separated).
pub(crate) struct ProgramFile {
    pub source: PathBuf,
    pub staged: String,
}

/// The result of the program walk: the files to stage (the entry NOT among
/// them) and, per importing file, the import tokens that name a staged
/// dependency module and must be rewritten to that module's relative staged
/// path (`token as written` -> `module dotted name`).
pub(crate) struct Discovery {
    pub files: Vec<ProgramFile>,
    pub rewrites: HashMap<PathBuf, Vec<(String, String)>>,
}

/// The program's source files, discovered from the entry: the entry's
/// folder-module peers plus the transitive closure of its relative,
/// own-package and dependency imports. The entry itself is NOT part of
/// `files`.
pub(crate) fn discover_program_files(
    entry: &Path,
    manifest: &Manifest,
    deps: &DepRoots,
) -> Result<Discovery> {
    let mut known: BTreeSet<PathBuf> = BTreeSet::new();
    let mut staged: HashMap<PathBuf, String> = HashMap::new();
    let mut rewrites: HashMap<PathBuf, Vec<(String, String)>> = HashMap::new();
    known.insert(normalize_path(entry));
    let mut queue: Vec<PathBuf> = vec![entry.to_path_buf()];

    // Peers of the entry's own folder-module share its namespace without an
    // import (D78), so no scan of imports can find them: they are the files
    // of the entry's directory declaring the same `module` line. A `_slow`
    // entry keeps its slow peers (the oracle's entry-sibling scan).
    if let Some(module) = module_line(&read_text(entry)?) {
        if let Some(dir) = entry.parent() {
            let entry_is_slow = entry
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.ends_with("_slow"));
            for peer in module_peer_files(dir, &module, entry_is_slow)? {
                let peer = normalize_path(&peer);
                if known.insert(peer.clone()) {
                    staged.insert(peer.clone(), staged_own(&peer, manifest));
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
            if let Some((dep_module, members)) = resolve_dep_import(&import, deps)? {
                // A dependency module: staged one directory per module, and
                // the importing file's token is rewritten to that
                // directory's relative path at staging time.
                for member in &members {
                    let member = normalize_path(member);
                    if known.insert(member.clone()) {
                        let staged_dir = dep_staged_dir(&dep_module);
                        let name = member
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        staged.insert(member.clone(), format!("{staged_dir}/{name}"));
                        queue.push(member);
                    }
                }
                if !members.is_empty() {
                    rewrites
                        .entry(normalize_path(&file))
                        .or_default()
                        .push((import, dep_module));
                }
                continue;
            }
            for candidate in resolve_import(&import, file_dir, manifest) {
                for member in module_files(&candidate) {
                    let member = normalize_path(&member);
                    if known.insert(member.clone()) {
                        staged.insert(member.clone(), staged_own(&member, manifest));
                        queue.push(member);
                    }
                }
            }
        }
    }
    known.remove(&normalize_path(entry));
    staged.remove(&normalize_path(entry));
    let files = known
        .into_iter()
        .map(|source| {
            let staged_path = staged
                .get(&source)
                .cloned()
                .unwrap_or_else(|| staged_own(&source, manifest));
            ProgramFile {
                source,
                staged: staged_path,
            }
        })
        .collect();
    Ok(Discovery { files, rewrites })
}

/// Stage the entry and the discovered program files under `stage_root`.
/// Package files keep their package-relative layout; dependency modules land
/// under `_deps/<module path>/`, one directory per module. Copies of files
/// whose imports name a staged dependency module are written with those
/// imports rewritten to the relative staged path (the user's sources are
/// never touched). Returns the staged entry.
pub(crate) fn stage_program(
    stage_root: &Path,
    source_base: &Path,
    entry: &Path,
    discovery: &Discovery,
) -> Result<PathBuf> {
    if stage_root.exists() {
        std::fs::remove_dir_all(stage_root).with_context(|| {
            format!("cannot clear stale program stage {}", stage_root.display())
        })?;
    }
    let entry_rel = relative_to_base(entry, source_base);
    let mut staged_entry = PathBuf::new();
    for (source, rel) in std::iter::once((entry, entry_rel.as_str())).chain(
        discovery
            .files
            .iter()
            .map(|file| (file.source.as_path(), file.staged.as_str())),
    ) {
        let target = stage_root.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("cannot create program stage directory {}", parent.display())
            })?;
        }
        let bytes = std::fs::read(source)
            .with_context(|| format!("cannot read {}", source.display()))?;
        let staged_dir = parent_dir_of(rel);
        let out: Vec<u8> = match discovery.rewrites.get(&normalize_path(source)) {
            Some(pairs) => {
                // A rewritten copy: decode (the BOM is not part of the
                // text), rewrite, and re-emit with the BOM restored, so a
                // BOM-carrying source keeps it exactly as a plain copy
                // would have.
                let bom = bytes.starts_with(b"\xef\xbb\xbf");
                let text = String::from_utf8(if bom { bytes[3..].to_vec() } else { bytes })
                    .with_context(|| {
                        format!("program source is not UTF-8: {}", source.display())
                    })?;
                let mut rewritten = rewrite_imports(&text, &staged_dir, pairs).into_bytes();
                if bom {
                    let mut with_bom = b"\xef\xbb\xbf".to_vec();
                    with_bom.append(&mut rewritten);
                    with_bom
                } else {
                    rewritten
                }
            }
            // No rewrite: the staged copy is byte for byte the source.
            None => bytes,
        };
        std::fs::write(&target, out).with_context(|| {
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

/// The staged path of an own-package file: its layout relative to the
/// package's source root, `/`-separated; a file outside the source root
/// keeps its file name only (Carina B1 reports any module it then cannot
/// reach).
fn staged_own(source: &Path, manifest: &Manifest) -> String {
    relative_to_base(source, &manifest.source_root)
}

fn relative_to_base(source: &Path, base: &Path) -> String {
    match source.strip_prefix(base) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

/// The staged directory of a dependency module: `_deps/<dotted module path
/// as directories>`.
fn dep_staged_dir(module: &str) -> String {
    format!("{DEPS_PREFIX}/{}", module.replace('.', "/"))
}

/// The directory part of a staged relative path (`""` for the stage root).
fn parent_dir_of(rel: &str) -> String {
    match rel.rfind('/') {
        Some(at) => rel[..at].to_owned(),
        None => String::new(),
    }
}

/// Rewrite every import token of `text` that `pairs` lists to the relative
/// path of the dependency module's staged directory, keeping the rest of the
/// line (the brace list included) byte for byte. The relative path always
/// ends with the module's last segment, because the seed's import visibility
/// matches on exactly that (274.11 E.10's named simplification).
fn rewrite_imports(text: &str, staged_dir: &str, pairs: &[(String, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    for raw_line in text.split_inclusive('\n') {
        let (body, eol) = match raw_line.strip_suffix('\n') {
            Some(without_lf) => match without_lf.strip_suffix('\r') {
                Some(without_cr) => (without_cr, "\r\n"),
                None => (without_lf, "\n"),
            },
            None => (raw_line, ""),
        };
        out.push_str(&rewrite_import_line(body, staged_dir, pairs));
        out.push_str(eol);
    }
    out
}

fn rewrite_import_line(body: &str, staged_dir: &str, pairs: &[(String, String)]) -> String {
    let indent = body.len() - body.trim_start().len();
    let after = &body[indent..];
    if after.starts_with("//") {
        return body.to_owned();
    }
    let keyword = if after.starts_with("import ") {
        "import ".len()
    } else if after.starts_with("export import ") {
        "export import ".len()
    } else {
        return body.to_owned();
    };
    let token_start = indent + keyword;
    let token_len: usize = body[token_start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '\\'))
        .map(char::len_utf8)
        .sum();
    let token_raw = &body[token_start..token_start + token_len];
    let token = token_raw.trim_end_matches('.');
    for (written, module) in pairs {
        if written == token {
            if let Some(rel) = rel_module_dir(staged_dir, &dep_staged_dir(module)) {
                // `token.len()`, not `token_raw.len()`: a trailing dot before
                // the brace list stays in place.
                return format!(
                    "{}{}{}",
                    &body[..token_start],
                    rel,
                    &body[token_start + token.len()..]
                );
            }
        }
    }
    body.to_owned()
}

/// The relative import path from staged directory `from_dir` to staged
/// directory `to_dir`: `./`/`../`-prefixed, ending with `to_dir`'s last
/// segment even when `to_dir` is an ancestor of `from_dir` (a bare `../..`
/// carries no segment, and the seed's visibility match needs the module's
/// last one). `None` when both are the same directory (a module never
/// imports itself).
fn rel_module_dir(from_dir: &str, to_dir: &str) -> Option<String> {
    if from_dir == to_dir {
        return None;
    }
    let from: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = to_dir.split('/').filter(|s| !s.is_empty()).collect();
    let mut common = 0;
    while common < from.len() && common < to.len() && from[common] == to[common] {
        common += 1;
    }
    let ups = from.len() - common;
    let mut down: Vec<&str> = to[common..].to_vec();
    let mut out = String::new();
    if down.is_empty() {
        // The target is an ancestor: name it from its parent so the last
        // segment survives (`../dep`, never a bare `..`).
        if ups == 0 {
            return None;
        }
        for _ in 0..ups - 1 {
            out.push_str("../");
        }
        down = to[common - 1..].to_vec();
        out.push_str("../");
        out.push_str(&down.join("/"));
        return Some(out);
    }
    if ups == 0 {
        out.push_str("./");
    } else {
        for _ in 0..ups {
            out.push_str("../");
        }
    }
    out.push_str(&down.join("/"));
    Some(out)
}

/// Resolve an import token against the dependency roots. `Some((module,
/// files))` when the token's first segment names a dependency: the module's
/// dotted name (the token itself, or the module line of the resolved files)
/// and the module's source files. `None` for relative, own-package, std and
/// unknown imports — the caller's own-package scan answers those.
fn resolve_dep_import(token: &str, deps: &DepRoots) -> Result<Option<(String, Vec<PathBuf>)>> {
    if token.starts_with("./") || token.starts_with("../") || token.starts_with("std.") || token == "std" {
        return Ok(None);
    }
    let first = token.split(['.', '/']).next().unwrap_or("");
    let Some(source_root) = deps.source_root(first) else {
        return Ok(None);
    };
    let rest = token
        .strip_prefix(first)
        .unwrap_or("")
        .trim_start_matches(['.', '/']);
    let members = if rest.is_empty() {
        // A bare dependency name addresses the dependency's root peers: the
        // files directly in its source root declaring `module <name>` (D78
        // rev-4, mirroring the oracle's `collect_root_peers` — never slow,
        // never test, current target only).
        module_peer_files(source_root, first, false)?
    } else {
        let mut target = source_root.to_path_buf();
        for segment in rest.split(['.', '/', '\\']) {
            if !segment.is_empty() {
                target.push(segment);
            }
        }
        let mut candidates = vec![target.clone()];
        if target.extension().is_none() {
            candidates.push(target.with_extension("nv"));
        }
        let mut out = Vec::new();
        for candidate in &candidates {
            out.extend(module_files(candidate));
        }
        out
    };
    if members.is_empty() {
        // The dependency has no such module: nothing is staged, and the
        // seed's refusal names the unhanded module as it does today.
        return Ok(None);
    }
    // The staged layout keys on the module the FILES declare, not on the
    // token's spelling: the two agree in a D78-clean package, and the
    // file's own word is the safer one when they do not.
    let module = module_line(&read_text(&members[0])?).unwrap_or_else(|| token.to_owned());
    Ok(Some((module, members)))
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
            // packages are the dependency roots `resolve_dep_import` stages
            // (module docs above).
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
                        && path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(peer_file_included)
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

/// Peer-file inclusion for a build-time scan, mirroring the oracle's
/// `peer_file_included` (imports.rs, [M-d376-slow-suffix-folder-module-peer-merge])
/// with `include_test_peers = false`: a `*.nv` name counts unless it carries
/// the `_slow` suffix (a self-contained slow-lane entry, never part of
/// someone else's compile unit), the `_test` suffix, or an OS suffix for a
/// target other than the current one. Suffixes peel outermost first:
/// `<core>[_<os>][_test][_slow]`.
fn peer_file_included(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".nv") else {
        return false;
    };
    if stem.strip_suffix("_slow").is_some() {
        return false;
    }
    if stem.strip_suffix("_test").is_some() {
        return false;
    }
    peer_active_for_target(stem, current_target_os())
}

/// The `_slow`-peeled, `_test`-agnostic inclusion used for the peers of a
/// slow-lane ENTRY (its folder peers merge as usual per the oracle's
/// entry-sibling scan): same as [`peer_file_included`] but `_slow` stays.
fn slow_entry_peer_included(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".nv") else {
        return false;
    };
    let no_slow = stem.strip_suffix("_slow").unwrap_or(stem);
    if no_slow.strip_suffix("_test").is_some() {
        return false;
    }
    peer_active_for_target(no_slow, current_target_os())
}

/// Plan 42.12 Ф.1 filename OS-suffix filter, mirrored from the oracle's
/// `peer_active_for_target`: no suffix is always active; `_unix`/`_posix`
/// cover linux, macos and unix targets.
fn peer_active_for_target(stem: &str, target: &str) -> bool {
    let suffix = if stem.ends_with("_windows") {
        Some("windows")
    } else if stem.ends_with("_linux") {
        Some("linux")
    } else if stem.ends_with("_macos") {
        Some("macos")
    } else if stem.ends_with("_unix") {
        Some("unix")
    } else if stem.ends_with("_posix") {
        Some("posix")
    } else {
        None
    };
    match suffix {
        None => true,
        Some("windows") => target == "windows",
        Some("linux") => target == "linux",
        Some("macos") => target == "macos",
        Some("unix") | Some("posix") => {
            target == "linux" || target == "macos" || target == "unix"
        }
        Some(_) => true,
    }
}

/// The target OS of the filename suffix filter: `NOVA_TARGET_OS` wins
/// (validated like the oracle's `current_target_os`), else the host — the
/// Carina build is pinned to windows-x86_64, so the host IS the target.
fn current_target_os() -> &'static str {
    if let Ok(value) = std::env::var("NOVA_TARGET_OS") {
        return match value.as_str() {
            "windows" => "windows",
            "linux" => "linux",
            "macos" => "macos",
            "unix" | "posix" => "unix",
            _ => "unknown",
        };
    }
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_family = "unix") {
        "unix"
    } else {
        "unknown"
    }
}

/// The peer files of a folder-module directory declaring `module` — used for
/// the entry's own directory, where every file is its own module unless it
/// declares the entry's module (D78: peers declare one shared name), and for
/// a dependency's root peers (a bare `import dep`). `include_slow` is set
/// only for the folder of a `_slow` entry itself (the oracle's entry-sibling
/// scan); every import/library resolution passes `false`
/// ([M-d376-slow-suffix-folder-module-peer-merge]).
fn module_peer_files(dir: &Path, module: &str, include_slow: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("cannot list the entry's directory {}", dir.display()))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let included = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
            if include_slow {
                slow_entry_peer_included(n)
            } else {
                peer_file_included(n)
            }
        });
        if !included {
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
    use super::{
        collect_dep_roots, discover_program_files, module_line, peer_active_for_target,
        rel_module_dir, rewrite_imports, scan_import_paths, stage_program, DepRoots,
    };
    use nova_driver_support::manifest::Manifest;
    use std::collections::HashMap;
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

    fn no_deps() -> DepRoots {
        DepRoots {
            roots: HashMap::new(),
        }
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
        let found = discover_program_files(&entry, &manifest(&dir), &no_deps()).unwrap();
        let sources: Vec<PathBuf> = found.files.iter().map(|f| f.source.clone()).collect();
        assert_eq!(
            sources,
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
        let found = discover_program_files(&entry, &manifest(&dir), &no_deps()).unwrap();
        let sources: Vec<PathBuf> = found.files.iter().map(|f| f.source.clone()).collect();
        assert_eq!(sources, vec![dir.join("two.nv")]);
        remove_dir(&dir);
    }

    #[test]
    fn staging_mirrors_the_package_relative_layout() {
        let dir = test_dir();
        write(
            &dir.join("nova.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\n[lib]\nsrc = \"src\"\n",
        );
        let manifest =
            nova_driver_support::manifest::parse_manifest(&dir.join("nova.toml"), &dir).unwrap();
        let entry = dir.join("src/main.nv");
        let helper = dir.join("src/helper/helper.nv");
        write(&entry, "module probe.main\nimport ./helper.{help}\n");
        write(&helper, "module probe.helper\n");
        let stage = dir.join("stage");
        let discovery = discover_program_files(&entry, &manifest, &no_deps()).unwrap();
        let staged = stage_program(&stage, &dir.join("src"), &entry, &discovery).unwrap();
        assert_eq!(staged, stage.join("main.nv"));
        assert!(stage.join("helper/helper.nv").is_file());
        remove_dir(&dir);
    }

    #[test]
    fn dependency_modules_are_staged_one_directory_per_module_with_rewritten_imports() {
        let dir = test_dir();
        // The root package `probe` with a path dependency `dep`.
        let root = dir.join("root");
        let dep = dir.join("dep");
        write(
            &root.join("nova.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ndep = { path = \"../dep\" }\n",
        );
        write(
            &dep.join("nova.toml"),
            "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n[lib]\nsrc = \"src\"\n",
        );
        let entry = root.join("main.nv");
        write(
            &entry,
            "module probe.main\nimport dep.{greet}\nimport dep.types.{T}\n\
             fn main() { println(greet()) }\n",
        );
        write(
            &dep.join("src/dep.nv"),
            "module dep\nimport dep.types.{T}\nexport fn greet() -> int => 1\n",
        );
        write(&dep.join("src/types.nv"), "module dep.types\nexport type T { n int }\n");
        // A test file of the dependency is not an interface.
        write(&dep.join("src/types_test.nv"), "module dep.types\nfn t() {}\n");
        // A `_slow` root peer is a self-contained slow-lane entry, never
        // part of an importer's unit ([M-d376-slow-suffix-folder-module-peer-merge]).
        write(
            &dep.join("src/leak_test_slow.nv"),
            "module dep\nfn slow_lane() {}\n",
        );

        let root_manifest =
            nova_driver_support::manifest::parse_manifest(&root.join("nova.toml"), &root).unwrap();
        let deps = collect_dep_roots(&root_manifest);
        assert_eq!(deps.source_root("dep"), Some(dep.join("src").as_path()));

        let found = discover_program_files(&entry, &root_manifest, &deps).unwrap();
        let mut staged_paths: Vec<String> =
            found.files.iter().map(|f| f.staged.clone()).collect();
        staged_paths.sort();
        assert_eq!(staged_paths, vec!["_deps/dep/dep.nv", "_deps/dep/types/types.nv"]);

        let stage = dir.join("stage");
        let staged_entry = stage_program(&stage, &root, &entry, &found).unwrap();
        let staged_entry_text = std::fs::read_to_string(&staged_entry).unwrap();
        assert!(
            staged_entry_text.contains("import ./_deps/dep.{greet}"),
            "staged entry: {staged_entry_text}"
        );
        assert!(
            staged_entry_text.contains("import ./_deps/dep/types.{T}"),
            "staged entry: {staged_entry_text}"
        );
        // The dependency's own qualified import became relative between the
        // staged module directories.
        let staged_dep = std::fs::read_to_string(stage.join("_deps/dep/dep.nv")).unwrap();
        assert!(
            staged_dep.contains("import ./types.{T}"),
            "staged dep module: {staged_dep}"
        );
        // The user's sources keep their text byte for byte.
        assert!(std::fs::read_to_string(&entry)
            .unwrap()
            .contains("import dep.{greet}"));
        remove_dir(&dir);
    }

    #[test]
    fn staging_preserves_the_bom_of_rewritten_and_plain_copies() {
        let dir = test_dir();
        let root = dir.join("root");
        let dep = dir.join("dep");
        write(
            &root.join("nova.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ndep = { path = \"../dep\" }\n",
        );
        write(
            &dep.join("nova.toml"),
            "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n",
        );
        let entry = root.join("main.nv");
        // BOM on the importing entry AND on the imported dependency module.
        std::fs::write(
            &entry,
            b"\xef\xbb\xbfmodule probe.main\nimport dep.{greet}\nfn main() {}\n",
        )
        .unwrap();
        std::fs::write(&dep.join("dep.nv"), b"\xef\xbb\xbfmodule dep\nexport fn greet() -> int => 1\n")
            .unwrap();
        let root_manifest =
            nova_driver_support::manifest::parse_manifest(&root.join("nova.toml"), &root).unwrap();
        let deps = collect_dep_roots(&root_manifest);
        let found = discover_program_files(&entry, &root_manifest, &deps).unwrap();
        let stage = dir.join("stage");
        let staged_entry = stage_program(&stage, &root, &entry, &found).unwrap();
        // The rewritten copy keeps its BOM and carries the rewritten import.
        let entry_bytes = std::fs::read(&staged_entry).unwrap();
        assert!(entry_bytes.starts_with(b"\xef\xbb\xbf"), "entry BOM lost");
        assert!(
            String::from_utf8_lossy(&entry_bytes).contains("import ./_deps/dep.{greet}"),
            "entry rewrite missing: {}",
            String::from_utf8_lossy(&entry_bytes)
        );
        // The plain copy is byte for byte the source.
        let staged_dep = std::fs::read(stage.join("_deps/dep/dep.nv")).unwrap();
        assert_eq!(
            staged_dep,
            b"\xef\xbb\xbfmodule dep\nexport fn greet() -> int => 1\n"
        );
        remove_dir(&dir);
    }

    #[test]
    fn peer_suffix_filter_matches_the_oracle_matrix() {
        // peer_active_for_target mirrors imports.rs (Plan 42.12 Ф.1).
        assert!(peer_active_for_target("util", "windows"));
        assert!(peer_active_for_target("util_windows", "windows"));
        assert!(!peer_active_for_target("util_linux", "windows"));
        assert!(!peer_active_for_target("util_windows", "linux"));
        assert!(peer_active_for_target("util_unix", "linux"));
        assert!(peer_active_for_target("util_posix", "macos"));
        assert!(peer_active_for_target("util_unix", "unix"));
        assert!(!peer_active_for_target("util_unix", "windows"));
        assert!(!peer_active_for_target("util_macos", "linux"));
    }

    #[test]
    fn rewrite_keeps_the_module_last_segment_for_ancestor_targets() {
        // From `_deps/dep/sub` to `_deps/dep` the path must spell `../dep`,
        // never a bare `..` (the seed matches the last segment).
        assert_eq!(
            rel_module_dir("_deps/dep/sub", "_deps/dep"),
            Some("../dep".to_owned())
        );
        assert_eq!(
            rel_module_dir("", "_deps/dep/types"),
            Some("./_deps/dep/types".to_owned())
        );
        assert_eq!(
            rel_module_dir("_deps/dep", "_deps/dep/types"),
            Some("./types".to_owned())
        );
        assert_eq!(
            rel_module_dir("_deps/a/x", "_deps/b"),
            Some("../../b".to_owned())
        );
        assert_eq!(rel_module_dir("_deps/dep", "_deps/dep"), None);
    }

    #[test]
    fn rewrite_touches_only_the_listed_tokens_and_keeps_line_endings() {
        let text = "module probe.main\r\n\
                    import dep.types.{T}\r\n\
                    import std.fs.{read}\r\n\
                    // import dep.types.{T} in a comment stays\r\n\
                    fn main() {}\r\n";
        let pairs = vec![("dep.types".to_owned(), "dep.types".to_owned())];
        let rewritten = rewrite_imports(text, "", &pairs);
        assert!(rewritten.contains("import ./_deps/dep/types.{T}\r\n"));
        assert!(rewritten.contains("import std.fs.{read}\r\n"));
        assert!(rewritten.contains("// import dep.types.{T} in a comment stays\r\n"));
    }
}
