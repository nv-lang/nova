#!/usr/bin/env python3
"""Selftest for check-carina-gate-ci.py: builds a minimal fake tree per case."""
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
GUARD = ROOT / "scripts/guards/check-carina-gate-ci.py"

PIN_SRC = "a" * 40
PIN_EXE = "b" * 64

GATE_NOVAC = """#!/usr/bin/env bash
par_add "$ROOT/scripts/guards/check-aaa-one.py" "m1"
guard "$ROOT/scripts/guards/check-bbb-two.sh" "$ROOT" || fail "x"
guard --deadline 300 "$ROOT/scripts/guards/check-ccc-three.py" "$ROOT" || fail "y"
par_add "$ROOT/scripts/guards/check-ddd-four.sh" "m4"
"""

GATE_CARINA = """#!/usr/bin/env bash
EXCLUDED_ORACLE="
check-ddd-four.sh
"
EXCLUDED_CORPUS="
check-ccc-three.py
"
EXCLUDED_FULL="
"
PY_GUARDS="
check-aaa-one.py
"
SH_GUARDS="
check-bbb-two.sh
"
"""

CARINA_GATE_YML = """name: carina-gate
on: push
jobs:
  carina-gate:
    steps:
      - run: python scripts/tools/carina-native-build.py fetch-seed
      - run: python scripts/tools/carina-native-build.py emit
      - run: python scripts/tools/carina-native-build.py libs
      - run: python scripts/tools/carina-native-build.py link
      - run: python scripts/tools/carina-native-build.py smoke
      - run: bash scripts/gate-carina.sh .
"""

ORACLE_COMPAT_YML = """name: carina-oracle-compat
on: push
jobs:
  compat:
    steps:
      - run: bash scripts/guards/check-ddd-four.sh .
"""

NATIVE_BUILD = 'SEED_SOURCE = "%s"\nSEED_EXE_SHA256 = "%s"\n' % (PIN_SRC, PIN_EXE)
CARINA_RS = 'pub const SEED_SOURCE: &str = "%s";\npub const SEED_EXE_SHA256: &str = "%s";\n' % (PIN_SRC, PIN_EXE)

GUARD_FILES = ["check-aaa-one.py", "check-bbb-two.sh", "check-ccc-three.py", "check-ddd-four.sh"]


def build_tree(t: pathlib.Path) -> dict[str, pathlib.Path]:
    files = {
        "scripts/gate-novac.sh": GATE_NOVAC,
        "scripts/gate-carina.sh": GATE_CARINA,
        ".github/workflows/carina-gate.yml": CARINA_GATE_YML,
        ".github/workflows/carina-oracle-compat.yml": ORACLE_COMPAT_YML,
        "scripts/tools/carina-native-build.py": NATIVE_BUILD,
        "nova-driver/src/carina_build.rs": CARINA_RS,
    }
    for g in GUARD_FILES:
        files["scripts/guards/" + g] = "# fixture\n"
    out = {}
    for rel, text in files.items():
        p = t / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8", newline="\n")
        out[rel] = p
    return out


def call(root: pathlib.Path):
    return subprocess.run([sys.executable, str(GUARD), str(root)],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def main():
    with tempfile.TemporaryDirectory(prefix="carina-gate-ci-selftest-") as td:
        t = pathlib.Path(td)
        f = build_tree(t)

        p = call(t)
        assert p.returncode == 0 and "ok:" in p.stdout, (p.stdout, p.stderr)
        print("PROBE baseline GREEN: " + p.stdout.strip())

        # A guard invoked in gate-novac.sh but dropped from both lists: RED.
        f["scripts/gate-carina.sh"].write_text(GATE_CARINA.replace("check-bbb-two.sh\n", ""),
                                               encoding="utf-8", newline="\n")
        p = call(t)
        assert p.returncode != 0 and "neither run nor excluded" in p.stderr, (p.stdout, p.stderr)
        print("PROBE unclassified guard RED: " + p.stderr.strip().splitlines()[1].strip())

        # An EXCLUDED_ORACLE guard not invoked in carina-oracle-compat.yml: RED.
        f["scripts/gate-carina.sh"].write_text(GATE_CARINA, encoding="utf-8", newline="\n")
        f[".github/workflows/carina-oracle-compat.yml"].write_text("name: x\n", encoding="utf-8", newline="\n")
        p = call(t)
        assert p.returncode != 0 and "moved IN here" in p.stderr, (p.stdout, p.stderr)
        print("PROBE oracle guard not moved RED: " + p.stderr.strip().splitlines()[1].strip())

        # Seed pins diverged between the Python tool and the Rust driver: RED.
        f[".github/workflows/carina-oracle-compat.yml"].write_text(ORACLE_COMPAT_YML,
                                                                   encoding="utf-8", newline="\n")
        f["nova-driver/src/carina_build.rs"].write_text(CARINA_RS.replace(PIN_SRC, "c" * 40),
                                                        encoding="utf-8", newline="\n")
        p = call(t)
        assert p.returncode != 0 and "diverged" in p.stderr, (p.stdout, p.stderr)
        print("PROBE pin divergence RED: " + p.stderr.strip().splitlines()[1].strip())

        # An Oracle invocation in an active line of carina-gate.yml: RED.
        f["nova-driver/src/carina_build.rs"].write_text(CARINA_RS, encoding="utf-8", newline="\n")
        f[".github/workflows/carina-gate.yml"].write_text(CARINA_GATE_YML + "      - run: cargo build --release\n",
                                                          encoding="utf-8", newline="\n")
        p = call(t)
        assert p.returncode != 0 and "cargo" in p.stderr, (p.stdout, p.stderr)
        print("PROBE cargo in carina-gate RED: " + p.stderr.strip().splitlines()[1].strip())

        # A required bootstrap token missing from carina-gate.yml: RED.
        f[".github/workflows/carina-gate.yml"].write_text(CARINA_GATE_YML.replace(" smoke", ""),
                                                          encoding="utf-8", newline="\n")
        p = call(t)
        assert p.returncode != 0 and "required token 'smoke'" in p.stderr, (p.stdout, p.stderr)
        print("PROBE missing smoke RED: " + p.stderr.strip().splitlines()[1].strip())

        # A stale exclusion (not invoked in gate-novac.sh): RED.
        f[".github/workflows/carina-gate.yml"].write_text(CARINA_GATE_YML, encoding="utf-8", newline="\n")
        f["scripts/gate-carina.sh"].write_text(GATE_CARINA.replace('EXCLUDED_FULL="\n"',
                                                                   'EXCLUDED_FULL="\ncheck-zzz-nine.sh\n"'),
                                               encoding="utf-8", newline="\n")
        (t / "scripts/guards/check-zzz-nine.sh").write_text("# fixture\n", encoding="utf-8")
        p = call(t)
        assert p.returncode != 0 and "stale exclusion" in p.stderr, (p.stdout, p.stderr)
        print("PROBE stale exclusion RED: " + p.stderr.strip().splitlines()[1].strip())

        # Restored: GREEN again.
        f["scripts/gate-carina.sh"].write_text(GATE_CARINA, encoding="utf-8", newline="\n")
        p = call(t)
        assert p.returncode == 0 and "ok:" in p.stdout, (p.stdout, p.stderr)
        print("PROBE restored GREEN: " + p.stdout.strip())

    print("test-check-carina-gate-ci.py PASS: partition, move, pin and oracle-form sabotage probes")


if __name__ == "__main__":
    main()
