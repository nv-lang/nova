"""Compare exact negative inputs on 135 and the final local candidate."""
import hashlib
import json
import os
import pathlib
import subprocess
import sys

s = pathlib.Path(sys.argv[1]).resolve()
run = s / sys.argv[2]
out = s / "final-refusal-comparison"
out.mkdir()
i = s / "install135"
env = dict(os.environ, NOVA_STD_PATH=str(i / "std/src"), NOVAC_SELF_PATH=str(i / "novac/src"))
baseline = next((s / "localappdata/Nova/carina-seeds/windows-x86_64/unpacked").rglob("novac.exe"))
rows = []
for name in ("live", "tag-live", "unclosed", "escaped-close", "generic-tag", "overloaded-tag"):
    source = run / (name + ".nv")
    row = dict(name=name, source_sha256=hashlib.sha256(source.read_bytes()).hexdigest())
    for phase, executable in (("before", baseline), ("after", s / "novac137.exe")):
        r = subprocess.run([str(executable), "emit", str(source)], cwd=i, env=env, capture_output=True, timeout=60)
        (out / (name + "-" + phase + ".stdout")).write_bytes(r.stdout)
        (out / (name + "-" + phase + ".stderr")).write_bytes(r.stderr)
        ds = [json.loads(line) for line in r.stdout.decode("utf-8", "replace").splitlines() if line.startswith('{"id":')]
        row[phase] = dict(rc=r.returncode, diagnostics=ds)
        print(name + " " + phase + " rc=" + str(r.returncode) + " diagnostics=" + str(len(ds)), flush=True)
        assert r.returncode == 1 and ds
    rows.append(row)
(out / "before-after.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8")
