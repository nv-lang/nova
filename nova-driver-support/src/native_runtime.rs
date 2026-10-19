//! Compiler-free native runtime and archive discovery/build helpers.
use crate::{
    bytes_to_string, parse_size_to_bytes, strip_verbatim_prefix, BoehmConfig, BuildOpts, GcKind,
    LibuvConfig, Mode, RuntimeConfig, Toolchain,
};
use anyhow::{anyhow, Result};
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn march_flag_for(native: bool) -> &'static str {
    if native {
        "native"
    } else {
        "x86-64-v3"
    }
}

pub fn march_flag() -> String {
    march_flag_for(std::env::var("NOVA_MARCH_NATIVE").as_deref() == Ok("1")).to_string()
}

pub fn runtime_define_args(runtime: Option<&RuntimeConfig>, prefix: &str) -> Vec<String> {
    let mut args = Vec::new();
    let Some(runtime) = runtime else {
        return args;
    };
    if let Some(value) = &runtime.fiber_stack {
        match parse_size_to_bytes(value) {
            Some(bytes) => args.push(format!("{}NOVA_FIBER_STACK_DEFAULT={}", prefix, bytes)),
            None => eprintln!(
                "nova: warning: [runtime] fiber_stack = \"{}\" unparseable — ignoring (using builtin 4MB default)",
                value
            ),
        }
    }
    if let Some(value) = &runtime.fibers_per_worker {
        match parse_size_to_bytes(value) {
            Some(count) => args.push(format!("{}NOVA_FIBERS_PER_WORKER_DEFAULT={}", prefix, count)),
            None => eprintln!(
                "nova: warning: [runtime] fibers_per_worker = \"{}\" unparseable — ignoring (using builtin 16384 default)",
                value
            ),
        }
    }
    args
}

pub fn effect_count_define_arg(c_file: &Path, prefix: &str) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open(c_file).ok()?;
    let mut first = String::new();
    std::io::BufReader::new(file).read_line(&mut first).ok()?;
    effect_count_define_arg_from_line(&first, prefix)
}

pub fn effect_count_define_arg_from_line(first_line: &str, prefix: &str) -> Option<String> {
    let count: u32 = first_line
        .split("nova-effect-count:")
        .nth(1)?
        .trim()
        .trim_end_matches("*/")
        .trim()
        .parse()
        .ok()?;
    if count == 0 {
        return None;
    }
    Some(format!("{}NOVA_MAX_EFFECT_STORAGES={}", prefix, count))
}

/// Windows system libs needed by libuv (linker dependencies).
#[cfg(target_os = "windows")]
pub const LIBUV_WIN_SYSLIBS: &[&str] = &[
    "ws2_32.lib",
    "iphlpapi.lib",
    "psapi.lib",
    "userenv.lib",
    "user32.lib",
    "shell32.lib",
    "ole32.lib",
    "uuid.lib",
    "advapi32.lib",
    "dbghelp.lib",
];

/// Linux system libs needed by libuv.
#[cfg(target_os = "linux")]
pub const LIBUV_UNIX_SYSLIBS: &[&str] = &["-lpthread", "-ldl", "-lrt", "-lm"];

#[cfg(target_os = "macos")]
pub const LIBUV_UNIX_SYSLIBS: &[&str] = &["-lpthread", "-ldl", "-lm"];

pub fn detect_or_build_libuv(
    rt_dir: &Path,
    repo_root: &Path,
    vcvars: Option<&Path>,
) -> Option<LibuvConfig> {
    let libuv_dir = rt_dir.join("libuv");
    let include_dir = libuv_dir.join("include");
    let uv_h = include_dir.join("uv.h");
    if !uv_h.is_file() {
        // №650: в линкованном worktree сабмодуль пуст ПО ПОСТРОЕНИЮ —
        // `git worktree add` его не материализует, а FATAL с командой,
        // которую человек должен запустить руками, — правило на памяти.
        // Команду из подсказки запускает сам тулчейн: это оффлайн (общий
        // `.git/modules` уже держит клон) и материализует ровно тот коммит,
        // который запинила ЭТА ветка, — копия из соседнего дерева могла бы
        // принести чужой пин. Только когда rt_dir внутри repo_root:
        // standalone-пакету (NOVA_RT_DIR наружу) сабмодуль недоступен,
        // ему — прежний честный FATAL.
        if let Ok(rel) = rt_dir.strip_prefix(repo_root) {
            let sub = format!("{}/libuv", rel.display()).replace('\\', "/");
            eprintln!(
                "nova: libuv submodule empty at {} — running \
                 `git submodule update --init {}` (offline, shared .git)",
                libuv_dir.display(),
                sub
            );
            let _ = std::process::Command::new("git")
                .arg("-C")
                .arg(repo_root)
                .args(["submodule", "update", "--init", "--"])
                .arg(&sub)
                .status();
        }
        if !uv_h.is_file() {
            eprintln!(
                "nova: FATAL libuv submodule not initialized at {}.\n\
                 Plan 22 F2: libuv is mandatory. Run:\n\
                 \tgit submodule update --init compiler-codegen/nova_rt/libuv",
                libuv_dir.display()
            );
            std::process::exit(1);
        }
    }
    let eventloop_src = rt_dir.join("eventloop.c");
    if !eventloop_src.is_file() {
        eprintln!(
            "nova: FATAL eventloop.c not found at {}",
            eventloop_src.display()
        );
        std::process::exit(1);
    }
    // #1134: the bucket is keyed by the environment that produced the objects.
    // Without it the first Linux to build wins forever -- see `libuv_cache_key`.
    let cache_dir = repo_root
        .join("target")
        .join("libuv-cache")
        .join(libuv_cache_key());
    let lib_name = if cfg!(target_os = "windows") {
        "libuv.lib"
    } else {
        "libuv.a"
    };
    let lib_file = cache_dir.join(lib_name);
    // Registry 221.1 #1824: two `nova` processes on a cold cache both built
    // into the one `obj/` (each `remove_dir_all`s it first) and could link a
    // half-written archive. The OS lock is taken before the first look.
    let lock_path = cache_dir.join(".nova-build.lock");
    let _xproc = match crate::fs_lock::lock_exclusive(&lock_path) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!(
                "nova: warning: libuv build: lock {}: {:#}",
                lock_path.display(),
                e
            );
            None
        }
    };
    if lib_file.is_file() {
        return Some(LibuvConfig {
            include_dir,
            lib_file,
            eventloop_src,
        });
    }
    // Build libuv lazy при первом запуске.
    eprintln!("nova: libuv not built, building (one-time, ~30 sec)...");
    if let Err(e) = build_libuv_lib(&libuv_dir, &cache_dir, vcvars) {
        eprintln!(
            "nova: FATAL failed to build libuv: {:#}\n\
             Plan 22 F2: libuv is mandatory. Check vcvars64.bat, \
             cl.exe / clang availability, and libuv submodule integrity.",
            e
        );
        std::process::exit(1);
    }
    if lib_file.is_file() {
        Some(LibuvConfig {
            include_dir,
            lib_file,
            eventloop_src,
        })
    } else {
        eprintln!(
            "nova: FATAL libuv build succeeded but {} not found",
            lib_file.display()
        );
        std::process::exit(1);
    }
}

