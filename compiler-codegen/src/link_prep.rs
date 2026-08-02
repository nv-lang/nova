//! Ф.1 (#268 [M-tls-vendor-autobuild-not-on-build-path], 2026-08-02): shared
//! link-preparation module — detect + auto-build vendored `[ffi]` native
//! libraries BEFORE linking a compile unit, called from BOTH build paths:
//!
//!   - `nova test` (`test_runner::run_one`) — detect-and-degrade: a still-
//!     missing lib after the auto-build attempt degrades to
//!     `SkipReason::FfiLibNotFound` (unchanged, see call site).
//!   - `nova build` (`nova-cli::cmd_build`) — loud diagnostic:
//!     `diagnose_missing_vendor_ffi` prints a FATAL, actionable message
//!     naming the offending package/lib and exits, instead of falling
//!     through to a cryptic linker error (`lld-link: could not open
//!     'mbedtls.lib'`) — the #268 symptom.
//!
//! Extracted from `test_runner.rs` (was already `pub` there and called from
//! both `run_one` and `cmd_build` via `test_runner::build_missing_vendor_ffi_libs`
//! since commit c137d2d9b / backlog #152 — this module gives that shared
//! mechanism its own home named for what it does, and adds the loud-
//! diagnostic layer #268 asked for). `ResolvedFfiConfig` itself, and its
//! `from_manifest`/`merge`, stay in `test_runner.rs` — too many other
//! call sites there (`BuildOpts`, `build_command`'s 3 toolchain branches)
//! to move without an unrelated-risk churn; this module borrows the type,
//! not owns it.

use crate::test_runner::{bytes_to_string, collect_c_files, strip_verbatim_prefix, ResolvedFfiConfig};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

/// Plan 193 Ф.2 gap-1: platform-specific candidate file names for a
/// declared `[ffi] libs` entry `name` — mirrors what each toolchain branch
/// in `build_command` actually emits (`<name>.lib` on Windows regardless of
/// Clang/MSVC — both target the MSVC ABI/lib format; `lib<name>.a` /
/// `lib<name>.so` on Linux, `lib<name>.a` / `lib<name>.dylib` on macOS).
fn ffi_lib_candidate_names(lib: &str) -> Vec<String> {
    if cfg!(target_os = "windows") {
        vec![format!("{}.lib", lib)]
    } else if cfg!(target_os = "macos") {
        vec![format!("lib{}.a", lib), format!("lib{}.dylib", lib)]
    } else {
        vec![format!("lib{}.a", lib), format!("lib{}.so", lib)]
    }
}

/// Plan 193 Ф.2 gap-1: detect-and-degrade probe for generic `[ffi] libs`.
/// Mirrors the retired built-in MbedtlsConfig/BrotliConfig contract
/// (missing native lib → graceful degrade, never a hard link error) —
/// generalized to ANY user-declared `[ffi] libs` entry that has an
/// explicit `lib_dirs` search path. Returns the first lib name that could
/// not be located in any declared `lib_dirs`, plus the dirs searched (for
/// the SkipReason message / the loud build-path diagnostic below).
///
/// `lib_dirs` empty (no explicit search path declared) → None: nothing to
/// verify against, falls back to the toolchain's own default search (system
/// `-l` resolution) — unchanged legacy behaviour; a hard link error is
/// still possible there, same as before this fix (no regression for
/// existing consumers relying on system-installed libs with no
/// non-default path, e.g. `-lsqlite3` found via the system linker path).
pub fn first_missing_ffi_lib(ffi: &ResolvedFfiConfig) -> Option<(String, Vec<PathBuf>)> {
    if ffi.lib_dirs.is_empty() {
        return None;
    }
    for lib in &ffi.libs {
        let candidates = ffi_lib_candidate_names(lib);
        let found = ffi.lib_dirs.iter()
            .any(|dir| candidates.iter().any(|name| dir.join(name).is_file()));
        if !found {
            return Some((lib.clone(), ffi.lib_dirs.clone()));
        }
    }
    None
}

