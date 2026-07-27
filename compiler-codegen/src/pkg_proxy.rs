//! Plan 233 §1 — прокси для скачивания пакетов (git-fetch путь).
//!
//! Индустрия-канон: прокси — свойство СРЕДЫ разработчика, не пакета
//! (cargo: `~/.cargo/config.toml` `[http].proxy`, НЕ `Cargo.toml`; go:
//! `GOPROXY` env). В коммитимом `nova.toml` прокси **не поддерживается** —
//! осознанное решение (`docs/plans/233-pkg-tooling.md` §1): у каждого
//! разработчика/CI своя сеть, коммитимый прокси создал бы
//! переносимость-ловушку.
//!
//! **Слои резолва, ПЕРВЫЙ существующий выигрывает:**
//! 1. env `NOVA_PKG_PROXY`, либо стандартные `HTTPS_PROXY`/`HTTP_PROXY`
//!    (и lowercase-варианты) — `git` уважает их сам через unmodified env
//!    (`git_cache::run_git` НЕ вызывает `env_clear()`, наследует родителя),
//!    но `NOVA_PKG_PROXY` — не git-нативная переменная, поэтому резолв
//!    здесь всегда переводится в явный `-c http.proxy=<url>` для нашего
//!    `git`-вызова (см. `git_cache::run_git`) — детерминированно, вне
//!    зависимости от того, распознал бы git саму переменную сам.
//! 2. `[net] proxy = "..."` в НЕкоммитимом `nova.override.toml` (новое имя,
//!    Plan 233 §2а) — соседний с ближайшим вверх по дереву `nova.toml`.
//!    Legacy-имя `nova.local.toml` тоже читается (deprecation warning).
//! 3. `[net] proxy = "..."` в глобальном пользовательском
//!    `$NOVA_HOME/config.toml` (либо `~/.nova/config.toml`, если
//!    `NOVA_HOME` не задан) — минимальная реализация нового файла-слоя.

use std::path::{Path, PathBuf};

/// Источник резолвнутого значения — для диагностики/тестов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxySource {
    /// env `NOVA_PKG_PROXY`.
    EnvNovaPkgProxy,
    /// Один из стандартных env (`HTTPS_PROXY`/`HTTP_PROXY`/lowercase) —
    /// хранит имя переменной.
    EnvStd(String),
    /// `[net] proxy` из `nova.override.toml` (или legacy `nova.local.toml`,
    /// путь которого и хранится здесь).
    OverrideToml(PathBuf),
    /// `[net] proxy` из глобального `$NOVA_HOME/config.toml`.
    GlobalConfig(PathBuf),
}

/// Результат резолва — значение + слой-источник.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProxy {
    pub url: String,
    pub source: ProxySource,
}

/// Слой 1: `NOVA_PKG_PROXY`, иначе стандартные `HTTPS_PROXY`/`HTTP_PROXY`
/// (и lowercase-варианты — распространённая практика POSIX-тулинга
/// уважать оба регистра).
fn env_proxy() -> Option<ResolvedProxy> {
    if let Ok(v) = std::env::var("NOVA_PKG_PROXY") {
        let v = v.trim();
        if !v.is_empty() {
            return Some(ResolvedProxy {
                url: v.to_string(),
                source: ProxySource::EnvNovaPkgProxy,
            });
        }
    }
    for name in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(v) = std::env::var(name) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(ResolvedProxy {
                    url: v.to_string(),
                    source: ProxySource::EnvStd(name.to_string()),
                });
            }
        }
    }
    None
}

