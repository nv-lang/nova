#!/usr/bin/env python3
"""Self-test the Windows Carina C-level verifier and its plan wording."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
from io import BytesIO
from pathlib import Path
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/tools/verify-carina-bootstrap-c.py"
PLAN = ROOT / "docs/plans/274.16-carina-all-examples-and-packages.md"
BOOTSTRAP = ROOT / "docs/dev/novac-bootstrap.md"
RELEASE_NOTES = ROOT / "docs/dev/carina-release-notes-v0.2.1.md"
PLAN_27411 = ROOT / "docs/plans/274.11-carina-release.md"
PLAN_274 = ROOT / "docs/plans/274-novac-self-hosted-compiler.md"


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


spec = importlib.util.spec_from_file_location("carina_c_level", SCRIPT)
if spec is None or spec.loader is None:
    fail("cannot load verifier")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

exe_bytes = b"selftest shipped executable"
c_bytes = b"stage-A generated C bytes"
baseline = module.load_baseline()
baseline["b1_exe_sha256"] = hashlib.sha256(exe_bytes).hexdigest()
baseline["expected_c_bytes"] = len(c_bytes)
baseline["expected_c_sha256"] = hashlib.sha256(c_bytes).hexdigest()
module.validate_b1_executable(exe_bytes, baseline)
try:
    module.validate_b1_executable(exe_bytes + b"tamper", baseline)
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("sabotaged shipped executable digest was accepted")
module.validate_c_observation(c_bytes, baseline)
sabotaged = copy.deepcopy(baseline)
sabotaged["expected_c_sha256"] = "0" * 64
try:
    module.check_c(c_bytes, sabotaged)
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("sabotaged expected C digest was accepted")
print(module.validate_c_observation(c_bytes, baseline))

archive_buffer = BytesIO()
with zipfile.ZipFile(archive_buffer, "w") as zf:
    zf.writestr("package/carina-bin", exe_bytes)
archive_bytes = archive_buffer.getvalue()
baseline["asset_name"] = "published.zip"
baseline["asset_sha256"] = hashlib.sha256(archive_bytes).hexdigest()
baseline["executable_member"] = "package/carina-bin"
zip_exe, zip_member = module.validate_zip_data("published.zip", archive_bytes, baseline)
if (zip_exe, zip_member) != (exe_bytes, baseline["executable_member"]):
    fail("valid published ZIP bytes rejected")
sabotaged = copy.deepcopy(baseline)
sabotaged["asset_sha256"] = "0" * 64
try:
    module.validate_zip_data("published.zip", archive_bytes, sabotaged)
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("sabotaged asset digest was accepted")
try:
    module.validate_zip_data("missing.zip", archive_bytes, baseline)
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("wrong asset name was accepted")
print("GREEN: ZIP name/digest and shipped B1 digest match managed test baseline")
try:
    module.validate_asset(ROOT / "scripts/guards/asset-that-does-not-exist.zip", baseline)
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("missing published asset was accepted")

module.validate_source_state(0, 0, 0, "")
module.validate_workspace_markers(True, False)
try:
    module.check_source_tree(ROOT / "source-tree-that-does-not-exist", baseline)
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("missing source tree was accepted")
try:
    module.validate_source_state(0, 1, 0, "")
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("sabotaged source revision was accepted")
try:
    module.validate_workspace_markers(False, False)
except module.VerificationError as exc:
    print(str(exc))
else:
    fail("sabotaged workspace source marker was accepted")
print("GREEN: source tree ancestry/content/cleanliness match managed test baseline")

with tempfile.TemporaryDirectory(prefix="carina-door-selftest-") as scratch:
    pinned_b1 = Path(scratch) / "pinned-b1"
    pinned_b1.write_bytes(exe_bytes)
    if module.select_b1_through_door(ROOT, pinned_b1) != pinned_b1.resolve():
        fail("novac_bin door did not select the explicit pinned B1")
print("GREEN: explicit pinned B1 selected through novac_bin door")

plan = PLAN.read_text(encoding="utf-8")
bootstrap = BOOTSTRAP.read_text(encoding="utf-8")
release_notes = RELEASE_NOTES.read_text(encoding="utf-8")
plan_27411 = PLAN_27411.read_text(encoding="utf-8")
plan_274 = PLAN_274.read_text(encoding="utf-8")
baseline_doc = (ROOT / "scripts/guards/carina-bootstrap-c.baseline").read_text(encoding="utf-8")
release_notes_flat = " ".join(release_notes.split())
required_plan = (
    "Решение владельца 2026-10-19",
    "25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0",
    "verifier не исполняет Oracle",
    "критерии 0.3",
    "саботаж asset/SHA/source или expected C digest",
    "Карина с0.2.1 собирается без запуска Oracle",
)
for text in required_plan:
    if text not in plan:
        fail(f"274.16 missing required scope/evidence wording: {text}")
if "Решение владельца 2026-10-19" not in bootstrap or "не gate" not in bootstrap:
    fail("bootstrap guide missing C-level decision or native-equality boundary")
if "должны совпасть по generated C и native binary" in plan:
    fail("stale native binary equality prerequisite remains in 274.16")
for text in ("Карина с0.2.1 собирается без запуска Oracle", "B1.c", "B2raw.c", "not proven and is not required",
             "Clang built the published B1 executable from that C", "cmd` raw stdout redirection",
             "absolute `NOVAC_SELF_PATH` was not tested"):
    if text not in release_notes_flat:
        fail(f"release notes missing exact owner claim/evidence boundary: {text}")
if "oracle" in release_notes.lower() and "one-time historical stage A" not in release_notes_flat:
    fail("release notes do not label Oracle as historical one-time stage A")
for text in ("B1.c=B2raw.c", "снимает будущее требование", "это не условие bootstrap/сборки/поведения Carina"):
    if text not in plan_27411:
        fail(f"274.11 missing current Oracle-free boundary: {text}")
for text in ("Windows C-level fixed point", "Native B2 executable/PDB/LIB equality не является условием",
             "критерий Windows Carina 0.2.1 выше уточнён решением владельца"):
    if text not in plan_274:
        fail(f"274 parent missing superseding Windows criterion: {text}")
plan_2743 = (ROOT / "docs/plans/274.3-review-findings-classes.md").read_text(encoding="utf-8")
migration_docs = (" ".join(plan_274.split()) + " " + " ".join(plan_27411.split()) + " "
                 + " ".join(plan_2743.split())).lower()
for text in ("обязательной abi symbol/coverage проверкой",
             "runtime headers", "старые gate-вызовы физически", "не отключаются"):
    if text not in migration_docs:
        fail(f"274 migration wording must preserve mandatory coverage: {text}")
if "update_reason" not in baseline_doc or "any future digest/source/asset change requires an explicit reviewed edit" not in baseline_doc:
    fail("managed baseline has no reason field")
print("PASS: verifier selftest — asset/source/C sabotage RED; restoration GREEN")
print("PASS: bootstrap docs guard — decision, evidence, Linux and 0.3 boundaries")
