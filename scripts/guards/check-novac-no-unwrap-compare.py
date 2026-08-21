# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-unwrap-compare.py — завёрнутый индекс
СРАВНИВАЮТ завёрнутым (П19, подплан 274 §9.1г.1).

ЗАЧЕМ. Вопрос владельца 2026-08-22: «`raw_row(x) == raw_row(y)` — почему не
`x == y`?». Замер на трёх строках: `==` на newtype над `int` работает напрямую,
поэтому распаковка ради сравнения не нужна. Но дело не в лишних символах.

`raw_ty(a) == raw_ty(b)` сравнивает ДВА INT — и пройдёт, даже если `a` это
`TyId`, а `b` это `DeclId`. То есть распаковка ради сравнения ОТКРЫВАЕТ ровно ту
дыру, ради закрытия которой обёртка и вводилась: смешение пространств. Завёрнутое
сравнение такую программу отвергает.

Второй вред — читаемость по контракту, который двери сами и объявили: «`raw_*` в
выражении говорит „я индексирую“; увидел в другом месте — смотришь на
скрещивание». Распаковка в сравнении делает это предложение ложным и превращает
каждое такое место в ложную тревогу для читателя.

Замер того же дня: 34 таких сравнения в 9 файлах — все написаны мной же за четыре
волны заворачивания, то есть волна заворачивания САМА производит этот класс, если
за ней не следить.

ПРАВИЛО. В `novac/src/**/*.nv` запрещено `raw_X(...) <op> raw_Y(...)`, где op —
`==` или `!=`. Сравнивай завёрнутые значения.

ЗАКОННО и стражем НЕ судится:
  * `raw_ty(t) >= types.len()` и любое сравнение с ДЛИНОЙ или числом — это
    индексирование, ровно то, зачем `raw_*` и существует;
  * `raw_row(x) < raw_row(y)` — упорядочивание (`<`, `>`, `<=`, `>=`): у
    завёрнутого типа порядка может не быть, и это отдельный вопрос;
  * одиночная распаковка в сравнении с int: `raw_vrow(v) == -1` — сентинел уже
    ловит `is_vrow`, но это другое правило (П19), не это.

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-unwrap-compare"

# raw_<space>( ... ) == raw_<space>( ... ) -- одно вложение скобок хватает:
# аргументом бывает `@ctx.prims.int_id` или `representation_of(@ctx, t)`.
PAIR = re.compile(
    r"raw_[a-z_]+\([^()]*(?:\([^()]*\)[^()]*)*\)\s*(?:==|!=)\s*raw_[a-z_]+\("
)


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
    unwraps = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            code = line.split("//", 1)[0]
            unwraps += len(re.findall(r"raw_[a-z_]+\(", code))
            if PAIR.search(code):
                bad.append(f"  {rel}:{n}: {code.strip()[:96]}")

    if bad:
        print(f"{NAME}: FAIL — завёрнутый индекс распакован РАДИ СРАВНЕНИЯ:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  `==` работает на завёрнутом типе напрямую (замерено). А распаковка", file=sys.stderr)
        print("  сравнивает два int — и пропустит сравнение ДВУХ РАЗНЫХ пространств,", file=sys.stderr)
        print("  то есть открывает ту самую дыру, ради которой обёртка вводилась.", file=sys.stderr)
        print("  Убери обе распаковки: `x == y`. Сравнение с ДЛИНОЙ или числом законно", file=sys.stderr)
        print("  и не судится — это индексирование, ровно зачем `raw_*` и нужен.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, распаковок `raw_*`: {unwraps}, "
          f"распаковок ради сравнения: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
