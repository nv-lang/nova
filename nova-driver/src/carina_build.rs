//! Carina-backed `nova build` driver path (274.16 Ш.4).
//!
//! This module owns workspace/seed/toolchain orchestration only. The Nova
//! compiler invocation is always the pinned external Carina B1 `emit`; this
//! path never asks the Rust Oracle to parse, check, or emit Nova source.
//!
//! Manifest/lock/toolchain/runtime helpers come from `nova-driver-support`;
//! this production target has no dependency on `nova_codegen`. Oracle-backed
//! compatibility commands live in the sibling `nova-compat` process.

use anyhow::{anyhow, bail, Context, Result};
use nova_driver_support::{self as driver, BuildOpts, GcKind, Mode, ToolchainOpts, ToolchainPref};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

const SEED_SOURCE: &str = "3729abcce42ec2d94efc898ec6eafe91ff6210d5";
const SEED_ZIP_SHA256: &str = "7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb";
const SEED_EXE_SHA256: &str = "824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4";
const SEED_ASSET: &str = "nova-carina-0.2.1-windows-x86_64.zip";
const SEED_URL: &str = "https://github.com/nv-lang/nova/releases/download/carina-v0.2.1/nova-carina-0.2.1-windows-x86_64.zip";
const DRIVER_STAMP: &str = "NOVA_CARINA_DRIVER_STAMP=carina-b1-build-v1";

