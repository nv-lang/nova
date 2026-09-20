# -*- coding: utf-8 -*-
"""Сузить «кириллица без кода» до похожего на ДИАГНОСТИКУ и показать остаток.

677 мест — это ЛЮБОЙ литерал с кириллицей вне комментария, включая отладочную
печать и внутренние сообщения. Предмет строки 1197 — то, что ПЕЧАТАЕТСЯ
ПОЛЬЗОВАТЕЛЮ как диагностика, поэтому здесь два числа:
  * места, где рядом стоит диагностический канал (`diag`, `bail`, `err`,
    `warn`, `push_error`, `report`), — верхняя оценка долга;
  * остаток — с примерами, чтобы было видно, что именно отсеяно, и чтобы
    отсев можно было оспорить.
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
ROOT = os.path.abspath(sys.argv[1])
IN = sys.argv[2]
OUT = sys.argv[3]

CHANNEL = re.compile(r"\b(diag|diagnostic|bail|err|error|warn|report|push_[a-z_]*|"
                     r"panic|expect|unreachable|note|hint)\w*\s*[!(]", re.I)

lines = io.open(IN, encoding="utf-8").read().split("\n")
start = lines.index(u"## Кириллица без кода в строке (ключ — файл:строка)") + 1
places = []
i = start
while i < len(lines):
    l = lines[i]
    m = re.match(r"^([^\s:]+):(\d+)$", l.strip())
    if m:
        places.append((m.group(1), int(m.group(2))))
    i += 1

chan, rest = [], []
for rel, n in places:
    p = os.path.join(ROOT, rel)
    try:
        src = io.open(p, encoding="utf-8", errors="replace").read().split("\n")
    except OSError:
        continue
    lo = max(0, n - 4)
    ctx = " ".join(src[lo:n + 1])
    (chan if CHANNEL.search(ctx) else rest).append((rel, n, src[n - 1].strip()[:110]))

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# Сужение «кириллица без кода» до диагностического канала\n#\n")
    fh.write(u"# ВСЕГО мест с кириллицей вне комментария и без кода в строке: %d\n" % len(places))
    fh.write(u"# ИЗ НИХ рядом с диагностическим каналом (не далее трёх строк выше):"
             u" %d — верхняя оценка долга\n" % len(chan))
    fh.write(u"# ОСТАТОК (отладка, внутренние тексты, данные): %d\n#\n" % len(rest))
    fh.write(u"# Признак КОНТЕКСТНЫЙ и потому грубый: он смотрит три строки выше\n"
             u"# литерала. Отсев печатается целиком ниже, чтобы его можно было\n"
             u"# оспорить, а не принимать на слово.\n\n")
    fh.write(u"## Рядом с диагностическим каналом\n\n")
    for rel, n, frag in chan:
        fh.write(u"%s:%d\n    %s\n" % (rel, n, frag))
    fh.write(u"\n## Остаток (отсеяно)\n\n")
    for rel, n, frag in rest:
        fh.write(u"%s:%d\n    %s\n" % (rel, n, frag))

print(u"мест всего %d; у канала %d; остаток %d" % (len(places), len(chan), len(rest)))
