# -*- coding: utf-8 -*-
"""1187, подготовка второй партии: УСЛОВИЕ у места печати, без единого прогона.

Зачем читать условия заранее. Первая партия показала цену догадки: из пятнадцати
проб восемь не дошли, и шесть из восьми оказались живыми, как только условие было
ПРОЧИТАНО (перепутанный порядок `ro const`, позиция поля, синтаксис границы без
двоеточия). Чтение стоит секунды, прогон — машину, а машина сейчас занята чужим
ярусом.

Что собирается на каждый код: место печати, функция и БЛИЖАЙШЕЕ УСЛОВИЕ выше —
строки `if`/`match`/`else if`/`while`, попавшие в окно над местом печати. Это не
разбор, а выписка: решение, какая форма нужна, принимает человек.

usage: python conditions.py <корень> <карта print-sites.txt> <сколько> <вывод>
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

ROOT, MAP, N, OUT = os.path.abspath(sys.argv[1]), sys.argv[2], int(sys.argv[3]), sys.argv[4]
DONE = {
    "E_BINDING_REQUIRES_INIT", "E_AMP_LITERAL", "E_ADDR_OF_NON_LVALUE",
    "E_ARRAY_INDEX_PTR_BANNED", "E_CALL_NOT_CALLABLE", "E_AMP_RECORD_LITERAL",
    "E_CONST_FIELD_IN_LITERAL", "E_BAD_FORMAT_SPEC", "E_BOUND_UNKNOWN",
    "E_BOUND_NOT_PROTOCOL", "E_TYPE_ARITY_MISMATCH", "E_CONST_RO_REDUNDANT",
    "E_CONST_MUT_CONFLICT", "E_CONST_CONSUME_CONFLICT", "E_LINT_EXPECT_NO_REASON",
}
COND = re.compile(r"^\s*(?:\}\s*)?(?:else\s+)?(if|match|while)\b|^\s*\.filter\(|^\s*=>\s*")

rows = []
for line in io.open(MAP, encoding="utf-8", errors="replace"):
    m = re.match(r"^(E_[A-Z0-9_]+|W_[A-Z0-9_]+)\s+(\S+)\s+(\S+):(\d+)\s+(\S+)", line)
    if m and m.group(1) not in DONE and m.group(2) == "literal":
        rows.append((m.group(1), m.group(3), int(m.group(4)), m.group(5)))
rows = rows[:N]

cache = {}
out_rows = []
for code, rel, ln, fn in rows:
    p = os.path.join(ROOT, rel)
    if rel not in cache:
        try:
            cache[rel] = io.open(p, encoding="utf-8", errors="replace").read().split("\n")
        except OSError:
            cache[rel] = []
    src = cache[rel]
    conds = []
    for i in range(max(0, ln - 16), ln - 1):
        s = src[i]
        if COND.search(s) and "//" not in s.strip()[:2]:
            conds.append((i + 1, s.strip()[:110]))
    out_rows.append((code, rel, ln, fn, conds[-3:]))

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# 1187, подготовка партии 2: условие у места печати (ЧТЕНИЕ, без прогонов)\n")
    fh.write(u"# Кодов выписано %d. Окно поиска условия — 15 строк над местом печати,\n"
             u"# берутся три ближайших `if`/`match`/`while`.\n#\n"
             u"# ЭТО ВЫПИСКА, А НЕ РАЗБОР: какая форма нужна, решает человек, читая\n"
             u"# условие целиком в файле. Цель — чтобы проба писалась по условию, а не\n"
             u"# по догадке: в партии 1 догадка стоила восьми недошедших из пятнадцати.\n\n"
             % len(out_rows))
    for code, rel, ln, fn, conds in out_rows:
        fh.write(u"%s\n    место: %s:%d   функция: %s\n" % (code, rel, ln, fn))
        if conds:
            for cl, text in conds:
                fh.write(u"    условие %d: %s\n" % (cl, text))
        else:
            fh.write(u"    условие: в окне 15 строк не найдено — читать функцию целиком\n")
        fh.write(u"\n")

print(u"выписано кодов: %d; вывод: %s" % (len(out_rows), OUT))
