# -*- coding: utf-8 -*-
"""Third pass: a registry entry starts with a row whose FIRST cell is an id.
A marker "has a number" if it occurs on such a row (any cell of it).
Everything else -- a continuation row with an empty first cell, or a plain
prose line -- is a mention, not an entry.
"""
import io
import os
import re
import sys

root = sys.argv[1]
backlog = os.path.join(root, "docs", "plans", "backlog-followups.md")
sweep = os.path.join(root, "docs", "plans", "221.1-bug-sweep.md")
base = os.path.join(root, "scripts", "guards", "bug-number-sync.baseline")

bl = set(x[1:-1] for x in re.findall(
    r"\[M-[a-z0-9_.-]+\]", io.open(backlog, encoding="utf-8").read()))
frozen = set(l.strip() for l in io.open(base, encoding="utf-8") if l.strip())
live = sorted(bl - frozen)

num_row = re.compile(r"^\|\s*([0-9]+)\s*\|")
q_row = re.compile(r"^\|\s*(Q[0-9]+)\s*\|")

numbered = {}
qonly = {}
mention = {}
for n, line in enumerate(io.open(sweep, encoding="utf-8", errors="replace"), 1):
    hits = re.findall(r"M-[a-z0-9_.-]+", line)
    if not hits:
        continue
    mn = num_row.match(line)
    mq = q_row.match(line)
    for h in hits:
        if mn:
            numbered.setdefault(h, (n, mn.group(1)))
        elif mq:
            qonly.setdefault(h, (n, mq.group(1)))
        else:
            mention.setdefault(h, n)

print("live markers judged by the guard: %d" % len(live))
n_num = [m for m in live if m in numbered]
n_q = [m for m in live if m not in numbered and m in qonly]
n_men = [m for m in live if m not in numbered and m not in qonly and m in mention]
n_abs = [m for m in live if m not in numbered and m not in qonly and m not in mention]
print("has a decimal-numbered row: %d" % len(n_num))
print("only a Q-row (no decimal number): %d" % len(n_q))
for m in n_q:
    print("  Q-ONLY %s  221.1:%d cell=%s" % (m, qonly[m][0], qonly[m][1]))
print("only a mention (continuation row / prose, NO entry of its own): %d" % len(n_men))
for m in n_men:
    print("  MENTION-ONLY %s  221.1:%d" % (m, mention[m]))
print("absent entirely: %d" % len(n_abs))