/// Plan 27 Ф.D (audit 2026-05-12): detect Boehm GC installation with
/// graceful fallback. Returns Some(config) если найден, None — иначе
/// (caller вызывает resolve_gc_or_exit для honest exit).
///
/// Шаг 2b опирается на main_worktree_root (ниже) — реестр №650.
///
/// **Lookup order:**
///
/// 1. `$NOVA_GC_LIB_DIR` (+ optional `$NOVA_GC_INCLUDE_DIR`) — CI/custom override.
/// 2. **Windows:**
///    a. Local vcpkg: `<cg_include>/vcpkg_installed/x64-windows-static/`.
///    b. Main-worktree vcpkg — для линкованного worktree, где
///       `vcpkg_installed` отсутствует по построению (№650).
///    c. Global vcpkg: `$VCPKG_ROOT/installed/x64-windows-static/`.
/// 3. **Linux:** проверяет `gc.h` в стандартных paths — если найден, возвращает
///    Some({include_dir: Some, lib_dir: None}). Иначе None.
/// 4. **macOS:** Homebrew (`/opt/homebrew/include/gc.h` на Apple Silicon или
///    `/usr/local/include/gc.h` на Intel).
pub fn detect_boehm(cg_include: &Path) -> Option<BoehmConfig> {
    // Порядок 2 расширен шагом 2b (главное дерево) — см. main_worktree_root
    // и реестр №650: свежий worktree не должен требовать ручного env-ритуала.
    // 1. Env override (highest priority).
    if let Ok(lib_dir_env) = std::env::var("NOVA_GC_LIB_DIR") {
        let lib_dir = PathBuf::from(&lib_dir_env);
        let include_dir = std::env::var("NOVA_GC_INCLUDE_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                // Авто-вывод include из lib: lib/../include (vcpkg-layout).
                lib_dir
                    .parent()
                    .map(|p| p.join("include"))
                    .filter(|p| p.exists())
            });
        return Some(BoehmConfig {
            include_dir,
            lib_dir: Some(lib_dir),
        });
    }

    // 2. Windows: vcpkg paths.
    #[cfg(target_os = "windows")]
    {
        // 2a. Local vcpkg (current behaviour).
        let local_inc = cg_include
            .join("vcpkg_installed")
            .join("x64-windows-static")
            .join("include");
        let local_lib = cg_include
            .join("vcpkg_installed")
            .join("x64-windows-static")
            .join("lib");
        if local_lib.join("gc.lib").is_file() {
            return Some(BoehmConfig {
                include_dir: Some(local_inc),
                lib_dir: Some(local_lib),
            });
        }
        // 2b. vcpkg ГЛАВНОГО дерева — если мы линкованный worktree (№650).
        // `vcpkg_installed` не под git и в worktree не появляется по
        // построению; до этой ветки каждое окно решало это руками — env на
        // главную репу, ритуал держался памятью. Подмена безопасна (ветка
        // не может нести свой vcpkg) и НЕ молчалива — строка ниже.
        if let Some(repo_root) = cg_include.parent() {
            if let Some(main) = main_worktree_root(repo_root) {
                let main_cg = main.join("compiler-codegen");
                let main_inc = main_cg
                    .join("vcpkg_installed")
                    .join("x64-windows-static")
                    .join("include");
                let main_lib = main_cg
                    .join("vcpkg_installed")
                    .join("x64-windows-static")
                    .join("lib");
                if main_lib.join("gc.lib").is_file() {
                    eprintln!(
                        "nova: worktree has no vcpkg_installed — using Boehm GC \
                         from main worktree {} (NOVA_GC_LIB_DIR overrides)",
                        main_cg.display()
                    );
                    return Some(BoehmConfig {
                        include_dir: Some(main_inc),
                        lib_dir: Some(main_lib),
                    });
                }
            }
        }
        // 2c. Global vcpkg via VCPKG_ROOT.
        if let Ok(vcpkg_root) = std::env::var("VCPKG_ROOT") {
            let global_inc = PathBuf::from(&vcpkg_root)
                .join("installed")
                .join("x64-windows-static")
                .join("include");
            let global_lib = PathBuf::from(&vcpkg_root)
                .join("installed")
                .join("x64-windows-static")
                .join("lib");
            if global_lib.join("gc.lib").is_file() {
                return Some(BoehmConfig {
                    include_dir: Some(global_inc),
                    lib_dir: Some(global_lib),
                });
            }
        }
        return None;
    }

    // 3. Linux: system libgc — проверяем header через известные paths.
    #[cfg(target_os = "linux")]
    {
        let _ = cg_include; // silence unused warning
        let candidates = [
            "/usr/include/gc.h",
            "/usr/include/gc/gc.h",
            "/usr/local/include/gc.h",
        ];
        for c in candidates {
            if std::path::Path::new(c).is_file() {
                // lib_dir None → linker finds via -lgc в standard path.
                let inc = std::path::Path::new(c).parent().map(PathBuf::from);
                return Some(BoehmConfig {
                    include_dir: inc,
                    lib_dir: None,
                });
            }
        }
        return None;
    }

    // 4. macOS: Homebrew paths.
    #[cfg(target_os = "macos")]
    {
        let _ = cg_include;
        let candidates = [
            "/opt/homebrew/include/gc.h", // Apple Silicon
            "/usr/local/include/gc.h",    // Intel
        ];
        for c in candidates {
            if std::path::Path::new(c).is_file() {
                let p = std::path::Path::new(c);
                let inc = p.parent().map(PathBuf::from);
                let lib = p
                    .parent()
                    .and_then(|d| d.parent())
                    .map(|prefix| prefix.join("lib"));
                return Some(BoehmConfig {
                    include_dir: inc,
                    lib_dir: lib,
                });
            }
        }
        return None;
    }

    #[allow(unreachable_code)]
    None
}

/// Корень ГЛАВНОГО рабочего дерева, если `root` — линкованный worktree
/// (реестр №650). У worktree `.git` — ФАЙЛ со строкой
/// `gitdir: <главная>/.git/worktrees/<имя>`; главная репа выводится из неё
/// без вызова git и без переменных окружения.
///
/// ЗАЧЕМ. Не-git-артефакты тулчейна (`vcpkg_installed`) в worktree не
/// появляются по построению, и каждое окно платило за это ручным ритуалом
/// «env на главную репу», который держался памятью (класс плана 254 —
/// правило без механизма; сработал 2026-08-14: пробный замер окна упал
/// ровно на этом).
///
/// ГРАНИЦА, и она принципиальна: через этот корень подменяется только то,
/// чего в worktree НЕ БЫВАЕТ (vcpkg — он не под git, ветка не может нести
/// свой). Исходники и `rt_dir` НЕ подменяются никогда: правка рантайма
/// веткой обязана компилироваться из ветки; молчаливая подмена дерева —
/// класс №283, и там она была вредна именно молчанием. Поэтому каждый
/// fallback по этому корню печатает строку о себе.
pub fn main_worktree_root(root: &Path) -> Option<PathBuf> {
    let git_file = root.join(".git");
    if !git_file.is_file() {
        return None; // главная репа: там `.git` — каталог
    }
    let text = std::fs::read_to_string(&git_file).ok()?;
    let gitdir_raw = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    let gitdir = {
        let p = Path::new(gitdir_raw);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            root.join(p)
        }
    };
    let worktrees = gitdir.parent()?; // …/.git/worktrees
    if worktrees.file_name()?.to_str()? != "worktrees" {
        return None;
    }
    let git_dir = worktrees.parent()?; // …/.git
    if git_dir.file_name()?.to_str()? != ".git" {
        return None;
    }
    let main = git_dir.parent()?;
    if main.is_dir() {
        Some(main.to_path_buf())
    } else {
        None
    }
}

/// #269 Ф.2: `nova_rt` sources use BOTH the flat upstream include convention
/// (`#include <gc.h>` — alloc_boehm.c, runtime.c, driver.c, ...) AND a
/// `gc/`-namespaced one (`#include <gc/gc.h>`, `<gc/gc_mark.h>` —
/// fiber_arena.c/fiber_arena_win.c only). vcpkg's `bdwgc` port happens to
/// install headers BOTH ways (`vcpkg_installed/.../include/gc.h` AND
/// `.../include/gc/gc.h`, confirmed by directory listing — bdwgc's own
/// `CMakeLists.txt` has no `install()` rule at all, so this is a vcpkg-side
/// convention, not upstream's), which is why the existing vcpkg/env path
/// never hit this gap. The raw bdwgc submodule tree only has the flat form
/// (`gc_dir/include/gc.h`, no nested `gc/` folder) — copy the handful of
/// headers `nova_rt` actually needs into `cache_dir/include/` (flat, for
/// `<gc.h>`) AND `cache_dir/include/gc/` (nested, for `<gc/gc.h>`) so a
/// single `-I cache_dir/include` satisfies both forms, matching vcpkg's
/// observed layout — found by running the actual clean-clone acceptance
/// gate (`fiber_arena_win.c(55): fatal error C1083: ... gc/gc.h: No such
/// file or directory`), not by reading the vcpkg port source ahead of time.
/// Copies into the submodule's OWN `include/` dir are deliberately avoided
/// (never write into a vendored git submodule's working tree — would show
/// up as an unexpected dirty submodule / risk being committed by accident).
fn populate_boehm_include_dir(gc_include: &Path, cache_include: &Path) -> Result<()> {
    let nested = cache_include.join("gc");
    std::fs::create_dir_all(&nested)
        .map_err(|e| anyhow!("create {}: {:#}", nested.display(), e))?;
    for entry in std::fs::read_dir(gc_include)
        .map_err(|e| anyhow!("read {}: {:#}", gc_include.display(), e))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("h") {
            continue;
        }
        let name = path.file_name().unwrap();
        std::fs::copy(&path, cache_include.join(name))
            .map_err(|e| anyhow!("copy {} (flat): {:#}", path.display(), e))?;
        std::fs::copy(&path, nested.join(name))
            .map_err(|e| anyhow!("copy {} (nested gc/): {:#}", path.display(), e))?;
    }
    Ok(())
}

