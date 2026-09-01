//! Plan 204 — `[replace]` end-to-end + real nova-tls `file://` smoke.
//!
//! Плановая дельта поверх уже закрытых Plan 03.1/03.2 (git+semver deps,
//! backtracking-резолвер, `nova.lock`): `[replace]`-блок перекрывает
//! источник `[dependencies]`-записи для локальной разработки (go-школа).
//! Голая `path`-запись без соответствующей git+version формы (ВНЕ границы
//! git-репозитория) — warning (`manifest::manifest_warnings`), не ошибка.
//!
//! **Дофикс №2 (2026-07-13, владелец вскрыл дыру):** закоммиченный
//! `[replace]` ломает чистый клон — исправлено:
//!   1. `[replace]` живёт ТОЛЬКО в соседнем override-файле (не коммитится;
//!      Plan 233 §2а переименовал `nova.local.toml` -> `nova.override.toml`,
//!      старое имя всё ещё читается с deprecation warning).
//!      В КОММИЧЕННОМ `nova.toml` — жёсткая ошибка `E_REPLACE_IN_MANIFEST`
//!      (не warning, без депрекейшна).
//!   2. Go-scope: `[replace]` действует ТОЛЬКО для корня текущей сборки;
//!      в манифесте ЗАВИСИМОСТИ (обходимой транзитивно) — игнорируется
//!      (её собственный `effective_source` никогда не консультируется) +
//!      warning `W_REPLACE_IN_DEPENDENCY`.
//!   3. Отсутствующий путь в АКТИВНОМ корневом `[replace]` — честная
//!      ошибка `E_REPLACE_PATH_MISSING` (не тихий откат на git/declared).
//!
//! Источник для git-теста — ЛОКАЛЬНАЯ репа `nova-tls` (сосед-репозиторий,
//! `../nova-tls` относительно этого worktree) через `file://` URL — офлайн,
//! детерминированно, без сети (реальный `v0.1.0` тег уже заведён владельцем).

use nova_codegen::ast::Item;
use nova_codegen::imports;
use nova_codegen::lockfile::{self, LockedSource};
use nova_codegen::manifest;
use nova_codegen::parser;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// `NOVA_HOME` is a process-global env var (`git_cache` reads it per-call,
/// not per-test) — tests that mutate it must not run concurrently with each
/// other (Rust's default test harness runs `#[test]` fns in parallel
/// threads within the SAME process). Serialize via this lock; recover from
/// poisoning (`unwrap_or_else`) so one panicking test doesn't cascade-fail
/// every other lock-holder after it.
static NOVA_HOME_LOCK: Mutex<()> = Mutex::new(());