/// [M-vendor-ffi-build-race-in-git-dep-cache] (backlog #152): global lock
/// serializing `build_missing_vendor_ffi_libs` — see its doc-comment for
/// the full race explanation. Plain `Mutex<()>` (not an `OnceLock`-wrapped
/// memo like `RT_ARCHIVE_MEMO`): the disk-based `is_built` check IS the
/// cache (cheap `Path::is_file` stats), so no separate in-memory memo is
/// needed — only the lock itself.
static VENDOR_FFI_BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Plan 193 Ф.2 gate-3 (mbedtls-vendored, 2026-07-12): generic
/// build-and-cache for `[ffi] vendor_src_dirs` — "195-pattern" extension of
/// the `detect_or_build_libuv` precedent (см. `test_runner::build_libuv_lib`,
/// whose cc-invocation shape this mirrors 1:1) generalized to ANY
/// user-declared native module. NOT mbedTLS-specific: compiles whatever
/// `.c` files a package vendors under its declared dirs, no knowledge of
/// what library it is.
///
/// No-op (Ok, nothing built) when `vendor_src_dirs`, `lib_dirs` or `libs`
/// is empty — unchanged legacy behaviour for every existing `[ffi]`
/// consumer that doesn't opt in. Cache check: if EVERY name in `libs`
/// already resolves to a file in `lib_dirs[0]` (via
/// `ffi_lib_candidate_names`), returns immediately without invoking the
/// compiler — cheap per-call check, same shape as
/// `detect_or_build_libuv`'s `lib_file.is_file()` fast path (this function
/// is called once per test, so the cache check dominates after the first
/// build in a `nova test` run).
///
/// Build: collects all `.c` files directly under each `vendor_src_dirs`
/// entry (non-recursive — flat `library/`-style upstream layouts), compiles
/// them with the SAME toolchain flags `build_libuv_lib` uses
/// (`/MT /O2` MSVC / `-O2 -fPIC` cc — CRT/PIC consistency with the rest of
/// the nova runtime is required for static linking to succeed), then
/// archives the resulting objects into `lib_dirs[0]` under EVERY name in
/// `libs` — identical combined archives (a static-archive linker only pulls
/// object members needed to resolve outstanding externals, so the same
/// content appearing under 3 different archive names, as mbedTLS's
/// `mbedtls`/`mbedx509`/`mbedcrypto` split needs, is harmless: whichever
/// archive is scanned first when a symbol is still unresolved satisfies it,
/// the others contribute nothing further). A real per-library object-list
/// split (matching upstream's own `CMakeLists.txt` `src_crypto`/`src_x509`/
/// `src_tls` sets) would avoid the redundant duplication but needs
/// per-package source-list config this generic mechanism intentionally
/// does not have — "минимально" per Plan 193 Ф.2 gate-3 scope.
///
/// Build failures are NOT fatal here — they're logged (eprintln) and
/// swallowed (Ok(())) so the caller decides what to do next:
///   - `nova test` (`run_one`): the EXISTING `first_missing_ffi_lib`
///     detect-and-degrade probe (called right after this, at the call
///     site) still runs and degrades gracefully to
///     `SkipReason::FfiLibNotFound` instead of a hard test-runner crash.
///   - `nova build` (`cmd_build`): `diagnose_missing_vendor_ffi` (below,
///     also called right after this) turns a still-missing lib into a
///     LOUD, actionable FATAL diagnostic instead of a cryptic linker
///     error — the #268 fix.
/// Either way this function only ever tries to IMPROVE on the outcome,
/// never regresses it.
///
/// [M-nova-build-vendor-ffi-no-autobuild] (2026-07-15): `pub` (was private
/// to `test_runner.rs`) so `nova-cli::cmd_build` can call it too — `nova
/// build` used to skip this step entirely (only `nova test`'s `run_one`
/// called it), so any example with a `vendor_src_dirs` native dep (e.g.
/// nova-tls's vendored mbedTLS) failed to link under plain `nova build`
/// unless the archives had been pre-built by a prior `nova test` run or
/// copied in by hand. `cmd_build` now calls this right after merging its
/// own + dependency `[ffi]` config, mirroring `run_one`'s call site 1:1
/// (same no-op/cache/swallow contract described above — a build failure
/// here just falls through to the next stage, which is either the SKIP
/// probe or the loud diagnostic, never straight to the raw link step
/// unguarded).
///
/// [M-vendor-ffi-build-race-in-git-dep-cache] (backlog #152, fixed):
/// serialize ALL vendor-FFI builds process-wide behind ONE global mutex
/// (`VENDOR_FFI_BUILD_LOCK`), held across the WHOLE re-check(disk) ->
/// build sequence — mirrors `detect_or_build_rt_archive`'s
/// `RT_ARCHIVE_MEMO` fix ([M-218-rt-archive-parallel-jobs-race]) 1:1.
/// `nova test --jobs N` runs its worker pool as N THREADS inside ONE
/// process, all calling this fn concurrently (once per test file, for
/// every `[ffi]` provider that test file's package/deps declare). Without
/// a lock held across the full sequence, two threads can both observe
/// "not yet built" on a cold cache and race to `remove_dir_all`+recreate
/// the SAME `.vendor-obj/` concurrently — one thread's `cl.exe` mid-write
/// when the other deletes the directory out from under it (`C1083:
/// Permission denied`, the ORIGINAL #152 symptom, self-healing only
/// because the second attempt runs uncontended). A single global lock
/// (not per-target_dir) is deliberate — vendor-FFI builds are a one-time
/// cold-cache cost per `nova test` run (the disk `already_built` check
/// that dominates every call after the first short-circuits before ever
/// touching the lock), so serializing DIFFERENT providers' builds too
/// costs nothing that matters in practice, and avoids needing a lock
/// registry keyed by canonicalized path.
///
/// Second half of the #152 fix lives at the CALL SITE (`run_one` /
/// `cmd_build`): each declared `[ffi]` provider (own package + every
/// dependency) is now built here SEPARATELY, one `ResolvedFfiConfig` per
/// provider, BEFORE the configs are merged for the link step — never call
/// this fn with a config that has already merged srcs/libs from more than
/// one provider. Merging first (the old behaviour) compiled DIFFERENT
/// vendor libraries' `.c` files together into ONE flat `.vendor-obj/` dir
/// and archived the combined object set under EVERY provider's lib names —
/// silently WRONG even single-threaded, not just racy: mbedTLS's
/// `library/platform.c` and brotli's `common/platform.c` share a basename,
/// so MSVC's directory-mode `/Fo` (object auto-named by basename) let
/// whichever compiled second clobber the first's `.obj`, producing
/// byte-identical-size `.lib` files for BOTH providers that were each
/// missing the OTHER'S symbols (observed as `undefined symbol:
/// BrotliDefaultAllocFunc` on files that never touch TLS — see
/// nova-polaris-tls PROGRESS.md repro).
pub fn build_missing_vendor_ffi_libs(ffi: &ResolvedFfiConfig, vcvars: Option<&Path>) {
    if ffi.vendor_src_dirs.is_empty() || ffi.lib_dirs.is_empty() || ffi.libs.is_empty() {
        return;
    }
    let target_dir = &ffi.lib_dirs[0];
    let is_built = || ffi.libs.iter().all(|lib| {
        let candidates = ffi_lib_candidate_names(lib);
        candidates.iter().any(|name| target_dir.join(name).is_file())
    });
    if is_built() {
        return;
    }
    // Hold the lock across the WHOLE re-check+build sequence below — see
    // VENDOR_FFI_BUILD_LOCK doc above.
    let _guard = match VENDOR_FFI_BUILD_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if is_built() {
        return; // another thread finished the build while we waited.
    }
    // Group `.c` sources by their ORIGINATING `vendor_src_dirs` entry
    // (rather than one flat Vec) — see `build_vendor_ffi_lib` doc for why:
    // each group compiles into its own obj subdirectory so two entries
    // that happen to ship a same-named source file (e.g. this package's
    // own `dec/static_init.c` vs `common/static_init.c`) can never clobber
    // each other's `.obj`.
    let mut srcs_by_dir: Vec<Vec<PathBuf>> = Vec::new();
    for dir in &ffi.vendor_src_dirs {
        let mut srcs: Vec<PathBuf> = Vec::new();
        if let Err(e) = collect_c_files(dir, &mut srcs, /*recursive*/ false) {
            eprintln!("nova: warning: vendor FFI build: read {}: {}", dir.display(), e);
            return;
        }
        srcs_by_dir.push(srcs);
    }
    let total_srcs: usize = srcs_by_dir.iter().map(|v| v.len()).sum();
    if total_srcs == 0 {
        eprintln!("nova: warning: vendor FFI build: no .c files found under {:?}", ffi.vendor_src_dirs);
        return;
    }
    eprintln!(
        "nova: FFI lib(s) {:?} not found in {}, building from vendored source ({} files, one-time)...",
        ffi.libs, target_dir.display(), total_srcs
    );
    if let Err(e) = std::fs::create_dir_all(target_dir) {
        eprintln!("nova: warning: vendor FFI build: create lib_dir {}: {}", target_dir.display(), e);
        return;
    }
    if let Err(e) = build_vendor_ffi_lib(&srcs_by_dir, &ffi.include_dirs, target_dir, &ffi.libs, vcvars) {
        eprintln!("nova: warning: vendor FFI build failed: {}", e);
        // Swallowed — caller's first_missing_ffi_lib (SKIP on the test
        // path) / diagnose_missing_vendor_ffi (FATAL on the build path)
        // handles what happens next.
    }
}