/// #269 Ф.2 [M-gc-lib-not-bundled-clean-install]: one-time fallback build of
/// Boehm GC from the vendored `bdwgc` submodule (`rt_dir/gc`) — mirrors
/// `detect_or_build_libuv` 1:1. Called ONLY when `detect_boehm` (env var /
/// vcpkg lookup, unchanged priority) returns `None` — a clean clone with no
/// vcpkg installed no longer dead-ends on a FATAL, it self-builds instead,
/// exactly like libuv already does.
///
/// **Windows only** in this window (Ф.2 scope decision, see PROGRESS-gc269.md):
/// bdwgc's official single-file amalgamation (`extra/gc.c`, includes every
/// other `.c` in the tree via relative `"../foo.c"` quote-includes — no
/// cmake needed, just compile this ONE file) needs a real atomics backend
/// on MSVC (`cl.exe` has no `__atomic_*` GCC/clang builtins), which bdwgc's
/// own `include/private/gc_atomic_ops.h` falls back to only via the
/// external `libatomic_ops` project's `atomic_ops.h` (confirmed against
/// bdwgc's own `CMakeLists.txt`: `if (... OR MSVC ...) include_directories
/// (libatomic_ops/src)`, and against the vcpkg `bdwgc` port, which pulls in
/// vcpkg's separate `libatomic-ops` port for exactly this reason) — hence
/// the SECOND submodule `rt_dir/libatomic_ops` (pinned v7.8.2, matching the
/// version found in this repo's own vcpkg cache). On x86_64 the needed
/// primitives are header-only (MSVC `_Interlocked*` intrinsics inline in
/// `atomic_ops.h` — confirmed empirically: linking against a `gc.lib` built
/// this way pulls in ZERO unresolved externals without any separately
/// compiled `atomic_ops.lib`; bdwgc's own CMakeLists agrees, leaving
/// `ATOMIC_OPS_LIBS` empty with a "assume library not needed" comment), so
/// only the header directory is needed — no second archive to build/link.
/// Linux/macOS already has a working zero-vcpkg path (`apt install
/// libgc-dev` / `brew install bdw-gc`, existing `detect_boehm` branches
/// below) and gcc/clang provide `__atomic_*` builtins directly
/// (`GC_BUILTIN_ATOMIC`), so a from-source fallback isn't the blocking gap
/// there — left for a follow-up window if a vcpkg-equivalent gap ever shows
/// up on those platforms (not verified end-to-end on this window's
/// Windows-only dev machine; explicitly NOT claimed done here).
///
/// Cache: `repo_root/target/gc-cache/gc.lib` (+ obj scratch dir, cleaned up
/// after build) — same `target/`-rooted convention as `libuv-cache`.
/// Idempotent/cheap on repeat calls (disk `is_file()` check short-circuits
/// before ever touching the compiler), safe to call from both the early
/// honest-exit check (`resolve_gc_or_exit`) and `build_command`'s own
/// flag-derivation block — see call sites.
pub fn detect_or_build_boehm_fallback(
    rt_dir: &Path,
    repo_root: &Path,
    vcvars: Option<&Path>,
) -> Option<BoehmConfig> {
    #[cfg(target_os = "windows")]
    {
        let gc_dir = rt_dir.join("gc");
        let ao_dir = rt_dir.join("libatomic_ops");
        let gc_include = gc_dir.join("include");
        let gc_amalgam = gc_dir.join("extra").join("gc.c");
        let ao_header = ao_dir.join("src").join("atomic_ops.h");
        if !gc_include.join("gc.h").is_file() || !gc_amalgam.is_file() || !ao_header.is_file() {
            // Submodule(s) not initialized — not fatal here; the caller's
            // combined FATAL message (resolve_gc_or_exit) names both the
            // vcpkg AND the submodule-init remedy.
            return None;
        }
        let cache_dir = repo_root.join("target").join("gc-cache");
        let gc_lib = cache_dir.join("gc.lib");
        let cache_include = cache_dir.join("include");
        // Registry 221.1 #1824: the same cross-process lock as libuv's.
        let lock_path = cache_dir.join(".nova-build.lock");
        let _xproc = match crate::fs_lock::lock_exclusive(&lock_path) {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!(
                    "nova: warning: bdwgc build: lock {}: {:#}",
                    lock_path.display(),
                    e
                );
                None
            }
        };
        if gc_lib.is_file()
            && cache_include.join("gc.h").is_file()
            && cache_include.join("gc").join("gc.h").is_file()
        {
            return Some(BoehmConfig {
                include_dir: Some(cache_include),
                lib_dir: Some(cache_dir),
            });
        }
        eprintln!(
            "nova: Boehm GC (gc.lib) not found via $NOVA_GC_LIB_DIR/vcpkg — \
             building from vendored bdwgc submodule (one-time, ~10 sec)..."
        );
        if let Err(e) = populate_boehm_include_dir(&gc_include, &cache_include) {
            eprintln!(
                "nova: warning: bdwgc fallback build failed: copy headers: {:#}",
                e
            );
            return None;
        }
        if let Err(e) = build_boehm_lib(&gc_dir, &ao_dir, &cache_dir, vcvars) {
            eprintln!("nova: warning: bdwgc fallback build failed: {:#}", e);
            return None;
        }
        if gc_lib.is_file() {
            eprintln!(
                "nova: gc.lib built from vendored bdwgc source ({})",
                gc_lib.display()
            );
            return Some(BoehmConfig {
                include_dir: Some(cache_include),
                lib_dir: Some(cache_dir),
            });
        }
        eprintln!(
            "nova: FATAL bdwgc fallback build succeeded but {} not found",
            gc_lib.display()
        );
        return None;
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Ф.2 scope decision: Linux/macOS keep the existing apt/brew-based
        // `detect_boehm` path unchanged — see fn doc above for why this
        // isn't the blocking gap on those platforms in this window.
        let _ = (rt_dir, repo_root, vcvars);
        None
    }
}

/// Plan 27 Ф.D + #269 Ф.2: если backend = Boehm, проверяет наличие через
/// `detect_boehm` (env var / vcpkg — unchanged priority), затем — #269 Ф.2 —
/// пытается one-time fallback build из вендорённого bdwgc-сабмодуля
/// (`detect_or_build_boehm_fallback`) ПЕРЕД honest fatal. На fail печатает
/// platform-specific install hint и завершает процесс. Возвращает
/// Some(BoehmConfig) если backend = Boehm и detection (или fallback build)
/// OK, None если backend = Malloc (Boehm не нужен).
///
/// `rt_dir`: `compiler-codegen/nova_rt` — anchors the `gc`/`libatomic_ops`
/// submodule lookup AND (via `.parent().parent()`) `repo_root` for the
/// `target/gc-cache` build cache, mirrors `detect_or_build_libuv`'s own
/// `rt_dir`/`repo_root` pair. `vcvars`: passed straight through to the
/// fallback builder's `cl.exe`/`lib.exe` invocations (Windows only).
pub fn resolve_gc_or_exit(
    gc: GcKind,
    cg_include: &Path,
    rt_dir: &Path,
    vcvars: Option<&Path>,
) -> Option<BoehmConfig> {
    if gc != GcKind::Boehm {
        return None;
    }
    if let Some(cfg) = detect_boehm(cg_include) {
        return Some(cfg);
    }
    if let Some(repo_root) = rt_dir.parent().and_then(|p| p.parent()) {
        if let Some(cfg) = detect_or_build_boehm_fallback(rt_dir, repo_root, vcvars) {
            return Some(cfg);
        }
    }
    // Honest fatal с platform-specific hint.
    #[cfg(target_os = "windows")]
    eprintln!(
        "nova: FATAL Boehm GC (gc.lib) not found.\n\
         \n\
         Lookup order tried:\n\
           1. $NOVA_GC_LIB_DIR env var\n\
           2. {}\\vcpkg_installed\\x64-windows-static\\lib\\gc.lib\n\
           3. $VCPKG_ROOT\\installed\\x64-windows-static\\lib\\gc.lib\n\
           4. vendored bdwgc submodule fallback build ({}\\gc\\extra\\gc.c)\n\
         \n\
         To fix (pick one):\n\
           cd compiler-codegen && vcpkg install bdwgc:x64-windows-static\n\
           git submodule update --init compiler-codegen/nova_rt/gc \\\n\
                                        compiler-codegen/nova_rt/libatomic_ops\n\
         \n\
         Or use --gc malloc for benchmarks (no GC, leaks).",
        cg_include.display(),
        rt_dir.display()
    );
    #[cfg(target_os = "linux")]
    eprintln!(
        "nova: FATAL Boehm GC (libgc) not found.\n\
         \n\
         Header `gc.h` not present in /usr/include, /usr/local/include.\n\
         \n\
         To fix:\n\
           sudo apt install libgc-dev        # Debian/Ubuntu\n\
           sudo dnf install gc-devel         # Fedora/RHEL\n\
           sudo pacman -S gc                 # Arch\n\
         \n\
         Or use --gc malloc for benchmarks (no GC, leaks)."
    );
    #[cfg(target_os = "macos")]
    eprintln!(
        "nova: FATAL Boehm GC (libgc) not found.\n\
         \n\
         Header `gc.h` not present in /opt/homebrew/include or /usr/local/include.\n\
         \n\
         To fix:\n\
           brew install bdw-gc\n\
         \n\
         Or use --gc malloc for benchmarks (no GC, leaks)."
    );
    std::process::exit(1);
}

