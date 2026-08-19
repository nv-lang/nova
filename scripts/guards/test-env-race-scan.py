# -*- coding: utf-8 -*-
"""Ядро check-test-env-races: одну переменную среды правит не больше одного
теста.

ЗАЧЕМ. Тесты Rust идут ПАРАЛЛЕЛЬНО в ОДНОМ процессе, а среда у процесса одна.
Две тестовые функции, правящие одну переменную, гоняются друг с другом, и
зелёное становится вопросом планировщика.

ПРЕЦЕДЕНТ (реестр №733, 2026-08-19). `march_flag_default` и
`march_flag_native_env` правили `NOVA_MARCH_NATIVE`. Локально гонка
выигрывалась годами, на CI (ubuntu-latest) проиграла — и это был ПЕРВЫЙ раз,
когда её было видно вообще: собственный набор крейта до №723 не гонял никто.

КАК ПИСАТЬ ПРАВИЛЬНО. Решение выносится в чистую функцию и проверяется без
среды; ЧТЕНИЕ среды проверяет РОВНО ОДИН тест, последовательно и в известном
порядке. Тогда гонки нет по построению, а не по везению.

Вывод:
    racy=<N>   переменных, которые правит больше одного теста
Отрицательное значение означает, что ядро не нашло исходников.

Аргумент: <корень>
"""
import collections
import io
import os
import re
import sys

SUBS = ("compiler-codegen", "nova-cli", "nova-lsp")
SKIP_DIRS = ("target", ".git", "node_modules", "__pycache__")

FN = re.compile(r"^\s*(?:pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_0-9]+)")
TESTATTR = re.compile(r"^\s*#\[\s*(?:test|tokio::test)\s*\]")
MUT = re.compile(r"env::(?:set_var|remove_var)\s*\(\s*\"([A-Z0-9_]+)\"")


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    pairs = collections.defaultdict(set)
    seen_src = False

    for sub in SUBS:
        base = os.path.join(root, sub, "src")
        if not os.path.isdir(base):
            continue
        seen_src = True
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for fn in filenames:
                if not fn.endswith(".rs"):
                    continue
                p = os.path.join(dirpath, fn)
                rel = os.path.relpath(p, root).replace("\\", "/")
                text = io.open(p, encoding="utf-8", errors="replace",
                               newline="").read()
                cur, pending = None, False
                for line in text.split("\n"):
                    if TESTATTR.match(line):
                        pending = True
                        continue
                    m = FN.match(line)
                    if m:
                        cur = m.group(1) if pending else None
                        pending = False
                    if cur:
                        for mm in MUT.finditer(line):
                            pairs[mm.group(1)].add((rel, cur))

    if not seen_src:
        sys.stdout.write("racy=-1\n")
        return 0

    bad = {v: s for v, s in pairs.items() if len(s) > 1}
    for v, s in sorted(bad.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        sys.stdout.write("  %s -- mutated by %d tests:\n" % (v, len(s)))
        for rel, fn in sorted(s):
            sys.stdout.write("      %s :: %s\n" % (rel, fn))
    sys.stdout.write("racy=%d\n" % len(bad))
    return 0


if __name__ == "__main__":
    sys.exit(main())
