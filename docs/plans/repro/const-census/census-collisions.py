# -*- coding: utf-8 -*-
u"""Опись имён констант novac: столкновения МЕЖДУ МОДУЛЯМИ (задание окна Карины).

Адрес ведущий — `файл : имя`, номер строки ПРИЛОЖЕНИЕМ: номер протухает от любой
правки выше по файлу, имя константы в файле уникально.

Считается ЧЕТЫРЕ вещи, и четвёртая — контрольная половина, без которой опись не
мера: доля, а не список.
  1. все `const` с модулем и признаком `export`;
  2. имена, встречающиеся в ДВУХ и более модулях;
  3. сколько из них не экспортированы НИГДЕ (ложно-приватные);
  4. сколько ВСЕГО имён констант в дереве.
Плюс попутное: повторяются ли имена импортов между модулями.

Вердиктов нет: «дефект или дисциплина имён» — слово окна Карины.
"""
import io
import os
import re
from collections import defaultdict

ROOT = u"D:/Sources/nv-lang/nova-wt-research/novac/src"
OUT = (u"C:/Users/B7E3~1/AppData/Local/Temp/claude/"
       u"d--Sources-nv-lang-nova/2ed7d346-7886-4768-b532-3cd5bde9137b/"
       u"scratchpad/const_census.txt")

CONST = re.compile(u"^(\\s*)(export\\s+)?const\\s+([A-Za-z_][A-Za-z0-9_]*)")
MODULE = re.compile(u"^module\\s+([A-Za-z0-9_.]+)")
IMPORT = re.compile(u"^import\\s+(\\S+)")

rows = []          # (imya, modul, fayl, stroka, export)
imports = defaultdict(set)   # imya importa -> moduli

for dirpath, _dirs, files in os.walk(ROOT):
    for fn in sorted(files):
        if not fn.endswith(u".nv"):
            continue
        p = os.path.join(dirpath, fn)
        rel = os.path.relpath(p, os.path.dirname(os.path.dirname(ROOT)))
        rel = rel.replace(u"\\", u"/")
        mod = u"?"
        with io.open(p, encoding="utf-8", errors="replace") as fh:
            for i, line in enumerate(fh, 1):
                m = MODULE.match(line)
                if m and mod == u"?":
                    mod = m.group(1)
                    continue
                m = CONST.match(line)
                if m:
                    # Константы ВНУТРИ тел (с отступом) тоже считаем, но метим:
                    # столкновение имён живёт на уровне модуля, и локальная
                    # константа в нём участвует, если она не в функции.
                    rows.append((m.group(3), mod, rel, i, bool(m.group(2)),
                                 len(m.group(1)) > 0))
                m = IMPORT.match(line)
                if m:
                    imports[m.group(1)].add(mod)

by_name = defaultdict(list)
for r in rows:
    by_name[r[0]].append(r)

cross = {}
for name, rs in by_name.items():
    mods = set(r[1] for r in rs)
    if len(mods) > 1:
        cross[name] = rs

never_exported = {n: rs for n, rs in cross.items()
                  if not any(r[4] for r in rs)}

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"ВСЕГО объявлений const: %d\n" % len(rows))
    fh.write(u"ВСЕГО РАЗНЫХ имён: %d\n" % len(by_name))
    fh.write(u"ИМЁН, встречающихся в ДВУХ и более модулях: %d (доля от разных "
             u"имён: %.1f%%)\n" % (len(cross), 100.0 * len(cross) / max(1, len(by_name))))
    fh.write(u"ИЗ НИХ не экспортированы НИГДЕ (ложно-приватные): %d\n\n"
             % len(never_exported))

    fh.write(u"=== СТОЛКНОВЕНИЯ, ведущий адрес `файл : имя` ===\n")
    for name in sorted(cross):
        rs = sorted(cross[name], key=lambda r: (r[2], r[3]))
        mark = u"  <-- НИГДЕ НЕ export" if name in never_exported else u""
        fh.write(u"\n%s%s\n" % (name, mark))
        for r in rs:
            fh.write(u"    %s : %s   [модуль %s, export=%s, в теле=%s]  (строка %d)\n"
                     % (r[2], r[0], r[1], u"да" if r[4] else u"нет",
                        u"да" if r[5] else u"нет", r[3]))

    fh.write(u"\n=== ПОПУТНОЕ: имена импортов в двух и более модулях ===\n")
    multi = {k: v for k, v in imports.items() if len(v) > 1}
    fh.write(u"разных путей импорта: %d, из них встречаются в 2+ модулях: %d\n"
             % (len(imports), len(multi)))
    for k in sorted(multi)[:25]:
        fh.write(u"    %-40s %s\n" % (k, u", ".join(sorted(multi[k]))))

print(u"const objavleniy: %d, raznyh imyon: %d" % (len(rows), len(by_name)))
print(u"stolknoveniy mezhdu modulyami: %d, iz nih nigde ne export: %d"
      % (len(cross), len(never_exported)))
print(u"fayl: %s" % OUT)
