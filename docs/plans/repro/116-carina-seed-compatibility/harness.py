#!/usr/bin/env python3
"""Offline verifier for task 116 receipts. Never spawns compilers or Oracle."""

import copy
import hashlib
import json
import os
import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parent
MANIFEST_PATH = ROOT / "manifest.json"

PINNED = {
    "main_head": "86af6bb82963148784868edb6e02e20aee6330de",
    "source_target": "novac/src/main.nv",
    "source_target_sha256": "3104bbe6b6deea8995d49dc78a7f36fe9096bb6437eeb937b41426d247e19da8",
    "seed_zip_sha256": "7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb",
    "seed_exe_sha256": "824bf597f8f073c68961df8b824ef5d4e72c4b463c47b727a4e121716f6996f4",
    "seed_source_sha": "3729abcce42ec2d94efc898ec6eafe91ff6210d5",
    "generated_c_sha256": "25c5404f1e8e175652f19c9ca6cb03d29de82cee70a7cd9f4258c6724115c8a0",
    "native_exe_sha256": "8287b9dd0bfb69170a14bdd5fbe420e4124966375dbfc5ab2334aebe555f3d08",
}


class Refusal(Exception):
    def __init__(self, code, detail):
        super().__init__(detail)
        self.code = code
        self.detail = detail


def digest(path):
    h = hashlib.sha256()
    with pathlib.Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def safe_path(relative):
    path = pathlib.PurePosixPath(relative)
    if path.is_absolute() or ".." in path.parts:
        raise Refusal("REFUSE_ARTIFACT_PATH", relative)
    target = ROOT.joinpath(*path.parts).resolve()
    try:
        target.relative_to(ROOT.resolve())
    except ValueError:
        raise Refusal("REFUSE_ARTIFACT_PATH", relative)
    return target


def require_output(path):
    target = pathlib.Path(path)
    if not target.is_file():
        raise Refusal("REFUSE_OUTPUT_MISSING", str(path))
    return target


def verify_record(record):
    target = safe_path(record["path"])
    if not target.is_file():
        raise Refusal("REFUSE_OUTPUT_MISSING", record["path"])
    actual_size = target.stat().st_size
    if actual_size != record["size"]:
        raise Refusal("REFUSE_RECEIPT_SIZE_MISMATCH", record["path"])
    actual_hash = digest(target)
    if actual_hash != record["sha256"]:
        raise Refusal("REFUSE_RECEIPT_HASH_MISMATCH", record["path"])


def verify_pins(pins):
    for name in ("seed_zip_sha256", "seed_exe_sha256", "seed_source_sha"):
        if pins.get(name) != PINNED[name]:
            raise Refusal("REFUSE_WRONG_SEED_PIN", name)
    if (pins.get("source_target") != PINNED["source_target"] or
            pins.get("source_target_sha256") != PINNED["source_target_sha256"]):
        raise Refusal("REFUSE_WRONG_SOURCE_PIN", "novac/src/main.nv")
    if pins.get("generated_c_sha256") != PINNED["generated_c_sha256"]:
        raise Refusal("REFUSE_WRONG_GENERATED_C_HASH", "generated C pin")
    if pins.get("native_exe_sha256") != PINNED["native_exe_sha256"]:
        raise Refusal("REFUSE_WRONG_NATIVE_EXE_HASH", "published EXE pin")
    if pins.get("compiler_source_kind") != "carina_seed":
        raise Refusal("REFUSE_ORACLE_COMPILER", str(pins.get("compiler_source_kind")))
    if pins.get("source_compiler_executable_sha256") != PINNED["seed_exe_sha256"]:
        raise Refusal("REFUSE_WRONG_SEED_COMPILER", "source compiler hash")
    if pins.get("oracle_executed_for_source_stage") is not False:
        raise Refusal("REFUSE_ORACLE_STAGE", "source-stage oracle execution is not permitted")


def load_json(relative):
    return json.loads(safe_path(relative).read_bytes())