/// Plan 193 Ф.2 gate-3: compile `srcs_by_dir` + archive into `target_dir`
/// under every name in `lib_names` (see `build_missing_vendor_ffi_libs`
/// doc). Object dir: `target_dir/.vendor-obj` (recreated each build
/// attempt, mirrors `build_libuv_lib`'s `obj_dir` handling).
///
/// [M-vendor-ffi-build-race-in-git-dep-cache] (backlog #152): `srcs` is
/// grouped by ORIGINATING `vendor_src_dirs` entry (index `i` in
/// `srcs_by_dir`) — each group is compiled into its OWN subdirectory
/// `.vendor-obj/<i>/` instead of one shared flat directory. Two entries
/// (from the SAME provider's own multi-dir `vendor_src_dirs`, e.g.
/// brotli's `dec/` + `common/`, or — before the call-site fix — from TWO
/// DIFFERENT providers merged together, e.g. mbedTLS + brotli) can ship a
/// source file with the same basename (`static_init.c`, `platform.c`);
/// MSVC's directory-mode `/Fo` (and the Unix branch's `basename.o`
/// naming) auto-name objects by basename only, so a shared flat dir would
/// let the group compiled LATER silently clobber the EARLIER group's
/// `.obj` — no compiler error, just a missing symbol at final link time.
/// Per-group subdirectories make that collision structurally impossible.
fn build_vendor_ffi_lib(srcs_by_dir: &[Vec<PathBuf>], include_dirs: &[PathBuf], target_dir: &Path,
                         lib_names: &[String], vcvars: Option<&Path>) -> Result<()> {
    let obj_dir = target_dir.join(".vendor-obj");
    if obj_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&obj_dir);
    }
    std::fs::create_dir_all(&obj_dir)
        .map_err(|e| anyhow!("create obj_dir: {}", e))?;
    let total_srcs: usize = srcs_by_dir.iter().map(|v| v.len()).sum();

    #[cfg(target_os = "windows")]
    {
        let vcv = vcvars.ok_or_else(|| anyhow!("vcvars required for vendor FFI build on Windows"))?;
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for (group_idx, srcs) in srcs_by_dir.iter().enumerate() {
            if srcs.is_empty() {
                continue;
            }
            let group_dir = obj_dir.join(group_idx.to_string());
            std::fs::create_dir_all(&group_dir)
                .map_err(|e| anyhow!("create obj group dir: {}", e))?;
            let rsp = group_dir.join("compile.rsp");
            let mut lines: Vec<String> = Vec::new();
            lines.push("/c /nologo /W0 /MT /O2 /D_WIN32_WINNT=0x0602 /DWIN32_LEAN_AND_MEAN \
                         /D_CRT_SECURE_NO_WARNINGS /D_CRT_SECURE_NO_DEPRECATE".to_string());
            for inc in include_dirs {
                lines.push(format!("/I \"{}\"", strip_verbatim_prefix(inc).display()));
            }
            lines.push(format!("/Fo\"{}\\\\\"", strip_verbatim_prefix(&group_dir).display()));
            for s in srcs {
                lines.push(format!("\"{}\"", strip_verbatim_prefix(s).display()));
            }
            // [M-nova-build-vendor-ffi-no-autobuild] follow-up (2026-07-15):
            // `\u{FEFF}` (UTF-8 BOM) prefix — cl.exe/link.exe response files
            // default to the process ANSI codepage when no BOM is present;
            // without it, any non-ASCII byte in a path (e.g. a Windows user
            // profile dir containing Cyrillic characters — exercised for real
            // the first time by THIS call site once vendor autobuild is
            // actually reached instead of short-circuited by a pre-built
            // cache hit, see `build_missing_vendor_ffi_libs` doc) gets
            // misdecoded, corrupting every subsequent source-file path in the
            // rsp and failing with a spurious `C1083: file not found`. BOM
            // makes cl.exe read the file as UTF-8 regardless of the console's
            // active codepage — same fix applied to the `lib.exe` archive rsp
            // below (its object-file paths live under the same tree).
            std::fs::write(&rsp, format!("\u{FEFF}{}", lines.join("\n")))
                .map_err(|e| anyhow!("write rsp: {}", e))?;
            let inner = format!(
                "\"call \"{}\" >nul 2>&1 && cl.exe @\"{}\"\"",
                vcv.display(), rsp.display()
            );
            let mut cmd = Command::new("cmd");
            cmd.raw_arg("/c").raw_arg(&inner);
            let out = cmd.output()
                .map_err(|e| anyhow!("spawn cl.exe: {}", e))?;
            if !out.status.success() {
                let combined = format!("{}{}",
                    bytes_to_string(&out.stdout),
                    bytes_to_string(&out.stderr));
                return Err(anyhow!("vendor FFI compile failed: {}",
                    combined.lines().take(15).collect::<Vec<_>>().join("\n")));
            }
            for entry in std::fs::read_dir(&group_dir)? {
                let p = entry?.path();
                if p.extension().and_then(|s| s.to_str()) == Some("obj") {
                    obj_files.push(p);
                }
            }
        }
        if obj_files.is_empty() {
            return Err(anyhow!("vendor FFI compile produced no .obj files"));
        }
        for lib in lib_names {
            let lib_file = target_dir.join(format!("{}.lib", lib));
            let lib_rsp = obj_dir.join(format!("lib_{}.rsp", lib));
            let mut lib_lines: Vec<String> = Vec::new();
            lib_lines.push("/nologo".to_string());
            lib_lines.push(format!("/OUT:\"{}\"", strip_verbatim_prefix(&lib_file).display()));
            for o in &obj_files {
                lib_lines.push(format!("\"{}\"", strip_verbatim_prefix(o).display()));
            }
            // BOM — see compile.rsp comment above (same non-ASCII-path
            // codepage-misdecode risk; obj_files live under the same tree).
            std::fs::write(&lib_rsp, format!("\u{FEFF}{}", lib_lines.join("\n")))
                .map_err(|e| anyhow!("write lib.rsp: {}", e))?;
            let lib_inner = format!(
                "\"call \"{}\" >nul 2>&1 && lib.exe @\"{}\"\"",
                vcv.display(), lib_rsp.display()
            );
            let mut lib_cmd = Command::new("cmd");
            lib_cmd.raw_arg("/c").raw_arg(&lib_inner);
            let lib_out = lib_cmd.output()
                .map_err(|e| anyhow!("spawn lib.exe: {}", e))?;
            if !lib_out.status.success() {
                return Err(anyhow!("lib.exe failed for {}: {}", lib,
                    bytes_to_string(&lib_out.stderr)));
            }
        }
        eprintln!("nova: vendor FFI lib(s) {:?} built ({} files)", lib_names, total_srcs);
        let _ = std::fs::remove_dir_all(&obj_dir);
        return Ok(());
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
        let mut obj_files: Vec<PathBuf> = Vec::new();
        for (group_idx, srcs) in srcs_by_dir.iter().enumerate() {
            if srcs.is_empty() {
                continue;
            }
            // Per-group subdir — see fn doc for why (basename collisions
            // between different vendor_src_dirs entries, e.g. brotli's
            // own `dec/static_init.c` vs `common/static_init.c`).
            let group_dir = obj_dir.join(group_idx.to_string());
            std::fs::create_dir_all(&group_dir)
                .map_err(|e| anyhow!("create obj group dir: {}", e))?;
            for src in srcs {
                let obj = group_dir.join(
                    src.file_name().unwrap().to_string_lossy().replace(".c", ".o")
                );
                let mut c = Command::new(&cc);
                c.args(["-c", "-O2", "-w", "-fPIC"]);
                for inc in include_dirs {
                    c.arg("-I").arg(inc);
                }
                c.arg("-o").arg(&obj);
                c.arg(src);
                let out = c.output()
                    .map_err(|e| anyhow!("spawn {}: {}", cc, e))?;
                if !out.status.success() {
                    return Err(anyhow!("vendor FFI compile failed on {}: {}",
                        src.display(), bytes_to_string(&out.stderr)));
                }
                obj_files.push(obj);
            }
        }
        for lib in lib_names {
            let lib_file = target_dir.join(format!("lib{}.a", lib));
            let mut ar = Command::new("ar");
            ar.arg("rcs").arg(&lib_file);
            for o in &obj_files {
                ar.arg(o);
            }
            let ar_out = ar.output()
                .map_err(|e| anyhow!("spawn ar: {}", e))?;
            if !ar_out.status.success() {
                return Err(anyhow!("ar failed for {}: {}", lib,
                    bytes_to_string(&ar_out.stderr)));
            }
        }
        eprintln!("nova: vendor FFI lib(s) {:?} built ({} files)", lib_names, total_srcs);
        let _ = std::fs::remove_dir_all(&obj_dir);
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (srcs_by_dir, include_dirs, target_dir, lib_names, vcvars, &obj_dir, total_srcs);
        Err(anyhow!("unsupported platform for vendor FFI build"))
    }
}

