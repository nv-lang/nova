# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-resolve-discipline.py — резолв имя→id идёт через
дверь, а промах резолва не становится тихим дефолтом.

ЧЕТЫРЕ ФОРМЫ, и каждая — один и тот же класс №652 «промах решает молча»:
  1. сравнение имён (`== name`) вне `names/` — это линейный скан там, где есть
     `names.NameTable` с O(1);
  2. `< 0` и следом `return T_INT` / `return 0` — промах превращается в тип;
  3. хвост-дефолт `T_INT` / `"nova_int"` последней строкой;
  4. остаточная ветка `return T_*` — хвост обязан быть `ice`.

Комментарии не судятся: доккомментарий, ЦИТИРУЮЩИЙ запрещённую форму, краснел
(правка 2026-08-17).

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-resolve-discipline"
RE_COMMENT = re.compile(r"^[ \t\v\f]*//")
RE_NAMECMP = re.compile(r"[=!]= (name|fname)([^A-Za-z0-9_]|$)")
RE_MISS = (re.compile(r"< 0.*return T_INT"), re.compile(r"< 0.*return 0 \}"))
RE_TAIL = re.compile(r'^[ \t\v\f]*(T_INT|"nova_int")[ \t\v\f]*$')
RE_TAILRET = re.compile(r"^[ \t\v\f]*return (T_INT|T_STR|T_BOOL|T_F64)[ \t\v\f]*$")


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
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        in_names = rel.startswith("names/") or "/names/" in rel
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            is_comment = bool(RE_COMMENT.match(line))
            if not in_names and not is_comment and RE_NAMECMP.search(line):
                bad.append(f"  {rel}:{n}:{line} — линейный резолв сравнением имён "
                           f"(дверь — names.NameTable)")
            if any(rx.search(line) for rx in RE_MISS):
                bad.append(f"  {rel}:{n}:{line} — промах резолва становится тихим дефолтом "
                           f"(класс №652)")
            if RE_TAIL.match(line):
                bad.append(f"  {rel}:{n}:{line} — хвост-дефолт вместо честного отказа")
            if RE_TAILRET.match(line):
                bad.append(f"  {rel}:{n}:{line} — остаточная ветка решает тип "
                           f"(хвост обязан быть ice)")

    if bad:
        print(f"{NAME}: FAIL — дисциплина резолва нарушена:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Резолв имя→id — только через names.NameTable (O(1));", file=sys.stderr)
        print("  промах резолва — ice()/диагностика в check, НИКОГДА не тихий int.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, линейных сканов и тихих int-дефолтов: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