fn lock_nova_home() -> std::sync::MutexGuard<'static, ()> {
    NOVA_HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn git(args: &[&str], cwd: Option<&Path>) -> String {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let out = cmd.output().expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn unique(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "nova_p204_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    git(&["init", "--quiet", &d], None);
    git(&["-C", &d, "config", "user.email", "t@t"], None);
    git(&["-C", &d, "config", "user.name", "t"], None);
}

fn commit_tag(dir: &Path, tag: &str) -> String {
    let d = dir.to_string_lossy().to_string();
    git(&["-C", &d, "add", "-A"], None);
    git(&["-C", &d, "commit", "--quiet", "-m", tag], None);
    git(&["-C", &d, "tag", tag], None);
    git(&["-C", &d, "rev-parse", "HEAD"], None)
}

/// Plan 204 lockfix (D420, Cargo-семантика): `[replace]` overrides a
/// `{ git, version }` dependency to a local `path` for the BUILD only.
/// `nova.lock` must still record the RELEASE resolution — git url +
/// resolved semver version + commit of the tag — NOT the replace path
/// (replace is a local overlay and never leaks into the lock). A repeated
/// sync with replace still active leaves the lock byte-identical.
#[test]
fn replace_does_not_leak_into_lock() {
    let _guard = lock_nova_home();
    let libb = unique("libb");
    init_repo(&libb);
    fs::write(
        libb.join("nova.toml"),
        "[package]\nname = \"libb\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(libb.join("core.nv"), "module libb.core\n\nexport fn b() -> int => 1\n").unwrap();
    let libb_commit = commit_tag(&libb, "v1.0.0");
    let libb_url = libb.to_string_lossy().replace('\\', "/");

    // Local sibling override — points at libb's dev checkout directly.
    let libb_local = unique("libb_local");
    fs::create_dir_all(&libb_local).unwrap();
    fs::write(
        libb_local.join("nova.toml"),
        "[package]\nname = \"libb\"\nversion = \"1.0.0-dev\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(libb_local.join("core.nv"), "module libb.core\n\nexport fn b() -> int => 999\n").unwrap();

    let consumer = unique("consumer");
    fs::create_dir_all(&consumer).unwrap();
    let libb_local_rel = pathdiff(&consumer, &libb_local);
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nlibb = {{ git = \"{}\", version = \"^1.0\" }}\n\
             [replace]\nlibb = {{ path = \"{}\" }}\n",
            libb_url, libb_local_rel,
        ),
    )
    .unwrap();

    let m = manifest::parse_manifest(&consumer.join("nova.toml"), &consumer).expect("parse");
    // No bare-path warning: the DECLARED [dependencies] form is git+version;
    // path only appears as a [replace] override.
    let warnings = manifest::manifest_warnings(&m, &consumer.join("nova.toml"));
    assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    // Build-side resolution DOES honor the replace (imports.rs codepath).
    match m.effective_source(&m.dependencies[0]) {
        nova_codegen::manifest::DepSource::Path(p) => assert_eq!(p, libb_local_rel),
        other => panic!("effective_source must honor replace, got {:?}", other),
    }

    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);
    let res = lockfile::sync(&consumer);
    assert!(res.is_ok(), "sync with replace: {:?}", res.err());

    // Lock records the RELEASE resolution: git + version + tag commit.
    let lock = lockfile::load(&consumer).expect("load").expect("lock exists");
    assert_eq!(lock.packages.len(), 1);
    match &lock.packages[0].source {
        LockedSource::Git { url, version, commit, .. } => {
            assert_eq!(url, &libb_url);
            assert_eq!(version.as_deref(), Some("1.0.0"), "^1.0 → tag v1.0.0");
            assert_eq!(commit, &libb_commit, "commit of the release tag, not dev path");
        }
        other => panic!("lock must record git (release) source, got {:?}", other),
    }

    // Second sync with replace still active — lock byte-identical.
    // Plan 233 §2: lockfile writes always go to the NEW name `nova.lock.toml`.
    let text1 = fs::read_to_string(consumer.join("nova.lock.toml")).unwrap();
    lockfile::sync(&consumer).expect("second sync");
    std::env::remove_var("NOVA_HOME");
    let text2 = fs::read_to_string(consumer.join("nova.lock.toml")).unwrap();
    assert_eq!(text1, text2, "repeat sync with active replace must not rewrite lock");

    fs::remove_dir_all(&libb).ok();
    fs::remove_dir_all(&libb_local).ok();
    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}

/// `[replace]` referencing a name absent from `[dependencies]` — dedicated
/// diagnostic (`W_REPLACE_UNKNOWN_DEP`), nothing to replace.
#[test]
fn replace_unknown_dep_warns() {
    let dir = unique("replace_unknown_int");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("nova.toml"),
        "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
         [replace]\nghost = { path = \"../ghost\" }\n",
    )
    .unwrap();
    let m = manifest::parse_manifest(&dir.join("nova.toml"), &dir).expect("parse");
    let ws = manifest::manifest_warnings(&m, &dir.join("nova.toml"));
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].code, "W_REPLACE_UNKNOWN_DEP");
    fs::remove_dir_all(&dir).ok();
}