#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    entry: &Path,
    output: Option<&Path>,
    mode: &str,
    toolchain: &str,
    vcvars: Option<&Path>,
    clang: Option<&Path>,
    timeout_secs: u64,
    keep_artifacts: bool,
    mono_depth: Option<usize>,
    contracts: &str,
) -> Result<()> {
    if !cfg!(target_os = "windows") {
        bail!("Carina build seed is pinned only for windows-x86_64");
    }
    if timeout_secs == 0 {
        bail!("--timeout must be >= 1 second");
    }
    if mono_depth.is_some() {
        bail!("Carina build does not support --mono-depth yet");
    }
    if contracts != "checked" {
        bail!("Carina build currently supports only --contracts=checked");
    }
    let mode = Mode::parse(mode)?;

    let entry = entry
        .canonicalize()
        .with_context(|| format!("entry not found: {}", entry.display()))?;
    let entry = normalize_verbatim_path(&entry);
    if !entry.is_file()
        || !entry
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("nv"))
    {
        bail!(
            "build entry must be an existing .nv file: {}",
            entry.display()
        );
    }
    let repo = find_workspace_root(&entry)?;
    let rt_dir =
        env_path_override("NOVA_RT_DIR").unwrap_or_else(|| repo.join("compiler-codegen/nova_rt"));
    let cg_include =
        env_path_override("NOVA_CG_INCLUDE").unwrap_or_else(|| repo.join("compiler-codegen"));
    ensure_directory(&rt_dir, "Nova runtime")?;
    ensure_directory(&cg_include, "Nova C headers")?;

    let package_dir = driver::find_package_dir(&entry)
        .ok_or_else(|| anyhow!("missing nova.toml for build entry {}", entry.display()))?;
    // find_package_dir canonicalizes internally, so the path is verbatim
    // (\\?\) while the entry was normalized: strip the prefix here or every
    // later relative-to-package computation (the program stage layout,
    // registry №1889) silently falls back to bare file names.
    let package_dir = normalize_verbatim_path(&package_dir);
    let manifest_path = package_dir.join("nova.toml");
    let manifest = driver::manifest::parse_manifest(&manifest_path, &package_dir)
        .ok_or_else(|| anyhow!("invalid package manifest: {}", manifest_path.display()))?;
    // This is driver-owned package/lock resolution, not compiler work.
    driver::lockfile::sync(&package_dir)
        .map_err(|e| anyhow!("dependency resolution (nova.lock.toml): {:#}", e))?;

    // The program's own module graph is the driver's (274.16 Ш.4, registry
    // №1889): the pinned B1 seed reads ONE program root recursively and
    // groups every directory into one unit, so the compiler's own
    // `novac/src` — right for building Carina itself — hides a user
    // program's modules, and a whole package root would merge a crowded
    // corpus directory into a single unit. Stage exactly the reachable
    // files into a fresh per-build tree with the package-relative layout
    // preserved, and hand THAT root to the seed. A program with no program
    // modules keeps the historical single-file invocation byte-for-byte.
    //
    // Dependency modules join the same staged tree (task 129): without them
    // the seed never sees a dependency's declarations and refuses every
    // consumer with `E_NOVAC_SUBSET` ("no such method in the declarations
    // handed to novac", measured by tasks 122/128).
    let dep_roots = crate::program::collect_dep_roots(&manifest);
    let discovery = crate::program::discover_program_files(&entry, &manifest, &dep_roots)?;

    let seed_capture_dir = env_path_override("NOVA_CARINA_CAPTURE_DIR");
    if seed_capture_dir.is_some() {
        // Consume the driver-only control before seed verification or any
        // later toolchain subprocess can inherit it.
        std::env::remove_var("NOVA_CARINA_CAPTURE_DIR");
    }
    let seed = prepare_seed(&repo)?;
    let target_dir = repo.join("target/carina-build");
    std::fs::create_dir_all(&target_dir)
        .with_context(|| format!("cannot create build directory {}", target_dir.display()))?;
    let stem = entry
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("nova-program");
    let build_id = std::process::id();
    let stale_c_file = target_dir.join(format!("{stem}.carina.c"));
    let c_file = target_dir.join(format!("{stem}.{build_id}.carina.c"));
    let temp_exe = target_dir.join(format!("{stem}.{build_id}.tmp.exe"));
    let obj_dir = target_dir.join(format!("{stem}.{build_id}.obj"));
    let stage_dir = target_dir.join(format!("{stem}.{build_id}.src"));
    let final_exe = output
        .map(PathBuf::from)
        .unwrap_or_else(|| entry.with_extension("exe"));
    remove_file_if_present(&stale_c_file)?;
    remove_file_if_present(&c_file)?;
    cleanup_native_intermediates(&temp_exe, &obj_dir)?;
    // Clear the single-process artifact names written by the first driver
    // probe before PID-scoped outputs were introduced.
    cleanup_native_intermediates(
        &target_dir.join(format!("{stem}.carina.tmp.exe")),
        &target_dir.join(format!("{stem}.obj")),
    )?;

    // Stage the program tree only when the program has modules of its own;
    // the single-file invocation keeps the historical root (the compiler's
    // own tree) and the real entry path, so the D89 golden path is untouched.
    let (emit_entry, self_root) = if discovery.files.is_empty() {
        (entry.clone(), repo.join("novac/src"))
    } else {
        let staged = crate::program::stage_program(
            &stage_dir,
            &manifest.source_root,
            &entry,
            &discovery,
        )?;
        println!(
            "nova build program: {} source file(s) staged under {}",
            discovery.files.len() + 1,
            stage_dir.display()
        );
        (staged, stage_dir.clone())
    };

    let carina_args = vec![
        "emit".to_owned(),
        emit_entry.to_string_lossy().into_owned(),
    ];
    let mut carina = Command::new(&seed.exe);
    configure_seed_command(&mut carina, &carina_args, &repo, &self_root);
    let emitted = carina
        .output()
        .context("could not start pinned Carina B1 emit")?;
    // The staged tree exists for the emit child only; the generated C and
    // the captured diagnostics outlive it. --keep-artifacts keeps it.
    if !keep_artifacts && !discovery.files.is_empty() {
        std::fs::remove_dir_all(&stage_dir)
            .with_context(|| format!("cannot remove program stage {}", stage_dir.display()))?;
    }
    capture_seed_child_if_requested(
        seed_capture_dir.as_deref(),
        &seed.exe,
        &carina_args,
        &repo,
        &self_root,
        &emitted,
    )?;
    if !emitted.status.success() {
        bail!(
            "Carina B1 emit failed (exit {}): {}",
            emitted.status.code().unwrap_or(1),
            emit_failure_detail(&emitted.stdout, &emitted.stderr)
        );
    }
    let c_prefix = emitted
        .stdout
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(emitted.stdout.len());
    if emitted.stdout.is_empty() || !emitted.stdout[c_prefix..].starts_with(b"/*") {
        let preview = String::from_utf8_lossy(&emitted.stdout[..emitted.stdout.len().min(120)]);
        let stderr = String::from_utf8_lossy(&emitted.stderr);
        bail!(
            "Carina B1 emit returned success but no generated C (stdout_bytes={}, stdout_prefix={:?}, stderr={:?})",
            emitted.stdout.len(),
            preview,
            stderr,
        );
    }
    std::fs::write(&c_file, &emitted.stdout)
        .with_context(|| format!("cannot write generated C {}", c_file.display()))?;
    let c_sha = sha256_file(&c_file)?;
    println!(
        "nova build seed: driver_stamp=carina-b1-build-v1 {} cwd={} argv={:?} seed=carina-b asset_sha256={} b1_sha256={} source_sha={} seed_freshness=verified platform=windows-x86_64 generated_c_sha256={}",
        DRIVER_STAMP,
        repo.display(),
        carina_args,
        SEED_ZIP_SHA256,
        SEED_EXE_SHA256,
        SEED_SOURCE,
        c_sha,
    );

    let pref = ToolchainPref::parse(toolchain)?;
    let tc = match driver::detect_toolchain(&ToolchainOpts {
        pref,
        explicit_clang: clang,
        explicit_vcvars: vcvars,
    }) {
        Ok(tc) => tc,
        Err(error) => {
            if !keep_artifacts {
                remove_file_if_present(&c_file)?;
            }
            return Err(error).context("no usable native C toolchain for Carina build");
        }
    };
    if !rt_dir.join("libuv/include/uv.h").is_file() {
        if !keep_artifacts {
            remove_file_if_present(&c_file)?;
        }
        bail!(
            "required Nova runtime input is missing: {}",
            rt_dir.join("libuv/include/uv.h").display()
        );
    }
    let libuv =
        match driver::native_runtime::detect_or_build_libuv(&rt_dir, &repo, tc.vcvars_path()) {
            Some(libuv) => libuv,
            None => {
                if !keep_artifacts {
                    remove_file_if_present(&c_file)?;
                }
                bail!("libuv toolchain input unavailable for Nova runtime");
            }
        };
    let own_ffi = driver::ResolvedFfiConfig::from_manifest(&manifest);
    let runtime = manifest.runtime.clone();
    if let Err(error) = std::fs::create_dir_all(&obj_dir) {
        if !keep_artifacts {
            remove_file_if_present(&c_file)?;
        }
        return Err(error).context("cannot create native object directory");
    }
    let opts = BuildOpts {
        c_file: &c_file,
        exe_file: &temp_exe,
        obj_dir: &obj_dir,
        cg_include: &cg_include,
        rt_dir: &rt_dir,
        mode,
        libuv: Some(&libuv),
        gc_kind: GcKind::default(),
        ffi: own_ffi.as_ref(),
        runtime: runtime.as_ref(),
    };
    let rt_archive =
        match driver::native_runtime::detect_or_build_rt_archive(&rt_dir, &repo, &tc, &opts) {
            Some(archive) => archive,
            None => {
                if !keep_artifacts {
                    remove_file_if_present(&c_file)?;
                    cleanup_native_intermediates(&temp_exe, &obj_dir)?;
                }
                bail!("could not build or locate Nova runtime archive");
            }
        };
    let runtime_sha = match sha256_file(&rt_archive.lib_file) {
        Ok(hash) => hash,
        Err(error) => {
            if !keep_artifacts {
                remove_file_if_present(&c_file)?;
                cleanup_native_intermediates(&temp_exe, &obj_dir)?;
            }
            return Err(error);
        }
    };
    println!(
        "nova build native inputs: toolchain={} mode={:?} runtime_archive={} runtime_sha256={}",
        tc.name(),
        mode,
        rt_archive.lib_file.display(),
        runtime_sha,
    );
    let compile_result =
        driver::native_build::compile_c_to_exe(&tc, &opts, Duration::from_secs(timeout_secs));
    if let Err(error) = compile_result {
        if !keep_artifacts {
            remove_file_if_present(&c_file)?;
            cleanup_native_intermediates(&temp_exe, &obj_dir)?;
        }
        return Err(error).context("Carina-generated C compile/link failed");
    }

    if let Some(parent) = final_exe.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create output directory {}", parent.display()))?;
    }
    if final_exe.exists() {
        remove_file_if_present(&final_exe)?;
    }
    if let Err(error) = std::fs::rename(&temp_exe, &final_exe) {
        // rename(2) cannot cross volumes (os error 17 on Windows): an output
        // path on another drive than the build dir must still publish.
        // Fall back to copy + remove; any failure of THAT is the real error.
        let published = std::fs::copy(&temp_exe, &final_exe)
            .and_then(|_| std::fs::remove_file(&temp_exe));
        if let Err(copy_error) = published {
            if !keep_artifacts {
                cleanup_native_intermediates(&temp_exe, &obj_dir)?;
                remove_file_if_present(&c_file)?;
            }
            return Err(copy_error)
                .with_context(|| {
                    format!(
                        "cannot publish native executable {} (rename failed too: {:#})",
                        final_exe.display(),
                        error
                    )
                });
        }
    }

    let binary_sha = sha256_file(&final_exe)?;
    println!(
        "nova build: {} binary_sha256={}",
        final_exe.display(),
        binary_sha
    );
    if !keep_artifacts {
        remove_file_if_present(&c_file)?;
        cleanup_native_intermediates(&temp_exe, &obj_dir)?;
    }
    Ok(())
}

