# -*- coding: utf-8 -*-
"""scripts/guards/check-no-data-in-c-format.py — данные не едут в СТРОКУ ФОРМАТА
сгенерированного C.

План: docs/plans/221.1-bug-sweep.md, строка №1073 (реестр 221.1).

ЧТО ЛОВИТ. Эмиттер строит вызов `printf` рустовым `format!`, и рустовый
плейсхолдер `{}` оказывается ВНУТРИ C-кавычек формата. Тогда текст, пришедший из
исходника пользователя (имя теста, паттерн `panics`), попадает туда, где C ищет
спецификаторы.

ЧЕМ ЭТО КОНЧАЕТСЯ — ЗАМЕРЕНО, а не предположено (2026-09-13, обратная проба на
ОДНОМ бинаре). Тест с именем `border %s and %d and %g and %% here` печатался как

    FAIL: border pct_fail_test.nv:6: assert failed: 1 == 2 and 0 and 0 and % here — (null)

`%s` в ИМЕНИ съел настоящее сообщение, `%d` и `%g` вытащили мусор, а сообщению
достался `(null)`. У трёх из шести мест после формата шли настоящие аргументы,
поэтому промах не просто печатал ерунду, а СДВИГАЛ весь список. `%n` в позиции
формата есть примитив ЗАПИСИ по указателю.

ПОЧЕМУ СТРАЖ, А НЕ ТОЛЬКО ФИКСТУРА. Фикстура держит ПОВЕДЕНИЕ шести мест,
которые были починены. Она не мешает появиться СЕДЬМОМУ: следующий автор напишет
`format!("printf(\"... {} ...\")")` и не узнает. Приёмка строки №1073 прямо
требует проверки свойством, а не перечнем носителей.

НУЛЕВАЯ ТЕРПИМОСТЬ, И ОНА ИЗМЕРЕНА: после починки таких мест НОЛЬ. База не
нужна — страж кусает только новое.

СУДИТ `compiler-codegen/src` — там живёт эмиттер. `$1` — корень, `$2` — override
каталога (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-no-data-in-c-format"

# C-вызов печати, чей формат открывается экранированной кавычкой: printf(\"...
CALL = re.compile(r'\b(printf|fprintf|sprintf|snprintf)\(\\"')


def offenders(text):
    """Строки, где рустовый плейсхолдер стоит ВНУТРИ C-строки формата."""
    out = []
    for i, line in enumerate(text.split("\n"), 1):
        for m in CALL.finditer(line):
            seg = line[m.end():]
            end = seg.find('\\"')          # конец C-строки формата
            fmt = seg[:end] if end > 0 else seg
            if "{" in fmt:
                out.append((i, " ".join(line.split())[:140]))
                break
    return out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "compiler-codegen" / "src"

    if not src.is_dir():
        print("%s ok: судить нечего (нет %s)" % (NAME, src))
        return 0

    bad = []
    seen = 0
    for dirpath, dirnames, filenames in os.walk(src):
        dirnames[:] = [d for d in dirnames if d != ".git"]
        for fn in sorted(filenames):
            if not fn.endswith(".rs"):
                continue
            p = pathlib.Path(dirpath) / fn
            seen += 1
            try:
                text = p.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            for i, line in offenders(text):
                rel = p.relative_to(root) if str(p).startswith(str(root)) else p
                bad.append((str(rel).replace("\\", "/"), i, line))

    if not bad:
        print("%s ok: файлов %d, данных в позиции формата 0" % (NAME, seen))
        return 0

    sys.stderr.write("%s: FAIL — данные в позиции C-формата: %d\n" % (NAME, len(bad)))
    for rel, i, line in bad[:15]:
        sys.stderr.write("    %s:%d\n        %s\n" % (rel, i, line))
    if len(bad) > 15:
        sys.stderr.write("    ... и ещё %d\n" % (len(bad) - 15))
    sys.stderr.write(
        "\n    Текст, пришедший из исходника пользователя, попал туда, где C ищет\n"
        "    спецификаторы. Печатай его АРГУМЕНТОМ: `printf(\"%s\", \"...\")`, а не\n"
        "    вставкой в формат. Если после формата уже идут аргументы, новый `%s`\n"
        "    обязан встать ПЕРЕД ними — иначе сдвинется весь список.\n"
        "    Замер 2026-09-13 (реестр 221.1 №1073): `%s` в имени теста съедал\n"
        "    сообщение, `%d`/`%g` тащили мусор, сообщению доставался `(null)`.\n")
    return 1


if __name__ == "__main__":
    sys.exit(main())
