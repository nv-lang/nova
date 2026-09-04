# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-naked-panic.py — явный инвариант идёт через
дверь `ice()`, а не голым `panic(` (конвенция П12.1).

ПОЧЕМУ. `ice(...)` из `novac.diag` рендерит `E_NOVAC_ICE` по схеме §7 и лишь
затем умирает по правилу языка. Голый `panic` — строка свободной формы без места
и без схемы: машинный читатель её не разберёт, а человек не поймёт, где сломался
инвариант.

ВНЕ СУДА: сама дверь (`diag/diag.nv`) — там `panic` и живёт, — и строки с
комментарием: упоминание в прозе не вызов.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень репозитория.
"""
import os
import pathlib
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-naked-panic"


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    src = root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (novac/src ещё нет)")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    hits = []
    for f in files:
        shown = str(f).replace("\\", "/")
        if shown.endswith("diag/diag.nv"):
            continue
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            if "panic(" not in line:
                continue
            if "// " in line:
                continue
            hits.append(f"{shown}:{n}:{line}")

    if hits:
        print(f"{NAME}: FAIL — голый panic( вне двери ice() ({len(hits)}):", file=sys.stderr)
        for h in hits[:10]:
            print(f"    {h}", file=sys.stderr)
        print("  Явный инвариант идёт через ice(...) из novac.diag (П12.1):", file=sys.stderr)
        print("  она рендерит E_NOVAC_ICE по схеме §7 и лишь затем умирает", file=sys.stderr)
        print("  по правилу языка. Голый panic — строка свободной формы без", file=sys.stderr)
        print("  места и схемы; машинный читатель её не разберёт.", file=sys.stderr)
        return 1

    if not files:
        # МИШЕНЬ УЕХАЛА, А НЕ «НАРУШЕНИЙ НЕТ» (класс №911, страж
        # check-guard-empty-root): каталог есть, подсудных файлов ноль —
        # печатать здесь правдоподобный счёт значит выдавать пустоту за
        # проверенное. Формулировка донорская, от check-novac-file-size.py.
        print(f"{NAME} ok: судить нечего (0 .nv-файлов в {src})")
        return 0

    print(f"{NAME} ok: голых panic( в novac/src нет (дверь — ice() в diag)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
