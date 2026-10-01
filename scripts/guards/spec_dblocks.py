# -*- coding: utf-8 -*-
"""scripts/guards/spec_dblocks.py -- ОБЩИЙ разбор D-блоков spec/decisions/*.md.

Дом разбора заголовков и указателей `>` под заголовком блока. Раньше жил внутри
check-dblock-supersede-pointers.py; стражи check-spec-amend-places.py (место
правила названо при амендменте) и check-spec-code-home.py (у кода диагностики
один дом) судят тем же разбором, и копия разошлась бы с оригиналом при первой
правке. Дублировать разбор в стражах нельзя -- импортируй отсюда.
"""
import glob
import io
import os
import re

HEAD = re.compile(r"^(#{2,3}) D(\d+)\b")
H2 = re.compile(r"^## D(\d+)\b")
SECTION = re.compile(r"^### (Что заменено|Supersedes)\b")
LOOKAHEAD = 8


def read_lines(path):
    with io.open(path, encoding="utf-8", errors="replace") as f:
        return f.read().splitlines()


def decision_files(root):
    return sorted(glob.glob(os.path.join(root, "spec", "decisions", "*.md")))


def scan(root):
    """-> (files, headings, supers).
    headings: номер(str) -> [(файл, номер строки 1-based, строки файла)]
    supers: [Dnew, файл, строка, [Dold...]] по разделам «Что заменено»."""
    files = decision_files(root)
    headings = {}
    supers = []
    for path in files:
        lines = read_lines(path)
        rel = os.path.relpath(path, root).replace("\\", "/")
        fence = False
        cur = None
        sec = None
        for i, ln in enumerate(lines):
            if ln.lstrip().startswith("```"):
                fence = not fence
            if fence:
                if sec is not None:
                    sec[3].extend(re.findall(r"\bD(\d+)\b", ln))
                continue
            m = HEAD.match(ln)
            if m:
                headings.setdefault(m.group(2), []).append((rel, i + 1, lines))
            m2 = H2.match(ln)
            if ln.startswith("## "):
                cur = m2.group(1) if m2 else None
                if sec is not None:
                    supers.append(sec)
                    sec = None
            elif ln.startswith("### "):
                if sec is not None:
                    supers.append(sec)
                    sec = None
                if cur and SECTION.match(ln):
                    sec = [cur, rel, i + 1, []]
            elif sec is not None:
                sec[3].extend(re.findall(r"\bD(\d+)\b", ln))
        if sec is not None:
            supers.append(sec)
    return files, headings, supers


def has_pointer(lines, idx, new):
    """В первых LOOKAHEAD непустых строках под заголовком (idx -- 0-based) есть строка
    на `>`, называющая D<new>."""
    seen = 0
    pat = re.compile(r"\bD%s\b" % new)
    for ln in lines[idx + 1:]:
        if not ln.strip():
            continue
        seen += 1
        if seen > LOOKAHEAD:
            break
        if ln.lstrip().startswith(">") and pat.search(ln):
            return True
    return False


def block_texts(root):
    """номер(int) -> текст блока: от заголовка `## Dn`/`### Dn` до следующего заголовка D
    или `## `. Подзаголовки `### Что заменено` остаются в своём блоке."""
    out = {}
    for path in decision_files(root):
        cur = None
        for ln in read_lines(path):
            m = HEAD.match(ln)
            if m:
                cur = int(m.group(2))
                out.setdefault(cur, [])
            elif ln.startswith("## "):
                cur = None
            if cur is not None:
                out[cur].append(ln)
    return {n: "\n".join(ls) for n, ls in out.items()}
