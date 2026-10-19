#!/usr/bin/env python3
"""Sabotage proof for the Carina Oracle-debt guard."""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
GUARD = ROOT / "scripts/guards/check-carina-no-oracle-build.py"


def call(root):
    return subprocess.run([sys.executable, str(GUARD), str(root)],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                          encoding="utf-8")


def main():
    scratch = os.environ.get("NOVA_TEST_TMPDIR") or os.environ.get("TMPDIR")
    with tempfile.TemporaryDirectory(prefix="carina-oracle-guard-", dir=scratch) as td:
        root = pathlib.Path(td)
        gate = root / "scripts/gate-novac.sh"
        gate.parent.mkdir(parents=True)
        legacy = '"$NOVA_BIN" build novac/src/main.nv -o novac/target/novac'
        gate.write_text(legacy + "\n", encoding="utf-8")
        row = {
            "path": "scripts/gate-novac.sh",
            "pattern": "oracle-build-novac",
            "count": 1,
            "lines_sha256": hashlib.sha256(legacy.encode("utf-8")).hexdigest(),
            "reason": "synthetic known legacy path for selftest only",
            "due": "remove before promotion",
        }
        debt = root / "scripts/guards/carina-oracle-debt.json"
        debt.parent.mkdir(parents=True)
        debt.write_text(json.dumps([row]), encoding="utf-8")

        p = call(root)
        assert p.returncode == 0 and "new matches=0" in p.stdout, (p.stdout, p.stderr)
        gate.write_text(legacy + '\n"$ORACLE" build novac/src/main.nv -o novac/target/novac\n', encoding="utf-8")
        p = call(root)
        assert p.returncode != 0 and "NEW/CHANGED" in p.stderr and "novac/src/main.nv" in p.stderr, (p.stdout, p.stderr)
        print("PROBE inserted Oracle build RED: " + p.stderr.splitlines()[1].strip())
        gate.write_text(legacy + "\n", encoding="utf-8")
        p = call(root)
        assert p.returncode == 0 and "new matches=0" in p.stdout, (p.stdout, p.stderr)
        print("PROBE Oracle build removed GREEN: " + p.stdout.strip())

        # Empty ledger is the retirement state, not an unchecked bypass:
        # no hits => GREEN; restore one active Oracle build => RED.
        gate.write_text("\n", encoding="utf-8")
        debt.write_text("[]\n", encoding="utf-8")
        p = call(root)
        assert p.returncode == 0 and "legacy Oracle debt matches=0" in p.stdout, (p.stdout, p.stderr)
        print("PROBE empty debt with no Oracle use GREEN: " + p.stdout.strip())
        debt.unlink()
        p = call(root)
        assert p.returncode != 0 and "missing debt ledger" in p.stderr, (p.stdout, p.stderr)
        print("PROBE missing debt ledger RED: " + p.stderr.splitlines()[1].strip())
        debt.write_text("[]\n", encoding="utf-8")
        gate.write_text('"$ORACLE" build novac/src/main.nv -o novac/target/novac\n', encoding="utf-8")
        p = call(root)
        assert p.returncode != 0 and "NEW" in p.stderr, (p.stdout, p.stderr)
        print("PROBE empty debt with new Oracle build RED: " + p.stderr.splitlines()[1].strip())

    print("test-check-carina-no-oracle-build PASS")


if __name__ == "__main__":
    main()