/// Bare `path`-dep in `[dependencies]` (no release form at all, no
/// `[replace]` involved) — `W_DEP_PATH_NO_RELEASE`, matching the existing
/// Plan 202/203 `{ path = "../nova-tls" }` pattern (must keep compiling —
/// warning only).
#[test]
fn bare_path_dep_warns_but_resolves() {
    let leaf = unique("leaf_bare");
    fs::create_dir_all(&leaf).unwrap();
    fs::write(
        leaf.join("nova.toml"),
        "[package]\nname = \"leaf\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();

    let app = unique("app_bare");
    fs::create_dir_all(&app).unwrap();
    let rel = pathdiff(&app, &leaf);
    fs::write(
        app.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nleaf = {{ path = \"{}\" }}\n",
            rel,
        ),
    )
    .unwrap();

    let m = manifest::parse_manifest(&app.join("nova.toml"), &app).expect("parse");
    let ws = manifest::manifest_warnings(&m, &app.join("nova.toml"));
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].code, "W_DEP_PATH_NO_RELEASE");

    // Still resolves fine (warning, not error) — collect_dep_graph succeeds.
    let graph = lockfile::collect_dep_graph(&app).expect("graph resolves despite warning");
    assert_eq!(graph.len(), 1);
    assert_eq!(graph[0].name, "leaf");
    assert!(matches!(graph[0].source, LockedSource::Path { .. }));
    fs::remove_dir_all(&leaf).ok();
    fs::remove_dir_all(&app).ok();
}

/// Real-world smoke (owner instruction, Plan 204): resolve `nova-tls`
/// `v0.1.0` from the ACTUAL sibling repo via a `file://` URL (git supports
/// local-path transports uniformly — same codepath as a real GitHub clone,
/// fully offline/deterministic). Skips gracefully if the sibling repo or
/// tag isn't present (CI checkout without nova-tls sibling).
#[test]
fn resolve_real_nova_tls_v0_1_0_via_file_url() {
    let _guard = lock_nova_home();
    // `cargo test`'s cwd is this crate's manifest dir (`<worktree>/compiler-codegen`);
    // nova-tls is a sibling of the WORKTREE root (`<parent-of-worktree>/nova-tls`),
    // so check both one and two levels up (plain worktree checkout vs.
    // running from the crate dir directly).
    let cwd = std::env::current_dir().unwrap();
    let nova_tls_dir = ["../nova-tls", "../../nova-tls", "../../../nova-tls"]
        .iter()
        .find_map(|rel| {
            let p = cwd.join(rel);
            match p.canonicalize() {
                Ok(p) if p.join(".git").exists() => Some(p),
                _ => None,
            }
        });
    let nova_tls_dir = match nova_tls_dir {
        Some(p) => p,
        None => {
            eprintln!("skip: sibling nova-tls repo not found next to worktree — not fatal");
            return;
        }
    };
    let tag_check = Command::new("git")
        .args(["-C", &nova_tls_dir.to_string_lossy(), "tag", "-l", "v0.1.0"])
        .output()
        .expect("run git tag -l");
    let has_tag = String::from_utf8_lossy(&tag_check.stdout).trim() == "v0.1.0";
    if !has_tag {
        eprintln!("skip: nova-tls has no v0.1.0 tag locally — not fatal");
        return;
    }

    // `canonicalize()` on Windows returns an extended-length `\\?\`-prefixed
    // path — strip it before building the `file://` URL (git's URL parser
    // doesn't understand the Windows verbatim prefix).
    let clean = nova_tls_dir
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let url = format!("file:///{}", clean);

    let consumer = unique("consumer_real_tls");
    fs::create_dir_all(&consumer).unwrap();
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ntls = {{ git = \"{}\", version = \"0.1\" }}\n",
            url,
        ),
    )
    .unwrap();

    let cache_home = unique("home_real_tls");
    std::env::set_var("NOVA_HOME", &cache_home);
    let res = lockfile::sync(&consumer);
    std::env::remove_var("NOVA_HOME");
    assert!(res.is_ok(), "sync real nova-tls via file://: {:?}", res.err());

    // Ожидание вычисляем ИЗ РЕПЫ, а не хардкодим: `^0.1` обязан выбрать
    // НАИБОЛЬШИЙ доступный 0.1.x. Прежняя версия теста ждала ровно "0.1.0" —
    // это был не инвариант, а снимок момента (тогда v0.1.0 и был старшим);
    // с появлением v0.1.1..v0.1.3 у соседней nova-tls тест покраснел, хотя
    // резолвер отработал ПРАВИЛЬНО (Plan 233-волна, 2026-07-27).
    let tags_out = Command::new("git")
        .args(["-C", &nova_tls_dir.to_string_lossy(), "tag", "-l", "v0.1.*"])
        .output()
        .expect("run git tag -l v0.1.*");
    let expected_patch = String::from_utf8_lossy(&tags_out.stdout)
        .lines()
        .filter_map(|t| t.trim().strip_prefix("v0.1.")?.parse::<u64>().ok())
        .max()
        .expect("at least one v0.1.x tag (guarded above)");
    let expected_version = format!("0.1.{}", expected_patch);

    let lock = lockfile::load(&consumer).expect("load").expect("lock exists");
    assert_eq!(lock.packages.len(), 1);
    match &lock.packages[0].source {
        LockedSource::Git { version, commit, .. } => {
            assert_eq!(
                version.as_deref(),
                Some(expected_version.as_str()),
                "^0.1 обязан резолвиться в НАИБОЛЬШИЙ доступный тег v0.1.x",
            );
            assert_eq!(commit.len(), 40, "full commit hash recorded");
        }
        other => panic!("expected Git lock entry, got {:?}", other),
    }

    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create_dir_all");
    }
    fs::write(path, content).expect("write fixture file");
}