/// Failure detail of a Carina B1 emit child assembled from BOTH of its
/// streams (registry №1922). B1 prints its §7 diagnostics as JSONL on
/// STDOUT, so stderr alone cannot name the refusal: a stdout-only child
/// would otherwise surface as a blank message, and a child writing to both
/// streams must not have its stdout diagnostics dropped. Each stream's text
/// appears exactly once.
fn emit_failure_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    let stdout = String::from_utf8_lossy(stdout);
    let stdout = stdout.trim();
    if stderr.is_empty() {
        stdout.to_owned()
    } else if stdout.is_empty() || stdout == stderr {
        stderr.to_owned()
    } else {
        format!("{stderr}\nseed stdout: {stdout}")
    }
}

/// Save the actual pinned seed child's raw streams before the success/failure
/// branch can discard either one. Capture is opt-in, creates a fresh directory,
/// and never overwrites prior evidence.
fn capture_seed_child_if_requested(
    capture_dir: Option<&Path>,
    executable: &Path,
    argv: &[String],
    cwd: &Path,
    self_root: &Path,
    output: &Output,
) -> Result<()> {
    let Some(capture_dir) = capture_dir else {
        return Ok(());
    };

    fs::create_dir(capture_dir).with_context(|| {
        format!(
            "cannot create fresh Carina child capture {}",
            capture_dir.display()
        )
    })?;

    write_new_capture_file(&capture_dir.join("stdout.bin"), &output.stdout)?;
    write_new_capture_file(&capture_dir.join("stderr.bin"), &output.stderr)?;
    let status = output.status.code().unwrap_or(1);
    write_new_capture_file(
        &capture_dir.join("status"),
        format!("{status}\n").as_bytes(),
    )?;
    let metadata = seed_child_metadata(executable, argv, cwd, self_root, status)?;
    write_new_capture_file(&capture_dir.join("launch.json"), metadata.as_bytes())?;
    Ok(())
}

