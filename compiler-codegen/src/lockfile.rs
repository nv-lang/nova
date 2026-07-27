//! Plan 03.1 Ф.4 — `nova.lock.toml`: фиксация графа зависимостей.
//!
//! `nova.lock.toml` пинит точные версии всех (транзитивных) зависимостей —
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
//! **Воспроизводимость.** `sync` загружает существующий `nova.lock.toml` в
//! `git_cache`-таблицу пинов до резолва графа — git-зависимости с уже
//! зафиксированным commit'ом не резолвятся «вживую» (ветка не «уедет»).

use crate::git_cache;
use crate::manifest::{DepSource, GitPin};
use crate::resolver::{self, DependencyProvider, PkgId};
use crate::semver::{Version, VersionReq};
use anyhow::{anyhow, bail, Context, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Источник зафиксированной зависимости.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockedSource {
    /// `path`-зависимость: путь как записан в `nova.toml` (относительный).
    Path { path: String },
    /// `git`-зависимость: URL, исходный пин (информативно / для
    /// `nova update`), резолвнутый точный commit и — для `version`-пина
    /// (Plan 03.2) — выбранная semver-версия (`None` для rev/tag/branch).
    Git {
        url: String,
        pin: String,
        commit: String,
        version: Option<String>,
    },
}

/// Одна запись `nova.lock.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedDep {
    pub name: String,
    pub source: LockedSource,
}

/// Разобранный / собранный `nova.lock.toml`.
#[derive(Debug, Clone)]
pub struct LockFile {
    pub version: u32,
    /// Записи, отсортированные по имени — детерминированный вывод.
    pub packages: Vec<LockedDep>,
}

/// Текущая версия формата `nova.lock.toml`.
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
    /// Сериализовать в текст `nova.lock.toml`.
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(
            "# nova.lock.toml — сгенерирован автоматически (Plan 03.1 / D78).\n\
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
                LockedSource::Git { url, pin, commit, version } => {
                    s.push_str("source = \"git\"\n");
                    s.push_str(&format!("git = \"{}\"\n", url));
                    s.push_str(&format!("pin = \"{}\"\n", pin));
                    if let Some(v) = version {
                        s.push_str(&format!("version = \"{}\"\n", v));
                    }
                    s.push_str(&format!("commit = \"{}\"\n", commit));
                }
            }
        }
        s
    }

    /// Разобрать текст `nova.lock.toml`. Неизвестные ключи игнорируются
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

    /// Plan 03.2 Ф.4: `(url, version)` git-записей с зафиксированной
    /// semver-версией — для seed'а preferred-версий резолвера
    /// (воспроизводимость: lock держит версию).
    pub fn git_versions(&self) -> Vec<(String, Version)> {
        self.packages
            .iter()
            .filter_map(|p| match &p.source {
                LockedSource::Git { url, version: Some(v), .. } => {
                    Version::parse(v).ok().map(|ver| (url.clone(), ver))
                }
                _ => None,
            })
            .collect()
    }
}

/// Собрать `LockedDep` из пар ключ-значение записи `[[package]]`.
fn record_to_dep(fields: &[(String, String)]) -> Result<LockedDep> {
    let get = |k: &str| fields.iter().find(|(fk, _)| fk == k).map(|(_, v)| v.as_str());
    let name = get("name")
        .ok_or_else(|| anyhow!("nova.lock.toml: запись [[package]] без `name`"))?
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
                    anyhow!("nova.lock.toml: git-запись `{}` без `commit`", name)
                })?
                .to_string(),
            version: get("version").map(|s| s.to_string()),
        },
        other => bail!("nova.lock.toml: запись `{}` с неизвестным source `{}`", name, other),
    };
    Ok(LockedDep { name, source: locked })
}

