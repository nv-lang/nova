//! Compiler-free build-driver support extracted from the Oracle test runner.
pub mod fs_lock;
pub mod git_cache;
pub mod link_prep;
pub mod lockfile;
pub mod manifest;
pub mod native_build;
pub mod native_runtime;
pub mod pkg_proxy;
pub mod process;
pub mod resolver;
pub mod semver;
use anyhow::{anyhow, Result};
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find the nearest package root for an existing source file.
///
/// This driver-facing helper deliberately depends only on filesystem
/// operations; compiler source-index caching is not part of driver behavior.
pub fn find_package_dir(file: &Path) -> Option<PathBuf> {
    let absolute = std::fs::canonicalize(file).ok()?;
    let mut dir = absolute.parent()?.to_path_buf();
    loop {
        if dir.join("nova.toml").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Plan 149 D233: `[runtime]` section config — fiber arena tuning.
///
/// `fiber_stack` — per-fiber stack slot size (human-friendly `"4MB"` или bare
/// bytes `"4194304"`). `fibers_per_worker` — max concurrent fibers per worker
/// (`"16384"`). Both baked as compile-time `-D...DEFAULT` flags; the
/// corresponding env var (NOVA_FIBER_STACK / NOVA_FIBERS_PER_WORKER) overrides at
/// runtime. Stored as raw strings; `parse_size_to_bytes` converts to the raw
/// integer the C `#define` consumes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// "4MB" | "4194304" — per-fiber stack slot size.
    pub fiber_stack: Option<String>,
    /// "16384" — max concurrent fibers per worker.
    pub fibers_per_worker: Option<String>,
}

/// Parse the runtime `[runtime]` size representation used by native build flags.
pub fn parse_size_to_bytes(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let digit_end = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    if digit_end == 0 {
        return None;
    }
    let number: u64 = value[..digit_end].parse().ok()?;
    let multiplier: u64 = match value[digit_end..].trim().to_ascii_uppercase().as_str() {
        "" => 1,
        "K" | "KB" => 1024,
        "M" | "MB" => 1024 * 1024,
        "G" | "GB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    if number == 0 {
        return None;
    }
    number.checked_mul(multiplier)
}

/// Generic view of the FFI fields a native build driver consumes.
pub trait ManifestFfiView {
    fn manifest_dir(&self) -> &Path;
    fn ffi_config(&self) -> Option<&FfiConfig>;
}

/// FFI paths resolved against the manifest directory.
#[derive(Debug, Clone, Default)]
pub struct ResolvedFfiConfig {
    pub c_shims: Vec<PathBuf>,
    pub include_dirs: Vec<PathBuf>,
    pub lib_dirs: Vec<PathBuf>,
    pub libs: Vec<String>,
    pub vendor_src_dirs: Vec<PathBuf>,
    pub cxx: bool,
}

pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    match path.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => path.to_path_buf(),
    }
}

impl ResolvedFfiConfig {
    pub fn from_manifest<M: ManifestFfiView>(manifest: &M) -> Option<Self> {
        let cfg = manifest.ffi_config()?;
        let base = manifest.manifest_dir();
        Some(Self {
            c_shims: cfg
                .c_shims
                .iter()
                .map(|p| strip_verbatim_prefix(&base.join(p)))
                .collect(),
            include_dirs: cfg
                .include_dirs
                .iter()
                .map(|p| strip_verbatim_prefix(&base.join(p)))
                .collect(),
            lib_dirs: cfg
                .lib_dirs
                .iter()
                .map(|p| strip_verbatim_prefix(&base.join(p)))
                .collect(),
            libs: cfg.libs.clone(),
            vendor_src_dirs: cfg
                .vendor_src_dirs
                .iter()
                .map(|p| strip_verbatim_prefix(&base.join(p)))
                .collect(),
            cxx: cfg.cxx,
        })
    }

