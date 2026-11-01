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
import shutil
import subprocess
import sys
import tempfile

H = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, "guard-git.py")
# Дерево для `-C` выводится от расположения теста, а не пишется литералом:
# путь к машине автора в отслеживаемом скрипте — класс №698 (первая редакция
# этого файла так и покраснела на check-no-machine-paths).
R = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                 os.pardir, os.pardir, os.pardir)).replace("\\", "/")

# Клетка «commit -a» зовёт правило ОБЛАСТИ коммита, а у того есть законное
# исключение «идёт слияние», которое хук выясняет У ДЕРЕВА из `-C`. Гейт
# 2026-11-01 покраснел ровно на этом: самотест шёл в тот момент, когда в
# ГЛАВНОЙ рабочей копии шло слияние, исключение сработало — и клетка,
# обязанная доказывать запрет, доказала пропуск. Дерево для этой клетки
# теперь одноразовый каталог вне git: состояние главного дерева тест
# больше не читает (см. tmp-каталог ниже, снимается в конце).
TMP = tempfile.mkdtemp(prefix="nova_guard_git_add_selftest_").replace("\\", "/")

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
    # Дерево здесь — TMP, а не R: правило области при слиянии в -C-дереве
    # коммит разрешает (законно), и клетка краснела бы при каждом прогоне
    # гейта во время чужого слияния (гейт 2026-11-01, [862s]).
    ("git -C %s commit -a -m x" % TMP, 2, "commit -a (pravilo oblasti)"),
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
shutil.rmtree(TMP, ignore_errors=True)
sys.exit(1 if bad else 0)
