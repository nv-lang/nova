# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-line-length.py — строка .nv не длиннее предела
(П29).

ТРИ ИСКЛЮЧЕНИЯ, которые страж пропускает сам, — потому что перенести их
нельзя или незачем:
  1. образец арма: до `=>` только имена, `|` и пробелы;
  2. одна длинная строковая литера (60+ символов внутри кавычек);
  3. хвостовой `///`-док, когда КОД до него укладывается в предел.

`import` ИСКЛЮЧЕНИЕМ БОЛЬШЕ НЕ ЯВЛЯЕТСЯ (2026-08-27). Прежнее «язык не переносит»
верно лишь наполовину: перенос ВНУТРИ `{...}` парсер действительно не принимает
(`expected identifier, got newline`), но несколько строк `import ../m.{...}` из
ОДНОГО модуля компилятор собирает — замерено `nova build` на пробном модуле. Длинный
импорт режется на строки по именам, и десять таких строк в novac/src (до 660 байт)
порезаны той же волной. Исключение держалось не на свойстве языка, а на
непроверенном утверждении о нём.

ДЛИНА СЧИТАЕТСЯ В БАЙТАХ. Так её считал awk под `LC_ALL=C`, и так же считает
терминал, в котором эту строку читают: кириллица в комментарии занимает два
байта на букву. Считать символы значило бы поднять фактический предел вдвое для
русских комментариев и опустить вердикт относительно shell-редакции.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории; $3 — предел (по умолчанию 120).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-line-length"
RE_ARM = re.compile(r"^[ \t\v\f]*[A-Za-z_][A-Za-z0-9_]*"
                    r"([ \t\v\f]*\|[ \t\v\f]*[A-Za-z_][A-Za-z0-9_]*)+[ \t\v\f]*$")
RE_LONG_LIT = re.compile(r'"[^"]{60,}"')


def lines_of(path, enc="utf-8"):
    """Строки как их видел awk: хвостовой перевод НЕ даёт лишней записи."""
    out = path.read_bytes().decode(enc, "replace").split("\n")
    if out and out[-1] == "":
        out.pop()
    return out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    lim = int(a[3]) if len(a) > 3 else 120

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
    total = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        # latin-1: один байт — один символ, ровно как у awk под LC_ALL=C.
        for n, line in enumerate(lines_of(f, "latin-1"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            total += 1
            if len(line) <= lim:
                continue
            p = line.find("=>")
            if p >= 0 and RE_ARM.match(line[:p]):
                continue
            if RE_LONG_LIT.search(line):
                continue
            p = line.find("///")
            if p > 0:
                code = line[:p].rstrip(" \t\v\f")
                if len(code) <= lim:
                    continue
            bad.append(f"  {rel}:{n} — {len(line)} символов (предел {lim})")

    if bad:
        print(f"{NAME}: FAIL — строки длиннее {lim} символов (П29):", file=sys.stderr)
        for b in bad[:20]:
            print(b, file=sys.stderr)
        if len(bad) > 20:
            print(f"  ... и ещё {len(bad) - 20}", file=sys.stderr)
        print("  Перенеси: прозу комментария по словам, длинное выражение — во", file=sys.stderr)
        print("  вспомогательное имя или в if-цепочку; длинный import — на несколько", file=sys.stderr)
        print("  строк `import ../m.{...}` того же модуля. Исключения (образец арма,", file=sys.stderr)
        print("  одна длинная литера, хвостовой ///-док) страж пропускает сам.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: строк .nv: {total}, длиннее {lim} символов вне четырёх исключений: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