    pub fn merge(&mut self, other: Self) {
        self.c_shims.extend(other.c_shims);
        self.include_dirs.extend(other.include_dirs);
        for dir in other.lib_dirs {
            if !self.lib_dirs.contains(&dir) {
                self.lib_dirs.push(dir);
            }
        }
        for lib in other.libs {
            if !self.libs.contains(&lib) {
                self.libs.push(lib);
            }
        }
        for dir in other.vendor_src_dirs {
            if !self.vendor_src_dirs.contains(&dir) {
                self.vendor_src_dirs.push(dir);
            }
        }
    }
}

#[derive(Clone)]
pub struct LibuvConfig {
    pub include_dir: PathBuf,
    pub lib_file: PathBuf,
    pub eventloop_src: PathBuf,
}

#[derive(Clone)]
pub struct BoehmConfig {
    pub include_dir: Option<PathBuf>,
    pub lib_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GcKind {
    Malloc,
    #[default]
    Boehm,
}

impl GcKind {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "malloc" => Ok(Self::Malloc),
            "boehm" => Ok(Self::Boehm),
            _ => Err(anyhow!(
                "unknown gc backend `{}` (expected malloc|boehm)",
                s
            )),
        }
    }

    pub fn alloc_c_name(self) -> &'static str {
        match self {
            Self::Malloc => "alloc.c",
            Self::Boehm => "alloc_boehm.c",
        }
    }

    pub fn tag(self) -> GcKindTag {
        match self {
            Self::Malloc => GcKindTag::Malloc,
            Self::Boehm => GcKindTag::Boehm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcKindTag {
    Malloc,
    Boehm,
}

impl GcKindTag {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "malloc" => Some(Self::Malloc),
            "boehm" => Some(Self::Boehm),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Malloc => "malloc",
            Self::Boehm => "boehm",
        }
    }
}

pub struct BuildOpts<'a> {
    pub c_file: &'a Path,
    pub exe_file: &'a Path,
    pub obj_dir: &'a Path,
    pub cg_include: &'a Path,
    pub rt_dir: &'a Path,
    pub mode: Mode,
    pub libuv: Option<&'a LibuvConfig>,
    pub gc_kind: GcKind,
    pub ffi: Option<&'a ResolvedFfiConfig>,
    pub runtime: Option<&'a RuntimeConfig>,
}

/// Plan 115 D214 [M-115-ffi-build-pipeline]: `[ffi]` section config.
///
/// Все пути относительные к директории `nova.toml`. Test_runner +
/// build pipeline резолвят их в absolute paths перед передачей clang.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FfiConfig {
    /// Список C / header файлов для compilation. Header-only inline shim'ы
    /// (как `nova_rt/sqlite_mini_ffi.h`) включаются через `#include`,
    /// .c файлы compilation units компилируются и линкуются.
    pub c_shims: Vec<String>,
    /// Include directories для clang `-I`. Дают доступ к user shim header'ам
    /// и third-party C library headers.
    pub include_dirs: Vec<String>,
    /// Plan 193 Ф.2 gap-1: library search directories для linker `-L`
    /// (Clang/GCC) / `/LIBPATH:` (MSVC) — non-default-path native libs
    /// (напр. vcpkg install без system-wide регистрации). Пусто → только
    /// toolchain default search path (Windows: нет системного аналога
    /// `/usr/lib` — без `lib_dirs` non-default `.lib` не найдётся).
    pub lib_dirs: Vec<String>,
    /// System library names для clang `-l<name>` linking. Например
    /// `libs = ["sqlite3", "png"]` → `-lsqlite3 -lpng`.
    pub libs: Vec<String>,
    /// Plan 193 Ф.2 gate-3 (mbedtls-vendored, 2026-07-12): vendored C source
    /// directories to build-and-cache when a declared `libs` entry is
    /// missing from `lib_dirs` — generic "195-pattern" extension of the
    /// monorepo's libuv one-time-build-and-cache precedent
    /// (`detect_or_build_libuv` in `test_runner.rs`), so ANY native module
    /// can vendor upstream C sources instead of requiring a prebuilt
    /// system/vcpkg lib. All `.c` files directly under each declared dir
    /// (non-recursive — matches typical upstream `library/`-style flat
    /// layouts, e.g. mbedTLS) are compiled + archived into `lib_dirs[0]`
    /// under EVERY name declared in `libs` (see
    /// `test_runner::build_missing_vendor_ffi_libs`). Empty (default) — no
    /// vendor build attempted, unchanged legacy behaviour (falls through to
    /// the existing `first_missing_ffi_lib` detect-and-degrade probe).
    pub vendor_src_dirs: Vec<String>,
    /// D466 (2026-09-09): the package links a C++ library, so the LINK step must
    /// run through a C++ driver (`clang++`/`g++`) instead of the C one. A static
    /// C++ archive does not carry its standard library: MSVC picks it up from
    /// directives inside the objects, ELF toolchains have no such thing and rely
    /// on the driver to add it. Declaring the LANGUAGE of the library is the true
    /// fact; `libs.linux = ["stdc++"]` would make every package author re-derive
    /// per platform what the toolchain already knows, and on Windows such an entry
    /// degrades the package into a SILENT FfiLibNotFound skip (measured on
    /// nova-duckdb, 2026-09-09). Default false — unchanged behaviour.
    pub cxx: bool,
}

