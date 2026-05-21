//! Plan 45 Ф.34.2 — incremental cache для `--watch` mode.
//!
//! Closes Ф.30.2 deferred decision. Mtime-based AST cache:
//! - WatchCache holds `BTreeMap<PathBuf, (SystemTime, Arc<Module>)>`.
//! - parse_with_cache(path) → check mtime, re-parse если changed, else reuse cached.
//! - `clear()` для manual invalidation.
//!
//! **Why Arc<Module> не Module:** doc pipeline пожирает `&Module` через
//! `doc::build(&module)`. Sharing cached parse результата между tick'ами
//! не требует mut access; `Arc::clone` cheap. Caller derefs `&*arc` для
//! `&Module` API.
//!
//! **Caveats:**
//! - mtime resolution OS-dependent (часто секунда). Очень быстрые edits
//!   могут не trigger re-parse — это known limitation в watch tools.
//! - Cache не считает import-graph dependencies. Если module A imports B
//!   и B changed, A не invalidated. Для single-file `--watch` это OK
//!   (один file = один module).
//! - File not found → cache eviction; subsequent call returns Err.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use crate::ast::Module;

/// Plan 45 Ф.34.2 — mtime-based cache для parsed Module objects.
#[derive(Debug, Default)]
pub struct WatchCache {
    entries: BTreeMap<PathBuf, CacheEntry>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    mtime: SystemTime,
    module: Arc<Module>,
}

/// Outcome когда parse_with_cache вызван — для logging / metrics в watch loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheOutcome {
    /// File не в cache — parsed first time.
    Miss,
    /// File в cache, mtime same — reused cached Arc.
    Hit,
    /// File в cache, mtime changed — re-parsed.
    Stale,
}

impl WatchCache {
    pub fn new() -> Self { Self::default() }

    /// Plan 45 Ф.34.2 — get-or-parse helper.
    ///
    /// Reads file mtime, compares с cached. Если miss/stale — re-reads source,
    /// re-parses через `parser::parse`, updates cache. Returns Arc<Module> +
    /// outcome (для metrics).
    ///
    /// Source string also returned (полезно для callers которые передают
    /// source в test_runner / render_html_with_source).
    pub fn parse_with_cache(
        &mut self,
        path: &Path,
    ) -> Result<(Arc<Module>, String, CacheOutcome), CacheError> {
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map_err(|e| CacheError::Io(format!("mtime для {}: {}", path.display(), e)))?;
        // Cache hit с unchanged mtime.
        if let Some(entry) = self.entries.get(path) {
            if entry.mtime == mtime {
                // Hit: reuse cached Arc. Re-read source чтобы returned source
                // matched (cache не stores source — only Module).
                let src = std::fs::read_to_string(path)
                    .map_err(|e| CacheError::Io(format!("read {}: {}", path.display(), e)))?;
                return Ok((Arc::clone(&entry.module), src, CacheOutcome::Hit));
            }
        }
        // Miss или stale — re-parse.
        let was_present = self.entries.contains_key(path);
        let src = std::fs::read_to_string(path)
            .map_err(|e| CacheError::Io(format!("read {}: {}", path.display(), e)))?;
        let mut module = crate::parser::parse(&src)
            .map_err(|d| CacheError::Parse(d.render(&src, &path.display().to_string())))?;
        // Type-check + infer effects (тот же pipeline что cmd_doc делает).
        let _ = crate::types::check_module(&module);
        crate::types::infer_effects(&mut module);
        // Plan 71 / D127: seed peer_files[0].path для doc::collect →
        // DocModule.source_paths → fixture-detection в lints. Parser
        // оставляет peer_files пустым; `nova doc` watch single-file mode
        // не зовёт resolve_imports_inline, поэтому seed'им здесь.
        if module.peer_files.is_empty() {
            module.peer_files.push(crate::ast::PeerFile {
                path: path.to_path_buf(),
                file_id: crate::diag::MAIN_FILE_ID,
                imports: module.imports.clone(),
                items_here: module.items.clone(),
                imported_item_names: std::collections::HashSet::new(),
                is_entry_module: true,
                module_name: module.name.clone(),
            });
        }
        let arc = Arc::new(module);
        self.entries.insert(path.to_path_buf(), CacheEntry {
            mtime,
            module: Arc::clone(&arc),
        });
        let outcome = if was_present { CacheOutcome::Stale } else { CacheOutcome::Miss };
        Ok((arc, src, outcome))
    }

    /// Plan 45 Ф.34.2 — manual invalidation (для cases когда external trigger).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Plan 45 Ф.34.2 — remove single file entry (e.g. file deleted).
    pub fn evict(&mut self, path: &Path) -> bool {
        self.entries.remove(path).is_some()
    }

