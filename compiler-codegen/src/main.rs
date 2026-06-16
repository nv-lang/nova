//! Nova codegen compiler CLI.

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "nova-codegen", version, about = "Nova codegen compiler — compiles Nova to C")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Type-check файл, без запуска.
    Check {
        file: PathBuf,
        /// Plan 123.5 (D217 §6 amend): emit field-cache analysis
        /// report — per-fn decisions of D217+D218+D219+chain каches.
        /// Human-readable text on stdout.
        #[arg(long = "explain-cache")]
        explain_cache: bool,
    },
    /// [UNSUPPORTED] tree-walking interpreter — use C codegen
    /// (`nova-codegen compile`, or `nova build` / `nova test`).
    Run { file: PathBuf },
    /// [UNSUPPORTED] interpreter-driven tests — use C codegen
    /// (`nova test`, or `nova-codegen compile`).
    #[command(name = "test-interp")]
    TestInterp { file: PathBuf },
    /// Скомпилировать Nova-файл в C (вывод в stdout или -o файл).
    Compile {
        file: PathBuf,
        /// Выходной .c файл (по умолчанию: <name>.c)
        #[arg(short = 'o')]
        output: Option<PathBuf>,
        /// Не вставлять Nova-исходник как `/* SRC: ... */` комментарии.
        /// По умолчанию аннотации включены — для удобства отладки .c.
        #[arg(long = "no-annotate-source")]
        no_annotate_source: bool,
        /// Отключить lint-проверки (export-fail-untyped и т.д.).
        #[arg(long = "no-lint")]
        no_lint: bool,
        /// Plan 140 Ф.2 (D24 amend): contract build-policy.
        /// `enforce` (default) — недоказанные контракты проверяются в runtime
        /// (debug И release; fail-fast abort), Z3-proven элидируются.
        /// `off` — все контракт-проверки элидируются глобально (legacy
        /// zero-cost; недоказанность под ответственность разработчика).
        #[arg(long = "contracts", value_parser = ["enforce", "off"], default_value = "enforce")]
        contracts: String,
    },
    /// Plan 13: auto-gen `std/runtime/string.nv` и `std/runtime/math.nv`
    /// из `runtime_registry.rs`. Файлы перезаписываются.
    /// `--check` режим — сравнить с существующими файлами; если diff,
    /// fail (для CI guard).
    EmitRuntimeStubs {
        /// Корень репозитория (где лежит `std/runtime/`). По умолчанию
        /// текущая директория.
        #[arg(long = "root", default_value = ".")]
        root: PathBuf,
        /// Не записывать; сравнить с существующими и упасть при
        /// несовпадении. Для CI/pre-commit guard.
        #[arg(long = "check")]
        check: bool,
    },
    /// Plan 13: распечатать registry runtime-функций для sanity.
    DumpRuntime,
    /// Plan 152.4.1 (Q-unicode-data): сгенерировать таблицы нормализации
    /// `std/unicode/norm_data.nv` из UCD (UnicodeData.txt + CompositionExclusions
    /// + DerivedNormalizationProps). Internal build tool — не через `nova` CLI.
    /// `--check` — сравнить с существующим файлом и fail при diff (CI guard).
    Unicode {
        /// Директория с UCD-файлами (UnicodeData.txt и т.д.).
        #[arg(long = "ucd-dir")]
        ucd_dir: PathBuf,
        /// Корень репозитория (куда писать `std/unicode/`). По умолчанию `.`.
        #[arg(long = "root", default_value = ".")]
        root: PathBuf,
        /// Версия Unicode, под которую сгенерированы таблицы (пин).
        #[arg(long = "unicode-version", default_value = "16.0")]
        unicode_version: String,
        /// Также сгенерировать UAX #15 conformance-фикстуру
        /// `nova_tests/plan152_4/normalization_conformance.nv` из
        /// NormalizationTest.txt (capped — см. `--conformance-limit`).
        #[arg(long = "emit-conformance")]
        emit_conformance: bool,
        /// Максимум case'ов в conformance-фикстуре (compile/run-time bound).
        /// Part 0 целиком + stride-выборка Parts 1-5 (чанки по 500 на test).
        #[arg(long = "conformance-limit", default_value_t = 1500)]
        conformance_limit: usize,
        /// Plan 156: ALSO emit full (uncapped) *_conformance_slow.nv slow-lane files.
        #[arg(long = "conformance-full")]
        conformance_full: bool,
        /// Не записывать; сравнить с существующим и упасть при несовпадении.
        #[arg(long = "check")]
        check: bool,
    },
    /// Plan 24: cross-platform test runner — сборка одного .nv в .exe
    /// и проверка EXPECT-маркера (D89). Заменяет per-file логику из
    /// run_tests.ps1.
    TestBuild {
        file: PathBuf,
        /// dev (по умолчанию) | release.
        #[arg(long, default_value = "dev")]
        mode: String,
        /// auto (по умолчанию) | clang | msvc | gcc.
        #[arg(long, default_value = "auto")]
        toolchain: String,
        /// Путь к vcvars64.bat (Windows). Auto-detect через vswhere.
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Путь к clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Путь к compiler-codegen/ (для include nova_rt headers).
        /// По умолчанию вычисляется из layout репо.
        #[arg(long = "cg-include")]
        cg_include: Option<PathBuf>,
        /// Путь к compiler-codegen/nova_rt/ (alloc.c/effects.c/fibers.c).
        #[arg(long = "rt-dir")]
        rt_dir: Option<PathBuf>,
        /// Tmp директория для .exe/.obj артефактов.
        #[arg(long = "tmp-dir")]
        tmp_dir: Option<PathBuf>,
        /// Display name (override; по умолчанию — basename файла).
        #[arg(long)]
        display: Option<String>,
        /// Сохранить .c / .exe / .obj артефакты после прогона.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// Plan 26 Ф.1: timeout на child-процесс в секундах. Default 60.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Plan 27 Ф.4: GC backend. boehm = Boehm GC (default). malloc = plain malloc (bench only).
        #[arg(long, value_parser = ["boehm", "malloc"], default_value = "boehm")]
        gc: String,
        /// Plan 140 Ф.2 (D24 amend): contract build-policy. `enforce`
        /// (default) — недоказанные контракты проверяются (debug И release);
        /// `off` — все контракт-проверки элидируются глобально (legacy).
        #[arg(long = "contracts", value_parser = ["enforce", "off"], default_value = "enforce")]
        contracts: String,
    },
    /// Plan 24: рекурсивный прогон всех .nv в `--tests-dir`. Заменяет
    /// run_tests.ps1 целиком; .ps1 / .sh wrapper'ы вызывают эту команду.
    TestAll {
        /// Корень nova_tests/ (рекурсивный поиск .nv).
        #[arg(long = "tests-dir", default_value = "nova_tests")]
        tests_dir: PathBuf,
        /// Корень std/ — добавляется если --include-stdlib.
        #[arg(long = "stdlib-dir", default_value = "std")]
        stdlib_dir: PathBuf,
        /// Включить std/* файлы в прогон.
        #[arg(long = "include-stdlib")]
        include_stdlib: bool,
        /// Plan 156: include *_slow.nv large/slow tests (default: skipped).
        #[arg(long = "include-slow")]
        include_slow: bool,
        /// Plan 156: run ONLY *_slow.nv large/slow tests.
        #[arg(long = "slow-only")]
        slow_only: bool,
        /// Фильтр по display-name (substring).
        #[arg(long)]
        filter: Option<String>,
        /// dev | release.
        #[arg(long, default_value = "dev")]
        mode: String,
        /// auto | clang | msvc | gcc.
        #[arg(long, default_value = "auto")]
        toolchain: String,
        /// Путь к vcvars64.bat (Windows).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Путь к clang.exe.
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Путь к compiler-codegen/.
        #[arg(long = "cg-include")]
        cg_include: Option<PathBuf>,
        /// Путь к compiler-codegen/nova_rt/.
        #[arg(long = "rt-dir")]
        rt_dir: Option<PathBuf>,
        /// Tmp директория. По умолчанию $TEMP/nova_tests или /tmp/nova_tests.
        #[arg(long = "tmp-dir")]
        tmp_dir: Option<PathBuf>,
        /// Сохранить .exe/.obj артефакты.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// Plan 26 Ф.1: timeout на child-процесс в секундах. Default 60.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Plan 26 Ф.3: количество параллельных worker'ов. 0 = num_cpus.
        #[arg(long, default_value_t = 0)]
        jobs: usize,
        /// Plan 26 Ф.4: text (default, human) | json | tap.
        #[arg(long, default_value = "text")]
        format: String,
        /// Plan 26 Ф.9: показывать output PASS-тестов тоже.
        #[arg(long, short = 'v')]
        verbose: bool,
        /// Plan 26 Ф.9: только FAIL + summary.
        #[arg(long, short = 'q')]
        quiet: bool,
        /// Plan 26 Ф.10: файл для last-results.json (для --rerun-failed).
        #[arg(long = "results-file")]
        results_file: Option<PathBuf>,
        /// Plan 26 Ф.10: прогнать только тесты которые fail/timeout
        /// в --results-file.
        #[arg(long = "rerun-failed")]
        rerun_failed: bool,
        /// Plan 26 Ф.12: количество retry для transient AV/race fail'ов
        /// (e.g., 'cannot open output file'). 0 = no retry. CI default 2.
        #[arg(long, default_value_t = 0)]
        retries: u32,
        /// Plan 27 Ф.4: GC backend. boehm = Boehm GC (default). malloc = plain malloc (bench only).
        #[arg(long, value_parser = ["boehm", "malloc"], default_value = "boehm")]
        gc: String,
        /// Plan 140 Ф.2 (D24 amend): contract build-policy. `enforce`
        /// (default) — недоказанные контракты проверяются (debug И release);
        /// `off` — все контракт-проверки элидируются глобально (legacy).
        #[arg(long = "contracts", value_parser = ["enforce", "off"], default_value = "enforce")]
        contracts: String,
    },
}

