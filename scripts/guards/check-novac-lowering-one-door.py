# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-lowering-one-door.py — армы РАЗМЕЩЕНИЯ ЗНАЧЕНИЯ в
эмиттере только убывают: их число равно базе, и база двигается рукой.

ПОЧЕМУ ЭТОТ СТРАЖ ПОЯВИЛСЯ (подплан 274.8, M0, 2026-09-06). Эмиттер сам решает,
куда ложится значение, в ВОСЬМИ позициях (хвост блока, тело `=>`, `return`,
инициализатор связывания, деструктурирующее связывание, `??`, if-значение и арм
match, предпроход подъёма) — и в каждой стоит СВОЙ набор армов по виду узла
(MatchExpr, IfExpr, RecordCtor, ArrayLit, …). Инвентарь 2026-09-06: 29 армов.
На каждую новую форму значения приходится дописывать арм в каждую позицию, и
трижды за один день (волны W3/W4 подплана 274.7) одну форму понизили в двух
местах. Это и есть трипвайр Э4 плана 274: понижение обязано быть одно — модуль
`lower`, дверь `FnBuilder.place`, — а эмиттер печатает терминаторы.

ЧТО СЧИТАЕТСЯ АРМОМ (правило счёта, машинное): строка кода в
`novac/src/emit_c/*.nv` (тесты исключены, комментарии сняты), где стоит
`NodeKind.<вид значения>` — MatchExpr, IfExpr, IfStmt, RecordCtor, ArrayLit,
InterpStr, Coalesce, Lit, TupleExpr — либо вызов общего арма `is_expr_kind(` /
`is_value_tail_kind(`. Каждое попадание — одно решение о размещении по виду
узла; несколько на строке считаются по отдельности. Число НЕ равно
инвентарю-по-чтению (29): правило счёта здесь грубее и стабильнее, и именно
поэтому оно годится в базу.

ХРАПОВИК ТОЧНЫЙ, В ОБЕ СТОРОНЫ: рост — красный (форма понижена в эмиттере, а
не в `lower`); убыль — тоже красный, пока базу не сдвинули вниз тем же
коммитом (иначе база молча отстаёт и перестаёт быть правдой; см. память о
храповиках, которые «сверяют N с числом строк базы»). Нет базы — красный с
командой, которая её заводит.

НЕ ПРОВЕРЯЕТ: что `lower` вообще зовут (это `check-novac-deps.py` и
дифференциал на M1–M3); формы, понижаемые в `lower` (там их одна дверь —
`check-novac-arch-class-proofs.py`).

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

Аргументы: [корень репозитория] [--update-baseline]
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-lowering-one-door"
BASELINE = "scripts/guards/novac-lowering-doors.baseline"
VALUE_KINDS = ("MatchExpr", "IfExpr", "IfStmt", "RecordCtor", "ArrayLit",
               "InterpStr", "Coalesce", "Lit", "TupleExpr")
RE_KIND = re.compile(r"NodeKind\.(" + "|".join(VALUE_KINDS) + r")\b")
RE_GENERAL = re.compile(r"\b(is_expr_kind|is_value_tail_kind)\(")


def code_part(line):
    """Строка без комментария: `//` внутри строкового литерала здесь не встречается
    (эмиттер печатает C через `\\n`, не через `//`), поэтому срез до `//` честен."""
    s = line.strip()
    if s.startswith("//"):
        return ""
    return line.split("//")[0]


def count_arms(path):
    n = 0
    for line in path.read_text(encoding="utf-8", errors="replace").split("\n"):
        code = code_part(line)
        if not code:
            continue
        n += len(RE_KIND.findall(code)) + len(RE_GENERAL.findall(code))
    return n


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    emit = root / "novac" / "src" / "emit_c"
    if not emit.is_dir():
        print(f"{NAME} ok: судить нечего (нет {emit})")
        return 0

    files = [p for p in sorted(emit.glob("*.nv")) if not p.name.endswith("_test.nv")]
    per_file = [(p.name, count_arms(p)) for p in files]
    total = sum(n for _, n in per_file)
    breakdown = ", ".join(f"{nm} {n}" for nm, n in per_file if n)

    base_path = root / BASELINE
    if "--update-baseline" in sys.argv:
        base_path.write_text(f"{total}\n", encoding="utf-8")
        print(f"{NAME}: база записана: {total} ({breakdown})")
        return 0
    if not base_path.is_file():
        print(f"{NAME}: FAIL — базы нет: {BASELINE}", file=sys.stderr)
        print(f"  заведите её: python scripts/guards/{NAME}.py . --update-baseline "
              f"(сегодня армов {total})", file=sys.stderr)
        return 1
    base = int(base_path.read_text(encoding="utf-8").strip())

    if total > base:
        print(f"{NAME}: FAIL — армов размещения значения в эмиттере {total} (база {base}) — РОСТ",
              file=sys.stderr)
        print(f"  {breakdown}", file=sys.stderr)
        print("  Новая форма значения понижается в `lower` (дверь FnBuilder.place), а не",
              file=sys.stderr)
        print("  армом в позициях эмиттера; см. docs/plans/274.8-novac-mir.md §0.",
              file=sys.stderr)
        return 1
    if total < base:
        print(f"{NAME}: FAIL — армов размещения {total}, а база {base} — база отстала",
              file=sys.stderr)
        print(f"  сдвиньте её вниз тем же коммитом: python scripts/guards/{NAME}.py . "
              f"--update-baseline", file=sys.stderr)
        return 1

    print(f"{NAME} ok: армов размещения значения в эмиттере {total} (база {base}): {breakdown}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
