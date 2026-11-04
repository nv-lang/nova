"""Task 137: two existing entries and isolated emit probes, no source rewrites.

Run from repository root. Argument: prepared session scratch containing
install135, nova135.exe, the verified combined ZIP and isolated seed cache.
Use the machine queue. Emit success is not build+run success.
"""
import json
import os
import pathlib
import subprocess
import sys

s = pathlib.Path(sys.argv[1]).resolve()
i = s / "install135"
out = s / "alternatives"
out.mkdir(exist_ok=True)
seed = next((s / "localappdata/Nova/carina-seeds/windows-x86_64/unpacked").rglob("novac.exe"))
env = dict(os.environ, NOVA_STD_PATH=str(i / "std/src"), NOVAC_SELF_PATH=str(i / "novac/src"),
           NOVA_RT_DIR=str(i / "nova_rt"), NOVA_CG_INCLUDE=str(i),
           NOVA_CARINA_SEED_ZIP=str(s / "nova-carina-0.2.2-135combined-windows-x86_64.zip"),
           LOCALAPPDATA=str(s / "localappdata"))
rows = []


def run(name, command, cwd):
    r = subprocess.run(command, cwd=cwd, env=env, capture_output=True, timeout=360)
    (out / (name + ".stdout")).write_bytes(r.stdout)
    (out / (name + ".stderr")).write_bytes(r.stderr)
    diagnostics = []
    for line in (r.stdout + r.stderr).decode("utf-8", "replace").splitlines():
        start = line.find('{"id":')
        if start >= 0:
            d = json.loads(line[start:])
            diagnostics.append({k: d[k] for k in ("code", "primary", "message")})
    row = dict(name=name, rc=r.returncode, diagnostics=diagnostics)
    rows.append(row)
    print(json.dumps(row, ensure_ascii=True), flush=True)
    return r


for name, entry in [("strings-build", "examples/basics/string_literals.nv"),
                    ("collections-build", "examples/tour/collections_tour.nv")]:
    exe = out / (name + ".exe")
    if exe.exists():
        exe.unlink()
    r = run(name, [str(s / "nova135.exe"), "build", str(pathlib.Path(entry).resolve()),
                   "-o", str(exe), "--toolchain", "clang", "--timeout", "300"], pathlib.Path.cwd())
    if r.returncode == 0:
        run(name + "-run", [str(exe)], out)

probes = {
    "quoted": 'fn main() { println("hello") }\n',
    "continuation": 'fn main() { println("one\\\n    line") }\n',
    "backtick": 'fn main() { println(`hello`) }\n',
    "tagged": 'fn tag(parts []str, args []str) -> str { parts[0] }\nfn main() { println(tag`hello`) }\n',
    "tag-direct": 'fn tag(parts []str, args []str) -> str { parts[0] }\nfn main() { println(tag([]str.of("hello"), []str.new())) }\n',
    "lambda": 'fn main() { ro f = |x| x + 1\nprintln(f(1)) }\n',
    "map": 'import std.collections.hash_map.{HashMap}\nfn main() { ro m HashMap[str, int] = ["a": 1]\nprintln(m.len()) }\n',
    "builder": 'fn tag(parts []str, args []str) -> str { consume sb = StringBuilder.new()\nfor p in parts { sb.append(p) }\nsb }\nfn main() { println(tag([]str.of("hello"), []str.new())) }\n',
}
for name, src in probes.items():
    p = out / (name + ".nv")
    p.write_text(src, encoding="utf-8")
    run(name + "-emit", [str(seed), "emit", str(p)], i)
(out / "results.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")
