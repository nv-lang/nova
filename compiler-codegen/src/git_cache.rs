//! Plan 03.1 Ф.2 — git-зависимости: fetch + локальный кэш.
//!
//! `[dependencies] foo = { git = "URL", rev|tag|branch = "..." }` —
//! зависимость из git-репозитория. Этот модуль обеспечивает её
//! материализацию на диске для межпакетного резолва (Ф.3).
//!
//! **Раскладка кэша** (`$NOVA_HOME/git` либо `~/.nova/git`):
//!   - `db/<repo-id>.git`        — bare-клон репозитория (объекты).
//!   - `co/<repo-id>/<commit>/`  — checkout рабочего дерева на commit'е.
//!
//! `<repo-id>` = читаемый stem URL + стабильный хэш URL — без коллизий,
//! но узнаваемо. Checkout адресуется **точным commit'ом** → immutable и
//! переиспользуем: повторная сборка того же пина — без сети.
//!
//! **Сеть.** Bare-клон делается один раз. Для `rev`/`tag` (≈immutable)
//! fetch выполняется лишь если пин не резолвится локально. Для
//! `branch`/без-пина — fetch на каждый резолв (ветка «уезжает»),
//! кроме offline-режима. `NOVA_OFFLINE=1` запрещает любые сетевые
//! операции — сборка только из готового кэша.
//!
//! **Воспроизводимость.** Даже `branch`-пин в Ф.4 фиксируется в
//! `nova.lock` точным commit'ом — см. `resolve_git_dep` параметр
//! `locked_commit`.

use crate::manifest::GitPin;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

/// Результат материализации git-зависимости.
#[derive(Debug, Clone)]
pub struct GitResolution {
    /// Каталог checkout'а — рабочее дерево на нужном commit'е.
    pub checkout: PathBuf,
    /// Точный commit hash (40 hex-символов).
    pub commit: String,
}

/// Корень кэша git-зависимостей: `$NOVA_HOME/git`, иначе `~/.nova/git`.
pub fn git_cache_root() -> Result<PathBuf> {
    if let Some(h) = std::env::var_os("NOVA_HOME") {
        return Ok(PathBuf::from(h).join("git"));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| anyhow!(
            "не удалось определить домашнюю директорию для кэша git \
             (ни HOME, ни USERPROFILE, ни NOVA_HOME не заданы)"
        ))?;
    Ok(PathBuf::from(home).join(".nova").join("git"))
}

/// Offline-режим (`NOVA_OFFLINE=1|true|on`): сетевые операции запрещены,
/// сборка идёт только из уже существующего кэша.
fn offline() -> bool {
    matches!(
        std::env::var("NOVA_OFFLINE").as_deref(),
        Ok("1") | Ok("true") | Ok("on")
    )
}

/// Стабильный идентификатор репозитория по URL — имя каталога в кэше.
/// Читаемый stem + 64-битный хэш всего URL (разные URL с одинаковым
/// stem не коллизируют).
fn repo_id(url: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    let stem: String = url
        .trim_end_matches('/')
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or("repo")
        .trim_end_matches(".git")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let stem = if stem.is_empty() { "repo".to_string() } else { stem };
    format!("{}-{:016x}", stem, h.finish())
}