def verify_smoke(bundle):
    summary = load_json("receipts/smoke/smoke-summary.json")
    if summary.get("all_three_passed") is not True:
        raise Refusal("REFUSE_SMOKE_NOT_GREEN", "three-case summary")
    if (summary.get("native_exe_executed") is not True or
            summary.get("confirmed_process_starts") != 3 or
            summary.get("unconfirmed_attempts") != 0 or
            summary.get("retry_count") != 0):
        raise Refusal("REFUSE_SMOKE_EXECUTION_STATE", "launch counters/state")

    if len(summary.get("steps", [])) != 3:
        raise Refusal("REFUSE_SMOKE_CASE_COUNT", "expected three recorded cases")

    expected_ids = ["01-no-args", "02-check-hello", "03-check-missing-absolute"]
    for step, expected_id in zip(summary["steps"], expected_ids):
        if step.get("id") != expected_id or step.get("expected_match") is not True:
            raise Refusal("REFUSE_SMOKE_CASE", expected_id)
        prefix = "receipts/smoke/" + expected_id
        stdout_path = safe_path(prefix + ".stdout.bin")
        stderr_path = safe_path(prefix + ".stderr.bin")
        status_path = safe_path(prefix + ".status")
        launch_path = safe_path(prefix + ".launch.json")
        stdout = stdout_path.read_bytes()
        stderr = stderr_path.read_bytes()
        status = int(status_path.read_text(encoding="ascii").strip())
        launch = json.loads(launch_path.read_bytes())
        expected = step["expected"]
        accepted = [value.encode("utf-8") for value in expected["stdout_variants"]]
        if stdout not in accepted or stderr != expected["stderr"].encode("utf-8"):
            raise Refusal("REFUSE_SMOKE_RAW_STREAM", expected_id)
        if status != expected["status"] or launch.get("exit_status") != status:
            raise Refusal("REFUSE_SMOKE_STATUS", expected_id)
        if launch.get("spawn_error") is not None or launch.get("timed_out") is not False:
            raise Refusal("REFUSE_SMOKE_LAUNCH", expected_id)
        if launch.get("environment_overrides", {}).get("NOVA_STD_PATH") is None:
            raise Refusal("REFUSE_SMOKE_ENV", expected_id)


