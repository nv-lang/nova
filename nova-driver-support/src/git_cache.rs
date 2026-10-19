//! Shared user-data root used by the driver package cache and proxy config.
use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub fn nova_home_dir() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("NOVA_HOME") {
        return Ok(PathBuf::from(home));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            anyhow!(
                "не удалось определить домашнюю директорию Nova \
                 (ни HOME, ни USERPROFILE, ни NOVA_HOME не заданы)"
            )
        })?;
    Ok(PathBuf::from(home).join(".nova"))
}
