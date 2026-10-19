//! Carina-backed `nova build` driver path (274.16 Ш.4).
//!
//! This module owns workspace/seed/toolchain orchestration only. The Nova
//! compiler invocation is always the pinned external Carina B1 `emit`; this
//! path never asks the Rust Oracle to parse, check, or emit Nova source.
//!
//! Transitional debt: manifest/lock/toolchain/runtime helpers are currently
//! imported from `nova_codegen`, so the monolithic user binary still physically
//! links that crate. Before the final Ш.4 release door, extract driver-only
//! helpers into an Oracle-free driver crate or prove equivalent compile-time
//! separation; the Carina `build` path itself must remain Oracle-free.

use anyhow::{anyhow, bail, Context, Result};
use nova_codegen::test_runner::{self, BuildOpts, GcKind, Mode, ToolchainOpts, ToolchainPref};
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
    let rt_dir = super::env_path_override("NOVA_RT_DIR")
        .unwrap_or_else(|| repo.join("compiler-codegen/nova_rt"));
    let cg_include = super::env_path_override("NOVA_CG_INCLUDE")
        .unwrap_or_else(|| repo.join("compiler-codegen"));
    ensure_directory(&rt_dir, "Nova runtime")?;
    ensure_directory(&cg_include, "Nova C headers")?;

    let package_dir = nova_codegen::manifest::find_package_dir(&entry)
        .ok_or_else(|| anyhow!("missing nova.toml for build entry {}", entry.display()))?;
    let manifest_path = package_dir.join("nova.toml");
    let manifest = nova_codegen::manifest::parse_manifest(&manifest_path, &package_dir)
        .ok_or_else(|| anyhow!("invalid package manifest: {}", manifest_path.display()))?;
    // This is driver-owned package/lock resolution, not compiler work.
    nova_codegen::lockfile::sync(&package_dir)
        .map_err(|e| anyhow!("dependency resolution (nova.lock.toml): {:#}", e))?;

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

    let carina_args = vec!["emit".to_owned(), entry.to_string_lossy().into_owned()];
    let mut carina = Command::new(&seed.exe);
    carina
        .args(&carina_args)
        .current_dir(&repo)
        .env("NOVA_STD_PATH", repo.join("std/src"))
        .env("NOVAC_SELF_PATH", repo.join("novac/src"));
    let emitted = carina
        .output()
        .context("could not start pinned Carina B1 emit")?;
    if !emitted.status.success() {
        let detail = String::from_utf8_lossy(&emitted.stderr);
        bail!(
            "Carina B1 emit failed (exit {}): {}",
            emitted.status.code().unwrap_or(1),
            detail.trim()
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
    let tc = match test_runner::detect_toolchain(&ToolchainOpts {
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
    let libuv = match test_runner::detect_or_build_libuv(&rt_dir, &repo, tc.vcvars_path()) {
        Some(libuv) => libuv,
        None => {
            if !keep_artifacts {
                remove_file_if_present(&c_file)?;
            }
            bail!("libuv toolchain input unavailable for Nova runtime");
        }
    };
    let own_ffi = test_runner::ResolvedFfiConfig::from_manifest(&manifest);
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
    let rt_archive = match test_runner::detect_or_build_rt_archive(&rt_dir, &repo, &tc, &opts) {
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
        test_runner::compile_c_to_exe(&tc, &opts, Duration::from_secs(timeout_secs));
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
        if !keep_artifacts {
            cleanup_native_intermediates(&temp_exe, &obj_dir)?;
            remove_file_if_present(&c_file)?;
        }
        return Err(error)
            .with_context(|| format!("cannot publish native executable {}", final_exe.display()));
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
    let mut dir = entry
        .parent()
        .ok_or_else(|| anyhow!("entry has no parent directory"))?;
    loop {
        if dir
            .join("docs/dev/carina-seeds/windows-x86_64.json")
            .is_file()
            && dir.join("compiler-codegen/nova_rt").is_dir()
        {
            return Ok(dir.to_path_buf());
        }
        dir = dir
            .parent()
            .ok_or_else(|| anyhow!("entry is not inside a Nova workspace: {}", entry.display()))?;
    }
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