def verify_bundle():
    bundle = json.loads(MANIFEST_PATH.read_bytes())
    if bundle.get("schema") != 1:
        raise Refusal("REFUSE_MANIFEST_SCHEMA", str(bundle.get("schema")))
    scope_source = bundle.get("scope_source", {})
    scope_path = pathlib.Path(scope_source.get("path", ""))
    if scope_path.is_absolute() or ".." in scope_path.parts:
        raise Refusal("REFUSE_SCOPE_SOURCE_PATH", str(scope_path))
    scope_file = (ROOT.parents[3] / scope_path).resolve()
    try:
        scope_file.relative_to(ROOT.parents[3].resolve())
    except ValueError:
        raise Refusal("REFUSE_SCOPE_SOURCE_PATH", str(scope_path))
    if not scope_file.is_file() or digest(scope_file) != scope_source.get("sha256"):
        raise Refusal("REFUSE_SCOPE_SOURCE_HASH", str(scope_path))
    verify_pins(bundle.get("pins", {}))
    retention = bundle.get("owner_retention_policy", {})
    if retention.get("generated_C_included") is not False:
        raise Refusal("REFUSE_RETENTION_POLICY", "generated C must remain excluded")
    if retention.get("native_EXE_included") is not False:
        raise Refusal("REFUSE_RETENTION_POLICY", "native EXE must remain excluded")
    if retention.get("seed_archive_or_exe_included") is not False:
        raise Refusal("REFUSE_RETENTION_POLICY", "published seed assets must remain external")
    files = bundle.get("files", [])
    if len(files) != bundle.get("audited_file_count"):
        raise Refusal("REFUSE_MANIFEST_COUNT", "file count mismatch")
    for record in files:
        verify_record(record)

    # Verify current source snapshot against the complete saved file manifest.
    source_rows = load_json("receipts/manifests/source-driver-inputs.json")
    if len(source_rows) != 565:
        raise Refusal("REFUSE_SOURCE_MANIFEST_COUNT", str(len(source_rows)))
    repository = ROOT.parents[3]
    for row in source_rows:
        path = pathlib.Path(row["path"])
        target = (repository / path).resolve()
        if not target.is_file():
            raise Refusal("REFUSE_SOURCE_MISSING", row["path"])
        if target.stat().st_size != row["size"] or digest(target) != row["sha256"]:
            raise Refusal("REFUSE_SOURCE_HASH", row["path"])
    expected_source_paths = {row["path"] for row in source_rows}
    source_roots = ["novac/src", "std/src", "nova-driver/src", "nova-driver-support/src"]
    actual_source_paths = {
        path.relative_to(repository).as_posix()
        for root in source_roots
        for path in (repository / root).rglob("*")
        if path.is_file()
    }
    if actual_source_paths != expected_source_paths:
        raise Refusal("REFUSE_SOURCE_INVENTORY_PATH_SET", "missing/extra source paths")

    native = load_json("receipts/manifests/native-input-inventory.json")
    runtime_rows = native["runtime_tree"]["file_records"]
    libuv_rows = native["libuv"]["files"]
    if len(runtime_rows) != 58 or len(libuv_rows) != 469:
        raise Refusal("REFUSE_NATIVE_INVENTORY_COUNT", "runtime/libuv count mismatch")
    for row in runtime_rows:
        target = repository / row["path"]
        if not target.is_file() or target.stat().st_size != row["size"] or digest(target) != row["sha256"]:
            raise Refusal("REFUSE_NATIVE_RUNTIME_HASH", row["path"])
    libuv_root = repository / native["libuv"]["task_worktree_path"]
    for row in libuv_rows:
        target = libuv_root / row["path"]
        if not target.is_file() or target.stat().st_size != row["size"] or digest(target) != row["sha256"]:
            raise Refusal("REFUSE_LIBUV_HASH", row["path"])

    # The omitted C/PE/seed bytes are deliberately not re-created from hashes.
    policy = bundle["owner_retention_policy"]
    if policy.get("byte_revalidation_after_scratch_loss", "").find("unavailable") < 0:
        raise Refusal("REFUSE_MISSING_BINARY_LIMITATION", "missing limitation note")

    direct = load_json("receipts/baseline/direct-probe-receipt.json")
    if direct["output"]["sha256"] != PINNED["generated_c_sha256"]:
        raise Refusal("REFUSE_DIRECT_C_RECEIPT", "direct probe hash mismatch")
    if direct["output"]["reuse_for_driver_or_native_build"] is not False:
        raise Refusal("REFUSE_C_REUSE", "direct diagnostic C was not a build input")

    build_audit = load_json("receipts/audits/post-same-volume-build-audit.json")
    if build_audit.get("verdict") != "PASS_PRODUCTION_BUILD_AND_PUBLICATION":
        raise Refusal("REFUSE_BUILD_AUDIT", str(build_audit.get("verdict")))
    build_checks = build_audit.get("checks", {})
    if (build_checks.get("all_46_old_outputs_preserved") is not True or
            build_checks.get("three_new_outputs") is not True or
            build_checks.get("cache_total_49_exact") is not True):
        raise Refusal("REFUSE_BUILD_CACHE_AUDIT", "46+3 cache receipt checks")
    if build_audit.get("native_sha256") != PINNED["native_exe_sha256"]:
        raise Refusal("REFUSE_NATIVE_EXE_AUDIT_HASH", "build audit pin mismatch")
    post_092 = build_audit.get("post_092", {})
    if post_092.get("count") != 1551 or post_092.get("missing") != [] or post_092.get("changed") != []:
        raise Refusal("REFUSE_092_POST_BUILD", "independent scoped input audit did not pass")

    cache_before = load_json("receipts/manifests/cache-before.json")
    if len(cache_before.get("files", [])) != 46:
        raise Refusal("REFUSE_CACHE_BASELINE_COUNT", "expected 46 prior cache files")
    smoke_audit = load_json("receipts/audits/post-native-smoke-audit.json")
    smoke_checks = smoke_audit.get("checks", {})
    if (smoke_audit.get("verdict") != "PASS_BOUNDED_NATIVE_CLI_SMOKE_3_OF_3" or
            smoke_checks.get("three_confirmed_starts_no_retries") is not True or
            smoke_checks.get("49_existing_cache_outputs_unchanged") is not True or
            smoke_checks.get("1551_092_pins_rehashed_after_smoke") is not True):
        raise Refusal("REFUSE_SMOKE_AUDIT", str(smoke_audit.get("verdict")))
    post_smoke_092 = smoke_audit.get("post_smoke_092", {})
    if post_smoke_092.get("count") != 1551 or post_smoke_092.get("missing") != [] or post_smoke_092.get("changed") != []:
        raise Refusal("REFUSE_092_POST_SMOKE", "independent scoped input audit did not pass")
    verify_smoke(bundle)

    return {
        "receipts": len(files),
        "source_inputs": len(source_rows),
        "smoke_cases": 3,
        "seed_assets_republished": False,
        "generated_C_bytes_included": False,
        "native_EXE_bytes_included": False,
        "oracle_or_native_launched_by_harness": False,
        "acceptance": "NOT CLAIMED",
    }


