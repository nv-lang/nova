//! Compatibility lock APIs retain the compiler's existing manifest snapshot.
//! The production driver calls the same support implementation with fresh reads.
//! This preserves parity, including the separately tracked stale-mutation defect;
//! it does not change D420 checks or grant permission to external path sources.
use anyhow::Result;
use nova_driver_support::lockfile as support;
use std::collections::HashMap;
use std::path::Path;
pub use support::{
    legacy_lock_path, load, lock_path, LockFile, LockedDep, LockedSource, LEGACY_LOCK_FILE_NAME,
    LOCK_FILE_NAME, LOCK_VERSION,
};

pub fn collect_dep_graph(entry_pkg_dir: &Path) -> Result<Vec<LockedDep>> {
    support::collect_dep_graph_with_provider(entry_pkg_dir, crate::manifest::parse_manifest)
}

pub fn collect_dep_graph_ex(
    entry_pkg_dir: &Path,
    resolved_versions: &HashMap<String, String>,
) -> Result<Vec<LockedDep>> {
    support::collect_dep_graph_ex_with_provider(
        entry_pkg_dir,
        resolved_versions,
        crate::manifest::parse_manifest,
    )
}

pub fn collect_replace_scope_warnings(
    entry_pkg_dir: &Path,
) -> Vec<crate::manifest::ManifestWarning> {
    support::collect_replace_scope_warnings_with_provider(
        entry_pkg_dir,
        crate::manifest::parse_manifest,
    )
}

pub fn sync(entry_pkg_dir: &Path) -> Result<Vec<LockedDep>> {
    support::sync_with_provider(entry_pkg_dir, crate::manifest::parse_manifest)
}

pub fn load_pins(entry_pkg_dir: &Path) -> Result<()> {
    support::load_pins_with_provider(entry_pkg_dir, crate::manifest::parse_manifest)
}

pub fn ensure_pins_loaded(pkg_dir: &Path) -> Result<()> {
    support::ensure_pins_loaded_with_provider(pkg_dir, crate::manifest::parse_manifest)
}

pub fn update(entry_pkg_dir: &Path, only: Option<&str>) -> Result<Vec<LockedDep>> {
    support::update_with_provider(entry_pkg_dir, only, crate::manifest::parse_manifest)
}

pub fn update_precise(
    entry_pkg_dir: &Path,
    dep_name: &str,
    dep_url: &str,
    version: &crate::semver::Version,
) -> Result<Vec<LockedDep>> {
    support::update_precise_with_provider(
        entry_pkg_dir,
        dep_name,
        dep_url,
        version,
        crate::manifest::parse_manifest,
    )
}
