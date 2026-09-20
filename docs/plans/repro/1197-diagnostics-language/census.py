# -*- coding: utf-8 -*-
"""Опись 1197, третья редакция: кириллица в ТЕКСТЕ, с кодом и БЕЗ кода.

ПОЧЕМУ ТРЕТЬЯ. Вторая считала только строки, несущие `[E_*]`/`[W_*]`, и
пропустила НОСИТЕЛЯ САМОЙ СТРОКИ 1197: `compiler-codegen/src/argbind.rs:66`
печатает «обязательный параметр `{}` не передан» и кода в своей строке НЕ несёт.
Ключ-код верен для поиска снаружи, но как ПРЕДИКАТ ОХВАТА он уже предмета:
часть диагностик собирается без кода в той же строке.

Поэтому считаются ДВА множества и оба печатаются:
  * кириллица в строковом литерале, где в ТОЙ ЖЕ строке стоит код — ключ код;
  * кириллица в строковом литерале БЕЗ кода в строке — ключ «файл:строка»,
    потому что другого устойчивого ключа у такой диагностики нет.

Литерал отделяется от комментария посимвольным проходом (как в
check-cli-output-language): комментарий пользователю не печатается.

usage: python diag-lang-census3.py <корень> <вывод>
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

ROOT = os.path.abspath(sys.argv[1])
OUT = sys.argv[2]
CYR = re.compile(r"[Ѐ-ӿ]")
NONASCII = re.compile(r"[^\x00-\x7F]")
CODE = re.compile(r"\[(E_[A-Z][A-Z0-9_]{2,}|W_[A-Z][A-Z0-9_]{2,})\]")
TERRITORIES = [("compiler-codegen/src", ".rs"), ("nova-cli/src", ".rs")]


def strings_of(line):
    """Куски внутри двойных кавычек ВНЕ комментария; список строк."""
    out, cur, in_str, esc = [], [], False, False
    i = 0
    while i < len(line):
        ch = line[i]
        if in_str:
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
                out.append("".join(cur))
                cur = []
                i += 1
                continue
            cur.append(ch)
            i += 1
            continue
        if ch == '"':
            in_str = True
            i += 1
            continue
        if ch == "/" and i + 1 < len(line) and line[i + 1] == "/":
            break
        i += 1
    if in_str and cur:
        out.append("".join(cur))   # многострочный литерал: хвост тоже текст
    return out


with_code, without_code, typo_only = {}, [], set()
files_read = lit_lines = 0

for sub, ext in TERRITORIES:
    base = os.path.join(ROOT, sub)
    if not os.path.isdir(base):
        continue
    for dirpath, dirs, names in os.walk(base):
        dirs[:] = [d for d in dirs if d not in (".git", "target")]
        for n in names:
            if not n.endswith(ext):
                continue
            p = os.path.join(dirpath, n)
            rel = os.path.relpath(p, ROOT).replace(os.sep, "/")
            try:
                text = io.open(p, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            files_read += 1
            for i, raw in enumerate(text.split("\n"), 1):
                lits = strings_of(raw)
                if not lits:
                    continue
                lit_lines += 1
                joined = " ".join(lits)
                if not NONASCII.search(joined):
                    continue
                m = CODE.search(joined)
                if CYR.search(joined):
                    if m:
                        with_code.setdefault(m.group(1), []).append((rel, i, joined.strip()[:110]))
                    else:
                        without_code.append((rel, i, joined.strip()[:110]))
                elif m:
                    typo_only.add(m.group(1))

with io.open(OUT, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(u"# Строка реестра 1197: язык текстов диагностик компилятора. ЗАМЕР, НЕ ВЕРДИКТ.\n"
             u"# Снято 2026-09-20 помощником, по заданию интегратора.\n#\n")
    fh.write(u"# ЗНАМЕНАТЕЛИ: файлов прочитано %d; строк со строковым литералом ВНЕ\n"
             u"# комментария %d.\n#\n" % (files_read, lit_lines))
    fh.write(u"# КИРИЛЛИЦА В ТЕКСТЕ, ключ КОД:            %d кодов\n" % len(with_code))
    fh.write(u"# КИРИЛЛИЦА В ТЕКСТЕ, кода в строке НЕТ:   %d мест\n" % len(without_code))
    fh.write(u"# только типографика (`—`, `§`, «»), кириллицы нет: %d кодов — НЕ долг,\n"
             u"# правило требует английского, а не ASCII.\n#\n" % len(typo_only))
    fh.write(u"# ПОЧЕМУ ДВА МНОЖЕСТВА. Носитель самой строки 1197 —\n"
             u"# `compiler-codegen/src/argbind.rs:66` — кода в своей строке НЕ несёт,\n"
             u"# и опись, построенная ТОЛЬКО по кодам, его пропускает. Ключ-код верен\n"
             u"# для поиска снаружи, но как предикат охвата он уже предмета.\n\n")
    fh.write(u"## Кириллица при названном коде\n\n")
    for c in sorted(with_code):
        rel, i, frag = with_code[c][0]
        fh.write(u"%-44s %s:%d\n" % (c, rel, i))
        if len(with_code[c]) > 1:
            fh.write(u"    (ещё вхождений: %d)\n" % (len(with_code[c]) - 1))
    fh.write(u"\n## Кириллица без кода в строке (ключ — файл:строка)\n\n")
    for rel, i, frag in sorted(without_code):
        fh.write(u"%s:%d\n    %s\n" % (rel, i, frag))

print(u"файлов %d; строк с литералом %d; кириллица с кодом %d кодов; без кода %d мест; типографика %d кодов"
      % (files_read, lit_lines, len(with_code), len(without_code), len(typo_only)))
print(u"опись: %s" % OUT)
