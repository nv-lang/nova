#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check-push-proven-by-ci.py -- main receives only a commit GitHub CI has judged.

Owner's decision 2026-09-30 (integrator window, on the question "should the
heavy gates run locally, once a day, or on GitHub CI?"): the local machine runs
only the cheap tier (guards, targeted fixtures); the heavy tier (mega-CU, crate
tests, conformance-full, novac-gate) is GitHub CI. The integrator pushes the
candidate to the branch `integrate` on origin -- the workflows trigger on it
(affa00973) -- and moves `main` to the SAME commit only when CI on that commit
is green. Measured the same evening: seven local push-tier gates in one evening
(limit: one a day), six red on what the guards catch in seconds, each holding
the one machine the three windows share for 5-40 minutes.

WHAT IT CHECKS (called by scripts/githooks/pre-push when refs/heads/main is
pushed, with the LOCAL sha being pushed):
  1. every REQUIRED workflow (list below) has a completed run on exactly this
     sha (any event: the `integrate` push, a PR, a dispatch); of several runs of
     one workflow the newest is judged;
  2. every job of those runs concluded success (or skipped) -- or is named in
     scripts/guards/ci-accepted-red.list together with a registry row that is
     still OPEN. A closed or missing row makes the entry stale, and a stale
     entry is a refusal even when everything is green: the exemption must leave
     together with its reason;
  3. a run still queued / in progress is a refusal ("wait"), not a pass.

WORDS OF THE VERDICT (G9: say only what was established): "no run" (CI never
judged this sha), "still <status> -- wait" (judging, no verdict yet), "<job>:
<conclusion>" (judged and not green). They are different findings and are not
folded into one "red".

CLOSED ROW -- the same reading as registry-routes-scan.py (the canon of the
registry guards): the `**Статус:**` field of the row contains ЗАКРЫТ / ПОЧИНЕНО
/ СНЯТ and not ЧАСТИЧНО. A row without the field cannot be judged and is a
refusal, not a guess.