/// Plan 22 Ф.1: compile libuv source files в libuv.lib / libuv.a.
/// Кэшируется в repo_root/target/libuv-cache/ через VERSION stamp.
fn build_libuv_lib(libuv_dir: &Path, cache_dir: &Path, vcvars: Option<&Path>) -> Result<()> {
    std::fs::create_dir_all(cache_dir).map_err(|e| anyhow!("create cache_dir: {:#}", e))?;
    let obj_dir = cache_dir.join("obj");
    if obj_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&obj_dir);
    }
    std::fs::create_dir_all(&obj_dir).map_err(|e| anyhow!("create obj_dir: {:#}", e))?;

    // Collect source files: src/*.c + src/{win,unix}/*.c.
    let mut srcs: Vec<PathBuf> = Vec::new();
    let src_root = libuv_dir.join("src");
    collect_c_files(&src_root, &mut srcs, /*recursive*/ false)?;
    #[cfg(target_os = "windows")]
    {
        collect_c_files(&src_root.join("win"), &mut srcs, /*recursive*/ false)?;
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        /* libuv puts platform-specific impls в src/unix/ как отдельные .c
         * (linux.c, freebsd.c, openbsd.c, darwin.c, sunos.c, aix.c,
         * ibmi.c, os390.c, ...). Whitelist approach: компилируем common
         * unix files + platform-specific subset. См. libuv CMakeLists.txt
         * для reference list.
         */
        const COMMON_UNIX: &[&str] = &[
            "async.c",
            "core.c",
            "dl.c",
            "fs.c",
            "getaddrinfo.c",
            "getnameinfo.c",
            "loop-watcher.c",
            "loop.c",
            "pipe.c",
            "poll.c",
            "process.c",
            "random-devurandom.c",
            "signal.c",
            "stream.c",
            "tcp.c",
            "thread.c",
            "tty.c",
            "udp.c",
        ];
        #[cfg(target_os = "linux")]
        const PLATFORM_FILES: &[&str] = &[
            "linux.c",
            "procfs-exepath.c",
            "proctitle.c",
            "random-getrandom.c",
            "random-sysctl-linux.c",
            "no-fsevents.c",
            /* hrtime: linux.c provides uv__hrtime; не подключаем posix-hrtime.c */
        ];
        #[cfg(target_os = "macos")]
        const PLATFORM_FILES: &[&str] = &[
            "darwin.c",
            "darwin-proctitle.c",
            "kqueue.c",
            "fsevents.c",
            "bsd-ifaddrs.c",
            "bsd-proctitle.c",
            "random-getentropy.c",
            "posix-hrtime.c", /* macOS uses generic POSIX hrtime */
        ];

        let unix_dir = src_root.join("unix");
        for name in COMMON_UNIX.iter().chain(PLATFORM_FILES.iter()) {
            let p = unix_dir.join(name);
            if p.is_file() {
                srcs.push(p);
            }
        }
    }
    if srcs.is_empty() {
        return Err(anyhow!(
            "no libuv source files found in {}",
            src_root.display()
        ));
    }

    let inc_pub = libuv_dir.join("include");
    let inc_src = libuv_dir.join("src");
    let inc_win = libuv_dir.join("src").join("win");

    #[cfg(target_os = "windows")]
    {
        let vcv = vcvars.ok_or_else(|| anyhow!("vcvars required for libuv build on Windows"))?;
        // Write response file (cl.exe @file).
        let rsp = cache_dir.join("compile.rsp");
        let mut lines: Vec<String> = Vec::new();
        lines.push("/c /nologo /W0 /MT /O2 /D_WIN32_WINNT=0x0602 /DWIN32_LEAN_AND_MEAN /DBUILDING_UV_SHARED=0".to_string());
        lines.push(format!("/I \"{}\"", inc_pub.display()));
        lines.push(format!("/I \"{}\"", inc_src.display()));
        lines.push(format!("/I \"{}\"", inc_win.display()));
        lines.push(format!("/Fo\"{}\\\\\"", obj_dir.display()));
        for s in &srcs {
            lines.push(format!("\"{}\"", s.display()));
        }
        // №287: UTF-8 BOM — cl.exe decodes a BOM-less response file under the
        // process ANSI codepage, so a non-ASCII path (a Cyrillic user
        // directory, say) is mangled and every source after it fails with a
        // spurious `C1083: file not found`. Same fix `link_prep.rs` already
        // carries; this sibling never got it.
        std::fs::write(&rsp, format!("\u{FEFF}{}", lines.join("\n")))
            .map_err(|e| anyhow!("write rsp: {:#}", e))?;
        let inner = format!(
            "\"call \"{}\" >nul 2>&1 && cl.exe @\"{}\"\"",
            vcv.display(),
            rsp.display()
        );
        let mut cmd = Command::new("cmd");
        #[cfg(target_os = "windows")]
        {
            cmd.raw_arg("/c").raw_arg(&inner);
        }
        let out = cmd.output().map_err(|e| anyhow!("spawn cl.exe: {:#}", e))?;
        if !out.status.success() {
            let combined = format!(
                "{}{}",
                bytes_to_string(&out.stdout),
                bytes_to_string(&out.stderr)
            );
            return Err(anyhow!(
                "libuv compile failed: {}",
                combined.lines().take(10).collect::<Vec<_>>().join("\n")
            ));
        }
        // Archive all .obj into libuv.lib через lib.exe.
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&obj_dir)? {
            let p = entry?.path();
            if p.extension().and_then(|s| s.to_str()) == Some("obj") {
                obj_files.push(p);
            }
        }
        let lib_file = cache_dir.join("libuv.lib");
        let lib_rsp = cache_dir.join("lib.rsp");
        let mut lib_lines: Vec<String> = Vec::new();
        lib_lines.push("/nologo".to_string());
        lib_lines.push(format!("/OUT:\"{}\"", lib_file.display()));
        for o in &obj_files {
            lib_lines.push(format!("\"{}\"", o.display()));
        }
        // №287: BOM — see the compile.rsp comment above (same non-ASCII-path
        // mangling applies to lib.exe response files).
        std::fs::write(&lib_rsp, format!("\u{FEFF}{}", lib_lines.join("\n")))
            .map_err(|e| anyhow!("write lib.rsp: {:#}", e))?;
        let lib_inner = format!(
            "\"call \"{}\" >nul 2>&1 && lib.exe @\"{}\"\"",
            vcv.display(),
            lib_rsp.display()
        );
        let mut lib_cmd = Command::new("cmd");
        lib_cmd.raw_arg("/c").raw_arg(&lib_inner);
        let lib_out = lib_cmd
            .output()
            .map_err(|e| anyhow!("spawn lib.exe: {:#}", e))?;
        if !lib_out.status.success() {
            return Err(anyhow!(
                "lib.exe failed: {}",
                bytes_to_string(&lib_out.stderr)
            ));
        }
        eprintln!("nova: libuv.lib built ({} files)", srcs.len());
        return Ok(());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        // Linux/macOS: compile через cc → object files → ar archive.
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for src in &srcs {
            let obj = obj_dir.join(
                src.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .replace(".c", ".o"),
            );
            let mut c = Command::new(&cc);
            c.args(["-c", "-O2", "-w", "-fPIC"]);
            c.arg("-D_GNU_SOURCE");
            c.arg("-I").arg(&inc_pub);
            c.arg("-I").arg(&inc_src);
            c.arg("-o").arg(&obj);
            c.arg(src);
            let out = c.output().map_err(|e| anyhow!("spawn {}: {:#}", cc, e))?;
            if !out.status.success() {
                return Err(anyhow!(
                    "libuv compile failed on {}: {}",
                    src.display(),
                    bytes_to_string(&out.stderr)
                ));
            }
            obj_files.push(obj);
        }
        let lib_file = cache_dir.join("libuv.a");
        let mut ar = Command::new("ar");
        ar.arg("rcs").arg(&lib_file);
        for o in &obj_files {
            ar.arg(o);
        }
        let ar_out = ar.output().map_err(|e| anyhow!("spawn ar: {:#}", e))?;
        if !ar_out.status.success() {
            return Err(anyhow!("ar failed: {}", bytes_to_string(&ar_out.stderr)));
        }
        eprintln!("nova: libuv.a built ({} files)", srcs.len());
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (libuv_dir, cache_dir, vcvars);
        Err(anyhow!("unsupported platform for libuv build"))
    }
}

/// #269 Ф.2: compile bdwgc's official single-file amalgamation
/// (`gc_dir/extra/gc.c` — includes every other `.c` in `gc_dir` via
/// relative `"../foo.c"` quote-includes, so compiling this ONE file is the
/// entire build, no cmake needed) into `gc.lib`. Mirrors `build_libuv_lib`'s
/// shape (rsp file, `cl.exe` under vcvars, `lib.exe` archive) — Windows
/// only, see `detect_or_build_boehm_fallback`'s doc for why Linux/macOS
/// aren't wired to this in this window.
///
/// Defines mirror EXACTLY what this repo's own vcpkg cache used to build
/// the `bdwgc` port for `x64-windows-static` (extracted from
/// `vcpkg_installed/.../blds/bdwgc/config-x64-windows-static-rel-ninja.log`
/// `DEFINES = ...` line) — proven byte-compatible by direct empirical
/// comparison: a `gc.lib` built here and one built by vcpkg both link a
/// throwaway `GC_INIT`/`GC_MALLOC`/`GC_gcollect` C program with ZERO
/// unresolved externals and both produce identical runtime behavior
/// (single- and multi-threaded `GC_register_my_thread`/`GC_MALLOC`
/// smoke test, verified in this window's PROGRESS-gc269.md). `/I
/// ao_dir/src` supplies bdwgc's own `include/private/gc_atomic_ops.h`
/// fallback-branch dependency (`#include "atomic_ops.h"`, needed on real
/// MSVC — `cl.exe` has no GCC/clang `__atomic_*` builtins for
/// `GC_BUILTIN_ATOMIC`) — header-only on x86_64, no separate
/// `atomic_ops.lib` to build (see caller's doc).
fn build_boehm_lib(
    gc_dir: &Path,
    ao_dir: &Path,
    cache_dir: &Path,
    vcvars: Option<&Path>,
) -> Result<()> {
    std::fs::create_dir_all(cache_dir).map_err(|e| anyhow!("create cache_dir: {:#}", e))?;
    let obj_dir = cache_dir.join("obj");
    if obj_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&obj_dir);
    }
    std::fs::create_dir_all(&obj_dir).map_err(|e| anyhow!("create obj_dir: {:#}", e))?;

    let gc_amalgam = gc_dir.join("extra").join("gc.c");
    let gc_include = gc_dir.join("include");
    let ao_include = ao_dir.join("src");

    #[cfg(target_os = "windows")]
    {
        let vcv =
            vcvars.ok_or_else(|| anyhow!("vcvars required for bdwgc fallback build on Windows"))?;
        let rsp = obj_dir.join("compile.rsp");
        let defines = "/DDONT_USE_USER32_DLL /DEMPTY_GETENV_RESULTS /DENABLE_DISCLAIM \
                        /DGC_ATOMIC_UNCOLLECTABLE /DGC_ENABLE_SUSPEND_THREAD /DGC_GCJ_SUPPORT \
                        /DGC_MISSING_EXECINFO_H /DGC_NOT_DLL /DGC_NO_SIGSETJMP /DGC_THREADS \
                        /DJAVA_FINALIZATION /DNO_GETCONTEXT /DPARALLEL_MARK /DTHREAD_LOCAL_ALLOC \
                        /D_CRT_SECURE_NO_DEPRECATE";
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("/c /nologo /W0 /MT /O2 {}", defines));
        lines.push(format!(
            "/I \"{}\"",
            strip_verbatim_prefix(&gc_include).display()
        ));
        lines.push(format!(
            "/I \"{}\"",
            strip_verbatim_prefix(&ao_include).display()
        ));
        lines.push(format!(
            "/Fo\"{}\\\\\"",
            strip_verbatim_prefix(&obj_dir).display()
        ));
        lines.push(format!(
            "\"{}\"",
            strip_verbatim_prefix(&gc_amalgam).display()
        ));
        // BOM — same non-ASCII-path codepage-misdecode risk as
        // `link_prep.rs`'s rsp files (see that module's matching comment);
        // this repo's own user profile path can contain Cyrillic.
        std::fs::write(&rsp, format!("\u{FEFF}{}", lines.join("\n")))
            .map_err(|e| anyhow!("write rsp: {:#}", e))?;
        let inner = format!(
            "\"call \"{}\" >nul 2>&1 && cl.exe @\"{}\"\"",
            vcv.display(),
            rsp.display()
        );
        let mut cmd = Command::new("cmd");
        cmd.raw_arg("/c").raw_arg(&inner);
        let out = cmd.output().map_err(|e| anyhow!("spawn cl.exe: {:#}", e))?;
        if !out.status.success() {
            let combined = format!(
                "{}{}",
                bytes_to_string(&out.stdout),
                bytes_to_string(&out.stderr)
            );
            return Err(anyhow!(
                "bdwgc amalgamation compile failed: {}",
                combined.lines().take(15).collect::<Vec<_>>().join("\n")
            ));
        }
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&obj_dir)? {
            let p = entry?.path();
            if p.extension().and_then(|s| s.to_str()) == Some("obj") {
                obj_files.push(p);
            }
        }
        if obj_files.is_empty() {
            return Err(anyhow!("bdwgc amalgamation compile produced no .obj files"));
        }
        let lib_file = cache_dir.join("gc.lib");
        let lib_rsp = obj_dir.join("lib.rsp");
        let mut lib_lines: Vec<String> = Vec::new();
        lib_lines.push("/nologo".to_string());
        lib_lines.push(format!(
            "/OUT:\"{}\"",
            strip_verbatim_prefix(&lib_file).display()
        ));
        for o in &obj_files {
            lib_lines.push(format!("\"{}\"", strip_verbatim_prefix(o).display()));
        }
        std::fs::write(&lib_rsp, format!("\u{FEFF}{}", lib_lines.join("\n")))
            .map_err(|e| anyhow!("write lib.rsp: {:#}", e))?;
        let lib_inner = format!(
            "\"call \"{}\" >nul 2>&1 && lib.exe @\"{}\"\"",
            vcv.display(),
            lib_rsp.display()
        );
        let mut lib_cmd = Command::new("cmd");
        lib_cmd.raw_arg("/c").raw_arg(&lib_inner);
        let lib_out = lib_cmd
            .output()
            .map_err(|e| anyhow!("spawn lib.exe: {:#}", e))?;
        if !lib_out.status.success() {
            return Err(anyhow!(
                "lib.exe failed: {}",
                bytes_to_string(&lib_out.stderr)
            ));
        }
        eprintln!("nova: gc.lib built (bdwgc extra/gc.c amalgamation)");
        let _ = std::fs::remove_dir_all(&obj_dir);
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (
            gc_amalgam, gc_include, ao_include, cache_dir, vcvars, &obj_dir,
        );
        Err(anyhow!("unsupported platform for bdwgc fallback build"))
    }
}

