//! Plan 28: `nova` CLI — единая точка входа для пользователя.
//!
//! `nova test`, `nova build`, `nova run`, `nova check`, `nova regen-runtime`.
//! Заменяет run_tests.ps1 / run_tests.sh / regen_runtime.ps1.
//!
//! `nova-codegen` CLI сохраняется нетронутым — используется IDE, CI, отладкой.

use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use nova_codegen::test_runner;

mod bench;
mod build_cache;

// ---------- Plan 36 R7: structured CLI error types ----------

/// Plan 36 R7: usage error — bad flag, path not found, wrong extension,
/// no nova.toml. Mapped to exit code **2** (distinct from diagnostic
/// failures which are exit=1).
///
/// Cargo-style: 0 = success, 1 = diagnostic, 2 = usage, 101 = panic.
#[derive(Debug)]
struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UsageError {}

/// Helper: создаёт UsageError-wrapped anyhow::Error.
fn usage_err(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(UsageError(msg.into()))
}

// ---------- CLI definition ----------

#[derive(Parser)]
#[command(
    name = "nova",
    version,
    about = "Nova language CLI — build, run, test Nova programs"
)]
struct Cli {
    /// Plan 36 R10: color control. `auto` (default) detects via env,
    /// `always` forces ANSI even in pipes, `never` disables completely.
    /// Respects NO_COLOR, CLICOLOR, CLICOLOR_FORCE, CI, TERM=dumb env vars.
    #[arg(long, global = true, default_value = "auto",
          value_parser = ["auto", "always", "never"])]
    color: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Type-check Nova source files or directories.
    ///
    /// Polymorphic positional path: file → single check, directory → recursive walk.
    /// Multiple paths supported. Empty → check current workspace (walks parents
    /// до nova.toml).
    Check {
        /// Paths to check (files or directories). If empty, uses workspace root.
        #[arg(num_args = 0..)]
        paths: Vec<PathBuf>,
        /// Number of parallel workers (0 = num_cpus, default 0).
        #[arg(long, default_value_t = 0)]
        jobs: usize,
        /// Show only failures and summary (no per-file `ok:` lines).
        #[arg(long, short = 'q', conflicts_with = "verbose")]
        quiet: bool,
        /// Show extra info per file (timing, expanded warnings).
        #[arg(long, short = 'v', conflicts_with = "quiet")]
        verbose: bool,
        /// List files that would be checked, without checking. Useful для
        /// отладки --skip / implicit-excludes.
        #[arg(long)]
        list: bool,
        /// Output format. `human` (default) — colored per-file; `short` —
        /// `file:line:col: msg` для grep. JSON/SARIF/JUnit — sub-plan 36.A.
        #[arg(long, default_value = "human", value_parser = ["human", "short"])]
        format: String,
        /// Include std/runtime/ (auto-gen, normally skipped).
        #[arg(long = "include-runtime")]
        include_runtime: bool,
        /// Skip files whose path matches this substring (repeatable).
        #[arg(long = "skip", value_name = "PATTERN")]
        skip: Vec<String>,
    },
    /// Run a Nova source file via the interpreter.
    Run {
        file: PathBuf,
    },
    /// Plan 03.1 Ф.5: добавить зависимость в `[dependencies]` nova.toml
    /// текущего пакета и обновить nova.lock.
    Add {
        /// Имя зависимости (должно совпадать с `[package].name` пакета).
        name: String,
        /// Локальная path-зависимость: путь к каталогу пакета.
        #[arg(long, value_name = "DIR", conflicts_with = "git")]
        path: Option<String>,
        /// Git-зависимость: URL репозитория.
        #[arg(long, value_name = "URL")]
        git: Option<String>,
        /// Git-пин: тег (только с --git).
        #[arg(long, requires = "git", conflicts_with_all = ["branch", "rev", "version"])]
        tag: Option<String>,
        /// Git-пин: ветка (только с --git).
        #[arg(long, requires = "git", conflicts_with_all = ["tag", "rev", "version"])]
        branch: Option<String>,
        /// Git-пин: commit / rev (только с --git).
        #[arg(long, requires = "git", conflicts_with_all = ["tag", "branch", "version"])]
        rev: Option<String>,
        /// Plan 03.2: git-пин semver-диапазоном (`^1.2`) — только с --git.
        #[arg(long, requires = "git", conflicts_with_all = ["tag", "branch", "rev"])]
        version: Option<String>,
    },
    /// Plan 03.1 Ф.5 / 03.2 Ф.4: пере-резолвить git-зависимости и
    /// обновить nova.lock. Без аргумента — все git-зависимости.
    Update {
        /// Имя зависимости для обновления (опционально — иначе все git).
        name: Option<String>,
        /// Plan 03.2: зафиксировать точную версию —
        /// `nova update --precise foo@1.2.3`.
        #[arg(long, value_name = "NAME@VERSION", conflicts_with = "name")]
        precise: Option<String>,
    },
    /// Plan 03.4: effect-surface пакета — агрегированные эффекты его
    /// публичного API («использует Net, Fs»).
    Info {
        /// Путь к пакету (.nv-файл / каталог) либо имя зависимости из
        /// `[dependencies]` текущего пакета.
        target: String,
        /// Формат вывода.
        #[arg(long, default_value = "human", value_parser = ["human", "json"])]
        format: String,
        /// Plan 03.4 Ф.2: сравнить effect-surface с базовым
        /// пакетом/версией (путь либо имя зависимости).
        #[arg(long, value_name = "PATH|dep")]
        diff: Option<String>,
        /// С `--diff`: ненулевой exit-код, если появились новые
        /// эффекты (CI-gate против supply-chain).
        #[arg(long = "fail-on-new", requires = "diff")]
        fail_on_new: bool,
    },
    /// Plan 45 / D107: produce documentation for a Nova source file.
    ///
    /// MVP: one file at a time, output to stdout. Supported formats:
    /// `markdown` (default), `json` (D107 schema v1).
    Doc {
        /// Path to a `.nv` file or directory. Optional when `--json-schema` is used.
        #[arg(num_args = 0..=1)]
        file: Option<PathBuf>,
        /// Output format: `markdown` (default) or `json`.
        #[arg(long = "format", default_value = "markdown")]
        format: String,
        /// Print the embedded JSON Schema 2020-12 and exit (offline-
        /// validation, IDE auto-completion, LLM prompt context).
        #[arg(long = "json-schema")]
        json_schema: bool,
        /// Include private (non-exported) items in output.
        /// By default only items marked `export` are documented.
        #[arg(long = "include-private")]
        include_private: bool,
        /// Plan 45 Ф.7: run doc-tests (`nova` fenced code blocks) instead
        /// of rendering. Reports pass/fail/skipped per test.
        #[arg(long = "test")]
        run_doc_tests: bool,
        /// Plan 45 Ф.14: validate doc-content without rendering. Reports
        /// broken intra-doc-links and missing summaries; exits non-zero
        /// on any issue. Useful in CI.
        #[arg(long = "check")]
        check: bool,
        /// Plan 45 Ф.15: re-render on file change (mtime poll, 500ms).
        /// Ctrl-C to exit. Works with --format and --check.
        #[arg(long = "watch")]
        watch: bool,
        /// Plan 45 Ф.21.6 / D105: report doc-coverage metrics
        /// (% items with summary, broken down by kind). Useful in CI.
        #[arg(long = "coverage")]
        coverage: bool,
        /// Plan 45 Ф.33.2: doc-coverage CI gate. If `--coverage-threshold N`
        /// is given (0-100), exit с non-zero code если % documented items <
        /// threshold. Useful как CI step: `nova doc <dir> --coverage --coverage-threshold 80`.
        #[arg(long = "coverage-threshold", value_name = "PERCENT")]
        coverage_threshold: Option<u32>,
        /// Plan 45 Ф.24.13: number of parallel parse jobs for workspace mode.
        /// Default 0 = auto (uses all logical CPUs). Ignored for single-file.
        #[arg(long = "jobs", default_value = "0")]
        jobs: usize,
        /// Plan 45 Ф.24.10: diff two JSON doc outputs for semver change detection.
        /// Usage: nova doc --diff old.json new.json
        /// Exit code: 0 = no breaking changes, 1 = major, 2 = minor, 3 = patch.
        #[arg(long = "diff", num_args = 2, value_names = ["OLD", "NEW"])]
        diff: Option<Vec<PathBuf>>,
        /// Plan 45 Ф.24.9: scan workspace directory for call-sites and attach
        /// top-3 usage examples to each documented fn. Accepts a path to the
        /// workspace root (all *.nv files scanned recursively).
        #[arg(long = "scrape-examples", value_name = "WORKSPACE")]
        scrape_examples: Option<PathBuf>,
        /// Plan 45 Ф.25.1: treat diagnostic warnings as errors. With
        /// `--strict`, the command exits non-zero if any warnings were
        /// produced (malformed doc-attrs, unknown doc-test modifiers,
        /// ambiguous intra-doc-links). Useful in CI to enforce clean docs.
        #[arg(long = "strict")]
        strict: bool,
        /// Plan 45 Ф.25.4: mutation testing for contracts. Generates
        /// mutants (`>` ↔ `>=`, `==` ↔ `!=`, drop `requires`) for each
        /// function with contracts; reports survived mutants (under-tested
        /// boundaries). Nova-unique — никто из rustdoc/godoc/typedoc не
        /// делает mutation testing для specifications.
        ///
        /// Default — text-based heuristic (быстро, ~1ms per mutant).
        /// `--real-exec` — actually run mutated doc-tests (true positive
        /// guarantee, ~100ms per mutant per test).
        #[arg(long = "mutate-contracts")]
        mutate_contracts: bool,
        /// Plan 45 Ф.28.2: enable real-exec mode для `--mutate-contracts`.
        /// Substitutes mutated_expr в source и actually runs doc-tests
        /// через test_runner. Slower (~100ms per mutant per test) но
        /// gives true positive guarantee. Без флага — text-based heuristic.
        #[arg(long = "real-exec", requires = "mutate_contracts")]
        real_exec: bool,
        /// Plan 45 Ф.31.4: write multi-page HTML output to directory.
        /// Each module → separate file (`<module.path>.html`). `index.html`
        /// — workspace overview. Only valid с `--format html`.
        #[arg(long = "output-dir", value_name = "DIR")]
        output_dir: Option<PathBuf>,
    },
    /// Plan 45 Ф.32.1 — query JSON doc output via DSL.
    ///
    /// Reads `nova doc --format json` output, filters items по query DSL,
    /// prints matched results as compact JSON.
    ///
    /// Query syntax: comma-separated key=value pairs. Supported keys:
    /// `kind`, `name`, `module`, `module-prefix`, `capability`, `effect`,
    /// `has-contracts`, `verified`, `stability`, `deprecated`.
    ///
    /// Examples:
    ///   nova doc-query out.json "kind=fn,capability=pure"
    ///   nova doc-query out.json "name=add,has-contracts=true"
    ///   nova doc-query out.json "module-prefix=std,effect=Fs"
    ///
    /// Foundation для future MCP server (Ф.32.2).
    DocQuery {
        /// Path to JSON file produced by `nova doc --format json`.
        json_file: PathBuf,
        /// Query DSL string (see command help для syntax).
        #[arg(default_value = "")]
        query: String,
    },
    /// Plan 45 Ф.32.3 — MCP server (JSON-RPC over stdio).
    ///
    /// Reads line-delimited JSON-RPC requests from stdin, writes responses
    /// to stdout. Compatible с MCP clients (Claude Code, MCP Inspector).
    ///
    /// Tools exposed:
    ///   - query_items(query) — search via DSL
    ///   - list_modules() — list module paths
    ///   - get_item(item_id) — fetch full item JSON
    ///
    /// Usage:
    ///   nova doc-mcp <file.nv|file.json>
    ///
    /// MCP client connects, sends `initialize`, then `tools/list`/`tools/call`.
    DocMcp {
        /// Path to .nv source или .json (pre-generated `nova doc --format json`).
        file: PathBuf,
        /// Plan 45 Ф.34.1: run as HTTP server на 127.0.0.1:PORT (POST /mcp).
        /// По умолчанию — stdio JSON-RPC loop.
        #[arg(long = "port", value_name = "PORT")]
        port: Option<u16>,
    },
    /// Compile a single Nova source file to a native binary.
    ///
    /// **Single file only** — `nova build` produces one binary per invocation
    /// (`-o output` is one path). For multi-file projects, use imports within
    /// the entry-point file. To typecheck multiple files use `nova check <dir>`.
    Build {
        /// Single .nv file (entry-point with `fn main`).
        file: PathBuf,
        /// Output binary path (default: <name>[.exe] next to source).
        #[arg(short = 'o')]
        output: Option<PathBuf>,
        /// Build mode: 'dev' (unoptimized) or 'release' (optimized).
        #[arg(long, default_value = "dev", value_parser = ["dev", "release"])]
        mode: String,
        /// C compiler to use.
        #[arg(long, default_value = "auto", value_parser = ["auto", "clang", "msvc", "gcc"])]
        toolchain: String,
        /// Path to vcvars64.bat (Windows, auto-detected via vswhere).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Path to clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Compiler timeout in seconds (default: 120).
        #[arg(long, default_value_t = 120)]
        timeout: u64,
        /// Keep .c / .exe / .obj build artifacts after the build.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// Plan 48 Ф.7.6: max monomorphization-instantiation depth (guards
        /// against polymorphic recursion). Default 500 unless overridden
        /// via env var NOVA_MONO_DEPTH.
        #[arg(long = "mono-depth", value_name = "N")]
        mono_depth: Option<usize>,
    },
    /// Run Nova tests from a directory or single file.
    ///
    /// Positional path: file → run that file (must have test blocks),
    /// directory → walk recursively. Default: `<repo>/nova_tests/`.
    Test {
        /// Path to tests directory or file. Default: `<repo>/nova_tests/`.
        #[arg(num_args = 0..=1)]
        path: Option<PathBuf>,
        /// Filter by display-name substring.
        #[arg(long)]
        filter: Option<String>,
        /// Number of parallel workers (0 = num_cpus, default 0).
        #[arg(long, default_value_t = 0)]
        jobs: usize,
        /// Output format: 'text', 'json', 'tap', or 'junit'.
        #[arg(long, default_value = "text", value_parser = ["text", "json", "tap", "junit"])]
        format: String,
        /// Build mode: 'dev' (unoptimized) or 'release' (optimized).
        #[arg(long, default_value = "dev", value_parser = ["dev", "release"])]
        mode: String,
        /// C compiler to use.
        #[arg(long, default_value = "auto", value_parser = ["auto", "clang", "msvc", "gcc"])]
        toolchain: String,
        /// Path to vcvars64.bat (Windows, auto-detected via vswhere).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Path to clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Per-test timeout in seconds (default: 60).
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Show output of passing tests too.
        #[arg(long, short = 'v')]
        verbose: bool,
        /// Show only failures and summary.
        #[arg(long, short = 'q')]
        quiet: bool,
        /// Path to write last-test-results.json.
        /// Default: <repo>/target/last-test-results.json
        #[arg(long = "results-file")]
        results_file: Option<PathBuf>,
        /// Re-run only tests that failed/timed-out in the last run.
        #[arg(long = "rerun-failed")]
        rerun_failed: bool,
        /// Retry count for transient failures (AV races, etc). Default 0.
        #[arg(long, default_value_t = 0)]
        retries: u32,
        /// Include std/ files in the run.
        #[arg(long = "include-stdlib")]
        include_stdlib: bool,
        /// Keep .c / .exe / .obj build artifacts after the run.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// GC backend: 'boehm' (default after Plan 27) or 'malloc' (no GC, internal only).
        /// Reserved for Plan 27 — currently accepted but has no effect.
        #[arg(long, value_parser = ["boehm", "malloc"])]
        gc: Option<String>,
        /// List collected tests without running them.
        #[arg(long)]
        list: bool,
        /// Path to a file containing test names (one per line) to run (exact display-name match).
        #[arg(long = "filter-from")]
        filter_from: Option<PathBuf>,
        /// Shuffle test execution order. Optional seed value; omit for random seed.
        /// Example: --shuffle 42  or  --shuffle (random seed).
        #[arg(long, num_args = 0..=1, value_name = "SEED")]
        shuffle: Option<Option<u64>>,
        /// Skip tests whose display name OR file path contains this substring.
        /// Repeatable: `--skip A --skip B` excludes both.
        /// Example: `nova test std/ --skip std/runtime/`.
        #[arg(long = "skip", value_name = "PATTERN")]
        skip: Vec<String>,
        /// Plan 48 Ф.7.6: max monomorphization-instantiation depth (guards
        /// against polymorphic recursion). Default 500 unless overridden
        /// via env var NOVA_MONO_DEPTH.
        #[arg(long = "mono-depth", value_name = "N")]
        mono_depth: Option<usize>,
    },
    /// Build and run a single Nova test file (used by IDE / CI for one-shot debug).
    #[command(name = "test-build")]
    TestBuild {
        file: PathBuf,
        /// Build mode: 'dev' (unoptimized) or 'release' (optimized).
        #[arg(long, default_value = "dev", value_parser = ["dev", "release"])]
        mode: String,
        /// C compiler to use.
        #[arg(long, default_value = "auto", value_parser = ["auto", "clang", "msvc", "gcc"])]
        toolchain: String,
        /// Path to vcvars64.bat (Windows, auto-detected via vswhere).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Path to clang.exe (override auto-detect).
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Timeout in seconds (default: 60).
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        /// Keep .c / .exe / .obj build artifacts after the run.
        #[arg(long = "keep-artifacts")]
        keep_artifacts: bool,
        /// GC backend: 'boehm' (default after Plan 27) or 'malloc' (no GC, internal only).
        /// Reserved for Plan 27 — currently accepted but has no effect.
        #[arg(long, value_parser = ["boehm", "malloc"])]
        gc: Option<String>,
        /// Plan 48 Ф.7.6: max monomorphization-instantiation depth (guards
        /// against polymorphic recursion). Default 500 unless overridden
        /// via env var NOVA_MONO_DEPTH.
        #[arg(long = "mono-depth", value_name = "N")]
        mono_depth: Option<usize>,
    },
    /// Regenerate std/runtime/*.nv stubs from the runtime registry.
    #[command(name = "regen-runtime")]
    RegenRuntime {
        /// Only compare — fail if stubs diverge from registry (CI guard).
        #[arg(long)]
        check: bool,
    },
    /// Plan 33.3 Ф.13: Contract inspection commands.
    ///
    /// Subcommands: list, verify, suggest, counterexample.
    /// All output JSON (AI-friendly schema). See docs/contracts-diag-schema.json.
    #[command(subcommand)]
    Contracts(ContractsCmd),
    /// Plan 57: Benchmark commands.
    ///
    /// Subcommands: run, diff, gate.
    /// Runs `bench "..." { measure { ... } }` declarations с Criterion-style
    /// adaptive sampling, statistical analysis (median/MAD/Welch's t-test),
    /// JSON v1 schema output, reproducibility metadata.
    #[command(subcommand)]
    Bench(BenchCmd),
    /// Plan 100.8 / D7: consume-type coverage analyzer.
    ///
    /// Scans a Nova source file or directory, collects all consume-typed
    /// bindings, and reports how many are covered via consume-methods,
    /// errdefer, or okdefer. Useful as a CI hygiene check.
    ///
    /// Exit code: 0 = all covered, 1 = uncovered bindings found, 2 = usage error.
    #[command(name = "consume-analyze")]
    ConsumeAnalyze {
        /// Path to a `.nv` file or directory to analyze.
        path: PathBuf,
        /// Output format: `human` (default) or `json`.
        #[arg(long, default_value = "human", value_parser = ["human", "json"])]
        format: String,
        /// Exit non-zero if any uncovered consume binding is found (CI gate).
        #[arg(long = "fail-on-uncovered")]
        fail_on_uncovered: bool,
    },
}

