#!/usr/bin/env python3
"""Docking: after a restart, work out WHO the live peers are and WHAT each one works on.

Why this exists: peer names (nova-XX) and session ids change on every restart, so a controller
that remembers them is wrong within one cycle. Identity is recovered from evidence instead:

  1. the peer's own `ListAgents` output, printed into its transcript ("This session is nova-XX")
     -- take the LAST occurrence, the earlier ones are stale names;
  2. the worktree it actually works in -- by frequency of tree paths in its transcript. This is
     the robust signal: window 274 gave 2936 hits of `nova-p274`, window 283 gave 1063.
  3. the INTEGRATOR is the exception and must not be guessed by frequency: he has no own
     worktree, he works in the main copy, so his transcript mentions every tree. Признаки:
     mentions of gate.sh / gate-bg / push / three mirrors, and cwd = the main copy.

Usage: python controller-dock.py [tail_mb]
Prints one block per live-looking session: id, name, tree, role guess, last activity, last text.
The controller must still READ the last text before acting -- this script gathers, it does not judge.
"""
import glob
import io
import json
import os
import re
import sys
import time

CFG = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
ME = os.environ.get("CLAUDE_CODE_SESSION_ID", "")
TAIL = int(float(sys.argv[1]) * 1e6) if len(sys.argv) > 1 else 3_000_000
FRESH_MIN = 90  # a session with no writes for longer is probably dead; still listed, marked so

NAME = re.compile(r"This session is (nova-[0-9a-z]+) \[([0-9a-f]+)\]")
TREE = re.compile(r"(nova-p\d+[a-z0-9]*|nova-[a-z]+\d*|claude-limits|nova-duckdb)")
INTEGRATOR = re.compile(r"(gate-bg|scripts/gate\.sh|push-after-gate|gitverse|sourcecraft|NOVAC-GATE)")


def text_of(msg):
    c = msg.get("content")
    if isinstance(c, str):
        return c
    out = []
    if isinstance(c, list):
        for b in c:
            if isinstance(b, dict):
                if b.get("type") == "text":
                    out.append(b.get("text", ""))
                elif b.get("type") == "tool_use":
                    out.append("[tool_use %s]" % b.get("name"))
    return "\n".join(out)


def scan(path):
    size = os.path.getsize(path)
    with open(path, "rb") as f:
        if size > TAIL:
            f.seek(size - TAIL)
            f.readline()
        raw = f.read().decode("utf-8", "replace")

    # The name is printed only when that session ran ListAgents, which may be hours back --
    # searching the tail alone returned "?" for every peer (measured 16:42Z). Scan the WHOLE
    # file for the name, in chunks, and keep the last hit: the tail is right for activity,
    # wrong for identity.
    names = NAME.findall(raw)
    if not names:
        names = []
        with open(path, "rb") as fh:
            carry = ""
            while True:
                chunk = fh.read(8_000_000)
                if not chunk:
                    break
                s = carry + chunk.decode("utf-8", "replace")
                names.extend(NAME.findall(s))
                carry = s[-200:]
    trees = {}
    for t in TREE.findall(raw):
        trees[t] = trees.get(t, 0) + 1
    integ_hits = len(INTEGRATOR.findall(raw))

    entries = []
    for ln in raw.splitlines():
        if ln.startswith("{"):
            try:
                entries.append(json.loads(ln))
            except Exception:
                pass

    cwd = None
    last_text = None
    last_text_ts = None
    for e in entries:
        if not cwd and e.get("cwd"):
            cwd = e["cwd"]
    for e in reversed(entries):
        if e.get("type") == "assistant":
            t = text_of(e.get("message") or {})
            if t.strip() and not t.startswith("[tool_use"):
                last_text, last_text_ts = t, e.get("timestamp")
                break

    return dict(
        name=names[-1][0] if names else "?",
        ref=names[-1][1] if names else "?",
        trees=sorted(trees.items(), key=lambda kv: -kv[1])[:3],
        integ_hits=integ_hits,
        cwd=cwd,
        last_type=entries[-1].get("type") if entries else "?",
        last_ts=entries[-1].get("timestamp") if entries else None,
        last_text=last_text,
        last_text_ts=last_text_ts,
        size=size,
    )


def main():
    files = [f for f in glob.glob(os.path.join(CFG, "projects", "*", "*.jsonl"))
             if os.path.basename(f) != ME + ".jsonl"]
    files.sort(key=os.path.getmtime, reverse=True)
    now = time.time()
    print("dock time: utc=%s local=%s" % (time.strftime("%H:%M:%SZ", time.gmtime()),
                                          time.strftime("%H:%M:%S")))
    print("(names/ids change on restart -- identity below comes from evidence, not memory)")
    shown = 0
    for path in files[:8]:
        age = (now - os.path.getmtime(path)) / 60
        if age > FRESH_MIN and shown >= 4:
            break
        info = scan(path)
        tree = info["trees"][0][0] if info["trees"] else "?"
        # the integrator works in the main copy: many gate/push mentions, no dominant own tree
        role = "window"
        if info["integ_hits"] >= 20 and (not info["trees"] or info["trees"][0][1] < 200):
            role = "INTEGRATOR (main copy)"
            tree = "nova (main)"
        print("=" * 96)
        print("id=%s  name=%s [%s]  role=%s" % (os.path.basename(path)[:8], info["name"], info["ref"], role))
        print("  tree=%s   trees_seen=%s   gate/push mentions=%d" % (tree, info["trees"], info["integ_hits"]))
        print("  cwd=%s  size=%.0fMB  last=%s @ %s  age=%.1f min" % (
            info["cwd"], info["size"] / 1e6, info["last_type"], info["last_ts"], age))
        print("  last text @ %s:" % info["last_text_ts"])
        t = (info["last_text"] or "").replace("\r", "").strip()
        print("    " + (t[:700] + (" ...[+%d]" % (len(t) - 700) if len(t) > 700 else "")).replace("\n", "\n    "))
        shown += 1
    print("=" * 96)
    print("Docked %d sessions. NEXT: read each last text before judging; a stale name is not a dead peer."
          % shown)


if __name__ == "__main__":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    main()
