# -*- coding: utf-8 -*-
u"""Сколько ФАЙЛОВ открывает каждая из трёх оставшихся форм плеча `match`.

Задание окна Карины 2026-09-20. Мерка её же, и в ней вся суть: греп считает,
сколько раз форма НАПИСАНА, а нужно — сколько файлов она ОТКРЫВАЕТ. У файла
ОДИН первый отказ разбора, остальное каскад; поэтому берётся ПЕРВЫЙ отказ вида
«did not parse this» в каждом файле и классифицируется строка, на которую он
указывает.

Основа — `docs/plans/repro/peers-one-unit/first_fallback.py` (её скрипт),
добавлена классификация по четырём корзинам:
  * голый `return` телом плеча   — `Pat => return e`
  * строковый литерал плеча      — `"a" => …`
  * символьный литерал плеча     — `'a' => …`
  * прочее                       — перечисляется полностью, а не сворачивается
                                   в число: «прочее», которого не видно, прячет
                                   пятую форму.

Счётчик НЕРАЗОБРАННЫХ строк обязателен и печатается даже нулём.
"""
import glob
import io
import json
import os
import re
import subprocess
import sys
from collections import defaultdict

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
EXE = u"./novac/target/novac.exe"

env = dict(os.environ)
env["NOVAC_SELF_PATH"] = "novac/src"
env["NOVAC_UNIT"] = "1"

first, unparsed = {}, 0
mods = sorted(set(os.path.dirname(p).replace(os.sep, u"/")
                  for p in glob.glob(os.path.join(ROOT, u"novac/src/*/*.nv"))))
for m in mods:
    fs = [os.path.relpath(p, ROOT).replace(os.sep, u"/")
          for p in sorted(glob.glob(m + u"/*.nv")) if not p.endswith(u"_test.nv")]
    if not fs:
        continue
    r = subprocess.run([EXE, "check"] + fs, cwd=ROOT, env=env,
                       capture_output=True, timeout=600)
    out = (r.stdout or b"").decode("utf-8", "replace")
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except ValueError:
            unparsed += 1
            continue
        if u"did not parse this" not in d.get(u"message", u""):
            continue
        p = (d.get(u"primary") or {}).get(u"file", u"")
        if p in first:
            continue
        first[p] = (d.get(u"primary") or {}).get(u"start", 0)

RET = re.compile(u"=>\\s*return\\b")
STR_ARM = re.compile(u'^\\s*"')
CHR_ARM = re.compile(u"^\\s*'")

buckets = defaultdict(list)
for p in sorted(first):
    lo = first[p]
    path = p if os.path.isabs(p) else os.path.join(ROOT, p)
    try:
        t = io.open(path, encoding="utf-8", newline="").read()
    except OSError:
        buckets[u"файл не прочёлся"].append((p, 0, u""))
        continue
    ln = t[:lo].count(u"\n") + 1
    a = t.rfind(u"\n", 0, lo) + 1
    b = t.find(u"\n", lo)
    src = t[a:b if b > 0 else len(t)].strip()
    rel = os.path.relpath(path, ROOT).replace(os.sep, u"/")
    if RET.search(src):
        key = u"голый return телом плеча"
    elif STR_ARM.match(src) and u"=>" in src:
        key = u"строковый литерал плеча"
    elif CHR_ARM.match(src) and u"=>" in src:
        key = u"символьный литерал плеча"
    else:
        key = u"прочее"
    buckets[key].append((rel, ln, src[:90]))

print(u"файлов с первым отказом-корзиной: %d" % len(first))
print()
for key in (u"голый return телом плеча", u"строковый литерал плеча",
            u"символьный литерал плеча", u"прочее", u"файл не прочёлся"):
    items = buckets.get(key, [])
    if not items and key in (u"прочее", u"файл не прочёлся"):
        continue
    ex = u"напр. %s:%d" % (items[0][0], items[0][1]) if items else u""
    print(u"%-26s файлов=%-3d %s" % (key, len(items), ex))
    if key in (u"прочее", u"файл не прочёлся"):
        for rel, ln, src in items:
            print(u"      %s:%d  %s" % (rel, ln, src))
print()
print(u"неразобранных строк вывода: %d" % unparsed)