/// Plan 57: `nova bench <subcommand>`.
#[derive(Subcommand)]
enum BenchCmd {
    /// Run benchmarks in a .nv file. Compiles в release-mode, запускает,
    /// собирает samples, выводит таблицу + опционально JSON/CSV/markdown.
    Run {
        /// Path to a .nv file containing `bench "..." { ... }` declarations.
        file: PathBuf,
        /// Substring filter — comma-separated bench-name fragments.
        #[arg(long, value_name = "PATTERN")]
        filter: Option<String>,
        /// Override sample count (default 100).
        #[arg(long)]
        samples: Option<u64>,
        /// Override warmup duration in ms (default 500).
        #[arg(long = "warmup-ms")]
        warmup_ms: Option<u64>,
        /// Override per-bench time budget in seconds (default 10).
        #[arg(long = "time-budget")]
        time_budget_s: Option<u64>,
        /// GC backend (malloc|boehm). Default boehm (Plan 27).
        #[arg(long, default_value = "boehm")]
        gc: String,
        /// Build mode (release|dev). Release is preferred for bench
        /// (5-20× faster + stable timings); dev is fallback когда
        /// release-mode требует lld (linux LTO). Default: release.
        #[arg(long, default_value = "release")]
        mode: String,
        /// Toolchain (auto|clang|msvc|gcc).
        #[arg(long, default_value = "auto")]
        toolchain: String,
        /// Path to vcvars64.bat (Windows MSVC).
        #[arg(long)]
        vcvars: Option<PathBuf>,
        /// Explicit clang path.
        #[arg(long)]
        clang: Option<PathBuf>,
        /// Compile timeout in seconds.
        #[arg(long, default_value_t = 120)]
        compile_timeout: u64,
        /// Bench process run timeout in seconds.
        #[arg(long, default_value_t = 600)]
        run_timeout: u64,
        /// Keep intermediate .c / .exe artifacts in tmp dir (debug).
        #[arg(long)]
        keep_artifacts: bool,
        /// Override mono instantiation depth limit.
        #[arg(long, value_name = "N")]
        mono_depth: Option<usize>,
        /// Write JSON v1 result to file.
        #[arg(long = "out")]
        out_json: Option<PathBuf>,
        /// Write CSV result to file.
        #[arg(long = "out-csv")]
        out_csv: Option<PathBuf>,
        /// Write markdown result to file (для PR comment).
        #[arg(long = "out-md")]
        out_md: Option<PathBuf>,
        /// Plan 57.B.2: Criterion-compatible JSON output directory.
        /// Layout: `<dir>/<safe-name>/new/{estimates,sample,benchmark}.json`.
        /// Compatible с cargo-criterion --message-format=criterion.
        #[arg(long = "out-criterion")]
        out_criterion: Option<PathBuf>,
        /// Plan 57.A.5: profile mode (cpu|heap|gc) + output path.
        /// CPU = wraps samply (must be installed via `cargo install samply`).
        #[arg(long = "profile", value_names = ["MODE", "OUT"], num_args = 2)]
        profile: Option<Vec<String>>,
        /// Plan 57.G.4: ASCII histogram per bench after the table
        /// (Unicode block chars, 40 buckets, M=median + [ ] Tukey fences).
        #[arg(long)]
        histogram: bool,
    },
    /// Compare two bench JSON results. Outputs Welch's t-test p-values
    /// + geomean delta + reproducibility check.
    Diff {
        /// Baseline JSON (older / reference).
        baseline: PathBuf,
        /// New JSON (recent / candidate).
        new: PathBuf,
        /// Output format (terminal|markdown|json).
        #[arg(long, default_value = "terminal")]
        format: String,
        /// Plan 57.F.2: AI regression interpretation. Opt-in — sends
        /// diff + git context к LLM (NOVA_AI_API_KEY required).
        #[arg(long)]
        explain: bool,
        /// Override AI config path (default: ~/.nova-ai.toml).
        #[arg(long = "ai-config")]
        ai_config: Option<PathBuf>,
        /// Override max tokens (default: 4000).
        #[arg(long = "ai-max-tokens")]
        ai_max_tokens: Option<u32>,
        /// AI dry-run: print would-be request body без API call.
        #[arg(long = "ai-dry-run")]
        ai_dry_run: bool,
        /// Baseline git SHA (для diff context). Auto-detected from
        /// JSON metadata если возможно.
        #[arg(long = "baseline-sha")]
        baseline_sha: Option<String>,
        /// New git SHA (для diff context). Auto-detected from JSON.
        #[arg(long = "new-sha")]
        new_sha: Option<String>,
    },
    /// CI gate — apply thresholds from bench.toml. Exit 0 = pass, 1 = regress.
    Gate {
        /// Baseline JSON.
        baseline: PathBuf,
        /// New JSON.
        new: PathBuf,
        /// Path to bench.toml (default: ./bench.toml).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Path to .nova-bench-noise.json (Plan 57.A.3 auto noise-floor).
        /// Default: ./.nova-bench-noise.json если есть.
        #[arg(long = "noise")]
        noise: Option<PathBuf>,
    },
    /// Plan 57.A.3: Auto-calibrate noise floor from N repeated runs of
    /// same baseline. Saves to .nova-bench-noise.json (machine-specific).
    Calibrate {
        /// JSON result files (>= 2) from repeated runs of the same source.
        #[arg(required = true, num_args = 2..)]
        runs: Vec<PathBuf>,
        /// Output noise floor JSON path.
        #[arg(long, default_value = ".nova-bench-noise.json")]
        out: PathBuf,
    },
    /// Plan 57.B.4: Diagnose CPU instructions counter availability.
    /// Linux: tries perf_event_open + measures known loop. Other OS: stub.
    #[command(name = "cpu-instr-check")]
    CpuInstrCheck,
    /// Plan 57.F.3: Diagnose memory bandwidth measurement availability.
    /// Linux: probes /sys/devices/uncore_imc_* + tries LLC-miss perf
    /// counter. Other OS: prints stub message.
    #[command(name = "membw-check")]
    MembwCheck,
    /// Plan 57.H.2: Hyperfine-style cross-binary timing — wall-clock
    /// measurement of arbitrary external commands. Output schema-compatible
    /// с `nova bench diff` (per-binary entry в JSON v1).
    ///
    /// Example:
    ///   nova bench hyperfine \
    ///     "old=./nova-old build large.nv" \
    ///     "new=./nova-new build large.nv" \
    ///     --samples 10 --warmup 2 --out result.json
    /// Plan 57.H.3: Run binary под Valgrind Callgrind, deterministic
    /// CPU instructions count (cross-platform fallback к perf_event_open
    /// Linux-only). Works на macOS + Linux with valgrind installed.
    ///
    /// Example:
    ///   nova bench callgrind ./my-bench --gc malloc --cache-sim
    Callgrind {
        /// Executable path.
        binary: PathBuf,
        /// Args для executable.
        #[arg(num_args = 0..)]
        args: Vec<String>,
        /// Enable cache simulation (I1/D1/LL miss counts). Slower.
        #[arg(long = "cache-sim")]
        cache_sim: bool,
        /// Optional cwd для command.
        #[arg(long = "workdir")]
        workdir: Option<PathBuf>,
        /// JSON output path для CallgrindResult.
        #[arg(long = "out")]
        out: Option<PathBuf>,
    },
    /// Plan 57.H.3: Check valgrind availability + version.
    #[command(name = "callgrind-check")]
    CallgrindCheck,
    Hyperfine {
        /// Specs: each "name=binary args..." или просто "binary args...".
        #[arg(required = true, num_args = 1..)]
        specs: Vec<String>,
        /// Warmup runs (discarded). Default 3.
        #[arg(long, default_value_t = 3)]
        warmup: u32,
        /// Sample runs (kept). Default 10.
        #[arg(long, default_value_t = 10)]
        samples: u32,
        /// Per-command timeout seconds. Default 300 (5 min).
        #[arg(long = "timeout", default_value_t = 300)]
        timeout_secs: u64,
        /// Optional cwd для commands.
        #[arg(long = "workdir")]
        workdir: Option<PathBuf>,
        /// JSON output path (default: print to stdout).
        #[arg(long = "out")]
        out: Option<PathBuf>,
    },
    /// Plan 57.D.4: Print recommended history branch name based on
    /// NOVA_BENCH_RUNNER_ID env (multi-runner CI matrix support).
    /// Returns `bench-history` если env не set, иначе `bench-history-<id>`.
    #[command(name = "runner-branch")]
    RunnerBranch,
    /// Plan 57.E.5: detect changepoints (anomalies) в historical median
    /// time-series per bench via PELT algorithm. Identifies regimes
    /// where perf significantly shifted (≥5% delta).
    #[command(name = "history-anomalies")]
    HistoryAnomalies {
        /// Branch (default: auto = NOVA_BENCH_RUNNER_ID-aware).
        #[arg(long, default_value = "auto")]
        branch: String,
        /// Output format (text|json).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Plan 57.F.1: SSH distributed bench coordination.
    /// Subcommands: list (configured remotes), ping (health), run.
    #[command(subcommand)]
    Remote(BenchRemoteCmd),
    /// Plan 57.C.8: Measure per-pass compile time для corpus file(s).
    /// Wraps nova build с NOVA_PERF_TIMER=1; parses __PERF__ markers.
    Corpus {
        /// .nv file or directory с .nv files.
        path: PathBuf,
        /// JSON output (default: terminal table).
        #[arg(long)]
        json: bool,
        /// Plan 57.D.5: HTML compiler-perf dashboard output path.
        /// Generates echarts stacked bar chart per-file + detail table.
        #[arg(long)]
        html: Option<PathBuf>,
        /// Custom echarts URL (для offline режима).
        #[arg(long = "echarts-url",
              default_value = "https://cdn.jsdelivr.net/npm/echarts@5.4.3/dist/echarts.min.js")]
        echarts_url: String,
        /// Build mode (release|dev).
        #[arg(long, default_value = "release")]
        mode: String,
        /// Toolchain (auto|clang|msvc).
        #[arg(long, default_value = "auto")]
        toolchain: String,
        /// GC backend (malloc|boehm).
        #[arg(long, default_value = "boehm")]
        gc: String,
    },
    /// Plan 57.A.1: Append result JSON to an orphan history branch.
    #[command(name = "history-add")]
    HistoryAdd {
        /// Result JSON to append (output of `nova bench run --out`).
        result: PathBuf,
        /// Orphan branch name (default: bench-history).
        #[arg(long, default_value = "auto")]
        branch: String,
        /// Push to remote after commit.
        #[arg(long)]
        push: bool,
        /// Remote name when --push (default: origin).
        #[arg(long, default_value = "origin")]
        remote: String,
        /// Dry-run: print what would be done, do not commit.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    /// Plan 57.A.1: List bench history entries (newest first).
    #[command(name = "history-list")]
    HistoryList {
        /// Branch (default: bench-history).
        #[arg(long, default_value = "auto")]
        branch: String,
    },
    /// Plan 57.C.6: squash older history entries по retention policy.
    /// Yearly squash recommended (см. docs/perf-conventions.md).
    #[command(name = "history-squash")]
    HistorySquash {
        /// Squash entries older than this date (YYYY-MM-DD UTC).
        #[arg(long = "before-date")]
        before_date: String,
        /// Branch (default: bench-history).
        #[arg(long, default_value = "auto")]
        branch: String,
        /// Push to remote after squash.
        #[arg(long)]
        push: bool,
        /// Remote name when --push.
        #[arg(long, default_value = "origin")]
        remote: String,
        /// Dry-run: print what would be removed, do not commit.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    /// Plan 57.A.2: Generate static HTML dashboard from history.
    /// Reads from history orphan branch, writes <out>/index.html +
    /// <out>/bench-<safe>.html per bench, plus <out>/data.json.
    Dashboard {
        /// History branch (default: bench-history).
        #[arg(long = "history-branch", default_value = "auto")]
        history_branch: String,
        /// Output directory (default: dashboard/).
        #[arg(long, default_value = "dashboard")]
        out: PathBuf,
        /// Max history entries to include (newest first).
        #[arg(long = "max-entries", default_value_t = 200)]
        max_entries: usize,
        /// Custom echarts URL (offline = local path).
        #[arg(long = "echarts-url",
              default_value = "https://cdn.jsdelivr.net/npm/echarts@5.4.3/dist/echarts.min.js")]
        echarts_url: String,
    },
}

/// Plan 57.F.1: `nova bench remote <subcommand>`.
#[derive(Subcommand)]
enum BenchRemoteCmd {
    /// List configured remotes from ~/.nova-bench-remotes.toml.
    List {
        /// Override config path (default: ~/.nova-bench-remotes.toml or
        /// $NOVA_BENCH_REMOTES env).
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// SSH health check для one remote.
    Ping {
        /// Remote name.
        name: String,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Parallel bench run на N remotes; gather results в gather-into dir.
    Run {
        /// Bench .nv file path (relative to repo root on remote).
        bench: PathBuf,
        /// Comma-separated remote names (or "all").
        #[arg(long, default_value = "all")]
        remotes: String,
        /// Output directory для per-remote JSON results.
        #[arg(long = "gather-into", default_value = "remote-results")]
        gather_into: PathBuf,
        /// Optional git SHA to checkout перед бенчем.
        #[arg(long)]
        sha: Option<String>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

/// Plan 33.3 Ф.13: `nova contracts <subcommand>`.
#[derive(Subcommand)]
enum ContractsCmd {
    /// List all contracts in a Nova source file.
    List {
        /// Path to .nv file.
        file: PathBuf,
    },
    /// Verify contracts in a Nova source file and output JSON diagnostics.
    Verify {
        /// Path to .nv file.
        file: PathBuf,
        /// SMT backend to use (overrides NOVA_SMT_BACKEND).
        #[arg(long)]
        backend: Option<String>,
    },
    /// Suggest contracts for a function (AI-assisted stubs).
    Suggest {
        /// Path to .nv file.
        file: PathBuf,
        /// Function name.
        fn_name: String,
    },
    /// Show counterexample for a failing contract.
    Counterexample {
        /// Path to .nv file.
        file: PathBuf,
        /// Function name.
        fn_name: String,
        /// Contract index (0-based).
        #[arg(long, default_value_t = 0)]
        contract_id: usize,
    },
}

// ---------- repo root discovery ----------

/// Walk up from CWD until a directory containing `nova.toml` is found.
/// Plan 35 sub-plan 35.B (sync): workspace-aware lookup из CWD. Использует
/// `nova_codegen::test_runner::find_repo_root_from` — тот же helper что
/// test_runner pipeline (D78 AD6: prefer `[workspace]`-marked nova.toml,
/// иначе topmost nova.toml). Без sync nova-cli мог найти первый встреченный
/// nova.toml (nova_tests/nova.toml в nested-repos), что ломало std-import
/// resolve.
fn find_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| anyhow!("cannot determine current directory: {}", e))?;
    // find_repo_root_from принимает path к файлу — даём ему синтетический
    // path в cwd (parent будет cwd), что симулирует «ищем root от cwd».
    let probe = cwd.join("__novacli_probe__.nv");
    nova_codegen::test_runner::find_repo_root_from(&probe)
        .or_else(|| {
            // Fallback: если canonicalize не сработал (probe не существует),
            // walk вверх от cwd напрямую.
            let mut dir = cwd.clone();
            let mut last_toml_dir: Option<PathBuf> = None;
            loop {
                let toml = dir.join("nova.toml");
                if toml.exists() {
                    if let Ok(content) = std::fs::read_to_string(&toml) {
                        if content.contains("[workspace]") {
                            return Some(dir);
                        }
                    }
                    last_toml_dir = Some(dir.clone());
                }
                match dir.parent() {
                    Some(p) if p != dir => dir = p.to_path_buf(),
                    _ => return last_toml_dir,
                }
            }
        })
        .ok_or_else(|| anyhow!(
            "nova.toml not found — are you inside a Nova project?\n\
             (Searched from {} up to filesystem root.)",
            std::env::current_dir().unwrap_or_default().display()
        ))
}

struct RepoPaths {
    tests_dir: PathBuf,
    stdlib_dir: PathBuf,
    cg_include: PathBuf,
    rt_dir: PathBuf,
    default_results_file: PathBuf,
}

fn resolve_paths(repo: &Path) -> RepoPaths {
    RepoPaths {
        tests_dir: repo.join("nova_tests"),
        stdlib_dir: repo.join("std"),
        cg_include: repo.join("compiler-codegen"),
        rt_dir: repo.join("compiler-codegen").join("nova_rt"),
        default_results_file: repo.join("target").join("last-test-results.json"),
    }
}

// ---------- ANSI colors ----------
// Plan 36 R10: `--color auto|always|never` + env detection.
//
// Auto-detection precedence (highest first):
//   1. `--color always|never` CLI flag
//   2. `CLICOLOR_FORCE=1` env → always
//   3. `NO_COLOR=*` env → never (https://no-color.org)
//   4. `CLICOLOR=0` env → never
//   5. `CI=true` env → never (CI logs usually no-ANSI)
//   6. `TERM=dumb` env → never
//   7. default → auto = on (если nothing above triggers)
//
// На Windows ANSI поддерживается в Windows Terminal / modern conhost.
// is-terminal crate отложен (post-MVP); пока default = always on.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(ColorMode::Auto),
            "always" => Ok(ColorMode::Always),
            "never" => Ok(ColorMode::Never),
            _ => Err(usage_err(format!(
                "invalid --color value `{}` (expected: auto, always, never)", s))),
        }
    }
}

static COLOR_MODE: std::sync::OnceLock<ColorMode> = std::sync::OnceLock::new();

fn set_color_mode(mode: ColorMode) {
    let _ = COLOR_MODE.set(mode);
}

fn colors_enabled() -> bool {
    let mode = COLOR_MODE.get().copied().unwrap_or(ColorMode::Auto);
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            // CLICOLOR_FORCE wins over all "off" signals.
            if std::env::var_os("CLICOLOR_FORCE")
                .map(|v| v != "0")
                .unwrap_or(false)
            {
                return true;
            }
            if std::env::var_os("NO_COLOR").is_some() {
                return false;
            }
            if std::env::var("CLICOLOR").as_deref() == Ok("0") {
                return false;
            }
            if std::env::var("CI").as_deref() == Ok("true") {
                return false;
            }
            if std::env::var("TERM").as_deref() == Ok("dumb") {
                return false;
            }
            true
        }
    }
}

fn red(s: &str) -> String {
    if colors_enabled() { format!("\x1b[31m{}\x1b[0m", s) } else { s.to_string() }
}
fn yellow(s: &str) -> String {
    if colors_enabled() { format!("\x1b[33m{}\x1b[0m", s) } else { s.to_string() }
}
fn green(s: &str) -> String {
    if colors_enabled() { format!("\x1b[32m{}\x1b[0m", s) } else { s.to_string() }
}
fn bold(s: &str) -> String {
    if colors_enabled() { format!("\x1b[1m{}\x1b[0m", s) } else { s.to_string() }
}

// ---------- helpers ----------

fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| anyhow!("failed to read {}: {}", path.display(), e))
}

/// Plan 45 Ф.24.13: parse + typecheck + infer_effects one .nv file.
/// Returns Ok((source, module)) or Err(warning_string) for graceful-fail.
///
/// Plan 71 / D127: после parse инициализирует `module.peer_files[0]` с
/// абсолютным path-ом файла, чтобы `doc::collect` мог populate
/// `DocModule.source_paths` для fixture-detection в lints. Parser
/// оставляет peer_files пустым — `resolve_imports_inline` заполняет в
/// build/test path'ах, но `nova doc` MVP single-file mode не резолвит
/// импорты, поэтому seed'им здесь.
fn parse_one_file(f: &Path) -> Result<(String, nova_codegen::ast::Module), String> {
    let src = match std::fs::read_to_string(f) {
        Ok(s) => s,
        Err(e) => return Err(format!("warning: {}: {}", f.display(), e)),
    };
    let path_str = f.to_string_lossy();
    match nova_codegen::parser::parse(&src) {
        Ok(mut m) => {
            let _ = nova_codegen::types::check_module(&m);
            nova_codegen::types::infer_effects(&mut m);
            ensure_entry_peer_path(&mut m, f);
            Ok((src, m))
        }
        Err(d) => Err(format!(
            "warning: {}: {}",
            path_str,
            d.render(&src, &path_str)
        )),
    }
}

/// Plan 71 / D127: seed `module.peer_files[0]` if empty. Used by
/// `nova doc` single-file mode (which does not invoke
/// `resolve_imports_inline`), so the doc collector can read the source
/// path для fixture-detection lints.
///
/// No-op if `peer_files` already populated (resolver уже сделал работу).
fn ensure_entry_peer_path(module: &mut nova_codegen::ast::Module, file_path: &Path) {
    if !module.peer_files.is_empty() {
        return;
    }
    module.peer_files.push(nova_codegen::ast::PeerFile {
        path: file_path.to_path_buf(),
        file_id: nova_codegen::diag::MAIN_FILE_ID,
        imports: module.imports.clone(),
        items_here: module.items.clone(),
        imported_item_names: std::collections::HashSet::new(),
        is_entry_module: true,
        module_name: module.name.clone(),
    });
}

/// Plan 81 Ф.8.1: построить `SourceMap` из `module.peer_files` для
/// cross-file диагностики. Каждый peer-файл получил уникальный `file_id`
/// в резолвере (`parse_with_file_id`); рендер через `render_with_map`
/// показывает ошибку из импортированного модуля в **правильном** файле
/// со сниппетом (а не byte-offset, применённый к entry-исходнику).
///
/// Источники peer'ов перечитываются с диска — дёшево, вызывается только
/// при наличии ошибок. file_id'ы резолвера сплошные (1..N), регистрация
/// в порядке id совпадает с авто-инкрементом `SourceMap::register`.
fn build_source_map(
    module: &nova_codegen::ast::Module,
    entry_src: &str,
    entry_path: &Path,
) -> nova_codegen::diag::SourceMap {
    let mut map = nova_codegen::diag::SourceMap::new();
    map.register_main(entry_path.to_path_buf(), entry_src.to_string());
    let max_fid = module
        .peer_files
        .iter()
        .map(|p| p.file_id)
        .max()
        .unwrap_or(0);
    for fid in 1..=max_fid {
        let (p, s) = module
            .peer_files
            .iter()
            .find(|pf| pf.file_id == fid)
            .map(|pf| {
                let src = std::fs::read_to_string(&pf.path).unwrap_or_default();
                (pf.path.clone(), src)
            })
            .unwrap_or_else(|| (PathBuf::from("<unknown>"), String::new()));
        // Снять Windows verbatim-префикс `\\?\` (peer-пути канонизированы
        // резолвером) — чтобы диагностика показывала чистый путь.
        let p = {
            let s = p.to_string_lossy();
            match s.strip_prefix(r"\\?\") {
                Some(rest) => PathBuf::from(rest),
                None => p.clone(),
            }
        };
        map.register(p, s);
    }
    map
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

/// Hash `path` to a short hex string — used to make unique tmp subdirs
/// without adding a sha256 crate dependency.
fn path_hash(path: &Path) -> String {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// RAII guard: removes a tmp directory on drop (best-effort, errors ignored).
/// Set `keep = true` to skip cleanup (--keep-artifacts).
struct TmpDirGuard<'a> {
    path: &'a Path,
    keep: bool,
}

impl Drop for TmpDirGuard<'_> {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(self.path);
        }
    }
}

