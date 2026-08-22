# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-arch-invariants.py — у каждого раздела карты есть
счётчик инвариантов (274.1 §2б).

ПОЧЕМУ. Инвариант, записанный прозой, не существует для машины и не считается
(№636): его нельзя ни пересчитать, ни заметить пропажу. Счётчик раздела — это
обещание, которое можно проверить арифметикой.

СУДЯТСЯ §1–§10 (с буквенными подразделами): §11 и дальше — приложения, они
инвариантов не несут.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override пути к архитектуре (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-arch-invariants"
RE_JUDGED = re.compile(r"^## ([1-9]|10)[аб]?\.")
RE_ANY_HEAD = re.compile(r"^## ")
RE_COUNTER = re.compile(r"Счётчик( раздела)?: *\*{0,2}[0-9]")
RE_COUNTER_N = re.compile(r"Счётчик( раздела)?: *\**[0-9]")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    doc = pathlib.Path(a[2]) if len(a) > 2 else root / "docs" / "dev" / "novac-architecture.md"

    if not doc.is_file():
        print(f"{NAME}: FAIL — нет {doc}", file=sys.stderr)
        return 1

    lines = doc.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")

    bad = []
    sec, cnt = "", False
    for line in lines:
        if RE_JUDGED.match(line):
            if sec and not cnt:
                bad.append(f"  {sec}")
            sec, cnt = line, False
            continue
        if RE_ANY_HEAD.match(line):
            if sec and not cnt:
                bad.append(f"  {sec}")
            sec = ""
            continue
        if sec and RE_COUNTER.search(line):
            cnt = True
    if sec and not cnt:
        bad.append(f"  {sec}")

    if bad:
        print(f"{NAME}: FAIL — разделы карты без счётчика инвариантов:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Каждый раздел карты (§1–§10) обязан считать свои инварианты", file=sys.stderr)
        print("  (274.1 §2б; №636 — инварианты прозой не считаются существующими).", file=sys.stderr)
        return 1

    n = sum(1 for l in lines if RE_COUNTER_N.search(l))
    print(f"{NAME} ok: счётчики на месте ({n} строк счёта)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