    /// Plan 45 Ф.34.2 — current cache size (entries count).
    pub fn len(&self) -> usize { self.entries.len() }

    /// Plan 45 Ф.34.2 — true если cache empty.
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

#[derive(Debug)]
pub enum CacheError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::Io(msg) => write!(f, "I/O error: {}", msg),
            CacheError::Parse(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for CacheError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    fn temp_path(name: &str) -> PathBuf {
        let pid = std::process::id();
        let tid = format!("{:?}", std::thread::current().id())
            .replace(['(', ')', ' '], "_");
        let dir = std::env::temp_dir()
            .join("nova_watch_cache_tests")
            .join(format!("{}_{}", pid, tid));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn write_file(path: &Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    #[test]
    fn first_call_is_miss() {
        let path = temp_path("test_miss.nv");
        write_file(&path, "module m\nexport fn f() -> int => 1\n");
        let mut cache = WatchCache::new();
        let (_arc, _src, outcome) = cache.parse_with_cache(&path).expect("parse");
        assert_eq!(outcome, CacheOutcome::Miss);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn second_call_unchanged_is_hit() {
        let path = temp_path("test_hit.nv");
        write_file(&path, "module m\nexport fn f() -> int => 1\n");
        let mut cache = WatchCache::new();
        let (_, _, first) = cache.parse_with_cache(&path).unwrap();
        assert_eq!(first, CacheOutcome::Miss);
        let (_, _, second) = cache.parse_with_cache(&path).unwrap();
        assert_eq!(second, CacheOutcome::Hit);
    }

    #[test]
    fn mtime_change_triggers_re_parse() {
        let path = temp_path("test_stale.nv");
        write_file(&path, "module m\nexport fn f() -> int => 1\n");
        let mut cache = WatchCache::new();
        let (_, _, first) = cache.parse_with_cache(&path).unwrap();
        assert_eq!(first, CacheOutcome::Miss);
        // Подождать чтобы mtime resolution позволила обновление.
        std::thread::sleep(Duration::from_millis(1100));
        write_file(&path, "module m\nexport fn f() -> int => 2\n");
        let (_, _, second) = cache.parse_with_cache(&path).unwrap();
        assert_eq!(second, CacheOutcome::Stale);
    }

    #[test]
    fn clear_empties_cache() {
        let path = temp_path("test_clear.nv");
        write_file(&path, "module m\nexport fn f() -> int => 1\n");
        let mut cache = WatchCache::new();
        let _ = cache.parse_with_cache(&path);
        assert_eq!(cache.len(), 1);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn evict_removes_single() {
        let path1 = temp_path("test_evict1.nv");
        let path2 = temp_path("test_evict2.nv");
        write_file(&path1, "module m1\nexport fn f() -> int => 1\n");
        write_file(&path2, "module m2\nexport fn g() -> int => 2\n");
        let mut cache = WatchCache::new();
        let _ = cache.parse_with_cache(&path1);
        let _ = cache.parse_with_cache(&path2);
        assert_eq!(cache.len(), 2);
        assert!(cache.evict(&path1));
        assert_eq!(cache.len(), 1);
        assert!(!cache.evict(&path1)); // already evicted
    }

    #[test]
    fn missing_file_returns_err() {
        let mut cache = WatchCache::new();
        let r = cache.parse_with_cache(Path::new("/nonexistent/path/foo.nv"));
        assert!(r.is_err());
        assert!(matches!(r.unwrap_err(), CacheError::Io(_)));
    }

    #[test]
    fn malformed_source_returns_parse_err() {
        let path = temp_path("test_malformed.nv");
        write_file(&path, "this is not valid nova source !!!");
        let mut cache = WatchCache::new();
        let r = cache.parse_with_cache(&path);
        assert!(r.is_err());
        assert!(matches!(r.unwrap_err(), CacheError::Parse(_)));
    }

    #[test]
    fn hit_returns_same_arc_pointer() {
        // Verify zero-copy: Arc::clone не дублирует Module.
        let path = temp_path("test_arc.nv");
        write_file(&path, "module m\nexport fn f() -> int => 1\n");
        let mut cache = WatchCache::new();
        let (arc1, _, _) = cache.parse_with_cache(&path).unwrap();
        let (arc2, _, outcome) = cache.parse_with_cache(&path).unwrap();
        assert_eq!(outcome, CacheOutcome::Hit);
        assert!(Arc::ptr_eq(&arc1, &arc2), "hit должен return same Arc pointer");
    }
}