/// Запустить `git` с аргументами; вернуть trimmed stdout или ошибку с
/// stderr. `cwd` — рабочий каталог (для `git -C` эквивалента).
fn run_git(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let out = cmd.output().with_context(|| {
        format!(
            "не удалось запустить `git {}` — git установлен и в PATH?",
            args.join(" ")
        )
    })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!(
            "`git {}` завершился с ошибкой:\n  {}",
            args.join(" "),
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `git rev-parse` пина в полный commit hash; `None` если пин не
/// резолвится в этом (bare) репозитории.
fn rev_parse(db: &Path, spec: &str) -> Option<String> {
    run_git(
        &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", spec)],
        Some(db),
    )
    .ok()
    .filter(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()))
}

/// git-ref-спецификации для пина (в порядке предпочтения). Bare-клон
/// (`git clone --bare`) хранит ветки в `refs/heads/*`, теги в
/// `refs/tags/*`.
fn pin_specs(pin: &GitPin) -> Vec<String> {
    match pin {
        GitPin::Rev(r) => vec![r.clone()],
        GitPin::Tag(t) => vec![format!("refs/tags/{}", t), t.clone()],
        GitPin::Branch(b) => vec![format!("refs/heads/{}", b), b.clone()],
        // Plan 03.2: Version резолвится отдельным путём (выбор тега).
        GitPin::Version(_) => Vec::new(),
        GitPin::Default => vec!["HEAD".to_string()],
    }
}

/// Человекочитаемое описание пина для диагностики.
fn pin_label(pin: &GitPin) -> String {
    match pin {
        GitPin::Rev(r) => format!("rev `{}`", r),
        GitPin::Tag(t) => format!("tag `{}`", t),
        GitPin::Branch(b) => format!("branch `{}`", b),
        GitPin::Version(req) => format!("версия `{}`", req),
        GitPin::Default => "ветка по умолчанию".to_string(),
    }
}

/// Plan 03.2: выбрать тег репозитория с наибольшей semver-версией,
/// удовлетворяющей `req`. Возвращает имя тега (не commit). Теги,
/// не парсящиеся как semver, пропускаются.
fn select_version_tag(db: &Path, req: &crate::semver::VersionReq) -> Option<String> {
    let out = run_git(&["tag", "--list"], Some(db)).ok()?;
    let mut best: Option<(crate::semver::Version, String)> = None;
    for line in out.lines() {
        let tag = line.trim();
        if tag.is_empty() {
            continue;
        }
        if let Ok(ver) = crate::semver::Version::parse_tag(tag) {
            if req.matches(&ver) {
                let better = match &best {
                    Some((bv, _)) => ver > *bv,
                    None => true,
                };
                if better {
                    best = Some((ver, tag.to_string()));
                }
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Memo на процесс: `(url, pin)` → уже выполненный резолв. Резолвер
/// зовёт `resolve_git_dep` на каждый импорт из git-зависимости —
/// без memo `branch`-пин делал бы `git fetch` на каждый импорт.
fn memo() -> &'static Mutex<HashMap<String, GitResolution>> {
    static MEMO: OnceLock<Mutex<HashMap<String, GitResolution>>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Plan 03.1 Ф.4: таблица зафиксированных в `nova.lock` commit'ов
/// (`git-url` → `commit`). `resolve_git_dep` без явного `locked_commit`
/// сверяется с ней — это и есть воспроизводимость из lockfile.
fn lock_table() -> &'static Mutex<HashMap<String, String>> {
    static T: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Plan 03.1 Ф.4: загрузить пины из `nova.lock`. После этого
/// `resolve_git_dep` для перечисленных URL берёт зафиксированный commit
/// (а не резолвит пин «вживую») — детерминированная сборка.
pub fn install_lock_entries<I>(entries: I)
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut t = lock_table().lock().unwrap();
    for (url, commit) in entries {
        t.insert(url, commit);
    }
}

/// Plan 03.1 Ф.4 (для `nova update`): забыть зафиксированный commit для
/// URL — следующий резолв пройдёт «вживую».
pub fn forget_lock_entry(url: &str) {
    lock_table().lock().unwrap().remove(url);
}

fn locked_commit_for(url: &str) -> Option<String> {
    lock_table().lock().unwrap().get(url).cloned()
}

fn memo_key(url: &str, pin: &GitPin, locked: Option<&str>) -> String {
    format!("{}\u{0}{:?}\u{0}{}", url, pin, locked.unwrap_or(""))
}

/// Материализовать git-зависимость в кэше по умолчанию
/// (`git_cache_root()`); вернуть checkout рабочего дерева на нужном
/// commit'е.
///
/// `locked_commit` — точный commit из `nova.lock` (Ф.4): если задан,
/// пин (особенно `branch`) **игнорируется как селектор** и используется
/// именно этот commit — воспроизводимость. `None` — резолв пина «вживую».
pub fn resolve_git_dep(
    url: &str,
    pin: &GitPin,
    locked_commit: Option<&str>,
) -> Result<GitResolution> {
    // Явный `locked_commit` приоритетнее; иначе — таблица из `nova.lock`.
    let effective: Option<String> = locked_commit
        .map(|s| s.to_string())
        .or_else(|| locked_commit_for(url));
    let key = memo_key(url, pin, effective.as_deref());
    if let Some(hit) = memo().lock().unwrap().get(&key).cloned() {
        return Ok(hit);
    }
    let root = git_cache_root()?;
    let res = resolve_git_dep_in(&root, url, pin, effective.as_deref())?;
    memo().lock().unwrap().insert(key, res.clone());
    Ok(res)
}

/// Ядро `resolve_git_dep` с явным корнем кэша — без memo и без
/// `git_cache_root()`. Прямой вызов — из тестов (изолированный
/// temp-кэш, без глобального `NOVA_HOME`).
/// Гарантировать bare-клон репозитория в кэше. Возвращает путь к нему
/// и флаг «уже существовал до вызова» (свежий клон fetch'ить не нужно).
fn ensure_db(cache_root: &Path, url: &str) -> Result<(PathBuf, bool)> {
    let rid = repo_id(url);
    let db = cache_root.join("db").join(format!("{}.git", rid));
    let db_existed = db.is_dir();
    if !db_existed {
        if offline() {
            bail!(
                "git-зависимость `{}` отсутствует в кэше, а режим offline \
                 (NOVA_OFFLINE) запрещает clone\n  кэш: {}",
                url,
                db.display()
            );
        }
        if let Some(parent) = db.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("не удалось создать каталог кэша {}", parent.display())
            })?;
        }
        run_git(
            &["clone", "--bare", "--quiet", url, &db.to_string_lossy()],
            None,
        )
        .with_context(|| format!("clone git-зависимости `{}`", url))?;
    }
    Ok((db, db_existed))
}

/// Plan 03.2: semver-теги репозитория — источник версий для резолва.
/// Возвращает `(version, tag-name)`, отсортировано по возрастанию
/// версии. Не-semver теги пропускаются.
pub fn list_versions(url: &str) -> Result<Vec<(crate::semver::Version, String)>> {
    let root = git_cache_root()?;
    list_versions_in(&root, url)
}

/// Ядро `list_versions` с явным корнем кэша (для тестов).
pub fn list_versions_in(
    cache_root: &Path,
    url: &str,
) -> Result<Vec<(crate::semver::Version, String)>> {
    let (db, db_existed) = ensure_db(cache_root, url)?;
    // Новые теги могли появиться upstream.
    if db_existed && !offline() {
        let _ = run_git(&["fetch", "--quiet", "--tags", "--prune", "origin"], Some(&db));
    }
    let out = run_git(&["tag", "--list"], Some(&db))?;
    let mut vers: Vec<(crate::semver::Version, String)> = Vec::new();
    for line in out.lines() {
        let tag = line.trim();
        if tag.is_empty() {
            continue;
        }
        if let Ok(v) = crate::semver::Version::parse_tag(tag) {
            vers.push((v, tag.to_string()));
        }
    }
    vers.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(vers)
}

pub fn resolve_git_dep_in(
    cache_root: &Path,
    url: &str,
    pin: &GitPin,
    locked_commit: Option<&str>,
) -> Result<GitResolution> {
    // --- 1. bare-клон репозитория (один раз) ---------------------------
    let (db, db_existed) = ensure_db(cache_root, url)?;

    // --- 2. определить целевой commit ---------------------------------
    let commit = if let Some(locked) = locked_commit {
        // Ф.4: точный commit из lockfile. Убедиться, что он есть в db;
        // при необходимости — fetch.
        if rev_parse(&db, locked).is_none() {
            if !offline() {
                let _ = run_git(&["fetch", "--quiet", "--tags", "--prune", "origin"], Some(&db));
            }
            rev_parse(&db, locked).ok_or_else(|| {
                anyhow!(
                    "зафиксированный в nova.lock commit `{}` git-зависимости \
                     `{}` не найден в репозитории",
                    locked,
                    url
                )
            })?
        } else {
            locked.to_string()
        }
    } else if let GitPin::Version(req) = pin {
        // Plan 03.2: semver-диапазон — теги репозитория источник версий.
        // Новый подходящий тег мог появиться upstream → fetch (как ветка),
        // кроме offline и свежего клона.
        if db_existed && !offline() {
            run_git(&["fetch", "--quiet", "--tags", "--prune", "origin"], Some(&db))
                .with_context(|| format!("fetch git-зависимости `{}`", url))?;
        }
        let tag = select_version_tag(&db, req).ok_or_else(|| {
            anyhow!(
                "git-зависимость `{}`: ни один тег не подходит под версию `{}`{}",
                url,
                req,
                if offline() { " (offline — fetch запрещён)" } else { "" },
            )
        })?;
        rev_parse(&db, &format!("refs/tags/{}", tag))
            .or_else(|| rev_parse(&db, &tag))
            .ok_or_else(|| {
                anyhow!(
                    "git-зависимость `{}`: тег `{}` не резолвится в commit",
                    url, tag,
                )
            })?
    } else {
        // Резолв пина «вживую».
        let specs = pin_specs(pin);
        let resolve = |db: &Path| specs.iter().find_map(|s| rev_parse(db, s));

        // `branch`/default — ветка движется: fetch перед резолвом
        // (кроме offline и кроме только что сделанного свежего клона).
        let moving = matches!(pin, GitPin::Branch(_) | GitPin::Default);
        if moving && db_existed && !offline() {
            run_git(&["fetch", "--quiet", "--tags", "--prune", "origin"], Some(&db))
                .with_context(|| format!("fetch git-зависимости `{}`", url))?;
        }

        match resolve(&db) {
            Some(c) => c,
            None => {
                // `rev`/`tag` мог появиться после нашего клона — один fetch-ретрай.
                if !offline() && db_existed && !moving {
                    run_git(
                        &["fetch", "--quiet", "--tags", "--prune", "origin"],
                        Some(&db),
                    )
                    .with_context(|| format!("fetch git-зависимости `{}`", url))?;
                }
                resolve(&db).ok_or_else(|| {
                    anyhow!(
                        "git-зависимость `{}`: {} не найден в репозитории{}",
                        url,
                        pin_label(pin),
                        if offline() {
                            " (offline — fetch запрещён)"
                        } else {
                            ""
                        }
                    )
                })?
            }
        }
    };

    // --- 3. checkout рабочего дерева на commit'е -----------------------
    let checkout = cache_root.join("co").join(repo_id(url)).join(&commit);
    if !checkout.is_dir() {
        if let Some(parent) = checkout.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("не удалось создать каталог checkout {}", parent.display())
            })?;
        }
        // worktree из bare-репозитория разделяет object store db —
        // нужный commit гарантированно доступен.
        run_git(
            &[
                "worktree",
                "add",
                "--detach",
                "--quiet",
                &checkout.to_string_lossy(),
                &commit,
            ],
            Some(&db),
        )
        .with_context(|| {
            format!("checkout commit `{}` git-зависимости `{}`", commit, url)
        })?;
    }

    Ok(GitResolution { checkout, commit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Создать временный git-репозиторий-источник с Nova-пакетом.
    /// Возвращает (путь репозитория, commit hash).
    fn make_source_repo(name: &str) -> (PathBuf, String) {
        let dir = std::env::temp_dir().join(format!(
            "nova_git_src_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("mkdir src repo");
        fs::write(
            dir.join("nova.toml"),
            "[package]\nname = \"gitlib\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("calc.nv"),
            "module gitlib.calc\n\nexport fn add(a int, b int) -> int => a + b\n",
        )
        .unwrap();
        run_git(&["init", "--quiet", &dir.to_string_lossy()], None).unwrap();
        run_git(&["-C", &dir.to_string_lossy(), "config", "user.email", "t@t"], None).unwrap();
        run_git(&["-C", &dir.to_string_lossy(), "config", "user.name", "t"], None).unwrap();
        run_git(&["-C", &dir.to_string_lossy(), "add", "-A"], None).unwrap();
        run_git(
            &["-C", &dir.to_string_lossy(), "commit", "--quiet", "-m", "init"],
            None,
        )
        .unwrap();
        run_git(&["-C", &dir.to_string_lossy(), "tag", "v1.0.0"], None).unwrap();
        let commit = run_git(
            &["-C", &dir.to_string_lossy(), "rev-parse", "HEAD"],
            None,
        )
        .unwrap();
        (dir, commit)
    }

    fn temp_cache(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nova_git_cache_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn repo_id_stable_and_distinct() {
        assert_eq!(repo_id("https://x.org/a.git"), repo_id("https://x.org/a.git"));
        assert_ne!(repo_id("https://x.org/a.git"), repo_id("https://x.org/b.git"));
        assert!(repo_id("https://x.org/foo.git").starts_with("foo-"));
    }

    #[test]
    fn resolve_by_tag_then_cache_hit() {
        let (src, commit) = make_source_repo("tag");
        let cache = temp_cache("tag");
        let url = src.to_string_lossy().to_string();

        let r = resolve_git_dep_in(&cache, &url, &GitPin::Tag("v1.0.0".into()), None)
            .expect("resolve by tag");
        assert_eq!(r.commit, commit);
        assert!(r.checkout.join("nova.toml").is_file());
        assert!(r.checkout.join("calc.nv").is_file());

        // Повторный резолв — из кэша, тот же результат.
        let r2 = resolve_git_dep_in(&cache, &url, &GitPin::Tag("v1.0.0".into()), None)
            .expect("resolve by tag (cached)");
        assert_eq!(r2.commit, commit);
        assert_eq!(r2.checkout, r.checkout);

        fs::remove_dir_all(&src).ok();
        fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn resolve_by_rev_and_default() {
        let (src, commit) = make_source_repo("rev");
        let cache = temp_cache("rev");
        let url = src.to_string_lossy().to_string();

        let r = resolve_git_dep_in(&cache, &url, &GitPin::Rev(commit.clone()), None)
            .expect("resolve by rev");
        assert_eq!(r.commit, commit);

        let d = resolve_git_dep_in(&cache, &url, &GitPin::Default, None)
            .expect("resolve default branch");
        assert_eq!(d.commit, commit);

        fs::remove_dir_all(&src).ok();
        fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn unknown_tag_is_error() {
        let (src, _) = make_source_repo("badtag");
        let cache = temp_cache("badtag");
        let url = src.to_string_lossy().to_string();

        let err = resolve_git_dep_in(&cache, &url, &GitPin::Tag("v9.9.9".into()), None)
            .expect_err("unknown tag must fail");
        assert!(err.to_string().contains("v9.9.9"), "err: {}", err);

        fs::remove_dir_all(&src).ok();
        fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn locked_commit_pins_exactly() {
        let (src, commit) = make_source_repo("locked");
        let cache = temp_cache("locked");
        let url = src.to_string_lossy().to_string();

        // branch-пин, но locked_commit фиксирует точный commit.
        let r = resolve_git_dep_in(
            &cache,
            &url,
            &GitPin::Branch("nonexistent-branch".into()),
            Some(&commit),
        )
        .expect("locked commit overrides pin");
        assert_eq!(r.commit, commit);

        fs::remove_dir_all(&src).ok();
        fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn missing_repo_is_error() {
        let cache = temp_cache("missing");
        let err = resolve_git_dep_in(
            &cache,
            "d:/__nova_definitely_no_such_git_repo__",
            &GitPin::Tag("v1".into()),
            None,
        )
        .expect_err("missing repo must fail");
        assert!(err.to_string().contains("clone"), "err: {}", err);
        fs::remove_dir_all(&cache).ok();
    }

    /// Plan 03.2: git-репо с несколькими semver-тегами. Возвращает
    /// (путь, [(tag, commit)]).
    fn make_multi_tag_repo(name: &str, tags: &[&str]) -> (PathBuf, Vec<(String, String)>) {
        let dir = std::env::temp_dir().join(format!(
            "nova_git_mt_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().to_string();
        fs::write(
            dir.join("nova.toml"),
            "[package]\nname = \"gitlib\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n",
        )
        .unwrap();
        run_git(&["init", "--quiet", &d], None).unwrap();
        run_git(&["-C", &d, "config", "user.email", "t@t"], None).unwrap();
        run_git(&["-C", &d, "config", "user.name", "t"], None).unwrap();
        let mut out = Vec::new();
        for (i, tag) in tags.iter().enumerate() {
            fs::write(dir.join("calc.nv"), format!("module gitlib.calc\n\nexport fn v() -> int => {}\n", i)).unwrap();
            run_git(&["-C", &d, "add", "-A"], None).unwrap();
            run_git(&["-C", &d, "commit", "--quiet", "-m", tag], None).unwrap();
            run_git(&["-C", &d, "tag", tag], None).unwrap();
            let commit = run_git(&["-C", &d, "rev-parse", "HEAD"], None).unwrap();
            out.push((tag.to_string(), commit));
        }
        (dir, out)
    }

    #[test]
    fn resolve_by_version_range() {
        use crate::semver::VersionReq;
        let (src, tags) = make_multi_tag_repo(
            "verrange",
            &["v1.0.0", "v1.1.0", "v1.2.0", "v2.0.0"],
        );
        let cache = temp_cache("verrange");
        let url = src.to_string_lossy().to_string();
        let commit_of = |t: &str| {
            tags.iter().find(|(tag, _)| tag == t).map(|(_, c)| c.clone()).unwrap()
        };

        // ^1.0 → наибольший в 1.x → v1.2.0.
        let r = resolve_git_dep_in(
            &cache, &url, &GitPin::Version(VersionReq::parse("^1.0").unwrap()), None,
        )
        .expect("resolve ^1.0");
        assert_eq!(r.commit, commit_of("v1.2.0"), "^1.0 → v1.2.0");

        // ~1.1 → >=1.1.0,<1.2.0 → v1.1.0.
        let r = resolve_git_dep_in(
            &cache, &url, &GitPin::Version(VersionReq::parse("~1.1").unwrap()), None,
        )
        .expect("resolve ~1.1");
        assert_eq!(r.commit, commit_of("v1.1.0"), "~1.1 → v1.1.0");

        // * → наибольший вообще → v2.0.0.
        let r = resolve_git_dep_in(
            &cache, &url, &GitPin::Version(VersionReq::parse("*").unwrap()), None,
        )
        .expect("resolve *");
        assert_eq!(r.commit, commit_of("v2.0.0"), "* → v2.0.0");

        // ^3.0 → нет подходящего тега → ошибка.
        let err = resolve_git_dep_in(
            &cache, &url, &GitPin::Version(VersionReq::parse("^3.0").unwrap()), None,
        )
        .expect_err("^3.0 must fail");
        assert!(err.to_string().contains("тег не подходит"), "err: {}", err);

        fs::remove_dir_all(&src).ok();
        fs::remove_dir_all(&cache).ok();
    }
}
