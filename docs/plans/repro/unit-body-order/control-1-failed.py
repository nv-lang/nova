# -*- coding: utf-8 -*-
u"""КОНТРОЛЬ к замеру порядка: умеет ли сравнение вообще заметить разницу.

«Наборы совпали» ничего не стоит, пока не показано, что сравнение УМЕЕТ
расходиться. Здесь берётся та же единица модуля `parse` и из неё убирается ОДИН
файл: набор обязан отличаться. Если не отличится — сравнение слепо, и вывод
прошлого прогона недействителен.

Это та же проба в обе стороны, что требуется от фикстуры: сломай условие —
покрасней, восстанови — позеленей.
"""
import glob
import io
import json
import os
import subprocess
from collections import Counter

ROOT = u"D:/Sources/nv-lang/nova-wt-research"
EXE = u"./novac/target/novac.exe"
PAD = os.path.dirname(os.path.abspath(__file__))
env = dict(os.environ)
env["NOVAC_SELF_PATH"] = "novac/src"


def peers(module):
    fs = sorted(glob.glob(os.path.join(ROOT, u"novac/src/%s/*.nv" % module)))
    return [f.replace(os.sep, u"/") for f in fs if not f.endswith(u"_test.nv")]


def build(files, out_path):
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


def check(unit):
    p = subprocess.run([EXE, "check", unit], cwd=ROOT, env=env,
                       capture_output=True, timeout=600)
    out = (p.stdout or b"").decode("utf-8", "replace")
    pairs = Counter()
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
            pairs[(d.get(u"code", u"?"), d.get(u"message", u""))] += 1
        except ValueError:
            pairs[(u"(unparsed)", line[:80])] += 1
    return pairs


fs = peers(u"parse")
full = os.path.join(PAD, u"ctl_full.nv")
short = os.path.join(PAD, u"ctl_short.nv")
build(fs, full)
build(fs[:-1], short)          # тот же порядок, но БЕЗ последнего файла
a, b = check(full), check(short)
d = (a - b) + (b - a)
print(u"polnaya edinitsa: par %d, bez odnogo fayla: par %d" % (sum(a.values()), sum(b.values())))
print(u"raznitsa: %d par -> sravnenie %s"
      % (sum(d.values()), u"VIDIT raznitsu" if d else u"SLEPO"))
for (code, msg), cnt in list(d.items())[:3]:
    print(u"   %s | %s | x%d" % (code, msg[:80], cnt))
