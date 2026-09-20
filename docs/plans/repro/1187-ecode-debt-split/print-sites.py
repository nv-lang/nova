# -*- coding: utf-8 -*-
"""1187: место, где код долга попадает в диагностику, И МЕХАНИЗМ этого попадания.

ПЕРВАЯ РЕДАКЦИЯ ИСКАЛА ТОЛЬКО ЛИТЕРАЛ `[КОД]` в строке — и объявила «без места
печати» двенадцать кодов. Чтение показало, что это была СЛЕПОТА СКАНЕРА, а не
свойство кодов: механизмов попадания ЧЕТЫРЕ, и литерал лишь один из них.

  * `literal`  — `"[E_X] текст"` прямо в сообщении;
  * `interp`   — `format!("[{code}] …")`, код подставляется переменной
                 (`parser/mod.rs:5514`, тройка const-конфликтов);
  * `const`    — `pub const E_X: &str = "E_X"`, код живёт значением;
  * `lint`     — `rule: "E_X"`, правило линта; канал вывода другой (`--lint`);
  * `table`    — `("E_X", …)` элементом таблицы диагностик;
  * `comment`  — имя встречается ТОЛЬКО в комментариях: места сборки нет.

Последний исход — единственный, который даёт право говорить «носителя нет», и
даже он проверяется чтением обеих строк, а не числом.

usage: python print-sites.py <корень> <база> <вывод>
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

ROOT, BASE, OUT = os.path.abspath(sys.argv[1]), sys.argv[2], sys.argv[3]
FN = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
DIRS = ("compiler-codegen/src", "nova-cli/src")

debt = []
for line in io.open(BASE, encoding="utf-8", errors="replace"):
    s = line.split("#", 1)[0].strip()
    if s.startswith("debt_code="):
        debt.append(s.split("=", 1)[1].strip())
debt = sorted(set(debt))


def code_part(line):
    """Строка без хвостового `//`-комментария; `//` внутри литерала сохраняется."""
    out, in_str, esc, i = [], False, False, 0
    while i < len(line):
        ch = line[i]
        if in_str:
            out.append(ch)
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
            i += 1
            continue
        if ch == '"':
            in_str = True
            out.append(ch)
            i += 1
            continue
        if ch == "/" and i + 1 < len(line) and line[i + 1] == "/":
            break
        out.append(ch)
        i += 1
    return "".join(out)


def mechanism(code, line):
    """Механизм попадания кода в диагностику — по ОДНОЙ исполнимой строке."""
    if "[%s]" % code in line:
        return "literal"
    if re.search(r'rule\s*:\s*"%s"' % code, line):
        return "lint"
    if re.search(r'const\s+%s\s*:\s*&str' % code, line) or \
       re.search(r'=\s*"%s"\s*;' % code, line):
        return "const"
    if re.search(r'\(\s*"%s"\s*,' % code, line):
        return "table"
    if '"%s"' % code in line:
        return "other-literal"
    if code in line:
        return "bare-name"
    return None


sites, mech = {}, {}
seen_anywhere, files_read = set(), 0
interp_sites = []
for sub in DIRS:
    base = os.path.join(ROOT, sub)
    if not os.path.isdir(base):
        continue
    for dirpath, dirs, names in os.walk(base):
        dirs[:] = [d for d in dirs if d not in (".git", "target")]
        for n in sorted(names):
            if not n.endswith(".rs"):
                continue
            p = os.path.join(dirpath, n)
            rel = os.path.relpath(p, ROOT).replace(os.sep, "/")
            try:
                lines = io.open(p, encoding="utf-8", errors="replace").read().split("\n")
            except OSError:
                continue
            files_read += 1
            fn_now = "?"
            for i, raw in enumerate(lines, 1):
                m = FN.match(raw)
                if m:
                    fn_now = m.group(1)
                for c in debt:
                    if c in raw:
                        seen_anywhere.add(c)
                exe = code_part(raw)
                if not exe:
                    continue
                # подстановка переменной: `format!("[{code}] …")` рядом с выбором кода
                if "[{code}]" in exe:
                    interp_sites.append((rel, i, fn_now))
                for c in debt:
                    if c in sites or c not in exe:
                        continue
                    mm = mechanism(c, exe)
                    if mm:
                        sites[c] = (rel, i, fn_now)
                        mech[c] = mm

comment_only = sorted(c for c in seen_anywhere if c not in sites)
never_seen = sorted(c for c in debt if c not in seen_anywhere)

by_mech = {}
for c, m in mech.items():
    by_mech.setdefault(m, []).append(c)

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# 1187: место и МЕХАНИЗМ попадания кода долга в диагностику\n")
    fh.write(u"# Вторая редакция (2026-09-20): первая искала только литерал и\n"
             u"# объявляла «без места печати» двенадцать кодов — это была слепота\n"
             u"# сканера, а не свойство кодов.\n#\n")
    fh.write(u"# ЗНАМЕНАТЕЛИ: кодов в базе %d; файлов .rs прочитано %d;\n"
             u"# с найденным механизмом %d; только в комментариях %d;\n"
             u"# не встречаются нигде %d.\n#\n"
             % (len(debt), files_read, len(sites), len(comment_only), len(never_seen)))
    for m in sorted(by_mech):
        fh.write(u"#   %-14s %d\n" % (m, len(by_mech[m])))
    fh.write(u"#\n# Мест подстановки `format!(\"[{code}] …\")` в дереве: %d\n\n"
             % len(interp_sites))
    fh.write(u"%-46s %-10s %-46s %s\n" % (u"код", u"механизм", u"место", u"функция"))
    for c in sorted(sites):
        rel, i, fn = sites[c]
        fh.write(u"%-46s %-10s %-46s %s\n" % (c, mech[c], u"%s:%d" % (rel, i), fn))
    fh.write(u"\n## Только в комментариях — места сборки нет (%d)\n\n" % len(comment_only))
    for c in comment_only:
        fh.write(u"  %s\n" % c)
    fh.write(u"\n## Не встречаются нигде (%d)\n\n" % len(never_seen))
    for c in never_seen:
        fh.write(u"  %s\n" % c)

print(u"кодов %d; с механизмом %d; только комментарии %d; нигде %d; файлов %d"
      % (len(debt), len(sites), len(comment_only), len(never_seen), files_read))
for m in sorted(by_mech):
    print(u"   %-14s %d" % (m, len(by_mech[m])))
print(u"вывод: %s" % OUT)
