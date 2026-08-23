# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-tyid-door.py — идентификатор типа спрашивают ДВЕРЬЮ,
а не сравнением с нулём (конвенция П18: «один вопрос — одна дверь»).

ПОЧЕМУ. `TyId` — не int, а newtype (`type TyId int`), и «есть ли здесь тип»
отвечает `is_ty`, которая СНАЧАЛА разворачивает обёртку (`raw_ty(t) >= 0`).
Голое `t >= 0` выглядит тем же вопросом и им НЕ является.

ЦЕНА ЗАМЕРЕНА 2026-08-23, волна В4 плана 274. Строка
`if args[at].type_id >= 0 && p.type_id != args[at].type_id { return false }`
в двери подбора кандидатов ответила ИСТИНОЙ для `no_ty()` — и каждая генерик-строка
перестала подходить под свой же вызов: параметр-ТИП сравнивался с отсутствующим
типом. Три фикстуры покраснели разом, а поиск причины занял сорок минут бисекции
по восьми выходам одной функции, потому что диагностика говорила «нет функции с
таким числом аргументов» — то есть про арность, которой ничего не было. Замена на
`is_ty(...)` вылечила всё; ПЯТЬ проб той же формы в изоляции ведут себя правильно,
поэтому механизм расхождения не сведён (записан в novac-divergences.md как
несведённая находка), а правило — сведено: спрашивай дверь.

ЧТО ЛОВИТ: сравнение поля-идентификатора типа (`type_id`, `ret_id`, `ty_id`) с
литералом `0` операторами `>=`, `<=`, `>`, `<`. Ровно эти три имени — они и есть
поля типа `TyId` в реестрах (§10.3в).

ЧЕГО НЕ ЛОВИТ (и почему это не дыра):
  * строку, где уже стоит `raw_ty(` — обёртка развёрнута, сравнение честное;
  * `== 0` / `!= 0` — это не «есть ли тип», а сравнение С КОНКРЕТНЫМ id, и оно
    законно (`prims.int_id` бывает нулём);
  * `types/types.nv` — дом самой двери: `is_ty` обязана сравнивать внутри себя;
  * тесты (`*_test.nv`) — они СТРОЯТ значения, а не спрашивают их.

$1 — корень репозитория; $2 — override сканируемой директории (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-tyid-door"

# Поле-идентификатор типа, сравниваемое с нулём ПОРЯДКОВЫМ оператором.
RE_BARE = re.compile(r"\b(type_id|ret_id|ty_id)\b\s*(>=|<=|>|<)\s*0\b")


def main():
    a = sys.argv + [""] * 3
    root = pathlib.Path(a[1] if a[1] else ".").resolve()
    src = pathlib.Path(a[2]) if a[2] else root / "novac" / "src"
    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    bad = []
    files = 0
    for f in sorted(src.rglob("*.nv")):
        rel = f.relative_to(src).as_posix()
        if rel.endswith("_test.nv"):
            continue
        # Дом двери: `is_ty` обязана сравнивать внутри себя.
        if rel == "types/types.nv":
            continue
        files += 1
        for n, line in enumerate(f.read_text(encoding="utf-8", errors="replace").split("\n"), 1):
            code = line.split("//", 1)[0]
            if "raw_ty(" in code:
                continue
            m = RE_BARE.search(code)
            if m:
                bad.append(f"  {rel}:{n}: `{m.group(0)}` — спроси дверь `is_ty(...)`: "
                           f"{code.strip()[:90]}")

    if bad:
        print(f"{NAME}: FAIL — идентификатор типа сравнивается с нулём вместо двери:",
              file=sys.stderr)
        for b in bad[:15]:
            print(b, file=sys.stderr)
        print("  `TyId` — newtype, и вопрос «есть ли здесь тип» отвечает `is_ty`,", file=sys.stderr)
        print("  которая сперва разворачивает обёртку. Голое сравнение выглядит тем", file=sys.stderr)
        print("  же вопросом и им не является: 2026-08-23 оно ответило ИСТИНОЙ для", file=sys.stderr)
        print("  `no_ty()`, и генерик-вызовы перестали находить свои объявления.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {files}, сравнений идентификатора типа с нулём вне двери: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
