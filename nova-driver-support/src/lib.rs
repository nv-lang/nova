//! Compiler-free build-driver support extracted from the Oracle test runner.
use anyhow::{anyhow, Result};
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    use super::{bytes_to_string, Mode, ToolchainPref};

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
fn which(name: &str) -> Option<PathBuf> {
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
        0x80 => 'Ђ',
        0x81 => 'Ѓ',
        0x82 => '‚',
        0x83 => 'ѓ',
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
        0x90 => 'ђ',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '•',
        0x96 => '–',
        0x97 => '—',
        0x99 => '™',
        0x9A => 'љ',
        0x9B => '›',
        0x9C => 'њ',
        0x9D => 'ќ',
        0x9E => 'ћ',
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
        0xBE => 'ѕ',
        0xBF => 'ї',
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