/// Запустить lint-проходы и вывести warning'и в stderr.
/// Lint-сообщения уже содержат «warning:» префикс — render через
/// прямой формат (без render() который вставляет «error:»).
fn run_lints(module: &nova_codegen::ast::Module, src: &str, file: &str) {
    let warnings = nova_codegen::lints::lint_module(module);
    for w in warnings {
        let (line, col) = nova_codegen::diag::byte_to_line_col(src, w.diag.span.start);
        eprintln!("{}:{}:{}: {} [{}]", file, line, col, w.diag.message, w.rule);
    }
}

/// D78 path/module enforcement. Если файл лежит внутри пакета
/// (нашли nova.toml в parent dirs), проверяем что declared module
/// соответствует file path относительно source root.
/// Если nova.toml не найден — skip (файл не часть пакета).
///
/// Bug fix 2026-06-01: emit W_D78_REV1_DEPRECATED warning для rev-1
/// legacy forms (вместо silent acceptance).
fn check_module_path(file: &PathBuf, module: &nova_codegen::ast::Module) -> Result<()> {
    use nova_codegen::manifest::ModulePathCheck;
    match nova_codegen::manifest::check_module_path(file.as_path(), &module.name) {
        Ok(ModulePathCheck::Rev3) => Ok(()),
        Ok(ModulePathCheck::Rev1Deprecated(msg)) => {
            eprintln!("warning: {}", msg);
            Ok(())
        }
        Err(msg) => Err(anyhow!("{}", msg)),
    }
}

fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Check { file, explain_cache } => cmd_check(&file, explain_cache),
        Cmd::Run { file } => cmd_run(&file),
        Cmd::TestInterp { file } => cmd_test(&file),
        Cmd::Compile { file, output, no_annotate_source, no_lint, contracts } =>
            cmd_compile(&file, output.as_deref(), !no_annotate_source, !no_lint, &contracts),
        Cmd::EmitRuntimeStubs { root, check } =>
            cmd_emit_runtime_stubs(&root, check),
        Cmd::DumpRuntime => cmd_dump_runtime(),
        Cmd::Unicode { ucd_dir, root, unicode_version, emit_conformance, conformance_limit, conformance_full, check } =>
            cmd_unicode(&ucd_dir, &root, &unicode_version, emit_conformance, conformance_limit, conformance_full, check),
        Cmd::TestBuild { file, mode, toolchain, vcvars, clang, cg_include, rt_dir, tmp_dir, display, keep_artifacts, timeout, gc, contracts } =>
            cmd_test_build(&file, &mode, &toolchain, vcvars.as_deref(), clang.as_deref(), cg_include.as_deref(), rt_dir.as_deref(), tmp_dir.as_deref(), display.as_deref(), keep_artifacts, timeout, &gc, &contracts),
        Cmd::TestAll { tests_dir, stdlib_dir, include_stdlib, include_slow, slow_only, filter, mode, toolchain, vcvars, clang, cg_include, rt_dir, tmp_dir, keep_artifacts, timeout, jobs, format, verbose, quiet, results_file, rerun_failed, retries, gc, contracts } =>
            cmd_test_all(&tests_dir, &stdlib_dir, include_stdlib, include_slow, slow_only, filter.as_deref(), &mode, &toolchain, vcvars.as_deref(), clang.as_deref(), cg_include.as_deref(), rt_dir.as_deref(), tmp_dir.as_deref(), keep_artifacts, timeout, jobs, &format, verbose, quiet, results_file.as_deref(), rerun_failed, retries, &gc, &contracts),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

/// Точка входа: запускаем всё в потоке с увеличенным стеком (32 MiB).
///
/// AST-обходы (type-checker, SCC-based purity inference, SMT encoder)
/// взаимно рекурсивны через expr ↔ block ↔ stmt. На Windows стек
/// главного потока по умолчанию 1 MiB — для глубоко вложенных выражений
/// и больших модулей этого недостаточно. Spawn с explicit stack_size
/// вместо linker flags: portably работает на Windows/Linux/macOS.
fn main() -> ExitCode {
    std::thread::Builder::new()
        .name("nova-main".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .expect("spawn main thread")
        .join()
        .unwrap_or(ExitCode::FAILURE)
}

fn read_file(path: &PathBuf) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| anyhow!("failed to read {}: {}", path.display(), e))
}

fn cmd_check(path: &PathBuf, explain_cache: bool) -> Result<()> {
    let src = read_file(path)?;
    let mut module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!(
            "{}",
            d.render(&src, &path.to_string_lossy())
        )
    })?;
    check_module_path(path, &module)?;
    // Plan 162.2 Ф.2: collect cross-module signatures before type-check so
    // that is_known_type / is_known_fn can suppress false-positive diagnostics
    // for symbols from transitively imported modules.
    {
        let sig_table = nova_codegen::test_runner::find_repo_root_from(path)
            .map(|repo| {
                let stdlib_dir = repo.join("std");
                nova_codegen::imports::collect_all_signatures(path, &module, &repo, &stdlib_dir)
                    .unwrap_or_else(|_| nova_codegen::imports::ModuleSigTable::new())
            });
        match sig_table {
            Some(st) => nova_codegen::types::check_module_with_sig_table(&module, st),
            None => nova_codegen::types::check_module(&module),
        }
    }
    .map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;
    // Plan 114.4.2 (D199) Ф.3 + 114.4.3 (V2): const fn AST rewrite + eval
    // также должен запускаться в check-режиме чтобы evaluator errors
    // (E_CONST_FN_EVAL_OVERFLOW / DIV_ZERO / DEPTH_EXCEEDED) fired.
    let cfn_errs = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    if !cfn_errs.is_empty() {
        let messages: Vec<String> = cfn_errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        return Err(anyhow!("{}", messages.join("\n")));
    }
    // Plan 114.4.4.5 V4.1: monomorphize mixed const fns (per-const-arg
    // specialization). Runs AFTER rewriter (fully-const fns dropped).
    let mono_errs = nova_codegen::const_fn_mono::specialize_mixed_const_fns(&mut module);
    if !mono_errs.is_empty() {
        let messages: Vec<String> = mono_errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        return Err(anyhow!("{}", messages.join("\n")));
    }

    // Plan 123.5 (D217 §6 amend): emit field-cache analysis report.
    if explain_cache {
        nova_codegen::types::annotate_map_literals(&mut module);
        nova_codegen::desugar::desugar_module(&mut module);
        nova_codegen::types::infer_effects(&mut module);
        nova_codegen::callnorm::normalize_module(&mut module);
    nova_codegen::chain_norm::normalize_chains_module(&mut module);
        let cfg = nova_codegen::field_cache::FieldCacheConfig::from_env_or_default();
        let report = nova_codegen::field_cache::analyze_module(&module, &cfg);
        emit_explain_report(&report);
    } else {
        println!("ok: {} parsed and checked", path.display());
    }
    Ok(())
}

/// Plan 123.5: human-readable per-fn cache decision report.
fn emit_explain_report(report: &nova_codegen::field_cache::ExplainReport) {
    if report.per_fn.is_empty() {
        println!("field-cache: no methods triggered caching under current config");
        return;
    }
    let total_caches: usize = report.per_fn.iter().map(|f| f.total()).sum();
    println!("field-cache report: {} method(s) affected, {} total cache(s) inserted",
        report.per_fn.len(), total_caches);
    println!();
    for info in &report.per_fn {
        println!("fn {} @{} — {} cache(s):",
            info.type_name, info.fn_name, info.total());
        if !info.ro_caches.is_empty() {
            println!("  D217 ro/mut field cache: {}", info.ro_caches.join(", "));
        }
        if !info.mut_caches.is_empty() {
            println!("  D217 mut field first-region cache: {}", info.mut_caches.join(", "));
        }
        if !info.licm_hoists.is_empty() {
            println!("  D218 LICM loop hoist: {}", info.licm_hoists.join(", "));
        }
        if !info.pure_caches.is_empty() {
            println!("  D219 pure call cache: {}", info.pure_caches.join(", "));
        }
        if !info.chain_caches.is_empty() {
            let paths: Vec<String> = info.chain_caches.iter()
                .map(|p| format!("@{}", p.join(".")))
                .collect();
            println!("  D217 V4 chain cache: {}", paths.join(", "));
        }
    }
}

