# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-export-has-caller.py — имя оправдывает СПРОС, а не
симметрия (конвенция П35; замер 2026-08-22).

ЗАЧЕМ. За одну сессию я шесть раз завёл имя, которое никто не спрашивал, и каждый
раз находил это руками — четыре двери (`candidates_from` вдобавок с off-by-one,
`same_vrow`, `same_decl`, `tparam_index`) и ещё две при перепроверке по указанию
владельца (`Reach.count`, `DefTable.type_decl_of`). Плюс 36 мёртвых импортов и две
обёртки вокруг ШАГА вместо личности. Это одна привычка в трёх формах: имя пишется
потому, что «дополняет набор» — есть `raw_X`, пусть будет и `no_X`; есть
толерантная половина, пусть будет и тотальная, — а не потому, что кто-то задал
вопрос.

Три формы, три механизма. Две уже были: `check-novac-import-exists.py` (правило B
— имя импортировано и не используется) и `check-novac-wrapper-is-stored.py`
(обёртка вокруг шага). Здесь — третья и самая частая: ЭКСПОРТ без вызывающего.

ПОЧЕМУ ЭТО ВООБЩЕ РАБОТАЕТ ДЛЯ novac. У novac нет внешних потребителей: это
ПРОГРАММА, а не библиотека, и всё, что она экспортирует, обязано быть кем-то
спрошено внутри `novac/src`. Для библиотеки такое правило было бы неверным — там
экспорт и есть контракт наружу; поэтому страж судит только `novac/src` и говорит
это вслух.

ПРАВИЛО. Каждое имя, объявленное `export type` / `export fn` / `export const` в
`novac/src/**/*.nv`, встречается в `novac/src` где-то КРОМЕ своего объявления.
Модульные тесты считаются вызывающими: тест — это спрос, причём самый честный.

ЧЕГО СТРАЖ НЕ ДЕЛАЕТ: не судит, СКОЛЬКО раз имя спрошено (один вызывающий — уже
спрос), и не отличает употребление от вызова (тип в поле — это употребление).
Названная слепая зона: имя, встречающееся только в строковом литерале, считается
употреблённым.

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-export-has-caller"

RE_EXP_TC = re.compile(r"^export\s+(?:type|const)\s+([A-Za-z_][A-Za-z0-9_]*)")
RE_EXP_FN = re.compile(r"^export\s+fn\s+(?:[A-Za-z_][A-Za-z0-9_\[\]]*\s+(?:mut\s+|consume\s+|ro\s+)?)?@?([A-Za-z_][A-Za-z0-9_]*)")


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

    exports = {}
    chunks = []
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        lines = f.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n")
        chunks.append("\n".join(x.split("//", 1)[0] for x in lines))
        for n, line in enumerate(lines, 1):
            m = RE_EXP_TC.match(line) or RE_EXP_FN.match(line)
            if m:
                exports.setdefault(m.group(1), f"{rel}:{n}")

    code = "\n".join(chunks)
    bad = []
    for nm, where in sorted(exports.items()):
        # объявление тоже попадает в счёт, поэтому спрос начинается со второго
        uses = len(re.findall(r"(^|[^A-Za-z0-9_])" + re.escape(nm) + r"($|[^A-Za-z0-9_])", code))
        if uses <= 1:
            bad.append(f"  {where}: `{nm}` экспортировано, а в novac/src не спрошено ни разу")

    if bad:
        print(f"{NAME}: FAIL — экспорт без вызывающего (П35):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Имя оправдывает СПРОС, а не симметрия набора. novac — программа, а не", file=sys.stderr)
        print("  библиотека: у его экспортов нет внешних потребителей, поэтому экспорт,", file=sys.stderr)
        print("  которого никто не спрашивает, — это код, который никто не исполняет.", file=sys.stderr)
        print("  Шесть таких за одну сессию (2026-08-22), и один из них нёс off-by-one,", file=sys.stderr)
        print("  который никогда не выстрелил, потому что вызывающего не было.", file=sys.stderr)
        print("  Либо появляется вызывающий, либо имя уходит.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: экспортов в novac/src: {len(exports)}, без вызывающего: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
