"""Replay task 137 emit-only minimizations using the preserved local 135 seed.

Arguments: session scratch, copied install135, verified combined novac.exe.
Run from the repository root through the machine queue. No Oracle is used.
This does not rebuild the seed and does not claim build+run acceptance.
"""
import os
import pathlib
import subprocess
import sys

scratch, install, seed = map(lambda p: pathlib.Path(p).resolve(), sys.argv[1:])
scratch.mkdir(parents=True, exist_ok=True)
original = pathlib.Path("examples/effects/serde_fs_build.nv").read_text(encoding="utf-8")
call = 'println(json_encode(Probe { a: n, name: "serde+fs" })!!)'
assert original.count(call) == 1
prefix = 'ro encoded = json_encode(Probe { a: n, name: "serde+fs" })\n'
cases = {
    "original": original,
    "plain": "fn main() { println(42) }\n",
    "bang": "fn main() { println(Some(42)!!) }\n",
    "option": "fn main() { println(Some(42) ?? 0) }\n",
    "with": "import std.fs.{real_fs}\nfn main() Fs Os -> () { with Fs = real_fs() { println(42) } }\n",
    "serde_without_bang": original.replace("})!!)", "}))"),
    "serde_throw": original.replace(call, prefix + 'ro text = match encoded { Ok(v) => v Err(e) => throw e }\nprintln(text)'),
    "serde_success_control": original.replace(call, prefix + 'ro text = match encoded { Ok(v) => v Err(e) => "error" }\nprintln(text)'),
}
env = dict(os.environ, NOVA_STD_PATH=str(install / "std/src"), NOVAC_SELF_PATH=str(install / "novac/src"))
for name, source in cases.items():
    entry = scratch / (name + ".nv")
    entry.write_text(source, encoding="utf-8")
    result = subprocess.run([str(seed), "emit", str(entry)], cwd=install, env=env, capture_output=True, timeout=60)
    (scratch / (name + ".stdout")).write_bytes(result.stdout)
    (scratch / (name + ".stderr")).write_bytes(result.stderr)
    print(f"{name} EMIT rc={result.returncode}", flush=True)
    if result.returncode:
        print((result.stdout + result.stderr).decode("utf-8", "replace"), flush=True)
