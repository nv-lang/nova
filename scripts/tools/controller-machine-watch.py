#!/usr/bin/env python3
"""Who is using the machine right now: attribute each heavy process to a worktree/owner.

The controller needs this to answer ONE question the integrator asked (16:15Z 2026-09-08):
did anyone take the machine out of turn? Queue after the tier: nova-04 (274), then nova-55.

Two different questions, deliberately separated -- a lesson from 16:14Z, when my count of 0
and window 283's count of 5 were BOTH right with different selections:
  BUSY  = gate / compiler / test processes = the slot is taken
  NOISE = stale tail -f, orphaned cmd.exe = load, not a slot

Usage: python controller-machine-watch.py
Prints one block per class, plus a verdict line naming the owning tree(s).
"""
import re
import subprocess
import sys
import time

# Heavy = actually occupies the machine's slot.
# Both spellings on purpose: the built oracle is `target/release/nova.exe` on Windows and
# `target/release/nova` on Linux, and novac lives at `novac/target/novac[.exe]`. A pattern
# that knew only the .exe form would find nothing on Linux and print "machine FREE" --
# silence dressed as a verdict. Caught by scripts/guards/check-guard-honesty.py, 2026-09-08.
# Anchored to release/ and target/ rather than a bare name: a bare `nova` would match every
# command line that merely contains the repository path.
BUSY = re.compile(r"(gate\.sh|gate-novac|nova test|(?:release|target)[/\\\\]novac?(?:\.exe)?(?![\w.])|cargo|clang|cl\.exe|link\.exe)")
# Noise = present in ps, but not a slot holder.
NOISE = re.compile(r"(tail -|tail\.exe|vcvars|cmd\.exe)")
# Attribute a process to a tree by the path inside its command line.
TREE = re.compile(r"(nova-p\d+\w*|nova-[a-z]+\d*|nova(?![-\w]))")


def ps():
    try:
        out = subprocess.run(["ps", "-ef"], capture_output=True, text=True, timeout=30)
        return out.stdout.splitlines()
    except Exception as e:  # noqa: BLE001
        print("ps failed: %s" % e)
        return []


def owner_of(line):
    """Which working copy launched this? Longest match wins (nova-p274 before nova).

    Subdirectories of the main copy are NOT trees: `nova/nova-cli/target/release/nova.exe`
    is the integrator running from `nova`, not a separate worktree (caught 16:31Z, when the
    watch reported the owner as "nova-cli" and that name exists nowhere as a working copy).
    """
    for sub in ("nova-cli", "compiler-codegen", "nova-lsp", "nova_rt"):
        line = line.replace("nova/" + sub, "nova/").replace("nova\\" + sub, "nova\\")
    hits = TREE.findall(line)
    if not hits:
        return "?"
    hits.sort(key=len, reverse=True)
    return hits[0]


def fields(ln):
    """UID PID PPID TTY STIME COMMAND -> (pid, ppid, stime, rest)."""
    p = ln.split(None, 5)
    if len(p) < 6:
        return None
    try:
        return int(p[1]), int(p[2]), p[4], p[5]
    except ValueError:
        return None


def main():
    # A single snapshot of `ps` can land in the gap BETWEEN a gate's child processes and report
    # a busy machine as free. Measured 17:22Z: a count taken at 17:22:42 said "free", another
    # at 17:23:10 found three holders -- the mega-CU child had started at 20:19:57 local and the
    # first sample simply missed the moment. So: sample twice, ~2s apart, and take the UNION.
    # A free verdict must be free in BOTH samples.
    lines = ps()
    time.sleep(2)
    second = ps()
    seen = set(lines)
    lines = lines + [ln for ln in second if ln not in seen]
    rows = {}
    for ln in lines[1:]:
        f = fields(ln)
        if f:
            rows[f[0]] = (f[1], ln)

    def inherited_owner(pid, depth=0):
        """A child of a known tree belongs to that tree: walk up the parents."""
        if depth > 6 or pid not in rows:
            return "?"
        ppid, ln = rows[pid]
        o = owner_of(ln)
        if o != "?":
            return o
        return inherited_owner(ppid, depth + 1)

    # The observer must not appear in its own measurement. Caught 16:16Z: this script's own
    # `ps`/`grep` carried the string "nova-p283" in its command line and was reported as that
    # tree taking the machine out of turn. A watch that accuses itself is worse than no watch.
    SELF = re.compile(r"(controller-machine-watch|controller-peers-|\bps -ef\b|grep -E|shell-snapshots)")

    busy, noise = [], []
    for ln in lines[1:]:
        if not ln.strip():
            continue
        if SELF.search(ln):
            continue
        # A process started days ago is not a slot holder, whatever it is called. The `ps`
        # output of a days-old process shows a DATE instead of a clock, so match either form:
        # "Sep  6" (dated) or a bare orphan whose parent is 1 and whose start is not today.
        # Caught 16:51Z: an orphaned `cmd.exe` with vcvars64 from Sep 6 was reported as
        # "machine BUSY by ?" for three cycles running, on a machine that was in fact free.
        stale = bool(re.search(r"\s(Sep|Aug|Jul|Jun)\s+\d+\s", ln)) or "vcvars" in ln
        if BUSY.search(ln) and not stale:
            busy.append(ln)
        elif NOISE.search(ln) or stale:
            noise.append(ln)

    print("time_utc=%s time_local=%s" % (time.strftime("%H:%M:%SZ", time.gmtime()),
                                         time.strftime("%H:%M:%S")))
    print()
    # The composition below is the UNION of two `ps` samples taken ~2s apart, so during a tier's
    # start it can list transient children that no single snapshot would show. The COUNT is not
    # inflated by that (a busy machine is busy), but the composition must not be passed off as one
    # instant slice -- integrator's note, 22:27 local 2026-09-08, after my "BUSY 9 then BUSY 3".
    print("BUSY (slot holders, union of two ps samples ~2s apart): %d" % len(busy))
    owners = {}
    for ln in busy:
        o = owner_of(ln)
        if o == "?":
            f = fields(ln)
            if f:
                o = inherited_owner(f[1])
        owners[o] = owners.get(o, 0) + 1
        print("  [%s] %s" % (o, ln[:110].strip()))
    print()
    print("NOISE (load, not a slot): %d" % len(noise))
    for ln in noise[:6]:
        print("  %s" % ln[:100].strip())
    print()
    if not busy:
        print("VERDICT: machine FREE (no gate/compiler/test process)")
    else:
        parts = ", ".join("%s x%d" % (k, v) for k, v in sorted(owners.items(), key=lambda kv: -kv[1]))
        print("VERDICT: machine BUSY by %s" % parts)


if __name__ == "__main__":
    sys.exit(main())
