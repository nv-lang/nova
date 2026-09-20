# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-invented-name-not-named.py — имя, ВЫДУМАННОЕ компилятором,
# ДОМ ПРАВИЛА: план 274.4; реестр 221.1 №534.
не проходит через дверь `named`.

ПРАВИЛО. У понижения две двери, создающие локаль, и они отвечают на РАЗНЫЕ вопросы:
  * `@ir.named(ty, name)` — имя дал АВТОР исходника (привязка `ro`/`mut`, переменная `for`,
    привязка образца). Дверь этим и ценна: её вызов есть утверждение «так написал человек»;
  * `@ir.temp(ty)` — имя даёт ТЕЛО, номером локали (`_novac_l<i>`).
Поэтому строковый литерал вида `_novac_*` в аргументе `named` — противоречие в терминах:
компилятор объявляет автором себя.

ЗАЧЕМ МАШИННО, а не на честном слове. До 2026-09-09 шесть мест сочиняли имена сами
(`_novac_tmp_t<uniq>`, `_novac_i_t<uniq>`, `_novac_scr_t<uniq>`) и отдавали их в `named`, потому
что `temp` не умел объявлять локаль первым присваиванием. Расцепление `LocalKind`/`DeclAt` это
починило, имена переехали на индекс — и осталась НОРМА, которую держало только внимание. Норма
без механизма отсюда и живёт: конвенция велит либо завести механизм, либо снять инвариант.

СВЯЗЬ С F30, из-за которой норма вообще нужна. Nova позволяет связать одно имя дважды в одной
области видимости; C — нет. Значит имя, выдуманное принтером, обязано быть НЕ ИЗ исходника,
иначе две привязки дадут одну переменную C и невалидный код из законной Nova.

ЧТО ПРОВЕРЯЕТ, в этом порядке:
  A. мишень на месте: дверь `named` объявлена в `novac/src/lower/ir.nv`. Нет двери — страж молча
     судил бы пустоту (класс №519), поэтому это ОТКАЗ, а не «ноль нарушений»;
  B. ни один вызов `@ir.named(...)` не несёт литерала с `_novac_` — ни в аргументе, ни собранным
     в интерполяции.

ЧЕГО СТРАЖ НЕ УСТАНАВЛИВАЕТ (Г9), и это надо знать, чтобы не переоценить зелёный:
  * он не доказывает, что имя, пришедшее из исходника, уникально: F30 разрешает повтор, и от
    столкновения двух АВТОРСКИХ имён защищает не он;
  * он не видит имени, собранного не литералом, а через переменную, значение которой пришло из
    строки без `_novac_` (например `"tmp" + n`). Такая форма в дереве не встречается, и вводить
    её значило бы обойти дверь ЗАМЫСЛОМ, а не ошибкой;
  * он ничего не говорит о том, СКОЛЬКО локалей создаётся через `temp` — это предмет меры волны
    (`scripts/tools/novac-tree-edges.py`), а не стража.

$1 — корень; $2 — override сканируемого каталога (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-invented-name-not-named"

# Вызов двери и его аргументы до закрывающей скобки на той же строке. Комментарии отсекаются
# ДО поиска: проза с примером `@ir.named(ty, "_novac_x")` нарушением не является.
# ДВЕРЬ, А НЕ НАПИСАНИЕ (правка 2026-09-10). Первая редакция искала `@ir.named(` и ослепла,
# как только билдер уехал внутрь получателя и обращения стали `@lo.ir.named(`: вердикт
# «вызовов named 0, из них с выдуманным именем 0» — зелёный на пустой выборке. Ловим вызов
# двери у выражения, которое ЕСТЬ билдер: цепочка получателя кончается на `ir` при любом
# написании (`@ir`, `@lo.ir`, будущее). Ведущая собака снимается: получатель приходит как
# `@ir`, и сравнение с `ir` без неё промахивалось бы ровно на старом написании.
RE_NAMED = re.compile(r"([@A-Za-z_][A-Za-z_0-9.@]*)\.named\s*\(([^)]*)\)")
RE_INVENTED = re.compile(r"_novac_")
RE_DOOR = re.compile(r"^export fn FnBuilder mut @named\s*\(", re.M)


def main() -> int:
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    scan = pathlib.Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else root / "novac" / "src"

    # --- A. мишень
    ir = root / "novac" / "src" / "lower" / "ir.nv"
    if len(sys.argv) > 2:
        cand = scan / "lower" / "ir.nv"
        if cand.is_file():
            ir = cand
    if not ir.is_file():
        print(f"{NAME}: ОТКАЗ — не найден {ir}: судить нечего", file=sys.stderr)
        return 1
    if not RE_DOOR.search(ir.read_bytes().decode("utf-8", "replace")):
        print(f"{NAME}: ОТКАЗ — в {ir} нет двери `export fn FnBuilder mut @named(`.",
              file=sys.stderr)
        print("  Либо дверь переименована, либо страж судит не тот файл. Ноль нарушений при",
              file=sys.stderr)
        print("  отсутствующей мишени — пустая выборка, а не проверка (класс №519).",
              file=sys.stderr)
        return 1

    if not scan.is_dir():
        print(f"{NAME}: ОТКАЗ — нет каталога {scan}", file=sys.stderr)
        return 1

    # --- B. вызовы
    calls = 0
    bad = []
    files = sorted(scan.rglob("*.nv"))
    for f in files:
        for n, raw in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            code = raw.split("//", 1)[0]
            for m in RE_NAMED.finditer(code):
                if m.group(1).split(".")[-1].lstrip("@") != "ir":
                    continue   # `named` у чего-то другого дверью билдера не является
                calls += 1
                if RE_INVENTED.search(m.group(2)):
                    bad.append((f.relative_to(root) if root in f.parents else f, n, raw.strip()))

    if bad:
        print(f"{NAME}: НАРУШЕНИЕ — выдуманное имя отдано двери `named`: {len(bad)}",
              file=sys.stderr)
        for p, n, line in bad:
            print(f"    {p}:{n}: {line[:110]}", file=sys.stderr)
        print("", file=sys.stderr)
        print("  `named` означает «имя дал АВТОР исходника». Имя вида `_novac_*` выдумал",
              file=sys.stderr)
        print("  компилятор — ему место в `@ir.temp(ty)`, которая нумерует локаль индексом", file=sys.stderr)
        print("  тела. Если временной нужно объявление первым присваиванием — это `DeclAt`,",
              file=sys.stderr)
        print("  а не возврат к `named` (274.8 M3).", file=sys.stderr)
        return 1

    # ВТОРАЯ МИШЕНЬ: дверь есть, а вызовов НОЛЬ — это не чистота, это слепота. Первая
    # редакция в таком случае печатала `ok`, и ровно так она соврала, когда обращения
    # переехали на путь через получателя (2026-09-10).
    if calls == 0:
        print(f"{NAME}: ОТКАЗ — дверь `named` объявлена, а вызовов НЕТ ни одного.",
              file=sys.stderr)
        print("  Ноль вызовов при живой двери — пустая выборка, а не чистое дерево:",
              file=sys.stderr)
        print("  скорее всего изменилось написание обращения, и образец его не видит.",
              file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv {len(files)}, вызовов `named` {calls}, "
          f"из них с выдуманным именем 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