// ---------- subcommand implementations ----------

fn check_module_path(path: &Path, module: &nova_codegen::ast::Module) -> Result<()> {
    nova_codegen::manifest::check_module_path(path, &module.name)
        .map_err(|msg| anyhow!("{}", msg))
}

// Plan 35 R31: resolve_imports_inline extracted в nova_codegen::imports
// для использования из всех 3 pipelines (cmd_check, cmd_build, test_runner).
// Local thin wrapper для backward-compat usage в cmd_build (UsageError tagging).

/// Plan 36 Ф.1 + 36.D: polymorphic path argument + verbosity + filters.
/// Принимает список путей (file-or-dir, рекурсивный walk для dir).
/// Empty → walks parents до nova.toml, использует workspace root.
///
/// Hard-coded skip (override через --include-runtime / --skip):
///   - `target/`, `node_modules/`, `vendor/`, `.git/`, `.hg/`, `.svn/`
///   - directories starting with `_` or `.`
///   - `std/runtime/` (auto-gen, Plan 13)
///
/// Flags:
///   - `--jobs N` parallel workers (0=num_cpus).
///   - `-q`/`--quiet` only failures + summary.
///   - `-v`/`--verbose` extra info per file (timing).
///   - `--list` list files без check.
///   - `--format human|short` (JSON/SARIF/JUnit — sub-plan 36.A).
///   - `--include-runtime` skip std/runtime/ override.
///   - `--skip PATTERN` repeatable substring skip filter.
#[allow(clippy::too_many_arguments)]
fn cmd_check(
    paths: &[PathBuf],
    jobs: usize,
    quiet: bool,
    verbose: bool,
    list_only: bool,
    format: &str,
    include_runtime: bool,
    skip: &[String],
) -> Result<()> {
    // Если пути не указаны — используем workspace root.
    let owned_root;
    let resolved_paths: Vec<PathBuf> = if paths.is_empty() {
        owned_root = find_repo_root()?;
        vec![owned_root.clone()]
    } else {
        paths.iter().cloned().collect()
    };

    // Собираем список .nv файлов: для file — сам файл, для dir — рекурсивный walk.
    let mut files: Vec<PathBuf> = Vec::new();
    for p in &resolved_paths {
        if !p.exists() {
            return Err(usage_err(format!("path not found: {}", p.display())));
        }
        if p.is_file() {
            // Проверка расширения.
            if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                return Err(usage_err(format!("not a Nova source: {}", p.display())));
            }
            files.push(p.clone());
        } else if p.is_dir() {
            let mut found = Vec::new();
            nova_codegen::test_runner::walk_nv(p, &mut found)
                .map_err(|e| anyhow!("walk {}: {}", p.display(), e))?;
            for f in found {
                if !should_skip_path_full(&f, include_runtime, skip) {
                    files.push(f);
                }
            }
        } else {
            return Err(usage_err(format!("path is neither file nor directory: {}", p.display())));
        }
    }

    // Дедуп через canonicalize.
    let mut seen = std::collections::HashSet::new();
    files.retain(|p| {
        match p.canonicalize() {
            Ok(c) => seen.insert(c),
            Err(_) => true,
        }
    });

    // Sort для детерминизма.
    files.sort();

    // --list: show files, no check.
    if list_only {
        for f in &files {
            println!("{}", f.display());
        }
        if !quiet {
            eprintln!("\n{} file(s) would be checked", files.len());
        }
        return Ok(());
    }

    if files.is_empty() {
        if !quiet {
            println!("no .nv files to check");
        }
        return Ok(());
    }

    let n_workers = if jobs == 0 { num_cpus() } else { jobs };
    let total = files.len();
    let start = std::time::Instant::now();

    // Parallel via thread::scope + mpsc.
    let (tx, rx) = std::sync::mpsc::channel::<CheckResult>();
    let files_arc = std::sync::Arc::new(files);

    std::thread::scope(|s| {
        let n_threads = n_workers.min(total).max(1);
        let chunk = (total + n_threads - 1) / n_threads;
        for i in 0..n_threads {
            let start = i * chunk;
            let end = ((i + 1) * chunk).min(total);
            if start >= end {
                continue;
            }
            let files = files_arc.clone();
            let tx = tx.clone();
            s.spawn(move || {
                for f in &files[start..end] {
                    let res = check_one_file(f, verbose);
                    let _ = tx.send(res);
                }
            });
        }
        drop(tx);
    });

    // Aggregate в стабильном file-name order.
    let mut results: Vec<CheckResult> = rx.iter().collect();
    results.sort_by(|a, b| a.file.cmp(&b.file));

    let mut pass = 0;
    let mut fail = 0;
    let mut warn_count = 0;
    for r in &results {
        match &r.error {
            None => {
                pass += 1;
                let timing = if verbose && r.elapsed_ms > 0 {
                    format!(" ({}ms)", r.elapsed_ms)
                } else {
                    String::new()
                };
                let warn_suffix = if r.warnings.is_empty() {
                    String::new()
                } else {
                    format!(" ({} warning(s))", r.warnings.len())
                };
                // -q: skip per-file ok: lines.
                if !quiet {
                    match format {
                        "short" => {
                            // short формат — без декорации, по строке на файл.
                            println!("{}: ok{}{}", r.file.display(), timing, warn_suffix);
                        }
                        _ => {
                            println!("{} {}{}{}",
                                green("ok:"), r.file.display(), timing, warn_suffix);
                        }
                    }
                }
                for w in &r.warnings {
                    if format == "short" {
                        eprintln!("{}", w);
                    } else {
                        eprintln!("  {} {}", bold(&yellow("warning:")), w);
                    }
                    warn_count += 1;
                }
            }
            Some(msg) => {
                fail += 1;
                let timing = if verbose && r.elapsed_ms > 0 {
                    format!(" ({}ms)", r.elapsed_ms)
                } else {
                    String::new()
                };
                if format == "short" {
                    // short: каждая строка ошибки на отдельной строке.
                    for line in msg.lines() {
                        eprintln!("{}: {}", r.file.display(), line);
                    }
                } else {
                    eprintln!("{} {}{}", red("FAIL:"), r.file.display(), timing);
                    for line in msg.lines() {
                        eprintln!("  {}", line);
                    }
                }
                for w in &r.warnings {
                    if format == "short" {
                        eprintln!("{}", w);
                    } else {
                        eprintln!("  {} {}", bold(&yellow("warning:")), w);
                    }
                    warn_count += 1;
                }
            }
        }
    }

    let elapsed = start.elapsed();

    // Summary — всегда (даже в --quiet).
    if format != "short" {
        println!();
        println!("===== SUMMARY =====");
    }
    let timing_suffix = if verbose {
        format!(" ({:.2}s)", elapsed.as_secs_f64())
    } else {
        String::new()
    };
    if fail == 0 {
        if warn_count == 0 {
            println!("{}: {}  FAIL: 0{}", green("PASS"), pass, timing_suffix);
        } else {
            println!("{}: {}  FAIL: 0  WARN: {}{}",
                green("PASS"), pass, warn_count, timing_suffix);
        }
        Ok(())
    } else {
        println!("PASS: {}  {}: {}  WARN: {}{}",
            pass, red("FAIL"), fail, warn_count, timing_suffix);
        Err(anyhow!("{} file(s) failed type-check", fail))
    }
}

#[derive(Debug)]
struct CheckResult {
    file: PathBuf,
    error: Option<String>,
    /// Lint warnings (Plan 36 Ф.0) — non-fatal, отображаются под file
    /// если present. PASS если только warnings (no error).
    warnings: Vec<String>,
    /// Per-file elapsed (milliseconds) — only filled когда verbose=true.
    /// 0 = not measured (Plan 36.D).
    elapsed_ms: u64,
}

/// Single-file check — full pipeline (Plan 36 Ф.0 correctness fix).
/// Phases:
///   1. parse
///   2. check_module_path (D78)
///   3. types::check_module (типы + effects/capability inference embedded)
///   4. types::infer_effects (D28 — effects на private fn)
///   5. lints::lint_module (anonymous-embed override и т.д.)
///
/// До Plan 36 Ф.0 `cmd_check` дёргал только 1-3 — molча пропускал
/// effect-inference и lints, которые `cmd_build` ловит. Это был
/// silent correctness gap.
fn check_one_file(path: &Path, verbose: bool) -> CheckResult {
    let t0 = if verbose { Some(std::time::Instant::now()) } else { None };
    let measure = |t0: Option<std::time::Instant>| -> u64 {
        t0.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0)
    };

    let src = match read_file(path) {
        Ok(s) => s,
        Err(e) => return CheckResult {
            file: path.to_path_buf(),
            error: Some(format!("read: {}", e)),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        },
    };
    let path_str = path.to_string_lossy();

    // 1. parse
    let mut module = match nova_codegen::parser::parse(&src) {
        Ok(m) => m,
        Err(d) => return CheckResult {
            file: path.to_path_buf(),
            error: Some(d.render(&src, &path_str)),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        },
    };

    // 2. check_module_path
    if let Err(e) = check_module_path(path, &module) {
        return CheckResult {
            file: path.to_path_buf(),
            error: Some(format!("{}", e)),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        };
    }

    // 2.5 Plan 35 R31: cross-file resolve через inline expansion.
    // Безопасно для check: type-check merged AST — same correctness как
    // cmd_build. Закрывает Plan 35 R19 (nova check parity).
    // Plan 35 sub-plan 35.A R27: вызываем безусловно (resolver auto-добавит
    // prelude если std/prelude.nv существует, даже без explicit imports).
    {
        if let Ok(repo) = find_repo_root() {
            let paths = resolve_paths(&repo);
            if let Err(e) = nova_codegen::imports::resolve_imports_inline(
                path, &mut module, &repo, &paths.stdlib_dir,
            ) {
                return CheckResult {
                    file: path.to_path_buf(),
                    error: Some(format!("import resolution: {}", e)),
                    warnings: Vec::new(),
                    elapsed_ms: measure(t0),
                };
            }
        }
        // Если find_repo_root() не нашёл nova.toml — silently skip imports
        // (single-file mode без cross-file context).
    }

    // 3. types::check_module
    if let Err(errs) = nova_codegen::types::check_module(&module) {
        // Plan 81 Ф.8.1: cross-file рендер — ошибка из импортированного
        // модуля показывается в правильном файле (через SourceMap по
        // file_id), а не byte-offset'ом в entry-исходнике.
        let smap = build_source_map(&module, &src, path);
        let msgs: Vec<String> =
            errs.iter().map(|d| d.render_with_map(&smap)).collect();
        return CheckResult {
            file: path.to_path_buf(),
            error: Some(msgs.join("\n")),
            warnings: Vec::new(),
            elapsed_ms: measure(t0),
        };
    }

    // 4. infer_effects (D28) — fills in inferred effects on private fn.
    nova_codegen::types::infer_effects(&mut module);

    // 5. lints::lint_module — anonymous-embed override, etc.
    let lint_warnings: Vec<String> = nova_codegen::lints::lint_module(&module)
        .into_iter()
        .map(|w| {
            let (line, col) = nova_codegen::diag::byte_to_line_col(&src, w.diag.span.start);
            format!("{}:{}:{}: {} [{}]", path.display(), line, col, w.diag.message, w.rule)
        })
        .collect();

    CheckResult {
        file: path.to_path_buf(),
        error: None,
        warnings: lint_warnings,
        elapsed_ms: measure(t0),
    }
}

/// Hard-coded skip patterns (Plan 36 R3 minimal version).
/// `include_runtime=true` отключает skip `std/runtime/`.
/// `skip` — user-provided substrings (--skip flag).
fn should_skip_path_full(p: &Path, include_runtime: bool, skip: &[String]) -> bool {
    let s = p.to_string_lossy().replace('\\', "/");
    // User-provided skip patterns first.
    for pat in skip {
        if !pat.is_empty() && s.contains(pat) {
            return true;
        }
    }
    // Skip auto-gen std/runtime/ (unless --include-runtime).
    if !include_runtime && (s.contains("/std/runtime/") || s.starts_with("std/runtime/")) {
        return true;
    }
    // Skip if any component starts with `_` or `.`, or is target/node_modules/vendor.
    for comp in p.components() {
        if let Some(name) = comp.as_os_str().to_str() {
            if name.starts_with('_') || name.starts_with('.') {
                return true;
            }
            if matches!(name, "target" | "node_modules" | "vendor") {
                return true;
            }
        }
    }
    false
}

/// Backward-compat shim (default flags — no override).
#[allow(dead_code)]
fn should_skip_path(p: &Path) -> bool {
    should_skip_path_full(p, false, &[])
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn cmd_run(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    let src = read_file(path)?;
    let path_str = path.to_string_lossy();
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    check_module_path(path, &module)?;
    // Plan 50 Ф.2 (закрытие [M-interp-named]): cross-file resolve через
    // inline expansion — тот же codepath, что в `cmd_build` и
    // `test_runner::codegen_to_c`. Импортированные callee мёрджатся в
    // `module` ДО type-check → `callnorm` ниже видит ВСЕ сигнатуры (в
    // т.ч. дефолты импортированных функций) и раскладывает named args
    // корректно. Раньше `cmd_run` нормализовал только single-file —
    // переставленные named для импортированного callee давали неверный
    // результат в `nova run` ([M-interp-named]).
    //
    // Graceful: если файл вне Nova-проекта (нет nova.toml) — repo не
    // найден, resolve пропускается; single-file без импортов работает
    // и так (prelude auto-import тоже требует repo — поведение
    // консистентно с отсутствием stdlib вне проекта).
    if let Some(repo) = nova_codegen::test_runner::find_repo_root_from(path) {
        let stdlib_dir = repo.join("std");
        nova_codegen::imports::resolve_imports_inline(path, &mut module, &repo, &stdlib_dir)
            .map_err(|e| anyhow!("import resolution: {}", e))?;
    }
    nova_codegen::types::check_module(&module).map_err(|errs| {
        let msgs: Vec<String> = errs
            .iter()
            .map(|d| d.render(&src, &path_str))
            .collect();
        anyhow!("{}", msgs.join("\n"))
    })?;
    // Plan 52 Ф.5: десугаринг map-литералов `[k: v]` → block-expression
    // ПОСЛЕ type-check, ДО callnorm/interp.
    // Plan 52 Ф.7: аннотация inferred K/V для turbofish в десугаринге.
    nova_codegen::types::annotate_map_literals(&mut module);
    nova_codegen::desugar::desugar_module(&mut module);
    // Plan 46 (D102) Ф.2: нормализация call-site для treewalk-interp —
    // named args → positional + вставка defaults. После resolve_imports
    // (нужны все сигнатуры) и type-check, до запуска интерпретатора.
    nova_codegen::callnorm::normalize_module(&mut module);
    let mut interp = nova_codegen::interp::Interpreter::new();
    interp
        .load_module(&module)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    interp
        .run_main()
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    Ok(())
}

/// Plan 45 Ф.12 / D107: `nova doc <file> [--format markdown|json]
/// [--json-schema]`.
///
/// MVP: один входной файл, вывод в stdout. Никаких подкоманд (workspace/
/// --output-dir/--watch — Plan 45.A или отдельные субкоманды позже).
fn cmd_doc(path: &Path, format: &str, json_schema: bool, include_private: bool, run_doc_tests: bool, check: bool, watch: bool, coverage: bool, coverage_threshold: Option<u32>, jobs: usize, scrape_examples: Option<&Path>, strict: bool, mutate_contracts: bool, real_exec: bool, output_dir: Option<&Path>) -> Result<()> {
    // Plan 45 Ф.33.3: load nova.toml [doc] config if present. CLI args override.
    let (final_strict, final_coverage_threshold) =
        apply_doc_config(strict, coverage_threshold);
    let strict = final_strict;
    let coverage_threshold = final_coverage_threshold;
    // `--json-schema` — печатает embedded схему и выходит (D107).
    if json_schema {
        println!("{}", nova_doc_embedded_schema());
        return Ok(());
    }
    // Plan 45 Ф.21.7: workspace-режим. Если path — каталог, рекурсивно
    // парсим все *.nv и строим multi-module DocTree.
    if path.is_dir() {
        // Plan 45 Ф.25.4: mutate_contracts пока single-file only (workspace
        // mode требует cross-module test-runner integration — Ф.25.5).
        return cmd_doc_workspace(path, format, include_private, run_doc_tests, check, coverage, coverage_threshold, jobs, strict, mutate_contracts, real_exec, output_dir);
    }
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    if watch {
        return cmd_doc_watch(path, format, include_private, run_doc_tests, check, coverage, strict);
    }
    let src = read_file(path)?;
    let path_str = path.to_string_lossy();
    let mut module = nova_codegen::parser::parse(&src)
        .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
    check_module_path(path, &module)?;
    // Plan 45 MVP: для single-file mode `nova doc <file>` НЕ резолвим
    // импорты — иначе items из auto-imported std/prelude и других
    // модулей попадают в output. Это даёт "documentation of THIS
    // file" по дефолту. Workspace-режим (`nova doc --workspace`) и
    // multi-module DocTree — Plan 45.A.
    //
    // Без resolve_imports type-check может ругаться на cross-file
    // символы. Для MVP doc-pipeline'а мы прощаем type-check ошибки
    // (но всё ещё парсим — без parse fail нельзя получить AST).
    // Если type-check падает — продолжаем с partial information;
    // production-grade `nova doc --check` (Plan 45 Ф.14) будет
    // делать полный type-check.
    let _ = nova_codegen::types::check_module(&module);
    nova_codegen::types::infer_effects(&mut module);
    // Plan 71 / D127: seed peer_files[0].path для DocModule.source_paths →
    // fixture-detection lints. Без resolve_imports_inline peer_files empty.
    ensure_entry_peer_path(&mut module, path);
    let mut tree = nova_codegen::doc::build(&module);
    // Plan 45 Ф.26.2 / Ф.23.4: populate handler matrix (Effect.handlers).
    nova_codegen::doc::populate_handler_matrix(&mut tree, &src);
    // Plan 45 Ф.22.3 / D107: source_root = parent dir файла, relative
    // от CWD (для portable/deterministic output across machines).
    tree.source_root = path.parent().map(|d| {
        let s = d.display().to_string();
        if s.is_empty() { ".".to_string() } else { s.replace('\\', "/") }
    });
    if !include_private {
        nova_codegen::doc::strip_private(&mut tree);
    }
    // Plan 45 Ф.24.9: --scrape-examples <workspace>
    if let Some(ws) = scrape_examples {
        nova_codegen::doc::scraper::scrape_examples(&mut tree, ws);
    }
    if coverage {
        return cmd_doc_coverage(&tree, coverage_threshold);
    }
    if check {
        let lint_config = build_lint_config_for(path);
        return cmd_doc_check(&tree, format, &lint_config, strict);
    }
    if mutate_contracts {
        return cmd_doc_mutate_contracts(&tree, format, if real_exec { Some(&src) } else { None });
    }
    if run_doc_tests {
        let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(&tree.doc_tests, Some(&src));
        let total = summary.results.len();
        let passed = summary.passed();
        let failed = summary.failed();
        let skipped = summary.skipped();
        for r in &summary.results {
            match &r.outcome {
                nova_codegen::doc::test_runner::DocTestOutcome::Passed => {
                    println!("ok   {}", r.id);
                }
                nova_codegen::doc::test_runner::DocTestOutcome::Failed(msg) => {
                    println!("FAIL {} — {}", r.id, msg);
                }
                nova_codegen::doc::test_runner::DocTestOutcome::Skipped(reason) => {
                    println!("skip {} ({})", r.id, reason);
                }
            }
        }
        println!(
            "\ndoc-tests: {} passed, {} failed, {} skipped (total {})",
            passed, failed, skipped, total
        );
        if !summary.all_passed() {
            std::process::exit(1);
        }
        return Ok(());
    }
    // Plan 45 Ф.31.4: html + --output-dir → multi-page output.
    if format == "html" {
        if let Some(out_dir) = output_dir {
            return write_html_multipage(&tree, out_dir);
        }
    }
    let out = match format {
        "markdown" | "md" => nova_codegen::doc::render_markdown_with_source(&tree, &src),
        "json" => nova_codegen::doc::render_json_with_source(&tree, &src),
        // Plan 45 Ф.31.1: HTML output MVP (single-page, embedded CSS).
        "html" => nova_codegen::doc::render_html(&tree),
        other => {
            return Err(usage_err(format!(
                "unknown --format `{}` (supported: `markdown`, `json`, `html`)",
                other
            )));
        }
    };
    print!("{}", out);
    // Plan 45 Ф.25.1: --strict — fail на любые diagnostic warnings.
    if strict && !tree.warnings.is_empty() {
        eprintln!("\ndoc: {} warning(s) (--strict mode)", tree.warnings.len());
        for w in &tree.warnings {
            eprintln!("  [{}] {}: {}", w.rule, w.item_id, w.message);
        }
        std::process::exit(1);
    }
    Ok(())
}

/// Plan 45 Ф.21.7 — workspace mode. Рекурсивно собирает все *.nv
/// в каталоге, парсит каждый, строит unified DocTree (cross-module
/// intra-doc-links резолвятся). Поддерживает --format / --check /
/// --test / --coverage / --include-private.
fn cmd_doc_workspace(
    dir: &Path,
    format: &str,
    include_private: bool,
    run_doc_tests: bool,
    check: bool,
    coverage: bool,
    coverage_threshold: Option<u32>,
    jobs: usize,
    strict: bool,
    mutate_contracts: bool,
    real_exec: bool,
    output_dir: Option<&Path>,
) -> Result<()> {
    let mut files: Vec<PathBuf> = Vec::new();
    walk_nv_files(dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        bail!("no .nv files found under `{}`", dir.display());
    }

    // Plan 45 Ф.24.13: parallel parse+typecheck via std::thread::scope.
    // Threshold: ≥4 files. Below — sequential (thread overhead not worth it).
    // jobs=0 → auto (logical CPUs); jobs=1 → sequential; jobs=N → N threads.
    const PARALLEL_THRESHOLD: usize = 4;
    let use_parallel = files.len() >= PARALLEL_THRESHOLD && jobs != 1;

    // Result type per file: Ok(source, module) or Err(warning_string).
    type ParseResult = Result<(String, nova_codegen::ast::Module), String>;

    let raw_results: Vec<ParseResult> = if use_parallel {
        let num_threads = if jobs == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(files.len())
        } else {
            jobs.min(files.len())
        };

        // Chunk files into num_threads buckets; each thread processes its chunk
        // sequentially. This avoids spawning one thread per file while still
        // saturating available CPUs.
        let chunk_size = files.len().div_ceil(num_threads);
        let chunks: Vec<&[PathBuf]> = files.chunks(chunk_size).collect();

        // Use a Mutex-wrapped results Vec so threads can push in order-agnostic way.
        // Then we re-sort by original file index to maintain deterministic order.
        let indexed_results: std::sync::Mutex<Vec<(usize, ParseResult)>> =
            std::sync::Mutex::new(Vec::with_capacity(files.len()));

        std::thread::scope(|s| {
            let mut base = 0usize;
            for chunk in &chunks {
                let chunk = *chunk;
                let start_idx = base;
                base += chunk.len();
                let results_ref = &indexed_results;
                s.spawn(move || {
                    for (i, f) in chunk.iter().enumerate() {
                        let file_idx = start_idx + i;
                        let pr = parse_one_file(f);
                        results_ref.lock().unwrap().push((file_idx, pr));
                    }
                });
            }
        });

        // Reconstruct in original file order.
        let mut indexed = indexed_results.into_inner().unwrap();
        indexed.sort_by_key(|(i, _)| *i);
        indexed.into_iter().map(|(_, r)| r).collect()
    } else {
        // Sequential path — identical logic, no threads.
        files.iter().map(|f| parse_one_file(f)).collect()
    };

    // Plan 45 Ф.22.5 / D107: workspace graceful-fail — продолжаем при
    // parse-ошибках в отдельных файлах (как rustdoc), выводим warnings
    // в stderr. Hard-fail только если 0 файлов распарсилось.
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::with_capacity(files.len());
    let mut sources: Vec<String> = Vec::with_capacity(files.len());
    let mut parse_warnings: Vec<String> = Vec::new();

    for pr in raw_results {
        match pr {
            Ok((src, m)) => {
                sources.push(src);
                modules.push(m);
            }
            Err(w) => parse_warnings.push(w),
        }
    }
    for w in &parse_warnings {
        eprintln!("{}", w);
    }
    if modules.is_empty() && !files.is_empty() {
        bail!("все файлы в `{}` содержат ошибки парсинга", dir.display());
    }
    let mut tree = nova_codegen::doc::build_workspace(&modules);
    // Plan 45 Ф.27.1: workspace handler matrix через per-module sources map.
    // Modules и sources Vec'ы parallel'ные (same length, same order), но
    // tree.modules sorted by path. Поэтому строим map module_path → source.
    let sources_by_module: std::collections::BTreeMap<String, String> = modules.iter()
        .zip(sources.iter())
        .map(|(m, s)| (m.name.join("."), s.clone()))
        .collect();
    nova_codegen::doc::populate_handler_matrix_workspace(&mut tree, &sources_by_module);
    // Plan 45 Ф.22.3 / D107: workspace source_root = переданный dir,
    // как есть (relative от CWD для portability).
    tree.source_root = Some(dir.display().to_string().replace('\\', "/"));
    if !include_private {
        nova_codegen::doc::strip_private(&mut tree);
    }
    if coverage {
        return cmd_doc_coverage(&tree, coverage_threshold);
    }
    if check {
        let lint_config = build_lint_config_for(dir);
        return cmd_doc_check(&tree, format, &lint_config, strict);
    }
    if mutate_contracts {
        // Plan 45 Ф.29.4: workspace-mode mutation testing real-exec.
        let report = if real_exec {
            nova_codegen::doc::mutation::run_mutation_analysis_executed_workspace(&tree, &sources_by_module)
        } else {
            nova_codegen::doc::mutation::run_mutation_analysis(&tree)
        };
        return print_mutation_report(&report, format);
    }
    if run_doc_tests {
        // Plan 45 Ф.22.7: workspace doc-tests с crate-scope.
        // Каждый тест получает source своего исходного модуля для
        // crate-scope injection (аналог single-file Ф.21.1).
        // Используем concatenated sources как best-effort fallback:
        // doc-test'ам нужен только scope своего модуля, но merged
        // sources покрывают cross-module cases тоже.
        let combined_source = sources.join("\n\n");
        let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(
            &tree.doc_tests,
            Some(&combined_source),
        );
        return print_doc_test_summary(summary);
    }
    // Plan 45 Ф.31.4: html + --output-dir → multi-page output (workspace mode).
    if format == "html" {
        if let Some(out_dir) = output_dir {
            return write_html_multipage(&tree, out_dir);
        }
    }
    let out = match format {
        "markdown" | "md" => nova_codegen::doc::render_markdown(&tree),
        "json" => nova_codegen::doc::render_json(&tree),
        // Plan 45 Ф.31.1: HTML output MVP в workspace mode.
        "html" => nova_codegen::doc::render_html(&tree),
        other => bail!("unknown --format `{}` (supported: `markdown`, `json`, `html`)", other),
    };
    print!("{}", out);
    // Plan 45 Ф.25.1: --strict — fail на любые diagnostic warnings.
    if strict && !tree.warnings.is_empty() {
        eprintln!("\ndoc: {} warning(s) (--strict mode)", tree.warnings.len());
        for w in &tree.warnings {
            eprintln!("  [{}] {}: {}", w.rule, w.item_id, w.message);
        }
        std::process::exit(1);
    }
    Ok(())
}

