# -*- coding: utf-8 -*-
u"""Карта кодов `E_*`: где объявлен, есть ли негативная фикстура.

Это КАРТА, а не вердикт: она не говорит, что проверка плоха, — она показывает,
где приёмка держится ни на чём. Выдача — файл, который читается человеком и
пересчитывается командой.

ДВЕ ОГОВОРКИ, и обе внутри выдачи, а не в письме:
  * «объявлен» — вхождение идентификатора в исходниках, НЕ доказанная
    достижимость: мёртвый код попадёт в список;
  * счёт фикстур берёт только ЖИВЫЕ строки `// EXPECT_COMPILE_ERROR …` и
    `nova:expect`, а не любые упоминания: первая редакция считала и
    КОММЕНТАРИИ, и один код («E_ARRAY_SUGAR_TRAILING_TYPE_ARGS») выглядел
    осиротевшей фикстурой, хотя стоял в историческом пояснении.
"""
import io
import os
import re
from collections import Counter

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
OUT = ROOT + u"/docs/plans/repro/1186-check-vs-test/ecodes-map.txt"
CODE = re.compile(u"\\bE_[A-Z][A-Z0-9_]{2,}\\b")

declared = Counter()
where = {}
for dirpath, dirs, names in os.walk(os.path.join(ROOT, u"compiler-codegen/src")):
    dirs[:] = [d for d in dirs if d not in (u"target", u".git")]
    for n in names:
        if not n.endswith(u".rs"):
            continue
        p = os.path.join(dirpath, n)
        t = io.open(p, encoding="utf-8", errors="replace").read()
        rel = os.path.relpath(p, ROOT).replace(os.sep, u"/")
        for m in CODE.finditer(t):
            c = m.group(0)
            declared[c] += 1
            where.setdefault(c, rel)

fixtured = Counter()
for dirpath, dirs, names in os.walk(os.path.join(ROOT, u"spec_tests")):
    dirs[:] = [d for d in dirs if d not in (u"target", u".git")]
    for n in names:
        if not n.endswith(u".nv"):
            continue
        t = io.open(os.path.join(dirpath, n), encoding="utf-8", errors="replace").read()
        for line in t.splitlines():
            s = line.strip()
            # ЖИВАЯ строка ожидания, а не упоминание в пояснении: маркер стоит в
            # НАЧАЛЕ комментария-строки фикстуры.
            if s.startswith(u"// EXPECT_COMPILE_ERROR") or u"nova:expect" in s:
                for m in CODE.finditer(s):
                    fixtured[m.group(0)] += 1

rows = sorted(declared.items(), key=lambda kv: (-kv[1], kv[0]))
naked = [c for c, _ in rows if c not in fixtured]

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# Карта кодов E_*: объявление в компиляторе против фикстуры в корпусе\n")
    fh.write(u"# Снято %s скриптом ecodes-map.py (он же пересчитывает).\n#\n" % u"2026-09-20")
    fh.write(u"# ОГОВОРКА 1: «объявлен» — вхождение идентификатора в исходниках, а НЕ\n"
             u"# доказанная достижимость. Мёртвый код попадёт в список.\n"
             u"# ОГОВОРКА 2: фикстурой считается ЖИВАЯ строка ожидания, а не любое\n"
             u"# упоминание кода: упоминание в историческом пояснении фикстуры выглядит\n"
             u"# как ожидание и однажды уже дало ложную находку.\n#\n")
    fh.write(u"# объявлено кодов: %d; из них БЕЗ живой фикстуры: %d\n\n"
             % (len(declared), len(naked)))
    fh.write(u"%-46s %6s %8s  %s\n" % (u"код", u"упом.", u"фикстур", u"первый файл"))
    for c, n in rows:
        fh.write(u"%-46s %6d %8d  %s\n" % (c, n, fixtured.get(c, 0), where[c]))

print(u"объявлено %d, без живой фикстуры %d" % (len(declared), len(naked)))
print(u"первые десять БЕЗ фикстуры (по частоте упоминания):")
for c, n in rows:
    if c in naked:
        print(u"   %-44s %4d  %s" % (c, n, where[c]))
        naked.remove(c)
        if len([x for x in rows if x[0] not in fixtured]) - len(naked) >= 10:
            break
print(u"карта: %s" % OUT)
