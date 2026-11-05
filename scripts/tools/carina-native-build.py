#!/usr/bin/env python3
"""Oracle-free native build of Carina (novac) from the pinned B1 seed.

Task 140: the CI carina-gate workflow builds the Carina compiler on a fresh
windows-latest runner WITHOUT the Rust Oracle and WITHOUT cargo:

    seed binary  --emit-->  novac.c  --clang/link-->  stage-B1 binary

The compile/link recipe is a line-by-line port of the proven reference
implementation in the Rust driver (do not re-derive flags by taste):

    nova-driver/src/carina_build.rs              (seed pins, emit orchestration)
    nova-driver-support/src/native_build.rs      (build_command, Clang branch)
    nova-driver-support/src/native_runtime.rs    (libuv/bdwgc one-time builds)
    nova-driver-support/src/lib.rs               (toolchain/vcvars discovery)

The pre-218 loose-sources path is used deliberately: every nova_rt/*.c
translation unit is passed to ONE clang invocation (the rt-archive is a
latency optimization, not a requirement). This keeps the CI path auditable
and free of the archive bucket-key machinery.

Seed pins below MUST match nova-driver/src/carina_build.rs
(SEED_SOURCE / SEED_EXE_SHA256); check-carina-gate-ci.py verifies it.

Windows-only: the oracle-free seed is published for windows-x86_64 only
(docs/dev/carina-seeds/windows-x86_64.json, stage carina-b).

Subcommands (each is one CI step, fail-closed):

    fetch-seed  download + verify + unpack the pinned seed
    emit        run the seed: novac/src/main.nv -> C on stdout
    libs        one-time libuv.lib + gc.lib builds (idempotent cache)
    link        clang-link the emitted C into the stage-B1 executable
    smoke       emit+link+run a hello-world with the freshly built compiler
"""
from __future__ import annotations

import argparse
import hashlib
import os
import pathlib
import shutil
import subprocess
import sys
import threading
import time
import urllib.request
import zipfile

# --- Seed pins: mirror nova-driver/src/carina_build.rs exactly. ---
SEED_SOURCE = "3729abcce42ec2d94efc898ec6eafe91ff6210d5"
SEED_EXE_SHA256 = "824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4"
SEED_DESCRIPTOR = "docs/dev/carina-seeds/windows-x86_64.json"

# nova_rt translation units of the pre-218 loose-sources path
# (native_build.rs build_command, use_rt_archive == False branch; the
# GC-backend alloc file is alloc_boehm.c for GcKind::Boehm).
RT_SOURCES = [
    "alloc_boehm.c",
    "effects.c",
    "fibers.c",
    "fiber_arena.c",
    "fiber_arena_win.c",
    "fiber_stats.c",
    "runtime.c",
    "driver.c",
    "typeid.c",
    "segv_diag.c",
]
# libuv-gated runtime units + the event loop (native_runtime.rs
# rt_archive_sources, libuv branch), placed BEFORE the generated C file on
# the command line exactly as build_command does for Windows.
RT_LIBUV_SOURCES = ["net.c", "fs.c", "process.c"]

# native_runtime.rs LIBUV_WIN_SYSLIBS, linked as -l<name without .lib>.
LIBUV_WIN_SYSLIBS = [
    "ws2_32",
    "iphlpapi",
    "psapi",
    "userenv",
    "user32",
    "shell32",
    "ole32",
    "uuid",
    "advapi32",
    "dbghelp",
]

# native_runtime.rs build_boehm_lib: mirrors the repo's vcpkg bdwgc
# x64-windows-static DEFINES line byte-for-byte.
BDWGC_DEFINES = [
    "/DDONT_USE_USER32_DLL",
    "/DEMPTY_GETENV_RESULTS",
    "/DENABLE_DISCLAIM",
    "/DGC_ATOMIC_UNCOLLECTABLE",
    "/DGC_ENABLE_SUSPEND_THREAD",
    "/DGC_GCJ_SUPPORT",
    "/DGC_MISSING_EXECINFO_H",
    "/DGC_NOT_DLL",
    "/DGC_NO_SIGSETJMP",
    "/DGC_THREADS",
    "/DJAVA_FINALIZATION",
    "/DNO_GETCONTEXT",
    "/DPARALLEL_MARK",
    "/DTHREAD_LOCAL_ALLOC",
    "/D_CRT_SECURE_NO_DEPRECATE",
]


