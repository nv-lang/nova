# -*- coding: utf-8 -*-
"""Packages leg of the 0.3 distance re-measurement (task 141 rerun)."""
import pathlib, re, subprocess, time

WT = pathlib.Path(r"<tree>\nv-lang\worktrees\nova-opencode-novac-tail-value")
S = pathlib.Path(r"<tree>\nv-lang\nova-opencode\scratch-opencode\measure-0.3-1118")
NOVA = WT / "nova-driver" / "target" / "release" / "nova.exe"
REPOS = pathlib.Path(r"<tree>\nv-lang")
TABLE_141 = WT / "doc/tasks/141-zamer-korpusa-0-3-vse-primery-i-pakety-k/evidence/packages.tsv"
LOGS = S / "logs-packages"
LOGS.mkdir(exist_ok=True)

entries = []
for line in TABLE_141.read_text(encoding="utf-8").splitlines()[1:]:
    if line.strip():
        c = line.split("\t")
        entries.append((c[0], c[3]))  # repo, entry

out = ["repo\tentry\tverdict\tbuild_rc\tbuild_s\tfail_class\tfirst_diag\tnote\tdriver_contour"]
for repo, entry in entries:
    src = REPOS / repo / entry
    exe = S / "bin-pkg" / (repo + "_" + pathlib.Path(entry).stem + ".exe")
    exe.parent.mkdir(exist_ok=True)
    t0 = time.time()
    b = subprocess.run([str(NOVA), "build", str(src), "-o", str(exe), "--toolchain", "clang", "--timeout", "120"],
                       capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=300)
    bs = round(time.time() - t0, 1)
    (LOGS / (repo + "_" + pathlib.Path(entry).stem + ".log")).write_text(b.stdout + "\n--stderr--\n" + b.stderr, encoding="utf-8")
    verdict = "PASS" if b.returncode == 0 else "FAIL"
    fcls = fdiag = note = ""
    if verdict == "FAIL":
        m = re.search(r'"code":"(E_\w+)".*?"message":"(.*?)"', b.stdout)
        if m:
            fcls, fdiag = m.group(1), m.group(2)[:200]
        else:
            fcls = "OTHER"
            fdiag = (b.stderr.strip().splitlines() or [""])[-1][:200]
        for ln in b.stdout.splitlines():
            if "source file(s) staged" in ln:
                note = ln.strip()[:200]
                break
    out.append("\t".join([repo, entry, verdict, str(b.returncode), str(bs), fcls, fdiag, note, "patched-scratch-driver-030head"]))
    print(repo, entry, "->", verdict, flush=True)
    (S / "packages.tsv").write_text("\n".join(out) + "\n", encoding="utf-8")
print("PACKAGES DONE", flush=True)
