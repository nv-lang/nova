# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-string-build-door.py — строка собирается ЧЕРЕЗ
StringBuilder, а не склейкой самой с собой.

ПОЧЕМУ. `out = "${out}..."` в цикле создаёт новую строку на каждом шаге и
копирует всё уже собранное: квадратично по длине результата. Хуже цены то, что
такая запись УЧИТ — следующий читатель берёт форму из соседнего кода, и она
расходится по компилятору бесшумно.

У проекта уже есть дверь и прецедент: эмиттер держит `consume body
StringBuilder` и пишет весь C одним проходом (`emit_c/shell.nv:76`). Правило
лишь запрещает вторую форму рядом с первой.

ЗАМЕР, ЗАВОДЯЩИЙ СТРАЖА (указание владельца 2026-08-31): в `resolve.nv`
`render_sig` собирал подпись протокола склейкой в цикле по параметрам. После
правки в коде компилятора таких мест НОЛЬ.

ЧТО СУДИТСЯ: присваивание, где справа интерполируется ТА ЖЕ переменная —
`x = "${x}...`. Ищется в `.nv` под `novac/src`, КРОМЕ `_test.nv`.

ПОЧЕМУ ТЕСТЫ ИСКЛЮЧЕНЫ, и это названо, а не умолчано: тест собирает текст
фикстуры ОДИН раз, длина известна автору и видна в самом тесте (сейчас это
`pipeline_test.nv`, строящий вложенность для проверки глубины #807). Цена
ограничена и заплачена однажды, а читаемость такой записи в тесте выше, чем у
билдера. Правило — про код, который исполняется на пользовательских программах.

ПЛАН: docs/plans/274.5-read-own-source.md (novac читает свой исходник);
правило — П36 конвенции novac.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-string-build-door"
# `имя = "...${имя}` — присваивание, где справа интерполируется та же переменная.
RE_SELF = re.compile(r'(\b[a-z_]\w*)\s*=\s*"[^"]*\$\{\s*\1\s*[}\.]')


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

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv вне тестов: страж потерял мишень",
              file=sys.stderr)
        return 1

    bad = []
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            s = line.lstrip(" \t\v\f")
            # комментарий, ЦИТИРУЮЩИЙ запрещённую форму, законен
            if s.startswith("//"):
                continue
            if RE_SELF.search(line):
                bad.append(f"  {rel}:{n} — строка склеивается сама с собой: {s[:72]}")

    if bad:
        print(f"{NAME}: FAIL — строка собирается склейкой, а не билдером:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  `out = \"${out}...\"` в цикле создаёт новую строку на каждом шаге и", file=sys.stderr)
        print("  копирует всё собранное — квадратично по длине результата.", file=sys.stderr)
        print("  Дверь уже есть и рядом: `consume sb = StringBuilder.new()`,", file=sys.stderr)
        print("  `sb.append(...)`, `sb.into_str()` — так эмиттер пишет весь C", file=sys.stderr)
        print("  одним проходом (emit_c/shell.nv:76). В позиции с явным ожидаемым", file=sys.stderr)
        print("  типом хвост пишется без `.into_str()`: `#coerce` даёт то же самое", file=sys.stderr)
        print("  (D429 R9), и lint зовёт явный вызов лишним.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv вне тестов: {len(files)}, склеек строки с самой собой: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
