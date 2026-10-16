#!/usr/bin/env python3
"""End-to-end fail-closed probes for the sole Oracle #685 known-red."""

import base64
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[3]
GUARD = ROOT / "scripts/guards/check-novac-differential.sh"
HELPER = ROOT / "scripts/guards/novac-known-red-685.py"
RECORD = ROOT / "scripts/guards/novac-known-red-685.json"
FIXTURE = ROOT / "novac/fixtures/call_operands/pos_1.nv"
FIXTURE_REL = "novac/fixtures/call_operands/pos_1.nv"
RECORD_REL = "scripts/guards/novac-known-red-685.json"


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def run_guard(root, novac, mode="normal", platform=None):
    env = os.environ.copy()
    env.update({
        "NOVAC_POOL_JOBS": "1",
        "NOVAC_CORPUS": "0",
        "NOVAC_BIN": str(novac),
        "NOVAC_SMOKE": "1",
        "SMOKE_MODE": mode,
        "SMOKE_PLATFORM": platform or ("windows" if sys.platform == "win32" else "linux"),
        "EXPECTED_SIGNATURE": str(root / "expected.json"),
        "NOVAC_TEST_PYTHON": bash_path(pathlib.Path(sys.executable)),
    })
    if "NOVAC_SELFTEST_TMPDIR_MSYS" in env:
        env["TMPDIR"] = env["NOVAC_SELFTEST_TMPDIR_MSYS"]
    return subprocess.run(
        [os.environ.get("NOVAC_SELFTEST_BASH", "bash"), bash_path(GUARD), bash_path(root), bash_path(novac)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=env,
        check=False,
    )


def write_stub(path, text):
    path.write_text(text, encoding="utf-8", newline="\n")
    path.chmod(0o755)


def bash_path(path):
    if os.name != "nt":
        return path.as_posix()
    drive, tail = os.path.splitdrive(str(path))
    return "/" + drive[0].lower() + "/" + tail.lstrip("\\/").replace("\\", "/")


def main():
    require(RECORD.is_file(), "known-red record is required for this selftest")
    record = json.loads(RECORD.read_text(encoding="utf-8"))
    current = "windows" if sys.platform == "win32" else "linux" if sys.platform.startswith("linux") else None
    require(current is not None, "selftest expects Windows or Linux")
    current_pair = record["entries"][0]["platforms"].get(current)
    if current_pair is None:
        equal = {"exit": 0, "stdout_b64": base64.b64encode(b"same\n").decode("ascii"), "stderr_b64": ""}
        current_pair = {"oracle": equal, "carina": equal}

    with tempfile.TemporaryDirectory(prefix="novac-known-red-685-") as temp:
        root = pathlib.Path(temp)
        fixture = root / FIXTURE_REL
        fixture.parent.mkdir(parents=True)
        shutil.copyfile(FIXTURE, fixture)
        guards = root / "scripts/guards"
        guards.mkdir(parents=True)
        registry = root / "docs/plans/221.1-bug-sweep.md"
        registry.parent.mkdir(parents=True)
        registry.write_text("| 685 | open |\n", encoding="utf-8")
        shutil.copyfile(HELPER, guards / HELPER.name)
        tools = root / "scripts/tools"
        tools.mkdir(parents=True)
        oracle = root / "nova-cli/target/release/nova.exe"
        oracle.parent.mkdir(parents=True)
        write_stub(oracle, "#!/bin/sh\nexit 0\n")
        novac = root / "novac/target/novac.exe"
        novac.parent.mkdir(parents=True)
        write_stub(novac, "#!/bin/sh\nexit 0\n")
        manifest = guards / "novac-conformance.list"
        manifest.write_text("", encoding="utf-8")
        shutil.copyfile(RECORD, guards / RECORD.name)
        expected = root / "expected.json"
        expected.write_text(json.dumps(current_pair), encoding="utf-8")
        fake_smoke = tools / "novac-e1-smoke.sh"
        write_stub(fake_smoke, r'''#!/bin/sh
if [ "${1:-}" = --prepare ]; then exit 0; fi
case "${2:-}" in
  */pos_2.nv) exit 1 ;;
esac
if [ -n "${NOVAC_SMOKE_SIGNATURE_FILE:-}" ]; then
  "$NOVAC_TEST_PYTHON" - "$EXPECTED_SIGNATURE" "$NOVAC_SMOKE_SIGNATURE_FILE" "$SMOKE_MODE" <<'PY'
import base64, json, pathlib, sys
pair = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
if sys.argv[3] in ("mutate", "mutate-carina"):
    side = "oracle" if sys.argv[3] == "mutate" else "carina"
    raw = bytearray(base64.b64decode(pair[side]["stdout_b64"]))
    raw[0] ^= 1
    pair[side]["stdout_b64"] = base64.b64encode(raw).decode("ascii")
elif sys.argv[3] == "mutate-exit":
    pair["oracle"]["exit"] = (pair["oracle"]["exit"] + 1) % 256
elif sys.argv[3] == "mutate-stderr":
    pair["carina"]["stderr_b64"] = "eA=="
elif sys.argv[3] == "converged":
    pair["carina"] = pair["oracle"]
pathlib.Path(sys.argv[2]).write_text(json.dumps(pair), encoding="utf-8")
PY
  [ "$SMOKE_MODE" = mutate ] && exit 1
  [ "$SMOKE_MODE" = converged ] && exit 0
  [ "$SMOKE_PLATFORM" = windows ] && exit 1
  exit 0
fi
[ "$SMOKE_PLATFORM" = windows ] && [ "$SMOKE_MODE" != after-fix ] && exit 1
exit 0
''')

        # The exact pinned observation is visible as one KNOWN-RED on Windows;
        # Linux has the pinned equal-output pair and remains ordinary/green.
        exact = run_guard(root, novac)
        require(exact.returncode == 0, "restored exact signature must be accepted:\n" + exact.stdout)
        if current == "windows":
            require("KNOWN-RED №685 path=" + FIXTURE_REL in exact.stdout,
                    "Windows exact red must print the visible record:\n" + exact.stdout)
            require("KNOWN-RED count=1" in exact.stdout,
                    "known-red count must be visible and excluded from matches:\n" + exact.stdout)
        else:
            require("KNOWN-RED №685" not in exact.stdout,
                    "Linux no-divergence must not claim a known-red:\n" + exact.stdout)
            require("KNOWN-RED count=0" in exact.stdout,
                    "Linux no-divergence count must be visible:\n" + exact.stdout)

        # Sabotage one byte on Oracle stdout: exact matching must turn red.
        mutated = run_guard(root, novac, "mutate")
        require(mutated.returncode != 0 and "known-red signature mismatch" in mutated.stdout,
                "one-byte Oracle-output mutation must be RED:\n" + mutated.stdout)
        changed_carina = run_guard(root, novac, "mutate-carina")
        require(changed_carina.returncode != 0 and "known-red signature mismatch" in changed_carina.stdout,
                "one-byte Carina-output mutation must be RED:\n" + changed_carina.stdout)
        changed_exit = run_guard(root, novac, "mutate-exit")
        require(changed_exit.returncode != 0 and "known-red signature mismatch" in changed_exit.stdout,
                "Oracle exit-code mutation must be RED:\n" + changed_exit.stdout)
        changed_stderr = run_guard(root, novac, "mutate-stderr")
        require(changed_stderr.returncode != 0 and "known-red signature mismatch" in changed_stderr.stdout,
                "Carina stderr mutation must be RED:\n" + changed_stderr.stdout)

        # Restoring the pinned observation recovers exactly the platform result.
        restored = run_guard(root, novac)
        require(restored.returncode == 0, "restoring output must recover green:\n" + restored.stdout)
        if current == "windows":
            converged = run_guard(root, novac, "converged")
            require(converged.returncode != 0 and "known-red signature mismatch" in converged.stdout,
                    "a stale record after Oracle fix must be RED:\n" + converged.stdout)
            record_path = guards / RECORD.name
            pinned = record_path.read_bytes()
            record_path.unlink()
            unrecorded = run_guard(root, novac)
            require(unrecorded.returncode != 0,
                    "removing the record while Windows still diverges must be RED:\n" + unrecorded.stdout)
            after_fix = run_guard(root, novac, "after-fix")
            require(after_fix.returncode == 0 and "KNOWN-RED №685" not in after_fix.stdout,
                    "after removing the fixed record, pos_1 must be ordinary and green:\n" + after_fix.stdout)
            record_path.write_bytes(pinned)

        # A separate, unregistered differential remains a hard failure.
        other = root / "novac/fixtures/pos_2.nv"
        other.write_text("x\n", encoding="utf-8")
        unrelated = run_guard(root, novac, "other-divergence")
        require(unrelated.returncode != 0 and "pos_2.nv" in unrelated.stdout,
                "a different differential must remain RED:\n" + unrelated.stdout)
        other.unlink()

        record_path = guards / RECORD.name
        pristine_record = record_path.read_bytes()
        record_path.write_text("{broken", encoding="utf-8")
        corrupt = run_guard(root, novac)
        require(corrupt.returncode != 0 and "known-red №685" in corrupt.stdout,
                "corrupt record must be RED:\n" + corrupt.stdout)
        record_path.write_bytes(pristine_record)

        duplicate = json.loads(pristine_record)
        duplicate["entries"].append(duplicate["entries"][0])
        record_path.write_text(json.dumps(duplicate), encoding="utf-8")
        duplicated = run_guard(root, novac)
        require(duplicated.returncode != 0 and "exactly one" in duplicated.stdout,
                "duplicate record must be RED:\n" + duplicated.stdout)
        record_path.write_bytes(pristine_record)

        pinned_registry = registry.read_bytes()
        registry.write_text("", encoding="utf-8")
        broken_registry = run_guard(root, novac)
        require(broken_registry.returncode != 0 and "no №685 row" in broken_registry.stdout,
                "broken registry link target must be RED:\n" + broken_registry.stdout)
        registry.write_bytes(pinned_registry)

        raw_fixture = FIXTURE.read_bytes()
        fixture.write_bytes(raw_fixture.replace(b"\r\n", b"\n"))
        canonical_checkout = run_guard(root, novac)
        require(canonical_checkout.returncode == 0,
                "LF checkout equivalent to canonical content must stay green:\n" + canonical_checkout.stdout)

        mutated_source = bytearray(raw_fixture)
        mutated_source[mutated_source.index(b"call_operands")] ^= 1
        fixture.write_bytes(mutated_source)
        changed_fixture = run_guard(root, novac)
        require(changed_fixture.returncode != 0 and "fixture changed" in changed_fixture.stdout,
                "one-byte source-content mutation must be RED:\n" + changed_fixture.stdout)
        fixture.write_bytes(raw_fixture)
        fixture.unlink()
        missing_fixture = run_guard(root, novac)
        require(missing_fixture.returncode != 0 and "missing known-red fixture" in missing_fixture.stdout,
                "missing fixture must be RED:\n" + missing_fixture.stdout)
        record_path.unlink()
        manifest.write_text("", encoding="utf-8")
        empty = run_guard(root, novac)
        require(empty.returncode != 0 and "empty fixture selection" in empty.stdout,
                "empty corpus must be RED:\n" + empty.stdout)

        if current == "windows":
            for line in mutated.stdout.splitlines():
                if "FAIL — фикстура принята обоими" in line or "known-red signature mismatch" in line:
                    print("sabotage (one Oracle stdout byte changed): " + line)
            for line in restored.stdout.splitlines():
                if "KNOWN-RED №685" in line or "KNOWN-RED count=" in line:
                    print("restored exact signature: " + line)

    print("test-novac-known-red-685: ok — exact signature, one-byte sabotage, unrelated diff, record/fixture/corpus fail-closed")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (AssertionError, OSError, ValueError, KeyError, IndexError) as error:
        print("test-novac-known-red-685: FAIL — " + str(error), file=sys.stderr)
        sys.exit(1)