/// Plan 233 §2: канонiческое (новое) имя lockfile'а — `.toml`-расширение
/// даёт универсальную TOML-подсветку в любом редакторе/на GitHub без
/// плагинов (старое `nova.lock` — абстрактное расширение, редакторы его не
/// распознают как TOML). Всегда используется при ЗАПИСИ (`sync`/
/// `drop_git_locks`).
pub const LOCK_FILE_NAME: &str = "nova.lock.toml";

/// Legacy-имя (pre-Plan-233) — при ЧТЕНИИ поддержано наравне с новым (см.
/// `load`), но только пока новое имя отсутствует; при обнаружении
/// печатается deprecation warning. Никогда не используется при записи.
pub const LEGACY_LOCK_FILE_NAME: &str = "nova.lock";

/// Путь к `nova.lock.toml` пакета (новое, канонiческое имя — см.
/// `LOCK_FILE_NAME`). Используется и для чтения (см. `load` — в паре с
/// legacy-путём), и ВСЕГДА для записи.
pub fn lock_path(pkg_dir: &Path) -> PathBuf {
    pkg_dir.join(LOCK_FILE_NAME)
}

/// Путь к legacy `nova.lock` пакета (pre-Plan-233 имя). Только для чтения
/// (fallback в `load`, deprecation warning) — запись никогда сюда не идёт.
pub fn legacy_lock_path(pkg_dir: &Path) -> PathBuf {
    pkg_dir.join(LEGACY_LOCK_FILE_NAME)
}

/// Загрузить lockfile пакета, если он есть. Plan 233 §2: читает ОБА имени —
/// новое (`nova.lock.toml`, приоритет, без warning) и, если новое
/// отсутствует, legacy (`nova.lock`, deprecation warning на stderr).
/// Отсутствуют оба — `Ok(None)`.
pub fn load(pkg_dir: &Path) -> Result<Option<LockFile>> {
    let path = lock_path(pkg_dir);
    if path.is_file() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("чтение {}", path.display()))?;
        return Ok(Some(LockFile::parse(&text)?));
    }
    let legacy = legacy_lock_path(pkg_dir);
    if legacy.is_file() {
        eprintln!(
            "warning: {} устарел, переименуйте в {} [W_LOCK_LEGACY_NAME]",
            legacy.display(),
            path.display(),
        );
        let text = std::fs::read_to_string(&legacy)
            .with_context(|| format!("чтение {}", legacy.display()))?;
        return Ok(Some(LockFile::parse(&text)?));
    }
    Ok(None)
}

/// Собрать полный (транзитивный) граф зависимостей пакета `entry_pkg_dir`.
///
/// Walks `[dependencies]` каждого пакета; `path`-deps резолвятся в
/// директорию, `git`-deps материализуются через `git_cache`
/// (с учётом уже загруженной lock-таблицы). Diamond-зависимости —
/// один раз. Цикл зависимостей пакетов (A→B→A) → ошибка.
pub fn collect_dep_graph(entry_pkg_dir: &Path) -> Result<Vec<LockedDep>> {
    collect_dep_graph_ex(entry_pkg_dir, &HashMap::new())
}

/// Plan 03.2 Ф.4: вариант с картой `url → resolved-version` (от
/// `resolve_version_deps`) — записи `git`-deps в графе получают поле
/// `version`.
pub fn collect_dep_graph_ex(
    entry_pkg_dir: &Path,
    resolved_versions: &HashMap<String, String>,
) -> Result<Vec<LockedDep>> {
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
    visit_pkg(entry_pkg_dir, &mut out, &mut seen, &mut stack, resolved_versions)?;
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
    resolved_versions: &HashMap<String, String>,
) -> Result<()> {
    let toml = pkg_dir.join("nova.toml");
    let Some(manifest) = crate::manifest::parse_manifest(&toml, pkg_dir) else {
        // Нет манифеста — нет объявленных зависимостей.
        return Ok(());
    };
    for dep in &manifest.dependencies {
        // Plan 204 lockfix (D420, Cargo-семантика): lock фиксирует
        // РЕЛИЗНОЕ разрешение — ДЕКЛАРИРОВАННЫЙ источник из
        // `[dependencies]` (git url + резолвнутый тег + commit), а НЕ
        // `[replace]`-override. `[replace]` — локальный overlay: применяется
        // только в module-resolution/сборке (imports.rs, effective_source),
        // в `nova.lock.toml` не записывается вовсе. Сборка с активным replace
        // просто использует path поверх lock, не переписывая его —
        // lock остаётся публикуемым источником истины.
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
                    visit_pkg(&dep_dir, out, seen, stack, resolved_versions)?;
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
                            // Plan 03.2 Ф.4: resolved-версия для
                            // version-пинов (resolve_version_deps).
                            version: resolved_versions.get(url).cloned(),
                        },
                    });
                    stack.push((dep.name.clone(), c));
                    visit_pkg(&res.checkout, out, seen, stack, resolved_versions)?;
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