// `pub(crate)` (Ф.1 #268 link-prep extraction, 2026-08-02): shared by both
// `build_libuv_lib` (below, unchanged) and `link_prep::build_missing_vendor_ffi_libs`.
pub fn collect_c_files(dir: &Path, out: &mut Vec<PathBuf>, recursive: bool) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries =
        std::fs::read_dir(dir).map_err(|e| anyhow!("read_dir {}: {:#}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| anyhow!("read_dir entry: {:#}", e))?;
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                collect_c_files(&path, out, true)?;
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("c") {
            out.push(path);
        }
    }
    Ok(())
}

// ---------- Plan 218: prebuilt runtime archive (libnova_rt) ----------
//
// C-compiling a fresh nova program pays ~6.5s recompiling the ~10-12
// nova_rt/*.c translation units on EVERY build (parsing windows.h/uv.h/gc.h
// dominates — docs/plans/218-prebuilt-runtime-archive.md). Mirrors
// `detect_or_build_libuv` above: compile the runtime .c files ONCE into a
// static archive, cache it keyed by content+flags, link the archive on
// subsequent builds instead of recompiling. Purely a build-latency
// optimization — any failure here (disabled, build error, missing tool)
// falls back to the pre-218 per-build inline-compile path (`build_command`'s
// existing behavior), never turns a build that used to succeed into a
// failure.
//
// **ABI hazard found while implementing this (not flagged by the original
// latency research):** `effects.c` DEFINES the actual TLS storage for
// `NovaEffectRegistry`, and `runtime.c` allocates/consumes
// `NovaEffectSnapshot` by `sizeof`. Both types are sized by the
// **per-program** `-DNOVA_MAX_EFFECT_STORAGES=N` marker (Plan 174.4 — see
// effects.h's doc-comment above `NOVA_MAX_EFFECT_STORAGES`): `N` = distinct
// effects USED BY THIS PROGRAM, read from a comment on line 1 of the
// generated `.c` by `effect_count_define_arg`. That define is deliberately
// applied to the WHOLE cc invocation (not `#define`d inside one `.c`)
// specifically so `NovaEffectRegistry`/`NovaEffectSnapshot` have an
// IDENTICAL layout in every TU compiled together. Precompiling
// `effects.c`/`runtime.c` into one FIXED-N archive and linking it against a
// DIFFERENT-N app.c would silently violate that invariant —
// `NovaEffectRegistry.count`'s byte offset shifts with N, and
// `NovaEffectSnapshot`-sized heap allocations could be undersized — a
// classic silent memory-corruption bug, not a link error. Same reasoning
// applies to `[runtime]` `fiber_stack`/`fibers_per_worker` overrides
// (`runtime_define_args`) baked into `fiber_arena.c`/`fiber_arena_win.c` —
// not memory-unsafe, but a behavior difference (wrong default stack/fiber
// count), which would violate the byte-identical-behavior gate.
//
// Fix: `rt_archive_key` below folds `N` and the runtime-defines into the
// bucket key — a program only ever links against an archive built with ITS
// OWN effect count / runtime overrides. This still captures the dominant
// dev-loop win (editing and rebuilding the SAME program repeatedly never
// changes its effect count) and, in practice, most programs share `N`
// (built-ins Fail+Time only, no custom effects) so cross-program reuse
// still happens routinely.

/// Resolved lookup/build result for one archive bucket.
#[derive(Clone)]
pub struct RtArchiveConfig {
    pub lib_file: PathBuf,
}

/// `NOVA_RT_ARCHIVE=0`/`off`/`false` disables — falls back to the pre-218
/// per-build inline compile of every `nova_rt/*.c` (escape hatch, symmetric
/// to `NOVA_CACHE=0` in `build_cache.rs`). Default: enabled.
fn rt_archive_enabled() -> bool {
    !matches!(
        std::env::var("NOVA_RT_ARCHIVE").as_deref(),
        Ok("0") | Ok("off") | Ok("false")
    )
}

/// The `nova_rt/*.c` translation units folded into the archive — exactly
/// the set `build_command` otherwise adds as individual source args (mirrors
/// the `rt_alloc..rt_segv_diag` + conditional `rt_net`/`rt_fs`/eventloop.c
/// list there 1:1 — keep in sync on future nova_rt additions). The
/// GC-backend alloc file varies with `gc_kind` (part of the bucket key).
fn rt_archive_sources(rt_dir: &Path, gc_kind: GcKind, libuv: Option<&LibuvConfig>) -> Vec<PathBuf> {
    let mut v = vec![
        rt_dir.join(gc_kind.alloc_c_name()),
        rt_dir.join("effects.c"),
        rt_dir.join("fibers.c"),
        rt_dir.join("fiber_arena.c"),
        rt_dir.join("fiber_arena_win.c"),
        rt_dir.join("fiber_stats.c"),
        rt_dir.join("runtime.c"),
        rt_dir.join("driver.c"),
        rt_dir.join("typeid.c"),
        rt_dir.join("segv_diag.c"),
    ];
    if let Some(uv) = libuv {
        v.push(rt_dir.join("net.c"));
        v.push(rt_dir.join("fs.c"));
        // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
        v.push(rt_dir.join("process.c"));
        v.push(uv.eventloop_src.clone());
    }
    v
}

/// Every `nova_rt/*.{c,h}` file that participates in content-hashing for
/// invalidation (Plan 218 requirement: hash CONTENT, never mtime — a
/// `touch` without real changes must NOT bust the cache). Broader than
/// `rt_archive_sources` on purpose: every archived `.c` `#include`s a chain
/// of `nova_rt/*.h` — a header-only edit (e.g. `fibers.h`) must invalidate
/// the archive even though no `.c` itself changed. Only files directly
/// inside `rt_dir` — does NOT recurse into `rt_dir/libuv` (that submodule
/// has its own independent cache/lifecycle, see `detect_or_build_libuv`).
fn rt_hashable_files(rt_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(rt_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            continue;
        }
        match p.extension().and_then(|s| s.to_str()) {
            Some("c") | Some("h") => out.push(p),
            _ => {}
        }
    }
    out.sort();
    out
}

/// Fingerprint (size + mtime-nanos) of the archive-builder compiler binary
/// — mirrors `build_cache.rs`'s compiler-exe fingerprint pattern. A
/// toolchain upgrade (new clang/cl.exe) must bust the archive.
fn rt_archive_compiler_fingerprint(cc_path: &Path) -> Option<(u64, u128)> {
    let meta = std::fs::metadata(cc_path).ok()?;
    let nanos = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some((meta.len(), nanos))
}

