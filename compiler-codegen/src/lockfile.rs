//! Plan 03.1 Ф.4 — `nova.lock`: фиксация графа зависимостей.
//!
//! `nova.lock` пинит точные версии всех (транзитивных) зависимостей —
//! воспроизводимая сборка. Коммитится в репозиторий (как `Cargo.lock`
//! для бинарей).
//!
//! **Формат** (минимальный TOML, парсер — ручной, как `manifest.rs`):
//!
//! ```text
//! version = 1
//!
//! [[package]]
//! name = "mathlib"
//! source = "path"
//! path = "../mathlib"
//!
//! [[package]]
//! name = "gitlib"
//! source = "git"
//! git = "https://example.org/gitlib.nv"
//! pin = "tag:v1.0.0"
//! commit = "a1b2c3d4e5f6..."
//! ```
//!
//! - `path`-deps: пин не нужен (локальны и мутабельны — берётся текущее
//!   содержимое). Запись — для полноты графа.
//! - `git`-deps: `commit` — точный 40-hex commit. Это **и есть**
//!   integrity-пин: git-commit криптографически адресует дерево
//!   исходников, подменить содержимое нельзя без смены commit'а
//!   (паритет с многолетним поведением `Cargo.lock`). Отдельный
//!   sha256-хэш дерева + подписи — supply-chain hardening Plan 03.4.
//! - Поле под effect-surface зависимости — **зарезервировано** (Plan
//!   03.4): неизвестные ключи парсер игнорирует, формат расширяем без
//!   breaking change.
//!
//! **Воспроизводимость.** `sync` загружает существующий `nova.lock` в
//! `git_cache`-таблицу пинов до резолва графа — git-зависимости с уже
//! зафиксированным commit'ом не резолвятся «вживую» (ветка не «уедет»).

use crate::git_cache;
use crate::manifest::{DepSource, GitPin};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Источник зафиксированной зависимости.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockedSource {
    /// `path`-зависимость: путь как записан в `nova.toml` (относительный).
    Path { path: String },
    /// `git`-зависимость: URL, исходный пин (информативно / для
    /// `nova update`) и резолвнутый точный commit.
    Git {
        url: String,
        pin: String,
        commit: String,
    },
}

/// Одна запись `nova.lock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedDep {
    pub name: String,
    pub source: LockedSource,
}

/// Разобранный / собранный `nova.lock`.
#[derive(Debug, Clone)]
pub struct LockFile {
    pub version: u32,
    /// Записи, отсортированные по имени — детерминированный вывод.
    pub packages: Vec<LockedDep>,
}

/// Текущая версия формата `nova.lock`.
pub const LOCK_VERSION: u32 = 1;

/// Строковое представление пина для записи в lockfile.
fn pin_str(pin: &GitPin) -> String {
    match pin {
        GitPin::Rev(r) => format!("rev:{}", r),
        GitPin::Tag(t) => format!("tag:{}", t),
        GitPin::Branch(b) => format!("branch:{}", b),
        GitPin::Version(req) => format!("version:{}", req),
        GitPin::Default => "default".to_string(),
    }
}

