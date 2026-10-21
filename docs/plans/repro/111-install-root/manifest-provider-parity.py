"""Compare compatibility add outcomes, including exact manifest and lock bytes.

This is a parity probe, NOT a claim that stale snapshot success is correct.
Use --work under the caller's session scratchpad. No compiler builds are run.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    work = Path(tempfile.mkdtemp(prefix="manifest-provider-parity-", dir=args.work))
    binaries = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
    records = []
    for scenario in ("external_flag", "external_no_flag", "internal"):
        app = work / scenario / "app"
        dep = app / "extlib" if scenario == "internal" else app.parent / "extlib"
        for pkg, name in ((app, "app"), (dep, "extlib")):
            pkg.mkdir(parents=True)
            if pkg == app or scenario != "internal":
                subprocess.run(["git", "-C", str(pkg), "init", "--quiet"], check=True)
            (pkg / "nova.toml").write_bytes(
                f'[package]\nname = "{name}"\n[lib]\nsrc = "."\n'.encode())
        manifest = app / "nova.toml"
        initial = manifest.read_bytes()
        lock = app / "nova.lock.toml"
        argv = ["--color", "never", "add", "extlib", "--path",
                "extlib" if scenario == "internal" else "../extlib"]
        if scenario == "external_flag":
            argv.append("--allow-external-path")
        for existing in (False, True):
            old_lock = (b'version = 1\n\n[[package]]\nname = "prior"\n'
                        b'source = "path"\npath = "prior"\n') if existing else None
            for snapshot in ("default", "disabled"):
                outcomes = []
                for label, binary in binaries.items():
                    manifest.write_bytes(initial)
                    if old_lock is None:
                        lock.unlink(missing_ok=True)
                    else:
                        lock.write_bytes(old_lock)
                    env = os.environ.copy()
                    env.pop("NOVA_SRC_INDEX", None)
                    if snapshot == "disabled":
                        env["NOVA_SRC_INDEX"] = "0"
                    result = subprocess.run([str(binary), *argv], cwd=app, env=env,
                                            capture_output=True, check=False)
                    outcome = {
                        "exit": result.returncode,
                        "stdout_hex": result.stdout.hex(), "stderr_hex": result.stderr.hex(),
                        "manifest_hex": manifest.read_bytes().hex(),
                        "lock_exists": lock.is_file(),
                        "lock_hex": lock.read_bytes().hex() if lock.is_file() else None,
                    }
                    outcomes.append(outcome)
                    records.append({"binary": label, "binary_sha256": sha256(binary.read_bytes()),
                                    "argv": argv, "cwd": str(app), "scenario": scenario,
                                    "existing_lock": existing, "snapshot": snapshot,
                                    "initial_manifest_hex": initial.hex(),
                                    "initial_lock_hex": old_lock.hex() if old_lock else None,
                                    **outcome})
                # Capture evidence before checking parity, including on a failing candidate.
                receipt = work / "receipts.json"
                receipt.write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
                assert outcomes[0] == outcomes[1], (scenario, existing, snapshot, outcomes)
                current = outcomes[1]
                if scenario == "external_no_flag":
                    assert current["exit"] == 2 and current["manifest_hex"] == initial.hex()
                    assert current["lock_hex"] == (old_lock.hex() if old_lock else None)
                elif scenario == "external_flag" and snapshot == "disabled":
                    assert current["exit"] == 1
                    assert b"E_DEP_PATH_OUTSIDE_REPO" in bytes.fromhex(current["stderr_hex"])
                    assert current["lock_hex"] == (old_lock.hex() if old_lock else None)
                else:
                    assert current["exit"] == 0
                    if snapshot == "default" and not existing:
                        # Explicitly expose, not hide, the known stale-success/no-lock defect.
                        assert not current["lock_exists"]
                    if scenario == "internal" and snapshot == "disabled":
                        assert current["lock_exists"]
                        assert b'name = "extlib"' in bytes.fromhex(current["lock_hex"])
                print(f"{scenario} existing={existing} snapshot={snapshot}: "
                      f"parity exit={current['exit']} lock_exists={current['lock_exists']}", flush=True)
    print(f"receipt: {receipt}\nreceipt_sha256: {sha256(receipt.read_bytes())}")


if __name__ == "__main__":
    main()
