# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-file-size.py — тысяча строк на файл (274 реш. 12).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19): ни один файл
`novac/src/**/*.nv` не длиннее 1000 строк. Решение 12 плана 274 — жёсткое, без
базлайна и без исключений: файл режется по смыслу на со-равные файлы того же
модуля (папка = один модуль).

ПОЧЕМУ PYTHON: shell-редакция поднимала `wc -l` на КАЖДЫЙ файл — 2.0с на дереве
из 32 файлов, из которых работой не было ничего. Один процесс делает то же за
доли секунды (П14).

$1 — корень репозитория; $2 — override пути к novac/src (шов самотеста).
"""
import pathlib
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-file-size"
LIMIT = 1000


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    bad, n = [], 0
    for p in sorted(src.rglob("*.nv")):
        n += 1
        # `wc -l` считает ПЕРЕВОДЫ строк, а не строки: файл без хвостового
        # перевода даёт на единицу меньше. Считаем так же, иначе вердикт
        # разойдётся с прежней редакцией на файлах без завершающей пустой.
        text = p.read_bytes()
        lines = text.count(b"\n")
        if lines > LIMIT:
            bad.append(f"  {p.as_posix()} — {lines} строк")

    if bad:
        print(f"{NAME}: FAIL — файлы длиннее {LIMIT} строк (274 реш. 12, §10.4):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Чинить так: резать файл по смыслу на со-равные файлы того же модуля", file=sys.stderr)
        print("  (папка = один модуль), не заводить базлайн и не просить исключение.", file=sys.stderr)
        return 1

    if n == 0:
        print(f"{NAME} ok: судить нечего (0 .nv-файлов в {src})")
        return 0

    print(f"{NAME} ok: файлов {n}, все не длиннее {LIMIT} строк")
    return 0


if __name__ == "__main__":
    sys.exit(main())