/// Q-interpreter-future / D274: the tree-walking interpreter is UNSUPPORTED.
/// The user-facing `nova run` was stubbed (Plan 157); this internal dev entry
/// point follows. The `interp/` module is kept for reference but is no longer
/// wired into a runnable command — use C codegen instead.
fn cmd_run(_path: &PathBuf) -> Result<()> {
    Err(anyhow!(
        "the tree-walking interpreter is currently NOT supported.\n\
         Use C codegen instead: `nova-codegen compile <file>`,\n\
         or the `nova` CLI: `nova build` / `nova test`."
    ))
}

fn cmd_compile(path: &PathBuf, output: Option<&std::path::Path>, annotate_source: bool, lint: bool, contracts: &str) -> Result<()> {
    let src = read_file(path)?;
    let mut module = nova_codegen::parser::parse(&src).map_err(|d| {
        anyhow!("{}", d.render(&src, &path.to_string_lossy()))
    })?;
    check_module_path(path, &module)?;
    // Plan 162.2 Ф.2: collect cross-module signatures before type-check so
    // that is_known_type / is_known_fn can suppress false-positive diagnostics
    // for symbols from transitively imported modules.
    let module_env = {
        let sig_table = nova_codegen::test_runner::find_repo_root_from(path)
            .map(|repo| {
                let stdlib_dir = repo.join("std");
                nova_codegen::imports::collect_all_signatures(path, &module, &repo, &stdlib_dir)
                    .unwrap_or_else(|_| nova_codegen::imports::ModuleSigTable::new())
            });
        match sig_table {
            Some(st) => nova_codegen::types::check_module_with_sig_table(&module, st),
            None => nova_codegen::types::check_module(&module),
        }
    }
    .map_err(|errs| {
        let messages: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        anyhow!("{}", messages.join("\n"))
    })?;
    // Plan 52 Ф.4: десугаринг map-литералов `[k: v]` → block-expression
    // ПОСЛЕ type-check, ДО effect-inference и codegen. После прохода
    // codegen видит обычные method-call'ы (with_capacity / insert).
    // Plan 52 Ф.7: аннотируем MapLit-узлы inferred K/V — для генерации
    // turbofish `HashMap[K,V].with_capacity(n)` в десугаринге.
    // Plan 114.4.2 (D199) Ф.3: rewrite const fn calls to literals в AST
    // и удалить const fn declarations (codegen drop). После check_module
    // (V1 subset enforced), до десугаринга / annotation passes — чтобы
    // dependent expressions видели уже-литералы.
    let cfn_errs = nova_codegen::const_fn_eval::rewrite_const_fn_calls(&mut module);
    if !cfn_errs.is_empty() {
        let messages: Vec<String> = cfn_errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        return Err(anyhow!("{}", messages.join("\n")));
    }
    // Plan 114.4.4.5 V4.1: monomorphize mixed const fns (per-const-arg
    // specialization). Runs AFTER rewriter (fully-const fns dropped).
    let mono_errs = nova_codegen::const_fn_mono::specialize_mixed_const_fns(&mut module);
    if !mono_errs.is_empty() {
        let messages: Vec<String> = mono_errs
            .iter()
            .map(|d| d.render(&src, &path.to_string_lossy()))
            .collect();
        return Err(anyhow!("{}", messages.join("\n")));
    }
    // Plan 126.2 Ф.2: inject synthesized built-in protocol methods (Equatable/
    // Hashable/Cloneable/Comparable/Printable) into module.items so codegen
    // emits C bodies + operator dispatch resolves them. After check_module,
    // before desugar/codegen. User-explicit methods always win.
    nova_codegen::protocols::auto_derive::inject_synthesized_methods(&mut module);
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    // D28: effect inference для private fn — добавить `Fail` если throw
    // в теле и нет явного Fail в effect-row.
    nova_codegen::types::infer_effects(&mut module);
    if lint {
        run_lints(&module, &src, &path.to_string_lossy());
    }
    // Plan 123.1 (D217): method-local receiver field caching. Pass —
    // pure AST→AST трансформация, semantic equivalence guaranteed.
    {
        let cfg = nova_codegen::field_cache::FieldCacheConfig::from_env_or_default();
        nova_codegen::field_cache::cache_module(&mut module, &cfg);
    }

    let mut emitter = nova_codegen::codegen::CEmitter::new();
    // Plan 14 std-fix: source передаётся всегда — для line:col в
    // codegen-ошибках. Активация SRC-комментариев — отдельный флаг.
    emitter.set_source_for_annotations(src.clone());
    // Plan 140.1 Ф.2 (D24/D13 amend): feed the source file name for the
    // location-first contract/assert diagnostic prefix (`<file>:<line>: ...`).
    // Use the file name (basename) to keep the prefix short and clickable.
    {
        let fname = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        emitter.set_source_file_name(fname);
    }
    if !annotate_source {
        emitter.disable_source_annotations();
    }
    // Plan 33.3 Ф.9.9: передаём proven контракты в codegen для
    // selective stripping (true zero-cost даже в debug).
    emitter.set_proven_contracts(&module_env.proven_contracts);
    // Plan 140.2 Part B (D257 / B.4): proven index-сайты для элизии bounds-check.
    emitter.set_proven_index_sites(&module_env.proven_index_sites);
    emitter.set_proven_index_sites_contract(&module_env.proven_index_sites_contract);
    // Plan 140.4 ([M-opt-elide-proven-overflow-checks]): proven `int`-overflow сайты
    // для элизии `nova_int_checked_*`.
    emitter.set_proven_overflow_sites(
        &module_env.proven_overflow_sites,
        &module_env.proven_overflow_sites_contract,
    );
    // Plan 140 Ф.2 (D24 amend): build-policy `--contracts=off` элидирует ВСЕ
    // контракт-проверки глобально (legacy zero-cost). Default `enforce` —
    // недоказанные проверяются (debug И release; Z3-proven уже элидированы).
    emitter.set_contracts_off(contracts == "off");
    let (c_code, warnings) = emitter
        .emit_module(&module)
        .map_err(|e| anyhow!("codegen error: {}", e))?;
    for w in &warnings {
        eprintln!("{}", w);
    }

    let out_path = match output {
        Some(p) => p.to_path_buf(),
        None => path.with_extension("c"),
    };
    std::fs::write(&out_path, &c_code)
        .map_err(|e| anyhow!("failed to write {}: {}", out_path.display(), e))?;
    eprintln!("ok: {} -> {}", path.display(), out_path.display());
    Ok(())
}

