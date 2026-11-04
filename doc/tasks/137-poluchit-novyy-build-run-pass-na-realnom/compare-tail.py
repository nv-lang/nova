"""Task 137 third candidate: copy postgres smoke, build, minimize tail loop.

Arguments: prepared session scratch, nova-postgres checkout at the 135 pin.
Use machine queue; never modifies the satellite checkout or its lockfiles.
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys

s, source = (pathlib.Path(p).resolve() for p in sys.argv[1:])
head = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
assert head == "8a46520269cf430e3f9d7a78ad9ae729ed23af8a", head
out = s / "tail-alternative-v2"
out.mkdir()
pkg = out / "postgres"
pkg.mkdir()
shutil.copytree(source / "src", pkg / "src")
shutil.copytree(source / "examples/smoke", pkg / "examples/smoke", ignore=shutil.ignore_patterns("target", ".nova"))
shutil.copy2(source / "nova.toml", pkg / "nova.toml")
# D420 requires a repository boundary for the example's local ../.. dependency.
# This isolated repository has no remote and no commits; the source checkout is untouched.
subprocess.run(["git", "-C", str(pkg), "init", "--quiet"], check=True)
i = s / "install135"
env = dict(os.environ, NOVA_STD_PATH=str(i / "std/src"), NOVAC_SELF_PATH=str(i / "novac/src"),
           NOVA_RT_DIR=str(i / "nova_rt"), NOVA_CG_INCLUDE=str(i),
           NOVA_CARINA_SEED_ZIP=str(s / "nova-carina-0.2.2-135combined-windows-x86_64.zip"),
           LOCALAPPDATA=str(s / "localappdata"))
seed = next((s / "localappdata/Nova/carina-seeds/windows-x86_64/unpacked").rglob("novac.exe"))
rows = []


def run(name, cmd, cwd):
    r = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, timeout=360)
    (out / (name + ".stdout")).write_bytes(r.stdout)
    (out / (name + ".stderr")).write_bytes(r.stderr)
    ds = []
    for line in (r.stdout+r.stderr).decode("utf-8", "replace").splitlines():
        k = line.find('{"id":')
        if k >= 0:
            ds.append(json.loads(line[k:]))
    rows.append(dict(name=name, rc=r.returncode, diagnostics=ds))
    print(name + " rc=" + str(r.returncode) + " diagnostics=" + str(len(ds)), flush=True)
    for d in ds[:3]:
        print(json.dumps(d, ensure_ascii=True), flush=True)


entry = pkg / "examples/smoke/src/main.nv"
run("smoke-build", [str(s / "nova135.exe"), "build", str(entry), "-o", str(out / "smoke.exe"),
                    "--toolchain", "clang", "--timeout", "300", "--keep-artifacts"], pkg / "examples/smoke")
probes = {
    "tail-loop": 'fn read_one() -> int { loop { return 42 } }\nfn main() { println(read_one()) }\n',
    "tail-while": 'fn read_one() -> int { while true { return 42 } }\nfn main() { println(read_one()) }\n',
    "tail-direct": 'fn read_one() -> int { return 42 }\nfn main() { println(read_one()) }\n',
}
for name, text in probes.items():
    p = out / (name + ".nv")
    p.write_text(text, encoding="utf-8")
    run(name + "-emit", [str(seed), "emit", str(p)], i)
(out / "results.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")
