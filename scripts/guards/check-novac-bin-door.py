#!/usr/bin/env python3
"""check-novac-bin-door.py -- Carina's binary is chosen by the door, never by a file name.

Registry 221.1 #1607 (docs/plans/221.1-bug-sweep.md). Twenty places in scripts/ chose the
self-hosted compiler's binary by writing its path -- most of them `novac/target/novac.exe`.
A Linux build writes `novac`, so in a clone that also held an old `novac.exe` the stale file
won by name: the cloud session p274-carina-k1 measured a Carina that no longer existed
(2026-10-02: 140/161 and 124/144 taken on the old binary; 15 false reds in
check-novac-no-cascade and check-novac-diag-schema).

THE RULE: the path is asked of the door -- `novac_bin ROOT` (the binary to run: NOVAC from
the caller, else the newer of the two files) and `novac_bin_out ROOT` (where to build), in
scripts/guards/lib/novac.sh, and `novac_bin(root)` in scripts/guards/lib/novac_bin.py.
Outside those two files a code line naming `novac/target/novac`, `novac.exe` or the parts
`"novac" / "target"` is a FAIL.

DOES NOT JUDGE: comments; the self-tests under scripts/guards/selftest/ (they BUILD fixture
trees with such files); a line that carries `novac-bin: not a selection -- <why>` (data in a
probe or a process matcher, not a choice of binary).

Usage: check-novac-bin-door.py ROOT   (exit 1 with every offending line)
"""
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

DOORS = {"scripts/guards/lib/novac.sh", "scripts/guards/lib/novac_bin.py"}
FORM = re.compile(r"novac/target/novac|novac\.exe|\"novac\"\s*/\s*\"target\"")
EXEMPT = "novac-bin: not a selection"


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    bad = []
    for dp, _, files in os.walk(os.path.join(root, "scripts")):
        rel_dir = os.path.relpath(dp, root).replace("\\", "/")
        if rel_dir.startswith("scripts/guards/selftest"):
            continue
        for f in files:
            if not f.endswith((".sh", ".py")):
                continue
            rel = f"{rel_dir}/{f}"
            if rel in DOORS or rel == "scripts/guards/check-novac-bin-door.py":
                continue
            for n, line in enumerate(open(os.path.join(dp, f), encoding="utf-8", errors="replace"), 1):
                code = line.strip()
                if code.startswith("#") or EXEMPT in line:
                    continue
                if FORM.search(line):
                    bad.append(f"{rel}:{n}: names Carina's binary by its file -- ask the door "
                               f"(novac_bin / novac_bin_out, scripts/guards/lib/novac.sh): {code[:100]}")
    if bad:
        for b in bad:
            print(b, file=sys.stderr)
        print(f"check-novac-bin-door FAIL: {len(bad)} line(s) (#1607)", file=sys.stderr)
        sys.exit(1)
    print("check-novac-bin-door ok: Carina's binary is chosen only by the door")


if __name__ == "__main__":
    main()
