# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-doc-language.py — дока, комментарии и строковые
литералы в novac/**.nv по-английски (конвенция П13).

ПОЧЕМУ. Строковые литералы .nv — это тексты диагностик КОМПИЛЯТОРА: их читает
пользователь языка, а не автор. Комментарии и `///`-док читает следующий, кто
сюда придёт, и он не обязан знать русский.

ЧТО ЗАКОННО КИРИЛЛИЦЕЙ: ссылка на правило или запись — `П13.5`, `№652`,
`§10.3а`, `§ 2б`. Форма ссылки: знак, цифры (можно через точку) и необязательная
буква. Такие ссылки снимаются со строки ПЕРЕД проверкой, и всё, что осталось
кириллицей, — нарушение.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-doc-language"
# Кириллический блок целиком (U+0400..U+047F) — то же множество, что shell-
# редакция набирала байтами UTF-8.
CYR = re.compile(r"[Ѐ-ѿ]")
# Необязательный буквенный суффикс ссылки: а-я и ё.
REF = re.compile(r"(П|№|§ ?)[0-9]+(\.[0-9]+)*[а-яё]?")


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

    bad = []
    refs = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        text = f.read_bytes().decode("utf-8", "replace").split("\n")
        if text and text[-1] == "":
            text.pop()
        for n, line in enumerate(text, 1):
            if line.endswith("\r"):
                line = line[:-1]
            probe, cnt = REF.subn("", line)
            refs += cnt
            if CYR.search(probe):
                bad.append(f"  {rel}:{n}: {line}")

    if bad:
        print(f"{NAME}: FAIL — русский текст в novac (конвенция П13: дока, комментарии "
              f"и строковые литералы .nv — по-английски):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Как чинить: перевести эти строки на английский — и ///-док, и обычные", file=sys.stderr)
        print("  // комментарии, и строковые литералы (это тексты диагностик компилятора).", file=sys.stderr)
        print("  Единственное, что тут законно кириллицей, — ссылки вида П13.5, №652 и §10.3а.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, строк с кириллицей: 0 "
          f"(законных ссылок П/№/§ + цифра: {refs})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