WHY THE EXISTING check-ci-status.sh DOES NOT COVER IT: that guard judges CI of
the CURRENT origin/main -- the previous push -- so it says "do not push on top
of red", not "push only what CI proved". And on 2026-09-30 every push of the day
went out with NOVA_SKIP_CI_CHECK=1 (nova-lint red on #1355), i.e. it was off.
That key does NOT switch this guard off: the concerns differ, the keys differ.

ESCAPE: NOVA_PUSH_UNPROVEN="<reason naming a registry row, e.g. #1234>" --
printed, and the reason must carry a row number (a bare "1" is refused).

Without gh / network: SKIP is NOT a pass here -- the push is refused with the
reason (use the escape key if GitHub itself is down).

Selftest seams (scripts/guards/selftest/test-check-push-proven-by-ci.sh):
  --runs-json F   replaces `gh run list` (a JSON list as gh prints it)
  --jobs-json F   replaces `gh run view <id> --json jobs` ({"<runId>": [jobs]})
  --registry F    replaces docs/plans/221.1-bug-sweep.md
  --accepted F    replaces scripts/guards/ci-accepted-red.list
"""
import json
import os
import re
import subprocess
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-push-proven-by-ci"
REQUIRED = [
    "nova-gate", "crate-tests", "nova-lint", "nova-test-regression",
    "contracts-crosscheck", "contracts-z3", "nova-doc",
]
ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
ACCEPTED = os.path.join(ROOT, "scripts", "guards", "ci-accepted-red.list")
REGISTRY = os.path.join(ROOT, "docs", "plans", "221.1-bug-sweep.md")
FIELD = u"**Статус:**"
CLOSED = re.compile(u"ЗАКРЫТ|ПОЧИНЕНО|СНЯТ")
PARTIAL = re.compile(u"ЧАСТИЧНО|ЧАСТИЧЕН|ЧАСТИЧНАЯ")
GREEN_JOB = ("success", "skipped")


def say(msg):
    print(NAME + ": " + msg)


def refuse(msg):
    print(NAME + ": FAIL -- " + msg, file=sys.stderr)
    sys.exit(1)


def gh(args):
    try:
        out = subprocess.run(["gh"] + args, capture_output=True, text=True,
                             encoding="utf-8", errors="replace", timeout=60)
    except (OSError, subprocess.TimeoutExpired) as e:
        refuse("gh is not usable (%s); CI proof cannot be read. If GitHub is down: "
               "NOVA_PUSH_UNPROVEN=\"<reason #NNNN>\"" % e)
    if out.returncode != 0:
        refuse("gh %s failed: %s" % (" ".join(args[:2]), (out.stderr or "").strip()[:200]))
    return out.stdout


def row_state(registry_text, num):
    """'open' / 'closed' / 'missing' / 'nofield' for registry row `num`."""
    prefix = "| %s |" % num
    for line in registry_text.splitlines():
        if line.startswith(prefix):
            i = line.find(FIELD)
            if i < 0:
                return "nofield"
            st = line[i + len(FIELD):i + len(FIELD) + 60]
            if CLOSED.search(st) and not PARTIAL.search(st):
                return "closed"
            return "open"
    return "missing"


def accepted_entries(accepted_path, registry_path):
    out = []
    if not os.path.exists(accepted_path):
        return out
    try:
        registry_text = open(registry_path, encoding="utf-8").read()
    except OSError as e:
        registry_text = None
        reg_err = str(e)
    base = os.path.basename(accepted_path)
    for n, line in enumerate(open(accepted_path, encoding="utf-8"), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = re.match(r"^(\S+)\s*/\s*(.+?)\s+#(\d{2,5})\b", line)
        if not m:
            refuse("%s:%d: unparsed entry (form: `<workflow> / <job prefix> #NNNN <why>`): %s"
                   % (base, n, line))
        wf, job, num = m.group(1), m.group(2).strip(), m.group(3)
        if registry_text is None:
            refuse("%s:%d: registry %s is unreadable (%s) -- the exemption for %s / %s "
                   "cannot be judged" % (base, n, registry_path, reg_err, wf, job))
        st = row_state(registry_text, num)
        if st != "open":
            why = {"closed": "is closed", "missing": "does not exist",
                   "nofield": "has no status field"}[st]
            refuse("%s:%d: registry row #%s %s -- the exemption for %s / %s is stale, "
                   "remove it together with its reason" % (base, n, num, why, wf, job))
        out.append((wf, job, num))
    return out


def take(args, flag):
    if flag in args:
        i = args.index(flag)
        if i + 1 >= len(args):
            refuse("%s needs a value" % flag)
        v = args[i + 1]
        del args[i:i + 2]
        return v
    return None


def main():
    args = sys.argv[1:]
    runs_file = take(args, "--runs-json")
    jobs_file = take(args, "--jobs-json")
    registry = take(args, "--registry") or REGISTRY
    accepted_path = take(args, "--accepted") or ACCEPTED
    if len(args) != 1 or args[0].startswith("-"):
        refuse("usage: check-push-proven-by-ci.py <sha> [--runs-json F --jobs-json F "
               "--registry F --accepted F]")
    sha = args[0]
    esc = os.environ.get("NOVA_PUSH_UNPROVEN", "")
    if esc:
        m = re.search(u"[#№](\\d{2,5})", esc)
        if not m:
            refuse("NOVA_PUSH_UNPROVEN must name a registry row (#NNNN) -- got: %r" % esc)
        # #1477: "a reason naming a registry row" -- the row has to EXIST. The
        # same guard refuses `#00` in ci-accepted-red.list as "does not exist";
        # the escape used to take it, so a typo'd or invented number passed.
        try:
            registry_text = open(registry, encoding="utf-8").read()
        except OSError as e:
            refuse("NOVA_PUSH_UNPROVEN names row #%s, but the registry %s is unreadable "
                   "(%s) -- the escape cannot be judged" % (m.group(1), registry, e))
        if row_state(registry_text, m.group(1)) == "missing":
            refuse("NOVA_PUSH_UNPROVEN names registry row #%s, which does not exist -- "
                   "got: %r" % (m.group(1), esc))
        say("SKIPPED by NOVA_PUSH_UNPROVEN: %s (sha %s NOT proven by CI)" % (esc, sha[:9]))
        return 0
    accepted = accepted_entries(accepted_path, registry)
    if runs_file:
        runs = json.load(open(runs_file, encoding="utf-8"))
    else:
        runs = json.loads(gh(["run", "list", "--commit", sha, "--limit", "100", "--json",
                              "databaseId,workflowName,status,conclusion,event,createdAt,headSha"])
                          or "[]")
    jobs_by_run = json.load(open(jobs_file, encoding="utf-8")) if jobs_file else {}
    latest = {}
    for r in sorted(runs, key=lambda r: r.get("createdAt", "")):
        # gh filters by --commit already; the offline file is filtered here the same way.
        hs = r.get("headSha") or ""
        if hs and not (hs.startswith(sha) or sha.startswith(hs)):
            continue
        latest[r.get("workflowName")] = r
    problems, notes, used = [], [], set()
    for wf in REQUIRED:
        r = latest.get(wf)
        if r is None:
            problems.append("%s: no run on %s -- push it to `integrate` first and wait for CI"
                            % (wf, sha[:9]))
            continue
        if r.get("status") != "completed":
            problems.append("%s: run %s still %s -- wait for the verdict"
                            % (wf, r.get("databaseId"), r.get("status")))
            continue
        if r.get("conclusion") == "success":
            continue
        rid = str(r.get("databaseId"))
        if jobs_file:
            jobs = jobs_by_run.get(rid, [])
        else:
            jobs = json.loads(gh(["run", "view", rid, "--json", "jobs"]) or "{}").get("jobs", [])
        bad = [(j.get("name", ""), j.get("conclusion")) for j in jobs
               if j.get("conclusion") not in GREEN_JOB]
        if not bad:
            problems.append("%s: run %s concluded %s but no job that is not green was listed "
                            "-- cannot tell what failed" % (wf, rid, r.get("conclusion")))
            continue
        for jn, jc in bad:
            hit = [a for a in accepted if a[0] == wf and jn.startswith(a[1])]
            if hit:
                used.add(hit[0])
                notes.append("%s / %s: %s, accepted by open row #%s" % (wf, jn, jc, hit[0][2]))
            else:
                problems.append("%s / %s: %s (run %s)" % (wf, jn, jc, rid))
    for n in notes:
        say("note: " + n)
    for a in accepted:
        judged = latest.get(a[0], {}).get("status") == "completed"
        if a not in used and judged:
            say("note: accepted entry `%s / %s #%s` matched no red job on %s -- if the job is "
                "green for good, the entry can go" % (a[0], a[1], a[2], sha[:9]))
    sys.stdout.flush()
    if problems:
        for p in problems:
            print("  " + p, file=sys.stderr)
        refuse("%d finding(s) across the %d required workflows: %s is not proven green. main moves only "
               "to a commit GitHub CI has judged (owner, 2026-09-30); escape: "
               "NOVA_PUSH_UNPROVEN=\"<reason #NNNN>\"" % (len(problems), len(REQUIRED), sha[:9]))
    say("ok: %d/%d required workflows green on %s%s" % (
        len(REQUIRED), len(REQUIRED), sha[:9],
        (" (%d red job(s) accepted by open rows)" % len(notes)) if notes else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
