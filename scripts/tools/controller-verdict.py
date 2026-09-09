#!/usr/bin/env python3
"""Verdict per neighbouring window, computed from EVIDENCE only -- never from the text's meaning.

Why this exists (owner, 14:05 local 2026-09-09, verbatim): "I do not believe you -- our script in
python according to all the rules, check and push by it". Three times in one shift I put a working
status on a window that stood: first by the age of the entry, then by a service entry type, then by
the CONTENT of its report ("I take them in order" read as work). Every time the written rule was
already there. So the rule stops being prose and becomes this program: it prints the signal it
used, and refuses to invent one.

The closed list of signals, exactly as .claude/commands/controller.md defines it:
  A. a process of the window's OWN tree holds the machine (gate.sh / gate-novac / compiler)
     -> waits for its own tier; legal standing, no push
  B. the last transcript entry carries a tool_use block -> the turn is open, the window acts
  C. the tree's HEAD commit is NEWER than the window's last assistant text -> work landed
  D. an uncommitted file in the tree is NEWER than that text -> work in progress
  E. none of the above and the text is younger than the threshold (default 5 min) -> too early
     to judge; not a status, an honest "wait one cycle"
  F. none of the above and the text is older than the threshold -> STOOD, push it

Anything else -- a promise, a plan, a list of what remains, a service entry (`attachment`,
`frame-link`, `queue-operation`) -- is NOT a signal and never reaches the verdict.

Usage:
  python controller-verdict.py [--threshold MIN] [--tail-mb N]
  python controller-verdict.py --prove      # both-ways probe on synthetic cases
"""
import glob
import importlib.util
import io
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
# The repository root is DERIVED, never hardcoded: the same lesson the integrator taught on the
# selftest (a wired path makes the tool read a stranger's files and report them as ours).
ROOT = os.environ.get("NOVA_REPO_ROOT") or os.path.dirname(HERE.rstrip("/\\").rstrip("tools").rstrip("/\\"))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
CFG = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
ME = os.environ.get("CLAUDE_CODE_SESSION_ID", "")

TIER_RE = None  # set after WATCH loads
THRESHOLD_MIN = 5.0
# The observer must not appear in its own measurement -- prohibition 11 of the role, and the new
# tool walked into it on its FIRST cycle (14:10Z): the integrator got "works on the unblocked"
# because the main copy had a dirty file... mine, `scripts/tools/README.md`, edited by me two
# minutes earlier. My own edits are not his work, so they are not a signal about him.
MINE_RE = None  # compiled below, after `re` is available through WATCH
MINE_PATTERNS = (
    "controller-dock.py", "controller-peers-scan.py", "controller-peers-deep.py",
    "controller-machine-watch.py", "controller-selftest.py", "controller-verdict.py",
    "commands/controller.md", "prompts/controller-handoff.md", "tools/README.md",
)
TAIL_BYTES = 800_000


def load(name):
    """Load a sibling tool as a module (their file names carry dashes, so no plain import)."""
    path = os.path.join(HERE, name)
    spec = importlib.util.spec_from_file_location(name.replace("-", "_")[:-3], path)
    mod = importlib.util.module_from_spec(spec)
    src = open(path, encoding="utf-8").read().replace("\nif __name__", "\nif False and __name__")
    exec(compile(src, path, "exec"), mod.__dict__)
    return mod


SCAN = load("controller-peers-scan.py")
WATCH = load("controller-machine-watch.py")
import re as _re
TIER_RE = _re.compile(r"(gate\.sh|gate-novac)")


def iso_to_epoch(ts):
    if not ts:
        return None
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp()
    except Exception:
        return None


def hhmmss(epoch):
    if epoch is None:
        return "?"
    return time.strftime("%H:%M:%S", time.localtime(epoch))


def has_tool_use(entry):
    """Signal B: the LAST entry of the transcript carries a tool_use block.

    Deliberately looks at the last entry only. A tool_use five entries back was already answered
    and told us nothing about now -- that conflation is what made a finished turn look open.
    """
    if (entry.get("type") or "") != "assistant":
        return False
    c = (entry.get("message") or {}).get("content")
    if isinstance(c, list):
        return any(isinstance(b, dict) and b.get("type") == "tool_use" for b in c)
    return False