/// Q-interpreter-future / D274: running tests through the tree-walking
/// interpreter is UNSUPPORTED. Tests run via C codegen (`nova test`). The
/// `interp/` module is kept for reference but is no longer wired here.
fn cmd_test(_path: &PathBuf) -> Result<()> {
    Err(anyhow!(
        "the tree-walking interpreter is currently NOT supported.\n\
         Run tests via C codegen instead: use the `nova` CLI `nova test`,\n\
         or compile with `nova-codegen compile <file>`."
    ))
}

/// Plan 13 Ф.3: emit-runtime-stubs.
/// Generates `std/runtime/string.nv` + `std/runtime/math.nv` из
/// `runtime_registry.rs`. С `--check` сравнивает с существующими и
/// fail'ит при diff'е.
fn cmd_emit_runtime_stubs(root: &PathBuf, check: bool) -> Result<()> {
    use nova_codegen::codegen::runtime_registry;
    let registry = runtime_registry::all();
    let groups = runtime_registry::group_by_module(&registry);
    let mut total_files = 0;
    let mut diffed_files: Vec<String> = Vec::new();
    for (module, fns) in &groups {
        let rel_path = runtime_registry::module_to_path(module);
        let abs_path = root.join(&rel_path);
        let content = runtime_registry::render_nv(module, fns);
        if check {
            let existing = std::fs::read_to_string(&abs_path)
                .map_err(|e| anyhow!("failed to read {}: {}", abs_path.display(), e))?;
            // Normalize line endings (Windows CRLF vs LF).
            let norm = |s: &str| s.replace("\r\n", "\n");
            if norm(&existing) != norm(&content) {
                diffed_files.push(rel_path);
            }
        } else {
            // Ensure parent dir exists.
            if let Some(parent) = abs_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("failed to create {}: {}", parent.display(), e))?;
            }
            std::fs::write(&abs_path, &content)
                .map_err(|e| anyhow!("failed to write {}: {}", abs_path.display(), e))?;
            println!("wrote {}", rel_path);
        }
        total_files += 1;
    }
    if check {
        if !diffed_files.is_empty() {
            return Err(anyhow!(
                "auto-generated files diverge from registry:\n  {}\n\
                 Run `nova-codegen emit-runtime-stubs` to regenerate.",
                diffed_files.join("\n  ")
            ));
        }
        println!("OK: {} runtime stub file(s) match registry.", total_files);
    } else {
        println!("emitted {} runtime stub file(s).", total_files);
    }
    Ok(())
}

/// Plan 156: derive the slow-lane sibling path for a conformance fixture, i.e.
/// `<dir>/<kind>_conformance.nv` -> `<dir>/<kind>_conformance_slow.nv`.
fn slow_conformance_path(fast: &Path) -> PathBuf {
    let stem = fast
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = fast
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "nv".to_string());
    let slow_name = format!("{}_slow.{}", stem, ext);
    match fast.parent() {
        Some(dir) => dir.join(slow_name),
        None => PathBuf::from(slow_name),
    }
}

