# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-ice-messages.py — текст `ice()` различает отказы
и называет модуль (П12).

ПОЧЕМУ. Site паники указывает на ТЕЛО `ice()`, а не на место вызова (аналога
`track_caller` нет). Значит различать отказы может только текст: одинаковый
текст в двух местах делает два разных отказа неразличимыми, а текст без имени
модуля не отвечает на первый вопрос читающего — где это сломалось.

ПРОВЕРЯЕТ ТРИ ВЕЩИ:
  1. тексты `ice("...")` уникальны;
  2. текст начинается с префикса модуля: «<модуль>: что сломалось»;
  3. УСЛОВНОГО `ice` нет: если у отказа есть условие, его место и условие умеет
     сообщить `assert` — он даёт файл, строку, свой текст и исходник условия.
     `ice()` остаётся там, где нужен never: позиция значения и недостижимая
     точка.

Тесты (`*_test.nv`) вне суда.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-ice-messages"
RE_ICE = re.compile(r'ice\("([^"]*)"')
RE_PREFIX = re.compile(r"^[a-z_]+: ")
RE_COND = re.compile(r"if .* \{ ice\(")


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

    entries = []                       # «rel:строка:текст», как их клал awk
    texts = []
    cond = []
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            for m in RE_ICE.finditer(line):
                entries.append(f"{rel}:{n}:{m.group(1)}")
                texts.append(m.group(1))
            if RE_COND.search(line):
                cond.append(f"{src}/{rel}:{n}:{line}")

    total = len(entries)
    if total == 0:
        print(f"{NAME} ok: судить нечего (вызовов ice() в {src}: 0)")
        return 0

    # --- дубли: один и тот же текст в двух местах ---------------------------
    seen, dups = set(), []
    for t in sorted(texts):
        if t in seen and t not in dups:
            dups.append(t)
        seen.add(t)
    if dups:
        print(f"{NAME}: FAIL — одинаковый текст ice() в разных местах "
              f"(П12: текст заменяет site):", file=sys.stderr)
        for m in dups:
            print(f"  «{m}» — здесь:", file=sys.stderr)
            for e in entries:
                if f":{m}" in e:
                    print(f"    {e}", file=sys.stderr)
        print("  Site паники указывает на тело ice(), а не на место вызова (аналога track_caller нет),", file=sys.stderr)
        print("  поэтому одинаковый текст делает два разных отказа неразличимыми. Назови в тексте", file=sys.stderr)
        print("  функцию или условие: «sem: leaf_text on a branch node».", file=sys.stderr)
        return 1

    # --- префикс модуля -----------------------------------------------------
    noprefix = [t for t in sorted(texts) if not RE_PREFIX.match(t)]
    if noprefix:
        print(f"{NAME}: FAIL — сообщение ice() без префикса модуля (П12):", file=sys.stderr)
        for m in noprefix:
            for e in entries:
                if f":{m}" in e:
                    print(f"  {e}", file=sys.stderr)
        print("  Форма: «<модуль>: что сломалось». Модуль первым словом — это первое,", file=sys.stderr)
        print("  что нужно знать читающему отказ, когда site указывает на дверь.", file=sys.stderr)
        return 1

    # --- условный ice -------------------------------------------------------
    if cond:
        print(f"{NAME}: FAIL — условный ice: у него есть условие, значит место "
              f"и условие может сообщить assert:", file=sys.stderr)
        for c in cond:
            line = c.replace(f"{src}/", "  ", 1)
            print(line.encode("utf-8")[:160].decode("utf-8", "ignore"), file=sys.stderr)
        print('  Пиши assert(<обратное условие>, "текст"): он даёт файл, строку, свой текст и', file=sys.stderr)
        print("  исходник условия, а §7-строку печатает граница супервизора (П7 п.2).", file=sys.stderr)
        print("  ice() остаётся там, где нужен never: позиция значения и недостижимая точка.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: вызовов ice(): {total}, все тексты уникальны и с префиксом модуля")
    return 0


if __name__ == "__main__":
    sys.exit(main())
