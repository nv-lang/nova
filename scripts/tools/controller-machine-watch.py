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
import os
import re
import subprocess
import sys
import time

# Derived, never remembered -- the integrator's rule, 21:41 local 2026-09-08: any tool of
# mine that knows the repository path must derive it, or it reads a stranger's files and
# reports them as its own. NOVA_REPO_ROOT overrides.
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get("NOVA_REPO_ROOT") or os.path.abspath(os.path.join(HERE, "..", ".."))

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
# Attribute a process to a tree by the path inside its command line -- and ONLY by a path.
# The name must sit between path separators: `d:/Sources/nv-lang/nova-p274/...`. Caught 08:23Z
# 2026-09-09, and it accused ME: the integrator's `bash scripts/gate.sh` carried the text of his
# commit message ("Donor: window nova-b8. Committed by name to still the tree for the tier"), the
# bare name matched inside that prose, and the watch printed `machine BUSY by nova-b8 x2` while I
# had launched nothing. The tool answered "which line contains this name" and called it an answer
# to "who holds the slot" -- the class this role exists to catch, in the role's own instrument.
# DERIVED, not guessed -- fixed 15:19Z 2026-09-09 after a six-case probe went 4/6. The pattern
# used to spell the names it expected (`nova`, `nova-pNNN`, `nova-<word>`), and two live cases
# broke it: a RELATIVE main-copy path (`./nova-cli/target/release/nova`) attributed to a
# non-existent tree called `nova-cli`, and a worktree whose name does not begin with "nova" at all
# (`claude-limits`) attributed to `nova-cli` as well, because the real name was invisible to the
# pattern. Both are one class: a check that recites the members of a set instead of reading the
# set. The set is on disk -- the directories beside the main copy that carry a `.git` -- so the
# names come from there, longest first (a longest-match alternation makes `nova-p274` win over
# `nova`), and the main copy's own subdirectories are excluded by NAME rather than by the shape of
# the path around them, which is what let the relative form slip through.
SUBDIRS = ("nova-cli", "compiler-codegen", "nova-lsp", "nova_rt", "novac", "std", "spec_tests")


def _tree_names():
    parent = os.path.dirname(os.path.abspath(ROOT)) if "ROOT" in globals() else None
    names = set()
    if parent and os.path.isdir(parent):
        for d in os.listdir(parent):
            p = os.path.join(parent, d)
            if os.path.isdir(os.path.join(p, ".git")) or os.path.isfile(os.path.join(p, ".git")):
                names.add(d)
    names.add(os.path.basename(os.path.abspath(ROOT)) if "ROOT" in globals() else "nova")
    names -= set(SUBDIRS)
    return sorted(names, key=len, reverse=True)


TREE_NAMES = _tree_names()
TREE = re.compile(r"[/\\](" + "|".join(re.escape(n) for n in TREE_NAMES) + r")(?=[/\\])") \
    if TREE_NAMES else re.compile(r"[/\\](nova)(?=[/\\])")
# Everything after `-c` / `-m` / `-F` is DATA the process was handed, not an address: a commit
# message, a python snippet, a grep pattern. Cut it before attributing.
DATA_ARG = re.compile(r"\s-(?:c|m|F)\s")
# The observer must not appear in its own measurement (caught 16:16Z and again 00:51Z: this
# script's own `ps`/`grep` carried a tree name in its command line and was reported as that tree
# taking the machine). Kept at module level so sibling tools reuse it rather than copy it.
SELF_RE = re.compile(r"(controller-\w+|\bps -ef\b|\bgrep\b|\bawk\b|\bwc\b|shell-snapshots)")


# STDOUT MUST NOT BE ABLE TO KILL THE MEASUREMENT -- fixed 16:24Z 2026-09-09, and the cause is
# the machine itself: the slot holder was `C:\Users\<russian name>\.cargo\bin\cargo.exe`, that
# path decodes with a replacement character (we read bytes with errors="replace"), and printing it
# to a cp1251 console raised UnicodeEncodeError. The traceback landed INSTEAD of the verdict line,
# so the watch answered nothing at all -- and the watch is the tool every other decision of this
# role stands on: "is the main tree free to edit", "who holds the slot", "did a tier start".
#
# The class is the one I have been writing down all shift, in its harshest form yet: the tool did
# not answer wrongly, it answered NOTHING, and a missing verdict reads to a hurried eye exactly
# like a quiet machine. Same shape as prohibition 16 ("busy by unknown is not a conclusion") --
# only here the unknown was produced by my own print statement.
try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:  # noqa: BLE001
    pass


def ps():
    """One `ps -ef` snapshot, decoded defensively.

    `text=True` decodes with the console codepage (cp1251 here), and a single byte outside it
    -- a user name, a path -- raises UnicodeDecodeError INSIDE subprocess. Measured 01:00:13Z
    2026-09-09: the second of the two samples died that way, `ps failed` was printed, and the
    verdict was still printed as if built from two samples. That is exactly the class I watch
    for in others: the tool answered a question it was not asked and stayed quiet about it.
    Decode bytes ourselves with errors="replace" -- a mangled character in a name never changes
    whether a process is a slot holder.
    """
    try:
        out = subprocess.run(["ps", "-ef"], capture_output=True, timeout=30)
        return out.stdout.decode("utf-8", errors="replace").splitlines()
    except Exception as e:  # noqa: BLE001
        print("ps failed: %s" % e)
        return None


