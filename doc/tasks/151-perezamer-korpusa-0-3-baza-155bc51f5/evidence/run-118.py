# -*- coding: utf-8 -*-
"""0.3 distance re-measurement runner (task 141 rerun, 2026-11-18).

Contour: patched-scratch-driver-030head — nova-driver @ main 155bc51f5 with
SEED_* pins at the LOCAL unpublished oracle-built Carina (stage oracle-a,
announced to the verifier on every run). One row per example, TSV like 141.
"""
import hashlib
import pathlib
import subprocess
import sys
import time

WT = pathlib.Path(r"<tree>\nv-lang\worktrees\nova-opencode-novac-tail-value")
S = pathlib.Path(r"<tree>\nv-lang\nova-opencode\scratch-opencode\measure-0.3-1118")
NOVA = WT / "nova-driver" / "target" / "release" / "nova.exe"
TABLE_141 = WT / "doc/tasks/141-zamer-korpusa-0-3-vse-primery-i-pakety-k/evidence/table-118.tsv"
OUT_TSV = S / "table-118.tsv"
LOGS = S / "logs-118"
LOGS.mkdir(exist_ok=True)

lines = TABLE_141.read_text(encoding="utf-8").splitlines()[1:]
entries = [(ls[0], ls[1]) for ls in (l.split("\t") for l in lines if l.strip())]

env_zip = str(S / "nova-carina-0.3.0-030head-windows-x86_64.zip")
out = [ "n\tpath\tverdict\tbuild_rc\tbuild_s\trun_rc\texe_sha256_16\tfirst_stdout\tfail_class\tfail_first_line\tdriver_contour" ]
for n, path in entries:
    src = WT / path
    exe = S / "bin" / f"{n}.exe"
    exe.parent.mkdir(exist_ok=True)
    t0 = time.time()
    b = subprocess.run([str(NOVA), "build", str(src), "-o", str(exe), "--toolchain", "clang", "--timeout", "120"],
                       capture_output=True, text=True, encoding="utf-8", errors="replace",
                       cwd=str(WT), env=None, timeout=300)
    bs = round(time.time() - t0, 1)
    (LOGS / f"{n}.log").write_text(b.stdout + "\n--stderr--\n" + b.stderr, encoding="utf-8")
    verdict, run_rc, sha16, first, fcls, fline = "", "", "", "", "", ""
    if b.returncode != 0:
        verdict = "FAIL"
        for ln in b.stdout.splitlines():
            if '"severity":"error"' in ln or "error:" in ln:
                fline = ln.strip()[:400]
                if "E_NOVAC_SUBSET" in ln:
                    fcls = "E_NOVAC_SUBSET"
                else:
                    fcls = "OTHER"
                break
        if not fline:
            fline = (b.stderr.strip().splitlines() or [""])[-1][:400]
            fcls = "OTHER"
    else:
        data = exe.read_bytes()
        sha16 = hashlib.sha256(data).hexdigest()[:16]
        r = subprocess.run([str(exe)], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60)
        run_rc = str(r.returncode)
        first = (r.stdout.splitlines() or [""])[0][:120]
        verdict = "PASS"
    out.append("\t".join([n, path, verdict, str(b.returncode), str(bs), run_rc, sha16, first, fcls, fline.replace("\t", " "), "patched-scratch-driver-030head"]))
    print(f"{n} {path} -> {verdict}", flush=True)
    OUT_TSV.write_text("\n".join(out) + "\n", encoding="utf-8")
print("RUN-118 DONE", flush=True)
