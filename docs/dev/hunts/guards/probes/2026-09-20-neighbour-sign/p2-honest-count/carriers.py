# -*- coding: utf-8 -*-
"""Ищет ЖИВЫХ носителей находки 2: самотест, у которого в строке вердикта
есть и рукописное число, и знак $ (то есть страж его пропускает).
Запуск: python carriers.py <КОРЕНЬ-РЕПОЗИТОРИЯ>"""
import re, os, io, sys

root = sys.argv[1]
RE_OK_LINE = re.compile(
    r'(?:^|;|&&|\|\||\{|\bthen\b|\belse\b|\bdo\b)\s*'
    r'(?:echo|printf)\s+"[^"]*\b(?:ok|OK|PASS)\b[^"]*"')
RE_LIT = re.compile(r"\b\d+\s*/\s*\d+\b|\b\d+\s+(?:случа|"
                    r"свойств|properties|cases|assert)")
n = 0
for d in ("scripts/guards/selftest", "scripts/selftest"):
    dd = os.path.join(root, d)
    if not os.path.isdir(dd):
        continue
    for fn in sorted(os.listdir(dd)):
        if not (fn.startswith("test-") and fn.endswith(".sh")):
            continue
        p = os.path.join(dd, fn)
        for i, l in enumerate(io.open(p, encoding="utf-8", errors="replace"), 1):
            m = RE_OK_LINE.search(l)
            if not m:
                continue
            t = m.group(0)
            if "$" in t and RE_LIT.search(t):
                print("%s/%s:%d: %s" % (d, fn, i, t.strip()[:130]))
                n += 1
print("carriers: %d" % n)