fn fn_names(items: &[Item]) -> HashSet<String> {
    items
        .iter()
        .filter_map(|it| match it {
            Item::Fn(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect()
}

/// Plan 204 дофикс №2 (owner correction) / Plan 233 §2а (renamed):
/// `nova.override.toml` (не коммитится) merges its `[replace]` into the
/// effective manifest — REAL end-to-end resolution
/// (`imports::resolve_imports_inline_ex`), not just
/// `manifest::parse_manifest` inspection. `app`'s declared `libx` points at
/// a directory whose module defines `wrong_marker`; `nova.override.toml`
/// overrides it to a sibling defining `right_marker` instead. If the
/// override weren't honored, resolution would hard-fail (`right_marker`
/// undefined in `libx_wrong`) rather than silently pick the wrong one — a
/// self-checking assertion.
#[test]
fn local_toml_replace_is_honored_in_real_resolution() {
    let root = unique("local_toml_e2e");
    let proj = root.join("proj");

    // D420 (E_DEP_PATH_OUTSIDE_REPO) became a hard error on 2026-08-08 and moved
    // into the single resolution point the same day, AFTER this fixture was last
    // touched (2026-07-27) -- so the fixture asks for a bare `path` dep from a tree
    // that is not a repository, and the refusal is correct. A real git is not
    // needed: the check looks for a `.git` entry. Same remedy as f9505aa45, which
    // fixed three unit tests of this class -- two of them about `[replace]` scope,
    // exactly this subject -- by adding the marker rather than weakening the rule.
    fs::create_dir_all(proj.join(".git")).unwrap();
    let app_dir = proj.join("app");
    write_file(
        &app_dir.join("nova.toml"),
        "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
         [dependencies]\nlibx = { path = \"../libx_wrong\" }\n",
    );
    write_file(
        &app_dir.join("nova.override.toml"),
        "[replace]\nlibx = { path = \"../libx_right\" }\n",
    );
    write_file(
        &app_dir.join("app.nv"),
        "module app\n\nimport libx.{right_marker}\n\nfn main() -> int => right_marker()\n",
    );

    write_file(
        &proj.join("libx_wrong").join("nova.toml"),
        "[package]\nname = \"libx\"\n[lib]\nsrc = \".\"\n",
    );
    write_file(
        &proj.join("libx_wrong").join("libx.nv"),
        "module libx\n\nexport fn wrong_marker() -> int => 1\n",
    );
    write_file(
        &proj.join("libx_right").join("nova.toml"),
        "[package]\nname = \"libx\"\n[lib]\nsrc = \".\"\n",
    );
    write_file(
        &proj.join("libx_right").join("libx.nv"),
        "module libx\n\nexport fn right_marker() -> int => 2\n",
    );

    let app_nv = app_dir.join("app.nv");
    let src = fs::read_to_string(&app_nv).expect("read entry");
    let mut module = parser::parse(&src).expect("entry parses");
    let stdlib = root.join("no_stdlib");

    imports::resolve_imports_inline_ex(&app_nv, &mut module, &proj, &stdlib, false).expect(
        "nova.override.toml [replace] must be honored — resolution must pick \
         libx_right (right_marker), not the declared libx_wrong",
    );
    assert!(fn_names(&module.items).contains("right_marker"));

    fs::remove_dir_all(&root).ok();
}

/// Plan 204 дофикс №2 (D420 go-scope + owner correction, full git flavor —
/// exact scenario from the task spec): package `app` (root) depends on a
/// GIT dependency `b`; `b`'s OWN manifest declares a normal path-dep `c`
/// AND a `[replace]` override for `c` pointing at a path that DOESN'T
/// EXIST. `app`'s build must still succeed (b's `[replace]` is inert —
/// go-scope: only the root's `[replace]` is ever consulted) — if the
/// dofix#2 bug were still present, `b`'s own files resolving their own `c`
/// import would hard-fail with the missing replace path. A
/// `W_REPLACE_IN_DEPENDENCY` warning must also surface.
#[test]
fn nested_git_dependency_replace_ignored_build_succeeds_with_warning() {
    let _guard = lock_nova_home();
    let b_repo = unique("b_repo");
    init_repo(&b_repo);
    write_file(
        &b_repo.join("nova.toml"),
        "[package]\nname = \"b\"\nversion = \"1.0.0\"\n[lib]\nsrc = \".\"\n\
         [dependencies]\nc = { path = \"cdep\" }\n\
         [replace]\nc = { path = \"nonexistent_c_override\" }\n",
    );
    write_file(&b_repo.join("b.nv"), "module b\n\nimport c.{c_fn}\n\nexport fn b_fn() -> int => c_fn()\n");
    write_file(
        &b_repo.join("cdep").join("nova.toml"),
        "[package]\nname = \"c\"\n[lib]\nsrc = \".\"\n",
    );
    write_file(&b_repo.join("cdep").join("c.nv"), "module c\n\nexport fn c_fn() -> int => 7\n");
    commit_tag(&b_repo, "v1.0.0");
    let b_url = b_repo.to_string_lossy().replace('\\', "/");

    let a_dir = unique("consumer_a_nested");
    write_file(
        &a_dir.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nb = {{ git = \"{}\", tag = \"v1.0.0\" }}\n",
            b_url,
        )
        .as_str(),
    );
    write_file(&a_dir.join("app.nv"), "module app\n\nimport b.{b_fn}\n\nfn main() -> int => b_fn()\n");

    let cache_home = unique("home_nested");
    std::env::set_var("NOVA_HOME", &cache_home);
    let sync_res = lockfile::sync(&a_dir);
    assert!(sync_res.is_ok(), "app build (lockfile sync) must succeed despite b's broken [replace]: {:?}", sync_res.err());

    let app_nv = a_dir.join("app.nv");
    let src = fs::read_to_string(&app_nv).expect("read entry");
    let mut module = parser::parse(&src).expect("entry parses");
    let stdlib = a_dir.join("no_stdlib");
    let resolve_res = imports::resolve_imports_inline_ex(&app_nv, &mut module, &a_dir, &stdlib, false);
    std::env::remove_var("NOVA_HOME");
    assert!(
        resolve_res.is_ok(),
        "resolution must succeed — b's OWN [replace] (nonexistent path) must be \
         ignored (go-scope: not root): {:?}",
        resolve_res.err(),
    );
    let names = fn_names(&module.items);
    assert!(names.contains("b_fn"), "b's b_fn merged");
    assert!(names.contains("c_fn"), "c's REAL c_fn merged via b's declared path, not b's ignored replace");

    let warnings = lockfile::collect_replace_scope_warnings(&a_dir);
    assert!(
        warnings.iter().any(|w| w.code == "W_REPLACE_IN_DEPENDENCY" && w.message.contains('c')),
        "expected W_REPLACE_IN_DEPENDENCY mentioning `c`, got {:?}", warnings,
    );

    fs::remove_dir_all(&b_repo).ok();
    fs::remove_dir_all(&a_dir).ok();
    fs::remove_dir_all(&cache_home).ok();
}

/// Plan 204 дофикс №2 owner correction / Plan 233 §2а (renamed): missing
/// path behind an ACTIVE ROOT `[replace]` override (nova.override.toml) —
/// dedicated `E_REPLACE_PATH_MISSING` error, NOT a silent fallback to the
/// declared git/path source.
#[test]
fn root_replace_missing_path_is_honest_error() {
    let root = unique("replace_missing_e2e");
    let proj = root.join("proj");
    // D420 (E_DEP_PATH_OUTSIDE_REPO) became a hard error on 2026-08-08 and moved
    // into the single resolution point the same day, AFTER this fixture was last
    // touched (2026-07-27) -- so the fixture asks for a bare `path` dep from a tree
    // that is not a repository, and the refusal is correct. A real git is not
    // needed: the check looks for a `.git` entry. Same remedy as f9505aa45, which
    // fixed three unit tests of this class -- two of them about `[replace]` scope,
    // exactly this subject -- by adding the marker rather than weakening the rule.
    fs::create_dir_all(proj.join(".git")).unwrap();
    let app_dir = proj.join("app");
    write_file(
        &app_dir.join("nova.toml"),
        "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
         [dependencies]\nlibx = { path = \"../libx_real\" }\n",
    );
    write_file(
        &app_dir.join("nova.override.toml"),
        "[replace]\nlibx = { path = \"../libx_does_not_exist\" }\n",
    );
    write_file(
        &app_dir.join("app.nv"),
        "module app\n\nimport libx.{whatever}\n\nfn main() -> int => whatever()\n",
    );
    write_file(
        &proj.join("libx_real").join("nova.toml"),
        "[package]\nname = \"libx\"\n[lib]\nsrc = \".\"\n",
    );
    write_file(
        &proj.join("libx_real").join("libx.nv"),
        "module libx\n\nexport fn whatever() -> int => 1\n",
    );
    // NOTE: `proj/libx_does_not_exist` deliberately never created.

    let app_nv = app_dir.join("app.nv");
    let src = fs::read_to_string(&app_nv).expect("read entry");
    let mut module = parser::parse(&src).expect("entry parses");
    let stdlib = root.join("no_stdlib");
    let err = imports::resolve_imports_inline_ex(&app_nv, &mut module, &proj, &stdlib, false)
        .expect_err("missing [replace] path must be a hard error, not a silent fallback");
    assert!(
        format!("{}", err).contains("E_REPLACE_PATH_MISSING"),
        "err: {}", err,
    );

    fs::remove_dir_all(&root).ok();
}

/// Plan 204 дофикс №2 (owner correction): `[replace]` declared directly in
/// the COMMITTED `nova.toml` — `manifest::check_no_committed_replace` must
/// hard-Err (`E_REPLACE_IN_MANIFEST`), no deprecation window (legacy zero —
/// nova-http, the one real consumer, migrated to `nova.local.toml`).
#[test]
fn committed_replace_hard_errors_e2e() {
    let dir = unique("committed_replace_e2e");
    write_file(
        &dir.join("nova.toml"),
        "[package]\nname = \"app\"\n[lib]\nsrc = \".\"\n\
         [dependencies]\ntls = { git = \"https://example.org/tls\", version = \"0.1\" }\n\
         [replace]\ntls = { path = \"../nova-tls\" }\n",
    );
    let toml_path = dir.join("nova.toml");
    let m = manifest::parse_manifest(&toml_path, &dir).expect("parse");
    let err = manifest::check_no_committed_replace(&m, &toml_path)
        .expect_err("committed [replace] must hard-error");
    assert!(err.contains("E_REPLACE_IN_MANIFEST"), "err: {}", err);

    fs::remove_dir_all(&dir).ok();
}

/// Naive relative-path from `from` to `to` (both must exist) — good enough
/// for temp-dir siblings under the same OS temp root (no `..`-nesting
/// beyond one level needed in these fixtures).
fn pathdiff(from: &Path, to: &Path) -> String {
    let from_parent = from.parent().unwrap();
    let to_name = to.file_name().unwrap().to_string_lossy();
    if from_parent == to.parent().unwrap() {
        format!("../{}", to_name)
    } else {
        to.to_string_lossy().replace('\\', "/")
    }
}