// ---------- toolchain detection ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dev,
    Release,
}

impl Mode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "dev" => Ok(Mode::Dev),
            "release" => Ok(Mode::Release),
            _ => Err(anyhow!("unknown mode `{}` (expected dev|release)", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolchainPref {
    Auto,
    Clang,
    Msvc,
    Gcc,
}

#[cfg(test)]
mod tests {
    use super::{
        bytes_to_string, find_package_dir, parse_size_to_bytes, FfiConfig, ManifestFfiView, Mode,
        ResolvedFfiConfig, ToolchainPref,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn mode_and_toolchain_parse_contract() {
        assert_eq!(Mode::parse("release").unwrap(), Mode::Release);
        assert_eq!(ToolchainPref::parse("clang").unwrap(), ToolchainPref::Clang);
        assert!(Mode::parse("unknown").is_err());
        assert!(ToolchainPref::parse("unknown").is_err());
    }

    #[test]
    fn child_output_decodes_utf8() {
        assert_eq!(
            bytes_to_string(b"native build output"),
            "native build output"
        );
    }

    #[test]
    fn cp1251_signature_bytes_keep_their_original_codepoints() {
        for (byte, expected) in [(0x80, '\u{0402}'), (0x81, '\u{0403}'),
            (0x83, '\u{0453}'), (0x90, '\u{0452}'), (0x9a, '\u{0459}'),
            (0x9c, '\u{045a}'), (0x9d, '\u{045c}'), (0x9e, '\u{045b}'),
            (0xbe, '\u{0455}'), (0xbf, '\u{0457}')]
        {
            assert_eq!(super::bytes_to_string(&[byte]), expected.to_string());
        }
    }

    #[test]
    fn package_lookup_finds_crate_root() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            find_package_dir(&root.parent().unwrap().join("compiler-codegen/src/lib.rs")),
            Some(root.parent().unwrap().canonicalize().unwrap())
        );
    }

    #[test]
    fn runtime_size_parse_contract() {
        assert_eq!(parse_size_to_bytes("4MB"), Some(4 * 1024 * 1024));
        assert_eq!(parse_size_to_bytes(" 8192 "), Some(8192));
        assert_eq!(parse_size_to_bytes("0"), None);
        assert_eq!(parse_size_to_bytes("4XB"), None);
    }

    #[test]
    fn ffi_manifest_paths_resolve_from_manifest_directory() {
        struct FixtureManifest {
            dir: std::path::PathBuf,
            ffi: Option<FfiConfig>,
        }
        impl ManifestFfiView for FixtureManifest {
            fn manifest_dir(&self) -> &Path {
                &self.dir
            }
            fn ffi_config(&self) -> Option<&FfiConfig> {
                self.ffi.as_ref()
            }
        }

        let manifest = FixtureManifest {
            dir: std::path::PathBuf::from("package"),
            ffi: Some(FfiConfig {
                c_shims: vec!["native/shim.c".into()],
                include_dirs: vec!["native/include".into()],
                ..FfiConfig::default()
            }),
        };
        let resolved = ResolvedFfiConfig::from_manifest(&manifest).unwrap();
        assert_eq!(resolved.c_shims, [PathBuf::from("package/native/shim.c")]);
        assert_eq!(
            resolved.include_dirs,
            [PathBuf::from("package/native/include")]
        );
    }
}

impl ToolchainPref {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "auto" => Ok(ToolchainPref::Auto),
            "clang" => Ok(ToolchainPref::Clang),
            "msvc" => Ok(ToolchainPref::Msvc),
            "gcc" => Ok(ToolchainPref::Gcc),
            _ => Err(anyhow!(
                "unknown toolchain `{}` (expected auto|clang|msvc|gcc)",
                s
            )),
        }
    }
}