/// Разобрать `[net]` секцию — минимальный line-based TOML-парсер, тот же
/// стиль, что `manifest::parse_local_toml`. Возвращает `proxy`, если
/// секция `[net]` присутствует и содержит непустой `proxy = "..."`.
fn parse_net_proxy(text: &str) -> Option<String> {
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_start_matches('[').trim_end_matches(']').trim().to_string();
            continue;
        }
        if section != "net" {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            if key.trim() != "proxy" {
                continue;
            }
            let raw_val = val.trim();
            let raw_val = raw_val.split('#').next().unwrap_or("").trim();
            let v = raw_val.trim_matches('"').trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Ближайший вверх по дереву от `start_dir` каталог с `nova.toml` — тот же
/// поиск, что `manifest::find_package_dir`, но принимает директорию, а не
/// файл (здесь нет конкретного .nv-файла — резолв идёт от CWD процесса).
fn find_package_dir(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.canonicalize().unwrap_or_else(|_| start_dir.to_path_buf());
    loop {
        if dir.join("nova.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Slay 2: `[net] proxy` соседнего с `nova.toml` override-файла. Новое имя
/// (`nova.override.toml`) проверяется первым; если отсутствует —
/// legacy-имя (`nova.local.toml`, deprecation warning на stderr, как
/// `lockfile::load` для `nova.lock`).
fn override_toml_proxy(start_dir: &Path) -> Option<ResolvedProxy> {
    let pkg_dir = find_package_dir(start_dir)?;
    let new_path = pkg_dir.join("nova.override.toml");
    if new_path.is_file() {
        if let Ok(text) = std::fs::read_to_string(&new_path) {
            if let Some(p) = parse_net_proxy(&text) {
                return Some(ResolvedProxy { url: p, source: ProxySource::OverrideToml(new_path) });
            }
        }
    }
    let legacy_path = pkg_dir.join("nova.local.toml");
    if legacy_path.is_file() {
        if let Ok(text) = std::fs::read_to_string(&legacy_path) {
            if let Some(p) = parse_net_proxy(&text) {
                eprintln!(
                    "warning: {} устарел, переименуйте в nova.override.toml \
                     [W_OVERRIDE_TOML_DEPRECATED]",
                    legacy_path.display(),
                );
                return Some(ResolvedProxy { url: p, source: ProxySource::OverrideToml(legacy_path) });
            }
        }
    }
    None
}

/// Слой 3: `[net] proxy` глобального `$NOVA_HOME/config.toml` (либо
/// `~/.nova/config.toml`).
fn global_config_proxy() -> Option<ResolvedProxy> {
    let root = crate::git_cache::nova_home_dir().ok()?;
    let path = root.join("config.toml");
    if !path.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    parse_net_proxy(&text).map(|p| ResolvedProxy { url: p, source: ProxySource::GlobalConfig(path) })
}

/// Резолвнуть эффективный прокси для скачивания пакетов, слоями (первый
/// существующий выигрывает): env → `nova.override.toml`/`nova.local.toml`
/// (ближайший вверх от `start_dir`) → `~/.nova/config.toml`.
pub fn resolve_pkg_proxy(start_dir: &Path) -> Option<ResolvedProxy> {
    env_proxy()
        .or_else(|| override_toml_proxy(start_dir))
        .or_else(global_config_proxy)
}

/// Удобный вход для `git_cache::run_git` — резолв от CWD процесса
/// (`nova build`/`add`/`update` и т.п. запускаются внутри пакета либо его
/// поддиректории — тот же допущение, что `package_dir_from_cwd()` в
/// `nova-cli/src/main.rs`).
pub fn resolve_pkg_proxy_for_cwd() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    resolve_pkg_proxy(&cwd).map(|r| r.url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // env::set_var — процесс-глобальный side effect; тесты этого модуля
    // должны идти СЕРИАЛЬНО (иначе гонки между потоками cargo test).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nova_pkgproxy_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// SAFETY: сериализовано через `ENV_LOCK` в каждом тесте, использующем
    /// env-переменные этого модуля (`NOVA_PKG_PROXY`, `HTTPS_PROXY`,
    /// `HTTP_PROXY`, `NOVA_HOME`) — единственная точка мутации на тест,
    /// остальной тред только читает уже установленное значение.
    unsafe fn set_env(k: &str, v: &str) {
        unsafe { std::env::set_var(k, v) };
    }
    unsafe fn clear_env(k: &str) {
        unsafe { std::env::remove_var(k) };
    }

    fn clear_all_proxy_env() {
        for k in ["NOVA_PKG_PROXY", "HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NOVA_HOME"] {
            unsafe { clear_env(k) };
        }
    }

    #[test]
    fn parse_net_proxy_basic() {
        let text = "[net]\nproxy = \"http://proxy.local:8080\"\n";
        assert_eq!(parse_net_proxy(text).as_deref(), Some("http://proxy.local:8080"));
    }

    #[test]
    fn parse_net_proxy_missing_section() {
        let text = "[other]\nproxy = \"http://x\"\n";
        assert_eq!(parse_net_proxy(text), None);
    }

    #[test]
    fn parse_net_proxy_empty_value_ignored() {
        let text = "[net]\nproxy = \"\"\n";
        assert_eq!(parse_net_proxy(text), None);
    }

    #[test]
    fn no_layers_set_resolves_none() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("none");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(resolve_pkg_proxy(&dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Слой 1 (env NOVA_PKG_PROXY) побеждает слой 2 (override.toml), даже
    /// если override.toml объявляет свой [net] proxy.
    #[test]
    fn env_nova_pkg_proxy_wins_over_override_toml() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("env_wins_override");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nova.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(dir.join("nova.override.toml"), "[net]\nproxy = \"http://from-override:1\"\n").unwrap();

        unsafe { set_env("NOVA_PKG_PROXY", "http://from-env:2") };
        let r = resolve_pkg_proxy(&dir).expect("resolved");
        assert_eq!(r.url, "http://from-env:2");
        assert_eq!(r.source, ProxySource::EnvNovaPkgProxy);

        unsafe { clear_env("NOVA_PKG_PROXY") };
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Стандартный HTTPS_PROXY тоже слой 1 — побеждает override.toml.
    #[test]
    fn env_https_proxy_wins_over_override_toml() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("https_env_wins");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nova.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(dir.join("nova.override.toml"), "[net]\nproxy = \"http://from-override:1\"\n").unwrap();

        unsafe { set_env("HTTPS_PROXY", "http://from-https-env:3") };
        let r = resolve_pkg_proxy(&dir).expect("resolved");
        assert_eq!(r.url, "http://from-https-env:3");
        assert_eq!(r.source, ProxySource::EnvStd("HTTPS_PROXY".to_string()));

        unsafe { clear_env("HTTPS_PROXY") };
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Слой 2 (override.toml) побеждает слой 3 (~/.nova/config.toml, тут
    /// эмулирован через NOVA_HOME).
    #[test]
    fn override_toml_wins_over_global_config() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("override_wins_global");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nova.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(dir.join("nova.override.toml"), "[net]\nproxy = \"http://from-override:1\"\n").unwrap();

        let home = unique_dir("override_wins_global_home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("config.toml"), "[net]\nproxy = \"http://from-global:4\"\n").unwrap();
        unsafe { set_env("NOVA_HOME", &home.to_string_lossy()) };

        let r = resolve_pkg_proxy(&dir).expect("resolved");
        assert_eq!(r.url, "http://from-override:1");

        unsafe { clear_env("NOVA_HOME") };
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    /// Ничего в env, ничего в override.toml — слой 3 (глобальный config)
    /// побеждает по умолчанию (последний слой каскада).
    #[test]
    fn global_config_used_when_nothing_else_set() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("global_only");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nova.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\n").unwrap();

        let home = unique_dir("global_only_home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("config.toml"), "[net]\nproxy = \"http://from-global:5\"\n").unwrap();
        unsafe { set_env("NOVA_HOME", &home.to_string_lossy()) };

        let r = resolve_pkg_proxy(&dir).expect("resolved");
        assert_eq!(r.url, "http://from-global:5");
        assert_eq!(r.source, ProxySource::GlobalConfig(home.join("config.toml")));

        unsafe { clear_env("NOVA_HOME") };
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&home).ok();
    }

    /// legacy `nova.local.toml` [net] proxy тоже читается (deprecation
    /// warning идёт на stderr — не проверяется тут, см. §2 override-тесты
    /// в manifest.rs для собственно warning-контракта).
    #[test]
    fn legacy_local_toml_still_read_for_proxy() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("legacy_local");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nova.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(dir.join("nova.local.toml"), "[net]\nproxy = \"http://from-legacy:6\"\n").unwrap();

        let r = resolve_pkg_proxy(&dir).expect("resolved");
        assert_eq!(r.url, "http://from-legacy:6");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Новое имя (`nova.override.toml`) побеждает legacy (`nova.local.toml`)
    /// когда оба присутствуют в одном каталоге.
    #[test]
    fn new_override_name_wins_over_legacy_when_both_present() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("both_names");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nova.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\n").unwrap();
        std::fs::write(dir.join("nova.override.toml"), "[net]\nproxy = \"http://from-new:7\"\n").unwrap();
        std::fs::write(dir.join("nova.local.toml"), "[net]\nproxy = \"http://from-legacy:8\"\n").unwrap();

        let r = resolve_pkg_proxy(&dir).expect("resolved");
        assert_eq!(r.url, "http://from-new:7");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Committed `nova.toml` НЕ поддерживает `[net] proxy` — осознанное
    /// решение плана 233 §1; секция там должна просто игнорироваться (не
    /// резолвиться как прокси-слой).
    #[test]
    fn committed_nova_toml_net_proxy_is_ignored() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_all_proxy_env();
        let dir = unique_dir("committed_ignored");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("nova.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n[net]\nproxy = \"http://should-not-be-used:9\"\n",
        )
        .unwrap();

        assert_eq!(resolve_pkg_proxy(&dir), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