/// Ф.1 (#268 [M-tls-vendor-autobuild-not-on-build-path], 2026-08-02):
/// loud, actionable diagnostic for the `nova build` link-prep path. Called
/// AFTER `build_missing_vendor_ffi_libs` has had a chance to auto-build
/// every provider (own package + each dependency, still UNMERGED — same
/// per-provider list `build_missing_vendor_ffi_libs` was called on, now
/// paired with the owning package's name so the message can name the
/// actual offending package instead of an anonymous merged `[ffi]` blob).
///
/// Returns `None` when every provider's declared `libs` are all present —
/// nothing to report, `cmd_build` proceeds to the real link step exactly
/// as before. Returns `Some(message)` when at least one declared lib is
/// STILL missing (auto-build was attempted and either didn't apply — no
/// `vendor_src_dirs` — or genuinely failed, in which case
/// `build_missing_vendor_ffi_libs`'s own `nova: warning: vendor FFI build
/// failed: ...` line is already on stderr above this) — the caller prints
/// it and exits, so `nova build` fails LOUD and early with package/lib/
/// hint context instead of falling through to the real link step and
/// surfacing a cryptic toolchain error (`lld-link: could not open
/// 'mbedtls.lib'`, the #268 symptom) with no indication of WHICH package
/// declared the missing lib or WHY the auto-build didn't cover it.
///
/// `nova test` does NOT call this — `run_one` keeps the existing
/// detect-and-degrade SKIP (`first_missing_ffi_lib` on the MERGED config +
/// `SkipReason::FfiLibNotFound`) unchanged: a missing native lib in a test
/// run degrades that one test gracefully, it must not abort the whole
/// `nova test` invocation (many other tests in the same run don't need
/// this provider at all).
pub fn diagnose_missing_vendor_ffi(providers: &[(String, ResolvedFfiConfig)]) -> Option<String> {
    let mut entries: Vec<String> = Vec::new();
    for (name, ffi) in providers {
        if let Some((lib, searched)) = first_missing_ffi_lib(ffi) {
            let searched_str = searched.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let hint = if ffi.vendor_src_dirs.is_empty() {
                format!(
                    "package `{name}` declares no [ffi] vendor_src_dirs for auto-build \
                     — a prebuilt `{lib}` must be placed manually under one of the \
                     lib_dirs above (see docs/ffi-cookbook.md)."
                )
            } else {
                format!(
                    "package `{name}` declares [ffi] vendor_src_dirs = {:?} — an \
                     automatic build from this vendored source was attempted but did \
                     not produce `{lib}`; see the `nova: warning: vendor FFI build \
                     failed: ...` line above (if any) for the underlying compiler/\
                     linker error, or check that a C toolchain (cl.exe/vcvars, or cc) \
                     is available.",
                    ffi.vendor_src_dirs
                )
            };
            entries.push(format!(
                "  - package `{name}`: [ffi] lib `{lib}` not found (searched: {searched_str})\n    {hint}"
            ));
        }
    }
    if entries.is_empty() {
        return None;
    }
    Some(format!(
        "nova: FATAL missing native [ffi] librar{} before link:\n{}\n\n\
         Aborting `nova build` here (loud, with package/lib context) instead of \
         falling through to the linker's own cryptic \"could not open\" error.",
        if entries.len() == 1 { "y" } else { "ies" },
        entries.join("\n")
    ))
}