/// Plan 152.4.1 (Q-unicode-data): generate `std/unicode/norm_data.nv` from the
/// UCD. `--check` compares against the existing file and fails on diff (CI
/// guard), mirroring `cmd_emit_runtime_stubs`.
#[allow(clippy::too_many_arguments)]
fn cmd_unicode(
    ucd_dir: &Path,
    root: &Path,
    version: &str,
    emit_conformance: bool,
    conformance_limit: usize,
    conformance_full: bool,
    check: bool,
) -> Result<()> {
    use nova_codegen::codegen::unicode_data;
    let tables = unicode_data::parse_ucd(ucd_dir)?;
    let content = unicode_data::render_norm_data_nv(&tables, version);
    let rel = "std/unicode/norm_data.nv";
    let abs = root.join(rel);
    let stats = format!(
        "{} nfd / {} nfkd / {} ccc / {} comp",
        tables.nfd.len(),
        tables.nfkd.len(),
        tables.ccc.len(),
        tables.comp.len()
    );
    // Plan 152.4.3: grapheme-break tables (UAX #29).
    let gtables = unicode_data::parse_grapheme_tables(ucd_dir)?;
    let gcontent = unicode_data::render_grapheme_data_nv(&gtables, version);
    let grel = "std/unicode/grapheme_data.nv";
    let gabs = root.join(grel);
    let gstats = format!(
        "{} gcb / {} extpict / {} incb ranges",
        gtables.gcb.len(),
        gtables.ext_pict.len(),
        gtables.incb.len()
    );
    // Plan 152.4.4: case folding + Unicode case mapping (UAX, SpecialCasing).
    let ctables = unicode_data::parse_case_tables(ucd_dir)?;
    let ccontent = unicode_data::render_case_data_nv(&ctables, version);
    let crel = "std/unicode/case_data.nv";
    let cabs = root.join(crel);
    let cstats = format!(
        "{} fold / {} lower / {} upper / {} title / {} cased / {} case-ign ranges",
        ctables.fold.len(),
        ctables.lower.len(),
        ctables.upper.len(),
        ctables.title.len(),
        ctables.cased.len(),
        ctables.case_ignorable.len()
    );
    // Plan 152.4.5: word-boundary tables (UAX #29).
    let wtables = unicode_data::parse_word_tables(ucd_dir)?;
    let wcontent = unicode_data::render_word_data_nv(&wtables, version);
    let wrel = "std/unicode/word_data.nv";
    let wabs = root.join(wrel);
    let wstats = format!("{} wb ranges", wtables.len());
    // Plan 152.4.6: sentence-boundary tables (UAX #29).
    let stables = unicode_data::parse_sentence_tables(ucd_dir)?;
    let scontent = unicode_data::render_sentence_data_nv(&stables, version);
    let srel = "std/unicode/sentence_data.nv";
    let sabs = root.join(srel);
    let sstats = format!("{} sb ranges", stables.len());
    // Plan 152.3b: General_Category + Alphabetic/White_Space tables (UCD).
    let cattables = unicode_data::parse_category_tables(ucd_dir)?;
    let catcontent = unicode_data::render_category_data_nv(&cattables, version);
    let catrel = "std/unicode/category_data.nv";
    let catabs = root.join(catrel);
    let catstats = format!(
        "{} gc / {} alpha / {} white-space ranges",
        cattables.gc.len(),
        cattables.alpha.len(),
        cattables.white_space.len()
    );
    // Plan 152.5b: collation (UCA / DUCET, UTS #10). Needs allkeys.txt (UCA).
    // Skipped gracefully if allkeys.txt is absent so the 152.4 UCD-only flow
    // still works in dirs without the UCA data.
    let coll_data: Option<(String, String, std::path::PathBuf, &str)> =
        if ucd_dir.join("allkeys.txt").exists() {
            let coll = unicode_data::parse_collation_tables(ucd_dir)?;
            let content = unicode_data::render_collate_data_nv(&coll, version);
            let rel = "std/unicode/collate_data.nv";
            let stats = format!(
                "{} single / {} contraction-keys / {} implicit ranges",
                coll.single.len(),
                coll.contractions.len(),
                coll.implicit.len()
            );
            Some((content, stats, root.join(rel), rel))
        } else {
            None
        };
    // Optional conformance fixtures: UAX #15 (normalization) + UAX #29 (graphemes)
    // + case-mapping breadth (Plan 152.4.4) + UTS #10 collation (Plan 152.5b).
    let mut confs: Vec<(String, std::path::PathBuf)> = if emit_conformance {
        vec![
            (
                unicode_data::render_conformance_nv(ucd_dir, conformance_limit)?,
                root.join("nova_tests/plan152_4/normalization_conformance.nv"),
            ),
            (
                unicode_data::render_grapheme_conformance_nv(ucd_dir, conformance_limit)?,
                root.join("nova_tests/plan152_4/grapheme_conformance.nv"),
            ),
            (
                unicode_data::render_case_conformance_nv(ucd_dir, conformance_limit)?,
                root.join("nova_tests/plan152_4/case_conformance.nv"),
            ),
            (
                unicode_data::render_word_conformance_nv(ucd_dir, conformance_limit)?,
                root.join("nova_tests/plan152_4/word_conformance.nv"),
            ),
            (
                unicode_data::render_sentence_conformance_nv(ucd_dir, conformance_limit)?,
                root.join("nova_tests/plan152_4/sentence_conformance.nv"),
            ),
        ]
    } else {
        Vec::new()
    };
    // Plan 152.5b: collation conformance (UTS #10) — only when the UCA test data
    // is present (CollationTest_SHIFTED.txt) and conformance is requested.
    if emit_conformance && coll_data.is_some() {
        if let Ok(c) = unicode_data::render_collation_conformance_nv(ucd_dir, conformance_limit) {
            confs.push((c, root.join("nova_tests/plan152_5/collation_conformance.nv")));
        }
    }
    // Plan 156: ALSO emit the FULL (uncapped) corpus as `<kind>_conformance_slow.nv`
    // slow-lane files. Re-render each conformance kind with limit = usize::MAX, then
    // rewrite the module declaration line (`...conformance` -> `...conformance_slow`)
    // and the destination path (`_conformance.nv` -> `_conformance_slow.nv`). This
    // reuses the exact same renderers (no renderer surgery / duplication).
    if emit_conformance && conformance_full {
        // (rendered-full-string, fast-path) for each kind being emitted.
        let mut full: Vec<(String, std::path::PathBuf)> = vec![
            (
                unicode_data::render_conformance_nv(ucd_dir, usize::MAX)?,
                root.join("nova_tests/plan152_4/normalization_conformance.nv"),
            ),
            (
                unicode_data::render_grapheme_conformance_nv(ucd_dir, usize::MAX)?,
                root.join("nova_tests/plan152_4/grapheme_conformance.nv"),
            ),
            (
                unicode_data::render_case_conformance_nv(ucd_dir, usize::MAX)?,
                root.join("nova_tests/plan152_4/case_conformance.nv"),
            ),
            (
                unicode_data::render_word_conformance_nv(ucd_dir, usize::MAX)?,
                root.join("nova_tests/plan152_4/word_conformance.nv"),
            ),
            (
                unicode_data::render_sentence_conformance_nv(ucd_dir, usize::MAX)?,
                root.join("nova_tests/plan152_4/sentence_conformance.nv"),
            ),
        ];
        if coll_data.is_some() {
            if let Ok(c) = unicode_data::render_collation_conformance_nv(ucd_dir, usize::MAX) {
                full.push((c, root.join("nova_tests/plan152_5/collation_conformance.nv")));
            }
        }
        for (content_full, fast_path) in full {
            // Rewrite the single `module ...conformance` declaration line to append
            // `_slow`. The substring `_conformance\n` appears exactly once in the
            // generated output (the module line), so a 1-shot replacen is precise.
            let slow_content = content_full.replacen("_conformance\n", "_conformance_slow\n", 1);
            // Derive the sibling `_conformance_slow.nv` path from the fast path.
            let slow_path = slow_conformance_path(&fast_path);
            confs.push((slow_content, slow_path));
        }
    }
    if check {
        let norm = |s: &str| s.replace("\r\n", "\n");
        let existing = std::fs::read_to_string(&abs)
            .map_err(|e| anyhow!("failed to read {}: {}", abs.display(), e))?;
        if norm(&existing) != norm(&content) {
            return Err(anyhow!(
                "{} diverges from UCD ({}).\n\
                 Run `nova-codegen unicode --ucd-dir <UCD-dir>` to regenerate.",
                rel, stats
            ));
        }
        {
            let ex = std::fs::read_to_string(&gabs)
                .map_err(|e| anyhow!("failed to read {}: {}", gabs.display(), e))?;
            if norm(&ex) != norm(&gcontent) {
                return Err(anyhow!(
                    "{} diverges from UCD ({}).\n\
                     Run `nova-codegen unicode --ucd-dir <UCD-dir>` to regenerate.",
                    grel, gstats
                ));
            }
        }
        {
            let ex = std::fs::read_to_string(&cabs)
                .map_err(|e| anyhow!("failed to read {}: {}", cabs.display(), e))?;
            if norm(&ex) != norm(&ccontent) {
                return Err(anyhow!(
                    "{} diverges from UCD ({}).\n\
                     Run `nova-codegen unicode --ucd-dir <UCD-dir>` to regenerate.",
                    crel, cstats
                ));
            }
        }
        {
            let ex = std::fs::read_to_string(&wabs)
                .map_err(|e| anyhow!("failed to read {}: {}", wabs.display(), e))?;
            if norm(&ex) != norm(&wcontent) {
                return Err(anyhow!(
                    "{} diverges from UCD ({}).\n\
                     Run `nova-codegen unicode --ucd-dir <UCD-dir>` to regenerate.",
                    wrel, wstats
                ));
            }
        }
        {
            let ex = std::fs::read_to_string(&sabs)
                .map_err(|e| anyhow!("failed to read {}: {}", sabs.display(), e))?;
            if norm(&ex) != norm(&scontent) {
                return Err(anyhow!(
                    "{} diverges from UCD ({}).\n\
                     Run `nova-codegen unicode --ucd-dir <UCD-dir>` to regenerate.",
                    srel, sstats
                ));
            }
        }
        {
            let ex = std::fs::read_to_string(&catabs)
                .map_err(|e| anyhow!("failed to read {}: {}", catabs.display(), e))?;
            if norm(&ex) != norm(&catcontent) {
                return Err(anyhow!(
                    "{} diverges from UCD ({}).\n\
                     Run `nova-codegen unicode --ucd-dir <UCD-dir>` to regenerate.",
                    catrel, catstats
                ));
            }
        }
        // Plan 152.5b: collation table (only if the UCA data was present).
        if let Some((cl_content, cl_stats, cl_abs, cl_rel)) = &coll_data {
            let ex = std::fs::read_to_string(cl_abs)
                .map_err(|e| anyhow!("failed to read {}: {}", cl_abs.display(), e))?;
            if norm(&ex) != norm(cl_content) {
                return Err(anyhow!(
                    "{} diverges from UCA DUCET ({}).\n\
                     Run `nova-codegen unicode --ucd-dir <UCA+UCD-dir>` to regenerate.",
                    cl_rel, cl_stats
                ));
            }
        }
        for (c, p) in &confs {
            let ex = std::fs::read_to_string(p)
                .map_err(|e| anyhow!("failed to read {}: {}", p.display(), e))?;
            if norm(&ex) != norm(c) {
                return Err(anyhow!("{} diverges from UCD test data; regenerate.", p.display()));
            }
        }
        print!("OK: {} ({}) + {} ({}) + {} ({}) + {} ({}) + {} ({}) + {} ({})", rel, stats, grel, gstats, crel, cstats, wrel, wstats, srel, sstats, catrel, catstats);
        if let Some((_, cl_stats, _, cl_rel)) = &coll_data {
            print!(" + {} ({})", cl_rel, cl_stats);
        }
        println!(" match UCD.");
    } else {
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow!("failed to create {}: {}", parent.display(), e))?;
        }
        std::fs::write(&abs, &content)
            .map_err(|e| anyhow!("failed to write {}: {}", abs.display(), e))?;
        println!("wrote {} ({}).", rel, stats);
        std::fs::write(&gabs, &gcontent)
            .map_err(|e| anyhow!("failed to write {}: {}", gabs.display(), e))?;
        println!("wrote {} ({}).", grel, gstats);
        std::fs::write(&cabs, &ccontent)
            .map_err(|e| anyhow!("failed to write {}: {}", cabs.display(), e))?;
        println!("wrote {} ({}).", crel, cstats);
        std::fs::write(&wabs, &wcontent)
            .map_err(|e| anyhow!("failed to write {}: {}", wabs.display(), e))?;
        println!("wrote {} ({}).", wrel, wstats);
        std::fs::write(&sabs, &scontent)
            .map_err(|e| anyhow!("failed to write {}: {}", sabs.display(), e))?;
        println!("wrote {} ({}).", srel, sstats);
        std::fs::write(&catabs, &catcontent)
            .map_err(|e| anyhow!("failed to write {}: {}", catabs.display(), e))?;
        println!("wrote {} ({}).", catrel, catstats);
        // Plan 152.5b: collation table (only if the UCA data was present).
        if let Some((cl_content, cl_stats, cl_abs, cl_rel)) = &coll_data {
            if let Some(parent) = cl_abs.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("failed to create {}: {}", parent.display(), e))?;
            }
            std::fs::write(cl_abs, cl_content)
                .map_err(|e| anyhow!("failed to write {}: {}", cl_abs.display(), e))?;
            println!("wrote {} ({}).", cl_rel, cl_stats);
        }
        for (c, p) in &confs {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("failed to create {}: {}", parent.display(), e))?;
            }
            std::fs::write(p, c)
                .map_err(|e| anyhow!("failed to write {}: {}", p.display(), e))?;
            println!("wrote {}.", p.display());
        }
    }
    Ok(())
}

