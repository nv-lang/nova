# -*- coding: utf-8 -*-
"""doc/tasks/124-raise-novac-check-cache-wall-by-measurem/evidence/extract-ci-walls.py

Task 124, round 2: pull the raw wall times of the two guarded steps from a
nova-gate CI run's novac-gate job log. The numbers the task needs:

  * novac-check-cache -- self-reported ("стена Nс") in its own line;
  * check-novac-emitted-unique -- no self-report: gate-novac.sh's guard()
    captures the whole output and prints it AFTER the guard finishes, so the
    block's timestamp is the completion time; the duration is the difference
    between the emitted-unique block's last line and the previous block's
    (check-novac-emission-size, the sequential guard right before it);
  * iteration-cost check-one -- full-tier runs only, self-reported in ms;
  * runner name of the job's VM and the tier verdict lines.

Usage: python extract-ci-walls.py <run-id> [run-id ...]
"""
import json
import re
import subprocess
import sys
from datetime import datetime

REPO = "nv-lang/nova"


def gh(path):
    p = subprocess.run(["gh", "api", path], capture_output=True)
    if p.returncode != 0:
        raise SystemExit(f"gh api {path} failed: {p.stderr.decode('utf-8', 'replace')}")
    return p.stdout


def ts(line):
    # GitHub job-log lines: 2026-10-26T11:29:25.1234567Z <text>
    m = re.match(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})\.\d+Z ", line)
    if not m:
        return None
    return datetime.strptime(m.group(1), "%Y-%m-%dT%H:%M:%S")


def main():
    for run in sys.argv[1:]:
        jobs = json.loads(gh(f"repos/{REPO}/actions/runs/{run}/jobs").decode("utf-8-sig"))
        nov = [j for j in jobs["jobs"] if j["name"].startswith("novac-gate")]
        if not nov:
            print(f"run {run}: no novac-gate job")
            continue
        j = nov[0]
        print(f"run {run}: job {j['id']} status={j['status']} conclusion={j['conclusion']} "
              f"runner={j.get('runner_name')} attempt={j.get('run_attempt')}")
        if j["status"] != "completed":
            print("  (not completed yet)")
            continue
        log = gh(f"repos/{REPO}/actions/jobs/{j['id']}/logs").decode("utf-8", "replace")
        t_esize = t_euniq = None
        for line in log.split("\n"):
            if "novac-check-cache:" in line and "стена" in line:
                print("  cache:", line.strip())
            if "check-novac-emission-size" in line and ("ok" in line or "FAIL" in line):
                t_esize = ts(line)
                print("  esize:", line.strip()[:160])
            if "check-novac-emitted-unique" in line and ("ok" in line or "FAIL" in line):
                t_euniq = ts(line)
                print("  euniq:", line.strip()[:160])
            if "iteration-cost:" in line:
                print("  itcost:", line.strip()[:200])
            if "ОБРЫВ" in line or "novac-gate ok:" in line or "novac-gate: FAIL" in line:
                print("  gate:", line.strip()[:200])
        if t_esize and t_euniq:
            print(f"  emitted-unique wall (log timestamps): {(t_euniq - t_esize).total_seconds():.0f}s")


if __name__ == "__main__":
    main()
