# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-grammar-excuse.py — отказ не ссылается на
незнание грамматики (274 §9.4).

ПОЧЕМУ. Парсер читает ЯЗЫК целиком; «вне подмножества» решает чекер и обязан
НАЗВАТЬ форму и этап: «outside the subset: a variadic parameter ... arrives with
generics (E2-b)». Отказ «construct not in the MVP grammar» не говорит ни что
написано не так, ни когда это заработает — он сообщает о СЕБЕ, а не о программе.
А если форма и правда не читается, это СИНТАКСИЧЕСКАЯ ошибка, и говорить надо
так (SYNTAX_MSG), а не про подмножество.

СУДИТ ТОЛЬКО СТРОКОВЫЕ ЛИТЕРАЛЫ: в комментарии история класса законна.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-grammar-excuse"
EXCUSES = (re.compile(r'"[^"]*MVP grammar[^"]*"'),
           re.compile(r'"[^"]*not in the grammar[^"]*"'),
           re.compile(r'"[^"]*unknown construct[^"]*"'))


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень", file=sys.stderr)
        return 1

    bad = []
    total = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = f.read_bytes().decode("utf-8", "replace").split("\n")
        if lines and lines[-1] == "":
            lines.pop()
        for n, line in enumerate(lines, 1):
            if line.endswith("\r"):
                line = line[:-1]
            s = line.lstrip(" \t\v\f")
            # комментарий или док — не судим: история класса там законна
            if s.startswith("//"):
                continue
            if any(rx.search(line) for rx in EXCUSES):
                bad.append(f"  {rel}:{n} — отказ ссылается на незнание грамматики: {s[:72]}")
            total += 1

    if bad:
        print(f"{NAME}: FAIL — диагностика ссылается на незнание грамматики (274 §9.4):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Парсер читает ЯЗЫК целиком; «вне подмножества» решает чекер и", file=sys.stderr)
        print("  обязан НАЗВАТЬ форму и этап: «outside the subset: a variadic", file=sys.stderr)
        print("  parameter ... arrives with generics (E2-b)».", file=sys.stderr)
        print("  Если форма и правда не читается — это СИНТАКСИЧЕСКАЯ ошибка, и", file=sys.stderr)
        print("  говорить надо так (SYNTAX_MSG), а не про подмножество.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: строк .nv: {total}, отговорок про грамматику: 0 (форму называет отказ)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