fn walk_nv_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            walk_nv_files(&p, out)?;
        } else if p.extension().map(|e| e == "nv").unwrap_or(false) {
            out.push(p);
        }
    }
    Ok(())
}

/// Helper для check-mode (используется и single-file и workspace).
/// Plan 45 Ф.23.12: теперь агрегирует все §11.5 lints.
// Plan 45 Ф.24.4: structured check issue (rule + item_id + message + severity).
struct CheckIssue {
    rule: String,
    item_id: String,
    message: String,
    severity: &'static str,
}

/// Plan 71 / D127: load manifest near `path` (file or dir) and build a
/// `LintConfig` reflecting `enforce-stability` flag and canonical fixture
/// directories. `path` может быть file (нормальный walk-up к nova.toml)
/// или directory (используем sentinel-файл внутри для canonicalize).
///
/// Manifest не найден → default lenient config (strict_stability=false).
fn build_lint_config_for(path: &Path) -> nova_codegen::doc::LintConfig {
    let mut cfg = nova_codegen::doc::LintConfig::default();
    // `find_manifest` ожидает существующий path (canonicalize'ит). Для
    // directory передаём sentinel-файл внутри (он не обязан существовать —
    // canonicalize упадёт и мы fallback'нем к walking parents from dir).
    let manifest = if path.is_dir() {
        // Используем any-file-like sentinel: parent walk from <dir>/anything
        // даст nova.toml в самом <dir> либо выше.
        nova_codegen::manifest::find_manifest(&path.join("__lint_probe__"))
            .or_else(|| {
                // Fallback: walking parents from path itself, ищем nova.toml
                // через manual loop (find_manifest требует canonicalize).
                let mut dir = path.to_path_buf();
                loop {
                    let toml = dir.join("nova.toml");
                    if toml.is_file() {
                        return nova_codegen::manifest::parse_manifest(&toml, &dir);
                    }
                    if !dir.pop() {
                        return None;
                    }
                }
            })
    } else {
        nova_codegen::manifest::find_manifest(path)
    };
    if let Some(m) = manifest {
        cfg.strict_stability = m.enforce_stability;
    }
    cfg
}

fn cmd_doc_check(
    tree: &nova_codegen::doc::DocTree,
    format: &str,
    lint_config: &nova_codegen::doc::LintConfig,
    strict: bool,
) -> Result<()> {
    let mut issues: Vec<CheckIssue> = Vec::new();
    // Broken links.
    for link in &tree.links {
        if link.target_id.is_none() {
            let from = link.from_id.as_deref().unwrap_or("<module>").to_string();
            issues.push(CheckIssue {
                rule: "broken-link".to_string(),
                item_id: from,
                message: format!("broken link [{}]", link.text),
                severity: "error",
            });
        }
    }
    // Missing summaries.
    for m in &tree.modules {
        for it in &m.items {
            if it.visibility == nova_codegen::doc::Visibility::Export && it.summary.is_none() {
                issues.push(CheckIssue {
                    rule: "missing-summary".to_string(),
                    item_id: it.id.clone(),
                    message: "exported item has no summary".to_string(),
                    severity: "error",
                });
            }
        }
    }
    // Lint violations. Plan 71 / D127: severity-aware — `public-missing-stability`
    // emit'ится с Warning по default; Error только под `enforce-stability = true`.
    for v in nova_codegen::doc::run_lints(tree, lint_config) {
        issues.push(CheckIssue {
            rule: v.rule.to_string(),
            item_id: v.item_id,
            message: v.message,
            severity: v.severity.as_str(),
        });
    }

    // Plan 45 Ф.25.1: doc-collection warnings (malformed attrs, ambiguous links,
    // unknown modifiers). Severity = warning — `--check` сам по себе не fail'ит
    // на warnings; `--strict` поверх него — fail. CI выбирает policy.
    for w in &tree.warnings {
        issues.push(CheckIssue {
            rule: w.rule.clone(),
            item_id: w.item_id.clone(),
            message: w.message.clone(),
            severity: "warning",
        });
    }

    // Plan 71 / D127: `--strict` эскалирует warnings до errors для CI gate'а.
    // Без `--strict` только true-error severity блокирует exit 1.
    let has_errors = issues
        .iter()
        .any(|i| i.severity == "error" || (strict && i.severity == "warning"));

    if format == "json" {
        // Plan 45 Ф.24.4: structured JSON output for CI.
        let mut by_rule: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for i in &issues {
            *by_rule.entry(i.rule.clone()).or_insert(0) += 1;
        }
        let by_rule_str = by_rule
            .iter()
            .map(|(k, v)| format!("    {:?}: {}", k, v))
            .collect::<Vec<_>>()
            .join(",\n");
        let issues_str = issues
            .iter()
            .map(|i| format!(
                "    {{\"rule\": {:?}, \"item_id\": {:?}, \"message\": {:?}, \"severity\": {:?}}}",
                i.rule, i.item_id, i.message, i.severity
            ))
            .collect::<Vec<_>>()
            .join(",\n");
        println!("{{\n  \"summary\": {{\n    \"count\": {},\n    \"by_rule\": {{\n{}\n    }}\n  }},\n  \"issues\": [\n{}\n  ]\n}}",
            issues.len(), by_rule_str, issues_str);
    } else {
        if issues.is_empty() {
            println!("doc-check: ok ({} item(s), {} link(s))",
                tree.modules.iter().map(|m| m.items.len()).sum::<usize>(),
                tree.links.len());
            return Ok(());
        }
        for i in &issues {
            eprintln!("doc-check: [{}] {}: {}", i.rule, i.item_id, i.message);
        }
        eprintln!("\ndoc-check: {} issue(s)", issues.len());
    }

    if has_errors {
        std::process::exit(1);
    }
    Ok(())
}

/// Plan 45 Ф.25.4 + Ф.28.2 — `nova doc --mutate-contracts [--real-exec]` mutation testing.
fn cmd_doc_mutate_contracts(
    tree: &nova_codegen::doc::DocTree,
    format: &str,
    source: Option<&str>,
) -> Result<()> {
    let report = match source {
        Some(src) => nova_codegen::doc::mutation::run_mutation_analysis_executed(tree, src),
        None => nova_codegen::doc::mutation::run_mutation_analysis(tree),
    };
    print_mutation_report(&report, format)
}

/// Plan 45 Ф.32.1 + Ф.32.2 — query doc output via DSL.
///
/// Accepts both:
/// - `.nv` source: re-parses + builds DocTree + queries (slower, fresh data).
/// - `.json` file: parses pre-generated `nova doc --format json` output
///   via custom JSON parser, executes query на parsed items array (fast,
///   no Nova compilation needed).
fn cmd_doc_query(path: &Path, query_str: &str) -> Result<()> {
    let q = nova_codegen::doc::query::parse_query(query_str)
        .map_err(|e| anyhow!("query parse error: {}", e))?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "nv" {
        let src = read_file(path)?;
        let path_str = path.to_string_lossy();
        let mut module = nova_codegen::parser::parse(&src)
            .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
        let _ = nova_codegen::types::check_module(&module);
        nova_codegen::types::infer_effects(&mut module);
        let tree = nova_codegen::doc::build(&module);
        let results = nova_codegen::doc::query::execute(&tree, &q);
        print!("{}", nova_codegen::doc::query::render_results_json(&results));
        Ok(())
    } else if ext == "json" {
        // Plan 45 Ф.32.2: parse pre-generated JSON, query directly on items array.
        let content = read_file(path)?;
        let json = nova_codegen::doc::json_parse::parse(&content)
            .map_err(|e| anyhow!("JSON parse error in {}: {}", path.display(), e))?;
        let results = nova_codegen::doc::query::execute_json(&json, &q);
        print!("{}", nova_codegen::doc::query::render_results_json(&results));
        Ok(())
    } else {
        bail!("unknown file extension `{}` — expected .nv or .json", ext);
    }
}

/// Plan 45 Ф.33.3 — load nova.toml [doc] config, apply to env, merge с CLI args.
///
/// Returns (effective_strict, effective_coverage_threshold). CLI values take
/// priority когда explicitly set; config fills defaults.
///
/// Lookup: walk up от CWD до root looking for `nova.toml`. If found, parse.
/// If not found OR parse fails, use defaults (no error — config optional).
fn apply_doc_config(cli_strict: bool, cli_coverage_threshold: Option<u32>) -> (bool, Option<u32>) {
    let cfg = match load_nova_toml_doc_config() {
        Some(c) => c,
        None => return (cli_strict, cli_coverage_threshold),
    };
    // Apply env vars (для downstream readers).
    cfg.apply_env();
    // CLI overrides config: если CLI flag set — use it, иначе config value.
    let strict = cli_strict || cfg.strict;
    let coverage_threshold = cli_coverage_threshold.or(cfg.coverage_threshold);
    (strict, coverage_threshold)
}

fn load_nova_toml_doc_config() -> Option<nova_codegen::doc::config::DocConfig> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..16 { // safety: max 16 parent walks
        let candidate = dir.join("nova.toml");
        if candidate.is_file() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                if let Ok(cfg) = nova_codegen::doc::config::DocConfig::from_toml_str(&content) {
                    return Some(cfg);
                }
            }
            // Found but parse failed — bail (don't keep walking).
            return None;
        }
        if !dir.pop() { break; }
    }
    None
}

/// Plan 45 Ф.32.3 + Ф.34.1 — `nova doc-mcp <file>` MCP server.
/// Default — stdio JSON-RPC loop. With `--port N` — HTTP server on 127.0.0.1:N.
fn cmd_doc_mcp(path: &Path, port: Option<u16>) -> Result<()> {
    // Load tree JSON: .nv → build + render_json, .json → read directly.
    let tree_json_str = match path.extension().and_then(|e| e.to_str()) {
        Some("nv") => {
            let src = read_file(path)?;
            let path_str = path.to_string_lossy();
            let mut module = nova_codegen::parser::parse(&src)
                .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?;
            let _ = nova_codegen::types::check_module(&module);
            nova_codegen::types::infer_effects(&mut module);
            let tree = nova_codegen::doc::build(&module);
            nova_codegen::doc::render_json_with_source(&tree, &src)
        }
        Some("json") => read_file(path)?,
        Some(other) => bail!("unknown file extension `.{}` — expected .nv or .json", other),
        None => bail!("file has no extension — expected .nv or .json"),
    };
    let tree_json = nova_codegen::doc::json_parse::parse(&tree_json_str)
        .map_err(|e| anyhow!("JSON parse: {}", e))?;
    eprintln!("nova doc-mcp: loaded doc tree from {}", path.display());
    match port {
        Some(p) => {
            // Plan 45 Ф.34.1: HTTP server mode.
            nova_codegen::doc::mcp::run_http_server(&tree_json, p)?;
        }
        None => {
            // Default: stdio JSON-RPC loop.
            eprintln!("nova doc-mcp: ready (read JSON-RPC on stdin)");
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            nova_codegen::doc::mcp::run_mcp_loop(&tree_json, stdin.lock(), stdout.lock())?;
        }
    }
    Ok(())
}

/// Plan 45 Ф.31.4 — write multi-page HTML output to directory.
/// Creates directory if not exists. Writes each `(filename, html)` pair.
fn write_html_multipage(tree: &nova_codegen::doc::DocTree, out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir)?;
    let pages = nova_codegen::doc::render_html_multipage(tree);
    let mut count = 0;
    for (filename, content) in &pages {
        let target = out_dir.join(filename);
        std::fs::write(&target, content)?;
        count += 1;
    }
    eprintln!("nova doc --format html --output-dir: wrote {} file(s) to {}",
        count, out_dir.display());
    Ok(())
}

