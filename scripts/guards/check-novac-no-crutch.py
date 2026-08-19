# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-crutch.py — в novac не бывает костылей (П34).

ЗАЧЕМ. Указание владельца 2026-08-19: «никаких костылей в novac не допускается в
принципе нигде». Механизм либо законен и назван, либо его нет. Слово «костыль» в
собственном тексте компилятора — это признание, что решение принято не было, а
отложено, и отложено БЕЗ срока: у костыля нет ни номера, ни владельца, ни
условия снятия, поэтому он и живёт вечно.

ПРАВИЛО. В `novac/src/**/*.nv` запрещены слова, называющие механизм костылём:
`костыль`, `crutch`, `hack`, `stopgap`, `for now`, `на время`.

ОДНО ИСКЛЮЧЕНИЕ, и оно не костыль, а УПРАВЛЯЕМАЯ форма: `workaround` внутри
блока с маркером `[LEGACY-#...]`. Такой обход несёт номер бага в реестре 221.1,
владельца и срок; его снимает та же волна, что чинит баг, и за этим следит
`check-novac-legacy-workarounds.py`. Разница ровно в том, что у него есть конец.

ТАКЖЕ ЗАКОННО отрицание: «это честная модель, а не workaround» — фраза, которая
объясняет, чем решение НЕ является.

ЧЕГО СТРАЖ НЕ ДЕЛАЕТ: не судит доку и план (там слово законно в историческом
рассказе — «это был костыль, снят тогда-то»). Названная слепая зона: прозу судит
приёмка.

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-crutch"
BANNED = re.compile(r"костыл|crutch|(^|[^A-Za-z])hack([^A-Za-z]|$)|stopgap|for now|на время", re.I)
WORKAROUND = re.compile(r"workaround", re.I)
LEGACY = re.compile(r"\[LEGACY-#")
NEGATED = re.compile(r"not a workaround|не workaround|а не workaround", re.I)


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

    bad = []
    governed = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        block = []                     # текущий непрерывный блок комментария
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            if BANNED.search(line):
                bad.append(f"  {rel}:{n}: {line.strip()[:88]}")
            if WORKAROUND.search(line) and not NEGATED.search(line):
                # Управляемая форма: маркер где-либо в ЭТОМ блоке комментария —
                # он бывает длиннее трёх строк, и обрывать его на окне значило бы
                # красить управляемый обход как костыль.
                if any(LEGACY.search(w) for w in block + [line]):
                    governed += 1
                else:
                    bad.append(f"  {rel}:{n}: обход без маркера [LEGACY-#...]: {line.strip()[:70]}")
            block = block + [line] if line.lstrip().startswith("//") else []

    if bad:
        print(f"{NAME}: FAIL — механизм назван костылём (П34):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Костыль — это решение, которое не приняли, а отложили без срока: у него нет", file=sys.stderr)
        print("  ни номера, ни владельца, ни условия снятия, поэтому он живёт вечно. Либо", file=sys.stderr)
        print("  механизм законен — тогда опиши его правилом, без извинений, — либо его нет.", file=sys.stderr)
        print("  Обход бага ОРАКУЛА — другое: у него есть форма [LEGACY-#NNN] со сроком и", file=sys.stderr)
        print("  строкой в реестре 221.1, и снимает его та же волна, что чинит баг.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, мест, названных костылём: 0 "
          f"(управляемых обходов под маркером: {governed})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
