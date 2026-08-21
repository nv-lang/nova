# -*- coding: utf-8 -*-
"""Ядро check-generic-static: `static` внутри generic-функции.

ЗАЧЕМ. Rust инстанцирует такой статик НА КАЖДУЮ МОНОМОРФИЗАЦИЮ — то есть у
каждого набора типов-параметров он СВОЙ. Для счётчика это обычно безобидно, а
для МЬЮТЕКСА смертельно: заведённый «чтобы сериализовать», он у каждого вызова
свой и не сериализует ничего.

ЧТО ЭТО СТОИЛО (реестр №736, 2026-08-19). `with_scc_env<F: FnOnce()>` держал
`static GUARD: Mutex<()>` внутри себя. Параметр там — ТИП ЗАМЫКАНИЯ, а он у
каждого вызова свой, значит у каждого теста был свой мьютекс. Тесты SCC-кэша
шли параллельно и портили друг другу глобальные счётчики: на CI
`v74_write_and_read_caches_isolated` дал left (0, 3) против right (0, 1).
Локально гонка выигрывалась.

НОЛЬ, БЕЗ ХРАПОВИКА: замер в день заведения дал РОВНО ОДИН случай на всё
дерево, и он починен. Один такой статик уже означает механизм, который
выглядит работающим и не работает.

Вывод:
    found=<N>
Отрицательное значение означает, что ядро не нашло исходников.

Аргумент: <корень>
"""
import io
import os
import re
import sys

SUBS = ("compiler-codegen", "nova-cli", "nova-lsp")
SKIP_DIRS = ("target", ".git", "node_modules", "__pycache__")
FN_GENERIC = re.compile(
    r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_0-9]+)\s*<")
STATIC = re.compile(r"^\s*static\s+[A-Z_0-9]+\s*:")


def walk(base):
    for dp, dn, fns in os.walk(base):
        dn[:] = [d for d in dn if d not in SKIP_DIRS]
        for fn in fns:
            if fn.endswith(".rs"):
                yield os.path.join(dp, fn)


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    seen_src = False
    hits = []

    for sub in SUBS:
        base = os.path.join(root, sub, "src")
        if not os.path.isdir(base):
            continue
        seen_src = True
        for p in walk(base):
            lines = io.open(p, encoding="utf-8", errors="replace",
                            newline="").read().split("\n")
            rel = os.path.relpath(p, root).replace("\\", "/")
            for i, line in enumerate(lines):
                m = FN_GENERIC.match(line)
                if not m:
                    continue
                # тело по балансу фигурных скобок; потолок в 400 строк —
                # защита от незакрытой скобки в макросе, а не оценка длины
                depth, started, j = 0, False, i
                while j < len(lines) and j < i + 400:
                    depth += lines[j].count("{") - lines[j].count("}")
                    if lines[j].count("{"):
                        started = True
                    if started and depth <= 0:
                        break
                    if started and STATIC.match(lines[j]):
                        hits.append((rel, j + 1, m.group(2),
                                     lines[j].strip()[:70]))
                    j += 1

    if not seen_src:
        sys.stdout.write("found=-1\n")
        return 0

    for rel, ln, fn, txt in hits:
        sys.stdout.write("  %s:%d  inside generic fn `%s`\n      %s\n"
                         % (rel, ln, fn, txt))
    sys.stdout.write("found=%d\n" % len(hits))
    return 0


if __name__ == "__main__":
    sys.exit(main())
