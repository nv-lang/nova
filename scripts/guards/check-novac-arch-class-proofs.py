# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-arch-class-proofs.py — у каждого класса проблем в
архитектуре есть все три доказательства (план 274.1 §4).

ТРИ ДОКАЗАТЕЛЬСТВА, и ни одно не заменяет другого:
  **Верность:** почему решение убивает класс;
  **Место:** какой модуль и какие слои карты;
  **Минимальность:** что ломается при снятии КАЖДОГО инварианта.
Класс с одним доказательством — это намерение, а не решение.

Раздел «Классы проблем» обязан существовать (требование приёмки 274.1, владелец
2026-08-14): его отсутствие — красный, а не «судить нечего».

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override пути к архитектуре (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-arch-class-proofs"
RE_SEC = re.compile(r"^## .*Классы проблем")
RE_HEAD = re.compile(r"^## ")
RE_CLASS = re.compile(r"^### К[0-9]")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    doc = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "dev" / "novac-architecture.md"

    if not doc.is_file():
        print(f"{NAME}: FAIL — нет {doc}", file=sys.stderr)
        return 1

    lines = doc.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")

    if not any(RE_SEC.match(l) for l in lines):
        print(f"{NAME}: FAIL — в архитектуре нет раздела «Классы проблем»", file=sys.stderr)
        print("  Требование приёмки 274.1: раздел обязан существовать (владелец 2026-08-14).",
              file=sys.stderr)
        return 1

    bad = []
    n_classes = 0
    in_sec = False
    cls, v, m, mi = "", False, False, False

    def check():
        if not cls:
            return
        if not (v and m and mi):
            miss = ("" if v else " Верность") + ("" if m else " Место") + \
                   ("" if mi else " Минимальность")
            bad.append(f"  {cls} — не хватает:{miss}")

    for line in lines:
        if RE_SEC.match(line):
            in_sec = True
            continue
        if in_sec and RE_HEAD.match(line):
            in_sec = False
        if in_sec and RE_CLASS.match(line):
            check()
            cls, v, m, mi = line, False, False, False
            n_classes += 1
            continue
        if in_sec and cls:
            if "**Верность:**" in line:
                v = True
            if "**Место:**" in line:
                m = True
            if "**Минимальность:**" in line:
                mi = True
    check()

    if bad:
        print(f"{NAME}: FAIL — классы без полного набора доказательств:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Каждому классу: **Верность:** (почему решение убивает класс),", file=sys.stderr)
        print("  **Место:** (модуль/слои карты), **Минимальность:** (что ломается", file=sys.stderr)
        print("  при снятии каждого инварианта). План 274.1 §4.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: классов {n_classes}, у каждого все три доказательства")
    return 0


if __name__ == "__main__":
    sys.exit(main())