def owner_of(line):
    """Which working copy launched this? Longest match wins (nova-p274 before nova).

    Subdirectories of the main copy are NOT trees: `nova/nova-cli/target/release/nova.exe`
    is the integrator running from `nova`, not a separate worktree (caught 16:31Z, when the
    watch reported the owner as "nova-cli" and that name exists nowhere as a working copy).
    """
    cut = DATA_ARG.search(line)
    if cut:
        line = line[:cut.start()]
    for sub in ("nova-cli", "compiler-codegen", "nova-lsp", "nova_rt"):
        line = line.replace("nova/" + sub + "/", "nova/").replace("nova\\" + sub + "\\", "nova\\")
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


# Lifted to module level 11:53Z 2026-09-09: controller-verdict.py asked the same question
# ("whose tree holds this slot") through owner_of() alone and printed `unknown` for a tier
# the watch had already named `nova`. Two of my own instruments disagreeing about one
# subject is a defect, not noise -- so the answer lives in ONE function both of them call.
def cwd_owner(pid):
    """The tree a process actually RUNS IN, read from the OS, not from its own text.

    Needed by the 08:47Z fix above: once attribution was narrowed to path components, the
    integrator's tier -- launched as the relative `bash scripts/gate.sh` -- carried no path
    at all, and the watch printed `BUSY by ?`. Prohibition 16 calls that an unhandled case,
    not a verdict, so the fix had a second side and it had to be checked too.

    Read it with msys `readlink`, NOT `os.readlink`: this runs under the Windows python,
    for which /proc does not exist at all (`WinError 3`) -- measured 08:52Z. A tool asking
    the wrong filesystem would answer "?" forever and look like an honest unknown.
    LIMIT, named because a probe found it (2/2 both ways, 08:56Z): /proc knows only msys
    pids, so a native `nova.exe` gets no cwd here -- it keeps its full path in argv, which
    owner_of() already reads, and its msys parent covers the rest.
    """
    try:
        out = subprocess.run(["readlink", "/proc/%d/cwd" % pid],
                             capture_output=True, timeout=10)
        target = out.stdout.decode("utf-8", errors="replace").strip()
    except Exception:  # noqa: BLE001
        return "?"
    if not target:
        return "?"
    return owner_of(target.rstrip("/\\") + "/")


def main():
    # A single snapshot of `ps` can land in the gap BETWEEN a gate's child processes and report
    # a busy machine as free. Measured 17:22Z: a count taken at 17:22:42 said "free", another
    # at 17:23:10 found three holders -- the mega-CU child had started at 20:19:57 local and the
    # first sample simply missed the moment. So: sample twice, ~2s apart, and take the UNION.
    # A free verdict must be free in BOTH samples.
    first = ps()
    time.sleep(2)
    second = ps()
    samples = [s for s in (first, second) if s is not None]
    if not samples:
        print("VERDICT: UNKNOWN -- both ps samples failed, no measurement taken")
        return 1
    lines = list(samples[0])
    if len(samples) > 1:
        seen = set(lines)
        lines += [ln for ln in samples[1] if ln not in seen]
    # The signature must name how many samples actually survived: a FREE verdict from ONE
    # sample is weaker than from two (a single snapshot can land in the gap between a gate's
    # children), and a reader who is not told cannot know which one he got.
    sample_note = "union of two ps samples ~2s apart" if len(samples) == 2 \
        else "ONE ps sample only -- the other failed, a FREE verdict here is UNPROVEN"
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
        o = cwd_owner(pid)
        if o != "?":
            return o
        o = owner_of(ln)
        if o != "?":
            return o
        return inherited_owner(ppid, depth + 1)

    # The observer must not appear in its own measurement. Caught 16:16Z: this script's own
    # `ps`/`grep` carried the string "nova-p283" in its command line and was reported as that
    # tree taking the machine out of turn. A watch that accuses itself is worse than no watch.
    # `grep -E` alone missed `grep -ciE` and other flag orders -- caught 00:51Z 2026-09-09, when
    # my own counting command showed up as an unidentified slot holder for the second time.
    # Match the tool, not one spelling of its flags.
    # Lives at module level (see SELF above main) so a sibling tool reuses THIS pattern instead of
    # copying it -- a second copy of the observer filter would drift, and the drift is invisible
    # until the watch accuses somebody. Named 14:07Z, when controller-verdict.py needed it.
    SELF = SELF_RE
    # THIRD occurrence of the same class (00:51Z 2026-09-09): a heredoc python probe of mine put
    # the BUSY pattern itself into its argv, so the watch listed its own source lines as slot
    # holders. Word filters cannot fix this -- the words are legitimately there. A real slot
    # holder is a process line that starts with the ps columns (uid pid ppid tty stime cmd);
    # anything without that shape is text that leaked into the listing, not a process.
    PS_ROW = re.compile(r"^\S+\s+\d+\s+\d+\s")

    busy, noise = [], []
    for ln in lines[1:]:
        if not ln.strip():
            continue
        if SELF.search(ln) or not PS_ROW.match(ln):
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
    print("BUSY (slot holders, %s): %d" % (sample_note, len(busy)))
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
