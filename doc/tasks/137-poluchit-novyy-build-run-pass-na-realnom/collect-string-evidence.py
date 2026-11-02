"""Retain byte-exact evidence of a measured local candidate, not the WIP tree."""
import hashlib
import json
import pathlib
import re
import shutil
import sys

s = pathlib.Path(sys.argv[1]).resolve()
label = sys.argv[2]
version = sys.argv[3] if len(sys.argv) > 3 else "v4"
root = pathlib.Path.cwd()
dest = root / "doc/tasks/137-poluchit-novyy-build-run-pass-na-realnom/evidence" / label
dest.mkdir(parents=True)
files = [
    "string-full-manifest-green.json", "string-full-manifest-sabotage.json", "string-backport.patch",
    "string-build-green.stdout", "string-build-green.stderr", "string-build-sabotage.stdout", "string-build-sabotage.stderr",
    "verify-run-green-v2.log", "verify-run-sabotage-v2.log", "std-before.stdout", "std-before.stderr", "std-after.stdout", "std-after.stderr",
    "string-final-green-v2/results.json", "string-sabotage-v2/results.json",
    "string-final-green-v2/real-native-command.json", "string-final-green-v2/real-run.stdout",
    "string-final-green-v2/live.stdout", "string-final-green-v2/tag-live.stdout",
    "string-final-green-v2/unclosed.stdout", "string-final-green-v2/escaped-close.stdout",
    "string-extra-probes/generic-tag-before.stdout", "string-extra-probes/generic-tag-after.stdout",
    "string-extra-probes/shell-tag-before.stdout", "string-extra-probes/shell-tag-after.stdout",
    "string-supplement/orm_decorators.stdout", "string-supplement/orm_demo.stdout", "subset-inventory.json",
]
files = [p.replace("-v2", "-" + version) for p in files]
if version == "v4":
    comparison_path = s / "std-batch-comparison/comparison.json"
    comparison = json.loads(comparison_path.read_text(encoding="utf-8"))
    witnesses = []
    for new in comparison["added"]:
        old = [d for d in comparison["before"]["diagnostics"]
               if d["primary"]["file"] == new["primary"]["file"]
               and d["primary"]["start"] == new["primary"]["start"] and "did not parse" in d["message"]]
        assert old, "new std diagnostic has no baseline fallback witness"
        witnesses.append(dict(after=new, baseline_fallbacks=old))
    summary = dict(before_rc=comparison["before"]["rc"], after_rc=comparison["after"]["rc"],
                   before_diagnostics=len(comparison["before"]["diagnostics"]),
                   after_diagnostics=len(comparison["after"]["diagnostics"]),
                   added=comparison["added"], removed=comparison["removed"], witnesses=witnesses,
                   raw_full_comparison_sha256=hashlib.sha256(comparison_path.read_bytes()).hexdigest(),
                   raw_full_comparison_path=str(comparison_path))
    (s / "std-batch-comparison/summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
    files = [p for p in files if not p.startswith("string-extra-probes/")]
    files += ["candidate-build-command-green.json", "candidate-build-command-sabotage.json",
              "string-final-green-v4/generic-tag.stdout", "string-final-green-v4/overloaded-tag.stdout",
              "final-refusal-comparison/before-after.json", "std-batch-comparison/summary.json",
              "std-batch-comparison/before-command.json", "std-batch-comparison/after-command.json",
              "string-extra-probes-v2/roundtrip.run.log", "string-extra-probes-v2/roundtrip.native.log"]
manifest = {}
for relative in files:
    source = s / relative
    assert source.is_file(), source
    target = dest / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    manifest[relative] = hashlib.sha256(source.read_bytes()).hexdigest()
artifacts = {
    "compiler": s / "novac137.exe",
    "compiler_sabotage": s / "novac137-sabotage.exe",
    "real_source": root / "examples/basics/string_literals.nv",
    "real_c": s / ("string-final-green-" + version + "/real.c"),
    "real_exe": s / ("string-final-green-" + version + "/real.exe"),
}
for key in ("compiler", "compiler_sabotage"):
    retained = s / (label + "-" + key + ".exe")
    shutil.copyfile(artifacts[key], retained)
    artifacts[key] = retained
artifacts["runtime_archive"] = next((s / "install135/target/rt-archive-cache").rglob("libnova_rt.lib"))
artifacts["libuv"] = next((s / "install135/target/libuv-cache").rglob("libuv.lib"))
artifacts["gc"] = s / "install135/target/gc-cache/gc.lib"
build_log = (s / "string-build-green.stdout").read_text(encoding="utf-8")
pid = re.search(r"main\.(\d+)\.src", build_log).group(1)
artifacts["compiler_c"] = s / ("install135/target/carina-build/main." + pid + ".carina.c")
assert hashlib.sha256(artifacts["compiler_c"].read_bytes()).hexdigest() == re.search(r"generated_c_sha256=([0-9a-f]{64})", build_log).group(1)
artifacts["builder_driver"] = s / "nova135.exe"
artifacts["parent_seed_zip"] = s / "nova-carina-0.2.2-135combined-windows-x86_64.zip"
record = {k: dict(path=str(p), sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for k, p in artifacts.items()}
record["evidence_sha256"] = manifest
record["warning"] = "Measured candidate snapshot; later working-tree edits are not asserted to belong to this binary. No seed asset is published."
(dest / "artifact-manifest.json").write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding="utf-8")
print("Evidence saved: " + str(dest))
