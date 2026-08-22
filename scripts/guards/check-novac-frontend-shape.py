# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-frontend-shape.py — фронтенд не возвращает Result
(план 274 §4 п.1; архитектура §6).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19): в модулях
фронтенда (`lex`, `parse`, `tree`, `syntax`) ни одна ЭКСПОРТИРОВАННАЯ функция не
объявляет `-> Result[`. Форма фронтенда — пара «(узел, диагностики)»: ошибки
живут ДАННЫМИ рядом с результатом, а не альтернативой ему, иначе разбор
обрывается на первой же ошибке и пользователь видит одну вместо всех.

ПОЧЕМУ PYTHON: shell-редакция поднимала два `grep` на КАЖДЫЙ файл — 6.6с там,
где работы на доли секунды (П14).

$1 — корень репозитория; $2 — override пути к novac/src (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-frontend-shape"
FRONTEND = ("lex", "parse", "tree", "syntax")

EXPORT_FN = re.compile(r"^[ \t]*export[ \t]+fn[ \t]")
RESULT_SIG = re.compile(r"^[ \t]*export[ \t]+fn[ \t].*->[ \t]*Result[ \t]*\[")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    files = []
    for d in FRONTEND:
        sub = src / d
        if sub.is_dir():
            files.extend(sorted(p for p in sub.glob("*.nv")))

    if not files:
        print(NAME + " ok: судить нечего (нет .nv во фронтенд-модулях novac/src/{lex,parse,tree,syntax}; файлов 0)")
        return 0

    bad, nexports = [], 0
    for p in files:
        for i, raw in enumerate(p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
            if EXPORT_FN.match(raw):
                nexports += 1
            if RESULT_SIG.match(raw):
                bad.append(f"  {p.as_posix()}:{i}:{raw}")

    if bad:
        print(f"{NAME}: FAIL — Result в экспортированных сигнатурах фронтенда:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Фронтенд не возвращает Result: форма — пара «(узел, диагностики)»,", file=sys.stderr)
        print("  ошибки — данные рядом с результатом, не альтернатива ему.", file=sys.stderr)
        print("  План 274 §4 п.1; архитектура §6.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов {len(files)}, экспортов fn {nexports}, '-> Result[' во фронтенде: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