impl LockFile {
    /// Сериализовать в текст `nova.lock`.
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(
            "# nova.lock — сгенерирован автоматически (Plan 03.1 / D78).\n\
             # Фиксирует точные версии зависимостей для воспроизводимых\n\
             # сборок. Не редактируйте вручную; коммитьте в репозиторий.\n\n",
        );
        s.push_str(&format!("version = {}\n", self.version));
        for p in &self.packages {
            s.push_str("\n[[package]]\n");
            s.push_str(&format!("name = \"{}\"\n", p.name));
            match &p.source {
                LockedSource::Path { path } => {
                    s.push_str("source = \"path\"\n");
                    s.push_str(&format!("path = \"{}\"\n", path));
                }
                LockedSource::Git { url, pin, commit } => {
                    s.push_str("source = \"git\"\n");
                    s.push_str(&format!("git = \"{}\"\n", url));
                    s.push_str(&format!("pin = \"{}\"\n", pin));
                    s.push_str(&format!("commit = \"{}\"\n", commit));
                }
            }
        }
        s
    }

    /// Разобрать текст `nova.lock`. Неизвестные ключи игнорируются
    /// (forward-compat — Plan 03.4 расширит формат).
    pub fn parse(text: &str) -> Result<LockFile> {
        let mut version: u32 = LOCK_VERSION;
        let mut packages: Vec<LockedDep> = Vec::new();
        // Текущая собираемая запись `[[package]]`.
        let mut cur: Option<Vec<(String, String)>> = None;

        let finish = |cur: &mut Option<Vec<(String, String)>>,
                      packages: &mut Vec<LockedDep>|
         -> Result<()> {
            if let Some(fields) = cur.take() {
                packages.push(record_to_dep(&fields)?);
            }
            Ok(())
        };

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[[package]]" {
                finish(&mut cur, &mut packages)?;
                cur = Some(Vec::new());
                continue;
            }
            if line.starts_with('[') {
                // Прочие секции — игнорируем (forward-compat).
                finish(&mut cur, &mut packages)?;
                cur = None;
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim().to_string();
                let v = v.trim().trim_matches('"').to_string();
                match &mut cur {
                    Some(fields) => fields.push((k, v)),
                    None => {
                        if k == "version" {
                            version = v.parse().unwrap_or(LOCK_VERSION);
                        }
                    }
                }
            }
        }
        finish(&mut cur, &mut packages)?;
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(LockFile { version, packages })
    }

    /// git-записи как пары `(url, commit)` — для `git_cache` lock-таблицы.
    pub fn git_pins(&self) -> Vec<(String, String)> {
        self.packages
            .iter()
            .filter_map(|p| match &p.source {
                LockedSource::Git { url, commit, .. } => {
                    Some((url.clone(), commit.clone()))
                }
                LockedSource::Path { .. } => None,
            })
            .collect()
    }
}

/// Собрать `LockedDep` из пар ключ-значение записи `[[package]]`.
fn record_to_dep(fields: &[(String, String)]) -> Result<LockedDep> {
    let get = |k: &str| fields.iter().find(|(fk, _)| fk == k).map(|(_, v)| v.as_str());
    let name = get("name")
        .ok_or_else(|| anyhow!("nova.lock: запись [[package]] без `name`"))?
        .to_string();
    let source = get("source").unwrap_or("");
    let locked = match source {
        "path" => LockedSource::Path {
            path: get("path").unwrap_or("").to_string(),
        },
        "git" => LockedSource::Git {
            url: get("git").unwrap_or("").to_string(),
            pin: get("pin").unwrap_or("default").to_string(),
            commit: get("commit")
                .ok_or_else(|| {
                    anyhow!("nova.lock: git-запись `{}` без `commit`", name)
                })?
                .to_string(),
        },
        other => bail!("nova.lock: запись `{}` с неизвестным source `{}`", name, other),
    };
    Ok(LockedDep { name, source: locked })
}

/// Путь к `nova.lock` пакета.
pub fn lock_path(pkg_dir: &Path) -> PathBuf {
    pkg_dir.join("nova.lock")
}

/// Загрузить `nova.lock` пакета, если он есть.
pub fn load(pkg_dir: &Path) -> Result<Option<LockFile>> {
    let path = lock_path(pkg_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("чтение {}", path.display()))?;
    Ok(Some(LockFile::parse(&text)?))
}

