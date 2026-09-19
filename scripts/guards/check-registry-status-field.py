# -*- coding: utf-8 -*-
"""scripts/guards/check-registry-status-field.py — строка реестра без поля
статуса не заводится заново, а старые держит база (реестр 221.1 №1160, п.1-2).

ЗАЧЕМ. Поле статуса — единственное, что читают машины, и его отсутствие НЕ
нейтрально: два стража толкуют такую строку противоположно. Сканер маршрутов,
не найдя статуса, числит строку открытой; храповик закрытий, не найдя поля,
числит её закрытой. Про одну строку две машины отвечают разное, и обе молча —
то есть спор о готовности релиза идёт на числах, которые не сходятся между собой
по построению.

ЗАМЕР, которым задание заведено (2026-09-19, окно-помощник): строк реестра 1106,
поля статуса нет у 677. Двадцать девять из них, те что пишут статус прозой
прошедшего времени, уже стережёт отдельный страж; остальные 648 не стерёг никто.

ПОЧЕМУ ЗАСЕВ, А НЕ НОЛЬ. Прямолинейный отказ судить покрасил бы гейт на 677
строках в первый же прогон, и первое же окно сняло бы стража целиком — ровно так
снимают стражей, натравленных на историю. База засеяна сегодняшним множеством,
отказ действует только на строки, заведённые ПОСЛЕ неё.

ПОЧЕМУ НОМЕРА, А НЕ ЧИСЛО (форма взята у `registry-closure.baseline`): счётчик
прячет подмену — одну строку починили, другую завели без поля, число то же.

ХРАПОВИК ТОЛЬКО ВНИЗ, и в ОБЕ стороны громкий: номер, которого в базе нет, —
новая строка без поля, это отказ; номер, который из реестра ушёл или получил
поле, обязан уйти из базы ТОЙ ЖЕ правкой, иначе база разрешит завести заново
ровно столько, сколько починили.

ЧЕГО ЭТОТ СТРАЖ НЕ ПРОВЕРЯЕТ, и это сказано честно: ВЕРЕН ли статус строки. Это
вердикт о дефекте, он принадлежит интегратору и разбирается партиями (пункт 3
строки №1160). Страж следит лишь за тем, чтобы вердикт было ВИДНО машине.

$1 — корень; $2 — override пути к реестру (шов самотеста);
$3 — override пути к базе.
"""
import io
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-registry-status-field"
RE_ROW = re.compile(r"^\|\s*(\d+)\s*\|")
# Образец поля собирается из частей НАМЕРЕННО: написанный целиком, он попадает
# в счёт собственного стража — строка №1160 трижды ловилась так на цитате.
FIELD = "**" + "Статус" + ":**"
RE_KEY = re.compile(r"^nofield[^\S\n]*=[^\S\n]*(\d+)")


def rows_without_field(path):
    """Номера строк реестра, у которых нет поля статуса."""
    out = set()
    for line in io.open(path, encoding="utf-8", errors="replace"):
        m = RE_ROW.match(line)
        if not m:
            continue
        if FIELD not in line:
            out.add(m.group(1))
    return out


def read_baseline(path):
    """(множество номеров, объявленное число). Число читается ПЕРВОЙ строкой
    `nofield=`: летопись базы длинная, и поиск «первого числа в файле» отдал бы
    год из хроники."""
    nums, declared = set(), None
    for line in io.open(path, encoding="utf-8", errors="replace"):
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        m = RE_KEY.match(s)
        if m:
            if declared is None:
                declared = int(m.group(1))
            continue
        if s.isdigit():
            nums.add(s)
    return nums, declared


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    reg = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "plans" / "221.1-bug-sweep.md"
    base = pathlib.Path(a[3]) if len(a) > 3 else (
        pathlib.Path(__file__).resolve().parent / "registry-nofield.baseline")

    if not reg.is_file():
        print(f"{NAME} ok: судить нечего (нет {reg})")
        return 0
    if not base.is_file():
        print(f"{NAME}: FAIL — нет базы {base}: страж без базы ничего не держит",
              file=sys.stderr)
        return 1

    now = rows_without_field(reg)
    want, declared = read_baseline(base)

    if declared is None:
        print(f"{NAME}: FAIL — в базе нет строки `nofield=N`: число нечем сверить",
              file=sys.stderr)
        return 1
    if declared != len(want):
        print(f"{NAME}: FAIL — база спорит сама с собой: объявлено {declared}, "
              f"номеров {len(want)}", file=sys.stderr)
        return 1

    added = sorted(now - want, key=int)
    gone = sorted(want - now, key=int)

    if added:
        print(f"{NAME}: FAIL — строка реестра заведена БЕЗ поля статуса: "
              f"{', '.join('№' + n for n in added)}", file=sys.stderr)
        print("  Поле — единственное, что читают машины. Без него сканер маршрутов "
              "сочтёт строку открытой, а храповик закрытий — закрытой, и обе молча.",
              file=sys.stderr)
        print("  Поставь поле статуса этой строке (значение — твой вердикт, не "
              "формальность). Старые строки держит база и трогать их не надо.",
              file=sys.stderr)
        return 1

    if gone:
        print(f"{NAME}: FAIL — база отстала: {len(gone)} номер(ов) больше не без "
              f"поля ({', '.join('№' + n for n in gone[:8])}"
              f"{' и др.' if len(gone) > 8 else ''}).", file=sys.stderr)
        print("  Опусти базу ТОЙ ЖЕ правкой, что и починку: оставленная высокой, "
              "она разрешит завести заново ровно столько, сколько починено.",
              file=sys.stderr)
        print(f"  Новое значение: nofield={len(now)}", file=sys.stderr)
        return 1

    print(f"{NAME} ok: строк без поля {len(now)} (база {declared}), новых нет")
    return 0


if __name__ == "__main__":
    sys.exit(main())