/// Конкретный детектированный toolchain. На Windows vcvars env захвачен
/// один раз при detect_toolchain — передаётся напрямую в Command::envs(),
/// избегая повторного вызова vcvars64.bat (~7 sec) на каждом тесте.
#[derive(Debug, Clone)]
pub enum Toolchain {
    /// `env`: vcvars64 env snapshot (Windows), empty on Linux/macOS.
    /// `vcvars`: path retained for detect_or_build_libuv (one-time build).
    Clang {
        clang: PathBuf,
        env: Vec<(OsString, OsString)>,
        vcvars: Option<PathBuf>,
    },
    /// `env`: vcvars64 env snapshot.
    /// `vcvars`: path retained for detect_or_build_libuv (one-time build).
    Msvc {
        env: Vec<(OsString, OsString)>,
        vcvars: Option<PathBuf>,
    },
    Gcc {
        gcc: PathBuf,
    },
}

impl Toolchain {
    pub fn name(&self) -> &'static str {
        match self {
            Toolchain::Clang { .. } => "clang",
            Toolchain::Msvc { .. } => "msvc",
            Toolchain::Gcc { .. } => "gcc",
        }
    }

    /// Path to vcvars64.bat, if any. Used only by detect_or_build_libuv
    /// (one-time build) — not used for per-test compilation.
    pub fn vcvars_path(&self) -> Option<&Path> {
        match self {
            Toolchain::Clang { vcvars, .. } => vcvars.as_deref(),
            Toolchain::Msvc { vcvars, .. } => vcvars.as_deref(),
            Toolchain::Gcc { .. } => None,
        }
    }
}

