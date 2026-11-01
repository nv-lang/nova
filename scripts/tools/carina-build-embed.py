#!/usr/bin/env python3
"""Targeted real-driver embed staging check (reg 1945, task 136).

Uses only synthetic public resource bytes. All mutable inputs and installation
outputs live in the caller's session scratchpad. No seed/pin overrides, Oracle,
satellite checkout builds or recursive checkout copies are used.
"""
from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import subprocess


def put(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def junction(link: Path, target: Path) -> None:
    link.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(target)],
                   check=True, capture_output=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", type=Path, required=True)
    parser.add_argument("--scratch", type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    driver = args.driver.resolve()
    work = args.scratch.resolve()
    work.mkdir(parents=True, exist_ok=False)
    install = work / "installation"
    # Only these two distribution assets are copied from the checkout.
    for rel in ["docs/dev/carina-seeds/windows-x86_64.json", "scripts/tools/carina-gate.py"]:
        put(install / rel, (repo / rel).read_bytes())
    (install / "novac/src").mkdir(parents=True)
    junction(install / "std/src", repo / "std/src")
    # Native compilation reads the runtime sources, writing its cache ONLY to
    # installation/target. An initialized runtime may be supplied explicitly.
    runtime = Path(os.environ.get("CARINA_EMBED_RUNTIME", str(repo / "compiler-codegen/nova_rt"))).resolve()
    junction(install / "runtime", runtime)
    fixture = repo / "spec_tests/conformance/fixtures/embed_stage_probe"
    project = work / "project"
    for source in fixture.rglob("*.nv"):
        rel = source.relative_to(fixture)
        if rel.as_posix() == "dep/assetdep.nv":
            rel = Path("dep/src/assetdep.nv")
        put(project / rel, source.read_bytes())
    put(project / "nova.toml", b'[package]\nname = "embed_stage_probe"\nversion = "0.1.0"\n[lib]\nsrc = "."\n[dependencies]\nassetdep = { path = "dep" }\n')
    put(project / "dep/nova.toml", b'[package]\nname = "assetdep"\nversion = "0.1.0"\n[lib]\nsrc = "src"\n')
    put(project / "dep/src/shell.tpl.c", b"shell-136")
    put(project / "dep/src/certs/localhost_cert.pem", b"cert-136")
    put(project / "dep/src/unreferenced.txt", b"must not copy")
    put(project / "dep/assets/data.nv", b"shell-136")
    put(project / "dep/assets/data.bin", b"shell-136")
    originals = {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in project.rglob("*") if p.is_file()}
    # D420 allows a plain path dependency only when its sources travel with
    # the same clone. This is an isolated synthetic repository, not a satellite.
    subprocess.run(["git", "init", "--quiet", str(project)], check=True)
    subprocess.run(["git", "-C", str(project), "add", "--",
                    *[p.relative_to(project).as_posix() for p in sorted(originals)]], check=True)
    env = os.environ.copy()
    env["NOVA_RT_DIR"] = str(install / "runtime")
    env["NOVA_CG_INCLUDE"] = str(repo / "compiler-codegen")
    env.pop("NOVA_CARINA_SEED_ZIP", None)
    env.pop("NOVA_CARINA_SEED_URL", None)
    exe = work / "embed-probe.exe"

    def build(label: str) -> subprocess.CompletedProcess:
        run = subprocess.run([str(driver), "build", str(project / "main.nv"), "-o", str(exe),
                              "--toolchain", "clang", "--keep-artifacts", "--timeout", "120"],
                             cwd=install, env=env, capture_output=True, timeout=240)
        put(work / (label + ".stdout"), run.stdout)
        put(work / (label + ".stderr"), run.stderr)
        print(run.stdout.decode("utf-8", "replace"), end="", flush=True)
        print(run.stderr.decode("utf-8", "replace"), end="", flush=True)
        return run

    result = build("positive")
    assert result.returncode == 0, f"carina-embed RED: build exit={result.returncode}"
    receipt = result.stdout.decode("utf-8", "replace")
    assert "driver_stamp=carina-b1-build-v1" in receipt
    assert "source file(s) staged under" in receipt
    native = subprocess.run([str(exe)], capture_output=True, timeout=30)
    actual = native.stdout.replace(b"\r\n", b"\n")
    assert native.returncode == 0 and actual == b"shell-136\ncert-136\nown\n", (native.returncode, actual, native.stderr)
    stages = list((install / "target/carina-build").glob("*.src"))
    assert len(stages) == 1, stages
    staged = stages[0]
    assert (staged / "_deps/assetdep/certs/localhost_cert.pem").read_bytes() == b"cert-136"
    assert (staged / "_deps/assetdep/shell.tpl.c").read_bytes() == b"shell-136"
    assert not (staged / "_deps/assetdep/unreferenced.txt").exists()
    assert all(hashlib.sha256(p.read_bytes()).hexdigest() == sha for p, sha in originals.items())
    print("carina-embed PASS: real driver build=0 run=0 stdout=shell-136,cert-136,own; sources unchanged", flush=True)

    resource_source = project / "dep/src/assetdep.nv"
    source_bytes = resource_source.read_bytes()
    resource_source.write_bytes(source_bytes.replace(b"shell.tpl.c", b"../assets/data.bin"))
    parent_data = build("parent-data-outside-source-root")
    assert parent_data.returncode == 0
    parent_native = subprocess.run([str(exe)], capture_output=True, timeout=30)
    assert parent_native.returncode == 0 and parent_native.stdout.replace(b"\r\n", b"\n") == b"shell-136\ncert-136\nown\n"
    print("carina-embed PASS: relative parent resource outside source-root preserves bytes", flush=True)
    resource_source.write_bytes(source_bytes.replace(b"shell.tpl.c", b"../assets/data.nv"))
    nv_data = build("nv-data-outside-source-root")
    assert nv_data.returncode == 0
    nv_native = subprocess.run([str(exe)], capture_output=True, timeout=30)
    assert nv_native.returncode == 0 and nv_native.stdout.replace(b"\r\n", b"\n") == b"shell-136\ncert-136\nown\n"
    print("carina-embed PASS: .nv bytes outside source-root but inside package-root survive as data", flush=True)
    resource_source.write_bytes(source_bytes.replace(b"shell.tpl.c", b"absent.bin"))
    missing = build("missing")
    assert missing.returncode != 0 and b"E_EMBED_NOT_FOUND" in missing.stdout + missing.stderr
    print("carina-embed PASS: missing resource rejected with E_EMBED_NOT_FOUND", flush=True)
    resource_source.write_bytes(source_bytes.replace(b"shell.tpl.c", b"../../outside.bin"))
    put(project / "outside.bin", b"outside the dependency package")
    outside = build("outside")
    assert outside.returncode != 0 and b"outside package root" in outside.stderr
    print("carina-embed PASS: package escape rejected before resource copy", flush=True)
    resource_source.write_bytes(source_bytes)
    assert all(hashlib.sha256(p.read_bytes()).hexdigest() == sha for p, sha in originals.items())


if __name__ == "__main__":
    main()