fn write_new_capture_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("cannot create fresh Carina capture file {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("cannot write Carina capture file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("cannot flush Carina capture file {}", path.display()))?;
    Ok(())
}

fn seed_child_metadata(
    executable: &Path,
    argv: &[String],
    cwd: &Path,
    self_root: &Path,
    status: i32,
) -> Result<String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| anyhow!("Carina seed executable path is not valid UTF-8"))?;
    let cwd = cwd
        .to_str()
        .ok_or_else(|| anyhow!("Carina seed cwd is not valid UTF-8"))?;
    let mut json_argv = Vec::with_capacity(argv.len() + 1);
    json_argv.push(json_string(executable));
    for arg in argv {
        json_argv.push(json_string(arg));
    }

    // The launch wrapper clears unlisted NOVA_*/NOVAC_* variables. Record the
    // complete relevant allowlist, including values inherited by the driver
    // and the two paths explicitly assigned to the seed child below.
    let mut env = Vec::new();
    for name in [
        "NOVA_CARINA_SEED_ZIP",
        "NOVA_RT_DIR",
        "NOVA_CG_INCLUDE",
        "NOVA_CLANG",
        "NOVA_GC_LIB_DIR",
        "NOVA_GC_INCLUDE_DIR",
        "NOVA_TARGET_OS",
        "NOVA_VCVARS",
        "NOVA_FEATURES",
        "NOVAC_UNIT",
    ] {
        env.push((name, env_text(name)?));
    }
    env.push((
        "NOVA_CARINA_CAPTURE_DIR",
        Some("<removed from child>".to_owned()),
    ));
    env.push(("NOVA_STD_PATH", Some(cwd_path(cwd, "std/src")?)));
    let self_root_text = self_root
        .to_str()
        .ok_or_else(|| anyhow!("Carina child program root is not valid UTF-8"))?;
    env.push(("NOVAC_SELF_PATH", Some(self_root_text.to_owned())));

    let mut json_env = Vec::with_capacity(env.len());
    for (name, value) in env {
        let value = value.map_or_else(|| "null".to_owned(), |v| json_string(&v));
        json_env.push(format!("{}:{value}", json_string(name)));
    }
    Ok(format!(
        "{{\"executable\":{},\"argv\":[{}],\"cwd\":{},\"environment\":{{{}}},\"removed_from_child\":[\"NOVA_CARINA_CAPTURE_DIR\"],\"exit_status\":{status}}}\n",
        json_string(executable),
        json_argv.join(","),
        json_string(cwd),
        json_env.join(","),
    ))
}

