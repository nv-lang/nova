# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-ref-field-names.py — поле-ссылка называет своё
пространство (конвенция П19).

ПОЧЕМУ. `int` в реестре — это всегда ссылка КУДА-ТО, и куда именно, из типа не
видно: id сущности, индекс строки, смещение. Имя — единственное место, где это
можно сказать, и суффикс делает ошибку «положил row туда, где ждали id»
видимой глазом, а не отладчиком.

СУФФИКСЫ: `_id` — id сущности из реестра; `_row` — индекс строки в векторе;
`_off`/`_cnt` — диапазон строк; `_len` — длина; `_line` — 1-based строка
ИСХОДНИКА (появился с ContractNote.src_line, волна канала контрактов
2026-09-02: номер строки — тоже ссылка, её пространство — текст файла).

ОДНО ЗАКОННОЕ ГОЛОЕ ИМЯ: `payload` — его смысл зависит от `kind`, и это сказано
в его доке.

ПОЧЕМУ PYTHON: shell-редакция поднимала `tr` и `awk` на КАЖДЫЙ файл (П14).

$1 — корень; $2 — override директории (по умолчанию novac/src/sem).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-ref-field-names"
RE_TYPE = re.compile(r"^(export )?type [A-Za-z_]")
RE_OPEN = re.compile(r"\{[ \t\v\f]*$")
RE_SUFFIX = re.compile(r"_(id|row|off|cnt|len|line)$")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src" / "sem"

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
        print(f"{NAME} ok: судить нечего (в {src} файлов .nv: 0)")
        return 0

    bad = []
    total = good = exempt = 0
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        inb = False
        text = f.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n")
        for n, raw in enumerate(text, 1):
            if RE_TYPE.match(raw):
                inb = bool(RE_OPEN.search(raw))
                continue
            if inb and raw.startswith("}"):
                inb = False
                continue
            if not inb:
                continue
            line = re.sub(r"//.*$", "", raw).strip(" \t\v\f")
            line = re.sub(r",$", "", line)
            if not line:
                continue
            parts = re.split(r"[ \t\v\f]+", line)
            if len(parts) < 2:
                continue
            fname, ftype = parts[0], parts[1]
            if ftype != "int":
                continue
            total += 1
            if fname == "payload":
                exempt += 1
                continue
            if RE_SUFFIX.search(fname):
                good += 1
                continue
            bad.append(f"  {rel}:{n}: поле `{fname} int` без суффикса пространства")

    if bad:
        print(f"{NAME}: FAIL — поле-ссылка не называет пространство (конвенция П19):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  '_id' — id сущности из реестра (id типа); '_row' — индекс строки в векторе;", file=sys.stderr)
        print("  '_off'/'_cnt' — диапазон строк. Голое имя легально одно — 'payload',", file=sys.stderr)
        print("  и только потому, что его смысл зависит от 'kind' (это сказано в его доке).", file=sys.stderr)
        return 1

    print(f"{NAME} ok: полей-ссылок int в реестрах: {total} (с суффиксом {good}, "
          f"полиморфных {exempt}), безымянных пространств: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