def git(tree, *args):
    try:
        out = subprocess.run(["git", "-C", tree] + list(args), capture_output=True, timeout=25)
        return out.stdout.decode("utf-8", "replace").strip()
    except Exception:
        return ""


def head_commit(tree):
    line = git(tree, "log", "-1", "--format=%h %ct")
    parts = line.split()
    if len(parts) == 2 and parts[1].isdigit():
        return parts[0], int(parts[1])
    return None, None


def newest_dirty_file(tree, limit=40):
    """Signal D: the newest mtime among files git reports as modified/untracked.

    Only files git already names -- a full walk of a worktree does not fit the timeout (measured
    04:37Z: two minutes on nova-p274, and the answer arrived after the push was needed).
    """
    out = git(tree, "status", "--porcelain")
    best_path, best_mtime = None, None
    for ln in out.splitlines()[:limit]:
        rel = ln[3:].strip().strip('"')
        if not rel:
            continue
        if any(rel.replace("\\", "/").endswith(sfx) for sfx in MINE_PATTERNS):
            continue  # my own file: see MINE_PATTERNS above
        p = os.path.join(tree, rel)
        try:
            m = os.path.getmtime(p)
        except OSError:
            continue
        if best_mtime is None or m > best_mtime:
            best_path, best_mtime = rel, m
    return best_path, best_mtime


def tree_of(path, cwd_hint):
    """Which working copy is this session's? Frequency of tree paths in its own transcript.

    The session's `cwd` is NOT enough: every window here reports the same cwd (the main copy),
    because that is where they were launched -- measured on all four sessions this shift.
    """
    try:
        lines = SCAN.read_tail_lines(path, 400_000)
    except Exception:
        lines = []
    blob = "\n".join(lines)
    parent = os.path.dirname(os.path.abspath(ROOT))
    best, best_n = None, 0
    try:
        names = [d for d in os.listdir(parent) if os.path.isdir(os.path.join(parent, d))]
    except OSError:
        names = []
    # A worktree name must appear MANY times to count as this session's home. Measured 14:37Z on
    # live tails: the window that lives in nova-p274 mentions it 290 times, claude-limits 183 --
    # while the integrator, who lives in the main copy, mentions nova-p274 exactly 4 times,
    # because I keep writing to him ABOUT their tier. Frequency without a floor made the tool
    # hand him a stranger's tree, and with it a stranger's slot as his "signal of work".
    # RAISED 20 -> 60 at 14:38Z 2026-09-09, because 20 was pierced by a stranger's tree and the
    # tool handed the integrator the wrong home for the second time. Measured on the same tails
    # this function reads (400 KB), so these numbers ARE facts about this code, not about the dock:
    #   integrator  gate/push=6   max foreign tree = 27 (claude-limits), own tree: none
    #   window 283  gate/push=1   nova-p283 = 141
    #   window 274  gate/push=2   nova-p274 = 141
    #   claude-limits/sdl         nova-sdl  = 113, claude-limits 26, nova-duckdb 53
    # 60 sits with a two-fold margin on BOTH sides: twice the loudest stranger, half the quietest
    # home. The dock disagreed with this function at 14:36Z (it said "nova (main)", correctly, by
    # its gate/push signal) -- and by my own rule a disagreement between two of my measurers is a
    # defect, not noise. The floor is the fix; the dock's signal stays its own.
    MIN_TREE_HITS = 60
    for d in names:
        if not os.path.isdir(os.path.join(parent, d, ".git")) and not os.path.isfile(os.path.join(parent, d, ".git")):
            continue
        n = blob.count(d)
        if d == os.path.basename(ROOT):
            # The main copy loses this contest by construction: its name is a prefix of every
            # worktree name (nova / nova-p274), so counting it plainly would always win.
            n = blob.count(os.path.basename(ROOT) + "/scripts") + blob.count("gate.sh") + blob.count("integrator-push")
        if n > best_n:
            best, best_n = d, n
    if best is None or best_n < MIN_TREE_HITS or best == os.path.basename(ROOT):
        return ROOT, best_n
    return os.path.join(parent, best), best_n


