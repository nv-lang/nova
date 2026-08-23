# -*- coding: utf-8 -*-
# Самотест правила «коммит без названной области» (инцидент 2026-08-23: коммит
# одного файла забрал 49 из чужого индекса в общем дереве nova-p274).
#
# Обе стороны обязательны: правило ловит форму, которая берёт весь индекс, и НЕ
# краснит на форме с путём, на --amend, на слове «git commit» внутри текста
# сообщения и на осознанном override.
#
# Путь к хуку выводится от расположения теста — литеральный путь к машине автора
# в отслеживаемом скрипте это класс №698 (см. check-no-machine-paths).
import json
import os
import subprocess
import sys

H = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, "guard-git.py")
R = "/d/Sources/nv-lang/nova"  # дерево без слияния в процессе

cases = [
    # ловим: область не названа — берётся весь индекс
    ('git -C %s commit -s -m "fix: one thing"' % R, 2, "commit bez oblasti"),
    ('git -C %s add a.md && git -C %s commit -s -F /tmp/m.txt' % (R, R), 2,
     "add + commit v odnoy tsepochke"),
    # пропускаем: область названа
    ('git -C %s commit -s --only -- a.md' % R, 0, "--only -- path"),
    ('git -C %s commit -s -F /tmp/m.txt --only -- a.md b.md' % R, 0, "--only, dva puti"),
    ('git -C %s commit -s -o a.md' % R, 0, "-o kratkaya forma"),
    ('git -C %s commit -s -- a.md' % R, 0, "-- path bez --only"),
    # пропускаем: правка последнего коммита
    ('git -C %s commit -s --amend -F /tmp/m.txt' % R, 0, "--amend"),
    # пропускаем: осознанный override с причиной
    ('git -C %s commit -s -F /tmp/m.txt  # index-verified: merge resolution' % R, 0,
     "override index-verified"),
    # не коммит вовсе / упоминание в тексте
    ('git -C %s status --porcelain' % R, 0, "ne commit"),
    ('echo "nikogda ne delay git commit bez oblasti"', 0, "upominanie v tekste"),
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
    print("  %-7s %-28s want=%d got=%d" % ("ok" if ok else "PROVAL", label, want, got))

print("PROVALOV: %d" % bad)
sys.exit(1 if bad else 0)