def fail(message: str) -> None:
    raise ValueError(message)


def sha256_file(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(argv, *, env=None, cwd=None, timeout=600, what="command"):
    """Run a child; on non-zero exit fail with the tail of both streams.

    A heartbeat line is printed every 30 s while the child runs: the harness
    watchdog kills a window with no output for ten minutes, and a 13 MB C
    compile is exactly the kind of step that would otherwise stay silent.
    """
    print("carina-native-build: %s: %s" % (what, argv[0]), flush=True)
    done = threading.Event()

    def heartbeat():
        started = time.monotonic()
        while not done.wait(30.0):
            print(
                "carina-native-build: %s still running (%ds)"
                % (what, int(time.monotonic() - started)),
                flush=True,
            )

    beat = threading.Thread(target=heartbeat, daemon=True)
    beat.start()
    try:
        out = subprocess.run(
            argv,
            env=env,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        fail("%s timed out after %ss: %s" % (what, timeout, argv[0]))
    except OSError as exc:
        fail("could not start %s: %s (%s)" % (what, argv[0], exc))
    finally:
        done.set()
    if out.returncode != 0:
        stdout = out.stdout.decode("utf-8", "replace")
        stderr = out.stderr.decode("utf-8", "replace")
        detail = (stderr + "\n" + stdout).strip()
        fail(
            "%s failed (exit %s): %s\n%s"
            % (what, out.returncode, argv[0], "\n".join(detail.splitlines()[-25:]))
        )
    return out


def require_windows() -> None:
    if os.name != "nt":
        fail("oracle-free Carina seed is pinned only for windows-x86_64")


# ---------- toolchain discovery (port of nova-driver-support/src/lib.rs) ----------


def find_vcvars() -> pathlib.Path:
    env = os.environ.get("NOVA_VCVARS")
    if env and pathlib.Path(env).is_file():
        return pathlib.Path(env)
    pf86 = os.environ.get("ProgramFiles(x86)")
    if pf86:
        vswhere = (
            pathlib.Path(pf86)
            / "Microsoft Visual Studio"
            / "Installer"
            / "vswhere.exe"
        )
        if vswhere.is_file():
            out = subprocess.run(
                [
                    str(vswhere),
                    "-latest",
                    "-products",
                    "*",
                    "-requires",
                    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                    "-find",
                    r"VC\Auxiliary\Build\vcvars64.bat",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            for line in out.stdout.decode("utf-8", "replace").splitlines():
                candidate = pathlib.Path(line.strip())
                if candidate.is_file():
                    return candidate
    fail(
        "vcvars64.bat not found. Install Visual Studio Build Tools, "
        "or set NOVA_VCVARS to vcvars64.bat path."
    )


def capture_vcvars_env(vcvars: pathlib.Path) -> dict[str, str]:
    """One-shot `cmd /c "call vcvars64.bat > nul && set"` env snapshot."""
    # NOTE: a single command STRING, not an argv list — list2cmdline would
    # backslash-escape the inner quotes, which cmd.exe mangles (measured:
    # '"C:\...\vcvars64.bat" is not recognized'). This is the exact shape
    # Rust passes via raw_arg in capture_vcvars_env.
    inner = 'cmd /c "call "%s" > nul && set"' % vcvars
    out = subprocess.run(inner, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if out.returncode != 0:
        fail("vcvars64.bat failed (exit %s)" % out.returncode)
    env: dict[str, str] = {}
    for line in out.stdout.decode("utf-8", "replace").splitlines():
        key, sep, value = line.partition("=")
        if sep:
            env[key] = value
    if "INCLUDE" not in env or "LIB" not in env:
        fail("vcvars env snapshot lacks INCLUDE/LIB: " + str(vcvars))
    return env


def find_in_env_path(env: dict[str, str], name: str) -> str:
    for key, value in env.items():
        if key.lower() == "path":
            hit = shutil.which(name, path=value)
            if hit:
                return hit
    fail("%s not found in the vcvars environment PATH" % name)


def find_clang() -> str:
    env = os.environ.get("NOVA_CLANG")
    if env and pathlib.Path(env).is_file():
        return env
    for candidate in (
        r"C:\Program Files\LLVM\bin\clang.exe",
        r"C:\Program Files (x86)\LLVM\bin\clang.exe",
    ):
        if pathlib.Path(candidate).is_file():
            return candidate
    hit = shutil.which("clang")
    if hit:
        return hit
    fail("clang not found. Install LLVM or set NOVA_CLANG to clang.exe path.")


# ---------- fetch-seed ----------


def cmd_fetch_seed(args) -> None:
    require_windows()
    repo = pathlib.Path(args.repo).resolve()
    descriptor = repo / SEED_DESCRIPTOR
    if not descriptor.is_file():
        fail("seed descriptor missing: " + str(descriptor))
    import json

    meta = json.loads(descriptor.read_text(encoding="utf-8"))
    cache = pathlib.Path(args.cache).resolve()
    cache.mkdir(parents=True, exist_ok=True)
    asset = cache / meta["asset_name"]
    if not asset.is_file():
        url = meta["asset_url"]
        print("carina-native-build: downloading seed %s" % url, flush=True)
        try:
            with urllib.request.urlopen(url, timeout=600) as response:
                asset.write_bytes(response.read())
        except (OSError, ValueError) as exc:
            fail("seed download failed: %s" % exc)
    # Fail-closed verification lives in ONE place (scripts/tools/carina-gate.py);
    # this tool delegates instead of re-implementing the checks.
    run(
        [
            sys.executable,
            str(repo / "scripts/tools/carina-gate.py"),
            "verify-seed",
            "--descriptor",
            str(descriptor),
            "--asset",
            str(asset),
            "--platform",
            "windows-x86_64",
            "--expected-seed-source-revision",
            SEED_SOURCE,
        ],
        cwd=repo,
        what="verify-seed",
    )
    unpacked = cache / "unpacked"
    exe = unpacked / pathlib.PurePosixPath(meta["asset_name"].removesuffix(".zip")) / "novac.exe"  # novac-bin: not a selection -- the seed archive's inner layout is the release's contract
    if not exe.is_file():
        if unpacked.exists():
            shutil.rmtree(unpacked)
        with zipfile.ZipFile(asset) as zf:
            zf.extractall(unpacked)
    if not exe.is_file():
        fail("verified seed archive does not contain novac.exe: " + str(exe))  # novac-bin: not a selection -- release archive layout
    source_sha = exe.with_name("SOURCE-SHA")
    if not source_sha.is_file():
        fail("verified seed archive is missing SOURCE-SHA")
    actual_source = source_sha.read_text(encoding="utf-8").strip()
    if actual_source != SEED_SOURCE:
        fail("stale Carina B1 source: expected %s, got %s" % (SEED_SOURCE, actual_source))
    exe_sha = sha256_file(exe)
    if exe_sha != SEED_EXE_SHA256:
        fail("Carina B1 executable SHA256 mismatch: %s" % exe_sha)
    print(
        "carina-native-build fetch-seed PASS: %s sha256=%s source=%s"
        % (exe, exe_sha, SEED_SOURCE)
    )


# ---------- emit ----------


def run_seed_emit(seed: pathlib.Path, repo: pathlib.Path, entry: pathlib.Path,
                  self_root: pathlib.Path, timeout: int) -> bytes:
    """configure_seed_command (carina_build.rs) ported: cwd=repo, env is the
    inherited environment minus NOVA_CARINA_CAPTURE_DIR plus NOVA_STD_PATH
    and NOVAC_SELF_PATH; C arrives on stdout."""
    env = dict(os.environ)
    env.pop("NOVA_CARINA_CAPTURE_DIR", None)
    env["NOVA_STD_PATH"] = str(repo / "std" / "src")
    env["NOVAC_SELF_PATH"] = str(self_root)
    out = run(
        [str(seed), "emit", str(entry)],
        env=env,
        cwd=repo,
        timeout=timeout,
        what="seed emit",
    )
    c_text = out.stdout
    if not c_text or not c_text.lstrip().startswith(b"/*"):
        fail(
            "seed emit returned success but no generated C "
            "(stdout_bytes=%d, prefix=%r)" % (len(c_text), c_text[:120])
        )
    return c_text


def cmd_emit(args) -> None:
    require_windows()
    repo = pathlib.Path(args.repo).resolve()
    seed = pathlib.Path(args.seed).resolve()
    entry = (repo / args.entry).resolve()
    self_root = (repo / args.self_root).resolve()
    if not seed.is_file():
        fail("seed executable missing: " + str(seed))
    if not entry.is_file():
        fail("emit entry missing: " + str(entry))
    if not self_root.is_dir():
        fail("NOVAC_SELF_PATH root missing: " + str(self_root))
    c_text = run_seed_emit(seed, repo, entry, self_root, args.timeout)
    out_path = pathlib.Path(args.out).resolve()
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(c_text)
    print(
        "carina-native-build emit PASS: %s bytes=%d sha256=%s"
        % (out_path, len(c_text), hashlib.sha256(c_text).hexdigest())
    )


# ---------- libs (one-time libuv.lib + gc.lib) ----------


def write_rsp(path: pathlib.Path, lines: list[str]) -> None:
    """cl.exe response file; UTF-8 BOM so non-ASCII paths survive the ANSI
    codepage decode (registry No.287)."""
    path.write_text("\ufeff" + "\n".join(lines), encoding="utf-8")


def build_libuv(repo: pathlib.Path, cache: pathlib.Path, vcvars_env: dict[str, str]) -> pathlib.Path:
    libuv_dir = repo / "compiler-codegen" / "nova_rt" / "libuv"
    if not (libuv_dir / "include" / "uv.h").is_file():
        fail(
            "libuv submodule not initialized at %s. CI must checkout with "
            "submodules: recursive." % libuv_dir
        )
    cache_dir = cache / "libuv"
    lib_file = cache_dir / "libuv.lib"
    if lib_file.is_file():
        print("carina-native-build: libuv.lib cached: " + str(lib_file))
        return lib_file
    obj_dir = cache_dir / "obj"
    if obj_dir.is_dir():
        shutil.rmtree(obj_dir)
    obj_dir.mkdir(parents=True)
    src_root = libuv_dir / "src"
    srcs = sorted(src_root.glob("*.c")) + sorted((src_root / "win").glob("*.c"))
    if not srcs:
        fail("no libuv source files found in " + str(src_root))
    lines = [
        "/c /nologo /W0 /MT /O2 /D_WIN32_WINNT=0x0602 /DWIN32_LEAN_AND_MEAN /DBUILDING_UV_SHARED=0",
        '/I "%s"' % (libuv_dir / "include"),
        '/I "%s"' % src_root,
        '/I "%s"' % (src_root / "win"),
        '/Fo"%s\\\\"' % obj_dir,
    ]
    lines += ['"%s"' % s for s in srcs]
    rsp = cache_dir / "compile.rsp"
    write_rsp(rsp, lines)
    cl = find_in_env_path(vcvars_env, "cl.exe")
    run([cl, "@%s" % rsp], env=vcvars_env, timeout=1800, what="libuv cl.exe")
    objs = sorted(obj_dir.glob("*.obj"))
    if not objs:
        fail("libuv compile produced no .obj files")
    lib_lines = ["/nologo", '/OUT:"%s"' % lib_file] + ['"%s"' % o for o in objs]
    lib_rsp = cache_dir / "lib.rsp"
    write_rsp(lib_rsp, lib_lines)
    lib = find_in_env_path(vcvars_env, "lib.exe")
    run([lib, "@%s" % lib_rsp], env=vcvars_env, timeout=600, what="libuv lib.exe")
    if not lib_file.is_file():
        fail("libuv build succeeded but %s not found" % lib_file)
    print("carina-native-build: libuv.lib built (%d files)" % len(srcs))
    return lib_file


def build_boehm(repo: pathlib.Path, cache: pathlib.Path, vcvars_env: dict[str, str]) -> tuple[pathlib.Path, pathlib.Path]:
    rt_dir = repo / "compiler-codegen" / "nova_rt"
    gc_dir = rt_dir / "gc"
    ao_dir = rt_dir / "libatomic_ops"
    gc_include = gc_dir / "include"
    gc_amalgam = gc_dir / "extra" / "gc.c"
    if not (gc_include / "gc.h").is_file() or not gc_amalgam.is_file():
        fail(
            "bdwgc submodule not initialized at %s. CI must checkout with "
            "submodules: recursive." % gc_dir
        )
    if not (ao_dir / "src" / "atomic_ops.h").is_file():
        fail(
            "libatomic_ops submodule not initialized at %s. CI must checkout "
            "with submodules: recursive." % ao_dir
        )
    cache_dir = cache / "gc"
    gc_lib = cache_dir / "gc.lib"
    cache_include = cache_dir / "include"
    if (
        gc_lib.is_file()
        and (cache_include / "gc.h").is_file()
        and (cache_include / "gc" / "gc.h").is_file()
    ):
        print("carina-native-build: gc.lib cached: " + str(gc_lib))
        return gc_lib, cache_include
    # nova_rt needs BOTH <gc.h> (flat) and <gc/gc.h> (nested); the raw bdwgc
    # tree only ships the flat form (native_runtime.rs populate_boehm_include_dir).
    (cache_include / "gc").mkdir(parents=True, exist_ok=True)
    for header in gc_include.glob("*.h"):
        shutil.copy2(header, cache_include / header.name)
        shutil.copy2(header, cache_include / "gc" / header.name)
    obj_dir = cache_dir / "obj"
    if obj_dir.is_dir():
        shutil.rmtree(obj_dir)
    obj_dir.mkdir(parents=True)
    lines = [
        "/c /nologo /W0 /MT /O2 %s" % " ".join(BDWGC_DEFINES),
        '/I "%s"' % gc_include,
        '/I "%s"' % (ao_dir / "src"),
        '/Fo"%s\\\\"' % obj_dir,
        '"%s"' % gc_amalgam,
    ]
    rsp = obj_dir / "compile.rsp"
    write_rsp(rsp, lines)
    cl = find_in_env_path(vcvars_env, "cl.exe")
    run([cl, "@%s" % rsp], env=vcvars_env, timeout=1800, what="bdwgc cl.exe")
    objs = sorted(obj_dir.glob("*.obj"))
    if not objs:
        fail("bdwgc amalgamation compile produced no .obj files")
    lib_lines = ["/nologo", '/OUT:"%s"' % gc_lib] + ['"%s"' % o for o in objs]
    lib_rsp = obj_dir / "lib.rsp"
    write_rsp(lib_rsp, lib_lines)
    lib = find_in_env_path(vcvars_env, "lib.exe")
    run([lib, "@%s" % lib_rsp], env=vcvars_env, timeout=600, what="bdwgc lib.exe")
    if not gc_lib.is_file():
        fail("bdwgc build succeeded but %s not found" % gc_lib)
    shutil.rmtree(obj_dir, ignore_errors=True)
    print("carina-native-build: gc.lib built (bdwgc extra/gc.c amalgamation)")
    return gc_lib, cache_include


def cmd_libs(args) -> None:
    require_windows()
    repo = pathlib.Path(args.repo).resolve()
    cache = pathlib.Path(args.cache).resolve()
    cache.mkdir(parents=True, exist_ok=True)
    vcvars_env = capture_vcvars_env(find_vcvars())
    libuv_lib = build_libuv(repo, cache, vcvars_env)
    gc_lib, gc_include = build_boehm(repo, cache, vcvars_env)
    print(
        "carina-native-build libs PASS: libuv=%s gc=%s gc_include=%s"
        % (libuv_lib, gc_lib, gc_include)
    )


# ---------- link ----------


def effect_count_define(c_file: pathlib.Path) -> str | None:
    """native_runtime.rs effect_count_define_arg: line-1 `nova-effect-count: N`
    marker -> -DNOVA_MAX_EFFECT_STORAGES=N (ABI-uniform across all TUs)."""
    with open(c_file, "rb") as f:
        first = f.readline().decode("utf-8", "replace")
    marker = first.split("nova-effect-count:")
    if len(marker) < 2:
        return None
    try:
        count = int(marker[1].strip().removesuffix("*/").strip())
    except ValueError:
        return None
    if count == 0:
        return None
    return "-DNOVA_MAX_EFFECT_STORAGES=%d" % count


def link_c_exe(repo: pathlib.Path, cache: pathlib.Path, c_file: pathlib.Path,
               exe_file: pathlib.Path, timeout: int) -> None:
    require_windows()
    rt_dir = repo / "compiler-codegen" / "nova_rt"
    cg_include = repo / "compiler-codegen"
    libuv_dir = rt_dir / "libuv"
    eventloop_src = rt_dir / "eventloop.c"
    if not eventloop_src.is_file():
        fail("eventloop.c not found at " + str(eventloop_src))
    vcvars_env = capture_vcvars_env(find_vcvars())
    libuv_lib = build_libuv(repo, cache, vcvars_env)
    gc_lib, gc_include = build_boehm(repo, cache, vcvars_env)
    clang = find_clang()

    args = [
        clang,
        "--target=x86_64-pc-windows-msvc",
        # Mode::Dev flags (native_build.rs build_command).
        "-O0",
        "-g",
        "-Wno-everything",
        "-ffunction-sections",
        "-fdata-sections",
        # Plan 198 belt-and-suspenders stack reserve (16 MiB).
        "-Wl,/stack:0x1000000",
        # GcKind::Boehm client-side defines (Plan 44.5).
        "-DNOVA_GC_BOEHM",
        "-DGC_THREADS",
    ]
    define = effect_count_define(c_file)
    if define:
        args.append(define)
    args += [
        "-I",
        str(cg_include),
        "-DNOVA_USE_LIBUV=1",
        "-I",
        str(libuv_dir / "include"),
    ]
    # Windows libuv block: loose libuv-gated rt sources, the archive, the
    # event loop, then the system libs (build_command Clang branch).
    args += [str(rt_dir / name) for name in RT_LIBUV_SOURCES]
    args.append(str(libuv_lib))
    args.append(str(eventloop_src))
    args += ["-l" + name for name in LIBUV_WIN_SYSLIBS]
    # Boehm link flags.
    args += ["-I", str(gc_include), "-L", str(gc_lib.parent), "-lgc"]
    if (gc_lib.parent / "atomic_ops.lib").is_file():
        args.append("-latomic_ops")
    args += ["-o", str(exe_file), str(c_file)]
    # Pre-218 loose rt sources (use_rt_archive == False branch).
    args += [str(rt_dir / name) for name in RT_SOURCES]

    exe_file.parent.mkdir(parents=True, exist_ok=True)
    run(args, env=vcvars_env, timeout=timeout, what="clang link")
    if not exe_file.is_file():
        fail("clang reported success but %s not found" % exe_file)
    print(
        "carina-native-build link PASS: %s sha256=%s"
        % (exe_file, sha256_file(exe_file))
    )


def cmd_link(args) -> None:
    repo = pathlib.Path(args.repo).resolve()
    cache = pathlib.Path(args.cache).resolve()
    c_file = pathlib.Path(args.c).resolve()
    if not c_file.is_file():
        fail("generated C file missing: " + str(c_file))
    link_c_exe(repo, cache, c_file, pathlib.Path(args.out).resolve(), args.timeout)


# ---------- smoke ----------


SMOKE_SOURCE = 'fn main() Io -> () {\n    print("carina-smoke-ok")\n}\n'
SMOKE_EXPECTED = "carina-smoke-ok"


def cmd_smoke(args) -> None:
    """End-to-end proof for the freshly built compiler: emit a hello-world,
    link it with the same recipe, run it, compare stdout byte-for-byte."""
    require_windows()
    repo = pathlib.Path(args.repo).resolve()
    cache = pathlib.Path(args.cache).resolve()
    exe = pathlib.Path(args.exe).resolve()
    if not exe.is_file():
        fail("built compiler executable missing: " + str(exe))
    work = pathlib.Path(args.workdir).resolve()
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    entry = work / "hello.nv"
    entry.write_text(SMOKE_SOURCE, encoding="utf-8", newline="\n")
    # A program with no modules keeps the historical single-file invocation:
    # NOVAC_SELF_PATH is the compiler's own tree (carina_build.rs).
    c_text = run_seed_emit(exe, repo, entry, repo / "novac" / "src", args.timeout)
    c_file = work / "hello.c"
    c_file.write_bytes(c_text)
    smoke_exe = work / "hello.exe"
    link_c_exe(repo, cache, c_file, smoke_exe, args.timeout)
    out = run([str(smoke_exe)], timeout=120, what="smoke run")
    actual = out.stdout.decode("utf-8", "replace").strip()
    if actual != SMOKE_EXPECTED:
        fail("smoke stdout mismatch: got %r, expected %r" % (actual, SMOKE_EXPECTED))
    print("carina-native-build smoke PASS: %s printed %r" % (smoke_exe, actual))


# ---------- main ----------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="carina-native-build")
    sub = parser.add_subparsers(dest="action", required=True)

    s = sub.add_parser("fetch-seed")
    s.add_argument("--repo", default=".")
    s.add_argument("--cache", required=True)
    s.set_defaults(func=cmd_fetch_seed)

    s = sub.add_parser("emit")
    s.add_argument("--repo", default=".")
    s.add_argument("--seed", required=True)
    s.add_argument("--entry", default="novac/src/main.nv")
    s.add_argument("--self-root", default="novac/src")
    s.add_argument("--out", required=True)
    s.add_argument("--timeout", type=int, default=900)
    s.set_defaults(func=cmd_emit)

    s = sub.add_parser("libs")
    s.add_argument("--repo", default=".")
    s.add_argument("--cache", required=True)
    s.set_defaults(func=cmd_libs)

    s = sub.add_parser("link")
    s.add_argument("--repo", default=".")
    s.add_argument("--cache", required=True)
    s.add_argument("--c", required=True)
    s.add_argument("--out", required=True)
    s.add_argument("--timeout", type=int, default=1800)
    s.set_defaults(func=cmd_link)

    s = sub.add_parser("smoke")
    s.add_argument("--repo", default=".")
    s.add_argument("--exe", required=True)
    s.add_argument("--cache", required=True)
    s.add_argument("--workdir", required=True)
    s.add_argument("--timeout", type=int, default=900)
    s.set_defaults(func=cmd_smoke)

    args = parser.parse_args(argv)
    try:
        args.func(args)
    except (OSError, ValueError) as exc:
        print("carina-native-build %s FAIL: %s" % (args.action, exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.exit(main())