/// Content-addressed key for one archive bucket. Every dimension that can
/// change the actual bytes/behavior of the compiled objects is folded in —
/// see the module doc above for the effect-count/runtime-define hazard
/// that isn't obvious from `nova_rt/*.c` alone. Separate buckets (Plan 218
/// requirement): dev vs release, GC boehm|malloc, platform (via
/// `std::env::consts::OS`), libuv on/off, per-program effect count, per-
/// package `[runtime]` overrides, release march. Returns `None` only if a
/// hashable file vanished mid-read (races with a concurrent edit) — caller
/// treats that exactly like "disabled" (falls back, non-fatal).
fn rt_archive_key(
    rt_dir: &Path,
    mode: Mode,
    gc_kind: GcKind,
    libuv_present: bool,
    effect_define: Option<&str>,
    runtime_defines: &[String],
    cc_fingerprint: Option<(u64, u128)>,
) -> Option<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    "nova-rt-archive-v1".hash(&mut h);
    format!("{:?}", mode).hash(&mut h);
    format!("{:?}", gc_kind).hash(&mut h);
    std::env::consts::OS.hash(&mut h);
    libuv_present.hash(&mut h);
    effect_define.hash(&mut h);
    runtime_defines.hash(&mut h);
    if matches!(mode, Mode::Release) {
        march_flag().hash(&mut h);
    }
    cc_fingerprint.hash(&mut h);

    let files = rt_hashable_files(rt_dir);
    files.len().hash(&mut h);
    for f in &files {
        f.to_string_lossy().hash(&mut h);
        let bytes = std::fs::read(f).ok()?;
        bytes.hash(&mut h);
    }
    Some(format!("{:016x}", h.finish()))
}

/// Process-wide memoization: `nova test`/`nova bench` build MANY files in
/// one process, each calling `detect_or_build_rt_archive` — without this,
/// every single build would re-read+re-hash ~1-1.5MB of `nova_rt/*.{c,h}`
/// content (cheap once, wasteful hundreds of times over one test run).
/// Keyed by the CHEAP dimensions only (no content hash) — nova_rt content
/// on disk cannot change mid-process for a single `nova test`/`nova build`
/// invocation (same non-goal as `build_cache.rs`, which makes the same
/// assumption for its own per-process source reads).
static RT_ARCHIVE_MEMO: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, Option<RtArchiveConfig>>>,
> = std::sync::OnceLock::new();

fn rt_archive_memo_key(
    rt_dir: &Path,
    mode: Mode,
    gc_kind: GcKind,
    libuv_present: bool,
    effect_define: Option<&str>,
    runtime_defines: &[String],
) -> String {
    format!(
        "{}|{:?}|{:?}|{}|{:?}|{:?}",
        rt_dir.display(),
        mode,
        gc_kind,
        libuv_present,
        effect_define,
        runtime_defines
    )
}

/// Resolve the fixed archive-builder compiler path for fingerprinting
/// (`rt_archive_compiler_fingerprint`). On Windows: search the vcvars-
/// captured `PATH` (from `tc`'s env snapshot — the current PROCESS's own
/// `PATH` does NOT have `cl.exe` unless vcvars was called for it) for
/// `cl.exe`; falls back to the literal `cl.exe` (unresolvable → fingerprint
/// becomes `None`, a safe degrade — see doc above). On Unix: `$CC` or `cc`.
/// Registry 221.1 #1134: resolve a compiler NAME to an absolute file by walking
/// PATH, so a fingerprint can actually be taken of it.
///
/// Needed because the Unix side of `resolve_archive_cc_path` returns the bare
/// name `cc` when `$CC` is unset, and `std::fs::metadata("cc")` resolves against
/// the CWD and fails -- so `rt_archive_compiler_fingerprint` quietly yields `None`
/// there and the archive key carries NO compiler dimension on Unix at all. That
/// blind spot is its own finding; this helper is what a fix for it would also use.
fn resolve_cc_absolute(name: &str) -> PathBuf {
    let p = PathBuf::from(name);
    if p.is_absolute() || name.contains('/') || name.contains('\\') {
        return p;
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let cand = dir.join(name);
            if cand.is_file() {
                return cand;
            }
        }
    }
    p
}

/// Registry 221.1 #1134: bucket key for the libuv archive.
///
/// WHY IT EXISTS. The cache used to be a single `target/libuv-cache/libuv.a`
/// taken on the strength of `if lib_file.is_file()` -- no key of any kind. The
/// first Linux to build it imposed its ABI on every later one: measured
/// 2026-09-16, a WSL Ubuntu 24.04 build (glibc 2.38+) was reused by a Debian 12
/// container and produced `undefined reference to '__isoc23_strtol'` in libuv's
/// own `threadpool.c`/`thread.c`/`linux.c`. The failure reads as a broken Nova
/// compiler and is nothing of the sort.
///
/// WHAT IS FOLDED IN: os, arch, and the COMPILER -- its resolved absolute path
/// plus size and mtime, the same fingerprint shape the runtime archive uses.
///
/// WHAT IS NOT, named rather than hidden: the same compiler binary against a
/// different libc is not distinguished. Reading a libc version portably costs a
/// subprocess on every run; the measured case (two distros) differs in the
/// compiler binary too, so this key catches it.
fn libuv_cache_key() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    "nova-libuv-v1".hash(&mut h);
    std::env::consts::OS.hash(&mut h);
    std::env::consts::ARCH.hash(&mut h);
    let cc_name = if cfg!(target_os = "windows") {
        std::env::var("CC").unwrap_or_else(|_| "cl.exe".to_string())
    } else {
        std::env::var("CC").unwrap_or_else(|_| "cc".to_string())
    };
    let cc_path = resolve_cc_absolute(&cc_name);
    cc_path.to_string_lossy().hash(&mut h);
    if let Some((len, nanos)) = rt_archive_compiler_fingerprint(&cc_path) {
        len.hash(&mut h);
        nanos.hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

fn resolve_archive_cc_path(tc: &Toolchain) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let env: &[(OsString, OsString)] = match tc {
            Toolchain::Clang { env, .. } => env,
            Toolchain::Msvc { env, .. } => env,
            Toolchain::Gcc { .. } => &[],
        };
        if let Some((_, path_val)) = env
            .iter()
            .find(|(k, _)| k.to_string_lossy().eq_ignore_ascii_case("PATH"))
        {
            for dir in std::env::split_paths(path_val) {
                let candidate = dir.join("cl.exe");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
        PathBuf::from("cl.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = tc;
        // Registry 221.1 #1135: this used to return the BARE NAME `cc` when `$CC`
        // is unset. `rt_archive_compiler_fingerprint` then called
        // `std::fs::metadata("cc")`, which resolves against the CWD, failed, and
        // yielded `None` -- so on Unix the archive key carried NO compiler
        // dimension at all and a clang upgrade did not bust the cache. Nothing
        // looked broken: the key stayed stable because the Option's discriminant
        // hashes. The measurement was simply absent, which is the same class as
        // #1118 and #1134 -- an absent answer standing in for a verdict.
        //
        // Found while fixing #1134, where this very function was the MODEL being
        // copied. `resolve_cc_absolute` walks PATH, and it exists because that fix
        // needed it.
        let name = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        resolve_cc_absolute(&name)
    }
}

/// Build (or reuse) the runtime archive for this exact bucket. `tc` is used
/// only for `vcvars_path()` on Windows — the archive itself is compiled
/// with a FIXED compiler (`cl.exe` via vcvars on Windows, `$CC`/`cc` on
/// Unix), independent of the app's chosen `--toolchain`. This exactly
/// mirrors the existing `detect_or_build_libuv`/`build_libuv_lib`
/// precedent: `libuv.lib` is likewise built once with cl.exe/cc and linked
/// into both clang- and msvc-toolchain app builds — COFF/ELF objects from
/// either front-end interlink fine on their platform. Consequence (and
/// deliberate, precedented trade-off): archived objects are NOT literally
/// byte-identical machine code to what `--toolchain=clang` would emit
/// inline (different compiler/no cross-TU `-flto` into the archive — see
/// `build_rt_archive_lib` doc) — they ARE behaviorally identical, which is
/// the actual gate (same C source, same effective defines, deterministic
/// semantics; `libuv.lib` has coexisted with `-flto` app builds under this
/// exact scheme since Plan 22).
///
/// Returns `None` on ANY failure — pure optimization, never a hard
/// requirement; callers fall back to the pre-218 inline-compile path.
pub fn detect_or_build_rt_archive(
    rt_dir: &Path,
    repo_root: &Path,
    tc: &Toolchain,
    opts: &BuildOpts,
) -> Option<RtArchiveConfig> {
    if !rt_archive_enabled() {
        return None;
    }
    let d_prefix = if cfg!(target_os = "windows") {
        "/D"
    } else {
        "-D"
    };
    let effect_define = effect_count_define_arg(opts.c_file, d_prefix);
    let runtime_defines = runtime_define_args(opts.runtime, d_prefix);
    let libuv_present = opts.libuv.is_some();

    let memo_key = rt_archive_memo_key(
        rt_dir,
        opts.mode,
        opts.gc_kind,
        libuv_present,
        effect_define.as_deref(),
        &runtime_defines,
    );
    let memo =
        RT_ARCHIVE_MEMO.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    // [M-218-rt-archive-parallel-jobs-race] fix: hold ONE guard across the
    // whole check -> build -> memoize sequence below (previously the lock
    // was released right after the lookup and re-acquired only to insert
    // the result — leaving a window between them). `nova test --jobs N`
    // runs its worker pool as N THREADS inside ONE process
    // (`std::thread::scope` in this file's parallel test runner, not
    // separate OS processes), all calling this fn concurrently for
    // (usually) the SAME bucket. With the lock released in that window,
    // every thread could observe a memo-miss AND an absent on-disk
    // `lib_file` at once, then race to build/overwrite the SAME
    // `target/rt-archive-cache/<key>/` bucket concurrently — shared obj
    // dir, shared `.rsp` files, shared output archive clobbered by
    // multiple `cl.exe`/`lib.exe` (or `cc`/`ar`) invocations at once, not
    // merely wasted work but actual corruption (observed as a flaky FAIL
    // under `--jobs`, PASS in isolation — the archive some later reader
    // linked against was mid-write). Holding the lock for the full
    // sequence makes the first thread to reach a bucket do the real
    // build while every other thread blocks; they then either hit the
    // now-populated memo (same bucket — the common case per the module
    // doc: most programs share one bucket) or build their own DIFFERENT
    // bucket serially right after (rare). `build_rt_archive_lib` below is
    // ALSO hardened with its own unique-scratch-dir + atomic-rename
    // publish (mirrors `build_cache.rs::store_c`'s temp+rename idiom) as
    // defense-in-depth for builders in SEPARATE OS processes, which this
    // in-process mutex can't serialize (e.g. a concurrent `nova build` in
    // another terminal against the same repo `target/`).
    let mut guard = match memo.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(cached) = guard.get(&memo_key) {
        return cached.clone();
    }

    let result = (|| -> Option<RtArchiveConfig> {
        let cc_path = resolve_archive_cc_path(tc);
        let cc_fingerprint = rt_archive_compiler_fingerprint(&cc_path);

        let key = rt_archive_key(
            rt_dir,
            opts.mode,
            opts.gc_kind,
            libuv_present,
            effect_define.as_deref(),
            &runtime_defines,
            cc_fingerprint,
        )?;

        let cache_dir = repo_root.join("target").join("rt-archive-cache").join(&key);
        let lib_name = if cfg!(target_os = "windows") {
            "libnova_rt.lib"
        } else {
            "libnova_rt.a"
        };
        let lib_file = cache_dir.join(lib_name);
        if lib_file.is_file() {
            return Some(RtArchiveConfig { lib_file });
        }

        let sources = rt_archive_sources(rt_dir, opts.gc_kind, opts.libuv);
        // #269 Ф.2: same fallback-aware resolution as `build_command`'s own
        // `boehm_cfg` (see that call site's comment) — this builder has its
        // OWN independent GC include/lib derivation (Plan 218 prebuilt
        // `libnova_rt` archive), so it needs the SAME fix, not just the
        // early honest-exit check in `resolve_gc_or_exit`. Found by running
        // the actual clean-clone acceptance gate: without this, the rt_*.c
        // sources (which `#include <gc.h>` unconditionally under
        // `GcKind::Boehm`) failed with "gc.h file not found" even though
        // the fallback `gc.lib` had already been built successfully one
        // step earlier — `boehm_cfg` here was silently `None`.
        let boehm_cfg = if opts.gc_kind == GcKind::Boehm {
            detect_boehm(opts.cg_include)
                .or_else(|| detect_or_build_boehm_fallback(rt_dir, repo_root, tc.vcvars_path()))
        } else {
            None
        };
        eprintln!(
            "nova: libnova_rt archive not built for this config, building (one-time, ~5-7 sec)..."
        );
        let build_result = build_rt_archive_lib(
            &sources,
            &cache_dir,
            &lib_file,
            rt_dir,
            opts.cg_include,
            opts.mode,
            opts.gc_kind,
            boehm_cfg.as_ref(),
            opts.libuv,
            effect_define.as_deref(),
            &runtime_defines,
            tc.vcvars_path(),
        );
        match build_result {
            Ok(()) if lib_file.is_file() => {
                eprintln!("nova: libnova_rt archive built ({})", lib_file.display());
                Some(RtArchiveConfig { lib_file })
            }
            Ok(()) => {
                eprintln!(
                    "nova: warning: libnova_rt archive build reported success but {} \
                     missing — falling back to inline compile",
                    lib_file.display()
                );
                None
            }
            Err(e) => {
                eprintln!(
                    "nova: warning: libnova_rt archive build failed ({:#}) — \
                     falling back to per-build inline compile",
                    e
                );
                None
            }
        }
    })();

    guard.insert(memo_key, result.clone());
    result
}

/// Unique-enough per-attempt tag for `build_rt_archive_lib`'s scratch
/// dir/files (see that fn's doc for why). PID gives cross-process
/// uniqueness (two separate `nova` OS processes racing on the same repo
/// `target/`); the atomic counter gives intra-process uniqueness across
/// threads in case this is ever called without the caller's memo-mutex
/// serialization; the nanosecond timestamp is a cheap extra safety
/// margin. [M-218-rt-archive-parallel-jobs-race].
fn unique_build_tag() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), n, nanos)
}