fn cwd_path(cwd: &str, suffix: &str) -> Result<String> {
    Path::new(cwd)
        .join(suffix)
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Carina child environment path is not valid UTF-8"))
}

fn env_text(name: &str) -> Result<Option<String>> {
    std::env::var_os(name)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow!("Carina child environment value {name} is not valid UTF-8"))
        })
        .transpose()
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{001f}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn configure_seed_command<'a>(
    command: &'a mut Command,
    argv: &[String],
    repo: &Path,
    self_root: &Path,
) {
    command
        .args(argv)
        .current_dir(repo)
        // This control belongs to the driver, not the seed compiler. The
        // optional capture is evidence-only and must not alter the child env.
        .env_remove("NOVA_CARINA_CAPTURE_DIR")
        .env("NOVA_STD_PATH", repo.join("std/src"))
        // The program's own root: the staged tree of the program's reachable
        // files for a multi-module build (registry №1889), or the compiler's
        // own tree for the historical single-file invocation.
        .env("NOVAC_SELF_PATH", self_root);
}

struct Seed {
    exe: PathBuf,
}

fn prepare_seed(repo: &Path) -> Result<Seed> {
    let descriptor = repo.join("docs/dev/carina-seeds/windows-x86_64.json");
    let cache = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("LOCALAPPDATA is required for the pinned Carina seed cache"))?
        .join("Nova/carina-seeds/windows-x86_64");
    std::fs::create_dir_all(&cache)?;
    let asset = std::env::var_os("NOVA_CARINA_SEED_ZIP")
        .map(PathBuf::from)
        .unwrap_or_else(|| cache.join(SEED_ASSET));
    if !asset.is_file() {
        let url = std::env::var("NOVA_CARINA_SEED_URL").unwrap_or_else(|_| SEED_URL.to_owned());
        let command = format!(
            "$ProgressPreference='SilentlyContinue'; Invoke-WebRequest -Uri '{}' -OutFile '{}'",
            ps_quote(&url),
            ps_quote(&asset.to_string_lossy()),
        );
        let download = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &command])
            .output()
            .context("could not download the published pinned Carina seed asset")?;
        if !download.status.success() {
            bail!(
                "published Carina seed download failed: {}",
                String::from_utf8_lossy(&download.stderr).trim()
            );
        }
    }
    let verify = Command::new("python")
        .arg(repo.join("scripts/tools/carina-gate.py"))
        .args(["verify-seed", "--descriptor"])
        .arg(&descriptor)
        .arg("--asset")
        .arg(&asset)
        .args([
            "--platform",
            "windows-x86_64",
            "--expected-seed-source-revision",
            SEED_SOURCE,
        ])
        .current_dir(repo)
        .output()
        .context("could not run fail-closed Carina seed verifier")?;
    print_output(&verify);
    if !verify.status.success() {
        bail!("pinned Carina seed verification failed; no fallback compiler is allowed");
    }

    let unpacked = cache.join("unpacked");
    let exe = unpacked.join("nova-carina-0.2.1-windows-x86_64/novac.exe");
    if !exe.is_file() {
        let command = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            ps_quote(&asset.to_string_lossy()),
            ps_quote(&unpacked.to_string_lossy()),
        );
        let extract = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &command])
            .output()
            .context("could not unpack verified Carina seed")?;
        if !extract.status.success() {
            bail!(
                "verified Carina seed extraction failed: {}",
                String::from_utf8_lossy(&extract.stderr).trim()
            );
        }
    }
    if !exe.is_file() {
        bail!("verified seed archive does not contain novac.exe");
    }
    let source = std::fs::read_to_string(exe.with_file_name("SOURCE-SHA"))
        .context("verified seed archive is missing SOURCE-SHA")?;
    if source.trim() != SEED_SOURCE {
        bail!(
            "stale Carina B1 source: expected {}, got {}",
            SEED_SOURCE,
            source.trim()
        );
    }
    let exe_sha = sha256_file(&exe)?;
    if exe_sha != SEED_EXE_SHA256 {
        bail!("Carina B1 executable SHA256 mismatch: {}", exe_sha);
    }
    Ok(Seed { exe })
}

