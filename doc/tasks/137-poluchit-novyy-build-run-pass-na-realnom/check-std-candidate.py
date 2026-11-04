"""Carina batch equivalent of directory check; the seed CLI takes file lists."""
import json
import os
import pathlib
import subprocess
import sys

s = pathlib.Path(sys.argv[1]).resolve()
root = pathlib.Path.cwd()
out = s / "std-batch-comparison"
out.mkdir()
std = root / "std/src"
files = sorted(str(p.relative_to(std)) for p in std.rglob("*.nv"))
baseline = next((s / "localappdata/Nova/carina-seeds/windows-x86_64/unpacked").rglob("novac.exe"))
env = dict(os.environ, NOVA_STD_PATH=str(s / "install135/std/src"), NOVAC_SELF_PATH=str(s / "install135/novac/src"))
records = {}
for phase, executable in [("before", baseline), ("after", s / "novac137.exe")]:
    command = [str(executable), "check"] + files
    (out / (phase + "-command.json")).write_text(json.dumps(dict(cwd=str(std), argv=command), ensure_ascii=False, indent=2), encoding="utf-8")
    print("std " + phase + " batch start: " + str(len(files)) + " files", flush=True)
    r = subprocess.run(command, cwd=std, env=env, capture_output=True, timeout=300)
    (out / (phase + ".stdout")).write_bytes(r.stdout)
    (out / (phase + ".stderr")).write_bytes(r.stderr)
    diagnostics = []
    for line in r.stdout.decode("utf-8", "replace").splitlines():
        if line.startswith('{"id":'):
            d = json.loads(line)
            diagnostics.append({key: d[key] for key in ("code", "primary", "message")})
    records[phase] = dict(rc=r.returncode, diagnostics=diagnostics)
    print("std " + phase + " " + ("PASS" if r.returncode == 0 else "FAIL") + " rc=" + str(r.returncode) + " diagnostics=" + str(len(diagnostics)), flush=True)
    assert r.returncode in (0, 1), "std check did not reach a normal verdict"
key = lambda d: json.dumps(d, sort_keys=True, ensure_ascii=False)
before = {key(d) for d in records["before"]["diagnostics"]}
after = {key(d) for d in records["after"]["diagnostics"]}
records["added"] = [json.loads(v) for v in sorted(after - before)]
records["removed"] = [json.loads(v) for v in sorted(before - after)]
(out / "comparison.json").write_text(json.dumps(records, ensure_ascii=False, indent=2), encoding="utf-8")
print("std diagnostic delta: added=" + str(len(after-before)) + " removed=" + str(len(before-after)), flush=True)
