//! Compiler-free C compile/link invocation and FFI flag construction.
use crate::link_prep::first_missing_ffi_lib;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::native_runtime::LIBUV_UNIX_SYSLIBS;
#[cfg(target_os = "windows")]
use crate::native_runtime::LIBUV_WIN_SYSLIBS;
use crate::native_runtime::{
    detect_boehm, detect_or_build_boehm_fallback, detect_or_build_rt_archive,
    effect_count_define_arg, march_flag, resolve_gc_or_exit, runtime_define_args, RtArchiveConfig,
};
use crate::process::{run_with_timeout, signal_note};
use crate::{bytes_to_string, which, BuildOpts, GcKind, Mode, ResolvedFfiConfig, Toolchain};
use anyhow::anyhow;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub fn ffi_have_defines(ffi: &ResolvedFfiConfig) -> Vec<String> {
    if ffi.lib_dirs.is_empty() || !ffi.libs.is_empty() && first_missing_ffi_lib(ffi).is_some() {
        return Vec::new();
    }
    ffi.libs
        .iter()
        .map(|lib| {
            let sanitized: String = lib
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_uppercase()
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("NOVA_FFI_HAVE_{}", sanitized)
        })
        .collect()
}

pub fn arg_c_source(c: &mut std::process::Command, cxx: bool, path: impl AsRef<std::ffi::OsStr>) {
    if cxx {
        c.arg("-x").arg("c");
    }
    c.arg(path);
    if cxx {
        c.arg("-x").arg("none");
    }
}

pub fn cxx_driver_name(stem: &str) -> String {
    if stem.starts_with("clang++") || stem.starts_with("g++") {
        stem.to_string()
    } else if stem.starts_with("clang") {
        stem.replacen("clang", "clang++", 1)
    } else if stem.starts_with("gcc") {
        stem.replacen("gcc", "g++", 1)
    } else {
        stem.to_string()
    }
}

#[cfg(test)]
mod cxx_driver_name_tests {
    use super::cxx_driver_name;

    /// POS: the two drivers we actually switch, with and without a version suffix
    /// and with the .exe that Windows adds.
    #[test]
    fn pos_known_drivers_are_switched() {
        assert_eq!(cxx_driver_name("clang"), "clang++");
        assert_eq!(cxx_driver_name("gcc"), "g++");
        assert_eq!(cxx_driver_name("clang-15"), "clang++-15");
        assert_eq!(cxx_driver_name("gcc-13"), "g++-13");
        assert_eq!(cxx_driver_name("clang.exe"), "clang++.exe");
    }

    /// POS: applying it twice must not produce `clang++++` -- the switch happens
    /// once per link, but nothing in the type system says so.
    #[test]
    fn pos_idempotent_on_an_already_cxx_driver() {
        assert_eq!(cxx_driver_name("clang++"), "clang++");
        assert_eq!(cxx_driver_name("g++"), "g++");
        assert_eq!(
            cxx_driver_name(cxx_driver_name("clang").as_str()),
            "clang++"
        );
    }

    /// NEG: a driver we do not recognise is returned UNCHANGED rather than
    /// guessed at. The caller then reports "no C++ driver found" naming this
    /// exact string, which is a true statement; inventing `cc++` would produce a
    /// misleading "no such file" about a name nobody ships.
    #[test]
    fn neg_unknown_driver_is_not_invented() {
        assert_eq!(cxx_driver_name("cc"), "cc");
        assert_eq!(cxx_driver_name("icx"), "icx");
        assert_eq!(cxx_driver_name(""), "");
    }
}

/// Имя файла `.c` для исходника: два последних сегмента каталога плюс имя.
///
/// ЗАЧЕМ ОНО НЕ ПРОСТО `<имя>.c` (починка 2026-09-09; дефект внесён в тот же
/// день переносом `.c` из соседства с исходником в каталог сборки).
/// Рядом с исходником `collections/range/core.nv` и `collections/set/core.nv`
/// давали РАЗНЫЕ пути. В одном каталоге сборки оба стали `core.c`, и три теста
/// `std` начали падать `CODEGEN-FAIL ... failed to write ...\core.c` — падал
/// тот, кто проиграл гонку за файл. Имён-двойников в `std/src` и `spec_tests`
/// ДВАДЦАТЬ ПЯТЬ, то есть это закономерность, а не редкость.
///
/// ЦЕНА ОШИБКИ БЫЛА НЕ В ПАДЕНИИ, А В ЕГО ВИДЕ: имя жертвы менялось от прогона
/// к прогону, отказ читался как МЕРЦАЮЩИЙ ТЕСТ, и первый разбор увёл меня в
/// сторону теста, судящего время. Уникальность имени здесь — условие того,
/// чтобы вердикт называл предмет, а не победителя гонки.
///
/// Хвост пути, а не хэш: человек в отладке ищет свой файл глазами, и
/// `collections_range_core.c` он найдёт, а `a3f9e1.c` — нет.
///
/// ОДИН ДОМ на оба места (писатель и проверка существования): копия разошлась
/// бы на первой правке, а расхождение здесь означает поиск файла там, где его
/// не пишут.
pub fn unique_c_base(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("cu");
    let uniq: String = path
        .parent()
        .map(|p| {
            let mut v: Vec<&str> = p
                .components()
                .rev()
                .take(2)
                .filter_map(|c| c.as_os_str().to_str())
                .collect();
            v.reverse();
            v.join("_")
        })
        .unwrap_or_default();
    if uniq.is_empty() {
        stem.to_string()
    } else {
        format!("{}_{}", uniq, stem)
    }
}

