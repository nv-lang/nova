# -*- coding: utf-8 -*-
"""1187, шаг 1: МЕСТО ПЕЧАТИ каждого кода долга и функция, которая его печатает.

Почему это первый шаг, а не список сразу. Разбить долг на живые / затенённые /
недостижимые можно только пробой, а пробу нельзя написать, не зная, ГДЕ код
печатается и при каком условии. Карта 1186 давала ПЕРВОЕ вхождение имени — а это
часто doc-комментарий этажом выше (оговорка 3 той карты, купленная на
`E_UNSAFE_REQUIRED`).

Что считается местом печати: строка, где код стоит ВНУТРИ строкового литерала в
исполнимом коде, — то есть ровно то, что уезжает в текст диагностики. Имя функции
берётся ближайшим `fn ...` выше по файлу; это грубо для вложенных замыканий и
названо грубым, а не выдано за точность.

usage: python ecode-print-sites.py <корень> <база> <вывод>
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

ROOT, BASE, OUT = os.path.abspath(sys.argv[1]), sys.argv[2], sys.argv[3]
FN = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
SRC = os.path.join(ROOT, "compiler-codegen", "src")

debt = []
for line in io.open(BASE, encoding="utf-8", errors="replace"):
    s = line.split("#", 1)[0].strip()
    if s.startswith("debt_code="):
        debt.append(s.split("=", 1)[1].strip())
debt = sorted(set(debt))


def strings_of(line):
    """Куски внутри двойных кавычек вне `//`-комментария."""
    out, cur, in_str, esc, i = [], [], False, False, 0
    while i < len(line):
        ch = line[i]
        if in_str:
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
                out.append("".join(cur))
                cur = []
                i += 1
                continue
            cur.append(ch)
            i += 1
            continue
        if ch == '"':
            in_str = True
            i += 1
            continue
        if ch == "/" and i + 1 < len(line) and line[i + 1] == "/":
            break
        i += 1
    if in_str and cur:
        out.append("".join(cur))
    return out


sites = {}          # код -> (файл, строка, функция, текст)
files_read = 0
for dirpath, dirs, names in os.walk(SRC):
    dirs[:] = [d for d in dirs if d not in (".git", "target")]
    for n in sorted(names):
        if not n.endswith(".rs"):
            continue
        p = os.path.join(dirpath, n)
        rel = os.path.relpath(p, ROOT).replace(os.sep, "/")
        try:
            lines = io.open(p, encoding="utf-8", errors="replace").read().split("\n")
        except OSError:
            continue
        files_read += 1
        fn_now = "?"
        for i, raw in enumerate(lines, 1):
            m = FN.match(raw)
            if m:
                fn_now = m.group(1)
            joined = " ".join(strings_of(raw))
            if not joined:
                continue
            for c in re.findall(r"\[(E_[A-Z][A-Z0-9_]{2,}|W_[A-Z][A-Z0-9_]{2,})\]", joined):
                if c in debt and c not in sites:
                    sites[c] = (rel, i, fn_now, joined.strip()[:90])

missing = [c for c in debt if c not in sites]

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# 1187, шаг 1: место ПЕЧАТИ кода долга (не первое вхождение имени)\n")
    fh.write(u"# Снято помощником 2026-09-20. ЗАМЕР, НЕ ВЕРДИКТ.\n#\n")
    fh.write(u"# ЗНАМЕНАТЕЛИ: кодов в базе долга %d; файлов .rs прочитано %d;\n"
             u"# место печати найдено у %d; НЕ найдено у %d.\n#\n"
             % (len(debt), files_read, len(sites), len(missing)))
    fh.write(u"# «Не найдено» значит: код объявлен в базе, но в строковом литерале\n"
             u"# исполнимого кода в форме `[КОД]` не встречается. Это НЕ «мёртвый код»:\n"
             u"# текст может собираться иначе (как у `argbind.rs`, где код приклеивается\n"
             u"# выше по течению). Такие коды идут в отдельную кучу и требуют чтения.\n\n")
    fh.write(u"%-46s %-52s %s\n" % (u"код", u"место печати", u"функция"))
    for c in sorted(sites):
        rel, i, fn, _ = sites[c]
        fh.write(u"%-46s %-52s %s\n" % (c, u"%s:%d" % (rel, i), fn))
    fh.write(u"\n## Место печати НЕ найдено (%d)\n\n" % len(missing))
    for c in missing:
        fh.write(u"  %s\n" % c)

print(u"кодов долга %d; место печати найдено %d; не найдено %d; файлов %d"
      % (len(debt), len(sites), len(missing), files_read))
print(u"вывод: %s" % OUT)