/// Plan 45 Ф.29.4 — shared mutation report printer (single-file + workspace).
fn print_mutation_report(
    report: &nova_codegen::doc::mutation::MutationReport,
    format: &str,
) -> Result<()> {
    if format == "json" {
        let mutants_str = report.mutants.iter()
            .map(|m| format!(
                "    {{\"item_id\": {:?}, \"contract_kind\": {:?}, \"operator\": {:?}, \"original_expr\": {:?}, \"mutated_expr\": {:?}, \"outcome\": {:?}}}",
                m.item_id, m.contract_kind, m.operator, m.original_expr, m.mutated_expr, m.outcome.as_str()
            ))
            .collect::<Vec<_>>()
            .join(",\n");
        println!("{{\n  \"summary\": {{\n    \"total\": {},\n    \"killed\": {},\n    \"survived\": {}\n  }},\n  \"mutants\": [\n{}\n  ]\n}}",
            report.total, report.killed, report.survived, mutants_str);
    } else {
        println!("doc-mutate: {} mutant(s) — killed {}, survived {}",
            report.total, report.killed, report.survived);
        if report.total == 0 {
            println!("  (no functions with contracts found — nothing to mutate)");
            return Ok(());
        }
        for m in &report.mutants {
            let symbol = match m.outcome {
                nova_codegen::doc::mutation::MutantOutcome::Killed => "[killed]",
                nova_codegen::doc::mutation::MutantOutcome::Survived => "[SURVIVED]",
                nova_codegen::doc::mutation::MutantOutcome::NoTests => "[no-tests]",
            };
            println!("  {} {}: {} `{}` → `{}` ({})",
                symbol, m.item_id, m.contract_kind, m.original_expr, m.mutated_expr, m.operator);
        }
        if report.survived > 0 {
            eprintln!("\n  {} mutant(s) survived — contracts may be under-tested",
                report.survived);
        }
    }
    if report.survived > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Helper: print doc-test summary + exit 1 on failures.
fn print_doc_test_summary(summary: nova_codegen::doc::test_runner::DocTestSummary) -> Result<()> {
    let total = summary.results.len();
    let passed = summary.passed();
    let failed = summary.failed();
    let skipped = summary.skipped();
    for r in &summary.results {
        match &r.outcome {
            nova_codegen::doc::test_runner::DocTestOutcome::Passed => println!("ok   {}", r.id),
            nova_codegen::doc::test_runner::DocTestOutcome::Failed(msg) => println!("FAIL {} — {}", r.id, msg),
            nova_codegen::doc::test_runner::DocTestOutcome::Skipped(reason) => println!("skip {} ({})", r.id, reason),
        }
    }
    println!("\ndoc-tests: {} passed, {} failed, {} skipped (total {})", passed, failed, skipped, total);
    if !summary.all_passed() {
        std::process::exit(1);
    }
    Ok(())
}

/// Plan 45 Ф.21.6 / D105: doc-coverage метрика. Считает items
/// (всего/задокументированных) и links (всего/broken). Output на
/// stdout, exit code = процент-непокрытых (0 если 100% покрыто).
///
/// Plan 45 Ф.33.2: optional `threshold` — если задан и actual coverage %
/// < threshold, exit 1 (CI gate). 0 если threshold None или satisfied.
fn cmd_doc_coverage(tree: &nova_codegen::doc::DocTree, threshold: Option<u32>) -> Result<()> {
    use std::collections::BTreeMap;
    let mut total: BTreeMap<&str, usize> = BTreeMap::new();
    let mut documented: BTreeMap<&str, usize> = BTreeMap::new();
    let mut with_examples: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &tree.modules {
        for it in &m.items {
            let kind: &str = match &it.kind {
                nova_codegen::doc::ItemKind::Fn(_) => "fn",
                nova_codegen::doc::ItemKind::Type(_) => "type",
                nova_codegen::doc::ItemKind::Const { .. } => "const",
                nova_codegen::doc::ItemKind::Effect { .. } => "effect",
                nova_codegen::doc::ItemKind::Protocol { .. } => "protocol",
                nova_codegen::doc::ItemKind::ReExport { .. } => "reexport",
            };
            *total.entry(kind).or_insert(0) += 1;
            if it.summary.is_some() {
                *documented.entry(kind).or_insert(0) += 1;
            }
            // Plan 45 Ф.23.19: track items with Examples section or doc-tests.
            let has_examples = it.sections.contains_key("examples")
                || tree.doc_tests.iter().any(|t| t.from_id.as_deref() == Some(&it.id));
            if has_examples {
                *with_examples.entry(kind).or_insert(0) += 1;
            }
        }
    }
    let total_items: usize = total.values().sum();
    let documented_items: usize = documented.values().sum();
    let total_with_examples: usize = with_examples.values().sum();
    let total_links = tree.links.len();
    let broken_links = tree.links.iter().filter(|l| l.target_id.is_none()).count();

    println!("doc-coverage:");
    println!("  items: {}/{} documented ({:.1}%)",
        documented_items, total_items,
        if total_items == 0 { 100.0 } else { 100.0 * documented_items as f64 / total_items as f64 });
    println!("  examples: {}/{} with examples ({:.1}%)",
        total_with_examples, total_items,
        if total_items == 0 { 100.0 } else { 100.0 * total_with_examples as f64 / total_items as f64 });
    for (kind, &total_n) in &total {
        let doc_n = documented.get(kind).copied().unwrap_or(0);
        let ex_n = with_examples.get(kind).copied().unwrap_or(0);
        println!("    {}: {}/{} documented, {}/{} with examples", kind, doc_n, total_n, ex_n, total_n);
    }
    println!("  links: {}/{} resolved ({} broken)",
        total_links - broken_links, total_links, broken_links);
    // Plan 45 Ф.33.2: CI gate.
    if let Some(min) = threshold {
        let pct = if total_items == 0 { 100.0 } else {
            100.0 * documented_items as f64 / total_items as f64
        };
        if (pct as u32) < min {
            eprintln!("\nERROR: documentation coverage {:.1}% < threshold {}%",
                pct, min);
            std::process::exit(1);
        }
        println!("\nOK: documentation coverage {:.1}% >= threshold {}%", pct, min);
    }
    Ok(())
}

/// Plan 45 Ф.15: watch mode — re-render при изменении `path` (mtime
/// poll каждые 500ms). Без `notify` dep'ы для minimal footprint.
/// Ctrl-C завершает loop.
fn cmd_doc_watch(
    path: &Path,
    format: &str,
    include_private: bool,
    run_doc_tests: bool,
    check: bool,
    coverage: bool,
    strict: bool,
) -> Result<()> {
    // Plan 45 Ф.34.2: use WatchCache для incremental parse.
    let mut cache = nova_codegen::doc::watch_cache::WatchCache::new();
    let mut last_mtime: Option<std::time::SystemTime> = None;
    eprintln!("nova doc --watch: monitoring {} (Ctrl-C to exit)", path.display());
    loop {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if mtime != last_mtime {
            last_mtime = mtime;
            // Очистка экрана + cursor home (ANSI). Сохраняем scrollback.
            eprint!("\x1b[2J\x1b[H");
            eprintln!(
                "─── nova doc --watch ({}) ───",
                chrono_like_now()
            );
            // Plan 45 Ф.34.2: parse через cache (re-uses unchanged AST).
            match cache.parse_with_cache(path) {
                Ok((module_arc, src, outcome)) => {
                    let tag = match outcome {
                        nova_codegen::doc::watch_cache::CacheOutcome::Miss => "parsed",
                        nova_codegen::doc::watch_cache::CacheOutcome::Hit => "cached",
                        nova_codegen::doc::watch_cache::CacheOutcome::Stale => "re-parsed",
                    };
                    eprintln!("({})", tag);
                    let mut tree = nova_codegen::doc::build(&*module_arc);
                    nova_codegen::doc::populate_handler_matrix(&mut tree, &src);
                    tree.source_root = path.parent().map(|d| {
                        let s = d.display().to_string();
                        if s.is_empty() { ".".to_string() } else { s.replace('\\', "/") }
                    });
                    if !include_private {
                        nova_codegen::doc::strip_private(&mut tree);
                    }
                    if coverage {
                        let _ = cmd_doc_coverage(&tree, None);
                    } else if check {
                        let lint_config = build_lint_config_for(path);
                        let _ = cmd_doc_check(&tree, format, &lint_config, strict);
                    } else if run_doc_tests {
                        let summary = nova_codegen::doc::test_runner::run_doc_tests_with_source(&tree.doc_tests, Some(&src));
                        let _ = print_doc_test_summary(summary);
                    } else {
                        let out = match format {
                            "markdown" | "md" => nova_codegen::doc::render_markdown_with_source(&tree, &src),
                            "json" => nova_codegen::doc::render_json_with_source(&tree, &src),
                            "html" => nova_codegen::doc::render_html(&tree),
                            other => { eprintln!("unknown --format `{}`", other); continue; }
                        };
                        print!("{}", out);
                    }
                }
                Err(e) => eprintln!("error: {}", e),
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// MVP timestamp для watch-header'а. Без chrono dep'ы — простая
/// HH:MM:SS из SystemTime (UTC).
fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let rem = now % 86_400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    format!("{:02}:{:02}:{:02} UTC", h, m, s)
}

// ── Plan 45 Ф.24.10: semver diff ─────────────────────────────────────────────

/// Severity of a doc API change.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum DiffSeverity {
    Patch,  // docs-only: summary/description changed, signature same
    Minor,  // added public item or new export
    Major,  // removed item, changed signature, changed kind
}

impl DiffSeverity {
    fn label(&self) -> &'static str {
        match self {
            DiffSeverity::Patch => "patch",
            DiffSeverity::Minor => "minor",
            DiffSeverity::Major => "major",
        }
    }
    /// CLI exit code per severity level.
    /// 0 = no changes, 1 = major, 2 = minor, 3 = patch.
    fn exit_code(max: Option<DiffSeverity>) -> u8 {
        match max {
            None => 0,
            Some(DiffSeverity::Major) => 1,
            Some(DiffSeverity::Minor) => 2,
            Some(DiffSeverity::Patch) => 3,
        }
    }
}

struct DiffChange {
    severity: DiffSeverity,
    item_id: String,
    description: String,
}

/// Plan 45 Ф.24.10: structural diff of two `nova doc --format json` outputs.
/// Compares items by stable ID, classifies changes, exits with severity code.
fn cmd_doc_diff(old_path: &Path, new_path: &Path) -> Result<()> {
    let old_src = std::fs::read_to_string(old_path)
        .map_err(|e| anyhow!("cannot read {}: {}", old_path.display(), e))?;
    let new_src = std::fs::read_to_string(new_path)
        .map_err(|e| anyhow!("cannot read {}: {}", new_path.display(), e))?;

    let old_json: serde_json::Value = serde_json::from_str(&old_src)
        .map_err(|e| anyhow!("{}: invalid JSON: {}", old_path.display(), e))?;
    let new_json: serde_json::Value = serde_json::from_str(&new_src)
        .map_err(|e| anyhow!("{}: invalid JSON: {}", new_path.display(), e))?;

    // Build id → item maps from "items" array.
    let old_items = collect_items_by_id(&old_json);
    let new_items = collect_items_by_id(&new_json);

    let mut changes: Vec<DiffChange> = Vec::new();

    // 1. Removed items (major).
    for (id, old_item) in &old_items {
        if is_public(old_item) && !new_items.contains_key(id.as_str()) {
            changes.push(DiffChange {
                severity: DiffSeverity::Major,
                item_id: id.clone(),
                description: "removed public item".to_string(),
            });
        }
    }

    // 2. Added items (minor).
    for (id, new_item) in &new_items {
        if is_public(new_item) && !old_items.contains_key(id.as_str()) {
            changes.push(DiffChange {
                severity: DiffSeverity::Minor,
                item_id: id.clone(),
                description: "added public item".to_string(),
            });
        }
    }

    // 3. Changed items.
    for (id, old_item) in &old_items {
        if let Some(new_item) = new_items.get(id.as_str()) {
            if !is_public(old_item) && !is_public(new_item) {
                continue; // private → private, skip
            }
            // visibility change: private→public (minor) or public→private (major)
            if is_public(old_item) != is_public(new_item) {
                let sev = if is_public(old_item) {
                    DiffSeverity::Major // was public, now private → breaking
                } else {
                    DiffSeverity::Minor // was private, now public → additive
                };
                changes.push(DiffChange {
                    severity: sev,
                    item_id: id.clone(),
                    description: if is_public(old_item) {
                        "item became private (was public)".to_string()
                    } else {
                        "item became public (was private)".to_string()
                    },
                });
                continue;
            }
            // kind change → major
            let old_kind = old_item.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let new_kind = new_item.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            if old_kind != new_kind {
                changes.push(DiffChange {
                    severity: DiffSeverity::Major,
                    item_id: id.clone(),
                    description: format!("kind changed {} → {}", old_kind, new_kind),
                });
                continue;
            }
            // signature change → major (compare signature sub-object as JSON)
            let old_sig = old_item.get("signature");
            let new_sig = new_item.get("signature");
            if signature_changed(old_sig, new_sig) {
                changes.push(DiffChange {
                    severity: DiffSeverity::Major,
                    item_id: id.clone(),
                    description: "signature changed".to_string(),
                });
                continue;
            }
            // docs-only change → patch
            let old_summary = old_item.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let new_summary = new_item.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let old_desc = old_item.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let new_desc = new_item.get("description").and_then(|v| v.as_str()).unwrap_or("");
            if old_summary != new_summary || old_desc != new_desc {
                changes.push(DiffChange {
                    severity: DiffSeverity::Patch,
                    item_id: id.clone(),
                    description: "documentation changed".to_string(),
                });
            }
        }
    }

    // Sort: major first, then minor, then patch; within each — by id.
    changes.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.item_id.cmp(&b.item_id)));

    let max_severity = changes.iter().map(|c| c.severity).max();

    // Output.
    if changes.is_empty() {
        println!("doc-diff: no changes between {} and {}", old_path.display(), new_path.display());
    } else {
        println!("doc-diff: {} change(s) [{} max severity]",
            changes.len(),
            max_severity.map(|s| s.label()).unwrap_or("none"));
        for c in &changes {
            println!("  [{}] {}: {}", c.severity.label(), c.item_id, c.description);
        }
    }

    let code = DiffSeverity::exit_code(max_severity);
    if code != 0 {
        std::process::exit(code as i32);
    }
    Ok(())
}

/// Build a map of item_id → &Value from `items` array in a doc JSON output.
fn collect_items_by_id(doc: &serde_json::Value) -> std::collections::BTreeMap<String, &serde_json::Value> {
    let mut map = std::collections::BTreeMap::new();
    if let Some(arr) = doc.get("items").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                map.insert(id.to_string(), item);
            }
        }
    }
    map
}

fn is_public(item: &serde_json::Value) -> bool {
    item.get("visibility").and_then(|v| v.as_str()) == Some("export")
}

/// Signature comparison: compare params array + return_type + effects.
/// Ignores line/column source info and verify_status (runtime result, not API).
fn signature_changed(old_sig: Option<&serde_json::Value>, new_sig: Option<&serde_json::Value>) -> bool {
    match (old_sig, new_sig) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(o), Some(n)) => {
            // Compare structural fields relevant to API compat.
            let fields = ["params", "return_type", "effects", "receiver"];
            for f in fields {
                if o.get(f) != n.get(f) {
                    return true;
                }
            }
            false
        }
    }
}

/// Embedded JSON Schema (D107) — пока минимальная заглушка с
/// корректным `format_version` дискриминатором. Полная схема —
/// Plan 45 Ф.9 (schema.rs); этот placeholder уже валидный JSON Schema
/// 2020-12, описывающий обязательный `format_version: u32` (1) и
/// высокоуровневый shape.
fn nova_doc_embedded_schema() -> &'static str {
    nova_codegen::doc::schema::schema_v1()
}

/// Plan 03.1 Ф.5: каталог Nova-пакета, в котором запущена команда —
/// ближайший вверх от cwd `nova.toml`. `None` — не внутри пакета.
fn package_dir_from_cwd() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join("nova.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Plan 03.1 Ф.5: вставить запись `name = <value>` в секцию
/// `[dependencies]` текста `nova.toml`. Если секции нет — создаётся в
/// конце файла. Дубль имени → ошибка.
fn insert_dependency(text: &str, name: &str, value: &str) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut dep_header: Option<usize> = None;
    let mut in_deps = false;
    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]";
            if in_deps {
                dep_header = Some(i);
            }
            continue;
        }
        if in_deps {
            if let Some((k, _)) = t.split_once('=') {
                if k.trim() == name {
                    return Err(usage_err(format!(
                        "зависимость `{}` уже объявлена в [dependencies] — \
                         правьте nova.toml вручную либо используйте `nova update`",
                        name,
                    )));
                }
            }
        }
    }
    let new_line = format!("{} = {}", name, value);
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    match dep_header {
        Some(idx) => out.insert(idx + 1, new_line),
        None => {
            if out.last().map(|l| !l.is_empty()).unwrap_or(false) {
                out.push(String::new());
            }
            out.push("[dependencies]".to_string());
            out.push(new_line);
        }
    }
    let mut result = out.join("\n");
    result.push('\n');
    Ok(result)
}

/// Plan 03.1 Ф.5: `nova add` — добавить зависимость в `nova.toml`
/// текущего пакета и обновить `nova.lock`.
fn cmd_add(
    name: &str,
    path: Option<&str>,
    git: Option<&str>,
    tag: Option<&str>,
    branch: Option<&str>,
    rev: Option<&str>,
    version: Option<&str>,
) -> Result<()> {
    let pkg_dir = package_dir_from_cwd().ok_or_else(|| {
        usage_err("`nova add` запускается внутри Nova-пакета (нет nova.toml)")
    })?;
    let toml_path = pkg_dir.join("nova.toml");
    // Только пакет (с `[package]`), не голый workspace-манифест.
    if nova_codegen::manifest::parse_manifest(&toml_path, &pkg_dir).is_none() {
        return Err(usage_err(format!(
            "{} не содержит секции [package] — `nova add` правит пакет, \
             не workspace",
            toml_path.display(),
        )));
    }

    let value = match (path, git) {
        (Some(p), None) => format!("{{ path = \"{}\" }}", p),
        (None, Some(url)) => {
            let pin = match (tag, branch, rev, version) {
                (Some(t), _, _, _) => format!(", tag = \"{}\"", t),
                (_, Some(b), _, _) => format!(", branch = \"{}\"", b),
                (_, _, Some(r), _) => format!(", rev = \"{}\"", r),
                (_, _, _, Some(v)) => format!(", version = \"{}\"", v),
                _ => String::new(),
            };
            format!("{{ git = \"{}\"{} }}", url, pin)
        }
        (None, None) => {
            return Err(usage_err(
                "укажите источник зависимости: --path <DIR> либо --git <URL>",
            ))
        }
        (Some(_), Some(_)) => unreachable!("clap conflicts_with path/git"),
    };

    let text = read_file(&toml_path)?;
    let updated = insert_dependency(&text, name, &value)?;
    std::fs::write(&toml_path, &updated)
        .map_err(|e| anyhow!("запись {}: {}", toml_path.display(), e))?;
    println!(
        "{} `{}` → {}",
        green("added:"),
        name,
        toml_path.display(),
    );

    // Обновить nova.lock (материализует git-зависимость, фиксирует commit).
    nova_codegen::lockfile::sync(&pkg_dir)
        .map_err(|e| anyhow!("nova.lock не обновлён: {}", e))?;
    println!("{} nova.lock обновлён", green("locked:"));
    Ok(())
}

/// Plan 03.1 Ф.5: `nova update` — пере-резолвить git-пины и обновить
/// `nova.lock`.
fn cmd_update(name: Option<&str>, precise: Option<&str>) -> Result<()> {
    let pkg_dir = package_dir_from_cwd().ok_or_else(|| {
        usage_err("`nova update` запускается внутри Nova-пакета (нет nova.toml)")
    })?;
    let toml_path = pkg_dir.join("nova.toml");
    let manifest = nova_codegen::manifest::parse_manifest(&toml_path, &pkg_dir)
        .ok_or_else(|| {
            usage_err(format!(
                "{} не содержит секции [package]",
                toml_path.display(),
            ))
        })?;

    // `--precise NAME@VERSION` — зафиксировать точную версию git-deps.
    if let Some(spec) = precise {
        let (pname, vstr) = spec.rsplit_once('@').ok_or_else(|| {
            usage_err(format!(
                "--precise: ожидается формат NAME@VERSION, получено `{}`",
                spec,
            ))
        })?;
        let version = nova_codegen::semver::Version::parse(vstr)
            .map_err(|e| usage_err(format!("--precise: {}", e)))?;
        let dep = manifest
            .dependencies
            .iter()
            .find(|d| d.name == pname)
            .ok_or_else(|| {
                usage_err(format!(
                    "зависимость `{}` не объявлена в [dependencies]",
                    pname,
                ))
            })?;
        let url = match &dep.source {
            nova_codegen::manifest::DepSource::Git { url, .. } => url.clone(),
            _ => {
                return Err(usage_err(format!(
                    "--precise применим только к git-зависимости (`{}` — нет)",
                    pname,
                )))
            }
        };
        nova_codegen::lockfile::update_precise(&pkg_dir, pname, &url, &version)
            .map_err(|e| anyhow!("обновление зависимостей: {}", e))?;
        println!(
            "{} `{}` зафиксирован на версии {}",
            green("updated:"),
            pname,
            version,
        );
        return Ok(());
    }

    // Если указано имя — проверить, что это объявленная git-зависимость.
    if let Some(n) = name {
        match manifest.dependencies.iter().find(|d| d.name == n) {
            None => {
                return Err(usage_err(format!(
                    "зависимость `{}` не объявлена в [dependencies]",
                    n,
                )))
            }
            Some(d) => {
                if !matches!(d.source, nova_codegen::manifest::DepSource::Git { .. }) {
                    return Err(usage_err(format!(
                        "зависимость `{}` не git — у path-зависимостей нет \
                         пина, обновлять нечего",
                        n,
                    )));
                }
            }
        }
    }
    let graph = nova_codegen::lockfile::update(&pkg_dir, name)
        .map_err(|e| anyhow!("обновление зависимостей: {}", e))?;
    match name {
        Some(n) => println!("{} git-пин `{}` пере-резолвлен", green("updated:"), n),
        None => println!(
            "{} git-пины пере-резолвлены ({} зависимост(и) в графе)",
            green("updated:"),
            graph.len(),
        ),
    }
    Ok(())
}

