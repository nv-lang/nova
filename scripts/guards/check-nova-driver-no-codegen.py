#!/usr/bin/env python3
"""Assert the exact production nova driver target has no compiler dependency.

Scope: docs/plans/274.16-carina-all-examples-and-packages.md, task 111.
The inspected target is nova-driver/Cargo.toml for Windows x86_64 MSVC.
Cargo failure and every direct/transitive compiler edge fail closed.
The support crate graph alone is not a production executable proof.
Success emits an LF-terminated ok: receipt for the gate wrapper.
--self-test exercises forbidden-edge recognition, not native compilation.
Real dependency sabotage and installed Carina D89 tests remain separate.
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

TARGET = "x86_64-pc-windows-msvc"


def forbidden(tree: str) -> list[str]:
    return [line for line in tree.splitlines() if "nova-codegen" in line.lower() or "compiler-codegen" in line.lower()]


def inspect(manifest: Path, target: str) -> tuple[int, str]:
    proc = subprocess.run(
        ["cargo", "tree", "--manifest-path", str(manifest), "--edges", "normal", "--target", target, "--prefix", "none"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    bad = forbidden(proc.stdout)
    if proc.returncode:
        return proc.returncode, proc.stdout
    if bad:
        return 1, "forbidden compiler dependency in production driver graph:\n" + "\n".join(bad) + "\n\nComplete dependency tree:\n" + proc.stdout
    return 0, proc.stdout


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("root", nargs="?", default=".")
    parser.add_argument("--target", default=TARGET)
    parser.add_argument("--manifest", type=Path, help="inspect an explicit manifest (guard default remains nova-driver/Cargo.toml)")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = Path(args.root).resolve()
    manifest = args.manifest.resolve() if args.manifest else root / "nova-driver" / "Cargo.toml"
    print(f"production manifest: {manifest}\ntarget triple: {args.target}")
    status, output = inspect(manifest, args.target)
    if status:
        print(output, file=sys.stderr)
        return status
    print(output, end="")
    print("check-nova-driver-no-codegen ok: production graph clean")
    if args.self_test:
        injected_edge = "nova-codegen v0.1.0 (/synthetic/compiler-codegen)"
        if not forbidden(output + "\n" + injected_edge):
            print("self-test failed: synthetic nova-codegen edge was not detected", file=sys.stderr)
            return 1
        print("self-test classifier: injected nova-codegen tree line RED; production graph remains GREEN")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