def slot_holders_by_tree():
    """Signal A: which trees hold the machine right now, and since when (earliest start)."""
    lines = WATCH.ps()
    if lines is None:
        return None
    rows = {}
    for ln in lines[1:]:
        f = WATCH.fields(ln)
        if f:
            rows[f[0]] = (f[1], ln)
    held = {}
    for pid, (ppid, ln) in rows.items():
        if not WATCH.BUSY.search(ln) or WATCH.SELF_RE.search(ln):
            continue
        # ONE answer for "whose tree is this", shared with the watch: cwd first (a tier launched
        # as a relative `bash scripts/gate.sh` carries no path at all), then the command line,
        # then the parents. Before 11:53Z this tool used owner_of() alone and printed `unknown`
        # where the watch printed `nova` -- two of my instruments disagreeing about one subject.
        owner = WATCH.cwd_owner(pid)
        if owner == "?":
            owner = WATCH.owner_of(ln)
        if owner == "?":
            # walk up to the parent, exactly as the watch does
            depth, cur = 0, ppid
            while owner == "?" and depth < 6 and cur in rows:
                owner = WATCH.owner_of(rows[cur][1])
                cur = rows[cur][0]
                depth += 1
        if owner == "?":
            owner = "unknown"
        stime = ln.split(None, 5)[4] if len(ln.split(None, 5)) >= 5 else "?"
        kind = "tier" if TIER_RE.search(ln) else "build/test"
        prev = held.get(owner)
        # A tier outranks a build: if both run in one tree, the tier is what the window waits for.
        if prev is None or (kind == "tier" and prev[0] != "tier") or (kind == prev[0] and stime < prev[1]):
            held[owner] = (kind, stime)
    return held


def verdict_for(entry, last_text_epoch, tree, held, now, threshold_min):
    """The closed list, in order. Returns (status, signal, push?)."""
    tree_name = os.path.basename(tree.rstrip("/\\")) if tree else "?"
    if held is None:
        return "UNKNOWN", "ps unavailable -- do not treat as free", False
    if tree_name in held:
        kind, stime = held[tree_name]
        # A holder is not automatically a TIER. Caught 14:08Z on the tool's first live run: a
        # one-second `nova build` from the main copy made the verdict say "waits for its own tier"
        # about a window that was simply compiling. Same class as everything else this shift --
        # the answer to "does this tree hold the slot" dressed as the answer to "is it gated".
        if kind == "tier":
            return "waits for its own tier", "own %s since %s" % (kind, stime), False
        return "works", "own-tree %s holds the slot since %s" % (kind, stime), False
    if has_tool_use(entry):
        return "works", "tool_use at %s" % hhmmss(iso_to_epoch(entry.get("timestamp"))), False
    h, ctime = head_commit(tree) if tree else (None, None)
    if ctime and last_text_epoch and ctime > last_text_epoch:
        return "works on the unblocked", "commit %s at %s newer than text" % (h, hhmmss(ctime)), False
    p, mt = newest_dirty_file(tree) if tree else (None, None)
    if mt and last_text_epoch and mt > last_text_epoch:
        return "works on the unblocked", "dirty %s at %s newer than text" % (p, hhmmss(mt)), False
    if last_text_epoch is None:
        return "UNKNOWN", "no assistant text in tail", False
    age = (now - last_text_epoch) / 60
    # A NEGATIVE age is impossible as an age, and printing it as one ("text -0.1 min old") hands
    # the reader a number that cannot be true and says nothing about why. Caught 13:43Z 2026-09-09
    # on a live cycle. The cause is in this tool's own order: `now` is sampled ONCE in main, before
    # the histories are read, so an entry written DURING the scan is newer than the measurement's
    # own clock. That is not noise -- it is the strongest possible signal of work, because the
    # window acted while I was measuring it. Same class as "the measurement before the edit answers
    # a different question": the number is right about the moment it was taken and wrong about the
    # moment it is used. So name it instead of smoothing it.
    if age < 0:
        return "works", "text at %s is NEWER than the scan start -- entry appeared mid-scan" % (
            hhmmss(last_text_epoch)), False
    if age < threshold_min:
        return "too early", "text %.1f min old, threshold %.0f" % (age, threshold_min), False
    return "STOOD", "no signal; silent %.1f min since %s" % (age, hhmmss(last_text_epoch)), True


