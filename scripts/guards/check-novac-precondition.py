# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-precondition.py — предусловие живёт в сигнатуре,
а не первой строкой тела (П20 п.5).

ПОЧЕМУ. `assert` первой строкой тела — это предусловие, спрятанное от читающего
СИГНАТУРУ: договор функции виден только тому, кто открыл её тело. Клауз
`requires` может быть НЕСКОЛЬКО, у каждой своё сообщение, и все они стоят там,
где договор и читают.

`assert` ГЛУБЖЕ в теле законен: он об инварианте над уже вычисленным, а не над
входом. Поэтому судится ровно первая содержательная строка тела.

ТЕЛО НАЧИНАЕТСЯ либо на той же строке (`fn f() {`), либо ниже — когда между
сигнатурой и телом стоят клаузы `requires`.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-precondition"
RE_FN = re.compile(r"^(export )?fn ")
RE_OPEN = re.compile(r"\{[ \t\v\f]*$")


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
            if nm.endswith(".nv") and not nm.endswith("_test.nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень (класс №519)",
              file=sys.stderr)
        return 1

    bad = []
    nfn = 0
    # Состояние НЕ сбрасывается на границе файла: так вёл себя единый awk-проход.
    infn = body = False
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, raw in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if raw.endswith("\r"):
                raw = raw[:-1]
            if RE_FN.match(raw):
                infn = True
                body = bool(RE_OPEN.search(raw))
                continue
            if infn and not body and RE_OPEN.search(raw):
                body = True
                continue
            if body:
                line = raw.lstrip(" \t\v\f")
                if not line:
                    continue
                if line.startswith("//"):
                    continue
                nfn += 1
                if line.startswith("assert("):
                    bad.append(f"  {rel}:{n} — assert первой строкой тела: это предусловие, "
                               f"его место в сигнатуре (requires)")
                body = infn = False

    if nfn == 0:
        print(f"{NAME}: FAIL — не нашлось ни одного тела функции: разбор сломался (класс №519)",
              file=sys.stderr)
        return 1
    if bad:
        print(f"{NAME}: FAIL — предусловие спрятано в теле (П20 п.5):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print('  Перенеси в сигнатуру: requires <условие>, "текст" — клауз может быть', file=sys.stderr)
        print("  НЕСКОЛЬКО, у каждой своё сообщение. assert глубже в теле законен: он", file=sys.stderr)
        print("  об инварианте над уже вычисленным, а не над входом.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: тел функций проверено: {nfn}, предусловий в теле: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
