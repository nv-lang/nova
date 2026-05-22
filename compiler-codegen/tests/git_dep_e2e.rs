//! Plan 03.1 Ф.2 — end-to-end: проект с `git`-зависимостью резолвится
//! через полный `resolve_imports_inline` (git clone → checkout в кэше →
//! межпакетный резолв Ф.3).
//!
//! Источник зависимости — локальный git-репозиторий (`git init` в temp),
//! поэтому тест offline и детерминирован. Один `#[test]` в файле —
//! `NOVA_HOME` ставится глобально без гонки с другими тестами.

use nova_codegen::ast::Item;
use nova_codegen::imports::resolve_imports_inline;
use nova_codegen::parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(args: &[&str], cwd: Option<&Path>) {
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
}

fn unique(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "nova_gite2e_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn git_dependency_resolves_end_to_end() {
    // --- 1. git-репозиторий-источник: Nova-пакет `gitlib` ---------------
    let src = unique("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("nova.toml"),
        "[package]\nname = \"gitlib\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n",
    )
    .unwrap();
    fs::write(
        src.join("calc.nv"),
        "module gitlib.calc\n\nexport fn add(a int, b int) -> int => a + b\n",
    )
    .unwrap();
    git(&["init", "--quiet", &src.to_string_lossy()], None);
    git(&["config", "user.email", "t@t"], Some(&src));
    git(&["config", "user.name", "t"], Some(&src));
    git(&["add", "-A"], Some(&src));
    git(&["commit", "--quiet", "-m", "init"], Some(&src));
    git(&["tag", "v1.0.0"], Some(&src));

    // --- 2. проект-потребитель с git-зависимостью ----------------------
    let consumer = unique("consumer");
    fs::create_dir_all(&consumer).unwrap();
    fs::write(
        consumer.join("nova.toml"),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\ngitlib = {{ git = \"{}\", tag = \"v1.0.0\" }}\n",
            src.to_string_lossy().replace('\\', "/"),
        ),
    )
    .unwrap();
    let main_path = consumer.join("main.nv");
    fs::write(
        &main_path,
        "module app.main\n\nimport gitlib.calc.{add}\n\n\
         fn use_dep(x int) -> int => add(x, 1)\n",
    )
    .unwrap();

    // --- 3. изолированный кэш git -------------------------------------
    let cache_home = unique("home");
    std::env::set_var("NOVA_HOME", &cache_home);

    // --- 4. резолв импортов entry --------------------------------------
    let src_text = fs::read_to_string(&main_path).unwrap();
    let mut module = parser::parse(&src_text).expect("parse main.nv");
    // stdlib_dir указываем несуществующим — `std`-импортов и prelude нет.
    let no_stdlib = consumer.join("__no_stdlib__");
    let res = resolve_imports_inline(&main_path, &mut module, &consumer, &no_stdlib);

    std::env::remove_var("NOVA_HOME");

    assert!(res.is_ok(), "resolve git dependency: {:?}", res.err());

    // `add` из git-зависимости должен быть слит в items entry-модуля.
    let has_add = module
        .items
        .iter()
        .any(|it| matches!(it, Item::Fn(f) if f.name == "add"));
    assert!(has_add, "функция `add` из git-зависимости не подмержена");

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&consumer).ok();
    fs::remove_dir_all(&cache_home).ok();
}