/// Plan 204 дофикс №2 (D420 go-scope): `[replace]` declared inside a
/// DEPENDENCY's own manifest is inert per Go-module semantics — only the
/// build ROOT's `[replace]` is ever consulted (enforced at resolution time
/// by `imports::lookup_dependency`'s root-check). This walks the SAME
/// dependency graph as [`collect_dep_graph_ex`] (declared `dep.source` only
/// — `[replace]` never affects graph traversal, mirroring the "replace
/// doesn't leak into lock" rule above) and, for every NON-ROOT package
/// visited, surfaces `W_REPLACE_IN_DEPENDENCY` if THAT package's own
/// manifest (`nova.toml` and/or its own `nova.local.toml`, already merged by
/// `manifest::parse_manifest`) declares a non-empty `[replace]` — dead
/// configuration the dependency author should know about (it is honored
/// only when THEY build their own package as the root, never when consumed
/// transitively).
pub fn collect_replace_scope_warnings(entry_pkg_dir: &Path) -> Vec<crate::manifest::ManifestWarning> {
    let mut out = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    seen.insert(canon(entry_pkg_dir));
    walk_replace_scope(entry_pkg_dir, &mut out, &mut seen);
    out
}

fn walk_replace_scope(
    pkg_dir: &Path,
    out: &mut Vec<crate::manifest::ManifestWarning>,
    seen: &mut HashSet<PathBuf>,
) {
    let toml = pkg_dir.join("nova.toml");
    let Some(manifest) = crate::manifest::parse_manifest(&toml, pkg_dir) else {
        return;
    };
    for dep in &manifest.dependencies {
        let dep_dir = match &dep.source {
            DepSource::Path(rel) => {
                let d = pkg_dir.join(rel);
                if d.is_dir() { Some(d) } else { None }
            }
            DepSource::Git { url, pin } => {
                git_cache::resolve_git_dep(url, pin, None).ok().map(|r| r.checkout)
            }
            DepSource::Registry(_) | DepSource::Invalid(_) => None,
        };
        let Some(dep_dir) = dep_dir else { continue };
        let c = canon(&dep_dir);
        if !seen.insert(c.clone()) {
            continue; // already visited (diamond dep / cycle) — no duplicate warnings.
        }
        let dep_toml = dep_dir.join("nova.toml");
        if let Some(dep_manifest) = crate::manifest::parse_manifest(&dep_toml, &dep_dir) {
            // Реестр 221.1 №135(б): непереносимый `path` в манифесте ЗАВИСИМОСТИ.
            // `manifest_warnings` вызывался только на КОРНЕВОМ манифесте, поэтому
            // потребитель пакета, чей манифест несёт `path = "../соседняя-репа"`,
            // не узнавал об этом ничего — до момента, когда сборка у него просто
            // не находила каталог (измерено на nova-polaris → nova-http, блокер
            // тегов A-V7). Тот же обход зависимостей, что уже используется для
            // `W_REPLACE_IN_DEPENDENCY` ниже, — правило и текст берём готовые.
            for w in crate::manifest::manifest_warnings(&dep_manifest, &dep_toml) {
                if w.code == "W_DEP_PATH_NO_RELEASE" {
                    out.push(crate::manifest::ManifestWarning {
                        code: "W_DEP_PATH_NO_RELEASE_IN_DEPENDENCY",
                        message: format!(
                            "зависимость `{}` НЕПЕРЕНОСИМА: {}\n    \
                             следствие: этот пакет не соберётся у того, у кого \
                             нет соседнего каталога — почини в самом пакете \
                             `{}`, а не здесь",
                            dep.name, w.message, dep_toml.display(),
                        ),
                    });
                }
            }
            for name in dep_manifest.replace.keys() {
                out.push(crate::manifest::ManifestWarning {
                    code: "W_REPLACE_IN_DEPENDENCY",
                    message: format!(
                        "зависимость `{}` объявляет [replace] `{}` в СВОЁМ \
                         манифесте ({}) — игнорируется: [replace] действует \
                         только для корневого пакета текущей сборки \
                         (go-семантика), не для зависимостей",
                        dep.name, name, dep_toml.display(),
                    ),
                });
            }
        }
        walk_replace_scope(&dep_dir, out, seen);
    }
}

