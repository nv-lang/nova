# -*- coding: utf-8 -*-
u"""Опись констант novac, вторая редакция — после проверки на ПУСТУЮ МИШЕНЬ.

Первый прогон дал «столкновений 0», и это верно, но само по себе ничего не
значит: ноль бывает и от того, что мерить нечего. Проверка мишени: имена из
поломки окна Карины В ДЕРЕВЕ ЕСТЬ (`B_LF` в `novac/src/lex/lex.nv`, `LF_BYTE`
в `novac/src/check/check.nv`) — то есть предмет существует, а ноль означает
другое: столкновений нет, ПОТОМУ ЧТО их обходят переименованием.

Значит мерить надо цену дисциплины, а не число нарушений:
  1. сколько констант НЕ экспортировано — каждая такая видима соседям по голому
     имени, то есть является миной для любого, кто выберет то же имя;
  2. сколько имён уникальны в дереве (если все — дисциплина уже оплачена);
  3. где обход ЗАПИСАН в комментарии — прямые улики цены;
  4. распределение по модулям: где мин больше всего.
Вердиктов нет.
"""
import io
import os
import re
from collections import defaultdict

ROOT = u"D:/Sources/nv-lang/nova-wt-research/novac/src"
OUT = (u"C:/Users/B7E3~1/AppData/Local/Temp/claude/"
       u"d--Sources-nv-lang-nova/2ed7d346-7886-4768-b532-3cd5bde9137b/"
       u"scratchpad/const_census2.txt")

CONST = re.compile(u"^(\\s*)(export\\s+)?const\\s+([A-Za-z_][A-Za-z0-9_]*)")
MODULE = re.compile(u"^module\\s+([A-Za-z0-9_.]+)")
DODGE = re.compile(u"(already declares|уже объявл|переименов|not `B_|"
                   u"undeclared in this translation unit|collide|столкнов)",
                   re.IGNORECASE)

rows, dodges = [], []
for dirpath, _dirs, files in os.walk(ROOT):
    for fn in sorted(files):
        if not fn.endswith(u".nv"):
            continue
        p = os.path.join(dirpath, fn)
        rel = os.path.relpath(p, os.path.dirname(os.path.dirname(ROOT))).replace(u"\\", u"/")
        mod = u"?"
        for i, line in enumerate(io.open(p, encoding="utf-8", errors="replace"), 1):
            m = MODULE.match(line)
            if m and mod == u"?":
                mod = m.group(1)
            m = CONST.match(line)
            if m:
                rows.append((m.group(3), mod, rel, i, bool(m.group(2))))
            if line.lstrip().startswith(u"//") and DODGE.search(line):
                dodges.append((rel, i, line.strip()[:150]))

priv = [r for r in rows if not r[4]]
by_mod = defaultdict(lambda: [0, 0])
for r in rows:
    by_mod[r[1]][0 if r[4] else 1] += 1
names = defaultdict(set)
for r in rows:
    names[r[0]].add(r[1])

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"ВСЕГО const: %d, из них НЕ export: %d (%.0f%%)\n"
             % (len(rows), len(priv), 100.0 * len(priv) / max(1, len(rows))))
    fh.write(u"РАЗНЫХ имён: %d; имён в 2+ модулях: %d\n"
             % (len(names), sum(1 for v in names.values() if len(v) > 1)))
    fh.write(u"\n=== ПО МОДУЛЯМ (export / НЕ export) ===\n")
    for mod in sorted(by_mod):
        fh.write(u"    %-22s %3d / %3d\n" % (mod, by_mod[mod][0], by_mod[mod][1]))
    fh.write(u"\n=== ЗАПИСАННЫЕ ОБХОДЫ (комментарии, называющие столкновение) ===\n")
    for rel, i, txt in dodges:
        fh.write(u"    %s (строка %d)\n        %s\n" % (rel, i, txt))
    fh.write(u"\n=== НЕ ЭКСПОРТИРОВАННЫЕ КОНСТАНТЫ, ведущий адрес `файл : имя` ===\n")
    for r in sorted(priv, key=lambda r: (r[2], r[0])):
        fh.write(u"    %s : %s   [модуль %s]  (строка %d)\n" % (r[2], r[0], r[1], r[3]))

print(u"vsego const %d, ne export %d, raznyh imyon %d, obhodov v kommentariyah %d"
      % (len(rows), len(priv), len(names), len(dodges)))
print(u"fayl: %s" % OUT)
