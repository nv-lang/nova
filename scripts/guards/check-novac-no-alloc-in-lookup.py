# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-alloc-in-lookup.py — дверь поиска НЕ аллоцирует
(конвенция П18, П14 «скорость компиляции — приоритет №1»).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19).

  ДВЕРЬ — это функция, которая: либо метод таблицы (имя приёмника кончается на
  `Table`), либо спрашивает таблицу (`.find(`/`.lookup(`) и при этом НИЧЕГО не
  пишет, либо объявлена дверью пометкой `#realtime nogc` — тогда она судится как
  дверь, что бы ни делала внутри (живой случай владельца 2026-08-16:
  `builtin_types()` строил вектор на каждый вызов `prim_c_name`, а `.find` в нём
  не было, и страж молчал).

  ПИШЕТ — это буфер, ДАННЫЙ снаружи: `StringBuilder` в сигнатуре или
  `@body.append(` в теле. Локальный StringBuilder внутри двери ролью эмиттера не
  считается — иначе дверь пряталась бы за собственным билдером (случай 5
  самотеста).

  ВНУТРИ ДВЕРИ запрещены: конструирование вектора (`.of(`, `[]T.new()`), сборка
  строки интерполяцией `${`, склейка `.concat(`, заведение `StringBuilder`.
  Дверь без пометки `#realtime nogc` — тоже красное (компилятор не проверяет
  того, что не объявлено); мутирующая форма (`mut @`) регистрирует, а не ищет, и
  пометки не требует. Строка с `ice(` не судится. Комментарии не судятся.

  `mangle.nv` исключён целиком: он ПИШЕТ имена, это его работа.

ПОЧЕМУ PYTHON: shell-редакция поднимала `tr` и `awk` на КАЖДЫЙ файл — 2.0с там,
где работы на доли секунды.

$1 — корень репозитория; $2 — override пути к novac/src (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-alloc-in-lookup"


def scan_file(rel, text, bad):
    fname = ""
    is_door = is_table = asks = writes = has_rt = is_mut = False
    hits = []
    attr = False

    def flush():
        nonlocal fname, is_door, is_table, asks, writes, has_rt, is_mut, hits
        if fname:
            door = is_door or is_table or (asks and not writes)
            if door:
                for ln, why, txt in hits:
                    bad.append(f"  {rel}:{ln}: дверь {fname} — {why}: {txt}")
                if not has_rt and not is_mut:
                    bad.append(f"  {rel}:{fnline[0]}: дверь {fname} — без пометки #realtime nogc: "
                               f"компилятор не проверяет того, что не объявлено")
        fname = ""
        is_door = is_table = asks = writes = has_rt = is_mut = False
        hits = []

    fnline = [0]
    for i, raw in enumerate(text.split("\n"), 1):
        if re.match(r"^#realtime\s+nogc", raw):
            attr = True
            continue
        if re.match(r"^(export )?fn ", raw):
            flush()
            has_rt = attr
            attr = False
            fname = re.sub(r"\(.*$", "", raw)
            fname = re.sub(r"^(export )?fn ", "", fname).rstrip()
            fnline[0] = i
            is_mut = bool(re.search(r"\smut\s+@", raw))
            is_table = bool(re.match(r"^[A-Za-z0-9_]*Table(\s|$)", fname))
            if is_table:
                is_door = True
            if "StringBuilder" in raw:
                writes = True
        elif re.match(r"^(export )?type ", raw):
            flush()

        line = re.sub(r"//.*$", "", raw)
        if not line.strip():
            continue
        if not fname:
            continue
        if ".find(" in line or ".lookup(" in line:
            asks = True
        if has_rt:
            asks = True
        attr = False
        if "@body.append(" in line:
            writes = True
        if "ice(" in line:
            continue
        why = ""
        if ".of(" in line or re.search(r"\]\.new\(\)", line):
            why = "конструируется вектор внутри двери"
        elif "${" in line:
            why = "строка собирается интерполяцией"
        elif ".concat(" in line:
            why = "строка склеивается concat"
        elif "StringBuilder" in line:
            why = "заводится StringBuilder"
        if why:
            hits.append((i, why, line.strip()[:60]))
    flush()


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    judged = [p for p in sorted(src.rglob("*.nv")) if not p.name.endswith("_test.nv")]
    if not judged:
        print(f"{NAME} ok: судить нечего (в {src} файлов .nv: 0)")
        return 0

    bad = []
    for p in judged:
        if p.name == "mangle.nv":
            continue
        scan_file(p.relative_to(src).as_posix(),
                  p.read_text(encoding="utf-8", errors="replace").replace("\r", ""), bad)

    if bad:
        print(f"{NAME}: FAIL — дверь поиска аллоцирует (конвенция П18, П14 «скорость»):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Функция, которая сама спрашивает таблицу (.find/.lookup), не строит.", file=sys.stderr)
        print("  Составной ключ — не текст: свяжи одноимённые строки и сравни целые.", file=sys.stderr)
        return 1

    # Как в shell-редакции: файлы, где вообще есть .find/.lookup.
    doors = sum(1 for p in judged
                if any(x in p.read_text(encoding="utf-8", errors="replace")
                       for x in (".find(", ".lookup(")))
    print(f"{NAME} ok: файлов .nv: {len(judged)} (из них с дверями поиска: {doors}), аллокаций в дверях: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
