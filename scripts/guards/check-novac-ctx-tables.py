# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-ctx-tables.py — каждая таблица строк в `Ctx` и в
контейнере канала чекера объявлена решением в плане (П17, §10.3б).

ЗАЧЕМ. Таблица заводится за минуту, а живёт всю жизнь компилятора: вторая
таблица о том же предмете — это вторая дверь, только незаметная, потому что
выглядит как поле структуры. Решение «почему это ОТДЕЛЬНАЯ таблица» пишется
один раз в §10.3б, и страж держит соответствие в обе стороны: поле без строки —
таблица без решения; строка без поля — протухший реестр (класс №519).

СВЕРЯЕТ:
  * поля `export type Ctx` в novac/src/sem/sem.nv  ↔  первая колонка §10.3б;
  * поля `export type CheckOut` в sem/channel.nv   ↔  §10.3б-канал.
Расхождение в ЛЮБУЮ сторону — красный, и обе стороны названы отдельно.

ПОЧЕМУ PYTHON: shell-редакция поднимала четыре awk, четыре `tr`, `comm`, `wc` и
временный каталог — 1.1с на разбор двух структур и двух разделов (П14).

$1 — корень; $2 — override sem.nv; $3 — override плана; $4 — override канала.
ШОВ САМОТЕСТА: подменили план ($3) — значит подменяют и мир, поэтому файл канала
обязаны назвать явно ($4), иначе синтетический план судил бы НАСТОЯЩИЙ канал и
краснел о разделе, которого в синтетике нет.
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-ctx-tables"

RE_FIELD = re.compile(r"^[ \t\v\f][ \t\v\f]*[a-z_][A-Za-z0-9_]*[ \t\v\f]")
RE_CLOSE = re.compile(r"^\}")


def read(path):
    return path.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n")


def fields_of(path, header):
    """Имена полей структуры: от `export type <T> {` до строки, начинающейся с `}`."""
    out = set()
    inb = False
    for line in read(path):
        if line.startswith(header):
            inb = True
            continue
        if inb and RE_CLOSE.match(line):
            inb = False
        if inb and RE_FIELD.match(line):
            out.add(re.split(r"\s", line.strip(" \t"), maxsplit=1)[0])
    return out


def plan_rows(plan_lines, start_re, stop_re, cell_re):
    """Имена из первой колонки таблицы названного раздела плана."""
    out = set()
    inb = False
    for line in plan_lines:
        if start_re.match(line):
            inb = True
            continue
        if inb and stop_re.match(line):
            inb = False
        if inb:
            m = cell_re.match(line)
            if m:
                out.add(m.group(1))
    return out


def main():
    a = sys.argv + [""] * 5
    root = pathlib.Path(a[1] if a[1] else ".").resolve()
    sem = pathlib.Path(a[2]) if a[2] else root / "novac" / "src" / "sem" / "sem.nv"
    plan = pathlib.Path(a[3]) if a[3] else root / "docs" / "plans" / "274-novac-self-hosted-compiler.md"
    if a[3]:
        chan = pathlib.Path(a[4]) if a[4] else None
    else:
        chan = pathlib.Path(a[4]) if a[4] else root / "novac" / "src" / "sem" / "channel.nv"

    if not sem.is_file():
        print(f"{NAME} ok: судить нечего (нет {sem})")
        return 0
    if not plan.is_file():
        print(f"{NAME}: FAIL — нет плана {plan}, состав таблиц не с чем сверить", file=sys.stderr)
        return 1

    plan_lines = read(plan)
    src = fields_of(sem, "export type Ctx {")
    # ТОЧКА в якоре обязательна: раздел §10.3б-канал (таблицы CheckOut) —
    # соседний, и без точки его строки читались бы как поля Ctx.
    pln = plan_rows(plan_lines,
                    re.compile(r"^#+ .*10\.3б\."),
                    re.compile(r"^#+ "),
                    re.compile(r"^\|[ \t]*`([a-z_][A-Za-z0-9_]*)`[ \t]*\|"))

    if not src:
        print(f"{NAME}: FAIL — в {sem} не нашлось ни одного поля `export type Ctx`: "
              f"страж потерял мишень (класс №519)", file=sys.stderr)
        return 1
    if not pln:
        print(f"{NAME}: FAIL — таблица §10.3б плана пуста или переименована: "
              f"сверять не с чем, а молчать нельзя (класс №519)", file=sys.stderr)
        return 1

    missing = sorted(src - pln)
    stale = sorted(pln - src)

    if missing:
        print(f"{NAME}: FAIL — таблица строк в Ctx заведена без решения (П17, план §10.3б):", file=sys.stderr)
        for m in missing:
            print(f"  {m} — нет строки в §10.3б", file=sys.stderr)
        print("  Впиши строку в §10.3б и ответь там на вопрос «почему это ОТДЕЛЬНАЯ таблица»,", file=sys.stderr)
        print("  либо своди с существующей: метод есть функция с получателем (план 273 §2).", file=sys.stderr)
        return 1
    if stale:
        print(f"{NAME}: FAIL — строка §10.3б без поля в Ctx (протухшая запись, класс №519):", file=sys.stderr)
        for s in stale:
            print(f"  {s} — такого поля в Ctx нет", file=sys.stderr)
        print("  Убери строку той же волной, что и поле: реестр, отставший от кода, судит воздух.", file=sys.stderr)
        return 1

    nc = ""
    if chan is not None and chan.is_file():
        csrc = fields_of(chan, "export type CheckOut {")
        cpln = plan_rows(plan_lines,
                         re.compile(r"^### 10.3б-канал"),
                         re.compile(r"^#{1,4} "),
                         re.compile(r"^\| `([a-z_]+)` \|"))
        if not csrc:
            print(f"{NAME}: FAIL — в {chan} не нашлось полей CheckOut: "
                  f"страж потерял мишень (класс №519)", file=sys.stderr)
            return 1
        if not cpln:
            print(f"{NAME}: FAIL — раздел §10.3б-канал плана пуст или переименован: "
                  f"сверять не с чем (класс №519)", file=sys.stderr)
            return 1
        cbad = sorted(csrc ^ cpln)
        if cbad:
            print(f"{NAME}: FAIL — таблица канала чекера заведена или снята без решения "
                  f"(П17, план §10.3б-канал):", file=sys.stderr)
            for c in cbad:
                print(f"  {c}", file=sys.stderr)
            return 1
        nc = len(csrc)

    print(f"{NAME} ok: таблиц строк в Ctx: {len(src)}, в канале чекера: {nc} — "
          f"все объявлены в §10.3б, протухших строк: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
