# -*- coding: utf-8 -*-
# Compare the two "is the row closed" readings over a registry file (read-only).
# A: check-push-proven-by-ci.py row_state ; B: registry-routes-scan.py status()
import io, re, sys
PARTIAL = re.compile(u"ЧАСТИЧНО|ЧАСТИЧЕН|ЧАСТИЧНАЯ")
CLOSED = re.compile(u"ЗАКРЫТ|ПОЧИНЕНО|СНЯТ")
FIELD = u"**Статус:**"
ROW = re.compile(u"^\\| ([0-9]+) \\|")

def a_state(line):
    i = line.find(FIELD)
    if i < 0:
        return "nofield"
    st = line[i + len(FIELD):i + len(FIELD) + 60]
    return "closed" if (CLOSED.search(st) and not PARTIAL.search(st)) else "open"

def b_state(line):
    m = re.search(u"Статус:\\s*(.{0,60})", line)
    st = m.group(1) if m else u""
    if not m:
        return "nofield"
    return "closed" if (CLOSED.search(st) and not PARTIAL.search(st)) else "open"

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
n = diff = 0
for line in io.open(sys.argv[1], encoding="utf-8").read().split(u"\n"):
    m = ROW.match(line)
    if not m:
        continue
    n += 1
    a, b = a_state(line), b_state(line)
    if a != b:
        diff += 1
        print("row %s: push-proven=%s routes-scan=%s" % (m.group(1), a, b))
print("rows=%d differ=%d" % (n, diff))