fn find_workspace_root(entry: &Path) -> Result<PathBuf> {
    // Installed paired packages expose their private driver root through
    // NOVA_RT_DIR. This keeps external project directories from needing to be
    // nested under the Nova source checkout and preserves the pinned seed inputs.
    if let Some(rt_dir) = env_path_override("NOVA_RT_DIR") {
        if let Some(root) = rt_dir.parent() {
            if has_carina_driver_assets(root) && rt_dir.is_dir() {
                return Ok(root.to_path_buf());
            }
        }
    }
    let mut dir = entry
        .parent()
        .ok_or_else(|| anyhow!("entry has no parent directory"))?;
    loop {
        if has_carina_driver_assets(dir) && dir.join("compiler-codegen/nova_rt").is_dir() {
            return Ok(dir.to_path_buf());
        }
        dir = dir.parent().ok_or_else(|| {
            anyhow!(
                "entry is not associated with a Nova workspace or installation: {}",
                entry.display()
            )
        })?;
    }
}

fn has_carina_driver_assets(root: &Path) -> bool {
    root.join("docs/dev/carina-seeds/windows-x86_64.json")
        .is_file()
        && root.join("scripts/tools/carina-gate.py").is_file()
        && root.join("novac/src").is_dir()
}

fn normalize_verbatim_path(path: &Path) -> PathBuf {
    let Some(value) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{}", rest))
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn ensure_directory(path: &Path, what: &str) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        bail!("{what} directory is missing: {}", path.display())
    }
}

fn remove_file_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            Err(e).with_context(|| format!("cannot remove stale build output {}", path.display()))
        }
    }
}

