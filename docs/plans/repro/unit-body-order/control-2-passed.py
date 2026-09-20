# -*- coding: utf-8 -*-
u"""Контроль, вторая попытка — ПЕРВАЯ ПРОВАЛИЛАСЬ, и это записано намеренно.

Первый контроль убирал ПОСЛЕДНИЙ файл единицы и не дал разницы. Вывод оттуда
неоднозначен: либо сравнение слепо, либо у выброшенного файла просто нет своих
диагностик — то есть контроль мерил не то, что обещал.

Здесь контроль однозначный: в единицу дописывается строка с заведомо негодным
текстом. Новая пара `(code, message)` обязана появиться; если не появится,
сравнение действительно слепо и вывод про порядок недействителен.

Плюс, чтобы объяснить провал первого: печатается, ИЗ КАКИХ файлов приходят
диагностики полной единицы — по смещению в собранном тексте.
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


def build(files, out_path, extra=u""):
    u"""Возвращает карту «смещение -> файл», чтобы диагностику можно было
    приписать её источнику, а не гадать."""
    module_line, imports, bodies, marks = None, [], [], []
    for p in files:
        start_line = len(bodies)
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
        marks.append((start_line, os.path.basename(p)))
    text = u"\n".join([module_line] + imports + [u""] + bodies) + u"\n" + extra
    io.open(out_path, "w", encoding="utf-8", newline="\n").write(text)
    head = len(u"\n".join([module_line] + imports + [u""])) + 1
    offs = []
    pos = head
    for i, l in enumerate(bodies):
        offs.append(pos)
        pos += len(l) + 1
    return text, marks, offs


def check(unit):
    p = subprocess.run([EXE, "check", unit], cwd=ROOT, env=env,
                       capture_output=True, timeout=600)
    out = (p.stdout or b"").decode("utf-8", "replace")
    pairs, spans = Counter(), []
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
            pairs[(d.get(u"code", u"?"), d.get(u"message", u""))] += 1
            spans.append(d.get(u"primary", {}).get(u"start", -1))
        except ValueError:
            pairs[(u"(unparsed)", line[:80])] += 1
    return pairs, spans


fs = peers(u"parse")
full = os.path.join(PAD, u"ctl2_full.nv")
broken = os.path.join(PAD, u"ctl2_broken.nv")
_t, marks, offs = build(fs, full)
build(fs, broken, extra=u"fn ??? broken( -> { }\n")

a, sa = check(full)
b, _sb = check(broken)
d = (a - b) + (b - a)
print(u"polnaya: par %d; s dopisannoy negodnoy strokoy: par %d" % (sum(a.values()), sum(b.values())))
print(u"raznitsa: %d -> sravnenie %s"
      % (sum(d.values()), u"VIDIT raznitsu" if d else u"SLEPO"))
for (code, msg), cnt in list(d.items())[:3]:
    print(u"   %s | %s | x%d" % (code, msg[:90], cnt))

print(u"\niz kakih faylov diagnostiki polnoy edinitsy (po smeshcheniyu):")
for s in sa:
    owner = u"?"
    for i, off in enumerate(offs):
        if off <= s:
            owner_idx = i
        else:
            break
    for start_line, name in marks:
        if start_line <= owner_idx:
            owner = name
    print(u"   smeshchenie %6d -> %s" % (s, owner))
