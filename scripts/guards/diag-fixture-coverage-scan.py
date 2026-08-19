# -*- coding: utf-8 -*-
"""Ядро стража check-diag-fixture-coverage.

Считает коды диагностик, которые компилятор УМЕЕТ выпускать, и сколько из них
не ловится ни одной neg-фикстурой.

ЧЕМ ЭТО ОТЛИЧАЕТСЯ ОТ ПРАВИЛА 5 (`check-test-fixture-coverage`): то правило
diff-based — оно требует фикстуру на НОВЫЙ код и ничего не знает про
накопленное. Пока его образец был слеп ко второй форме записи (№639), коды
приезжали без фикстур, и узнать сколько их можно было только счётом по всему
дереву. Здесь — этот счёт, с храповиком.

ДВЕ ФОРМЫ ЗАПИСИ КОДА, обе обязательны:
    A. `"E_FOO"`             — код отдельным литералом (таблицы, match-армы);
    B. `"[E_FOO] сообщение"` — КАНОН: код внутри текста сообщения.
Замер 2026-08-19: A — 79 кодов, B — 364, только в B — 342. Образец, знающий
одну форму, видит пятую часть поверхности.

Вывод:
    declared=<N>   всего кодов
    covered=<N>    названы хотя бы одной neg-фикстурой
    missing=<N>    без фикстуры (это и есть храповик)
Отрицательное значение missing означает, что ядро не нашло дерево.

Аргумент: <корень>
"""
import io
import os
import re
import sys

FORM_A = re.compile(r'"((?:E|W)_[A-Z0-9_]{2,})"')
FORM_B = re.compile(r'\[((?:E|W)_[A-Z0-9_]{2,})\]')
CODE_ANY = re.compile(r'\b((?:E|W)_[A-Z0-9_]{2,})\b')
MARKER = re.compile(r"EXPECT_COMPILE_(?:ERROR|WARNING)")

SRC_DIRS = ("compiler-codegen", "nova-cli")
FIXTURE_DIRS = ("spec_tests", "std")
SKIP_DIRS = ("target", ".git", "node_modules", "__pycache__")


def walk(root, ext):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fn in filenames:
            if fn.endswith(ext):
                yield os.path.join(dirpath, fn)


def read(p):
    try:
        return io.open(p, encoding="utf-8", errors="replace", newline="").read()
    except OSError:
        return ""


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."

    declared, where = set(), {}
    seen_src = False
    for sub in SRC_DIRS:
        base = os.path.join(root, sub, "src")
        if not os.path.isdir(base):
            base = os.path.join(root, sub)
        if not os.path.isdir(base):
            continue
        seen_src = True
        for p in walk(base, ".rs"):
            t = read(p)
            rel = os.path.relpath(p, root).replace("\\", "/")
            for rx in (FORM_A, FORM_B):
                for m in rx.finditer(t):
                    declared.add(m.group(1))
                    where.setdefault(m.group(1), rel)

    if not seen_src:
        sys.stdout.write("declared=-1\ncovered=-1\nmissing=-1\n")
        return 0

    covered = set()
    for sub in FIXTURE_DIRS:
        base = os.path.join(root, sub)
        if not os.path.isdir(base):
            continue
        for p in walk(base, ".nv"):
            t = read(p)
            if not MARKER.search(t):
                continue
            for m in CODE_ANY.finditer(t):
                covered.add(m.group(1))

    missing = sorted(k for k in declared if k not in covered)
    for k in missing[:15]:
        sys.stdout.write("  %-46s %s\n" % (k, where.get(k, "?")))
    if len(missing) > 15:
        sys.stdout.write("  ... and %d more\n" % (len(missing) - 15))
    sys.stdout.write("declared=%d\ncovered=%d\nmissing=%d\n"
                     % (len(declared), len(declared) - len(missing),
                        len(missing)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
