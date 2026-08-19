# -*- coding: utf-8 -*-
"""Ядро check-std-module-coverage: модули `std/src/**` без единого теста.

ЗАЧЕМ. База известных отказов (`std-test-fail.baseline`) сторожит тесты,
которые ПАДАЮТ. Модуль, у которого тестов нет ВОВСЕ, ей невидим по
построению: непокрытость неотличима от исправности — ровно то, о чём запись
№471 («целый модуль выпадает из проверки, а гейт остаётся зелёным»).

МОДУЛЬ = ПАПКА. Модель модулей Nova: папка — один модуль из равноправных
файлов, значит `std/src/runtime/string` это отдельный модуль, а не часть
`std/src/runtime`, и тесты ему полагаются свои (конвенция «тесты std рядом с
модулем»).

NEG-КАТАЛОГИ НЕ СЧИТАЮТСЯ. Папка `neg` (и `*_neg`) держит фикстуры на
compile-error: у них нет и не может быть блока `test "…"`, их проверяет
отдельный лейн. Требовать от них теста значило бы завести страж, который
заставляет писать бессмыслицу.

Вывод:
    modules=<N>   всего папок-модулей с `.nv`
    covered=<N>   с хотя бы одним `test "…"`
    bare=<N>      без единого (это и есть храповик)
Отрицательное значение bare означает, что ядро не нашло дерева.

Аргумент: <корень>
"""
import io
import os
import re
import sys

TEST = re.compile(r"^\s*test\s+\"")
SKIP_DIRS = (".git", "target", "node_modules", "__pycache__")


def is_neg_dir(rel):
    last = rel.rstrip("/").split("/")[-1]
    return last == "neg" or last.endswith("_neg")


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    base = os.path.join(root, "std", "src")
    if not os.path.isdir(base):
        sys.stdout.write("modules=-1\ncovered=-1\nbare=-1\n")
        return 0

    mods = {}
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        nv = [f for f in filenames if f.endswith(".nv")]
        if not nv:
            continue
        rel = os.path.relpath(dirpath, root).replace("\\", "/")
        if is_neg_dir(rel):
            continue
        has_test = False
        for f in nv:
            try:
                t = io.open(os.path.join(dirpath, f), encoding="utf-8",
                            errors="replace", newline="").read()
            except OSError:
                continue
            if any(TEST.match(l) for l in t.split("\n")):
                has_test = True
                break
        mods[rel] = has_test

    bare = sorted(m for m, v in mods.items() if not v)
    for m in bare:
        sys.stdout.write("  %s\n" % m)
    sys.stdout.write("modules=%d\ncovered=%d\nbare=%d\n"
                     % (len(mods), len(mods) - len(bare), len(bare)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
