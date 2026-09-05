#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-novac-module-decl-uniform.py — папка novac/src/<m> объявляет ОДИН
# модуль во всех своих файлах, и это `novac.<m>`. План: docs/plans/274-novac-self-hosted-compiler.md
# §10.3а; архитектура docs/dev/novac-architecture.md §3 («папка — один модуль co-equal
# файлов»). Самотест: selftest/test-check-novac-module-decl-uniform.sh.
"""ЗАЧЕМ. 2026-09-05: шесть файлов `novac/src/emit_c/` объявляли `module emit_c`, четыре —
`module novac.emit_c`. Через тест-файлы папка компилировалась (мой гейт зелен), через
каталог — `nova test novac/src` — каждый файл судился своим входом, и двенадцать падали
`E_D78_MODULE_PATH_MISMATCH`: расколотая папка перестаёт быть folder-модулем. Интегратор
потратил на это вечер; ни один страж раскола не видел, потому что модульные тесты
входят списком файлов. Расколотая папка — не стиль, а другая программа.

ЧТО СЧИТАЕТ: для каждой папки `novac/src/<m>` (без корня) — множество строк `module …`
по всем её `.nv` (тесты включительно: они co-equal файлы того же модуля).
ЧТО КРАСНИТ: (1) в папке больше одного объявления; (2) объявление не равно `novac.<m>`;
(3) файл без строки `module` в первых 40 строках; (4) ноль папок или ноль файлов под
судом — мишень потеряна, не «нарушений 0».

Аргументы: $1 — корень репозитория; $2 — override каталога исходников (шов самотеста).
Вход для гейта — main(): run-guards.py зовёт именно её.
"""
import io
import os
import re
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")

NAME = "check-novac-module-decl-uniform"
RE_MODULE = re.compile(r"^\s*module\s+([A-Za-z_][A-Za-z0-9_.]*)\s*$")


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def module_of(path):
    with io.open(path, encoding="utf-8", errors="replace") as f:
        for i, line in enumerate(f):
            if i >= 40:
                break
            m = RE_MODULE.match(line)
            if m:
                return m.group(1)
    return None


def main():
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    src = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.path.join(root, "novac", "src")
    if not os.path.isdir(src):
        print("%s ok: судить нечего (нет %s)" % (NAME, src))
        return 0

    folders = 0
    files = 0
    bad = []
    for d in sorted(os.listdir(src)):
        dp = os.path.join(src, d)
        if not os.path.isdir(dp):
            continue
        nvs = sorted(fn for fn in os.listdir(dp) if fn.endswith(".nv"))
        if not nvs:
            continue
        folders += 1
        files += len(nvs)
        want = "novac.%s" % d
        seen = {}
        for fn in nvs:
            mod = module_of(os.path.join(dp, fn))
            rel = "novac/src/%s/%s" % (d, fn)
            if mod is None:
                bad.append("  %s — нет строки `module` в первых 40 строках" % rel)
                continue
            seen.setdefault(mod, []).append(fn)
            if mod != want:
                bad.append("  %s — объявляет `module %s`, папка требует `module %s`" % (rel, mod, want))
        if len(seen) > 1:
            parts = ", ".join("`%s` (%d)" % (k, len(v)) for k, v in sorted(seen.items()))
            bad.append("  novac/src/%s — РАСКОЛ: %s — папка перестала быть одним модулем" % (d, parts))

    if folders == 0 or files == 0:
        return fail("под судом ни одной папки с .nv в %s — мишень потеряна, а не «нарушений 0»" % src)
    if bad:
        sys.stderr.write("%s: FAIL — объявление модуля в папке не единообразно (папка = один модуль co-equal файлов):\n" % NAME)
        for b in bad:
            sys.stderr.write(b + "\n")
        sys.stderr.write("  Через тест-файлы расколотая папка ещё компилируется, через каталог — нет (E_D78,\n"
                         "  2026-09-05). Одно объявление на папку: `module novac.<папка>` во всех её файлах.\n")
        return 1
    print("%s ok: папок %d, файлов .nv %d — в каждой папке одно объявление модуля формы novac.<папка>"
          % (NAME, folders, files))
    return 0


if __name__ == "__main__":
    sys.exit(main())
