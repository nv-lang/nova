#!/usr/bin/env python3
"""Selftest for check-carina-seed-drift.py (task 140).

Builds a tiny git repo per case: a base commit (the "seed's tree"), a drift
commit on top, a seed descriptor pinning the base revision, and a baseline
file. Cases: no drift, recorded drift, unrecorded drift, baseline row without
a task number, stale baseline row, shrunk drift, missing descriptor, unknown
revision. Each case asserts the guard's verdict both ways.
"""
import json
import pathlib
import subprocess
import sys
import tempfile

GUARD = pathlib.Path(__file__).resolve().parent.parent / "check-carina-seed-drift.py"

fails = 0


def ok(msg):
    print("  ok  " + msg)


def bad(msg):
    global fails
    fails += 1
    print("  FAIL: " + msg, file=sys.stderr)


def git(repo, *args):
    subprocess.run(["git", "-C", str(repo)] + list(args), capture_output=True, check=True)


def make_repo(drifts):
    """repo with a base commit; drift commits touching the given files."""
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="carina-seed-drift-selftest-"))
    repo = tmp / "repo"
    (repo / "novac/src").mkdir(parents=True)
    (repo / "std/src").mkdir(parents=True)
    (repo / "docs/dev/carina-seeds").mkdir(parents=True)
    (repo / "scripts/guards").mkdir(parents=True)
    (repo / "novac/src/a.nv").write_text("// a\n", encoding="utf-8")
    (repo / "std/src/b.nv").write_text("// b\n", encoding="utf-8")
    git(repo, "init", "-q", "-b", "main")
    git(repo, "config", "user.email", "selftest@example.com")
    git(repo, "config", "user.name", "selftest")
    git(repo, "add", "-A")
    git(repo, "commit", "-qm", "base")
    base = subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"],
                          capture_output=True, check=True).stdout.decode().strip()
    for rel in drifts:
        p = repo / rel
        p.write_text(p.read_text(encoding="utf-8") + "// drift\n", encoding="utf-8")
    if drifts:
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "drift")
    desc = {"source_revision": base}
    (repo / "docs/dev/carina-seeds/windows-x86_64.json").write_text(
        json.dumps(desc), encoding="utf-8")
    return repo


def write_baseline(repo, lines):
    (repo / "scripts/guards/carina-seed-drift.baseline").write_text(
        "\n".join(lines) + "\n", encoding="utf-8")


def run(repo):
    return subprocess.run([sys.executable, str(GUARD), str(repo)],
                          capture_output=True, text=True, encoding="utf-8", errors="replace")


# 1. no drift at all -> green, empty list
repo = make_repo([])
write_baseline(repo, [])
r = run(repo)
if r.returncode == 0 and "0 drifted" in r.stdout:
    ok("ноль дрейфа — зелёный")
else:
    bad("ноль дрейфа покраснел: %s" % (r.stderr or r.stdout))

# 2. drift recorded with a task -> green, list published
repo = make_repo(["novac/src/a.nv"])
write_baseline(repo, ["novac/src/a.nv # task 140"])
r = run(repo)
if r.returncode == 0 and "novac/src/a.nv # task 140" in r.stdout:
    ok("дрейф с номером задачи — зелёный, строка опубликована")
else:
    bad("записанный дрейф покраснел: %s" % (r.stderr or r.stdout))

# 3. drift without a baseline row -> red, file named
repo = make_repo(["std/src/b.nv"])
write_baseline(repo, [])
r = run(repo)
if r.returncode != 0 and "std/src/b.nv" in r.stderr and "without a task" in r.stderr:
    ok("дрейф без строки базы — красный, файл назван")
else:
    bad("дрейф без базы прошёл: rc=%d" % r.returncode)

# 4. baseline row without a task number -> red
repo = make_repo(["novac/src/a.nv"])
write_baseline(repo, ["novac/src/a.nv # просто так"])
r = run(repo)
if r.returncode != 0 and "task NNN" in r.stderr:
    ok("строка базы без номера задачи — красная")
else:
    bad("строка без номера прошла")

# 5. stale baseline row (file no longer drifts) -> red
repo = make_repo([])
write_baseline(repo, ["novac/src/a.nv # task 140"])
r = run(repo)
if r.returncode != 0 and "stale baseline row" in r.stderr:
    ok("протухшая строка базы — красная")
else:
    bad("протухшая строка прошла")

# 6. drift shrank but baseline keeps a second file -> red (both directions held)
repo = make_repo(["novac/src/a.nv"])
write_baseline(repo, ["novac/src/a.nv # task 140", "std/src/b.nv # task 140"])
r = run(repo)
if r.returncode != 0 and "std/src/b.nv" in r.stderr:
    ok("сужение дрейфа при широкой базе — красное")
else:
    bad("сужение при широкой базе прошло")

# 7. no descriptor -> red, fail-closed
repo = make_repo([])
(repo / "docs/dev/carina-seeds/windows-x86_64.json").unlink()
write_baseline(repo, [])
r = run(repo)
if r.returncode != 0 and "descriptor" in r.stderr:
    ok("нет дескриптора сида — красный, не «судить нечего»")
else:
    bad("отсутствие дескриптора прошло")

# 8. unknown revision (git objects absent) -> red, fail-closed
repo = make_repo([])
desc = {"source_revision": "0" * 40}
(repo / "docs/dev/carina-seeds/windows-x86_64.json").write_text(json.dumps(desc), encoding="utf-8")
write_baseline(repo, [])
r = run(repo)
if r.returncode != 0 and "cannot be measured" in r.stderr:
    ok("неизвестная ревизия сида — красная, не «судить нечего»")
else:
    bad("неизвестная ревизия прошла: rc=%d" % r.returncode)

if fails:
    print("test-check-carina-seed-drift FAIL: %d case(s)" % fails, file=sys.stderr)
    sys.exit(1)
print("test-check-carina-seed-drift ok: 8 cases, drift held in both directions")