def main():
    args = sys.argv[1:]
    threshold = THRESHOLD_MIN
    if "--threshold" in args:
        i = args.index("--threshold")
        threshold = float(args[i + 1])
    files = glob.glob(os.path.join(CFG, "projects", "*", "*.jsonl"))
    files = [f for f in files if os.path.basename(f) != ME + ".jsonl"]
    files.sort(key=os.path.getmtime, reverse=True)
    held = slot_holders_by_tree()
    now = time.time()
    print("time_utc=%s time_local=%s  root=%s" % (
        time.strftime("%H:%M:%SZ", time.gmtime(now)), time.strftime("%H:%M:%S", time.localtime(now)), ROOT))
    print("slot holders by tree: %s" % (held if held else "none"))
    print("")
    print("%-10s | %-11s | %-24s | %-46s | %s" % ("id", "tree", "status", "signal", "push"))
    rows = []
    for f in files[:4]:
        sid = os.path.basename(f)[:-6]
        tail = SCAN.parse(SCAN.read_tail_lines(f, TAIL_BYTES))
        if not tail:
            continue
        last_entry = tail[-1]
        last_text_ts = None
        for e in reversed(tail):
            if e.get("type") == "assistant" and SCAN.text_of(e.get("message") or {}).strip():
                last_text_ts = e.get("timestamp")
                break
        cwd = next((e["cwd"] for e in tail if e.get("cwd")), None)
        tree, _ = tree_of(f, cwd)
        status, signal, push = verdict_for(
            last_entry, iso_to_epoch(last_text_ts), tree, held, now, threshold)
        print("%-10s | %-11s | %-24s | %-46s | %s" % (
            sid[:8], os.path.basename(tree or "?")[:11], status, signal[:46], "YES" if push else "no"))
        if push:
            rows.append((sid[:8], os.path.basename(tree or "?")))
    print("")
    print("PUSH: %s" % (", ".join("%s (%s)" % r for r in rows) if rows else "nobody"))
    # The LIMIT, said out loud because a tool that hides it is worse than no tool: this program
    # answers "is there a signal of work", NOT "is the standing legal". A window blocked by the
    # owner's word or by somebody else's slot shows up as STOOD here and must NOT be pushed --
    # that judgement stays with the controller and its three signs of legal standing.
    print("LIMIT: PUSH is raw. Legal standing (owner's word, another window's slot, an")
    print("       irreversible action awaiting authorisation) is judged by the controller.")
    return 0


