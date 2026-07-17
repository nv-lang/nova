// Plan 185 Ф.3 ([M-185-lint-deny-gate]): `nova lint --deny` — CI/приёмочный
// гейт (W→E). Без `--deny` находки реестра (compiler-codegen/src/lints.rs
// ::CONV_RULES) — информационные: печатаются как `warning:`, exit 0 даже при
// находках (как rustc warn-lints). С `--deny` (или `--deny=W_X,W_Y`) денай-
// находки печатаются как `error:` и переводят exit-код в 1.
//
// Гоняем против реально собранного `nova`-бинаря (как interp_unsupported.rs)
// — это контракт CLI (флаг + exit-код + текст), не мок внутренней функции.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nova() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nova"))
}

/// Изолированная temp-директория с одним файлом, несущим заведомую находку
/// W_RETIRED_PREFIX (`as_`-префикс, D410 ретракция) — чистое AST-правило без
/// in_test/in_vec_module-исключений (в отличие от W_VEC_SPELLING), поэтому
/// срабатывает детерминированно независимо от того, где лежит temp-файл.
/// Изоляция обязательна: Nova трактует директорию как один folder-module
/// co-equal файлов, поэтому расшаренный temp-каталог собрал бы файлы разных
/// тестов в один модуль и столкнул бы `fn main`.
fn isolated_nv_with_finding(tag: &str) -> (PathBuf, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_lint_deny_{}_{}", std::process::id(), tag));
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    let file = dir.join("main.nv");
    fs::write(
        &file,
        "module t\n\nfn as_thing() -> int {\n    return 0\n}\n\nfn main() {\n}\n",
    )
    .expect("write temp .nv");
    (dir, file)
}

fn isolated_nv_clean(tag: &str) -> (PathBuf, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_lint_deny_{}_{}", std::process::id(), tag));
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    let file = dir.join("main.nv");
    fs::write(&file, "module t\n\nfn main() {\n}\n").expect("write temp .nv");
    (dir, file)
}

fn combined_output(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Без `--deny`: находка есть, но она информационная — `warning:` в выводе,
/// exit 0 (иначе `nova lint` без `--deny` был бы неотличим от жёсткого
/// гейта, что и было хвостом Ф.3 до этого фикса).
#[test]
fn nova_lint_without_deny_warns_but_exits_zero() {
    let (dir, file) = isolated_nv_with_finding("plain");
    let out = nova().arg("lint").arg(&file).output().expect("spawn `nova lint`");
    let _ = fs::remove_dir_all(&dir);
    let combined = combined_output(&out);

    assert!(
        out.status.success(),
        "`nova lint` (без --deny) должен завершаться exit 0 даже при находках \
         (info-only); status={:?}\n{combined}",
        out.status
    );
    assert!(
        combined.contains("W_RETIRED_PREFIX") && combined.contains("warning:"),
        "ожидалась находка W_RETIRED_PREFIX уровня `warning:`; получено:\n{combined}"
    );
    assert!(
        !combined.contains("error:"),
        "без --deny находки не должны печататься как `error:`; получено:\n{combined}"
    );
}

/// С bare `--deny`: та же находка теперь `error:`, exit ≠ 0 (жёсткий
/// CI/приёмочный гейт).
#[test]
fn nova_lint_deny_promotes_to_error_and_exits_nonzero() {
    let (dir, file) = isolated_nv_with_finding("deny-all");
    let out = nova()
        .arg("lint")
        .arg("--deny")
        .arg(&file)
        .output()
        .expect("spawn `nova lint --deny`");
    let _ = fs::remove_dir_all(&dir);
    let combined = combined_output(&out);

    assert!(
        !out.status.success(),
        "`nova lint --deny` должен завершаться exit ≠0 при любой денай-находке; \
         status={:?}\n{combined}",
        out.status
    );
    assert!(
        combined.contains("W_RETIRED_PREFIX") && combined.contains("error:"),
        "ожидалась денай-находка W_RETIRED_PREFIX уровня `error:`; получено:\n{combined}"
    );
}

/// `--deny=W_OTHER` (правило, не совпадающее с реальной находкой) — находка
/// остаётся `warning:`-only, прогон не валится: выборочный деай не
/// затрагивает недenied-правила.
#[test]
fn nova_lint_deny_selective_rule_does_not_deny_unrelated_finding() {
    let (dir, file) = isolated_nv_with_finding("deny-other");
    let out = nova()
        .arg("lint")
        .arg("--deny=W_NONVARIADIC_OF")
        .arg(&file)
        .output()
        .expect("spawn `nova lint --deny=W_NONVARIADIC_OF`");
    let _ = fs::remove_dir_all(&dir);
    let combined = combined_output(&out);

    assert!(
        out.status.success(),
        "--deny другого правила не должен валить прогон на W_RETIRED_PREFIX; \
         status={:?}\n{combined}",
        out.status
    );
    assert!(
        combined.contains("W_RETIRED_PREFIX") && combined.contains("warning:"),
        "находка должна остаться `warning:`-only; получено:\n{combined}"
    );
}

/// `--deny=W_RETIRED_PREFIX` (правило, совпадающее с реальной находкой) —
/// выборочный денай СРАБАТЫВАЕТ: `error:` + exit ≠0.
#[test]
fn nova_lint_deny_selective_rule_denies_matching_finding() {
    let (dir, file) = isolated_nv_with_finding("deny-match");
    let out = nova()
        .arg("lint")
        .arg("--deny=W_RETIRED_PREFIX")
        .arg(&file)
        .output()
        .expect("spawn `nova lint --deny=W_RETIRED_PREFIX`");
    let _ = fs::remove_dir_all(&dir);
    let combined = combined_output(&out);

    assert!(
        !out.status.success(),
        "--deny=W_RETIRED_PREFIX должен денаить именно эту находку; \
         status={:?}\n{combined}",
        out.status
    );
    assert!(
        combined.contains("W_RETIRED_PREFIX") && combined.contains("error:"),
        "ожидался `error:` для денай-правила; получено:\n{combined}"
    );
}

/// Чистый файл (без находок) — exit 0 и с `--deny`, и без: `--deny` не
/// придумывает находки на пустом месте.
#[test]
fn nova_lint_clean_file_exits_zero_with_or_without_deny() {
    let (dir, file) = isolated_nv_clean("clean");
    let out = nova()
        .arg("lint")
        .arg("--deny")
        .arg(&file)
        .output()
        .expect("spawn `nova lint --deny` on clean file");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        out.status.success(),
        "чистый файл должен давать exit 0 даже с --deny; status={:?}\n{}",
        out.status,
        combined_output(&out)
    );
}