/// Plan 03.4: резолвит цель `nova info` (путь либо имя зависимости) в
/// effect-surface публичного API + имя пакета.
fn info_surface(
    target: &str,
) -> Result<(nova_codegen::effect_surface::EffectSurface, String)> {
    // --- резолв цели в каталог/файл пакета ---------------------------
    let path = Path::new(target);
    let (pkg_root, default_name): (PathBuf, String) = if path.exists() {
        let p = path
            .canonicalize()
            .map_err(|e| usage_err(format!("{}: {}", target, e)))?;
        let name = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(target)
            .to_string();
        (p, name)
    } else {
        // Не путь → имя зависимости текущего пакета.
        let pkg_dir = package_dir_from_cwd().ok_or_else(|| {
            usage_err(format!(
                "`{}` — не путь и не внутри Nova-пакета (нет nova.toml для \
                 поиска зависимости)",
                target,
            ))
        })?;
        let manifest = nova_codegen::manifest::parse_manifest(
            &pkg_dir.join("nova.toml"),
            &pkg_dir,
        )
        .ok_or_else(|| usage_err("nova.toml без секции [package]"))?;
        let dep = manifest
            .dependencies
            .iter()
            .find(|d| d.name == target)
            .ok_or_else(|| {
                usage_err(format!(
                    "`{}` — не путь и не объявленная зависимость",
                    target,
                ))
            })?;
        let dir = match &dep.source {
            nova_codegen::manifest::DepSource::Path(rel) => pkg_dir.join(rel),
            nova_codegen::manifest::DepSource::Git { url, pin } => {
                nova_codegen::git_cache::resolve_git_dep(url, pin, None)
                    .map_err(|e| anyhow!("git-зависимость `{}`: {}", target, e))?
                    .checkout
            }
            _ => {
                return Err(usage_err(format!(
                    "зависимость `{}` — registry-версия; `nova info` для \
                     registry появится с Plan 03.3",
                    target,
                )))
            }
        };
        (dir, target.to_string())
    };

    // --- собрать .nv-файлы пакета ------------------------------------
    let mut files: Vec<PathBuf> = Vec::new();
    if pkg_root.is_file() {
        files.push(pkg_root.clone());
    } else {
        walk_nv_files(&pkg_root, &mut files)?;
    }
    if files.is_empty() {
        bail!("в `{}` не найдено .nv-файлов", pkg_root.display());
    }
    files.sort();

    // --- парс модулей -----------------------------------------------
    let mut modules: Vec<nova_codegen::ast::Module> = Vec::new();
    for f in &files {
        let src = read_file(f)?;
        let m = nova_codegen::parser::parse(&src)
            .map_err(|d| anyhow!("{}", d.render(&src, &f.to_string_lossy())))?;
        modules.push(m);
    }

    // --- effect-surface публичного API ------------------------------
    let mut tree = nova_codegen::doc::build_workspace(&modules);
    nova_codegen::doc::strip_private(&mut tree);
    let surface = nova_codegen::effect_surface::compute(&tree);

    // Имя пакета — из nova.toml, иначе имя цели.
    let pkg_name = if pkg_root.is_dir() {
        nova_codegen::manifest::parse_manifest(&pkg_root.join("nova.toml"), &pkg_root)
            .map(|m| m.package_name)
            .unwrap_or(default_name)
    } else {
        default_name
    };
    Ok((surface, pkg_name))
}

/// Plan 03.4 Ф.1/Ф.2: `nova info` — effect-surface пакета (и
/// effect-diff с `--diff`).
fn cmd_info(
    target: &str,
    format: &str,
    diff: Option<&str>,
    fail_on_new: bool,
) -> Result<()> {
    let (surface, pkg_name) = info_surface(target)?;

    // --- Ф.2: effect-diff с базовой целью ---------------------------
    if let Some(base_target) = diff {
        let (base, base_name) = info_surface(base_target)?;
        let d = nova_codegen::effect_surface::diff(&base, &surface);
        if format == "json" {
            let out = serde_json::json!({
                "package": pkg_name,
                "base": base_name,
                "added": d.added,
                "removed": d.removed,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
        } else {
            println!(
                "{} {} (база: {})",
                bold("Effect-diff:"),
                pkg_name,
                base_name,
            );
            if d.is_empty() {
                println!("  без изменений — effect-surface идентична");
            } else {
                for e in &d.added {
                    println!("  {} {}  — новый эффект", green("+"), e);
                }
                for e in &d.removed {
                    println!("  {} {}  — убран", yellow("-"), e);
                }
            }
        }
        if fail_on_new && !d.added.is_empty() {
            return Err(anyhow!(
                "effect-diff: появились новые эффекты ({}) — требуется ревью",
                d.added.join(", "),
            ));
        }
        return Ok(());
    }

    if format == "json" {
        let by_effect: serde_json::Map<String, serde_json::Value> = surface
            .by_effect
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect();
        let out = serde_json::json!({
            "package": pkg_name,
            "public_fns": surface.total_public_fns,
            "effectful_fns": surface.effectful_fns,
            "effect_surface": surface.effects,
            "by_effect": by_effect,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    // human
    println!("{} {}", bold("Пакет:"), pkg_name);
    println!(
        "Публичный API: {} функц., {} с эффектами",
        surface.total_public_fns, surface.effectful_fns,
    );
    println!();
    if surface.is_pure() {
        println!(
            "{} ∅ — публичный API без эффектов (pure)",
            bold("Effect-surface:"),
        );
    } else {
        println!("{} {}", bold("Effect-surface:"), surface.effects.join(", "));
        let width = surface.effects.iter().map(|e| e.chars().count()).max().unwrap_or(0);
        for eff in &surface.effects {
            let fns = surface.by_effect.get(eff).cloned().unwrap_or_default();
            println!(
                "  {:<width$}  ← {}",
                eff,
                fns.join(", "),
                width = width,
            );
        }
    }
    Ok(())
}

// ── Plan 100.8 / D7: `nova consume-analyze` ──────────────────────────────

/// Count `let consume` bindings in a Block (top-level stmts only — V1 approximation).
/// Full recursive walk would require traversing Expr variants; for V1 the top-level
/// count gives a useful hygiene signal with minimal complexity.
fn count_consume_bindings_in_block(block: &nova_codegen::ast::Block) -> usize {
    block.stmts.iter().filter(|s| {
        matches!(s, nova_codegen::ast::Stmt::Let(ld) if ld.consume)
    }).count()
}

/// Count consume bindings across all top-level function blocks in a module.
fn module_consume_count(module: &nova_codegen::ast::Module) -> usize {
    let mut total = 0;
    for item in &module.items {
        if let nova_codegen::ast::Item::Fn(fd) = item {
            match &fd.body {
                nova_codegen::ast::FnBody::Block(b) => {
                    total += count_consume_bindings_in_block(b);
                }
                nova_codegen::ast::FnBody::Expr(_) | nova_codegen::ast::FnBody::External => {}
            }
        }
    }
    total
}

/// Parse a single .nv file and run type-check (includes check_consume).
/// Returns (module, diagnostics).
fn consume_analyze_parse(
    path: &Path,
) -> Result<(nova_codegen::ast::Module, Vec<nova_codegen::diag::Diagnostic>)> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("cannot read {}: {}", path.display(), e))?;
    let tokens = nova_codegen::lexer::lex(&src)
        .map_err(|d| anyhow!("{}", d.message))?;
    let mut parser = nova_codegen::parser::Parser::new(tokens);
    let module = parser.parse_module().map_err(|d| anyhow!("{}", d.message))?;
    let diags = match nova_codegen::types::check_module(&module) {
        Ok(_) => vec![],
        Err(errs) => errs,
    };
    Ok((module, diags))
}

fn cmd_consume_analyze(path: &Path, format: &str, fail_on_uncovered: bool) -> Result<()> {
    // Collect .nv files (file or directory).
    let files: Vec<PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        let mut fs = Vec::new();
        nova_codegen::test_runner::walk_nv(path, &mut fs)
            .map_err(|e| anyhow!("walk {}: {}", path.display(), e))?;
        fs.sort();
        fs
    } else {
        return Err(usage_err(format!("path not found: {}", path.display())));
    };

    // Per-file analysis results.
    struct FileReport {
        file: PathBuf,
        total: usize,
        uncovered: usize,
        uncovered_msgs: Vec<String>,
    }

    let mut reports: Vec<FileReport> = Vec::new();
    for file in &files {
        let (module, diags) = consume_analyze_parse(file)?;
        let total = module_consume_count(&module);
        let d133_errs: Vec<_> = diags
            .iter()
            .filter(|d| d.message.contains("D133"))
            .collect();
        let uncovered = d133_errs.len();
        let uncovered_msgs: Vec<String> = d133_errs
            .iter()
            .map(|d| d.message.clone())
            .collect();
        reports.push(FileReport { file: file.clone(), total, uncovered, uncovered_msgs });
    }

    let grand_total: usize = reports.iter().map(|r| r.total).sum();
    let grand_uncovered: usize = reports.iter().map(|r| r.uncovered).sum();
    let grand_covered = grand_total.saturating_sub(grand_uncovered);

    if format == "json" {
        let file_entries: Vec<serde_json::Value> = reports.iter().map(|r| {
            serde_json::json!({
                "file": r.file.display().to_string(),
                "consume_bindings": r.total,
                "covered": r.total.saturating_sub(r.uncovered),
                "uncovered": r.uncovered,
                "uncovered_diagnostics": r.uncovered_msgs,
            })
        }).collect();
        let out = serde_json::json!({
            "schema": "nova-consume-analyze/v1",
            "total_bindings": grand_total,
            "covered": grand_covered,
            "uncovered": grand_uncovered,
            "files": file_entries,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("{}", bold("Coverage report (consume-analyze):"));
        println!();
        for r in &reports {
            let covered = r.total.saturating_sub(r.uncovered);
            let status = if r.uncovered == 0 { "✅" } else { "❌" };
            println!(
                "  {} {}: {} bindings; {} covered, {} NOT COVERED",
                status,
                r.file.display(),
                r.total,
                covered,
                r.uncovered,
            );
            for msg in &r.uncovered_msgs {
                println!("    ⚠  {}", msg);
            }
        }
        println!();
        println!(
            "  Summary: {} total bindings, {} covered, {} uncovered",
            grand_total, grand_covered, grand_uncovered,
        );
    }

    if fail_on_uncovered && grand_uncovered > 0 {
        return Err(anyhow!(
            "consume-analyze: {} uncovered consume binding(s) — CI gate failed",
            grand_uncovered,
        ));
    }
    Ok(())
}

fn cmd_build(
    path: &Path,
    output: Option<&Path>,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    timeout_secs: u64,
    keep_artifacts: bool,
    mono_depth: Option<usize>,
) -> Result<()> {
    if timeout_secs == 0 {
        return Err(usage_err("--timeout must be >= 1 second"));
    }
    // Plan 36 R6: validate path semantics before canonicalize (better errors).
    // `nova build` принимает **только single file** (по дизайну — `-o output`
    // один binary, multi-source builds через imports внутри одного entry-point).
    if !path.exists() {
        return Err(usage_err(format!("path not found: {}", path.display())));
    }
    if path.is_dir() {
        return Err(usage_err(format!(
            "`nova build` requires a single .nv file, got directory: {}\n\
             For multi-file projects, use imports within one entry-point file.\n\
             To check multiple files: `nova check <dir>` (typecheck only).",
            path.display())));
    }
    if path.extension().and_then(|s| s.to_str()) != Some("nv") {
        return Err(usage_err(format!("not a Nova source: {}", path.display())));
    }

    let build_start = std::time::Instant::now();
    let repo = find_repo_root()?;
    let paths = resolve_paths(&repo);

    let path = path.canonicalize()
        .map_err(|e| usage_err(format!("cannot resolve path {}: {}", path.display(), e)))?;
    let src = read_file(&path)?;
    let path_str = path.to_string_lossy();

    // parse — Plan 57.C.1 PerfTimer hooks
    let mut module = {
        let _t = nova_codegen::perf_timer::PerfTimer::new("parse");
        nova_codegen::parser::parse(&src)
            .map_err(|d| anyhow!("{}", d.render(&src, &path_str)))?
    };
    check_module_path(&path, &module)?;

    // Plan 03.1 Ф.4: синхронизировать `nova.lock` пакета entry-файла —
    // зафиксировать граф зависимостей (path/git) для воспроизводимой
    // сборки. Запускается ДО резолва импортов: материализует git-deps в
    // кэше и загружает зафиксированные commit'ы, которыми затем
    // пользуется резолвер. Файл без пакета (нет `nova.toml`) —
    // зависимостей нет, шаг пропускается.
    if let Some(pkg_dir) = nova_codegen::manifest::find_package_dir(&path) {
        let _t = nova_codegen::perf_timer::PerfTimer::new("dep-lock");
        nova_codegen::lockfile::sync(&pkg_dir)
            .map_err(|e| anyhow!("резолюция зависимостей (nova.lock): {}", e))?;
        // Plan 03.4 Ф.3: capability-confined deps — проверить, что
        // зависимости с `forbid` не используют запрещённые эффекты.
        nova_codegen::effect_surface::check_forbidden(&pkg_dir)?;
    }

    // Plan 35 Ф.1 MVP: cross-file resolve через inline expansion.
    {
        let _t = nova_codegen::perf_timer::PerfTimer::new("imports-resolve");
        nova_codegen::imports::resolve_imports_inline(&path, &mut module, &repo, &paths.stdlib_dir)?;
    }

    // Plan 81 Ф.9: content-addressed build cache. После резолва импортов
    // (выше) известен полный набор исходных файлов сборки. При
    // байт-идентичных входах переиспользуем сгенерированный `.c`, минуя
    // type-check / effects / lints / desugar / callnorm / codegen.
    // Отключается NOVA_CACHE=0; пропускается при --keep-artifacts
    // (debug-режим — нужен полный набор реальных промежуточных артефактов).
    let cache_key: Option<String> =
        if build_cache::cache_enabled() && !keep_artifacts {
            let feats: Vec<String> =
                nova_codegen::imports::enabled_features().into_iter().collect();
            build_cache::compute_c_key(
                &module.peer_files,
                &feats,
                nova_codegen::imports::current_target_os(),
                mono_depth,
            )
        } else {
            None
        };

    let c_code: String = match cache_key
        .as_deref()
        .and_then(|k| build_cache::load_c(&repo, k))
    {
        Some(cached) => {
            // Cache hit. Кэш пишется ТОЛЬКО после успешной сборки —
            // байт-идентичный вход уже прошёл type-check и codegen.
            eprintln!("{} build cache hit — reusing generated C", green("note:"));
            cached
        }
        None => {
            // Cache miss — полный Rust-side пайплайн.
            {
                let _t = nova_codegen::perf_timer::PerfTimer::new("type-check");
                nova_codegen::types::check_module(&module).map_err(|errs| {
                    let msgs: Vec<String> = errs.iter()
                        .map(|d| d.render(&src, &path_str))
                        .collect();
                    anyhow!("{}", msgs.join("\n"))
                })?;
            }
            {
                let _t = nova_codegen::perf_timer::PerfTimer::new("effects-infer");
                nova_codegen::types::infer_effects(&mut module);
            }
            {
                let _t = nova_codegen::perf_timer::PerfTimer::new("lints");
                for w in nova_codegen::lints::lint_module(&module) {
                    let (line, col) = nova_codegen::diag::byte_to_line_col(&src, w.diag.span.start);
                    eprintln!("{} {}:{}:{}: {} [{}]", bold(&yellow("warning:")), path.display(), line, col, w.diag.message, w.rule);
                }
            }
            // Plan 52 Ф.4: десугаринг map-литералов `[k: v]` → block-expr.
            {
                let _t = nova_codegen::perf_timer::PerfTimer::new("annotate-maps");
                nova_codegen::types::annotate_map_literals(&mut module);
            }
            {
                let _t = nova_codegen::perf_timer::PerfTimer::new("desugar");
                nova_codegen::desugar::desugar_module(&mut module);
            }
            // Plan 46 (D102) Ф.2: нормализация call-site — named → positional.
            {
                let _t = nova_codegen::perf_timer::PerfTimer::new("callnorm");
                nova_codegen::callnorm::normalize_module(&mut module);
            }
            let (c_code, warnings) = {
                let _t = nova_codegen::perf_timer::PerfTimer::new("codegen");
                let mut emitter = nova_codegen::codegen::CEmitter::new();
                emitter.set_source_for_annotations(src.clone());
                if let Some(n) = mono_depth {
                    emitter.set_mono_depth_limit(n);
                }
                emitter.emit_module(&module)
                    .map_err(|e| anyhow!("codegen error: {}", e))?
            };
            for w in &warnings {
                eprintln!("{}", w);
            }
            // Записываем `.c` в кэш — только теперь, после успешного
            // codegen (кэшируем лишь заведомо валидный артефакт).
            if let Some(k) = &cache_key {
                build_cache::store_c(&repo, k, &c_code);
            }
            c_code
        }
    };

    // determine output path
    let exe_stem = path.file_stem().unwrap_or_default();
    let exe_name = if cfg!(target_os = "windows") {
        format!("{}.exe", exe_stem.to_string_lossy())
    } else {
        exe_stem.to_string_lossy().into_owned()
    };
    let final_exe = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            // Place output next to CWD, not next to the source file (predictable).
            std::env::current_dir()
                .unwrap_or_else(|_| path.parent().unwrap_or(&path).to_path_buf())
                .join(&exe_name)
        });
    if final_exe.is_dir() {
        bail!("output path is a directory: {}", final_exe.display());
    }

    // write .c to a unique tmp dir; cleaned up on any exit path via Drop
    let hash = path_hash(&path);
    let tmp_path = default_tmp_dir().join(format!("build-{}", &hash[..hash.len().min(12)]));
    std::fs::create_dir_all(&tmp_path)
        .map_err(|e| anyhow!("create tmp dir: {}", e))?;
    let _tmp_guard = TmpDirGuard { path: &tmp_path, keep: keep_artifacts };
    let c_file = tmp_path.join(format!("{}.c", exe_stem.to_string_lossy()));
    let exe_file = tmp_path.join(&exe_name);
    std::fs::write(&c_file, &c_code)
        .map_err(|e| anyhow!("write .c file: {}", e))?;

    // detect toolchain
    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    // detect libuv
    let libuv = test_runner::detect_or_build_libuv(&paths.rt_dir, &repo, tc.vcvars_path());

    test_runner::install_cancel_handler();

    // compile .c → .exe
    // Plan 115 D214 [M-115-ffi-build-pipeline]: nova-cli build не подхватывает
    // [ffi] (это test-runner job; standalone build пока без manifest-driven
    // FFI). Followup: extend nova-cli build чтобы также резолвить manifest.
    let build_opts = test_runner::BuildOpts {
        c_file: &c_file,
        exe_file: &exe_file,
        obj_dir: &tmp_path,
        cg_include: &paths.cg_include,
        rt_dir: &paths.rt_dir,
        mode,
        libuv: libuv.as_ref(),
        gc_kind: test_runner::GcKind::default(),
        ffi: None,
    };
    {
        let _t = nova_codegen::perf_timer::PerfTimer::new("c-compile");
        test_runner::compile_c_to_exe(&tc, &build_opts, Duration::from_secs(timeout_secs))?;
    }

    // move exe to final destination
    if let Some(parent) = final_exe.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("create output dir: {}", e))?;
    }
    std::fs::rename(&exe_file, &final_exe)
        .or_else(|_| std::fs::copy(&exe_file, &final_exe).map(|_| ()))
        .map_err(|e| anyhow!("move executable: {}", e))?;

    println!("{} {} ({:.2}s)", green("built:"), final_exe.display(), build_start.elapsed().as_secs_f64());
    Ok(())
}

fn cmd_test(
    path_arg: Option<&Path>,
    filter: Option<&str>,
    jobs: usize,
    format: &str,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    timeout_secs: u64,
    verbose: bool,
    quiet: bool,
    results_file: Option<&Path>,
    rerun_failed: bool,
    retries: u32,
    include_stdlib: bool,
    keep_artifacts: bool,
    gc: &str,
    list_only: bool,
    filter_from: Option<&Path>,
    shuffle: Option<Option<u64>>,
    skip: &[String],
    mono_depth: Option<usize>,
) -> Result<()> {
    if timeout_secs == 0 {
        return Err(usage_err("--timeout must be >= 1 second"));
    }
    if let Some(f) = filter {
        if f.is_empty() {
            return Err(usage_err("--filter cannot be empty"));
        }
    }
    if verbose && quiet {
        return Err(usage_err("cannot use --verbose and --quiet simultaneously"));
    }
    let repo = find_repo_root()?;
    let paths = resolve_paths(&repo);

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

    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    let libuv = test_runner::detect_or_build_libuv(
        &paths.rt_dir,
        &repo,
        tc.vcvars_path(),
    );

    // Plan 36 Ф.1: positional path argument.
    // None → default <repo>/nova_tests/.
    // Some(file) → use parent dir as tests_dir, filter to single file via display name.
    // Some(dir) → use as tests_dir.
    let (tests_dir, single_file_filter): (PathBuf, Option<String>) = match path_arg {
        None => (paths.tests_dir.clone(), None),
        Some(p) => {
            if !p.exists() {
                return Err(usage_err(format!("path not found: {}", p.display())));
            }
            if p.is_file() {
                if p.extension().and_then(|s| s.to_str()) != Some("nv") {
                    return Err(usage_err(format!("not a Nova source: {}", p.display())));
                }
                // Use parent dir as tests_dir, derive display name relative to it.
                let parent = p.parent()
                    .ok_or_else(|| usage_err(format!("cannot get parent of {}", p.display())))?
                    .to_path_buf();
                let stem = p.file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| usage_err(format!("invalid file name: {}", p.display())))?;
                (parent, Some(stem.to_string()))
            } else if p.is_dir() {
                (p.to_path_buf(), None)
            } else {
                return Err(usage_err(format!("path is neither file nor directory: {}", p.display())));
            }
        }
    };

    if format == test_runner::OutputFormat::Text {
        eprintln!(
            "Toolchain: {}, mode={:?}, jobs={}, tests-dir={}",
            tc.name(),
            mode,
            jobs,
            tests_dir.display()
        );
        if libuv.is_none() {
            eprintln!("warning: libuv not found — concurrency tests will fail");
        }
    }

    if !tests_dir.is_dir() {
        return Err(usage_err(format!("tests directory not found: {}", tests_dir.display())));
    }

    // If single-file requested, filter through display-name to that one file
    // (path-filter ⊥ test-name-filter — single-file is path-filter).
    // If user passes both single-file path + --filter — single-file wins (path is
    // path-filter, --filter is test-name-filter; current test_runner uses single
    // string filter for display-name, so we cannot orthogonally combine).
    let effective_filter: Option<String> = single_file_filter.or_else(|| filter.map(str::to_string));
    let effective_filter_ref: Option<&str> = effective_filter.as_deref();
    let stdlib_dir_opt = if include_stdlib {
        Some(paths.stdlib_dir.as_path())
    } else {
        None
    };

    // ensure target/ dir exists for the default results file
    let results_path: PathBuf = results_file
        .map(Path::to_path_buf)
        .unwrap_or(paths.default_results_file);
    if results_path.is_dir() {
        bail!("results file path is a directory: {}", results_path.display());
    }
    if rerun_failed && !results_path.is_file() {
        bail!(
            "--rerun-failed requires a previous results file at {}\n\
             Run `nova test` first to generate it.",
            results_path.display()
        );
    }
    if let Some(parent) = results_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow!("cannot create results dir {}: {}", parent.display(), e))?;
    }

    let gc_kind = test_runner::GcKind::parse(gc)?;
    // --shuffle: None = no shuffle, Some(None) = random seed, Some(Some(n)) = fixed seed
    let shuffle_seed: Option<u64> = match shuffle {
        None => None,
        Some(None) => Some(0),      // 0 = random seed (resolved in run_all)
        Some(Some(n)) => Some(n),
    };
    let tmp_dir = default_tmp_dir();
    let opts = test_runner::TestAllOpts {
        tests_dir: &tests_dir,
        stdlib_dir: stdlib_dir_opt,
        include_stdlib,
        filter: effective_filter_ref,
        mode,
        toolchain: tc,
        cg_include: &paths.cg_include,
        rt_dir: &paths.rt_dir,
        tmp_dir: &tmp_dir,
        keep_artifacts,
        libuv,
        timeout: Duration::from_secs(timeout_secs),
        jobs,
        format,
        verbosity,
        cache_dir: None,
        results_file: Some(&results_path),
        rerun_failed,
        retries,
        gc_kind,
        list_only,
        filter_from,
        shuffle_seed,
        skip,
        mono_depth,
    };

    // Plan 57.D.1: optionally aggregate PerfTimer markers across all
    // tests. Activated по NOVA_PERF_TIMER_AGGREGATE=1 — suppress
    // per-test __PERF__ markers и emit aggregated table в конце.
    let aggregate = std::env::var("NOVA_PERF_TIMER_AGGREGATE")
        .map(|v| v == "1" || v == "true" || v == "yes")
        .unwrap_or(false);
    if aggregate {
        nova_codegen::perf_timer::enable_aggregation();
    }

    let summary = test_runner::run_all(opts)?;
    test_runner::print_summary(&summary, format);

    if aggregate {
        let table = nova_codegen::perf_timer::dump_aggregated();
        if !table.is_empty() { eprintln!("{}", table); }
    }

    if summary.fail > 0 {
        Err(anyhow!("{} test(s) failed", summary.fail))
    } else {
        Ok(())
    }
}