/// Compile `sources` → objects and archive them into `lib_file`. Mirrors
/// `build_libuv_lib`'s Windows(cl.exe+lib.exe)/Unix(cc+ar) structure exactly.
/// `mode` controls optimization level only — deliberately NO `-flto`/`/GL`
/// here even in Release (matches `build_libuv_lib`, which never LTOs
/// `libuv.lib` either): mixing an LTO-bitcode static archive built by one
/// compiler with a final link potentially done by a DIFFERENT compiler
/// (clang vs gcc on Unix, since the archive's compiler is fixed but the
/// app's `--toolchain` is not) risks incompatible bitcode formats. app.c
/// itself still gets full `-flto` in `build_command` — only the archived
/// runtime's cross-TU inlining into app.c is traded away, a bounded,
/// precedented cost (`libuv.lib` already forgoes it entirely).
///
/// **[M-218-rt-archive-parallel-jobs-race]:** everything this fn writes
/// (obj files, `.rsp` files, the linked archive itself) goes into a
/// scratch dir UNIQUE to this one build ATTEMPT
/// (`cache_dir/.build-<pid>-<counter>-<nanos>`, never a fixed shared
/// `cache_dir/obj` — that was the actual corruption vector: two builders
/// writing the same obj/.rsp/archive paths at once). The real `lib_file`
/// path is only ever touched once, at the very end, via an atomic
/// rename — a reader either sees no file or a fully-written one, never a
/// partial one. The caller (`detect_or_build_rt_archive`) already
/// serializes same-process callers with a widened mutex; this is
/// defense-in-depth for builders in separate OS processes, which that
/// mutex cannot see.
#[allow(clippy::too_many_arguments)]
fn build_rt_archive_lib(
    sources: &[PathBuf],
    cache_dir: &Path,
    lib_file: &Path,
    rt_dir: &Path,
    cg_include: &Path,
    mode: Mode,
    gc_kind: GcKind,
    boehm_cfg: Option<&BoehmConfig>,
    libuv: Option<&LibuvConfig>,
    effect_define: Option<&str>,
    runtime_defines: &[String],
    vcvars: Option<&Path>,
) -> Result<()> {
    std::fs::create_dir_all(cache_dir).map_err(|e| anyhow!("create cache_dir: {:#}", e))?;
    let obj_dir = cache_dir.join(format!(".build-{}", unique_build_tag()));
    std::fs::create_dir_all(&obj_dir).map_err(|e| anyhow!("create obj_dir: {:#}", e))?;
    for src in sources {
        if !src.is_file() {
            let _ = std::fs::remove_dir_all(&obj_dir);
            return Err(anyhow!("rt archive source not found: {}", src.display()));
        }
    }
    // Linked INSIDE the isolated scratch dir first — published to the
    // real `lib_file` path only via the atomic rename at the bottom.
    let tmp_lib_file = obj_dir.join(
        lib_file
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("libnova_rt.tmp")),
    );

    let build: Result<()> = (|| -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            let vcv = vcvars.ok_or_else(|| {
                anyhow!("vcvars required for libnova_rt archive build on Windows")
            })?;
            let mode_flags = match mode {
                Mode::Dev => "/Od /Z7",
                Mode::Release => "/O2 /DNDEBUG",
            };
            let mut lines: Vec<String> = Vec::new();
            lines.push(format!("/c /nologo /W0 {} /Gy", mode_flags));
            lines.push(format!(
                "/FI \"{}\"",
                rt_dir.join("nova_msvc_compat.h").display()
            ));
            if gc_kind == GcKind::Boehm {
                lines.push("/DNOVA_GC_BOEHM /DGC_THREADS".to_string());
                if let Some(cfg) = boehm_cfg {
                    if let Some(inc) = &cfg.include_dir {
                        lines.push(format!("/I \"{}\"", inc.display()));
                    }
                }
            }
            for da in runtime_defines {
                lines.push(da.clone());
            }
            if let Some(ea) = effect_define {
                lines.push(ea.to_string());
            }
            if let Some(uv) = libuv {
                lines.push("/DNOVA_USE_LIBUV=1".to_string());
                lines.push(format!("/I \"{}\"", uv.include_dir.display()));
            }
            lines.push(format!("/I \"{}\"", cg_include.display()));
            lines.push(format!("/Fo\"{}\\\\\"", obj_dir.display()));
            for s in sources {
                lines.push(format!("\"{}\"", s.display()));
            }
            let rsp = obj_dir.join("compile.rsp");
            // №287: BOM against ANSI-codepage mangling of non-ASCII paths.
            std::fs::write(&rsp, format!("\u{FEFF}{}", lines.join("\n")))
                .map_err(|e| anyhow!("write rsp: {:#}", e))?;
            let inner = format!(
                "\"call \"{}\" >nul 2>&1 && cl.exe @\"{}\"\"",
                vcv.display(),
                rsp.display()
            );
            let mut cmd = Command::new("cmd");
            cmd.raw_arg("/c").raw_arg(&inner);
            let out = cmd.output().map_err(|e| anyhow!("spawn cl.exe: {:#}", e))?;
            if !out.status.success() {
                let combined = format!(
                    "{}{}",
                    bytes_to_string(&out.stdout),
                    bytes_to_string(&out.stderr)
                );
                return Err(anyhow!(
                    "libnova_rt compile failed: {}",
                    combined.lines().take(20).collect::<Vec<_>>().join("\n")
                ));
            }
            let mut obj_files: Vec<PathBuf> = Vec::new();
            for entry in
                std::fs::read_dir(&obj_dir).map_err(|e| anyhow!("read obj_dir: {:#}", e))?
            {
                let p = entry
                    .map_err(|e| anyhow!("read_dir entry: {:#}", e))?
                    .path();
                if p.extension().and_then(|s| s.to_str()) == Some("obj") {
                    obj_files.push(p);
                }
            }
            if obj_files.len() != sources.len() {
                return Err(anyhow!(
                    "libnova_rt compile: expected {} .obj files, found {} in {}",
                    sources.len(),
                    obj_files.len(),
                    obj_dir.display()
                ));
            }
            let lib_rsp = obj_dir.join("lib.rsp");
            let mut lib_lines: Vec<String> = vec![
                "/nologo".to_string(),
                format!("/OUT:\"{}\"", tmp_lib_file.display()),
            ];
            for o in &obj_files {
                lib_lines.push(format!("\"{}\"", o.display()));
            }
            // №287: BOM against ANSI-codepage mangling of non-ASCII paths.
            std::fs::write(&lib_rsp, format!("\u{FEFF}{}", lib_lines.join("\n")))
                .map_err(|e| anyhow!("write lib.rsp: {:#}", e))?;
            let lib_inner = format!(
                "\"call \"{}\" >nul 2>&1 && lib.exe @\"{}\"\"",
                vcv.display(),
                lib_rsp.display()
            );
            let mut lib_cmd = Command::new("cmd");
            lib_cmd.raw_arg("/c").raw_arg(&lib_inner);
            let lib_out = lib_cmd
                .output()
                .map_err(|e| anyhow!("spawn lib.exe: {:#}", e))?;
            if !lib_out.status.success() {
                return Err(anyhow!(
                    "lib.exe failed: {}",
                    bytes_to_string(&lib_out.stderr)
                ));
            }
            eprintln!("nova: libnova_rt.lib built ({} files)", sources.len());
            return Ok(());
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
            let mut obj_files: Vec<PathBuf> = Vec::new();
            for src in sources {
                let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("rt");
                let obj = obj_dir.join(format!("{}.o", stem));
                let mut c = Command::new(&cc);
                match mode {
                    Mode::Dev => {
                        c.args(["-O0", "-g", "-w"]);
                    }
                    Mode::Release => {
                        c.arg("-O3");
                        c.arg(format!("-march={}", march_flag()));
                        c.arg("-DNDEBUG");
                        c.arg("-w");
                    }
                }
                c.arg("-c");
                c.arg("-fPIC");
                // [M-linux-mn-conformance-red] fix (2026-07-20): without per-
                // function/data sections, `-Wl,--gc-sections` at the final link
                // (main `build_command`'s Linux/macOS branch) can only discard
                // an ENTIRE archive-member .o as a unit, never an individual
                // dead function within one — so once effects.o/runtime.o/etc.
                // get pulled in for symbols they DO provide, dead code inside
                // them (e.g. `nova_bench_heap_sampler_thread` in bench.h,
                // included unconditionally, whose `NOVA_BENCH_STATE_DEFINE`
                // globals are only DEFINED in bench_mode builds) drags in
                // unresolved externs even for a plain `nova test`/`nova build`.
                // Confirmed empirically on WSL2/Linux: `undefined reference to
                // _nova_bench_heap_sample_interval_ns`/`_nova_bench_heap_sampler_stop`
                // when linking against the rt-archive, absent in the pre-218
                // per-build inline-compile path (which already passes these
                // flags — see `build_command`'s Clang/Gcc Unix branches). The
                // Windows half of this function already has the equivalent
                // (`/Gy`, function-level linking) — this brings the Unix half
                // to parity.
                c.arg("-ffunction-sections");
                c.arg("-fdata-sections");
                c.arg("-D_GNU_SOURCE");
                if gc_kind == GcKind::Boehm {
                    c.arg("-DNOVA_GC_BOEHM");
                    c.arg("-DGC_THREADS");
                    if let Some(cfg) = boehm_cfg {
                        if let Some(inc) = &cfg.include_dir {
                            let s = inc.to_string_lossy();
                            if !s.starts_with("/usr/include") {
                                c.arg("-I").arg(inc);
                            }
                        }
                    }
                }
                for da in runtime_defines {
                    c.arg(da);
                }
                if let Some(ea) = effect_define {
                    c.arg(ea);
                }
                if let Some(uv) = libuv {
                    c.arg("-DNOVA_USE_LIBUV=1");
                    c.arg("-I").arg(&uv.include_dir);
                }
                c.arg("-I").arg(cg_include);
                c.arg("-o").arg(&obj);
                c.arg(src);
                let out = c.output().map_err(|e| anyhow!("spawn {}: {:#}", cc, e))?;
                if !out.status.success() {
                    return Err(anyhow!(
                        "libnova_rt compile failed on {}: {}",
                        src.display(),
                        bytes_to_string(&out.stderr)
                    ));
                }
                obj_files.push(obj);
            }
            let mut ar = Command::new("ar");
            ar.arg("rcs").arg(&tmp_lib_file);
            for o in &obj_files {
                ar.arg(o);
            }
            let ar_out = ar.output().map_err(|e| anyhow!("spawn ar: {:#}", e))?;
            if !ar_out.status.success() {
                return Err(anyhow!("ar failed: {}", bytes_to_string(&ar_out.stderr)));
            }
            eprintln!("nova: libnova_rt.a built ({} files)", sources.len());
            return Ok(());
        }
        #[allow(unreachable_code)]
        {
            let _ = (
                sources,
                cache_dir,
                lib_file,
                rt_dir,
                cg_include,
                mode,
                gc_kind,
                boehm_cfg,
                libuv,
                effect_define,
                runtime_defines,
                vcvars,
                &tmp_lib_file,
            );
            Err(anyhow!("unsupported platform for libnova_rt archive build"))
        }
    })();

    // [M-218-rt-archive-parallel-jobs-race]: publish atomically. By this
    // point the archive is either fully written at `tmp_lib_file` (inside
    // the isolated scratch dir) or `build` is `Err` and nothing has
    // touched the real `lib_file` path at all — a half-finished build
    // NEVER becomes visible at `lib_file`. Mirrors
    // `build_cache.rs::store_c`'s temp-file + `fs::rename` idiom (same
    // repo, same class of problem: don't let readers observe a partial
    // write).
    let published = build.and_then(|()| {
        if !tmp_lib_file.is_file() {
            return Err(anyhow!(
                "libnova_rt archive build reported success but {} missing",
                tmp_lib_file.display()
            ));
        }
        match std::fs::rename(&tmp_lib_file, lib_file) {
            Ok(()) => Ok(()),
            Err(e) => {
                // This process's callers are already serialized by
                // `detect_or_build_rt_archive`'s widened mutex, but a
                // genuinely separate `nova` OS process racing on the same
                // repo `target/` is NOT covered by that mutex. If it won
                // and already published a file at `lib_file`, that
                // archive is content-addressed by the very same bucket
                // key we just built from — byte-identical inputs, so
                // treat its presence as success instead of surfacing a
                // spurious failure (e.g. a Windows sharing violation from
                // renaming over a file the other process still has open).
                if lib_file.is_file() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "publish {} -> {}: {:#}",
                        tmp_lib_file.display(),
                        lib_file.display(),
                        e
                    ))
                }
            }
        }
    });

    // Scratch dir is disposable regardless of outcome — best-effort
    // cleanup, never fails the build over it (matches this module's other
    // `let _ = std::fs::remove_dir_all(...)` best-effort precedent, e.g.
    // `build_libuv_lib` above).
    let _ = std::fs::remove_dir_all(&obj_dir);

    published
}