def expect_refusal(code, action):
    try:
        action()
    except Refusal as error:
        if error.code == code:
            return
        raise AssertionError("expected %s, got %s" % (code, error.code))
    raise AssertionError("expected refusal %s" % code)


def self_test():
    bundle = json.loads(MANIFEST_PATH.read_bytes())
    wrong_seed = json.loads(json.dumps(bundle))
    wrong_seed["pins"]["seed_exe_sha256"] = "0" * 64
    expect_refusal("REFUSE_WRONG_SEED_PIN", lambda: verify_pins(wrong_seed["pins"]))

    wrong_source = json.loads(json.dumps(bundle))
    wrong_source["pins"]["source_target_sha256"] = "0" * 64
    expect_refusal("REFUSE_WRONG_SOURCE_PIN", lambda: verify_pins(wrong_source["pins"]))

    wrong_hash = dict(bundle["files"][0])
    wrong_hash["sha256"] = "0" * 64
    expect_refusal("REFUSE_RECEIPT_HASH_MISMATCH", lambda: verify_record(wrong_hash))

    missing = "receipts/__self_test_missing_output__.bin"
    expect_refusal("REFUSE_OUTPUT_MISSING", lambda: require_output(safe_path(missing)))

    oracle = json.loads(json.dumps(bundle["pins"]))
    oracle["compiler_source_kind"] = "oracle"
    expect_refusal("REFUSE_ORACLE_COMPILER", lambda: verify_pins(oracle))
    return {"wrong_seed": "named refusal", "wrong_source": "named refusal", "wrong_hash": "named refusal", "missing_output": "named refusal", "oracle_impostor": "named refusal", "oracle_execution_count": 0}


def main(argv):
    if argv == ["verify"]:
        try:
            result = verify_bundle()
        except Refusal as error:
            print("REFUSED %s: %s" % (error.code, error.detail), file=sys.stderr)
            return 1
        print("OFFLINE RECEIPT VERIFY PASS: %s" % json.dumps(result, sort_keys=True))
        print("This verifies available receipts only; excluded C/PE/seed bytes and task acceptance are not claimed.")
        return 0
    if argv == ["self-test"]:
        print("OFFLINE REFUSAL SELF-TEST PASS: %s" % json.dumps(self_test(), sort_keys=True))
        return 0
    print("usage: python harness.py verify | self-test", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