fn cleanup_native_intermediates(temp_exe: &Path, obj_dir: &Path) -> Result<()> {
    for extension in ["exe", "lib", "pdb", "ilk", "exp"] {
        remove_file_if_present(&temp_exe.with_extension(extension))?;
    }
    match std::fs::remove_dir_all(obj_dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            Err(e).with_context(|| format!("cannot remove native objects {}", obj_dir.display()))
        }
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let output = Command::new("python")
        .args([
            "-c",
            "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())",
        ])
        .arg(path)
        .output()
        .context("Python is required for SHA256 build provenance")?;
    if !output.status.success() {
        bail!("could not calculate SHA256 for {}", path.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn print_output(output: &Output) {
    if !output.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
}

fn ps_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::{
        capture_seed_child_if_requested, configure_seed_command, emit_failure_detail,
        find_workspace_root, seed_child_metadata,
    };
    use std::path::PathBuf;
    use std::process::{ExitStatus, Output};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_CAPTURE_DIR: AtomicUsize = AtomicUsize::new(0);

    fn test_capture_dir() -> PathBuf {
        let root = std::env::var_os("NOVA_TEST_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&root).unwrap();
        let id = NEXT_CAPTURE_DIR.fetch_add(1, Ordering::Relaxed);
        root.join(format!(
            "nova-driver-seed-capture-{}-{id}",
            std::process::id()
        ))
    }

    fn exit_status(code: i32) -> ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(code << 8)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(code as u32)
        }
    }

    fn output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: exit_status(code),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    fn remove_test_dir(path: &std::path::Path) {
        if path.exists() {
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn emit_failure_detail_shows_stdout_jsonl_when_stderr_is_blank() {
        // Registry №1922 carrier shape: B1 refuses with the §7 diagnostics on
        // STDOUT and an empty stderr.
        let jsonl = b"{\"id\":\"d0\",\"code\":\"E_NOVAC_SUBSET\"}\n";
        assert_eq!(
            emit_failure_detail(jsonl, b""),
            "{\"id\":\"d0\",\"code\":\"E_NOVAC_SUBSET\"}"
        );
    }

    #[test]
    fn emit_failure_detail_shows_stderr_when_stdout_is_blank() {
        assert_eq!(
            emit_failure_detail(b"", b"link driver missing\n"),
            "link driver missing"
        );
    }

    #[test]
    fn emit_failure_detail_keeps_both_streams_without_duplication() {
        let detail = emit_failure_detail(b"stdout-diagnostics", b"stderr-notes\n");
        assert!(detail.contains("stderr-notes"), "detail: {detail}");
        assert!(detail.contains("stdout-diagnostics"), "detail: {detail}");
        assert_eq!(detail.matches("stderr-notes").count(), 1);
        assert_eq!(detail.matches("stdout-diagnostics").count(), 1);
    }

    #[test]
    fn emit_failure_detail_does_not_repeat_identical_streams() {
        assert_eq!(
            emit_failure_detail(b"same text", b"same text\n"),
            "same text"
        );
    }

    #[test]
    fn emit_failure_detail_is_blank_when_both_streams_are_blank() {
        assert_eq!(emit_failure_detail(b"\n", b"  \n"), "");
    }

    #[test]
    fn resolves_source_checkout_root_for_an_external_entry_path() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let entry = root.join("spec_tests/conformance/standalone/stdout_hello.nv");
        assert_eq!(find_workspace_root(&entry).unwrap(), root);
    }

    #[test]
    fn captures_exact_raw_bytes_and_nonzero_child_status() {
        let dir = test_capture_dir();
        let executable = PathBuf::from(r#"C:\seed\carina"雪.exe"#);
        let args = vec!["emit".to_owned(), "arg\nwith-line-break.nv".to_owned()];
        let cwd = PathBuf::from(r#"D:\source\repo"#);
        let self_root = PathBuf::from(r#"D:\source\repo\target\program.src"#);
        let child_output = output(7, b"\0C\r\n", b"seed warning\n");

        capture_seed_child_if_requested(Some(&dir), &executable, &args, &cwd, &self_root, &child_output)
            .unwrap();

        assert_eq!(
            std::fs::read(dir.join("stdout.bin")).unwrap(),
            child_output.stdout
        );
        assert_eq!(
            std::fs::read(dir.join("stderr.bin")).unwrap(),
            child_output.stderr
        );
        assert_eq!(std::fs::read(dir.join("status")).unwrap(), b"7\n");
        let metadata = std::fs::read_to_string(dir.join("launch.json")).unwrap();
        assert!(metadata.contains(r#""executable":"C:\\seed\\carina\"雪.exe""#));
        assert!(metadata
            .contains(r#""argv":["C:\\seed\\carina\"雪.exe","emit","arg\nwith-line-break.nv"]"#));
        assert!(metadata.contains(r#""exit_status":7"#));
        remove_test_dir(&dir);
    }

    #[test]
    fn capture_refuses_to_overwrite_existing_evidence() {
        let dir = test_capture_dir();
        let executable = PathBuf::from("seed.exe");
        let args = vec!["emit".to_owned(), "main.nv".to_owned()];
        let cwd = PathBuf::from("repo");
        let self_root = PathBuf::from("repo/novac/src");
        let first = output(0, b"first stdout", b"first stderr");
        capture_seed_child_if_requested(Some(&dir), &executable, &args, &cwd, &self_root, &first)
            .unwrap();

        let second = output(9, b"second stdout", b"second stderr");
        assert!(
            capture_seed_child_if_requested(Some(&dir), &executable, &args, &cwd, &self_root, &second,)
                .is_err()
        );
        assert_eq!(
            std::fs::read(dir.join("stdout.bin")).unwrap(),
            b"first stdout"
        );
        assert_eq!(
            std::fs::read(dir.join("stderr.bin")).unwrap(),
            b"first stderr"
        );
        assert_eq!(std::fs::read(dir.join("status")).unwrap(), b"0\n");
        remove_test_dir(&dir);
    }

    #[test]
    fn absent_capture_variable_keeps_default_path_side_effect_free() {
        let dir = test_capture_dir();
        let result = capture_seed_child_if_requested(
            None,
            std::path::Path::new("seed.exe"),
            &["emit".to_owned(), "main.nv".to_owned()],
            std::path::Path::new("repo"),
            std::path::Path::new("repo/novac/src"),
            &output(0, b"C", b""),
        );
        assert!(result.is_ok());
        assert!(!dir.exists());
    }

    #[cfg(windows)]
    #[test]
    fn capture_control_is_removed_from_the_seed_child_environment() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let mut command = std::process::Command::new("cmd.exe");
        command.env("NOVA_CARINA_CAPTURE_DIR", "driver-only-control");
        let args = vec![
            "/d".to_owned(),
            "/c".to_owned(),
            "if defined NOVA_CARINA_CAPTURE_DIR (exit /b 9) else (exit /b 0)".to_owned(),
        ];
        configure_seed_command(&mut command, &args, &repo, &repo.join("novac/src"));
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "capture control leaked into seed child: stdout={:?}, stderr={:?}",
            output.stdout,
            output.stderr
        );
    }

    #[test]
    fn child_metadata_escapes_paths_and_arguments_as_json_strings() {
        let metadata = seed_child_metadata(
            std::path::Path::new(r#"C:\seed\carina"雪.exe"#),
            &["emit".to_owned(), "line\nnext.nv".to_owned()],
            std::path::Path::new(r#"D:\source\repo"#),
            std::path::Path::new(r#"D:\source\repo\target\program.src"#),
            1,
        )
        .unwrap();
        assert!(metadata.contains(r#""argv":["C:\\seed\\carina\"雪.exe","emit","line\nnext.nv"]"#));
        assert!(metadata.contains(r#""line\nnext.nv""#));
        assert!(metadata.contains(r#""NOVA_CARINA_CAPTURE_DIR":"<removed from child>""#));
        assert!(metadata.contains(r#""exit_status":1"#));
    }
}

fn env_path_override(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}
