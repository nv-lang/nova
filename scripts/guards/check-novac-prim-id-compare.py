#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-prim-id-compare.py — «это целое?» спрашивается у
ДВЕРИ СЕМЬИ, а не сравнением с одним представителем (реестр №910, план 274.5
§3-пред59).

ЗАЧЕМ. До 2026-09-04 чекер и эмиттер novac спрашивали «это целое» как
`t == @ctx.prims.int_id` и «это вещественное» как `t == @ctx.prims.f64_id` — в
двенадцати местах. Так каждый из десяти остальных числовых типов универсума
(`i8..i64`, `u8..u64`, `uint`, `f32`) выпадал из литералов, арифметики,
сравнений и печати РАЗОМ, хотя был объявлен в таблице `builtins`. Дверь семьи
(`sem/type_shape.nv`: `prim_row_of`, `is_int_prim`, `is_float_prim`,
`is_numeric_prim`) читает таблицу, и ей нечему расходиться; сравнение с id —
копия таблицы в одну строку, и она расходится при первом же новом типе.

ЧТО СЧИТАЕТ: строки `novac/src/check/*.nv` и `novac/src/emit_c/*.nv` (без
`*_test.nv`), в которых `TyId` сравнивается (`==`/`!=`) с `prims.int_id` или
`prims.f64_id` — с любой стороны оператора, с `@ctx.` или `ctx.` впереди.
Остальные id (`str_id`, `bool_id`, `char_id`, `unit_id`) не считаются: у их
семей один член, и сравнение с id и есть вопрос о семье.

ХРАПОВИК ВНИЗ: база — `scripts/guards/novac-prim-id-compare.baseline`
(`count=N`), рост над базой красный. Остаток на день заведения назван в базе
построчно: пара `int`/`f64` в `int_f64_mix` (измеренное поведение оракула, не
семья), индекс обязан быть `int` (`binds.nv`), хвостовой `return 0` для
`int`-функций формы E1 (`emit_c.nv`). Цель — только эти; всё прочее уходит в
дверь.

МИШЕНЬ НЕ ПОТЕРЯНА: ноль файлов `.nv` под судом — красное, а не «сравнений 0».
Урок охоты guards × К7 того же дня: девять стражей из десяти печатали зелёный
ноль, когда их якорь уезжал.

Аргументы: $1 — корень репозитория (по умолчанию — репозиторий стража);
$2 — override каталога `novac/src` (шов самотеста); env NOVAC_PRIM_ID_BASELINE —
override базы. Самотест: selftest/test-check-novac-prim-id-compare.sh.
Вход для гейта — `main()` (run-guards.py исполняет стражей в одном процессе и
зовёт именно её; страж без `main` падает трейсбеком, а не вердиктом — поймано
первым же прогоном loop-яруса 2026-09-04).
"""
import io
import os
import re
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")

NAME = "check-novac-prim-id-compare"

# `x == @ctx.prims.int_id`, `ctx.prims.f64_id != y` — either side, either prefix.
PAT = re.compile(r"(==|!=)\s*@?ctx\.prims\.(int|f64)_id\b|@?ctx\.prims\.(int|f64)_id\s*(==|!=)")


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def judged_files(src):
    out = []
    for sub in ("check", "emit_c"):
        d = os.path.join(src, sub)
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            if fn.endswith(".nv") and not fn.endswith("_test.nv"):
                out.append(os.path.join(d, fn))
    return out


def shown(p, root):
    """The path as the reader will look for it: relative to the root when it
    lives under it; as given when the selftest points at another drive (Windows
    relpath refuses to cross mounts -- the first selftest run tripped on it)."""
    try:
        return os.path.relpath(p, root).replace("\\", "/")
    except ValueError:
        return p.replace("\\", "/")


def main():
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    src = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else os.path.join(root, "novac", "src")
    base_file = os.environ.get("NOVAC_PRIM_ID_BASELINE",
                               os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                            "novac-prim-id-compare.baseline"))

    files = judged_files(src)
    if not files:
        return fail("под судом ни одного файла .nv в %s/{check,emit_c} — мишень потеряна, а не «сравнений 0»" % src)

    hits = []
    for p in files:
        with io.open(p, encoding="utf-8", errors="replace") as f:
            for n, line in enumerate(f, 1):
                code = line.split("//", 1)[0]
                if PAT.search(code):
                    hits.append("%s:%d: %s" % (shown(p, root), n, code.strip()[:110]))

    try:
        base_t = io.open(base_file, encoding="utf-8", errors="replace").read()
    except IOError:
        return fail("нет базы %s (ключ count=N) — храповик судить нечем" % base_file)
    m = re.search(r"^count=(\d+)\s*$", base_t, re.M)
    if not m:
        return fail("в базе %s нет строки count=N — храповик судить нечем" % base_file)
    base = int(m.group(1))

    if len(hits) > base:
        sys.stderr.write("%s: FAIL — сравнений TyId с prims.int_id/f64_id: %d, база %d. «Это целое?» спрашивают у двери "
                         "семьи (sem/type_shape.nv: is_int_prim / is_float_prim / is_numeric_prim / prim_row_of), "
                         "не у одного id (№910):\n" % (NAME, len(hits), base))
        for h in hits:
            sys.stderr.write("    %s\n" % h)
        return 1

    print("%s ok: файлов .nv: %d, сравнений с int_id/f64_id: %d (база %d) — семья спрашивается у двери"
          % (NAME, len(files), len(hits), base))
    return 0


if __name__ == "__main__":
    sys.exit(main())
