//! Ф.5.1 (Plan 33.6): in-memory subsumption cache для повторяющихся VC.
//!
//! Module-level cache: одинаковые контракты (типичные: `x >= 0 || x < 0`,
//! `a + b == b + a` и пр.) в больших модулях с 100+ fn — проверяем 1 раз,
//! re-use для всех остальных fn.
//!
//! Cache scope: per-`verify_module` invocation. Не persistent (cache.rs —
//! отдельный on-disk cache по Ф.12).
//!
//! Canonical form (V1): xxhash64 от pretty-printed SmtTerm. Alpha-rename
//! не делаем (наивный V1). Достаточно для exact-match VCs.

use std::collections::HashMap;
use std::cell::RefCell;
use crate::verify::ir::SmtTerm;

// Ф.12.3 (Plan 33.6): module-scoped thread-local cache.
// Reset'ится в начале verify_module. Используется через with_cache helper.
thread_local! {
    static MODULE_CACHE: RefCell<SubsumptionCache> = RefCell::new(SubsumptionCache::new());
}

/// Reset module-scoped cache (вызывается в начале verify_module).
pub fn reset_module_cache() {
    MODULE_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        cache.map.clear();
        cache.hits = 0;
        cache.misses = 0;
    });
}

/// Get stats после verify_module (для diagnostic).
pub fn module_cache_stats() -> (u64, u64) {
    MODULE_CACHE.with(|c| {
        let cache = c.borrow();
        (cache.hits, cache.misses)
    })
}

/// Lookup в module cache (thread-local).
pub fn module_cache_lookup(term: &SmtTerm) -> Option<CachedSat> {
    MODULE_CACHE.with(|c| c.borrow_mut().lookup(term))
}

/// Insert в module cache (thread-local).
pub fn module_cache_insert(term: &SmtTerm, result: CachedSat) {
    MODULE_CACHE.with(|c| c.borrow_mut().insert(term, result))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CachedSat {
    Unsat,
    Sat,
    Unknown,
}

#[derive(Debug, Default)]
pub struct SubsumptionCache {
    pub map: HashMap<u64, CachedSat>,
    pub hits: u64,
    pub misses: u64,
}

impl SubsumptionCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lookup. Возвращает cached если есть.
    pub fn lookup(&mut self, term: &SmtTerm) -> Option<CachedSat> {
        let key = canonical_hash(term);
        if let Some(r) = self.map.get(&key) {
            self.hits += 1;
            Some(*r)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert.
    pub fn insert(&mut self, term: &SmtTerm, result: CachedSat) {
        let key = canonical_hash(term);
        self.map.insert(key, result);
    }

    /// Hit rate (для diagnostic / stats).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

/// Простой xxhash64-style hash (использует std hasher для V1; xxhash в V2).
fn canonical_hash(term: &SmtTerm) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let pretty = term.pretty();
    let mut h = DefaultHasher::new();
    pretty.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_smoke() {
        let mut c = SubsumptionCache::new();
        let t = SmtTerm::BoolLit(true);
        assert!(c.lookup(&t).is_none());
        c.insert(&t, CachedSat::Unsat);
        assert_eq!(c.lookup(&t), Some(CachedSat::Unsat));
        assert_eq!(c.hits, 1);
        assert_eq!(c.misses, 1);
        assert!((c.hit_rate() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn cache_different_terms() {
        let mut c = SubsumptionCache::new();
        let a = SmtTerm::IntLit(1);
        let b = SmtTerm::IntLit(2);
        c.insert(&a, CachedSat::Unsat);
        assert_eq!(c.lookup(&b), None);
        assert_eq!(c.lookup(&a), Some(CachedSat::Unsat));
    }
}
