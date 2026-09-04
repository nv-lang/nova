# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-file-decls-door.py — объявления файла берутся ОДНОЙ
дверью `file_decls`, а не обходом детей корня (конвенция П18).

ПОЧЕМУ. `export fn f()` разбирается в `ExportDecl`, ОБЁРТЫВАЮЩИЙ объявление — и
это осознанная форма, её довод записан у парсера. Чего тот довод не покрыл: на
уровне ФАЙЛА объявления ищут пять разных обходов, и для каждого из них
обёрнутое объявление просто отсутствует.

ЦЕНА ЗАМЕРЕНА 2026-08-23, в тот же час, когда `export` вошёл в подмножество.
Обходов оказалось ПЯТЬ, и найдены они были по одному, каждый — своей аварией:
  * сборщик (`collect_types`) — объявления не было в реестре;
  * корень достижимости (`mono`) — тело не эмитилось;
  * эмиттер — `undefined symbol: novac_fn_helper__nova_int__to_nova_int` от
    ЛИНКЕРА, то есть после успешной компиляции;
  * типизирующий обход — ICE «no type recorded for this node» из эмиттера;
  * обход подмножества (`check`) — отказ вместо суждения.
Ни одна из этих аварий не назвала причину: пропущенный обход не краснеет, он
МОЛЧИТ, и объявление становится невидимым ровно для одного этапа.

ЧТО ЛОВИТ, две формы — обе те, которыми эти пять были написаны:
  1. `branch_children(file)` (или `(f)`, когда параметр назван так) вне двери;
  2. `for … in children` в функции, которая рядом утверждает `NodeKind.File` —
     обход детей корня, полученных разбором узла.

ЧЕГО НЕ ЛОВИТ: обход детей ЛЮБОГО другого узла — это не вопрос о файле;
`sem/file_shape.nv` — дом самой двери; тесты (`*_test.nv`) строят деревья руками.

$1 — корень репозитория; $2 — override сканируемой директории (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-file-decls-door"

RE_RAW = re.compile(r"branch_children\(\s*(file|unit)\s*\)")
RE_FILE_ASSERT = re.compile(r"NodeKind\.File")
RE_CHILDREN_LOOP = re.compile(r"\bfor\s+\w+\s+in\s+children\b")


def main():
    a = sys.argv + [""] * 3
    root = pathlib.Path(a[1] if a[1] else ".").resolve()
    src = pathlib.Path(a[2]) if a[2] else root / "novac" / "src"
    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    bad = []
    files = walks = 0
    for f in sorted(src.rglob("*.nv")):
        rel = f.relative_to(src).as_posix()
        if rel.endswith("_test.nv"):
            continue
        # Дом двери: она обязана обойти детей корня внутри себя. Путь ПЕРЕЕХАЛ
        # в тот же день, когда `slots.nv` перешагнул тысячу строк и решение 12
        # вырезало файловые вопросы в свой файл, — и страж сказал об этом сам,
        # покраснев на доме двери. Исключение по ПУТИ живёт ровно до тех пор,
        # пока путь не сменится: это цена точности, и она названа.
        if rel == "sem/file_shape.nv":
            continue
        files += 1
        lines = f.read_text(encoding="utf-8", errors="replace").split("\n")
        saw_file_kind = -99
        for n, line in enumerate(lines, 1):
            code = line.split("//", 1)[0]
            if RE_FILE_ASSERT.search(code):
                saw_file_kind = n
            m = RE_RAW.search(code)
            if m:
                walks += 1
                bad.append(f"  {rel}:{n}: `{m.group(0)}` — объявления файла берёт "
                           f"дверь `file_decls`: {code.strip()[:70]}")
                continue
            if RE_CHILDREN_LOOP.search(code) and n - saw_file_kind <= 6:
                walks += 1
                bad.append(f"  {rel}:{n}: обход детей КОРНЯ мимо двери "
                           f"`file_decls`: {code.strip()[:70]}")

    if bad:
        print(f"{NAME}: FAIL — объявления файла берутся мимо двери (П18):", file=sys.stderr)
        for b in bad[:15]:
            print(b, file=sys.stderr)
        print("  `export fn f()` лежит ВНУТРИ обёртки видимости, и для сырого обхода", file=sys.stderr)
        print("  его нет. Пропущенный обход не краснеет — он молчит: объявление", file=sys.stderr)
        print("  становится невидимым ровно для одного этапа (2026-08-23: пять", file=sys.stderr)
        print("  обходов, пять разных аварий, ни одна не назвала причину).", file=sys.stderr)
        return 1

    if files == 0:
        # МИШЕНЬ УЕХАЛА, А НЕ «НАРУШЕНИЙ НЕТ» (класс №911, страж
        # check-guard-empty-root): каталог есть, подсудных файлов ноль —
        # печатать здесь правдоподобный счёт значит выдавать пустоту за
        # проверенное. Формулировка донорская, от check-novac-file-size.py.
        print(f"{NAME} ok: судить нечего (0 .nv-файлов в {src})")
        return 0

    print(f"{NAME} ok: файлов .nv: {files}, обходов детей корня мимо двери: {walks}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
