"""Targeted Carina-only native acceptance; no seed pin changes.

Arguments: session scratch, candidate exe, output directory name.
The supplied candidate emits C; clang links the already built 135 runtime.
Subset refusal probes are generated in scratch, not language-negative tests:
live slots are valid Nova and only this candidate's unsupported boundary.
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys

s = pathlib.Path(sys.argv[1]).resolve()
candidate = pathlib.Path(sys.argv[2]).resolve()
out = s / sys.argv[3]
out.mkdir()
i = s / "install135"
env = dict(os.environ, NOVA_STD_PATH=str(i / "std/src"), NOVAC_SELF_PATH=str(i / "novac/src"))
clang = shutil.which("clang") or r"C:\Program Files\LLVM\bin\clang.exe"
rt = next((i / "target/rt-archive-cache").rglob("libnova_rt.lib"))
uv = next((i / "target/libuv-cache").rglob("libuv.lib"))
gc = i / "target/gc-cache/gc.lib"
results = []


def invoke(label, cmd, timeout=90):
    r = subprocess.run([str(a) for a in cmd], cwd=i, env=env, capture_output=True, timeout=timeout)
    (out / (label + ".stdout")).write_bytes(r.stdout)
    (out / (label + ".stderr")).write_bytes(r.stderr)
    print(label + " rc=" + str(r.returncode), flush=True)
    return r


def build_run(label, entry, expected):
    r = invoke(label + "-emit", [candidate, "emit", entry])
    if r.returncode:
        print((r.stdout+r.stderr).decode("utf-8", "replace")[-5000:], flush=True)
        results.append(dict(name=label, stage="emit", passed=False))
        return
    assert r.stdout.lstrip().startswith(b"/*"), "emit returned no C"
    c = out / (label + ".c")
    c.write_bytes(r.stdout)
    exe = out / (label + ".exe")
    cmd = [clang, "--target=x86_64-pc-windows-msvc", "-fuse-ld=lld", "-O0", "-Wno-everything",
           "-ffunction-sections", "-fdata-sections", "-Wl,/stack:0x1000000",
           "-DNOVA_GC_BOEHM", "-DGC_THREADS", "-DNOVA_USE_LIBUV=1",
           "-I", i, "-I", i / "nova_rt/libuv/include", "-I", i / "target/gc-cache/include",
           c, rt, uv, gc, "-o", exe]
    cmd += ["-l" + lib for lib in ["ws2_32", "iphlpapi", "psapi", "userenv", "user32", "shell32", "ole32", "uuid", "advapi32", "dbghelp"]]
    (out / (label + "-native-command.json")).write_text(json.dumps([str(a) for a in cmd], ensure_ascii=False), encoding="utf-8")
    r = invoke(label + "-native", cmd)
    if r.returncode:
        print(r.stderr.decode("utf-8", "replace")[-5000:], flush=True)
        results.append(dict(name=label, stage="native", passed=False))
        return
    r = invoke(label + "-run", [exe], timeout=30)
    actual = r.stdout.decode("utf-8").replace("\r\n", "\n")
    ok = r.returncode == 0 and actual == expected
    print(label + " BUILD+RUN " + ("PASS" if ok else "FAIL") + " stdout=" + repr(actual), flush=True)
    results.append(dict(name=label, stage="run", passed=ok, stdout=actual,
                        c_sha256=hashlib.sha256(c.read_bytes()).hexdigest(),
                        exe_sha256=hashlib.sha256(exe.read_bytes()).hexdigest()))


root = pathlib.Path.cwd()
if "--fixture-only" in sys.argv:
    build_run("fixture", root / "spec_tests/conformance/standalone/carina_text_backtick_tag.nv",
              "tag:hello\nint:world\nbacktick-ok\n")
    (out / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
    sys.exit(0 if all(r["passed"] for r in results) else 1)
build_run("real", root / "examples/basics/string_literals.nv",
          "hello Nova\none long line, written across two source lines\nfirst\nsecond\nthird\na tagged literal\ncost: $ and more\nshell: $HOME plain\nC:\\new\\table\ntick ` slash \\ dollar ${name}\n")
fixture = root / "spec_tests/conformance/standalone/carina_text_backtick_tag.nv"
expected = "tag:hello\nint:world\nbacktick-ok\n"
build_run("fixture", fixture, expected)
crlf = out / "fixture-crlf.nv"
crlf.write_bytes(fixture.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
build_run("fixture-crlf", crlf, expected)
for name, expected_stdout in [
    ("str_eq", "true\nfalse\ntrue\n"),
    ("strings", "len = 12\nupper = HELLO, WORLD\nstarts with 'hello' = true\ncontains 'world' = true\n"),
    ("requires_string_guard", "1\n"),
    ("interp_display", "p Pt end\nPtPt\n"),
]:
    build_run("previous-" + name, root / ("examples/basics/" + name + ".nv"), expected_stdout)
negative = {
    "live": ('fn main() { ro x = 42\nprintln(`value=${x}`) }\n', "live interpolation slots"),
    "tag-live": ('fn tag(parts []str, args []str) -> str { parts[0] }\nfn main() { ro x = "v"\nprintln(tag`value=${x}`) }\n', "live interpolation slots"),
    "unclosed": ('fn main() { println(`unclosed) }\n', "did not parse"),
    "escaped-close": ('fn main() { println(`unclosed\\`) }\n', "did not parse"),
    "generic-tag": ('fn tag[T](parts []str, args []T) -> str { parts[0] }\nfn main() { println(tag`hello`) }\n', "uniquely resolved concrete vector"),
    "overloaded-tag": ('fn tag(parts []str, args []str) -> str { parts[0] }\nfn tag(parts []str, args []int) -> str { parts[0] }\nfn main() { println(tag`hello`) }\n', "uniquely resolved concrete vector"),
}
for label, (source, message) in negative.items():
    p = out / (label + ".nv")
    p.write_text(source, encoding="utf-8")
    r = invoke(label, [candidate, "emit", p])
    ds = [json.loads(line) for line in r.stdout.decode("utf-8", "replace").splitlines() if line.startswith('{"id":')]
    token_start = p.read_bytes().index(b"tag`" if label in ("generic-tag", "overloaded-tag") else b"`")
    ok = r.returncode != 0 and any(d["code"] == "E_NOVAC_SUBSET" and message in d["message"]
                                 and d["primary"]["start"] == token_start for d in ds)
    print(label + " REFUSAL " + ("PASS" if ok else "FAIL"), flush=True)
    results.append(dict(name=label, stage="negative", passed=ok, diagnostics=ds))
(out / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
sys.exit(0 if all(r["passed"] for r in results) else 1)
