#!/usr/bin/env python3
"""Self-test the Windows Carina C-level verifier and its plan wording."""

from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/tools/verify-carina-c-level-fixed-point.py"
PLAN = ROOT / "docs/plans/274.16-carina-all-examples-and-packages.md"
BOOTSTRAP = ROOT / "docs/dev/novac-bootstrap.md"


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


spec = importlib.util.spec_from_file_location("carina_c_level", SCRIPT)
if spec is None or spec.loader is None:
    fail("cannot load verifier")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

with tempfile.TemporaryDirectory(prefix="carina-c-level-selftest-") as temp:
    root = Path(temp)
    exe_bytes = b"selftest shipped executable"
    c_bytes = b"stage-A generated C bytes"
    module.NOVAC_SHA256 = hashlib.sha256(exe_bytes).hexdigest()
    module.ASSET_SHA256 = "unused-for-direct-exe"
    module.EXPECTED_C_SIZE = len(c_bytes)
    module.EXPECTED_C_SHA256 = hashlib.sha256(c_bytes).hexdigest()
    exe = root / "novac.exe"
    exe.write_bytes(exe_bytes)
    if module.validate_asset(exe)[0] != exe_bytes:
        fail("valid shipped executable rejected")
    module.check_c(c_bytes, expected_size=len(c_bytes),
                   expected_sha256=hashlib.sha256(c_bytes).hexdigest())
    try:
        module.check_c(c_bytes, expected_size=len(c_bytes), expected_sha256="0" * 64)
    except module.VerificationError as exc:
        if "RED" not in str(exc):
            fail("bad expected C digest did not report RED")
    else:
        fail("sabotaged expected C digest was accepted")
    try:
        module.validate_asset(root / "missing.zip")
    except module.VerificationError as exc:
        if "RED" not in str(exc):
            fail("missing asset did not report RED")
    else:
        fail("missing asset was accepted")
    archive = root / "published.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("package/novac.exe", exe_bytes)
    module.ASSET_SHA256 = hashlib.sha256(archive.read_bytes()).hexdigest()
    if module.validate_asset(archive) != (exe_bytes, "package/novac.exe"):
        fail("valid published ZIP rejected")
    module.ASSET_SHA256 = "0" * 64
    try:
        module.validate_asset(archive)
    except module.VerificationError as exc:
        if "RED" not in str(exc):
            fail("sabotaged asset digest did not report RED")
    else:
        fail("sabotaged asset digest was accepted")

    original_run = module.subprocess.run
    module.subprocess.run = lambda *args, **kwargs: subprocess.CompletedProcess(args[0], 1, b"", b"")
    try:
        module.check_source_tree(root)
    except module.VerificationError as exc:
        if "RED" not in str(exc):
            fail("sabotaged source revision did not report RED")
    else:
        fail("sabotaged source revision was accepted")
    finally:
        module.subprocess.run = original_run

plan = PLAN.read_text(encoding="utf-8")
bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
required = (
    "Решение владельца 2026-10-19",
    "25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0",
    "verifier не исполняет Oracle",
    "критерии 0.3",
    "саботаж asset/SHA/source или expected C digest",
)
for text in required:
    if text not in plan:
        fail(f"274.16 missing required scope/evidence wording: {text}")
if "Решение владельца 2026-10-19" not in bootstrap or "не gate" not in bootstrap:
    fail("bootstrap guide missing C-level decision or native-equality boundary")
if "должны совпасть по generated C и native binary" in plan:
    fail("stale native binary equality prerequisite remains in 274.16")
print("PASS: verifier selftest — asset/source/C sabotage RED; restoration GREEN")
print("PASS: bootstrap docs guard — decision, evidence, Linux and 0.3 boundaries")
