# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-wrapper-is-stored.py — обёртка заводится вокруг
ИДЕНТИЧНОСТИ, а идентичность где-то ХРАНИТСЯ (подплан 274 §9.1г.1, урок волны В4).

ЗАЧЕМ. Указание владельца 2026-08-22: «правила не работают сами по себе без
авто-проверки». Правило вывела волна В4, и стоило оно целой волны: план велел
завернуть индекс строки `defs`, а замер показал, что этот индекс не переживает
того `rows[d]`, которое кормит — все пять читателей брали `find(name)` и
индексировали на следующей строке. Индекс, не переживающий своего
индексирования, — это ШАГ, а не личность, и типизировать шаг значит завести
церемонию: обёртку, три двери к ней и строку в базе поверхности ради числа,
живущего полторы строки. Пространство было удалено, а дверь стала отдавать СТРОКУ.

ПРИЗНАК, по которому шаг отличается от личности, машинный: **личность ХРАНИТСЯ**.
У завёрнутого пространства обязано быть место, где его значение лежит: поле
записи, элемент вектора (`[]X`) или полезная нагрузка плеча суммы. Шаг не хранится
нигде — он только производится и тут же потребляется, поэтому встречается
исключительно в сигнатурах.

ПРАВИЛО. Каждый `export type X int` в `novac/src` обязан иметь хотя бы одно место
ХРАНЕНИЯ внутри ТЕЛА какого-нибудь объявления типа: `поле X`, `поле []X` или
`| Плечо(X)`. Сигнатуры, импорты и вызовы не считаются — там значение проезжает,
а не живёт.

ПРОВЕРЕНО НА ПЯТИ ЖИВЫХ ПРОСТРАНСТВАХ (2026-08-22): `FnRow` (`FnDef.next_row`,
`Ctx.fn_decl_rows`, `Instance.row`, `DefTarget.DefFn`), `TyId` (`ParamDef.type_id`,
`CheckOut.types`), `DeclId` (`DefTarget.DefType`), `VariantRow`
(`DefTarget.DefVariant`, `CalleeTarget.CalleeVariant`), `NodeId` (`Node.Branch.id`)
— у всех есть. У отменённого `DefRow` не было бы ни одного, то есть страж поймал
бы ошибку ДО волны, а не после.

ЧЕГО СТРАЖ НЕ ДЕЛАЕТ: не судит `export type X str`/`bool` (пространств-индексов
среди них пока нет — названная слепая зона) и не требует хранения от обёрток вне
`novac/src`.

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-wrapper-is-stored"

RE_WRAPPER = re.compile(r"^export type ([A-Z][A-Za-z0-9_]*) int\s*$")
RE_TYPE_OPEN = re.compile(r"^export type ([A-Z][A-Za-z0-9_]*)(?: value)? \{\s*$")
RE_ENUM_OPEN = re.compile(r"^export type ([A-Z][A-Za-z0-9_]*) enum\s*$")


def bodies(lines):
    """Тела объявлений типов: (имя, строки тела). Запись — до закрывающей скобки,
    сумма — до первой строки, не начинающейся с `|`."""
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        m = RE_TYPE_OPEN.match(line)
        if m:
            body = []
            j = i + 1
            while j < len(lines) and not lines[j].startswith("}"):
                body.append(lines[j])
                j += 1
            out.append((m.group(1), body))
            i = j + 1
            continue
        m = RE_ENUM_OPEN.match(line)
        if m:
            body = []
            j = i + 1
            while j < len(lines) and (lines[j].strip().startswith("|") or lines[j].strip() == ""):
                if lines[j].strip():
                    body.append(lines[j])
                j += 1
            out.append((m.group(1), body))
            i = j
            continue
        i += 1
    return out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень (класс №519)",
              file=sys.stderr)
        return 1

    wrappers = {}          # имя -> "файл:строка"
    all_bodies = []        # (файл, имя типа, строки тела)
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = f.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n")
        for n, line in enumerate(lines, 1):
            m = RE_WRAPPER.match(line)
            if m:
                wrappers[m.group(1)] = f"{rel}:{n}"
        for tname, body in bodies(lines):
            all_bodies.append((rel, tname, body))

    if not wrappers:
        print(f"{NAME} ok: завёрнутых пространств: 0 (судить нечего)")
        return 0

    bad = []
    stored = {}
    for w, where in sorted(wrappers.items()):
        hit = None
        pat = re.compile(r"(^|[\s\[\](,])" + w + r"($|[\s\)\],/])")
        for rel, tname, body in all_bodies:
            if tname == w:
                continue                      # своё же объявление не хранение
            for bl in body:
                code = bl.split("///", 1)[0].split("//", 1)[0]
                if pat.search(code):
                    hit = f"{rel} / {tname}: {bl.strip()[:64]}"
                    break
            if hit:
                break
        if hit:
            stored[w] = hit
        else:
            bad.append(f"  {where}: `{w}` не ХРАНИТСЯ ни в одном теле объявления — "
                       f"ни поле, ни `[]{w}`, ни плечо суммы")

    if bad:
        print(f"{NAME}: FAIL — завёрнутое пространство нигде не хранится (§9.1г.1, урок В4):",
              file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Обёртка заводится вокруг ИДЕНТИЧНОСТИ, а идентичность где-то лежит:", file=sys.stderr)
        print("  поле, элемент вектора, плечо суммы. Значение, которое только", file=sys.stderr)
        print("  производится и тут же потребляется, — это ШАГ, и типизировать шаг", file=sys.stderr)
        print("  значит завести церемонию вокруг числа, живущего полторы строки.", file=sys.stderr)
        print("  Верный ход тогда — не обёртка, а ДВЕРЬ, отдающая то, за чем шли", file=sys.stderr)
        print("  (волна В4: `DefTable.def_of` вместо `find` + `rows[d]`).", file=sys.stderr)
        return 1

    print(f"{NAME} ok: завёрнутых пространств: {len(wrappers)}, у всех есть место хранения "
          f"({', '.join(sorted(stored))})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
