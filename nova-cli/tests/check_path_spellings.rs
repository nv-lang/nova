//! `nova check .` обязан проверять, а не молчать (реестр 221.1 №724).
//!
//! ЧТО БЫЛО. `classify_skip_path` шёл по ВСЕМ компонентам пути найденного файла
//! и объявлял файл служебным, если хоть один компонент начинается с `.` или `_`.
//! Путь, записанный как `./src`, имеет первым компонентом `.` — значит служебным
//! объявлялся КАЖДЫЙ файл, обход оставался пустым, команда печатала «no .nv
//! files to check» и возвращала НОЛЬ. Замер на рабочей области с одной
//! намеренной ошибкой типа: `.` → 0/ничего, `./src` → 0/ничего, `src` →
//! 1/ошибка найдена, абсолютный путь → 1/ошибка найдена.
//!
//! ПОЧЕМУ ЭТО ХУЖЕ ОБЫЧНОЙ ОШИБКИ. Отказ неотличим от успеха: `nova check .` в
//! чужом CI печатает бодрое «no .nv files to check» и выходит нулём на проекте,
//! который не компилируется. Ни один тест этого не ловил, потому что все
//! существующие звали `nova check <файл>`, а файловая ветка классификатор
//! обходит вовсе.
//!
//! ЧТО ПРОВЕРЯЕТСЯ ЗДЕСЬ — ровно то, что отличало красное от зелёного: ОДНА
//! рабочая область, ОДНА заведомая ошибка, и все написания пути к ней обязаны
//! дать один ответ. Плюс обратная сторона: правило про служебные каталоги
//! никуда не делось и продолжает работать ВНУТРИ дерева.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-cli has a parent dir")
        .to_path_buf()
}

/// Эфемерная рабочая область с ОДНИМ файлом, который заведомо не проходит
/// проверку типов. Лежит под `target/` намеренно: `target` — служебное имя, и
/// то, что оно стоит НАД корнем обхода, не должно дисквалифицировать дерево.
fn make_broken_workspace(name: &str) -> PathBuf {
    let ws = repo_root().join("target").join(name);
    let _ = fs::remove_dir_all(&ws);
    fs::create_dir_all(ws.join("src")).expect("mkdir src/");
    fs::write(
        ws.join("nova.toml"),
        "[package]\nname = \"spellings\"\nversion = \"0.1.0\"\n\n[lib]\nsrc = \"src\"\n",
    )
    .expect("write nova.toml");
    fs::write(
        ws.join("src").join("main.nv"),
        "module spellings.main\n\nfn main() -> () {\n    ro q int = \"deliberately not an int\"\n}\n",
    )
    .expect("write main.nv");
    ws
}

fn run_check(cwd: &Path, arg: &str) -> (bool, String) {
    // `--color never` ОБЯЗАТЕЛЕН, а не для красоты: `nova` включает цвет
    // автоматически, когда видит терминал, и тогда в выводе стоит не
    // `FAIL: 1`, а `FAIL<ESC>[0m: 1`. Тест, который проходит под `cargo test`
    // и падает под тем же прогоном из-под стража, — это не тест, а лотерея;
    // поймано ровно так, при первом настоящем прогоне check-crate-tests.
    let out = Command::new(env!("CARGO_BIN_EXE_nova"))
        .arg("check")
        .arg("--color")
        .arg("never")
        .arg(arg)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn `nova check`");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn check_finds_the_error_however_the_path_is_spelled() {
    let ws = make_broken_workspace("check_spellings_ws");
    let abs = ws.join("src");
    let abs = abs.to_string_lossy().to_string();

    for arg in [".", "./src", "src", abs.as_str()] {
        let (ok, text) = run_check(&ws, arg);
        assert!(
            !ok,
            "`nova check {arg}` reported SUCCESS on a workspace that does not \
             type-check.\nA green light indistinguishable from a real one is the \
             whole defect.\noutput:\n{text}"
        );
        assert!(
            !text.contains("no .nv files to check"),
            "`nova check {arg}` walked the tree and found no sources at all.\n\
             output:\n{text}"
        );
    }
}

#[test]
fn housekeeping_dirs_are_still_skipped_inside_the_tree() {
    // Обратная сторона той же правки: сузив правило до содержимого дерева,
    // легко снять его вовсе. `_private` обязан по-прежнему пропускаться.
    let ws = make_broken_workspace("check_spellings_housekeeping_ws");
    let hidden = ws.join("src").join("_private");
    fs::create_dir_all(&hidden).expect("mkdir _private");
    fs::write(
        hidden.join("main.nv"),
        "module _private.main\n\nfn main() -> () {\n    ro q int = \"also broken\"\n}\n",
    )
    .expect("write _private/main.nv");

    let (_ok, text) = run_check(&ws, "src");
    // Ровно ОДИН проверенный файл: `_private` пропущен. Два означали бы, что,
    // сузив правило до содержимого дерева, я снял его вовсе.
    assert!(
        text.contains("FAIL: 1"),
        "exactly ONE file should have been checked — the `_private` peer must \
         stay skipped.\noutput:\n{text}"
    );
    assert!(
        !text.contains("FAIL: 2"),
        "the `_private` peer was checked too — the housekeeping rule is gone.\n\
         output:\n{text}"
    );
    // Пропуск при этом НЕ сообщается: разбивку по причинам `nova check` печатает
    // только когда не осталось ни одного файла. Это отдельная скупость вывода, а
    // не часть этого дефекта, — фиксирую наблюдением, а не требованием.
}
