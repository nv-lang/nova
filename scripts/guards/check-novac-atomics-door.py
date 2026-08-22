# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-atomics-door.py — атомики и TLS только через дверь
`novac/src/atomics/` (274 §8.1, архитектура).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19): вне каталога
`atomics/` в novac/src не встречается ни `__atomic_`, ни `thread_local`, ни
`nova_atomic_` — ни в коде, ни в эмитируемых строках. Примитив живёт за дверью,
остальные зовут обёртку.

ПОЧЕМУ PYTHON: shell-редакция поднимала `grep` на КАЖДЫЙ файл (и судит она ВСЕ
файлы, не только .nv) — 3.2с на дереве, из которых работой не было ничего.

$1 — корень репозитория; $2 — override пути к novac/src (шов самотеста).
"""
import pathlib
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-atomics-door"
NEEDLES = ("__atomic_", "thread_local", "nova_atomic_")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    bad, n = [], 0
    for p in sorted(src.rglob("*")):
        if not p.is_file():
            continue
        # дверь: всё, что лежит в atomics/, не судится
        if "atomics" in p.relative_to(src).parts:
            continue
        n += 1
        try:
            text = p.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for i, line in enumerate(text.replace("\r", "").split("\n"), 1):
            if any(x in line for x in NEEDLES):
                bad.append(f"  {p.as_posix()}:{i}:{line}")

    if bad:
        print(f"{NAME}: FAIL — атомики/TLS мимо двери (274 §8.1, архитектура):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Чинить так: перенести примитив в novac/src/atomics/ и звать", file=sys.stderr)
        print("  обёртку оттуда; '__atomic_'/'thread_local'/'nova_atomic_' вне", file=sys.stderr)
        print("  двери не живут — ни в коде, ни в эмитируемых строках.", file=sys.stderr)
        return 1

    if n == 0:
        print(f"{NAME} ok: судить нечего (вне atomics/ нет файлов в {src})")
        return 0

    print(f"{NAME} ok: файлов вне двери {n}, атомики/TLS только через atomics/")
    return 0


if __name__ == "__main__":
    sys.exit(main())