/// Синхронизировать `nova.lock.toml` пакета `entry_pkg_dir`:
///   1. загрузить существующий lock в `git_cache`-таблицу пинов
///      (воспроизводимость — git-deps не резолвятся «вживую»);
///   2. собрать актуальный граф зависимостей;
///   3. записать `nova.lock.toml`.
///
/// Вызывается из `nova build`. Возвращает собранный граф.
pub fn sync(entry_pkg_dir: &Path) -> Result<Vec<LockedDep>> {
    sync_ex(entry_pkg_dir, &[])
}

/// Ядро `sync` с дополнительными корневыми ограничениями `extra_root`
/// (`nova update --precise` передаёт сюда точное `=X`).
fn sync_ex(
    entry_pkg_dir: &Path,
    extra_root: &[(PkgId, VersionReq)],
) -> Result<Vec<LockedDep>> {
    let existing = load(entry_pkg_dir)?;
    // Plan 03.2 Ф.4: предпочтительные версии из существующего lock —
    // воспроизводимость (резолвер держит зафиксированную версию).
    let mut preferred: HashMap<PkgId, Version> = HashMap::new();
    if let Some(ex) = &existing {
        git_cache::install_lock_entries(ex.git_pins());
        for (url, ver) in ex.git_versions() {
            preferred.insert(url, ver);
        }
    }
    // Plan 03.2 Ф.3: согласованный резолв версионных git-зависимостей
    // (`version = "^1.2"`) — до обхода графа, чтобы collect_dep_graph
    // взял зафиксированные резолвером commit'ы и версии.
    let resolved = resolve_version_deps(entry_pkg_dir, &preferred, extra_root)?;
    let graph = collect_dep_graph_ex(entry_pkg_dir, &resolved)?;
    // Не плодим `nova.lock.toml` на ровном месте: пустой граф и файла ещё нет
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

/// Загрузить `nova.lock.toml` (если есть) в `git_cache`-таблицу пинов — без
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
/// Реализация: снять целевые git-записи из существующего `nova.lock.toml`,
/// затем `sync` — снятые с пина зависимости резолвятся «вживую» (берётся
/// текущий commit ветки/тега), остальные остаются зафиксированными.
pub fn update(entry_pkg_dir: &Path, only: Option<&str>) -> Result<Vec<LockedDep>> {
    drop_git_locks(entry_pkg_dir, only)?;
    sync(entry_pkg_dir)
}

/// Снять git-записи из `nova.lock.toml`: `only = Some(name)` — одну, `None`
/// — все. Path-deps не затрагиваются. Снятая с пина зависимость при
/// следующем `sync` пере-резолвится «вживую».
fn drop_git_locks(entry_pkg_dir: &Path, only: Option<&str>) -> Result<()> {
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
    Ok(())
}

/// Plan 03.2 Ф.4 (`nova update --precise`): пере-резолвить зависимость
/// `dep_name` (git-URL `dep_url`) на **точную** версию `version`.
/// Резолвер обязан её выполнить (с учётом транзитивных ограничений)
/// либо упасть с конфликтом.
pub fn update_precise(
    entry_pkg_dir: &Path,
    dep_name: &str,
    dep_url: &str,
    version: &Version,
) -> Result<Vec<LockedDep>> {
    // Снять текущий пин целевой зависимости — иначе старая версия
    // осталась бы preferred.
    drop_git_locks(entry_pkg_dir, Some(dep_name))?;
    let exact = VersionReq::parse(&format!("={}", version))
        .map_err(|e| anyhow!("некорректная версия `{}`: {}", version, e))?;
    sync_ex(entry_pkg_dir, &[(dep_url.to_string(), exact)])
}

// ---------------------------------------------------------------------
// Plan 03.2 Ф.3 — git-backed DependencyProvider + интеграция резолвера.
// ---------------------------------------------------------------------

/// `DependencyProvider` поверх git: версии пакета — semver-теги
/// репозитория, зависимости версии — `[dependencies]` из `nova.toml` на
/// соответствующем теге. `PkgId` = git-URL.
///
/// `resolve_git_dep_in` вызывается напрямую (в обход lock-таблицы) —
/// провайдер обязан видеть РЕАЛЬНЫЕ теги, а не зафиксированный commit.
struct GitProvider {
    /// url → (version, tag-name), отсортировано — кэш `list_versions`.
    versions: RefCell<HashMap<String, Vec<(Version, String)>>>,
    /// (url, version) → зависимости — кэш разобранных `nova.toml`.
    deps: RefCell<HashMap<(String, String), Vec<(PkgId, VersionReq)>>>,
}

impl GitProvider {
    fn new() -> GitProvider {
        GitProvider {
            versions: RefCell::new(HashMap::new()),
            deps: RefCell::new(HashMap::new()),
        }
    }

    fn versions_with_tags(&self, url: &str) -> Result<Vec<(Version, String)>, String> {
        if let Some(c) = self.versions.borrow().get(url) {
            return Ok(c.clone());
        }
        let vs = git_cache::list_versions(url).map_err(|e| e.to_string())?;
        self.versions.borrow_mut().insert(url.to_string(), vs.clone());
        Ok(vs)
    }

    fn tag_of(&self, url: &str, ver: &Version) -> Result<String, String> {
        self.versions_with_tags(url)?
            .into_iter()
            .find(|(v, _)| v == ver)
            .map(|(_, t)| t)
            .ok_or_else(|| format!("версия {} пакета `{}` не найдена среди тегов", ver, url))
    }

    /// Точный commit выбранной версии — для записи в lock-таблицу.
    fn commit_of(&self, url: &str, ver: &Version) -> Result<String> {
        let tag = self.tag_of(url, ver).map_err(|e| anyhow!(e))?;
        let root = git_cache::git_cache_root()?;
        let res = git_cache::resolve_git_dep_in(&root, url, &GitPin::Tag(tag), None)?;
        Ok(res.commit)
    }
}

impl DependencyProvider for GitProvider {
    fn available_versions(&self, pkg: &PkgId) -> Result<Vec<Version>, String> {
        Ok(self
            .versions_with_tags(pkg)?
            .into_iter()
            .map(|(v, _)| v)
            .collect())
    }

    fn dependencies(
        &self,
        pkg: &PkgId,
        ver: &Version,
    ) -> Result<Vec<(PkgId, VersionReq)>, String> {
        let key = (pkg.clone(), ver.to_string());
        if let Some(c) = self.deps.borrow().get(&key) {
            return Ok(c.clone());
        }
        let tag = self.tag_of(pkg, ver)?;
        let root = git_cache::git_cache_root().map_err(|e| e.to_string())?;
        let res = git_cache::resolve_git_dep_in(&root, pkg, &GitPin::Tag(tag), None)
            .map_err(|e| e.to_string())?;
        let toml = res.checkout.join("nova.toml");
        let manifest = crate::manifest::parse_manifest(&toml, &res.checkout)
            .ok_or_else(|| {
                format!("git-пакет `{}`@{}: нет `[package]` в nova.toml", pkg, ver)
            })?;
        // В граф версий идут только версионные git-зависимости; path /
        // точечные git-deps резолвятся обычным обходом collect_dep_graph.
        let mut out = Vec::new();
        for d in &manifest.dependencies {
            if let DepSource::Git { url, pin: GitPin::Version(req) } = &d.source {
                out.push((url.clone(), req.clone()));
            }
        }
        self.deps.borrow_mut().insert(key, out.clone());
        Ok(out)
    }
}

/// Plan 03.2 Ф.3/Ф.4: согласованно разрешить версионные git-зависимости
/// пакета `entry_pkg_dir`, зафиксировать выбранные commit'ы в
/// `git_cache`-таблице пинов и вернуть карту `url → resolved-version`
/// (для `version`-поля `nova.lock.toml`).
///
/// `preferred` — версии из существующего `nova.lock.toml`: резолвер держит
/// их, пока ограничения позволяют (воспроизводимость). Реагирует только
/// на `{ git = "...", version = "..." }`-зависимости; иначе — no-op.
fn resolve_version_deps(
    entry_pkg_dir: &Path,
    preferred: &HashMap<PkgId, Version>,
    extra_root: &[(PkgId, VersionReq)],
) -> Result<HashMap<String, String>> {
    let toml = entry_pkg_dir.join("nova.toml");
    let Some(manifest) = crate::manifest::parse_manifest(&toml, entry_pkg_dir) else {
        return Ok(HashMap::new());
    };
    // Plan 204 lockfix (D420): version-резолв идёт по ДЕКЛАРИРОВАННЫМ
    // `[dependencies]` (release-форма) даже при активном `[replace]` —
    // lock обязан фиксировать релизное разрешение (тег+commit), replace
    // в lock не протекает (Cargo-семантика; применяется только в сборке).
    let mut root_version_deps: Vec<(PkgId, VersionReq)> = manifest
        .dependencies
        .iter()
        .filter_map(|d| match &d.source {
            DepSource::Git { url, pin: GitPin::Version(req) } => {
                Some((url.clone(), req.clone()))
            }
            _ => None,
        })
        .collect();
    // Plan 03.2 Ф.4: `nova update --precise` — дополнительное точное
    // ограничение (`=X`); резолвер обязан его выполнить либо упасть.
    root_version_deps.extend(extra_root.iter().cloned());
    if root_version_deps.is_empty() {
        return Ok(HashMap::new());
    }
    let provider = GitProvider::new();
    let resolution =
        resolver::resolve_with_preferences(&provider, &root_version_deps, preferred)
            .map_err(|e| anyhow!("резолв версий git-зависимостей:\n  {}", e))?;
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut versions: HashMap<String, String> = HashMap::new();
    for (url, ver) in &resolution.selected {
        let commit = provider
            .commit_of(url, ver)
            .with_context(|| format!("commit версии {} пакета `{}`", ver, url))?;
        entries.push((url.clone(), commit));
        versions.insert(url.clone(), ver.to_string());
    }
    git_cache::install_lock_entries(entries);
    Ok(versions)
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
                        pin: "version:^1.0".into(),
                        commit: "a".repeat(40),
                        version: Some("1.4.2".into()),
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
                        version: None,
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

    fn tmp_pkg(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nova_p233_lock_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// Plan 233 §2: `lock_path` — новое каноническое имя.
    #[test]
    fn lock_path_uses_new_name() {
        let dir = PathBuf::from("some/pkg");
        assert_eq!(lock_path(&dir), dir.join("nova.lock.toml"));
        assert_eq!(legacy_lock_path(&dir), dir.join("nova.lock"));
    }

    /// `load` читает НОВОЕ имя (`nova.lock.toml`), если оно есть — без
    /// обращения к legacy-файлу вовсе.
    #[test]
    fn load_reads_new_name() {
        let dir = tmp_pkg("new_name");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("nova.lock.toml"),
            "version = 1\n\n[[package]]\nname = \"p\"\nsource = \"path\"\npath = \"../p\"\n",
        )
        .unwrap();
        let lf = load(&dir).expect("load").expect("Some");
        assert_eq!(lf.packages.len(), 1);
        assert_eq!(lf.packages[0].name, "p");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 233 §2 (back-compat): `load` falls back to the LEGACY name
    /// (`nova.lock`) when the new name is absent — content still parses.
    #[test]
    fn load_falls_back_to_legacy_name() {
        let dir = tmp_pkg("legacy_name");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("nova.lock"),
            "version = 1\n\n[[package]]\nname = \"legacy\"\nsource = \"path\"\npath = \"../legacy\"\n",
        )
        .unwrap();
        let lf = load(&dir).expect("load").expect("Some");
        assert_eq!(lf.packages.len(), 1);
        assert_eq!(lf.packages[0].name, "legacy");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Both names present — the NEW name wins (legacy ignored, no
    /// warning), mirroring `manifest`'s override-file precedence.
    #[test]
    fn load_prefers_new_name_when_both_present() {
        let dir = tmp_pkg("both_names");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("nova.lock.toml"),
            "version = 1\n\n[[package]]\nname = \"new\"\nsource = \"path\"\npath = \"../new\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("nova.lock"),
            "version = 1\n\n[[package]]\nname = \"old\"\nsource = \"path\"\npath = \"../old\"\n",
        )
        .unwrap();
        let lf = load(&dir).expect("load").expect("Some");
        assert_eq!(lf.packages[0].name, "new");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Neither name present — `Ok(None)`, matching pre-Plan-233 behavior.
    #[test]
    fn load_none_when_neither_name_present() {
        let dir = tmp_pkg("neither_name");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir).expect("load").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Plan 233 §2: `sync` on a package whose ONLY existing lock is the
    /// LEGACY name still reads it (preferred versions honored) but writes
    /// the graph to the NEW name — legacy file is left untouched on disk
    /// (least-surprise; `load` will prefer the new file on next read, so
    /// the deprecation warning naturally stops firing afterwards).
    #[test]
    fn sync_reads_legacy_writes_new_name() {
        let dir = tmp_pkg("sync_legacy_to_new");
        let sub = dir.join("leaf");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            dir.join("nova.toml"),
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nleaf = { path = \"leaf\" }\n",
        )
        .unwrap();
        std::fs::write(
            sub.join("nova.toml"),
            "[package]\nname = \"leaf\"\n[lib]\nsrc = \".\"\n[dependencies]\n",
        )
        .unwrap();
        // Pre-existing LEGACY lock (stale content — sync must overwrite via
        // the NEW name, not edit the legacy file in place).
        std::fs::write(dir.join("nova.lock"), "version = 1\n").unwrap();

        let graph = sync(&dir).expect("sync");
        assert_eq!(graph.len(), 1);
        assert!(dir.join("nova.lock.toml").is_file(), "sync must write the NEW name");
        let new_content = std::fs::read_to_string(dir.join("nova.lock.toml")).unwrap();
        assert!(new_content.contains("leaf"), "content: {}", new_content);
        // Legacy file untouched (still its stale placeholder content) —
        // sync never edits/deletes it.
        let legacy_content = std::fs::read_to_string(dir.join("nova.lock")).unwrap();
        assert_eq!(legacy_content, "version = 1\n");

        std::fs::remove_dir_all(&dir).ok();
    }
}
