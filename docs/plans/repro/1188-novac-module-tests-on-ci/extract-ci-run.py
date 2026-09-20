# -*- coding: utf-8 -*-
"""Извлечь из лога прогона CI то, чем падает шаг конвенций, и что уходит в skipped."""
import io
import json
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
LOG, RUN, OUT = sys.argv[1], sys.argv[2], sys.argv[3]

meta = json.loads(subprocess.run(
    ["gh", "run", "view", RUN, "--json",
     "databaseId,headSha,createdAt,conclusion,jobs"],
    capture_output=True).stdout.decode("utf-8", "replace"))

rows = [l for l in io.open(LOG, encoding="utf-8", errors="replace").read().split("\n")
        if l.startswith("novac-gate")]
body = [l.split("\t", 2)[-1] for l in rows]
body = [re.sub(r"^\S+Z ", "", l) for l in body]

guards = []
for l in body:
    m = re.match(r"(check-[a-z0-9-]+): FAIL", l)
    if m and m.group(1) not in guards:
        guards.append(m.group(1))
gate_fails = [l for l in body if l.startswith("NOVAC-GATE FAIL:")]

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# №1188: чем падает шаг конвенций СЕГОДНЯ и что уходит следом в skipped\n")
    fh.write(u"# Прогон %s, коммит %s, начат %s, вердикт %s\n#\n"
             % (meta["databaseId"], meta["headSha"][:12], meta["createdAt"], meta["conclusion"]))
    fh.write(u"# ВАЖНО: это НОВЕЙШИЙ доступный прогон, и он судит НЕ сегодняшний main:\n"
             u"# main ушёл вперёд и на момент замера не был запушен.\n\n")
    for j in meta["jobs"]:
        steps = j.get("steps", [])
        fa = [s["name"] for s in steps if s["conclusion"] == "failure"]
        sk = [s["name"] for s in steps if s["conclusion"] == "skipped"]
        fh.write(u"## %s — %s\n" % (j["name"], j["conclusion"]))
        fh.write(u"   шагов %d; упало: %s\n" % (len(steps), ", ".join(fa) or u"—"))
        fh.write(u"   в skipped ушло %d: %s\n\n" % (len(sk), ", ".join(sk) or u"—"))
    fh.write(u"## Стражи, отказавшие внутри шага конвенций (в порядке вывода): %d\n\n"
             % len(guards))
    for g in guards:
        fh.write(u"  %s\n" % g)
    fh.write(u"\n## Строки вердикта самого гейта (`NOVAC-GATE FAIL:`): %d\n\n"
             % len(gate_fails))
    for l in gate_fails:
        fh.write(u"  %s\n" % l[:200])

print(u"стражей с FAIL: %d; строк NOVAC-GATE FAIL: %d" % (len(guards), len(gate_fails)))
print(u"вывод: %s" % OUT)
