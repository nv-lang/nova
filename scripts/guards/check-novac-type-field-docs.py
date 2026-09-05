# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-type-field-docs.py — у каждого типа, функции и поля
есть документация (конвенция П13).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19):
  * `type` и `fn` — ///-док СТРОКОЙ ВЫШЕ. Строки-атрибуты (`#impl(...)`,
    `#realtime nogc`) между доком и декларацией память о доке НЕ сбрасывают;
  * поле записи — с пином оракула >= 9a69411b3 требуется ///-док (D104 rev-2):
    хвостом на строке поля или строкой выше; до этого пина принимается
    переходная форма `//`. Пин читается из `novac/nova.toml`, строка
    `#  oracle-pin: <sha>`, и сравнивается через `git merge-base --is-ancestor`.

ПОЧЕМУ PYTHON: shell-редакция поднимала awk на КАЖДЫЙ файл — 1.5-3.0с там, где
работы на доли секунды (П14).

$1 — корень репозитория; $2 — override пути к novac/src (шов самотеста).
"""
import pathlib
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-type-field-docs"
PIN_GATE = "9a69411b3"


def field_strict(root):
    toml = root / "novac" / "nova.toml"
    if not toml.is_file():
        return False
    m = re.search(r"^#\s*oracle-pin:\s*([0-9a-f]+)",
                  toml.read_text(encoding="utf-8", errors="replace").replace("\r", ""), re.M)
    if not m:
        return False
    r = subprocess.run(["git", "-C", str(root), "merge-base", "--is-ancestor", PIN_GATE, m.group(1)],
                       capture_output=True)
    return r.returncode == 0


def scan(rel, text, strict, bad):
    prev_comment = prev_doc = False
    in_block = False
    for i, raw in enumerate(text.split("\n"), 1):
        line = raw
        if re.match(r"^[ \t]*///", line):
            prev_comment = prev_doc = True
            continue
        if re.match(r"^[ \t]*//", line):
            prev_comment, prev_doc = True, False
            continue
        if re.match(r"^[ \t]*#", line):
            continue

        is_type = bool(re.match(r"^(export )?type [A-Za-z_]", line))
        is_fn = bool(re.match(r"^(export )?fn ", line))
        if is_fn:
            if not prev_doc:
                bad.append(f"  {rel}:{i}: функция без ///-дока: {line[:70]}")
            in_block = False
        elif is_type:
            if not prev_doc:
                bad.append(f"  {rel}:{i}: тип без ///-дока: {line}")
            in_block = bool(re.search(r"\{[ \t]*$", line))
        elif in_block:
            if line.startswith("}"):
                in_block = False
            elif re.match(r"^[ \t]+[a-z_][a-zA-Z0-9_]* ", line):
                if strict:
                    if "///" not in line and not prev_doc:
                        bad.append(f"  {rel}:{i}: поле без ///-дока (D104 rev-2: trailing или сверху): {line}")
                elif "//" not in line and not prev_comment:
                    bad.append(f"  {rel}:{i}: поле без комментария: {line}")
        prev_comment = prev_doc = False


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    strict = field_strict(root)
    judged = [p for p in sorted(src.rglob("*.nv")) if not p.name.endswith("_test.nv")]

    bad = []
    for p in judged:
        scan(p.relative_to(src).as_posix(),
             p.read_text(encoding="utf-8", errors="replace").replace("\r", ""), strict, bad)

    if bad:
        print(f"{NAME}: FAIL — типы/функции/поля без документации (конвенция П13):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  type и fn — ///-док строкой выше, простыми словами, коротко, по-английски;", file=sys.stderr)
        print("  поле — // на строке поля (что хранит, чем индексируется).", file=sys.stderr)
        return 1

    mode = "/// строго (D104 rev-2)" if strict else "переходный // (пин < 9a69411b3)"
    if not judged:
        # МИШЕНЬ УЕХАЛА, А НЕ «НАРУШЕНИЙ НЕТ» (класс №911, страж
        # check-guard-empty-root): каталог есть, подсудных файлов ноль —
        # печатать здесь правдоподобный счёт значит выдавать пустоту за
        # проверенное. Формулировка донорская, от check-novac-file-size.py.
        print(f"{NAME} ok: судить нечего (0 .nv-файлов в {src})")
        return 0

    print(f"{NAME} ok: файлов .nv: {len(judged)}, типов/функций/полей без документации: 0 (поля: {mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