fn cmd_test_build(
    path: &Path,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    timeout_secs: u64,
    keep_artifacts: bool,
    gc: &str,
    mono_depth: Option<usize>,
) -> Result<()> {
    if timeout_secs == 0 {
        bail!("--timeout must be >= 1 second");
    }
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    let repo = find_repo_root()?;
    let paths = resolve_paths(&repo);

    let mode = test_runner::Mode::parse(mode)?;
    let pref = test_runner::ToolchainPref::parse(toolchain)?;
    let tc_opts = test_runner::ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    };
    let tc = test_runner::detect_toolchain(&tc_opts)?;

    let libuv = test_runner::detect_or_build_libuv(&paths.rt_dir, &repo, tc.vcvars_path());

    let display = path
        .strip_prefix(&paths.tests_dir)
        .or_else(|_| path.strip_prefix(&repo))
        .unwrap_or(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/");

    let gc_kind = test_runner::GcKind::parse(gc)?;
    let tmp_dir = default_tmp_dir();
    let opts = test_runner::TestBuildOpts {
        nv_file: path,
        toolchain: &tc,
        mode,
        cg_include: &paths.cg_include,
        rt_dir: &paths.rt_dir,
        tmp_dir: &tmp_dir,
        display: &display,
        keep_artifacts,
        libuv: libuv.as_ref(),
        timeout: Duration::from_secs(timeout_secs),
        gc_kind,
        verbosity: test_runner::Verbosity::Normal,
        mono_depth,
        // Plan 83.1 Ф.5: single-file run — один процесс, нет
        // oversubscription, бюджет не нужен.
        maxprocs_budget: None,
    };

    test_runner::install_cancel_handler();
    let outcome = test_runner::run_one(&opts);
    let label = outcome.label();
    let elapsed = outcome.elapsed();
    let detail = outcome.detail();
    if outcome.is_pass() {
        println!("{} {} ({:.2}s)", green(label), display, elapsed.as_secs_f64());
        Ok(())
    } else {
        eprintln!("{} {} ({:.2}s)", bold(&red(label)), display, elapsed.as_secs_f64());
        if !detail.is_empty() {
            eprintln!("{}", detail);
        }
        Err(anyhow!("test failed"))
    }
}

fn cmd_regen_runtime(check: bool) -> Result<()> {
    let repo = find_repo_root()?;
    use nova_codegen::codegen::runtime_registry;
    let registry = runtime_registry::all();
    let groups = runtime_registry::group_by_module(&registry);
    let mut total = 0;
    let mut diffs: Vec<String> = Vec::new();
    for (module, fns) in &groups {
        let rel = runtime_registry::module_to_path(module);
        let abs = repo.join(&rel);
        let content = runtime_registry::render_nv(module, fns);
        if check {
            let existing = std::fs::read_to_string(&abs)
                .map_err(|e| anyhow!("failed to read {}: {}", abs.display(), e))?;
            let norm = |s: &str| s.replace("\r\n", "\n");
            if norm(&existing) != norm(&content) {
                diffs.push(rel);
            }
        } else {
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| anyhow!("create dir {}: {}", parent.display(), e))?;
            }
            std::fs::write(&abs, &content)
                .map_err(|e| anyhow!("write {}: {}", abs.display(), e))?;
            println!("wrote {}", rel);
        }
        total += 1;
    }
    if check {
        if !diffs.is_empty() {
            return Err(anyhow!(
                "auto-generated files diverge from registry:\n  {}\n\
                 Run `nova regen-runtime` to regenerate.",
                diffs.join("\n  ")
            ));
        }
        println!("OK: {} runtime stub file(s) match registry.", total);
    } else {
        println!("emitted {} runtime stub file(s).", total);
    }
    Ok(())
}

// ---------- nova contracts ----------

