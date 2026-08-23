# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-selftest-interpreter.py — самотест зовёт стража
ЧЕРЕЗ интерпретатор, а не напрямую (класс «зелено на Windows, мертво на Linux»;
замер 2026-08-23).

ЗАЧЕМ. Стражи в этом дереве лежат с режимом 100644 — без бита исполнения, по
соглашению репозитория. На Windows это неважно: MSYS запускает файл по шебангу.
На Linux `"$G" ...` даёт «Permission denied» (rc=126), и самотест валится ВЕСЬ
— каждый случай печатает FAIL, ни одного `ok`.

Замер 2026-08-23 (по красному CI): два самотеста из 147 звали стража напрямую —
оба мои, оба вчерашние. На CI они напечатали НОЛЬ строк `ok`, и мой же страж
счётчиков реестра доложил «реестр обещает 12 случаев, самотест печатает 0» —
то есть красный пришёл не оттуда, где ошибка. Остальные 145 зовут `bash "$G"`
или `python "$G"` (460 мест) — конвенция была, соблюдалась молча и потому не
держалась ничем.

ПРАВИЛО ПЛОСКОЕ, база ноль: прямой вызов — это не наследство, а опечатка,
которая на машине автора не видна.

ЧЕГО НЕ ПРОВЕРЯЕТ: вызовы через переменную с другим именем (конвенция — `$G`,
названная слепая зона); строки, которые ПЕЧАТАЮТ прямой вызов (`printf`/`echo`/
`cat` пишут фикстуру — это данные, а не вызов; без исключения страж краснел на
собственном самотесте, поймано первым прогоном по дереву); прочие
платформозависимые инструменты внутри самотестов (`cygpath` и родня — отдельный
класс, план 274 §9.1д К3).

$1 — корень; $2 — override каталога самотестов (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-selftest-interpreter"

# «$G» в позиции КОМАНДЫ: начало строки, после $( или ` , после && / || / ; .
RE_DIRECT = re.compile(r'(?:^|\$\(|`|&&|\|\||;)\s*"\$G"')
RE_OK = re.compile(r'\b(?:bash|sh|python|python3)\s+"\$G"')
RE_WRITES = re.compile(r"(?:printf|echo|cat)\b")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    sel = pathlib.Path(a[2]) if len(a) > 2 else root / "scripts" / "guards" / "selftest"

    if not sel.is_dir():
        print(f"{NAME} ok: судить нечего (нет {sel})")
        return 0

    files = sorted(sel.glob("*.sh"))
    if not files:
        print(f"{NAME}: FAIL — в {sel} нет ни одного самотеста: страж потерял "
              f"мишень (класс №519)", file=sys.stderr)
        return 1

    bad = []
    for f in files:
        for n, line in enumerate(f.read_text(encoding="utf-8", errors="replace").split("\n"), 1):
            s = line.strip()
            if s.startswith("#"):
                continue
            # СТРОКА, КОТОРАЯ ПЕЧАТАЕТ, НЕ ЗОВЁТ. Самотесты пишут фикстуры
            # через printf/echo/cat, и текст фикстуры законно содержит прямой
            # вызов — иначе нельзя проверить, что страж его ловит. Поймано
            # первым же прогоном по дереву: страж покраснел на СВОЁМ самотесте.
            if RE_WRITES.match(s):
                continue
            if RE_DIRECT.search(line) and not RE_OK.search(line):
                bad.append(f"  {f.name}:{n}: {s[:72]}")

    if bad:
        print(f"{NAME}: FAIL — самотест зовёт стража напрямую (мест {len(bad)}):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Стражи лежат без бита исполнения (100644). На Windows файл", file=sys.stderr)
        print("  запускается по шебангу, на Linux это rc=126 — и самотест", file=sys.stderr)
        print("  печатает НОЛЬ строк `ok`, то есть красный приходит не оттуда,", file=sys.stderr)
        print("  где ошибка (замер 2026-08-23, красный CI). Зови через", file=sys.stderr)
        print("  интерпретатор: `bash \"$G\"` или `python \"$G\"`.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: самотестов {len(files)}, прямых вызовов стража: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