pub fn build_command(tc: &Toolchain, opts: &BuildOpts) -> Command {
    // Plan 27 Ф.1: alloc source chosen by GC backend.
    let rt_alloc = opts.rt_dir.join(opts.gc_kind.alloc_c_name());
    let rt_effects = opts.rt_dir.join("effects.c");
    let rt_fibers = opts.rt_dir.join("fibers.c");
    // Plan 44.2 Etap 1: fiber stack arena POSIX (mmap). Windows-branch
    // файла — no-op marker.
    let rt_fiber_arena = opts.rt_dir.join("fiber_arena.c");
    // Plan 82 Ф.1: fiber stack arena Windows (VirtualAlloc lazy-commit).
    // POSIX-branch файла — no-op marker. Оба файла линкуются всегда,
    // каждый — пустой TU вне своей ОС.
    let rt_fiber_arena_win = opts.rt_dir.join("fiber_arena_win.c");
    // Plan 44.2 Etap 3: cross-platform stats wrappers for std.runtime.fibers.
    let rt_fiber_stats = opts.rt_dir.join("fiber_stats.c");
    // Plan 44 Этап 0: M:N runtime (opt-in через nova_runtime_init).
    let rt_runtime = opts.rt_dir.join("runtime.c");
    // Plan 83.11 Ф.2: centralized I/O driver — dedicated thread с UV loop.
    let rt_driver = opts.rt_dir.join("driver.c");
    /* Plan 61 Ф.1: TypeId weak-fallback (nova_typeid_to_name). Codegen может
     * emit'ить overriding implementation в preamble; weak fallback —
     * safety-net для minimal tests. */
    let rt_typeid = opts.rt_dir.join("typeid.c");
    /* Plan 83.11 §12.31: in-process SEGV crash localizer. Gated by
     * NOVA_DIAG_SEGV env var; no overhead when unset. No-op TU on non-Windows. */
    let rt_segv_diag = opts.rt_dir.join("segv_diag.c");
    // Plan 83.12/183: net.c — std/net substrate (one FFI layer, byte transport,
    // zero-copy, M:N-safe), compiled only when libuv is available (conditional
    // on libuv presence, added inside the libuv if-let blocks per toolchain).
    // Plan 182 Ф.1: net2.c renamed to net.c (old std/net removed, net2 promoted).
    let rt_net = opts.rt_dir.join("net.c");
    // Plan 176 Ф.2: fs.c — std/fs async uv_fs_* backend, same libuv gating as net.c.
    let rt_fs = opts.rt_dir.join("fs.c");
    // Plan 265 Ф.1: process.c — std/os subprocess substrate (uv_spawn/uv_process_t),
    // same libuv gating as net.c/fs.c.
    let rt_process = opts.rt_dir.join("process.c");
    let march = march_flag();

    // Plan 218: prebuilt runtime archive. If a bucket-matching `libnova_rt`
    // is cached (or gets built here, one-time), link it instead of adding
    // every rt_* source below as an individual per-build compile unit —
    // see the "Plan 218" doc block above `detect_or_build_rt_archive` for
    // the bucket-key rationale (effect-count/runtime-define ABI hazard).
    // `None` (disabled via `NOVA_RT_ARCHIVE=0`, or any build failure) keeps
    // the exact pre-218 behavior below — zero regression by construction.
    let rt_archive: Option<RtArchiveConfig> = opts
        .rt_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|repo_root| detect_or_build_rt_archive(opts.rt_dir, repo_root, tc, opts));
    let use_rt_archive = rt_archive.is_some();

    // Plan 27 Ф.1+Ф.D + #269 Ф.2: Boehm paths resolved via detect_boehm (env
    // overrides + local vcpkg + global vcpkg), falling back — #269 Ф.2 — to
    // the vendored bdwgc submodule build when neither is present (mirrors
    // `resolve_gc_or_exit`'s own fallback exactly; MUST be consulted here
    // too, not just at the early honest-exit check above, otherwise a
    // successful fallback build there is invisible to the ACTUAL compile
    // flags computed below — `vcpkg_include`/`vcpkg_lib` would silently
    // fall through to their `unwrap_or_else` legacy vcpkg-path default,
    // which doesn't exist on a fallback-built clean clone). Idempotent —
    // the fallback fn's own cache check makes this a cheap disk stat on the
    // (overwhelmingly common) case where `resolve_gc_or_exit` already ran.
    // На Linux/macOS Some(BoehmConfig) с include_dir=Some из system path,
    // lib_dir=None — линкер через -lgc. Под Windows detect_boehm/fallback
    // всегда даёт both Some(...). Если backend = Malloc → cfg = None, paths
    // не используются.
    let boehm_cfg = if opts.gc_kind == GcKind::Boehm {
        detect_boehm(opts.cg_include).or_else(|| {
            opts.rt_dir
                .parent()
                .and_then(|p| p.parent())
                .and_then(|repo_root| {
                    detect_or_build_boehm_fallback(opts.rt_dir, repo_root, tc.vcvars_path())
                })
        })
    } else {
        None
    };
    // Legacy fallback path (для случаев когда detect_boehm вернул None и
    // mistake'нно дошли до build_command — например тест прямо вызывает
    // build_command минуя resolve_gc_or_exit). Оставляем как safety-net.
    let vcpkg_include = boehm_cfg
        .as_ref()
        .and_then(|c| c.include_dir.clone())
        .unwrap_or_else(|| {
            opts.cg_include
                .join("vcpkg_installed")
                .join("x64-windows-static")
                .join("include")
        });
    let vcpkg_lib = boehm_cfg
        .as_ref()
        .and_then(|c| c.lib_dir.clone())
        .unwrap_or_else(|| {
            opts.cg_include
                .join("vcpkg_installed")
                .join("x64-windows-static")
                .join("lib")
        });

    // Plan 22: libuv linkage. Если libuv config present — добавляем
    // eventloop.c в sources, -DNOVA_USE_LIBUV=1, libuv include, libuv.lib
    // + Windows system libs.
    let libuv_eventloop = opts.libuv.map(|c| c.eventloop_src.clone());
    let libuv_include = opts.libuv.map(|c| c.include_dir.clone());
    let libuv_lib = opts.libuv.map(|c| c.lib_file.clone());

    match tc {
        Toolchain::Clang { clang, env, .. } => {
            // GCC-style flags. Target явный (msvc/linux/darwin).
            let target = if cfg!(target_os = "windows") {
                "--target=x86_64-pc-windows-msvc"
            } else if cfg!(target_os = "macos") {
                "" // системный default
            } else {
                "" // linux: default
            };
            let mut flags: Vec<String> = match opts.mode {
                // Plan 140 Ф.1 (D24 amend): контракты эмитятся безусловно
                // (enforce-with-elision), `#ifdef NOVA_CONTRACTS_RUNTIME` снят
                // на codegen → флаг `-DNOVA_CONTRACTS_RUNTIME=1` больше не нужен
                // ни в debug, ни в release. Недоказанные контракты проверяются
                // в обоих режимах; Z3-proven элидируются на codegen (zero-cost).
                // Build-opt-out (`--contracts=off`) — Ф.2.
                Mode::Dev => vec![
                    "-O0".to_string(),
                    "-g".to_string(),
                    "-Wno-everything".to_string(),
                ],
                Mode::Release => vec![
                    "-O3".to_string(),
                    "-flto".to_string(),
                    format!("-march={}", march),
                    "-DNDEBUG".to_string(),
                    "-Wno-everything".to_string(),
                ],
            };
            if !target.is_empty() {
                flags.insert(0, target.to_string());
            }
            // Plan 81 Ф.7.1: linker-level DCE. -ffunction-sections /
            // -fdata-sections кладут каждую функцию/данные в отдельную
            // секцию; линкер затем удаляет неиспользуемые. Отсечение
            // делает линкер (как в Go) — без анализа в компиляторе,
            // near-zero риск. На Linux/macOS активируется -Wl,--gc-sections
            // (ниже, cfg-блок); на Windows lld-link folding включён по
            // умолчанию (/OPT:REF) — секции дают линкеру гранулярность.
            flags.push("-ffunction-sections".to_string());
            flags.push("-fdata-sections".to_string());
            // Plan 82 Ф.5: release-mode добавляет -flto; LLVM LTO требует
            // LLVM-линкера. Без -fuse-ld=lld clang на Windows падает
            // «error: LTO requires -fuse-ld=lld» (MSVC link.exe не умеет
            // LLVM LTO). Чинит `nova bench` и `nova test --mode release`
            // на Windows; lld поставляется в комплекте LLVM.
            #[cfg(target_os = "windows")]
            if matches!(opts.mode, Mode::Release) {
                flags.push("-fuse-ld=lld".to_string());
            }
            // Plan 198 (defect #8a insurance, NOT the fix): a modest stack
            // RESERVE safety margin on Windows. The real fix for the merged-CU
            // stack overflow is bounding `nova_fn_main_impl`'s C frame via
            // fixed-size test-chunk functions (see emit_main_wrapper /
            // TEST_CHUNK_SIZE in emit_c.rs) — that keeps the frame constant
            // regardless of corpus size. This flag is pure belt-and-suspenders:
            // RESERVE only consumes address space (pages commit lazily), so a
            // generous bump costs nothing at rest and gives headroom for
            // legitimately deep call stacks (generics/recursion) in real test
            // bodies, without masking a still-broken O(N) frame. Both
            // lld-link and link.exe accept `/stack:<reserve>`.
            #[cfg(target_os = "windows")]
            flags.push("-Wl,/stack:0x1000000".to_string()); // 16 MiB reserve
                                                            // Plan 44.2 P41-5 + audit round 5: stack-clash protection (CVE-2017-1000366).
                                                            // -fstack-clash-protection inserts page-by-page probing on stack frames
                                                            // >4KB, preventing skip past single guard page in one SP subtraction.
                                                            // -fstack-protector-strong adds canaries on functions with arrays.
                                                            // Linux/macOS clang/gcc support. Windows clang-cl/MSVC: skip (different
                                                            // mechanisms via /GS by default).
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                flags.push("-fstack-clash-protection".to_string());
                flags.push("-fstack-protector-strong".to_string());
                // Plan 81 Ф.7.1: GNU ld / lld удаляют неиспользуемые
                // секции (function/data sections выше). На Windows
                // lld-link делает то же по умолчанию (/OPT:REF).
                flags.push("-Wl,--gc-sections".to_string());
            }
            // Plan 44.5: NOVA_GC_BOEHM activates GC root registration in fibers.h.
            // GC_THREADS — Boehm compiled with -DGC_THREADS (vcpkg build.ninja confirms);
            // client side must define it too to expose GC_register_my_thread / GC_allow_register_threads.
            // Required for M:N workers (Plan 44.5 Layer 4+5).
            if opts.gc_kind == GcKind::Boehm {
                flags.push("-DNOVA_GC_BOEHM".to_string());
                flags.push("-DGC_THREADS".to_string());
            }
            // Plan 149 D233: nova.toml [runtime] → -DNOVA_FIBER_STACK_DEFAULT /
            // -DNOVA_FIBERS_PER_WORKER_DEFAULT (raw ints). After GC, before libuv.
            for da in runtime_define_args(opts.runtime, "-D") {
                flags.push(da);
            }
            // Plan 174.4: effect-registry size (all TUs, ABI-uniform).
            if let Some(da) = effect_count_define_arg(opts.c_file, "-D") {
                flags.push(da);
            }

            // Direct clang invocation with pre-captured vcvars env.
            // On Windows: env snapshot from capture_vcvars_env() at detect_toolchain() time.
            // Saves ~7s per test by avoiding `call vcvars64.bat` on every compile.
            // D466: a package that declares `[ffi] cxx = true` links a C++
            // library, and a static C++ archive does not carry its standard
            // library. ELF toolchains add it only when invoked through the C++
            // driver, so switch the executable: clang -> clang++, gcc -> g++.
            // The switch is on the LAST path component only; a directory named
            // .../clang-15/bin/ must not be rewritten. If the C++ driver is not
            // on disk we do NOT silently fall back to the C one -- a silent
            // fallback here produces an undefined-symbol wall pointing at the
            // user's own code, which is exactly how this defect was found.
            let driver = match opts.ffi.map(|f| f.cxx) {
                Some(true) => {
                    let p: &std::path::Path = clang.as_ref();
                    let stem = p.file_name().and_then(|s| s.to_str()).unwrap_or("clang");
                    let cxx_name = cxx_driver_name(stem);
                    let cand = p.with_file_name(&cxx_name);
                    if cand.exists() || which(&cxx_name).is_some() {
                        if cand.exists() {
                            cand.to_string_lossy().into_owned()
                        } else {
                            cxx_name
                        }
                    } else {
                        eprintln!(
                            "error: [ffi] cxx = true, but no C++ driver was found (looked for {} \
                             next to {} and on PATH). A C++ library cannot be linked by the C \
                             driver: the standard library would be missing and every C++ symbol \
                             would come back undefined, pointing at your code instead of here.",
                            cxx_name,
                            clang.display()
                        );
                        std::process::exit(2);
                    }
                }
                _ => clang.to_string_lossy().into_owned(),
            };
            let cxx_mode = opts.ffi.map(|f| f.cxx).unwrap_or(false);
            let mut c = Command::new(&driver);
            if !env.is_empty() {
                // Replace process env with the vcvars snapshot so clang sees
                // INCLUDE, LIB, PATH from VS Build Tools without re-running the bat.
                c.env_clear().envs(env.iter().cloned());
            }
            for f in &flags {
                if !f.is_empty() {
                    c.arg(f);
                }
            }
            c.arg("-I").arg(opts.cg_include);
            // Plan 22 libuv (cross-platform): defines + include path only
            // here. [M-linux-mn-conformance-red] fix (2026-07-20): the
            // LIBRARY (`libuv.a`) and any libuv-dependent LOOSE SOURCES
            // (`rt_net.c`/`rt_fs.c`, non-archive path) are placed LATER —
            // right after `opts.c_file` + the rt-archive/individual rt_*
            // sources are added (see the matching block below, right before
            // FFI libs). Root cause: GNU `ld` resolves archive members only
            // against symbols undefined AT THE MOMENT the archive is seen on
            // the command line; a reference appearing LATER (from an object
            // added AFTER the archive) is never satisfied — confirmed
            // empirically on WSL2/Linux (`nova test`, Plan 218 rt-archive
            // path, default-on): `undefined reference to uv_strerror`
            // (`fibers.h`'s `_nova_sleep_via_libuv`/`nova_blocking_offload`,
            // folded into `libnova_rt.a`) because `libuv.a` used to be added
            // HERE — before `libnova_rt.a`/`opts.c_file` further down.
            // Windows/MSVC's linker does a full symbol-table pass (not
            // strictly left-to-right for `.lib`), so the Windows sub-block
            // below stays at this ORIGINAL (early) position — zero behavior
            // change there, this split only takes effect on non-Windows.
            if let (Some(inc_path), Some(_lib_path), Some(_evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(inc_path);
                // Windows: libuv link via -L/-l flags (env has LIB set by vcvars).
                #[cfg(target_os = "windows")]
                {
                    // Plan 83.12/183: net.c compiled only when libuv is present.
                    // Plan 218: already inside libnova_rt.lib when the archive is
                    // active — skip re-adding as a loose source (double-define).
                    if !use_rt_archive {
                        arg_c_source(&mut c, cxx_mode, &rt_net);
                        // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                        arg_c_source(&mut c, cxx_mode, &rt_fs);
                        // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                        arg_c_source(&mut c, cxx_mode, &rt_process);
                    }
                    c.arg(_lib_path);
                    if !use_rt_archive {
                        arg_c_source(&mut c, cxx_mode, _evloop);
                    }
                    for syslib in LIBUV_WIN_SYSLIBS {
                        c.arg(format!("-l{}", syslib.replace(".lib", "")));
                    }
                }
            }
            // Plan 27 Ф.1+Ф.D: Boehm link flags for Clang.
            if opts.gc_kind == GcKind::Boehm {
                #[cfg(target_os = "windows")]
                {
                    c.arg("-I").arg(&vcpkg_include);
                    c.arg("-L").arg(&vcpkg_lib);
                    c.arg("-lgc");
                    // #269 Ф.2: conditional on existing — see the matching
                    // MSVC-branch comment (`atomic_ops_lib.is_file()`
                    // above) for the full rationale: the vcpkg-built
                    // `bdwgc` port links a separate `atomic_ops.lib`, the
                    // #269 Ф.2 fallback build doesn't produce/need one
                    // (header-only atomics on x86_64 MSVC).
                    if vcpkg_lib.join("atomic_ops.lib").is_file() {
                        c.arg("-latomic_ops");
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    // Linux/macOS: если detect_boehm нашёл non-system path
                    // (например Homebrew /opt/homebrew или env override) —
                    // передаём явно. Иначе linker ищет в system path через -lgc.
                    if let Some(cfg) = &boehm_cfg {
                        if let Some(inc) = &cfg.include_dir {
                            // Передаём только если non-default (не /usr/include).
                            let s = inc.to_string_lossy();
                            if !s.starts_with("/usr/include") {
                                c.arg("-I").arg(inc);
                            }
                        }
                        if let Some(lib) = &cfg.lib_dir {
                            c.arg("-L").arg(lib);
                        }
                    }
                    c.arg("-lgc");
                    #[cfg(target_os = "linux")]
                    c.arg("-lpthread");
                }
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI flags
            // BEFORE -o + c_file. include_dirs → -I; .h shims → -include
            // (force-included в каждый TU AFTER cg_include setup чтобы
            // shim header мог `#include "nova_rt/nova_rt.h"`); .c shims
            // → отдельные compilation units; libs (-l) в link phase ниже.
            if let Some(ffi) = opts.ffi {
                for inc in &ffi.include_dirs {
                    c.arg("-I").arg(inc);
                }
                // Plan 193 Ф.2 gate-3: generic feature-gate defines — see
                // `ffi_have_defines` doc-comment.
                for def in ffi_have_defines(ffi) {
                    c.arg(format!("-D{}=1", def));
                }
                for shim in &ffi.c_shims {
                    let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("c") {
                        arg_c_source(&mut c, cxx_mode, shim);
                    } else if ext.eq_ignore_ascii_case("h") {
                        c.arg("-include").arg(shim);
                    }
                }
            }
            c.arg("-o").arg(opts.exe_file);
            arg_c_source(&mut c, cxx_mode, opts.c_file);
            // Plan 218: prebuilt archive replaces the individual rt_* source
            // args below when available (see `use_rt_archive` computed above).
            if let Some(cfg) = &rt_archive {
                c.arg(&cfg.lib_file);
            } else {
                arg_c_source(&mut c, cxx_mode, &rt_alloc);
                arg_c_source(&mut c, cxx_mode, &rt_effects);
                arg_c_source(&mut c, cxx_mode, &rt_fibers);
                arg_c_source(&mut c, cxx_mode, &rt_fiber_arena); /* Plan 44.2 Etap 1 */
                arg_c_source(&mut c, cxx_mode, &rt_fiber_arena_win); /* Plan 82 Ф.1 */
                arg_c_source(&mut c, cxx_mode, &rt_fiber_stats); /* Plan 44.2 Etap 3 */
                arg_c_source(&mut c, cxx_mode, &rt_runtime); /* Plan 44 Этап 0 */
                arg_c_source(&mut c, cxx_mode, &rt_driver); /* Plan 83.11 Ф.2 */
                arg_c_source(&mut c, cxx_mode, &rt_typeid); /* Plan 61 Ф.1 */
                arg_c_source(&mut c, cxx_mode, &rt_segv_diag); /* Plan 83.11 §12.31 */
            }
            // [M-linux-mn-conformance-red] fix: libuv object/library
            // placement, non-Windows only — see the comment at the early
            // libuv defines block above (`-DNOVA_USE_LIBUV=1` site). Must
            // come AFTER `opts.c_file` + `libnova_rt.a`/individual rt_*
            // sources (just above) so `ld` sees the libuv-dependent
            // references BEFORE `libuv.a` on the command line.
            #[cfg(not(target_os = "windows"))]
            if let (Some(_inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                // Plan 83.12/183: net.c compiled only when libuv is present.
                // Plan 218: already inside libnova_rt.a when the archive is active
                // — skip re-adding as a loose source (would double-define symbols).
                if !use_rt_archive {
                    arg_c_source(&mut c, cxx_mode, &rt_net);
                    // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                    arg_c_source(&mut c, cxx_mode, &rt_fs);
                    // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                    arg_c_source(&mut c, cxx_mode, &rt_process);
                    arg_c_source(&mut c, cxx_mode, evloop);
                }
                /* Linux ld обрабатывает .a archives только для symbols
                 * undefined в момент когда archive seen. Используем
                 * --start-group / --end-group чтобы symbols искались
                 * commutative с object files в command line. */
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--start-group");
                c.arg(lib_path);
                for syslib in LIBUV_UNIX_SYSLIBS {
                    c.arg(syslib);
                }
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--end-group");
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: system libs (-l) в link phase.
            // Plan 193 Ф.2 gap-1: lib_dirs (-L) BEFORE -l so the linker's
            // search path includes non-default-path native libs (mirrors
            // detect_mbedtls's now-generalized -L <lib_dir> pattern).
            if let Some(ffi) = opts.ffi {
                for dir in &ffi.lib_dirs {
                    c.arg("-L").arg(dir);
                }
                for lib in &ffi.libs {
                    c.arg(format!("-l{}", lib));
                }
            }
            c
        }
        Toolchain::Msvc { env, .. } => {
            // cl.exe с pre-captured vcvars env (no bat overhead per compile).
            let mut c = Command::new("cl.exe");
            c.env_clear().envs(env.iter().cloned());
            match opts.mode {
                Mode::Dev => {
                    // /Z7 (а НЕ /Zi): CodeView в .obj без PDB. /Zi
                    // создаёт vc<N>.pdb в cwd (cl-проектная PDB); при
                    // параллельном `nova test` (16 jobs) все cl.exe'ы
                    // лезут в одну PDB → C1041 «cannot open program
                    // database». /Z7 даёт ту же отладочную информацию
                    // без shared-PDB contention (стандартное решение
                    // для параллельных билдов: Ninja/MSBuild делают
                    // также для unity-сборок).
                    c.args(["/nologo", "/W0", "/Od", "/Z7"]);
                    // Plan 140 Ф.1 (D24 amend): `/DNOVA_CONTRACTS_RUNTIME=1`
                    // снят — контракты эмитятся безусловно (enforce-with-elision),
                    // `#ifdef` на codegen больше нет. Проверяется в debug И release.
                }
                Mode::Release => {
                    c.args(["/nologo", "/W0", "/O2", "/DNDEBUG"]);
                }
            }
            // /std: НЕ задаём. MSVC default («Microsoft C») — permissive
            // C99+/C11+ с расширениями: codegen эмитит struct-cast
            // `(nova_str)(x)` (GCC/Clang extension), валидный в permissive
            // mode, но в strict /std:c11 → C2440 «cannot convert struct».
            // compat-header диспатчит по `sizeof`, не по `_Generic` —
            // работает без /std:c11.
            // Plan 82 followup: GCC/Clang builtin compat для cl.exe.
            // Runtime использует __atomic_* / __builtin_*-builtin'ы (sync.h
            // Tier-1 — clang); MSVC их не имеет → C2065. /FI force-инклюдит
            // compat-header в КАЖДЫЙ TU (генерированный тест-код + nova_rt
            // .c) до любых других include'ов; macros/inline-функции
            // отображают GCC builtin'ы на _Interlocked* / _BitScan* / rdtsc.
            // Под clang-cl (`__clang__` defined) compat-header — no-op.
            c.arg("/FI").arg(opts.rt_dir.join("nova_msvc_compat.h"));
            // Plan 81 Ф.7.1: /Gy — function-level linking (каждая функция
            // в свой COMDAT); link.exe /OPT:REF (default в release) удаляет
            // неиспользуемые. MSVC-эквивалент -ffunction-sections.
            c.arg("/Gy");
            // Plan 44.5: NOVA_GC_BOEHM + GC_THREADS — Boehm compiled with -DGC_THREADS;
            // client must define it too for GC_register_my_thread API (M:N workers).
            // ВАЖНО: кавычки в аргументы НЕ добавляем вручную. `Command`
            // сам экранирует каждый аргумент для CreateProcess по правилам
            // MSVC CRT (см. clang-ветку — там пути передаются «сырыми»).
            // Ручная кавычка `/Fo"path\\"` попадёт в argv буквально → cl.exe
            // видит кавычку как часть имени → D8036 «invalid /Fo». Путь с
            // пробелом обрабатывается экранированием Command автоматически.
            if opts.gc_kind == GcKind::Boehm {
                c.arg("/DNOVA_GC_BOEHM");
                c.arg("/DGC_THREADS");
                c.arg(format!("/I{}", vcpkg_include.display()));
            }
            // Plan 149 D233: nova.toml [runtime] → /DNOVA_FIBER_STACK_DEFAULT /
            // /DNOVA_FIBERS_PER_WORKER_DEFAULT (raw ints). After GC, before libuv.
            for da in runtime_define_args(opts.runtime, "/D") {
                c.arg(da);
            }
            // Plan 174.4: effect-registry size (all TUs, ABI-uniform).
            if let Some(da) = effect_count_define_arg(opts.c_file, "/D") {
                c.arg(da);
            }
            c.arg(format!("/I{}", opts.cg_include.display()));
            // /Fo с завершающим '\' → cl.exe трактует как директорию
            // (каждый .obj по имени исходника); без '\' — как имя файла,
            // что с несколькими source-файлами даёт D8036.
            c.arg(format!("/Fo{}\\", opts.obj_dir.display()));
            c.arg(format!("/Fe{}", opts.exe_file.display()));
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI shim flags (MSVC).
            // include_dirs → /I; .c shims → compilation units;
            // .h shims → /FI<header> (force-include); libs → /link <name>.lib.
            if let Some(ffi) = opts.ffi {
                for inc in &ffi.include_dirs {
                    c.arg(format!("/I{}", inc.display()));
                }
                // Plan 193 Ф.2 gate-3: generic feature-gate defines — see
                // `ffi_have_defines` doc-comment.
                for def in ffi_have_defines(ffi) {
                    c.arg(format!("/D{}=1", def));
                }
                for shim in &ffi.c_shims {
                    let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("c") {
                        c.arg(shim);
                    } else if ext.eq_ignore_ascii_case("h") {
                        c.arg("/FI").arg(shim);
                    }
                }
            }
            // Plan 22: libuv for MSVC.
            if let (Some(inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("/DNOVA_USE_LIBUV=1");
                c.arg(format!("/I{}", inc_path.display()));
                // Plan 83.12/183: net.c compiled only when libuv is present.
                // Plan 218: already inside libnova_rt.lib when the archive is
                // active — skip re-adding as a loose source (double-define).
                if !use_rt_archive {
                    c.arg(&rt_net);
                    // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                    c.arg(&rt_fs);
                    // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                    c.arg(&rt_process);
                    c.arg(evloop);
                }
                c.arg(lib_path);
                #[cfg(target_os = "windows")]
                for syslib in LIBUV_WIN_SYSLIBS {
                    c.arg(syslib);
                }
            }
            c.arg(opts.c_file);
            // Plan 218: prebuilt archive replaces the individual rt_* source
            // args below when available (see `use_rt_archive` computed above).
            if let Some(cfg) = &rt_archive {
                c.arg(&cfg.lib_file);
            } else {
                c.arg(&rt_alloc);
                c.arg(&rt_effects);
                c.arg(&rt_fibers);
                c.arg(&rt_fiber_arena); /* Plan 44.2 Etap 1 */
                c.arg(&rt_fiber_arena_win); /* Plan 82 Ф.1 */
                c.arg(&rt_fiber_stats); /* Plan 44.2 Etap 3 */
                c.arg(&rt_runtime); /* Plan 44 Этап 0 */
                c.arg(&rt_driver); /* Plan 83.11 Ф.2 */
                c.arg(&rt_typeid); /* Plan 61 Ф.1 */
                c.arg(&rt_segv_diag); /* Plan 83.11 §12.31 */
            }
            // Plan 27 Ф.1: Boehm link flags for MSVC (after sources, before /link).
            // Plan 115 D214 [M-115-ffi-build-pipeline]: also pass user FFI libs.
            // Plan 193 Ф.2 gap-1: also open the /link phase when lib_dirs is
            // declared alone (e.g. libs list empty but search path wired for
            // a future c_shim-only consumer — matches `!libs.is_empty()` OR
            // gate below).
            let has_link_phase = opts.gc_kind == GcKind::Boehm
                || opts
                    .ffi
                    .map_or(false, |f| !f.libs.is_empty() || !f.lib_dirs.is_empty());
            if has_link_phase {
                c.arg("/link");
                // Plan 198 (defect #8a insurance, NOT the fix — see the
                // matching clang-branch comment above): modest RESERVE-only
                // stack safety margin.
                c.arg("/STACK:0x1000000"); // 16 MiB reserve
                if opts.gc_kind == GcKind::Boehm {
                    // PathBuf-аргумент — Command экранирует сам; ручные кавычки
                    // не нужны (и вредны, см. комментарий к /Fo выше).
                    c.arg(vcpkg_lib.join("gc.lib"));
                    // #269 Ф.2: conditional on existing — vcpkg's bdwgc port
                    // links a separate `atomic_ops.lib` (its own port
                    // dependency), but the #269 Ф.2 fallback build (vendored
                    // bdwgc amalgamation) doesn't produce one: the needed
                    // atomics are header-only on x86_64 MSVC (confirmed
                    // empirically — see `detect_or_build_boehm_fallback`
                    // doc), so there is nothing to link there. Guarding on
                    // `.is_file()` keeps the vcpkg path byte-identical
                    // (file always present there) while making the
                    // fallback path not fail with a "cannot open
                    // atomic_ops.lib" linker error over a file that was
                    // never needed.
                    let atomic_ops_lib = vcpkg_lib.join("atomic_ops.lib");
                    if atomic_ops_lib.is_file() {
                        c.arg(atomic_ops_lib);
                    }
                }
                if let Some(ffi) = opts.ffi {
                    // Plan 193 Ф.2 gap-1: /LIBPATH: BEFORE bare <name>.lib
                    // entries — MSVC's `LIB` env var (vcvars-snapshot,
                    // env_clear()-isolated above) doesn't see external
                    // overrides, so a non-default-path native lib is
                    // otherwise unfindable on Windows (no system default
                    // search path analogue to /usr/lib).
                    for dir in &ffi.lib_dirs {
                        c.arg(format!("/LIBPATH:{}", dir.display()));
                    }
                    for lib in &ffi.libs {
                        // MSVC: -l<name> не поддерживается, нужен <name>.lib.
                        c.arg(format!("{}.lib", lib));
                    }
                }
            }
            c
        }
        Toolchain::Gcc { gcc } => {
            let mut c = Command::new(gcc);
            match opts.mode {
                Mode::Dev => {
                    c.args(["-O0", "-g", "-w"]);
                    // Plan 140 Ф.1 (D24 amend): `-DNOVA_CONTRACTS_RUNTIME=1`
                    // снят — контракты эмитятся безусловно (enforce-with-elision).
                }
                Mode::Release => {
                    c.arg("-O3");
                    c.arg("-flto");
                    c.arg(format!("-march={}", march));
                    c.arg("-DNDEBUG");
                    c.arg("-w");
                }
            }
            // Plan 81 Ф.7.1: linker-level DCE (GNU ld удаляет
            // неиспользуемые секции).
            c.arg("-ffunction-sections");
            c.arg("-fdata-sections");
            c.arg("-Wl,--gc-sections");
            // Plan 44.5: NOVA_GC_BOEHM + GC_THREADS for M:N worker thread registration.
            if opts.gc_kind == GcKind::Boehm {
                c.arg("-DNOVA_GC_BOEHM");
                c.arg("-DGC_THREADS");
            }
            // Plan 149 D233: nova.toml [runtime] → -DNOVA_FIBER_STACK_DEFAULT /
            // -DNOVA_FIBERS_PER_WORKER_DEFAULT (raw ints). After GC, before libuv.
            for da in runtime_define_args(opts.runtime, "-D") {
                c.arg(da);
            }
            // Plan 174.4: effect-registry size (all TUs, ABI-uniform).
            if let Some(da) = effect_count_define_arg(opts.c_file, "-D") {
                c.arg(da);
            }
            c.arg("-I").arg(opts.cg_include);
            // Plan 22 libuv (Linux): defines + include path only here.
            // [M-linux-mn-conformance-red] fix (2026-07-20): object/library
            // placement (rt_net.c/rt_fs.c loose sources + libuv.a itself)
            // MOVED below, after `opts.c_file` + rt-archive/individual rt_*
            // sources — same root cause + fix rationale as the Clang branch
            // above (GNU `ld` only resolves archive members against symbols
            // undefined AT THE POINT the archive is seen; `libuv.a` was
            // being placed BEFORE `libnova_rt.a`, so `uv_strerror` etc.
            // (referenced from fibers.h, folded into the rt-archive) never
            // resolved — confirmed empirically, `undefined reference to
            // uv_strerror`).
            if let (Some(inc_path), Some(_lib_path), Some(_evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                c.arg("-DNOVA_USE_LIBUV=1");
                c.arg("-I").arg(inc_path);
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI shim flags (GCC).
            // .h shims via -include (force-include); .c via compilation unit.
            if let Some(ffi) = opts.ffi {
                for inc in &ffi.include_dirs {
                    c.arg("-I").arg(inc);
                }
                // Plan 193 Ф.2 gate-3: generic feature-gate defines — see
                // `ffi_have_defines` doc-comment.
                for def in ffi_have_defines(ffi) {
                    c.arg(format!("-D{}=1", def));
                }
                for shim in &ffi.c_shims {
                    let ext = shim.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext.eq_ignore_ascii_case("c") {
                        c.arg(shim);
                    } else if ext.eq_ignore_ascii_case("h") {
                        c.arg("-include").arg(shim);
                    }
                }
            }
            c.arg("-o").arg(opts.exe_file);
            c.arg(opts.c_file);
            // Plan 218: prebuilt archive replaces the individual rt_* source
            // args below when available (see `use_rt_archive` computed above).
            if let Some(cfg) = &rt_archive {
                c.arg(&cfg.lib_file);
            } else {
                c.arg(&rt_alloc);
                c.arg(&rt_effects);
                c.arg(&rt_fibers);
                c.arg(&rt_fiber_arena); /* Plan 44.2 Etap 1 */
                c.arg(&rt_fiber_arena_win); /* Plan 82 Ф.1 */
                c.arg(&rt_fiber_stats); /* Plan 44.2 Etap 3 */
                c.arg(&rt_runtime); /* Plan 44 Этап 0 */
                c.arg(&rt_driver); /* Plan 83.11 Ф.2 */
                c.arg(&rt_typeid); /* Plan 61 Ф.1 */
                c.arg(&rt_segv_diag); /* Plan 83.11 §12.31 */
            }
            // [M-linux-mn-conformance-red] fix: libuv object/library
            // placement — see the comment at the early libuv defines block
            // above. Must come AFTER `opts.c_file` + `libnova_rt.a`/
            // individual rt_* sources (just above) so `ld` sees the
            // libuv-dependent references BEFORE `libuv.a` on the command line.
            if let (Some(_inc_path), Some(lib_path), Some(evloop)) =
                (&libuv_include, &libuv_lib, &libuv_eventloop)
            {
                // Plan 83.12/183: net.c compiled only when libuv is present.
                // Plan 218: already inside libnova_rt.a when the archive is
                // active — skip re-adding as a loose source (double-define).
                if !use_rt_archive {
                    c.arg(&rt_net);
                    // Plan 176 Ф.2: fs.c — std/fs backend, same libuv gate.
                    c.arg(&rt_fs);
                    // Plan 265 Ф.1: process.c — std/os subprocess backend, same libuv gate.
                    c.arg(&rt_process);
                    c.arg(evloop);
                }
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--start-group");
                c.arg(lib_path);
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                for syslib in LIBUV_UNIX_SYSLIBS {
                    c.arg(syslib);
                }
                #[cfg(target_os = "linux")]
                c.arg("-Wl,--end-group");
            }
            // Plan 115 D214 [M-115-ffi-build-pipeline]: user FFI libs (GCC).
            // Plan 193 Ф.2 gap-1: lib_dirs (-L) BEFORE -l, same as Clang.
            if let Some(ffi) = opts.ffi {
                for dir in &ffi.lib_dirs {
                    c.arg("-L").arg(dir);
                }
                for lib in &ffi.libs {
                    c.arg(format!("-l{}", lib));
                }
            }
            // Plan 27 Ф.1+Ф.D: Boehm link flags for GCC.
            if opts.gc_kind == GcKind::Boehm {
                if let Some(cfg) = &boehm_cfg {
                    if let Some(inc) = &cfg.include_dir {
                        let s = inc.to_string_lossy();
                        if !s.starts_with("/usr/include") {
                            c.arg("-I").arg(inc);
                        }
                    }
                    if let Some(lib) = &cfg.lib_dir {
                        c.arg("-L").arg(lib);
                    }
                }
                c.arg("-lgc");
                #[cfg(target_os = "linux")]
                c.arg("-lpthread");
            }
            c
        }
    }
}

/// Plan 28 Ф.0: публичная обёртка над `build_command` + `run_with_timeout`.
/// Используется из `nova-cli` (`nova build`) минуя subprocess.
///
/// Компилирует `opts.c_file` → `opts.exe_file` через выбранный toolchain.
/// Возвращает путь к exe на success, anyhow::Error на fail.
pub fn compile_c_to_exe(
    tc: &Toolchain,
    opts: &BuildOpts,
    timeout: Duration,
) -> anyhow::Result<PathBuf> {
    // Plan 27 Ф.D + #269 Ф.2: graceful exit (or vendored fallback build) если
    // backend = Boehm и libgc не найден.
    let _ = resolve_gc_or_exit(opts.gc_kind, opts.cg_include, opts.rt_dir, tc.vcvars_path());
    let cmd = build_command(tc, opts);
    let out = run_with_timeout(cmd, timeout).map_err(|e| anyhow!("spawn compiler: {:#}", e))?;
    let ok = out.status.map(|s| s.success()).unwrap_or(false);
    if !ok {
        let stderr = bytes_to_string(&out.stderr);
        let stdout = bytes_to_string(&out.stdout);
        let detail = if stderr.is_empty() { stdout } else { stderr };
        let reason = if out.status.is_none() {
            format!("compiler timed out after {:.1}s", timeout.as_secs_f64())
        } else {
            // 975: назвать СИГНАЛ, а не только «ошибка компилятора». На
            // двухъядерном раннере CI компилятор и линкер убивает OOM-killer,
            // и тогда `detail` пуст, а сообщение молчит о причине. Приём тот
            // же, что на уже починенном пути запуска (~4108).
            let sig = out.status.map(|st| signal_note(&st)).unwrap_or_default();
            format!("compiler error{}:\n{}", sig, detail.trim())
        };
        return Err(anyhow!("{}", reason));
    }
    Ok(opts.exe_file.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::build_command;
    use crate::{BuildOpts, GcKind, Mode, RuntimeConfig, Toolchain};
    use std::path::Path;

    #[test]
    fn gcc_command_keeps_runtime_defines_and_native_inputs() {
        let runtime = RuntimeConfig {
            fiber_stack: Some("2MB".into()),
            fibers_per_worker: Some("128".into()),
        };
        let opts = BuildOpts {
            c_file: Path::new("entry.c"),
            exe_file: Path::new("out.exe"),
            obj_dir: Path::new("obj"),
            cg_include: Path::new("include"),
            rt_dir: Path::new("nova_rt"),
            mode: Mode::Dev,
            libuv: None,
            gc_kind: GcKind::Malloc,
            ffi: None,
            runtime: Some(&runtime),
        };
        let command = build_command(&Toolchain::Gcc { gcc: "gcc".into() }, &opts);
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"-DNOVA_FIBER_STACK_DEFAULT=2097152".to_string()));
        assert!(args.contains(&"-DNOVA_FIBERS_PER_WORKER_DEFAULT=128".to_string()));
        assert!(args.contains(&"entry.c".to_string()));
        assert!(args.contains(&"out.exe".to_string()));
        assert!(args.iter().any(|arg| Path::new(arg).ends_with("effects.c")));
    }
}

// ---------- Plan 209 Ф.2: multi-TU parallel compile + link ----------

/// Build a `clang -c <src> -o <obj>` compile-only Command sharing `flags`/
/// `includes`/`force_includes` with every other part/runtime-object compile
/// in this CU (ABI-invariant — see `compile_multi_tu_to_exe` doc).
pub fn clang_compile_obj_cmd(
    clang: &Path,
    env: &[(OsString, OsString)],
    flags: &[String],
    includes: &[PathBuf],
    force_includes: &[PathBuf],
    src: &Path,
    obj: &Path,
) -> Command {
    let mut c = Command::new(clang);
    if !env.is_empty() {
        c.env_clear().envs(env.iter().cloned());
    }
    for f in flags {
        if !f.is_empty() {
            c.arg(f);
        }
    }
    for inc in includes {
        c.arg("-I").arg(inc);
    }
    for fi in force_includes {
        c.arg("-include").arg(fi);
    }
    c.arg("-c").arg(src);
    c.arg("-o").arg(obj);
    c
}

/// GCC mirror of [`clang_compile_obj_cmd`] (no vcvars env to replay).
pub fn gcc_compile_obj_cmd(
    gcc: &Path,
    flags: &[String],
    includes: &[PathBuf],
    force_includes: &[PathBuf],
    src: &Path,
    obj: &Path,
) -> Command {
    let mut c = Command::new(gcc);
    for f in flags {
        if !f.is_empty() {
            c.arg(f);
        }
    }
    for inc in includes {
        c.arg("-I").arg(inc);
    }
    for fi in force_includes {
        c.arg("-include").arg(fi);
    }
    c.arg("-c").arg(src);
    c.arg("-o").arg(obj);
    c
}
