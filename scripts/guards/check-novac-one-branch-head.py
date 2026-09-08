# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-one-branch-head.py — голову ветвления C печатают ТОЛЬКО названные
двери эмиттера novac, и каждая из них её действительно печатает.
План: docs/plans/274.8-novac-mir.md (волна M2b-2, шаг 3а); норма — П21 «одна дверь на вопрос»
и `docs/dev/conventions-governance.md` («инвариант на честном слове запрещён»).

ПОЧЕМУ ЭТОТ СТРАЖ ПОЯВИЛСЯ (замер 2026-09-08). До волны одну форму — `if (<условие>) {` —
печатали ЧЕТЫРЕ места: оператор `if`, `if let`, guard арма `match` и if-значение, плюс `??`
печатал ту же голову с тестом Option'а. Пятая копия уже расходилась с остальными: у двух хвостов
`else` разошлось поведение на цепочечном `if let` (реестр №991), и нашлось это пробой, а не
чтением. Волна свела их к одной двери. Инвариант «одна дверь» держится теперь этим стражем, а не
вниманием следующего окна: без механизма четвёртая копия появляется на первой же новой форме,
и разойдётся она молча — C соберётся.

ЧТО СТРАЖ УСТАНАВЛИВАЕТ (Г9: вердикт говорит ровно установленное). Где в
`novac/src/emit_c/**/*.nv` стоит `@body.append(...)`, чей ПЕЧАТАЕМЫЙ ТЕКСТ начинается головой
ветвления C — то есть необязательными пробелами и `if (`. Что этот текст ЗНАЧИТ в найденном
месте, страж не судит. Проверяются два случая, и оба обязательны:
  * голова печатается в функции, которой НЕТ в списке дверей ниже — отказ (новая копия формы);
  * дверь из списка перестала печатать голову — отказ (дверь переехала или исчезла, и дом
    инварианта пропал, хотя список о нём ещё говорит).
Второй случай — не симметрия ради симметрии: страж, который только запрещает новое, зеленеет и
на дереве, где двери больше нет вовсе.

ИСТОЛКОВАНИЕ, а не установленное (в вердикте подаётся как возможность): первый случай похож на
пятую копию формы, второй — на переехавшую дверь. Что из этого верно, решает читатель.

ДВЕРИ И ПОЧЕМУ ИМЕННО ОНИ:
  * `@print_if_head` (emit_c/emit_flow.nv) — голова оператора `if`, `if let`, guard'а арма и
    двух форм позиции значения (if-значение, `??`). ОДНА дверь на все пять;
  * `@print_arm_head` (emit_c/emit_match.nv) — голова арма `match`: `if (!<флаг> && (<тест>)) {`.
    Отдельная дверь, потому что печатает флаг первого совпадения, которого нет ни у одной другой
    формы (флаг — артефакт печати, см. `SwitchTerm` в lower/ir.nv);
  * `@emit_requires_prologue` (emit_c/emit_requires.nv) — `if (!(<условие>)) nova_contract_...`:
    ДРУГАЯ форма (отрицание контракта, не ветвление тела), и в волну она не входит. Названа
    здесь границей, а не забыта.

ЦЕНА, ЗАМЕРЕНА, А НЕ ОЦЕНЕНА (2026-09-08, под чужим ярусом novac — то есть под нагрузкой,
как этот страж и будет жить): три прогона 202/229/208 мс; читается 11 файлов,
172 423 байта, компилятор не запускается. ПРАВИЛО СЧЁТА: цена растёт с числом файлов в
`novac/src/emit_c/`, а не с размером дерева — повторить замер значит пересчитать по этому
каталогу. Ярус — loop.

ИСПОЛЬЗОВАНИЕ:
    python scripts/guards/check-novac-one-branch-head.py [КОРЕНЬ]
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-one-branch-head"

# Двери: имя функции -> файл, в котором она обязана жить.
DOORS = {
    "@print_if_head": "emit_flow.nv",
    "@print_arm_head": "emit_match.nv",
    "@emit_requires_prologue": "emit_requires.nv",
}

# `@body.append("` + необязательные пробелы + `if (`  — голова ветвления C.
RE_HEAD = re.compile(r'@body\.append\(\s*"[ \t]*if \(')
RE_FN = re.compile(r'^\s*(?:export\s+)?fn\s[^\n]*?(@[a-z_][a-z0-9_]*)\s*\(')


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    d = root / "novac" / "src" / "emit_c"
    if not d.is_dir():
        print(f"{NAME} ok: судить нечего (нет {d})")
        return 0

    files = sorted(p for p in d.rglob("*.nv"))
    if not files:
        print(f"{NAME}: FAIL — в {d} нет ни одного .nv: искать нечего, а это не зелено", file=sys.stderr)
        return 1

    strays = []          # голова вне двери
    door_prints = {k: 0 for k in DOORS}
    scanned = 0
    for p in files:
        scanned += 1
        fn = ""
        for i, line in enumerate(p.read_text(encoding="utf-8", errors="replace").split("\n"), 1):
            m = RE_FN.match(line)
            if m:
                fn = m.group(1)
            s = line.strip()
            if s.startswith("//"):
                continue
            if not RE_HEAD.search(line):
                continue
            if fn in DOORS:
                if p.name != DOORS[fn]:
                    strays.append((p.name, i, fn, "дверь найдена в чужом файле"))
                else:
                    door_prints[fn] += 1
            else:
                strays.append((p.name, i, fn or "<вне функции>", "голова вне названных дверей"))

    silent = [k for k, n in door_prints.items() if n == 0]

    if strays or silent:
        print(f"{NAME}: FAIL — голову ветвления печатает не только названная дверь:", file=sys.stderr)
        for f, i, fn, why in strays:
            print(f"    {f}:{i}  в {fn} — {why}", file=sys.stderr)
        for k in silent:
            print(f"    дверь {k} ({DOORS[k]}) НЕ печатает голову — переехала или исчезла", file=sys.stderr)
        print("    Одну форму печатает ОДНА дверь (П21): до волны M2b-2 их было пять, и две уже", file=sys.stderr)
        print("    разошлись молча (реестр №991). Новая форма — параметр существующей двери,", file=sys.stderr)
        print("    а не новая копия; новая ДВЕРЬ — правка этого списка тем же слиянием, с причиной.", file=sys.stderr)
        return 1

    total = sum(door_prints.values())
    parts = ", ".join(f"{k} {n}" for k, n in sorted(door_prints.items()))
    print(f"{NAME} ok: файлов {scanned}, голов ветвления {total}, все в названных дверях ({parts})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
