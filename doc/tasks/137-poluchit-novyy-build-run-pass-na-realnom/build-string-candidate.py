"""Build a LOCAL, unpublished Carina-only candidate from preserved task 135.

Run in this checkout through the machine queue:
  python <this-file> <prepared-session-scratch> <task135-preserved> green|sabotage

The scratch contains install135, nova135.exe and the verified local 135 ZIP.
Only task 137's diff is overlaid; copying whole newer compiler files would
accidentally reintroduce unrelated bootstrap paths omitted by the 135 backport.
"""
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys

s, preserved = (pathlib.Path(p).resolve() for p in sys.argv[1:3])
mode = sys.argv[3]
assert mode in ("green", "sabotage")
tree = s / "string-tree"
root = pathlib.Path.cwd()
base = "a4d03b307be91b4bb220bc66200dfa333275bf17"
if not tree.exists():
    shutil.copytree(preserved / "tree", tree, ignore=shutil.ignore_patterns("target", ".git", "fixtures"))
tracked = ["lex/lex.nv", "parse/expr.nv", "check/calls.nv", "check/binds.nv", "check/strings.nv",
           "check/literal_rules.nv", "sem/literal_spelling.nv", "emit_c/emit_requires.nv", "emit_c/emit_interp.nv"]
added = ["sem/backtick.nv", "check/tagged_args.nv"]
pattern = re.compile(r'(ice_at\(\s*)"([^"\n]+:\d+)"(,\s*"([^"\n]+)")')
for f in tracked:
    text = (preserved / "tree/novac/src" / f).read_text(encoding="utf-8")
    reference = subprocess.check_output(["git", "show", base + ":novac/src/" + f], cwd=root).decode("utf-8")
    pins = {m.group(4): m.group(2) for m in pattern.finditer(reference)}
    text = pattern.sub(lambda m: m.group(1) + '"' + pins.get(m.group(4), m.group(2)) + '"' + m.group(3), text)
    (tree / "novac/src" / f).write_text(text, encoding="utf-8", newline="\n")
patch = subprocess.check_output(["git", "diff", base, "--"] + ["novac/src/" + f for f in tracked], cwd=root)
(s / "string-backport.patch").write_bytes(patch)
subprocess.run(["git", "-C", str(tree), "apply", "--ignore-whitespace", "-"], input=patch, check=True)
for f in added:
    shutil.copy2(root / "novac/src" / f, tree / "novac/src" / f)
for f in tracked + added:
    p = tree / "novac/src" / f
    lines = p.read_text(encoding="utf-8").splitlines(keepends=True)
    for n, line in enumerate(lines, 1):
        lines[n - 1] = re.sub(r'"novac/src/[^"\n]+:\d+"', '"novac/src/' + f + ':' + str(n) + '"', line)
    p.write_text("".join(lines), encoding="utf-8", newline="\n")
if mode == "sabotage":
    p = tree / "novac/src/sem/backtick.nv"
    text = p.read_text(encoding="utf-8")
    old = 'if c == 0x0A { out.append("\\\\n") }'
    assert text.count(old) == 1
    p.write_text(text.replace(old, 'if c == 0x0A { out.append("\\\\t") }'), encoding="utf-8")
manifest = {str(p.relative_to(tree)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted((tree / "novac/src").rglob("*")) if p.is_file()}
(s / ("string-full-manifest-" + mode + ".json")).write_text(json.dumps(manifest, indent=2), encoding="utf-8")
i = s / "install135"
env = dict(os.environ, NOVA_RT_DIR=str(i / "nova_rt"), NOVA_CG_INCLUDE=str(i),
           NOVA_CARINA_SEED_ZIP=str(s / "nova-carina-0.2.2-135combined-windows-x86_64.zip"), LOCALAPPDATA=str(s / "localappdata"))
exe = s / ("novac137.exe" if mode == "green" else "novac137-sabotage.exe")
cmd = [str(s / "nova135.exe"), "build", str(tree / "novac/src/main.nv"), "-o", str(exe),
       "--toolchain", "clang", "--timeout", "300", "--keep-artifacts"]
(s / ("candidate-build-command-" + mode + ".json")).write_text(json.dumps(cmd, ensure_ascii=False), encoding="utf-8")
print("Building local candidate through combined Carina 135", flush=True)
r = subprocess.run(cmd, env=env, capture_output=True, timeout=480)
(s / ("string-build-" + mode + ".stdout")).write_bytes(r.stdout)
(s / ("string-build-" + mode + ".stderr")).write_bytes(r.stderr)
print("candidate BUILD rc=" + str(r.returncode), flush=True)
print((r.stdout + r.stderr).decode("utf-8", "replace")[-12000:], flush=True)
if r.returncode == 0:
    print("candidate EXE sha256=" + hashlib.sha256(exe.read_bytes()).hexdigest(), flush=True)
sys.exit(r.returncode)
