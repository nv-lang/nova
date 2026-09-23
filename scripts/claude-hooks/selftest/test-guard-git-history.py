# -*- coding: utf-8 -*-
# Самотест правил хука guard-git.py о пуше силой и переписывании истории.
#
# Из чего вырос (2026-09-23, очередь механизмов лестницы правил,
# docs/dev/rules-for-agents.md §12). AGENTS.md запрещает `git push --force` и
# переписывание истории без разрешения, а держались эти правила только текстом.
# Законные формы стоят ПЕРВЫМИ: хук, ложно отказывающий обычный пуш или чтение
# лога со словом «rebase», снесут вместе с правилом.
#
# Самотест ЗАПУСКАЕТ хук подпроцессом (как соседние test-guard-git-*.py); дерево
# для `-C` выводится от расположения теста — путь к машине литералом это №698.
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
H = os.path.join(HERE, os.pardir, "guard-git.py")
R = os.path.abspath(os.path.join(HERE, os.pardir, os.pardir, os.pardir)).replace("\\", "/")

cases = [
    # ── НЕ ТРОГАЕМ ──────────────────────────────────────────────────────
    ("git -C %s push origin main" % R, 0, "push obychnyy"),
    ("git -C %s push --follow-tags origin main" % R, 0, "push --follow-tags"),
    ("git -C %s push origin v0.1.2" % R, 0, "push tega"),
    ("git -C %s log --grep rebase --oneline" % R, 0, "log --grep rebase"),
    ("git -C %s log --oneline -- docs/rebase-notes.md" % R, 0, "put' so slovom rebase"),
    ("git -C %s pull --ff-only origin main" % R, 0, "pull --ff-only"),
    # ── ЛОВИМ ───────────────────────────────────────────────────────────
    ("git -C %s push --force origin main" % R, 2, "push --force"),
    ("git -C %s push -f origin main" % R, 2, "push -f"),
    ("git -C %s push origin +main" % R, 2, "push +ref"),
    ("git -C %s push --force-with-lease origin main" % R, 2, "push --force-with-lease"),
    ("git -C %s rebase main" % R, 2, "rebase"),
    ("git -C %s filter-branch --tree-filter x HEAD" % R, 2, "filter-branch"),
    ("git -C %s pull --rebase origin main" % R, 2, "pull --rebase"),
]

bad = 0
for cmd, want, label in cases:
    p = subprocess.run([sys.executable, H],
                       input=json.dumps({"tool_input": {"command": cmd}}),
                       capture_output=True, text=True)
    got = p.returncode
    ok = got == want
    if not ok:
        bad += 1
    print("  %-7s %-30s want=%d got=%d" % ("ok" if ok else "PROVAL", label, want, got))

print("SLUCHAEV: %d" % len(cases))
print("PROVALOV: %d" % bad)
sys.exit(1 if bad else 0)