/// Поиск исполняемого в `PATH` — кросс-платформенный аналог `which` / `Get-Command`.
pub fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exe_name = if cfg!(target_os = "windows") && !name.ends_with(".exe") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(&exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn find_clang_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    if let Some(env_path) = std::env::var_os("NOVA_CLANG") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    if cfg!(target_os = "windows") {
        let candidates = [
            PathBuf::from(r"C:\Program Files\LLVM\bin\clang.exe"),
            PathBuf::from(r"C:\Program Files (x86)\LLVM\bin\clang.exe"),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
    } else {
        let candidates = [
            PathBuf::from("/usr/bin/clang"),
            PathBuf::from("/usr/local/bin/clang"),
            PathBuf::from("/opt/homebrew/bin/clang"),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
    }
    which("clang")
}

fn find_gcc_path() -> Option<PathBuf> {
    if let Some(env_path) = std::env::var_os("NOVA_GCC") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    if !cfg!(target_os = "windows") {
        let candidates = [
            PathBuf::from("/usr/bin/gcc"),
            PathBuf::from("/usr/local/bin/gcc"),
        ];
        for c in &candidates {
            if c.is_file() {
                return Some(c.clone());
            }
        }
    }
    which("gcc")
}

/// Найти vcvars64.bat. На Windows — через `vswhere.exe`. На Linux/macOS — None.
fn find_vcvars(explicit: Option<&Path>) -> Option<PathBuf> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    if let Some(env_path) = std::env::var_os("NOVA_VCVARS") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    let pf86 = std::env::var("ProgramFiles(x86)").ok()?;
    let vswhere = PathBuf::from(&pf86)
        .join("Microsoft Visual Studio")
        .join("Installer")
        .join("vswhere.exe");
    if !vswhere.is_file() {
        return None;
    }
    let output = Command::new(&vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-find",
            r"VC\Auxiliary\Build\vcvars64.bat",
        ])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let p = PathBuf::from(line.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Capture the environment produced by vcvars64.bat once.
/// Returns key-value pairs suitable for `Command::envs()`.
/// Calling vcvars once at startup and passing its env directly to clang/cl
/// avoids the ~7-second `call vcvars64.bat` overhead on every compile.
#[cfg(target_os = "windows")]
fn capture_vcvars_env(vcvars: &Path) -> Result<Vec<(OsString, OsString)>> {
    let inner = format!("\"call \"{}\" > nul && set\"", vcvars.display());
    let mut cmd = Command::new("cmd");
    cmd.raw_arg("/c").raw_arg(&inner);
    let out = cmd.output().map_err(|e| anyhow!("spawn cmd: {:#}", e))?;
    if !out.status.success() {
        return Err(anyhow!(
            "vcvars64.bat failed (exit {:?})",
            out.status.code()
        ));
    }
    let stdout = bytes_to_string(&out.stdout);
    let mut vars: Vec<(OsString, OsString)> = Vec::new();
    for line in stdout.lines() {
        if let Some(eq) = line.find('=') {
            let key = &line[..eq];
            let val = &line[eq + 1..];
            vars.push((OsString::from(key), OsString::from(val)));
        }
    }
    Ok(vars)
}

#[cfg(not(target_os = "windows"))]
fn capture_vcvars_env(_vcvars: &Path) -> Result<Vec<(OsString, OsString)>> {
    Ok(vec![])
}

pub struct ToolchainOpts<'a> {
    pub pref: ToolchainPref,
    pub explicit_clang: Option<&'a Path>,
    pub explicit_vcvars: Option<&'a Path>,
}

pub fn detect_toolchain(opts: &ToolchainOpts) -> Result<Toolchain> {
    let clang = find_clang_path(opts.explicit_clang);
    let vcvars = find_vcvars(opts.explicit_vcvars);
    let gcc = find_gcc_path();

    // Capture vcvars env once. On non-Windows this is a no-op.
    // The ~7s call vcvars64.bat cost is paid here once, not per-test.
    let vcvars_env: Option<Vec<(OsString, OsString)>> = if let Some(ref v) = vcvars {
        let env =
            capture_vcvars_env(v).map_err(|e| anyhow!("vcvars64.bat capture failed: {:#}", e))?;
        Some(env)
    } else {
        None
    };

    let try_clang = || -> Result<Toolchain> {
        let clang = clang.clone().ok_or_else(|| {
            anyhow!(
                "clang not found. Install LLVM:\n  \
                 - Windows: `winget install LLVM.LLVM`\n  \
                 - Linux: `apt install clang` or `dnf install clang`\n  \
                 - macOS: ships with Xcode CLI tools\n  \
                 Or set NOVA_CLANG to clang.exe path."
            )
        })?;
        if cfg!(target_os = "windows") && vcvars_env.is_none() {
            return Err(anyhow!(
                "clang on Windows requires vcvars64.bat for MSVC SDK headers/libs. \
                 Install Visual Studio Build Tools, or set NOVA_VCVARS."
            ));
        }
        Ok(Toolchain::Clang {
            clang,
            env: vcvars_env.clone().unwrap_or_default(),
            vcvars: vcvars.clone(),
        })
    };
    let try_msvc = || -> Result<Toolchain> {
        if !cfg!(target_os = "windows") {
            return Err(anyhow!("MSVC toolchain unavailable on non-Windows OS"));
        }
        let env = vcvars_env.clone().ok_or_else(|| {
            anyhow!(
                "vcvars64.bat not found. Install Visual Studio Build Tools, \
                 or set NOVA_VCVARS to vcvars64.bat path."
            )
        })?;
        Ok(Toolchain::Msvc {
            env,
            vcvars: vcvars.clone(),
        })
    };
    let try_gcc = || -> Result<Toolchain> {
        let gcc = gcc
            .clone()
            .ok_or_else(|| anyhow!("gcc not found in PATH. Install GCC."))?;
        Ok(Toolchain::Gcc { gcc })
    };

    match opts.pref {
        ToolchainPref::Clang => try_clang(),
        ToolchainPref::Msvc => try_msvc(),
        ToolchainPref::Gcc => try_gcc(),
        ToolchainPref::Auto => {
            // Windows: Clang > MSVC > GCC. Linux/macOS: Clang > GCC.
            if cfg!(target_os = "windows") {
                try_clang().or_else(|_| try_msvc()).or_else(|_| try_gcc())
            } else {
                try_clang().or_else(|_| try_gcc())
            }
        }
    }
}
pub fn bytes_to_string(b: &[u8]) -> String {
    // (1) Strict UTF-8.
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    // (2) Windows CP1251 fallback.
    #[cfg(target_os = "windows")]
    {
        // Простой CP1251 → Unicode mapping (только printable + кириллица).
        // CP1251 char 0x80-0xFF → Unicode code points.
        let mut out = String::with_capacity(b.len());
        for &c in b {
            if c < 0x80 {
                out.push(c as char);
            } else {
                // CP1251 → Unicode mapping table.
                out.push(cp1251_to_char(c));
            }
        }
        return out;
    }
    // (3) Lossy fallback.
    #[allow(unreachable_code)]
    String::from_utf8_lossy(b).into_owned()
}

#[cfg(target_os = "windows")]
fn cp1251_to_char(c: u8) -> char {
    // Полный mapping CP1251 (0x80-0xFF).
    match c {
        0x80 => '\u{0402}',
        0x81 => '\u{0403}',
        0x82 => '‚',
        0x83 => '\u{0453}',
        0x84 => '„',
        0x85 => '…',
        0x86 => '†',
        0x87 => '‡',
        0x88 => '€',
        0x89 => '‰',
        0x8A => 'Љ',
        0x8B => '‹',
        0x8C => 'Њ',
        0x8D => 'Ќ',
        0x8E => 'Ћ',
        0x8F => 'Џ',
        0x90 => '\u{0452}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x99 => '™',
        0x9A => '\u{0459}',
        0x9B => '›',
        0x9C => '\u{045a}',
        0x9D => '\u{045c}',
        0x9E => '\u{045b}',
        0x9F => 'џ',
        0xA0 => '\u{A0}',
        0xA1 => 'Ў',
        0xA2 => 'ў',
        0xA3 => 'Ј',
        0xA4 => '¤',
        0xA5 => 'Ґ',
        0xA6 => '¦',
        0xA7 => '§',
        0xA8 => 'Ё',
        0xA9 => '©',
        0xAA => 'Є',
        0xAB => '«',
        0xAC => '¬',
        0xAD => '\u{AD}',
        0xAE => '®',
        0xAF => 'Ї',
        0xB0 => '°',
        0xB1 => '±',
        0xB2 => 'І',
        0xB3 => 'і',
        0xB4 => 'ґ',
        0xB5 => 'µ',
        0xB6 => '¶',
        0xB7 => '·',
        0xB8 => 'ё',
        0xB9 => '№',
        0xBA => 'є',
        0xBB => '»',
        0xBC => 'ј',
        0xBD => 'Ѕ',
        0xBE => '\u{0455}',
        0xBF => '\u{0457}',
        0xC0..=0xDF => {
            // А-Я (0xC0='А', 0xDF='Я')
            char::from_u32(0x0410 + (c - 0xC0) as u32).unwrap_or('?')
        }
        0xE0..=0xFF => {
            // а-я (0xE0='а', 0xFF='я')
            char::from_u32(0x0430 + (c - 0xE0) as u32).unwrap_or('?')
        }
        _ => '?',
    }
}
