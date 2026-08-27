# -*- coding: utf-8 -*-
"""Ядро стража check-fixed-but-open.

Считает строки реестра, которые стоят ОТКРЫТЫМИ, хотя коммит с заголовком
`fix(#N)` / `feat(#N)` уже в истории СУДИМОГО ДЕРЕВА (`HEAD`, не `--all`:
ссылки других worktree в общем .git — не его история).

Вывод — машинно-читаемые строки:
    unexpected=<N>      строк вне базы (это и есть отказ)
    stale=<N>           записей базы, которые больше не нужны (заметка)
    open_with_fix=<N>   всего строк «открыта, но правка есть»
Отрицательное значение unexpected означает, что ядро не нашло реестр.

Аргументы: <корень> [<файл-с-историей>]
Второй аргумент — для самотеста: строки вида `<sha>|<subject>` вместо
вызова git. В бою не используется.
"""
import io
import os
import re
import subprocess
import sys

REG_REL = os.path.join("docs", "plans", "221.1-bug-sweep.md")
BASE_REL = os.path.join("scripts", "guards", "fixed-but-open.baseline")

# Обе формы записи статуса, которые реально встречаются в реестре:
# `Статус: ОТКРЫТ` и `**Статус:** OPEN`.
#
# ДВОЕТОЧИЕ ОБЯЗАТЕЛЬНО (реестр 221.1 №775, замер 2026-08-26). Раньше стояло
# `Статус:?` — двоеточие НЕОБЯЗАТЕЛЬНОЕ, и выражение совпадало с ХРОНИКОЙ
# `**Статус был:** …` — формой, которой реестр по своей же конвенции записывает
# СНЯТЫЙ статус. `search` берёт ПЕРВОЕ вхождение, поэтому у строки с хроникой
# впереди страж читал слово `был` вместо живого статуса и считал строку НЕ
# открытой. Замер на день правки: 39 строк читались иначе, пять из них —
# открытые, а одна (№567) была открытой ПРИ СЛИТОМ ФИКСЕ, то есть ровно тем
# случаем, ради которого страж и заведён. Соседний `registry-routes-scan.py`
# требует двоеточие и потому не спотыкался никогда.
ST = re.compile(u"\\**\u0421\u0442\u0430\u0442\u0443\u0441\\**\\s*:\\s*\\**\\s*"
                u"([A-Za-z\u0410-\u042f\u0430-\u044f]+)")
OPEN_WORDS = (u"OPEN", u"\u041e\u0422\u041a\u0420\u042b\u0422")
ROW = re.compile(r"^\|\s*(\d{2,4})\s*\|")


def rows_of(path):
    t = io.open(path, encoding="utf-8", newline="").read()
    out = {}
    for line in t.split("\n"):
        m = ROW.match(line)
        if m:
            out[int(m.group(1))] = line
    return out


def fixed_numbers(root, histfile):
    if histfile:
        raw = io.open(histfile, encoding="utf-8", newline="").read()
    else:
        # HEAD, а не `--all` (2026-08-26, окно 274): worktree делят один .git, и
        # `--all` видит НЕПУШЕННЫЙ коммит `fix(#567)` на локальном main другого
        # окна — а судит ЭТО дерево, где строка 567 честно открыта. Правка
        # «приземлилась» только там, откуда её видит судимое дерево; чужая
        # ветка — не история этого дерева. Тот же класс, что «git add -A
        # подметает чужой индекс»: общий .git, чужие ссылки.
        raw = subprocess.check_output(
            ["git", "-C", root, "log", "--format=%h|%s", "HEAD"],
            stderr=subprocess.STDOUT).decode("utf-8", "replace")
    seen = {}
    for line in raw.split("\n"):
        if "|" not in line:
            continue
        sha, subj = line.split("|", 1)
        head = subj.split(":", 1)[0]
        if not head.startswith(("fix(", "feat(")):
            continue
        for num in re.findall(r"#(\d{2,4})", head):
            seen.setdefault(int(num), sha.strip())
    return seen


def baseline_of(root):
    p = os.path.join(root, BASE_REL)
    if not os.path.exists(p):
        return set()
    acc = set()
    for line in io.open(p, encoding="utf-8", newline="").read().split("\n"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = re.match(r"^(\d{2,4})\b", line)
        if m:
            acc.add(int(m.group(1)))
    return acc


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    histfile = sys.argv[2] if len(sys.argv) > 2 else None
    reg = os.path.join(root, REG_REL)
    if not os.path.exists(reg):
        sys.stdout.write("unexpected=-1\nstale=-1\nopen_with_fix=-1\n")
        return 0

    rows = rows_of(reg)
    fixed = fixed_numbers(root, histfile)
    base = baseline_of(root)

    open_with_fix = []
    for num in sorted(rows):
        m = ST.search(rows[num])
        if not m:
            continue
        word = m.group(1).upper()
        if not any(word.startswith(w) for w in OPEN_WORDS):
            continue
        if num in fixed:
            open_with_fix.append((num, fixed[num]))

    nums = set(n for n, _ in open_with_fix)
    unexpected = [(n, s) for n, s in open_with_fix if n not in base]
    stale = sorted(base - nums)

    for n, s in unexpected:
        sys.stdout.write("  #%-4d fix in %s, but the row is still OPEN\n"
                         % (n, s))
    for n in stale:
        sys.stdout.write("  note: #%d is in the baseline but no longer "
                         "open-with-fix\n" % n)
    sys.stdout.write("unexpected=%d\nstale=%d\nopen_with_fix=%d\n"
                     % (len(unexpected), len(stale), len(open_with_fix)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
