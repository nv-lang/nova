# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-pch.py — компиляция C идёт с предкомпилированной
прелюдией (П28).

ПРАВИЛО. PCH во ВСЕХ режимах: собрать один раз на ревизию оракула
(`-x c-header` -> `*.pch` со штампом), дальше каждая компиляция несёт
`-include-pch`. Линковка и сборка самого PCH исключены — им кэшировать нечего.

ПРОВЕРЯЕТ ЧЕТЫРЕ ВЕЩИ:
  * каждая строка компиляции C (`-c`) несёт `-include-pch`;
  * хоть один инструмент СТРОИТ PCH — иначе кэшировать нечего;
  * имя PCH несёт штамп ревизии оракула: протухший кэш даёт мусорный объектник;
  * PCH строится не безусловно — иначе цена возвращается на каждый прогон.

РАСШИРЕНИЕ НЕ ЧАСТЬ ЛИЧНОСТИ ИНСТРУМЕНТА: судятся и `*.sh`, и `*.py` (класс
F121 — судья, перечисляющий подсудимых глобом по расширению, слепнет по мере
переезда инструментов на python).

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории со скриптами (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-pch"
RE_CLANG = re.compile(r'REAL_CLANG|\$CL "|clang\.exe')
RE_STAMP = re.compile(r"PCH=.*\$[A-Z_]*STAMP|prelude-\$")
RE_GUARDED = re.compile(r'if \[ ! -f "\$PCH" \]|\[ -f "\$PCH" \] \|\|')


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "scripts"

    files = []
    if src.is_dir():
        for dirpath, _dirs, names in os.walk(src):
            for nm in names:
                stem, ext = os.path.splitext(nm)
                if ext not in (".sh", ".py"):
                    continue
                if stem.startswith("novac-") or stem.startswith("check-novac-"):
                    files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного инструмента novac: "
              f"страж потерял мишень (класс №519)", file=sys.stderr)
        return 1

    bad = []
    npch = ncomp = 0
    for f in files:
        rel = f.name
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            if re.match(r"^[ \t\v\f]*#", line):
                continue
            if not RE_CLANG.search(line):
                continue
            if "-x c-header" in line:
                npch += 1
                continue
            if " -c " not in line:
                continue
            ncomp += 1
            if "-include-pch" not in line:
                bad.append(f"  {rel}:{n} — компиляция C без -include-pch: "
                           f"каждый вызов снова разбирает заголовки")

    if ncomp == 0:
        print(f"{NAME}: FAIL — не найдено ни одной компиляции C: разбор сломался, "
              f"а молчать нельзя (класс №519)", file=sys.stderr)
        return 1
    if npch == 0:
        bad.append("  ни один инструмент не СТРОИТ PCH (-x c-header -> *.pch): кэшировать нечего")

    smoke = root / "scripts" / "tools" / "novac-e1-smoke.sh"
    if smoke.is_file():
        text = smoke.read_text(encoding="utf-8", errors="replace")
        if not RE_STAMP.search(text):
            bad.append("  имя PCH не несёт штамп ревизии оракула: протухший кэш "
                       "даст мусорный объектник")
        if not RE_GUARDED.search(text):
            bad.append("  PCH строится безусловно (нет проверки «файла нет»): "
                       "цена возвращается на каждый прогон")

    if bad:
        print(f"{NAME}: FAIL — компиляция C без предкомпилированной прелюдии (П28):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Правило: PCH во ВСЕХ режимах. Собрать один раз на ревизию оракула", file=sys.stderr)
        print("  (-x c-header -> *.pch со штампом), дальше каждая компиляция несёт", file=sys.stderr)
        print("  -include-pch. Линковка и сборка самого PCH исключены.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: компиляций C: {ncomp}, все с -include-pch; "
          f"сборок PCH: {npch} (по штампу оракула, кэшируется)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
