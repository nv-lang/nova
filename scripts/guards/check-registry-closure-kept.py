# -*- coding: utf-8 -*-
"""scripts/guards/check-registry-closure-kept.py — закрытая запись реестра не
возвращается в ОТКРЫТ молча.

ПОЧЕМУ. Закрытие записи — это ЗАМЕР: строка несёт «чем доказано» и держателя
класса. Потерять её дороже, чем не закрыть: работа выглядит несделанной, и
следующее окно делает её заново.

ЦЕНА ЗАМЕРЕНА НА СЕБЕ (2026-08-30). Окно 274 разрешило конфликт реестра при
слиянии механически — «интегратор владеет реестром, берём сторону main», — и
строка 812, закрытая этим же окном часом раньше, вернулась в ОТКРЫТ: две тысячи
знаков доказательств исчезли МОЛЧА. Правило владения верно для строк, которые
ведёт интегратор, и неверно для строки, закрытой чужим замером.

ПОЧЕМУ МНОЖЕСТВО, А НЕ СЧЁТЧИК. Счётчик закрытых прячет потерю за чужим
закрытием: одну потеряли, другую закрыли — число то же. База держит НОМЕРА, и
исчезновение конкретного номера видно поимённо.

ПОЧЕМУ ЭТОГО НЕ ЛОВИЛИ. Проверено пробой в обе стороны 2026-08-30: строка 812
временно возвращена в ОТКРЫТ, и `check-registry-routes`, `-entry-shape`,
`-row-closed`, `-single-verdict` остались ЗЕЛЁНЫМИ все четыре.

ЗАКОННОЕ ПЕРЕОТКРЫТИЕ существует (дефект вернулся) — тогда номер убирается из
базы строкой хроники, как в любом храповике проекта.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override пути к реестру (шов самотеста);
$3 — override пути к базе.
"""
import io
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-registry-closure-kept"
RE_ROW = re.compile(r"^\|\s*(\d+)\s*\|")
RE_OPEN = re.compile(r"\*\*Статус:\*\*\s*ОТКРЫТ")


def closed_numbers(path):
    out = set()
    for line in io.open(path, encoding="utf-8", errors="replace"):
        m = RE_ROW.match(line)
        if not m:
            continue
        if not RE_OPEN.search(line):
            out.add(m.group(1))
    return out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    reg = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "plans" / "221.1-bug-sweep.md"
    base = pathlib.Path(a[3]) if len(a) > 3 else (
        pathlib.Path(__file__).resolve().parent / "registry-closure.baseline")

    if not reg.is_file():
        print(f"{NAME} ok: судить нечего (нет {reg})")
        return 0

    now = closed_numbers(reg)

    if not base.is_file():
        print(f"{NAME}: FAIL — нет базы {base}: страж без базы ничего не держит", file=sys.stderr)
        return 1

    want = set()
    for line in io.open(base, encoding="utf-8", errors="replace"):
        line = line.strip()
        if line.startswith("#") or not line:
            continue
        for tok in line.split():
            if tok.isdigit():
                want.add(tok)

    lost = sorted(want - now, key=int)
    if lost:
        print(f"{NAME}: FAIL — записи реестра ПЕРЕСТАЛИ быть закрытыми, а хроники в базе нет:",
              file=sys.stderr)
        print("  " + ", ".join("№" + n for n in lost), file=sys.stderr)
        print("  Закрытие — это ЗАМЕР: строка несёт «чем доказано» и держателя класса.", file=sys.stderr)
        print("  Чаще всего это слияние, разрешённое в пользу стороны, которая закрытия", file=sys.stderr)
        print("  не знает (замер 2026-08-30: строка 812 потеряла две тысячи знаков", file=sys.stderr)
        print("  доказательств молча). Верни закрытие из своей ветки; если запись", file=sys.stderr)
        print("  переоткрыта ОСОЗНАННО — убери номер из базы строкой хроники.", file=sys.stderr)
        return 1

    gained = sorted(now - want, key=int)
    extra = f", новых закрытых с прошлой базы: {len(gained)}" if gained else ""
    print(f"{NAME} ok: закрытых записей {len(now)}, из базы не потеряно ни одной{extra}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
