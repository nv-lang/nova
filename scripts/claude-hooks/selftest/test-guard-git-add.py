# -*- coding: utf-8 -*-
# Самотест правила «git add только по именам» хука guard-git.py.
#
# Из чего вырос (2026-09-23, разметка правил AGENTS.md по лестнице). AGENTS.md
# запрещает `git add -A`, `.`, `-u` и `git commit -a`, а регулярка хука знала
# только `-A`, `--all` и `.`: форма `-u` проходила молча, и ни одной клетки
# самотеста на это правило не было вовсе. `git commit -a` проверяется здесь же,
# потому что его держит ДРУГОЕ правило (область коммита) — клетка доказывает,
# что форма закрыта, а не то, каким именно правилом.
#
# Самотест ЗАПУСКАЕТ хук подпроцессом, как соседний test-guard-git-readonly.py:
# вырезанная из исходника регулярка перестаёт мерить, как только правка меняет
# её разметку. Путь к хуку выводится от расположения теста (класс №698).
import json
import os
import subprocess
import sys

H = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, "guard-git.py")
R = "/d/Sources/nv-lang/nova"

cases = [
    # ── НЕ ТРОГАЕМ: по именам — законная форма, первой ────────────────────
    ("git -C %s add file.txt" % R, 0, "add po imeni"),
    ("git -C %s add src/-u-named.nv" % R, 0, "imya fayla s -u vnutri"),
    ("git -C %s add docs/a.md docs/b.md" % R, 0, "add dvukh imyon"),
    # ── ЛОВИМ: подметающие формы ─────────────────────────────────────────
    ("git -C %s add -A" % R, 2, "add -A"),
    ("git -C %s add --all" % R, 2, "add --all"),
    ("git -C %s add ." % R, 2, "add ."),
    ("git -C %s add -u" % R, 2, "add -u"),
    ("git -C %s add --update" % R, 2, "add --update"),
    ("git -C %s commit -a -m x" % R, 2, "commit -a (pravilo oblasti)"),
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