/// Plan 13: dump runtime registry для sanity-check'а.
fn cmd_dump_runtime() -> Result<()> {
    use nova_codegen::codegen::runtime_registry;
    let registry = runtime_registry::all();
    let groups = runtime_registry::group_by_module(&registry);
    println!("Nova runtime registry: {} function(s) total.", registry.len());
    for (module, fns) in &groups {
        println!("\n=== {} ({} fns) ===", module, fns.len());
        for f in fns {
            let recv = match f.receiver {
                Some(r) => format!("{} ", r),
                None => String::new(),
            };
            let dot = if f.is_static { "." } else { "@" };
            let mu = if f.is_mut { "mut " } else { "" };
            let params: Vec<String> = f.params.iter()
                .map(|(n, ty)| format!("{} {}", n, ty))
                .collect();
            println!(
                "  {}{}{}{}({}) -> {}    [c: {}]",
                recv, mu, dot, f.name, params.join(", "), f.return_ty, f.c_name
            );
        }
    }
    Ok(())
}

// ---------- Plan 24: cross-platform test runner ----------

use nova_codegen::test_runner;

fn default_repo_root() -> PathBuf {
    // Если запущен из корня репо (cwd = nova-lang) — `.`. Если из любого
    // другого места — пользователь передаст явные --tests-dir / --cg-include.
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn default_tmp_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(temp) = std::env::var_os("TEMP") {
            return PathBuf::from(temp).join("nova_tests");
        }
    }
    if let Some(tmpdir) = std::env::var_os("TMPDIR") {
        return PathBuf::from(tmpdir).join("nova_tests");
    }
    PathBuf::from("/tmp/nova_tests")
}