/// Собрать полный (транзитивный) граф зависимостей пакета `entry_pkg_dir`.
///
/// Walks `[dependencies]` каждого пакета; `path`-deps резолвятся в
/// директорию, `git`-deps материализуются через `git_cache`
/// (с учётом уже загруженной lock-таблицы). Diamond-зависимости —
/// один раз. Цикл зависимостей пакетов (A→B→A) → ошибка.
pub fn collect_dep_graph(entry_pkg_dir: &Path) -> Result<Vec<LockedDep>> {
    let mut out: Vec<LockedDep> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    // Стек cycle-detection засеян entry-пакетом — цикл, замыкающийся
    // на сам entry (A→B→A), ловится сразу и с правильной цепочкой.
    let entry_name = crate::manifest::parse_manifest(
        &entry_pkg_dir.join("nova.toml"),
        entry_pkg_dir,
    )
    .map(|m| m.package_name)
    .unwrap_or_else(|| "<entry>".to_string());
    let mut stack: Vec<(String, PathBuf)> = vec![(entry_name, canon(entry_pkg_dir))];
    visit_pkg(entry_pkg_dir, &mut out, &mut seen, &mut stack)?;
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn visit_pkg(
    pkg_dir: &Path,
    out: &mut Vec<LockedDep>,
    seen: &mut HashSet<PathBuf>,
    stack: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let toml = pkg_dir.join("nova.toml");
    let Some(manifest) = crate::manifest::parse_manifest(&toml, pkg_dir) else {
        // Нет манифеста — нет объявленных зависимостей.
        return Ok(());
    };
    for dep in &manifest.dependencies {
        match &dep.source {
            DepSource::Path(rel) => {
                let dep_dir = pkg_dir.join(rel);
                if !dep_dir.is_dir() {
                    bail!(
                        "зависимость `{}`: path `{}` не существует\n  ожидалось: {}",
                        dep.name,
                        rel,
                        dep_dir.display(),
                    );
                }
                let c = canon(&dep_dir);
                check_cycle(&dep.name, &c, stack)?;
                if seen.insert(c.clone()) {
                    out.push(LockedDep {
                        name: dep.name.clone(),
                        source: LockedSource::Path { path: rel.clone() },
                    });
                    stack.push((dep.name.clone(), c));
                    visit_pkg(&dep_dir, out, seen, stack)?;
                    stack.pop();
                }
            }
            DepSource::Git { url, pin } => {
                let res = git_cache::resolve_git_dep(url, pin, None)
                    .with_context(|| format!("git-зависимость `{}`", dep.name))?;
                let c = canon(&res.checkout);
                check_cycle(&dep.name, &c, stack)?;
                if seen.insert(c.clone()) {
                    out.push(LockedDep {
                        name: dep.name.clone(),
                        source: LockedSource::Git {
                            url: url.clone(),
                            pin: pin_str(pin),
                            commit: res.commit.clone(),
                        },
                    });
                    stack.push((dep.name.clone(), c));
                    visit_pkg(&res.checkout, out, seen, stack)?;
                    stack.pop();
                }
            }
            // registry / некорректные записи в граф не попадают —
            // диагностируются на этапе резолва импортов (Ф.3).
            DepSource::Registry(_) | DepSource::Invalid(_) => {}
        }
    }
    Ok(())
}

fn check_cycle(
    name: &str,
    dep_canon: &Path,
    stack: &[(String, PathBuf)],
) -> Result<()> {
    if let Some(pos) = stack.iter().position(|(_, d)| d == dep_canon) {
        let mut chain: Vec<String> =
            stack[pos..].iter().map(|(n, _)| n.clone()).collect();
        chain.push(name.to_string());
        bail!(
            "цикл зависимостей пакетов:\n  {}",
            chain.join(" → "),
        );
    }
    Ok(())
}

/// Синхронизировать `nova.lock` пакета `entry_pkg_dir`:
///   1. загрузить существующий lock в `git_cache`-таблицу пинов
///      (воспроизводимость — git-deps не резолвятся «вживую»);
///   2. собрать актуальный граф зависимостей;
///   3. записать `nova.lock`.
///
/// Вызывается из `nova build`. Возвращает собранный граф.
pub fn sync(entry_pkg_dir: &Path) -> Result<Vec<LockedDep>> {
    let existing = load(entry_pkg_dir)?;
    if let Some(ex) = &existing {
        git_cache::install_lock_entries(ex.git_pins());
    }
    let graph = collect_dep_graph(entry_pkg_dir)?;
    // Не плодим `nova.lock` на ровном месте: пустой граф и файла ещё нет
    // — фиксировать нечего. Если lock уже был (зависимости убрали) —
    // перезаписываем, чтобы он отражал актуальное состояние.
    if graph.is_empty() && existing.is_none() {
        return Ok(graph);
    }
    let lock = LockFile {
        version: LOCK_VERSION,
        packages: graph.clone(),
    };
    let path = lock_path(entry_pkg_dir);
    std::fs::write(&path, lock.render())
        .with_context(|| format!("запись {}", path.display()))?;
    Ok(graph)
}

/// Загрузить `nova.lock` (если есть) в `git_cache`-таблицу пинов — без
/// перезаписи файла. Для read-only потребителей (например `nova run`
/// уже собранного проекта).
pub fn load_pins(entry_pkg_dir: &Path) -> Result<()> {
    if let Some(existing) = load(entry_pkg_dir)? {
        git_cache::install_lock_entries(existing.git_pins());
    }
    Ok(())
}

/// Plan 03.1 Ф.5 (`nova update`): пере-резолвить git-пины зависимостей.
/// `only = Some(name)` — обновить одну зависимость; `None` — все
/// git-зависимости. `path`-deps пинов не имеют — не затрагиваются.
///
/// Реализация: снять целевые git-записи из существующего `nova.lock`,
/// затем `sync` — снятые с пина зависимости резолвятся «вживую» (берётся
/// текущий commit ветки/тега), остальные остаются зафиксированными.
pub fn update(entry_pkg_dir: &Path, only: Option<&str>) -> Result<Vec<LockedDep>> {
    if let Some(existing) = load(entry_pkg_dir)? {
        let kept: Vec<LockedDep> = existing
            .packages
            .into_iter()
            .filter(|p| match &p.source {
                LockedSource::Git { .. } => match only {
                    Some(n) => p.name != n,
                    None => false,
                },
                LockedSource::Path { .. } => true,
            })
            .collect();
        let trimmed = LockFile {
            version: LOCK_VERSION,
            packages: kept,
        };
        let path = lock_path(entry_pkg_dir);
        std::fs::write(&path, trimmed.render())
            .with_context(|| format!("запись {}", path.display()))?;
    }
    sync(entry_pkg_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_parse_roundtrip() {
        let lock = LockFile {
            version: LOCK_VERSION,
            packages: vec![
                LockedDep {
                    name: "gitlib".into(),
                    source: LockedSource::Git {
                        url: "https://x.org/g.nv".into(),
                        pin: "tag:v1.0.0".into(),
                        commit: "a".repeat(40),
                    },
                },
                LockedDep {
                    name: "mathlib".into(),
                    source: LockedSource::Path {
                        path: "../mathlib".into(),
                    },
                },
            ],
        };
        let text = lock.render();
        let back = LockFile::parse(&text).expect("parse");
        assert_eq!(back.version, LOCK_VERSION);
        assert_eq!(back.packages, lock.packages);
    }

    #[test]
    fn parse_ignores_unknown_keys() {
        // Forward-compat: ключ `effects` (резерв Plan 03.4) не ломает парсер.
        let text = "version = 1\n\n[[package]]\nname = \"g\"\nsource = \"git\"\n\
                    git = \"u\"\npin = \"default\"\ncommit = \"abc\"\n\
                    effects = [\"Net\"]\n";
        let lf = LockFile::parse(text).expect("parse");
        assert_eq!(lf.packages.len(), 1);
        assert_eq!(lf.packages[0].name, "g");
    }

    #[test]
    fn git_pins_extracts_only_git() {
        let lock = LockFile {
            version: 1,
            packages: vec![
                LockedDep {
                    name: "g".into(),
                    source: LockedSource::Git {
                        url: "u".into(),
                        pin: "default".into(),
                        commit: "c".into(),
                    },
                },
                LockedDep {
                    name: "p".into(),
                    source: LockedSource::Path { path: "../p".into() },
                },
            ],
        };
        let pins = lock.git_pins();
        assert_eq!(pins, vec![("u".to_string(), "c".to_string())]);
    }

    #[test]
    fn parse_rejects_git_without_commit() {
        let text = "version = 1\n[[package]]\nname = \"g\"\nsource = \"git\"\ngit = \"u\"\n";
        assert!(LockFile::parse(text).is_err());
    }

    #[test]
    fn package_cycle_is_error() {
        // pkg_a → pkg_b → pkg_a через path-зависимости.
        let base = std::env::temp_dir().join(format!(
            "nova_lockcyc_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let a = base.join("pkg_a");
        let b = base.join("pkg_b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            a.join("nova.toml"),
            "[package]\nname = \"pkg_a\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\npkg_b = { path = \"../pkg_b\" }\n",
        )
        .unwrap();
        std::fs::write(
            b.join("nova.toml"),
            "[package]\nname = \"pkg_b\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\npkg_a = { path = \"../pkg_a\" }\n",
        )
        .unwrap();
        let err = collect_dep_graph(&a).expect_err("package cycle must error");
        assert!(err.to_string().contains("цикл"), "err: {}", err);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn collect_path_graph_transitive() {
        // app → mid → leaf (path-зависимости) — все три в графе.
        let base = std::env::temp_dir().join(format!(
            "nova_lockgraph_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = base.join("app");
        let mid = base.join("mid");
        let leaf = base.join("leaf");
        for d in [&app, &mid, &leaf] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(
            app.join("nova.toml"),
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nmid = { path = \"../mid\" }\n",
        )
        .unwrap();
        std::fs::write(
            mid.join("nova.toml"),
            "[package]\nname = \"mid\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nleaf = { path = \"../leaf\" }\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("nova.toml"),
            "[package]\nname = \"leaf\"\n[lib]\nsrc = \".\"\n[dependencies]\n",
        )
        .unwrap();
        let graph = collect_dep_graph(&app).expect("collect graph");
        let names: Vec<&str> = graph.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["leaf", "mid"]); // sorted, без самого app
        std::fs::remove_dir_all(&base).ok();
    }
}