fn cmd_contracts(sub: ContractsCmd) -> Result<()> {
    match sub {
        ContractsCmd::List { file } => cmd_contracts_list(&file),
        ContractsCmd::Verify { file, backend } => cmd_contracts_verify(&file, backend.as_deref()),
        ContractsCmd::Suggest { file, fn_name } => cmd_contracts_suggest(&file, &fn_name),
        ContractsCmd::Counterexample { file, fn_name, contract_id } => {
            cmd_contracts_counterexample(&file, &fn_name, contract_id)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Plan 57: `nova bench` command group.
// ─────────────────────────────────────────────────────────────────────────

fn cmd_bench(sub: BenchCmd) -> Result<()> {
    match sub {
        BenchCmd::Run { file, filter, samples, warmup_ms, time_budget_s,
                        gc, mode, toolchain, vcvars, clang, compile_timeout, run_timeout,
                        keep_artifacts, mono_depth, out_json, out_csv, out_md,
                        out_criterion, profile, histogram } => {
            let repo = find_repo_root()?;
            let paths = resolve_paths(&repo);
            let pref = test_runner::ToolchainPref::parse(&toolchain)?;
            let gc_kind = test_runner::GcKind::parse(&gc)?;
            let mode_enum = test_runner::Mode::parse(&mode)?;
            let tc_opts = test_runner::ToolchainOpts {
                pref,
                explicit_clang: clang.as_deref(),
                explicit_vcvars: vcvars.as_deref(),
            };
            let color = should_use_color();
            let opts = bench::run::BenchRunOpts {
                bench_path: &file,
                repo: &repo,
                stdlib_dir: &paths.stdlib_dir,
                cg_include: &paths.cg_include,
                rt_dir: &paths.rt_dir,
                tc_opts,
                filter,
                samples,
                warmup_ms,
                time_budget_s,
                gc_kind,
                compile_timeout_secs: compile_timeout,
                run_timeout_secs: run_timeout,
                keep_artifacts,
                mono_depth,
                mode: mode_enum,
                out_json: out_json.as_deref(),
                out_csv: out_csv.as_deref(),
                out_md: out_md.as_deref(),
                out_criterion: out_criterion.as_deref(),
                color,
                histogram,
            };
            // Plan 57.A.5: profile mode — отдельный run после measurement.
            if let Some(prof_args) = profile.as_ref() {
                if prof_args.len() != 2 {
                    return Err(usage_err("--profile expects: MODE OUT (2 args)"));
                }
                let mode = bench::profile::ProfileMode::parse(&prof_args[0])?;
                let prof_out = PathBuf::from(&prof_args[1]);
                eprintln!("profile: building separate bench-exe (no instrumentation overhead в measurement run)...");
                let exe = bench::run::compile_for_profile(&opts)?;
                // Reduced samples для profile (don't need 100 — just enough trace).
                let prof_opts = bench::profile::ProfileOpts {
                    mode,
                    out: &prof_out,
                    bench_exe: &exe,
                    filter: opts.filter.as_deref(),
                    samples_override: 30,
                };
                let pexit = bench::profile::run(prof_opts)?;
                if pexit != 0 { std::process::exit(pexit); }
                return Ok(());
            }
            let exit = bench::run::run(opts)?;
            if exit != 0 {
                std::process::exit(exit);
            }
            Ok(())
        }
        BenchCmd::Diff { baseline, new, format, explain, ai_config,
                          ai_max_tokens, ai_dry_run, baseline_sha, new_sha } => {
            let fmt = bench::diff::DiffFormat::parse(&format)?;
            if !explain && !ai_dry_run {
                let exit = bench::diff::compare(&baseline, &new, fmt)?;
                if exit != 0 { std::process::exit(exit); }
                return Ok(());
            }
            // Plan 57.F.2: --explain branch — load pair, print regular
            // diff, then AI section.
            let loaded = bench::diff::load_pair(&baseline, &new)?;
            // Print regular diff output (so --explain doesn't suppress).
            let regular = match fmt {
                bench::diff::DiffFormat::Terminal =>
                    bench::diff::terminal_format(&loaded.rows, &loaded.compat_warnings),
                bench::diff::DiffFormat::Markdown =>
                    bench::diff::markdown_format(&loaded.rows, &loaded.compat_warnings),
                bench::diff::DiffFormat::Json =>
                    bench::diff::json_format(&loaded.rows, &loaded.compat_warnings)?,
            };
            print!("{}", regular);
            // Privacy warning (first-use; ignore returned status).
            eprintln!("\nai: --explain sends diff + git context к external LLM API. \
                Set NOVA_AI_NO_WARN=1 to suppress.");
            let cfg = bench::ai::AiConfig::load(ai_config.as_deref(), ai_max_tokens)?;
            // Resolve SHAs: CLI flag → JSON meta.
            let base_sha = baseline_sha.or_else(||
                loaded.baseline.metadata.compiler.nova_sha.clone());
            let new_sha_resolved = new_sha.or_else(||
                loaded.new.metadata.compiler.nova_sha.clone());
            let repo = find_repo_root().ok();
            let (git_diff, git_commits) = if let Some(r) = repo.as_ref() {
                if cfg.include_git_diff || cfg.include_commits {
                    bench::ai::collect_git_context(r,
                        base_sha.as_deref(), new_sha_resolved.as_deref(),
                        cfg.max_commits)
                } else { (None, None) }
            } else { (None, None) };
            let final_diff = if cfg.include_git_diff { git_diff } else { None };
            let final_commits = if cfg.include_commits { git_commits } else { None };
            let note = match (&base_sha, &new_sha_resolved) {
                (Some(b), Some(n)) => Some(format!("baseline sha: {}; new sha: {}", b, n)),
                _ => None,
            };
            let pb = bench::ai::PromptBuilder {
                diff_rows: &loaded.rows,
                git_diff: final_diff,
                git_commits: final_commits,
                note: note.as_deref(),
            };
            let prompt = pb.build();
            let response = bench::ai::call_api(&cfg, &prompt, ai_dry_run)?;
            let out = match fmt {
                bench::diff::DiffFormat::Markdown =>
                    bench::ai::render_markdown(&response),
                _ => bench::ai::render_terminal(&response),
            };
            print!("{}", out);
            Ok(())
        }
        BenchCmd::Gate { baseline, new, config, noise } => {
            let exit = bench::gate::run(&baseline, &new,
                config.as_deref(), noise.as_deref())?;
            if exit != 0 { std::process::exit(exit); }
            Ok(())
        }
        BenchCmd::Corpus { path, json, html, echarts_url, mode, toolchain, gc } => {
            // Discover nova-cli path (self).
            let self_exe = std::env::current_exe()
                .map_err(|e| anyhow!("locate self: {}", e))?;
            let files = if path.is_dir() {
                bench::corpus::list_corpus_files(&path)?
            } else {
                vec![path.clone()]
            };
            if files.is_empty() {
                return Err(usage_err(format!("no .nv files in {}", path.display())));
            }
            eprintln!("nova bench corpus: {} files", files.len());
            let mut entries = Vec::with_capacity(files.len());
            for f in &files {
                eprintln!("nova bench corpus: measuring {}", f.display());
                let e = bench::corpus::measure_file(
                    &self_exe, f, &gc, &mode, &toolchain)?;
                entries.push(e);
            }
            if json {
                let v = bench::corpus::render_json(&entries);
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if let Some(html_path) = html {
                let h = bench::corpus::render_html(&entries, &echarts_url);
                std::fs::write(&html_path, h)
                    .map_err(|e| anyhow!("write HTML: {}", e))?;
                eprintln!("nova bench corpus: wrote HTML to {}", html_path.display());
            } else {
                print!("{}", bench::corpus::render_terminal(&entries));
            }
            Ok(())
        }
        BenchCmd::RunnerBranch => {
            println!("{}", bench::history::default_branch());
            Ok(())
        }
        BenchCmd::Remote(sub) => cmd_bench_remote(sub),
        BenchCmd::HistoryAnomalies { branch, format } => {
            let repo = find_repo_root()?;
            let branch = resolve_history_branch(&branch);
            let results = bench::anomaly::scan_history(&repo, &branch)?;
            if format == "json" {
                let arr: Vec<serde_json::Value> = results.iter().map(|(name, cps)| {
                    let cps_json: Vec<serde_json::Value> = cps.iter().map(|(entry, cp)| {
                        serde_json::json!({
                            "timestamp_unix": entry.timestamp_unix,
                            "git_sha": entry.git_sha,
                            "filename": entry.filename,
                            "index": cp.index,
                            "mean_before_ns": cp.mean_before,
                            "mean_after_ns": cp.mean_after,
                            "delta_pct": cp.delta_pct,
                        })
                    }).collect();
                    serde_json::json!({"bench": name, "changepoints": cps_json})
                }).collect();
                let out = serde_json::json!({
                    "format_version": "1",
                    "kind": "bench-anomalies",
                    "branch": branch,
                    "results": arr,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("Anomaly scan на branch `{}`:", branch);
                if results.is_empty() {
                    println!("(no significant changepoints — все benches stable)");
                } else {
                    for (name, cps) in &results {
                        println!("\n{}: {} changepoint(s) detected:", name, cps.len());
                        for (entry, cp) in cps {
                            let dt = chrono_iso_short(entry.timestamp_unix);
                            let sign = if cp.delta_pct >= 0.0 { "+" } else { "" };
                            println!("  @{} (commit {}): {:.1} ns → {:.1} ns ({}{:.1}%)",
                                dt, &entry.git_sha[..entry.git_sha.len().min(12)],
                                cp.mean_before, cp.mean_after,
                                sign, cp.delta_pct);
                        }
                    }
                }
            }
            Ok(())
        }
        BenchCmd::CpuInstrCheck => {
            println!("CPU instructions counter availability:");
            println!("  OS: {}", std::env::consts::OS);
            println!("  Arch: {}", std::env::consts::ARCH);
            let avail = bench::cpu_instr::available();
            println!("  Available: {}", if avail { "yes ✓" } else { "no ✗" });
            if avail {
                let r = bench::cpu_instr::measure_instructions(|| {
                    let mut x: u64 = 0;
                    for _ in 0..1000 { x = x.wrapping_add(1); }
                    std::hint::black_box(x);
                });
                match r {
                    Ok(n) => println!("  Test (1000-iter loop): {} instructions", n),
                    Err(e) => println!("  Test failed: {}", e),
                }
            } else if cfg!(target_os = "linux") {
                println!("  Hint: try `sudo sysctl -w kernel.perf_event_paranoid=1`");
                println!("        или grant `CAP_PERFMON` capability to nova binary.");
            } else {
                println!("  Note: CPU instructions counter is Linux-only \
                          (uses perf_event_open syscall).");
            }
            Ok(())
        }
        BenchCmd::MembwCheck => {
            println!("Memory bandwidth measurement availability:");
            println!("  OS: {}", std::env::consts::OS);
            println!("  Arch: {}", std::env::consts::ARCH);
            let llc_avail = bench::membw::available();
            let mbm_avail = bench::membw::available_mbm();
            println!("  LLC-miss counter: {}",
                if llc_avail { "yes ✓" } else { "no ✗" });
            println!("  Uncore MBM events: {}",
                if mbm_avail { "yes ✓" } else { "no ✗" });
            let codes = bench::membw::mbm_event_codes();
            if !codes.is_empty() {
                println!("\n  Detected PMU events:");
                for ev in &codes {
                    println!("    {:<48}  type={}  config=0x{:x}",
                        ev.name, ev.pmu_type, ev.config);
                }
            }
            if llc_avail {
                println!("\n  Test (10MB memset):");
                // 10 MB allocation + write — forces LLC traffic.
                let r = bench::membw::measure_bandwidth(|| {
                    let mut v = vec![0u8; 10 * 1024 * 1024];
                    for i in 0..v.len() { v[i] = (i & 0xff) as u8; }
                    std::hint::black_box(&v);
                });
                match r {
                    Ok(sample) => println!("    ≈ {} via {}",
                        bench::membw::fmt_bytes(sample.bytes),
                        sample.source.as_str()),
                    Err(e) => println!("    Test failed: {}", e),
                }
            } else if cfg!(target_os = "linux") {
                println!("\n  Hint: try `sudo sysctl -w kernel.perf_event_paranoid=1`");
                println!("        для uncore_imc events может потребоваться CAP_PERFMON.");
            } else {
                println!("\n  Note: memory bandwidth is Linux-only \
                          (perf_event_open + sysfs uncore_imc).");
            }
            Ok(())
        }
        BenchCmd::CallgrindCheck => {
            println!("Valgrind Callgrind cross-platform CPU instructions (Plan 57.H.3):");
            println!("  OS: {}", std::env::consts::OS);
            let avail = bench::callgrind::available();
            println!("  valgrind в PATH: {}",
                if avail { "yes ✓" } else { "no ✗" });
            if avail {
                if let Some(v) = bench::callgrind::version_string() {
                    println!("  Version: {}", v);
                }
            } else if cfg!(target_os = "linux") {
                println!("  Install: sudo apt-get install valgrind  / dnf install valgrind");
            } else if cfg!(target_os = "macos") {
                println!("  Install: brew install --HEAD valgrind");
            } else {
                println!("  Note: Valgrind не supports Windows. Use perf_event_open path");
                println!("        (Linux), или install WSL for valgrind на Windows.");
            }
            Ok(())
        }
        BenchCmd::Callgrind { binary, args, cache_sim, workdir, out } => {
            let opts = bench::callgrind::CallgrindOpts {
                binary: &binary,
                args: &args,
                workdir: workdir.as_deref(),
                out_file: None,
                cache_sim,
            };
            let r = bench::callgrind::measure(opts)?;
            println!("callgrind {}:", binary.display());
            println!("  Instructions (Ir): {}", r.instructions);
            if let Some(v) = r.i1_misses {
                println!("  I1 misses:         {}", v);
            }
            if let Some(v) = r.d1_misses {
                println!("  D1 misses:         {}", v);
            }
            if let Some(v) = r.ll_misses {
                println!("  LL misses:         {}", v);
            }
            println!("  Raw output: {}", r.raw_output_path.display());
            if let Some(p) = out {
                let json = serde_json::json!({
                    "format_version": "1",
                    "kind": "callgrind-result",
                    "binary": binary.to_string_lossy(),
                    "instructions": r.instructions,
                    "i1_misses": r.i1_misses,
                    "d1_misses": r.d1_misses,
                    "ll_misses": r.ll_misses,
                });
                std::fs::write(&p, serde_json::to_string_pretty(&json)?)
                    .map_err(|e| anyhow!("write JSON {}: {}", p.display(), e))?;
                eprintln!("wrote callgrind JSON to {}", p.display());
            }
            Ok(())
        }
        BenchCmd::Hyperfine { specs, warmup, samples, timeout_secs, workdir, out } => {
            let mut parsed_specs = Vec::with_capacity(specs.len());
            for s in &specs {
                parsed_specs.push(bench::hyperfine::HyperfineSpec::parse(s)?);
            }
            let opts = bench::hyperfine::HyperfineOpts {
                specs: parsed_specs,
                warmup_runs: warmup,
                samples,
                timeout_secs,
                workdir,
            };
            let benches = bench::hyperfine::run(opts)?;
            // Always print terminal table.
            let sampling = bench::repro::SamplingMeta {
                warmup_ns: 0, target_ns: 0,
                samples: samples as u64, time_budget_ns: 0,
            };
            let meta = bench::repro::collect("hyperfine", sampling);
            print!("{}", bench::report::terminal_report(&meta, &benches, should_use_color()));
            if let Some(p) = out {
                bench::hyperfine::write_json(&benches, &p)?;
                eprintln!("wrote JSON to {}", p.display());
            }
            Ok(())
        }
        BenchCmd::Calibrate { runs, out } => {
            let n = bench::noise::calibrate(&runs)?;
            n.save(&out)?;
            eprintln!("calibration: saved to {} (suite noise={:.2}%, {} benches)",
                out.display(), n.suite_noise_pct, n.per_bench.len());
            for (name, noise_pct) in &n.per_bench {
                eprintln!("  {:<40} noise={:.2}%", name, noise_pct);
            }
            Ok(())
        }
        BenchCmd::HistoryAdd { result, branch, push, remote, dry_run } => {
            let repo = find_repo_root()?;
            let branch = resolve_history_branch(&branch);
            let opts = bench::history::HistoryAddOpts {
                result_json: &result,
                branch,
                repo: &repo,
                push,
                remote,
                dry_run,
            };
            let exit = bench::history::add(opts)?;
            if exit != 0 { std::process::exit(exit); }
            Ok(())
        }
        BenchCmd::HistorySquash { before_date, branch, push, remote, dry_run } => {
            let repo = find_repo_root()?;
            let branch = resolve_history_branch(&branch);
            let before_unix = parse_date_to_unix(&before_date)?;
            let exit = bench::history::squash(&repo, &branch, before_unix,
                push, &remote, dry_run)?;
            if exit != 0 { std::process::exit(exit); }
            Ok(())
        }
        BenchCmd::HistoryList { branch } => {
            let repo = find_repo_root()?;
            let branch = resolve_history_branch(&branch);
            let entries = bench::history::list(&repo, &branch)?;
            if entries.is_empty() {
                println!("(no entries in branch `{}`)", branch);
            } else {
                println!("{:<14}  {:<16}  filename", "timestamp", "sha");
                for e in &entries {
                    println!("{:<14}  {:<16}  {}", e.timestamp_unix,
                        e.git_sha, e.filename);
                }
                println!("\n{} total entries", entries.len());
            }
            Ok(())
        }
        BenchCmd::Dashboard { history_branch, out, max_entries, echarts_url } => {
            let repo = find_repo_root()?;
            let history_branch = resolve_history_branch(&history_branch);
            let opts = bench::dashboard::DashboardOpts {
                repo: &repo,
                history_branch,
                out_dir: &out,
                max_entries,
                echarts_url,
            };
            let exit = bench::dashboard::generate(opts)?;
            if exit != 0 { std::process::exit(exit); }
            Ok(())
        }
    }
}

/// Plan 57.F.1: `nova bench remote ...` dispatcher.
fn cmd_bench_remote(sub: BenchRemoteCmd) -> Result<()> {
    use bench::remote::{RemotesFile, RemoteConfig, run_distributed};

    // Resolve config path with subcommand-provided override.
    fn load(explicit: Option<&Path>) -> Result<RemotesFile> {
        let path = RemotesFile::resolve_path(explicit)
            .ok_or_else(|| anyhow!(
                "cannot resolve remotes config path (set --config or \
                 $NOVA_BENCH_REMOTES, or set $HOME/$USERPROFILE)"))?;
        if !path.exists() {
            bail!("remotes config not found at {} \
                   (create it or pass --config)", path.display());
        }
        let f = RemotesFile::load_or_default(&path);
        for err in &f.parse_errors {
            eprintln!("warning: {}: {}", path.display(), err);
        }
        Ok(f)
    }

    match sub {
        BenchRemoteCmd::List { config } => {
            let f = load(config.as_deref())?;
            if f.remotes.is_empty() {
                println!("(no remotes configured)");
                return Ok(());
            }
            println!("{:<20}  {:<30}  {:<12}  {:<6}  repo",
                "name", "host", "user", "port");
            println!("{:<20}  {:<30}  {:<12}  {:<6}  ----",
                "----", "----", "----", "----");
            for r in &f.remotes {
                let port = r.ssh_port.map(|p| p.to_string())
                    .unwrap_or_else(|| "22".to_string());
                println!("{:<20}  {:<30}  {:<12}  {:<6}  {}",
                    r.name, r.host, r.user, port, r.repo);
            }
            println!("\n{} remote(s)", f.remotes.len());
            Ok(())
        }
        BenchRemoteCmd::Ping { name, config } => {
            let f = load(config.as_deref())?;
            let r = f.find(&name)
                .ok_or_else(|| anyhow!("remote `{}` not found", name))?;
            eprintln!("ping {} ({}@{}) ...", r.name, r.user, r.host);
            r.ping()?;
            println!("OK: {} reachable", r.name);
            Ok(())
        }
        BenchRemoteCmd::Run { bench, remotes, gather_into, sha, config } => {
            let f = load(config.as_deref())?;
            if f.remotes.is_empty() {
                bail!("no remotes configured — cannot run distributed");
            }
            // Resolve selector → list of &RemoteConfig.
            let selected: Vec<&RemoteConfig> = if remotes == "all" {
                f.remotes.iter().collect()
            } else {
                let mut out = Vec::new();
                for name in remotes.split(',').map(|s| s.trim())
                                            .filter(|s| !s.is_empty()) {
                    let r = f.find(name)
                        .ok_or_else(|| anyhow!("remote `{}` not found", name))?;
                    out.push(r);
                }
                if out.is_empty() {
                    bail!("--remotes resolved to empty set");
                }
                out
            };
            let bench_str = bench.to_string_lossy().to_string();
            eprintln!("nova bench remote run: {} bench={} → {}",
                selected.len(), bench_str, gather_into.display());
            let results = run_distributed(&selected, sha.as_deref(),
                &bench_str, &gather_into);
            // Summary + non-zero exit if any failure.
            let mut failures = 0;
            println!("\nResults:");
            for (name, res) in &results {
                match res {
                    Ok(path) => println!("  ✓ {:<20}  {}", name, path.display()),
                    Err(e) => {
                        println!("  ✗ {:<20}  ERROR: {}", name, e);
                        failures += 1;
                    }
                }
            }
            println!("\n{}/{} succeeded",
                results.len() - failures, results.len());
            if failures > 0 { std::process::exit(2); }
            Ok(())
        }
    }
}

/// Plan 57.E.5: short ISO timestamp YYYY-MM-DD HH:MM для anomaly output.
fn chrono_iso_short(secs: u64) -> String {
    // Reuse repro::ReproMeta::timestamp_iso8601 logic (Howard Hinnant alg).
    let z = (secs / 86400) as i64;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let z_adj = z + 719468;
    let era = if z_adj >= 0 { z_adj } else { z_adj - 146096 } / 146097;
    let doe = (z_adj - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m_cal = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m_cal <= 2 { y + 1 } else { y } as i32;
    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, m_cal, d, h, m)
}

/// Plan 57.D.4: resolve "auto" → bench::history::default_branch()
/// (NOVA_BENCH_RUNNER_ID-aware). Иначе return as-is.
fn resolve_history_branch(s: &str) -> String {
    if s == "auto" { bench::history::default_branch() } else { s.to_string() }
}

/// Plan 57.C.6: parse YYYY-MM-DD to unix timestamp (UTC midnight).
/// Inverse of unix_to_ymdhms (Howard Hinnant alg).
fn parse_date_to_unix(s: &str) -> Result<u64> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(usage_err(format!("expected YYYY-MM-DD, got `{}`", s)));
    }
    let y: i64 = parts[0].parse()
        .map_err(|_| usage_err(format!("invalid year: {}", parts[0])))?;
    let m: u64 = parts[1].parse()
        .map_err(|_| usage_err(format!("invalid month: {}", parts[1])))?;
    let d: u64 = parts[2].parse()
        .map_err(|_| usage_err(format!("invalid day: {}", parts[2])))?;
    if m < 1 || m > 12 || d < 1 || d > 31 {
        return Err(usage_err(format!("invalid date: {}", s)));
    }
    // Howard Hinnant (inverse of unix_to_ymdhms).
    let yp = if m <= 2 { y - 1 } else { y };
    let era = if yp >= 0 { yp } else { yp - 399 } / 400;
    let yoe = (yp - era * 400) as u64;  // [0, 399]
    let mu = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mu + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_epoch = era * 146097 + (doe as i64) - 719468;
    Ok((days_since_epoch * 86400) as u64)
}

/// True if terminal output should be colorized. Mirrors existing color
/// detection — see ColorMode in main(). For bench subcommands мы вызываем
/// эту утилиту до сложного command dispatch parsing.
fn should_use_color() -> bool {
    if std::env::var("NO_COLOR").is_ok() { return false; }
    if let Ok(v) = std::env::var("CI") { if !v.is_empty() { return false; } }
    if let Ok(v) = std::env::var("TERM") { if v == "dumb" { return false; } }
    // Pessimistic — на Windows ConPTY часто работает, но безопаснее false.
    std::env::var("CLICOLOR_FORCE").map(|v| !v.is_empty()).unwrap_or(false)
        || std::io::IsTerminal::is_terminal(&std::io::stdout())
}

fn contracts_parse_file(file: &std::path::Path) -> Result<nova_codegen::ast::Module> {
    let src = std::fs::read_to_string(file)
        .map_err(|e| anyhow!("cannot read {}: {}", file.display(), e))?;
    let tokens = nova_codegen::lexer::lex(&src)
        .map_err(|d| anyhow!("{}", d.message))?;
    let mut parser = nova_codegen::parser::Parser::new(tokens);
    parser.parse_module().map_err(|d| anyhow!("{}", d.message))
}

fn cmd_contracts_list(file: &std::path::Path) -> Result<()> {
    let module = contracts_parse_file(file)?;
    let mut items = Vec::new();
    for item in &module.items {
        if let nova_codegen::ast::Item::Fn(fd) = item {
            if fd.contracts.is_empty() { continue; }
            let contracts: Vec<serde_json::Value> = fd.contracts.iter().enumerate().map(|(i, c)| {
                serde_json::json!({
                    "id": i,
                    "kind": format!("{:?}", c.kind).to_lowercase(),
                    "span": { "start": c.span.start, "end": c.span.end }
                })
            }).collect();
            items.push(serde_json::json!({
                "fn": fd.name,
                "verify_mode": format!("{:?}", fd.verify_mode).to_lowercase(),
                "trusted": fd.is_trusted,
                "contracts": contracts
            }));
        }
    }
    let out = serde_json::json!({
        "schema": "nova-contracts-diag/v1",
        "file": file.display().to_string(),
        "module": module.name.join("."),
        "functions": items
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_contracts_verify(file: &std::path::Path, backend: Option<&str>) -> Result<()> {
    if let Some(b) = backend {
        std::env::set_var("NOVA_SMT_BACKEND", b);
    }
    let module = contracts_parse_file(file)?;
    // Run type-check pass to populate types (needed for verify).
    let report = nova_codegen::verify::verify_module(&module);
    let mut diagnostics = Vec::new();
    for (fn_name, span) in &report.proven {
        diagnostics.push(serde_json::json!({
            "kind": "proven",
            "fn": fn_name,
            "span": { "start": span.start, "end": span.end }
        }));
    }
    for diag in &report.warnings {
        diagnostics.push(serde_json::json!({
            "kind": "warning",
            "message": diag.message,
            "span": { "start": diag.span.start, "end": diag.span.end }
        }));
    }
    for diag in &report.errors {
        diagnostics.push(serde_json::json!({
            "kind": "error",
            "message": diag.message,
            "span": { "start": diag.span.start, "end": diag.span.end }
        }));
    }
    let out = serde_json::json!({
        "schema": "nova-contracts-diag/v1",
        "file": file.display().to_string(),
        "module": module.name.join("."),
        "proven_count": report.proven.len(),
        "warning_count": report.warnings.len(),
        "error_count": report.errors.len(),
        "diagnostics": diagnostics
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    if !report.errors.is_empty() {
        return Err(anyhow!("{} contract error(s)", report.errors.len()));
    }
    Ok(())
}

fn cmd_contracts_suggest(file: &std::path::Path, fn_name: &str) -> Result<()> {
    let module = contracts_parse_file(file)?;
    let fd = module.items.iter().find_map(|item| {
        if let nova_codegen::ast::Item::Fn(f) = item {
            if f.name == fn_name { return Some(f); }
        }
        None
    }).ok_or_else(|| anyhow!("function `{}` not found in {}", fn_name, file.display()))?;

    let mut suggestions = Vec::new();
    if fd.contracts.is_empty() {
        // Generate basic stub suggestions based on return type.
        if let Some(rt) = &fd.return_type {
            let rt_str = format!("{rt:?}");
            if rt_str.contains("Bool") {
                suggestions.push("ensures result == true || result == false");
            } else if rt_str.contains("Int") {
                suggestions.push("requires true");
                suggestions.push("ensures result >= 0");
            }
        }
        if suggestions.is_empty() {
            suggestions.push("requires true");
            suggestions.push("ensures true");
        }
    }

    let out = serde_json::json!({
        "schema": "nova-contracts-diag/v1",
        "fn": fn_name,
        "has_contracts": !fd.contracts.is_empty(),
        "suggestions": suggestions
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_contracts_counterexample(file: &std::path::Path, fn_name: &str, contract_id: usize) -> Result<()> {
    let module = contracts_parse_file(file)?;
    let report = nova_codegen::verify::verify_module(&module);
    // Find error for this fn + contract_id.
    let msg = report.errors.iter()
        .find(|d| d.message.contains(&format!("`{fn_name}`")))
        .map(|d| d.message.clone())
        .unwrap_or_else(|| format!("no counterexample found for `{fn_name}` contract {contract_id}"));
    let out = serde_json::json!({
        "schema": "nova-contracts-diag/v1",
        "fn": fn_name,
        "contract_id": contract_id,
        "counterexample": msg
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

// ---------- entry point ----------

/// Точка входа: запускаем всё в потоке с увеличенным стеком (64 MiB).
///
/// AST-обходы (parser, type-checker, codegen emit) взаимно рекурсивны
/// через expr ↔ block ↔ stmt. На Windows стек главного потока по
/// умолчанию мал — для глубоко вложенных выражений и больших модулей
/// (особенно в debug-сборке) этого недостаточно. Spawn с explicit
/// stack_size — тот же паттерн, что в `nova-codegen/src/main.rs`.
fn main() -> ExitCode {
    std::thread::Builder::new()
        .name("nova-main".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .expect("spawn main thread")
        .join()
        .unwrap_or(ExitCode::FAILURE)
}

fn run() -> ExitCode {
    // Plan 36 R7: guarantee exit=101 on panic (cargo convention, cross-platform).
    // Без этого default Rust panic handler даёт 101 на Unix но 0xC0000409 на Windows.
    std::panic::set_hook(Box::new(|info| {
        let msg = info.payload().downcast_ref::<&str>().copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<no message>");
        let loc = info.location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown>".to_string());
        eprintln!("nova: internal error at {}: {}", loc, msg);
        eprintln!("This is a bug in nova. Please report it.");
        std::process::exit(101);
    }));

    let cli = Cli::parse();

    // Plan 36 R10: apply --color setting before any output.
    match ColorMode::parse(&cli.color) {
        Ok(mode) => set_color_mode(mode),
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    }

    let result = match cli.cmd {
        Cmd::Check { paths, jobs, quiet, verbose, list, format, include_runtime, skip } => cmd_check(
            &paths,
            jobs,
            quiet,
            verbose,
            list,
            &format,
            include_runtime,
            &skip,
        ),
        Cmd::Run { file } => cmd_run(&file),
        Cmd::Add { name, path, git, tag, branch, rev, version } => cmd_add(
            &name,
            path.as_deref(),
            git.as_deref(),
            tag.as_deref(),
            branch.as_deref(),
            rev.as_deref(),
            version.as_deref(),
        ),
        Cmd::Update { name, precise } => cmd_update(name.as_deref(), precise.as_deref()),
        Cmd::Info { target, format, diff, fail_on_new } => {
            cmd_info(&target, &format, diff.as_deref(), fail_on_new)
        }
        Cmd::Doc { file, format, json_schema, include_private, run_doc_tests, check, watch, coverage, coverage_threshold, jobs, diff, scrape_examples, strict, mutate_contracts, real_exec, output_dir } => {
            // Plan 45 Ф.24.10: --diff old.json new.json
            // cmd_doc_diff uses process::exit for severity codes; propagate Err.
            if let Some(paths) = diff {
                cmd_doc_diff(&paths[0], &paths[1])
            } else {
            // Plan 45 Ф.23.17: --json-schema works without FILE argument.
            if json_schema && file.is_none() {
                println!("{}", nova_doc_embedded_schema());
                return ExitCode::SUCCESS;
            }
            let path = file.as_deref().unwrap_or_else(|| {
                eprintln!("error: FILE argument required (unless --json-schema)");
                std::process::exit(1);
            });
            cmd_doc(path, &format, json_schema, include_private, run_doc_tests, check, watch, coverage, coverage_threshold, jobs, scrape_examples.as_deref(), strict, mutate_contracts, real_exec, output_dir.as_deref())
            } // else (no --diff)
        }
        Cmd::DocQuery { json_file, query } => cmd_doc_query(&json_file, &query),
        Cmd::DocMcp { file, port } => cmd_doc_mcp(&file, port),
        Cmd::Build { file, output, mode, toolchain, vcvars, clang, timeout, keep_artifacts, mono_depth } => cmd_build(
            &file,
            output.as_deref(),
            &mode,
            &toolchain,
            vcvars.as_deref(),
            clang.as_deref(),
            timeout,
            keep_artifacts,
            mono_depth,
        ),
        Cmd::Test {
            path, filter, jobs, format, mode, toolchain, vcvars, clang, timeout,
            verbose, quiet, results_file, rerun_failed, retries,
            include_stdlib, keep_artifacts, gc,
            list, filter_from, shuffle, skip, mono_depth,
        } => cmd_test(
            path.as_deref(),
            filter.as_deref(),
            jobs,
            &format,
            &mode,
            &toolchain,
            vcvars.as_deref(),
            clang.as_deref(),
            timeout,
            verbose,
            quiet,
            results_file.as_deref(),
            rerun_failed,
            retries,
            include_stdlib,
            keep_artifacts,
            gc.as_deref().unwrap_or("boehm"),
            list,
            filter_from.as_deref(),
            shuffle,
            &skip,
            mono_depth,
        ),
        Cmd::TestBuild { file, mode, toolchain, vcvars, clang, timeout, keep_artifacts, gc, mono_depth } => cmd_test_build(
            &file,
            &mode,
            &toolchain,
            vcvars.as_deref(),
            clang.as_deref(),
            timeout,
            keep_artifacts,
            gc.as_deref().unwrap_or("boehm"),
            mono_depth,
        ),
        Cmd::RegenRuntime { check } => cmd_regen_runtime(check),
        Cmd::Contracts(sub) => cmd_contracts(sub),
        Cmd::Bench(sub) => cmd_bench(sub),
        Cmd::ConsumeAnalyze { path, format, fail_on_uncovered } => {
            cmd_consume_analyze(&path, &format, fail_on_uncovered)
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {}", bold(&red("error:")), e);
            // Plan 36 R7: usage errors → exit=2, diagnostics → exit=1.
            // Discriminate через downcast UsageError type.
            if e.downcast_ref::<UsageError>().is_some() {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE  // = 1
            }
        }
    }
}

#[cfg(test)]
mod plan03_1_tests {
    use super::insert_dependency;

    #[test]
    fn insert_into_existing_section() {
        let t = "[package]\nname = \"x\"\n[dependencies]\nfoo = { path = \"../foo\" }\n";
        let out = insert_dependency(t, "bar", "{ path = \"../bar\" }").unwrap();
        assert!(out.contains("bar = { path = \"../bar\" }"), "out: {}", out);
        assert!(out.contains("foo = { path = \"../foo\" }"), "foo сохранён");
    }

    #[test]
    fn create_section_when_absent() {
        let t = "[package]\nname = \"x\"\n";
        let out = insert_dependency(t, "bar", "{ git = \"u\", tag = \"v1\" }").unwrap();
        assert!(out.contains("[dependencies]"), "секция создана: {}", out);
        assert!(out.contains("bar = { git = \"u\", tag = \"v1\" }"));
    }

    #[test]
    fn duplicate_is_error() {
        let t = "[dependencies]\nfoo = { path = \"../foo\" }\n";
        assert!(insert_dependency(t, "foo", "{ path = \"../x\" }").is_err());
    }

    #[test]
    fn duplicate_check_scoped_to_dependencies() {
        // Ключ `foo` в [package] не блокирует зависимость `foo`.
        let t = "[package]\nfoo = \"bar\"\n[dependencies]\n";
        let out = insert_dependency(t, "foo", "{ path = \"../foo\" }").unwrap();
        assert!(out.contains("foo = { path = \"../foo\" }"), "out: {}", out);
    }
}