#[allow(clippy::too_many_arguments)]
fn cmd_test_build(
    file: &PathBuf,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    cg_include: Option<&Path>,
    rt_dir: Option<&Path>,
    tmp_dir: Option<&Path>,
    display: Option<&str>,
    keep_artifacts: bool,
    timeout_secs: u64,
    gc: &str,
    contracts: &str,
) -> Result<()> {
    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let repo_root = default_repo_root();
    let cg_include_buf = cg_include
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen"));
    let rt_dir_buf = rt_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen").join("nova_rt"));
    let tmp_dir_buf = tmp_dir.map(Path::to_path_buf).unwrap_or_else(default_tmp_dir);
    std::fs::create_dir_all(&tmp_dir_buf)
        .map_err(|e| anyhow!("create tmp_dir {}: {}", tmp_dir_buf.display(), e))?;

    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    let display_owned = match display {
        Some(d) => d.to_string(),
        None => file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<unknown>".to_string()),
    };
    let libuv = test_runner::detect_or_build_libuv(&rt_dir_buf, &repo_root, vcvars);
    let gc_kind = test_runner::GcKind::parse(gc)?;
    let opts = test_runner::TestBuildOpts {
        nv_file: file,
        toolchain: &tc,
        mode,
        cg_include: &cg_include_buf,
        rt_dir: &rt_dir_buf,
        tmp_dir: &tmp_dir_buf,
        display: &display_owned,
        keep_artifacts,
        libuv: libuv.as_ref(),
        timeout: std::time::Duration::from_secs(timeout_secs),
        gc_kind,
        verbosity: test_runner::Verbosity::Normal,
        mono_depth: None,
        // Plan 83.1 Ф.5: single-file run — один процесс, нет
        // oversubscription, бюджет не нужен.
        maxprocs_budget: None,
        // Plan 140 Ф.2 (D24 amend): `--contracts=off` → элидировать все
        // контракт-проверки на codegen (legacy zero-cost). Default enforce.
        contracts_off: contracts == "off",
    };
    let status = test_runner::run_one(&opts);
    let label = status.label();
    let detail = status.detail();
    if detail.is_empty() {
        println!("{:<14} {}", label, display_owned);
    } else {
        println!("{:<14} {}  # {}", label, display_owned, detail);
    }
    if status.is_pass() {
        Ok(())
    } else {
        Err(anyhow!("test failed: {}", display_owned))
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_test_all(
    tests_dir: &PathBuf,
    stdlib_dir: &PathBuf,
    include_stdlib: bool,
    include_slow: bool,
    slow_only: bool,
    filter: Option<&str>,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    cg_include: Option<&Path>,
    rt_dir: Option<&Path>,
    tmp_dir: Option<&Path>,
    keep_artifacts: bool,
    timeout_secs: u64,
    jobs: usize,
    format: &str,
    verbose: bool,
    quiet: bool,
    results_file: Option<&Path>,
    rerun_failed: bool,
    retries: u32,
    gc: &str,
    contracts: &str,
) -> Result<()> {
    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let format = test_runner::OutputFormat::parse(format)?;
    let verbosity = if quiet {
        test_runner::Verbosity::Quiet
    } else if verbose {
        test_runner::Verbosity::Verbose
    } else {
        test_runner::Verbosity::Normal
    };
    let jobs = if jobs == 0 { test_runner::default_jobs() } else { jobs };
    let repo_root = default_repo_root();
    let cg_include_buf = cg_include
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen"));
    let rt_dir_buf = rt_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo_root.join("compiler-codegen").join("nova_rt"));
    let tmp_dir_buf = tmp_dir.map(Path::to_path_buf).unwrap_or_else(default_tmp_dir);

    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    // Plan 26 Ф.8: information messages в stderr (как у cargo); per-test
    // events и summary — в stdout. Wrappers смогут просто прогонять stdout.
    if format == test_runner::OutputFormat::Text {
        eprintln!(
            "Toolchain: {}, mode={:?}, jobs={}, tests-dir={}",
            tc.name(),
            mode,
            jobs,
            tests_dir.display()
        );
    }

    let libuv = test_runner::detect_or_build_libuv(&rt_dir_buf, &repo_root, vcvars);
    if format == test_runner::OutputFormat::Text {
        if libuv.is_some() {
            eprintln!("libuv: enabled");
        } else {
            eprintln!("libuv: disabled (Time.sleep через busy-yield fallback)");
        }
    }

    let stdlib_dir_opt = if include_stdlib {
        Some(stdlib_dir.as_path())
    } else {
        None
    };
    let gc_kind = test_runner::GcKind::parse(gc)?;
    let slow_lane = if slow_only {
        test_runner::SlowLane::Only
    } else if include_slow {
        test_runner::SlowLane::Include
    } else {
        test_runner::SlowLane::Exclude
    };
    let opts = test_runner::TestAllOpts {
        tests_dir,
        stdlib_dir: stdlib_dir_opt,
        include_stdlib,
        filter,
        mode,
        toolchain: tc,
        cg_include: &cg_include_buf,
        rt_dir: &rt_dir_buf,
        tmp_dir: &tmp_dir_buf,
        keep_artifacts,
        libuv,
        timeout: std::time::Duration::from_secs(timeout_secs),
        jobs,
        format,
        verbosity,
        cache_dir: None, // Ф.5 — не реализовано, оставлен крючок в opts.
        results_file,
        rerun_failed,
        retries,
        gc_kind,
        list_only: false,
        filter_from: None,
        shuffle_seed: None,
        skip: &[],
        mono_depth: None,
        // Plan 140 Ф.2 (D24 amend): `--contracts=off` → элидировать все
        // контракт-проверки на codegen для всех тестов прогона (legacy).
        contracts_off: contracts == "off",
        // Plan 156: slow-lane selection (--include-slow / --slow-only).
        slow_lane,
    };
    let summary = test_runner::run_all(opts)?;
    test_runner::print_summary(&summary, format);

    if summary.fail > 0 {
        Err(anyhow!("{} test(s) failed", summary.fail))
    } else {
        Ok(())
    }
}
