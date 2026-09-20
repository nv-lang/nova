# -*- coding: utf-8 -*-
u"""Зависит ли вердикт чекера от ПОРЯДКА ТЕЛ в единице компиляции.

Задание окна Карины, 2026-09-20. Сборка — правилом из её же пробы
`docs/plans/repro/peers-one-unit/peers_probe.py`: одна строка `module`, затем
импорты (с дедупликацией, как там), затем тела; добавлен параметр ПОРЯДКА
файлов.

Три раскладки, а не две — слово заказчика, и оно верное: две точки дают прямую
через что угодно, третья отличает «зависимости нет» от «совпало».
  * алфавит;
  * обратный;
  * поворот на один (первый файл уходит в конец).

Сравнивается МУЛЬТИМНОЖЕСТВО пар `(code, message)`, СМЕЩЕНИЯ ИГНОРИРУЮТСЯ:
смещения обязаны меняться при перестановке тел — это арифметика, а не находка,
и сравнение по ним дало бы «различается всё» при пустом выводе. Число диагностик
тоже мерится, но отдельно: одинаковое число при разном наборе — самый интересный
случай, и он виден только так.

Вердиктов нет: разошлись наборы или нет — факт, «дефект это или законная
зависимость» решает окно Карины.
"""
import glob
import io
import json
import os
import subprocess
import sys
from collections import Counter

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
EXE = u"./novac/target/novac.exe"
PAD = os.path.dirname(os.path.abspath(__file__))
MODULES = (u"parse", u"check")

env = dict(os.environ)
env["NOVAC_SELF_PATH"] = "novac/src"


def peers(module):
    fs = sorted(glob.glob(os.path.join(ROOT, u"novac/src/%s/*.nv" % module)))
    return [f.replace(os.sep, u"/") for f in fs if not f.endswith(u"_test.nv")]


def build(files, out_path):
    u"""Сборка по правилу единицы: module, импорты, тела — в ПОРЯДКЕ files."""
    module_line, imports, bodies = None, [], []
    for p in files:
        for l in io.open(p, encoding="utf-8").read().split(u"\n"):
            st = l.strip()
            if st.startswith(u"module "):
                module_line = module_line or l
                continue
            if st.startswith(u"import "):
                if l not in imports:
                    imports.append(l)
                continue
            bodies.append(l)
    io.open(out_path, "w", encoding="utf-8", newline="\n").write(
        u"\n".join([module_line] + imports + [u""] + bodies) + u"\n")
    return len(imports), len(bodies)


def check(unit_path):
    p = subprocess.run([EXE, "check", unit_path], cwd=ROOT, env=env,
                       capture_output=True, timeout=600)
    out = (p.stdout or b"").decode("utf-8", "replace")
    pairs = Counter()
    unparsed = 0
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except ValueError:
            # Неразбираемая строка СЧИТАЕТСЯ: молчаливый пропуск занизил бы
            # набор ровно там, где вывод сломан.
            unparsed += 1
            pairs[(u"(неразбираемая строка)", line[:80])] += 1
            continue
        pairs[(d.get(u"code", u"?"), d.get(u"message", u""))] += 1
    return pairs, unparsed


def layouts(fs):
    # Имена раскладок ASCII: они идут в ИМЯ ФАЙЛА единицы, а не-ASCII в пути
    # ломает запуск МОЛЧА — поймано на себе в первом же прогоне: вывод стал
    # одной неразбираемой строкой вместо четырёх диагностик, и без счёта
    # неразбираемых это прошло бы за «диагностик 1, наборы совпали».
    return ((u"alpha", list(fs)),
            (u"reverse", list(reversed(fs))),
            (u"rotate", fs[1:] + fs[:1]))


summary = []
for module in MODULES:
    fs = peers(module)
    print(u"\n== модуль %s: файлов %d ==" % (module, len(fs)))
    res = {}
    for name, order in layouts(fs):
        unit = os.path.join(PAD, u"unit_%s_%s.nv" % (module, name))
        n_imp, n_bod = build(order, unit)
        pairs, unparsed = check(unit)
        res[name] = pairs
        print(u"   %-9s диагностик %4d (импортов %d, строк тел %d)%s"
              % (name, sum(pairs.values()), n_imp, n_bod,
                 u", НЕРАЗБИРАЕМЫХ %d" % unparsed if unparsed else u""))

    base = res[u"alpha"]
    diffs = []
    for name in (u"reverse", u"rotate"):
        d = (base - res[name]) + (res[name] - base)
        diffs.append((name, d))
    total_diff = sum(sum(d.values()) for _n, d in diffs)
    summary.append((module,
                    [sum(res[n].values()) for n, _ in layouts(fs)],
                    total_diff, diffs))

print(u"\n=== ИТОГ ===")
for module, nums, total_diff, diffs in summary:
    print(u"%-6s alpha=%d reverse=%d rotate=%d   наборы (code,message): %s"
          % (module, nums[0], nums[1], nums[2],
             u"СОВПАЛИ" if total_diff == 0
             else u"РАЗОШЛИСЬ на %d пар" % total_diff))
    for name, d in diffs:
        for (code, msg), cnt in list(d.items())[:3]:
            print(u"      пример расхождения (%s): %s | %s | x%d"
                  % (name, code, msg[:90], cnt))