def prove():
    """Both-ways probe: each signal must decide, and its ABSENCE must fall through to STOOD.

    The probe carries the case a narrowing fix must not break (measured 08:52Z, when tightening
    the slot attribution silently turned a real holder into "unknown"): every signal is checked
    present AND absent on the same synthetic window.
    """
    now = 1_000_000.0
    text = now - 600  # ten minutes ago -> beyond any threshold
    tu = {"type": "assistant", "timestamp": "2026-09-09T11:00:00Z",
          "message": {"content": [{"type": "tool_use", "name": "Bash"}]}}
    txt = {"type": "assistant", "timestamp": "2026-09-09T11:00:00Z",
           "message": {"content": [{"type": "text", "text": "I take them in order, starting with text"}]}}
    tree = os.path.join(os.path.dirname(ROOT), "nova-probe-does-not-exist")
    cases = [
        ("A own tier holds the slot", txt, {"nova-probe-does-not-exist": ("tier", "12:54:26")}, "waits for its own tier", False),
        ("A absent -> falls through", txt, {}, "STOOD", True),
        ("A build, not tier -> works", txt, {"nova-probe-does-not-exist": ("build/test", "14:07:41")}, "works", False),
        ("B tool_use in last entry", tu, {}, "works", False),
        ("B absent (text only)", txt, {}, "STOOD", True),
        ("promise in the text is NOT a signal", txt, {}, "STOOD", True),
        ("ps unavailable", txt, None, "UNKNOWN", False),
    ]
    fail = 0
    print("%-38s | %-24s | %-24s | %s" % ("probe case", "expected", "got", "ok"))
    for label, entry, held, want, want_push in cases:
        status, signal, push = verdict_for(entry, text, tree, held, now, THRESHOLD_MIN)
        ok = status == want and push == want_push
        fail += 0 if ok else 1
        print("%-38s | %-24s | %-24s | %s" % (label, want, status, "OK" if ok else "FAIL"))
    # The threshold case needs a fresh text, otherwise it is the STOOD case in disguise.
    status, _, push = verdict_for(txt, now - 60, tree, {}, now, THRESHOLD_MIN)
    ok = status == "too early" and not push
    fail += 0 if ok else 1
    print("%-38s | %-24s | %-24s | %s" % ("E fresh text under threshold", "too early", status, "OK" if ok else "FAIL"))
    # F: an entry born DURING the scan -- timestamp after `now`. Must be named, not printed as a
    # negative age. And it must say `works`: the window acted while being measured.
    status, sig, push = verdict_for(txt, now + 6, tree, {}, now, THRESHOLD_MIN)
    ok = status == "works" and not push and "NEWER than the scan start" in sig
    fail += 0 if ok else 1
    print("%-38s | %-24s | %-24s | %s" % ("F entry newer than scan start", "works (named)", status,
                                          "OK" if ok else "FAIL"))
    # G is the side the fix must NOT touch (rule bought 08:52Z: a probe on a narrowing fix must
    # carry the case the narrowing was not meant to reach). A text exactly at the threshold edge
    # but POSITIVE stays "too early", not "works" -- otherwise the new branch would have swallowed
    # every fresh text and turned the tool into a machine that never pushes anybody.
    status, _, push = verdict_for(txt, now - 1, tree, {}, now, THRESHOLD_MIN)
    ok = status == "too early" and not push
    fail += 0 if ok else 1
    print("%-38s | %-24s | %-24s | %s" % ("G one-second-old text stays early", "too early", status,
                                          "OK" if ok else "FAIL"))
    # H and I: the tree floor, both ways, on a synthetic tail. Written as a probe because the
    # floor was pierced TWICE by live data (20 -> a stranger's tree at 14:36Z), and a number
    # nobody exercises drifts back into a suggestion.
    import tempfile as _tf
    home = os.path.basename(os.path.dirname(os.path.abspath(ROOT))) and "nova-p274"
    for label, n, want_main in (("H own tree above the floor", 100, False),
                                ("I stranger below the floor", 27, True)):
        d = _tf.mkdtemp(prefix="tree-probe-")
        p = os.path.join(d, "t.jsonl")
        with open(p, "w", encoding="utf-8") as fh:
            for _ in range(n):
                fh.write('{"cwd":"x","text":"/d/Sources/nv-lang/%s/x"}\n' % home)
        got, hits = tree_of(p, None)
        is_main = os.path.abspath(got) == os.path.abspath(ROOT)
        ok = (is_main == want_main)
        fail += 0 if ok else 1
        print("%-38s | %-24s | %-24s | %s" % (
            label, "main copy" if want_main else "own tree",
            "main copy" if is_main else os.path.basename(got), "OK" if ok else "FAIL"))
    print("")
    print("PROVE %s (%d cases)" % ("OK" if not fail else "FAIL", len(cases) + 5))
    return 1 if fail else 0


if __name__ == "__main__":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    sys.exit(prove() if "--prove" in sys.argv[1:] else main())