#[cfg(test)]
mod tests {
    use super::{effect_count_define_arg_from_line, march_flag_for, runtime_define_args};
    use crate::RuntimeConfig;

    #[test]
    fn archive_march_key_uses_native_flag_only_when_requested() {
        assert_eq!(march_flag_for(false), "x86-64-v3");
        assert_eq!(march_flag_for(true), "native");
    }

    #[test]
    fn runtime_build_defines_keep_valid_values_and_skip_invalid() {
        let config = RuntimeConfig {
            fiber_stack: Some("4MB".into()),
            fibers_per_worker: Some("bad".into()),
        };
        assert_eq!(
            runtime_define_args(Some(&config), "-D"),
            ["-DNOVA_FIBER_STACK_DEFAULT=4194304"]
        );
    }

    #[test]
    fn effect_count_define_requires_positive_integer_marker() {
        assert_eq!(
            effect_count_define_arg_from_line("/* nova-effect-count: 7 */\n", "-D"),
            Some("-DNOVA_MAX_EFFECT_STORAGES=7".into())
        );
        assert_eq!(
            effect_count_define_arg_from_line("/* no marker */\n", "-D"),
            None
        );
        assert_eq!(
            effect_count_define_arg_from_line("/* nova-effect-count: 0 */\n", "-D"),
            None
        );
    }
}
