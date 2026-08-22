# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-string-keys.py — таблица не ключуется строкой
(архитектура §4а, К2 §16), и ключ двери не СИНТЕЗИРУЕТСЯ строкой.

ДВЕ ПОЛОВИНЫ ОДНОГО ПРАВИЛА:
  (а) `Map[str` вне `names/` — таблицы ключуются `DeclId`/`NodeId`, а не именем.
      Внутри `names/` строковый ключ законен, но обязан нести `NamespaceId`:
      `Map[(NamespaceId, str), ...]` или `Map[NsKey, ...]`;
  (б) СИНТЕЗ ключа интерполяцией: `@names.put("${owner}.${fd.name}", row)`.
      Первая половина на это молчала — дверь-то легальна, — а стоит такой ключ
      аллокации и форматирования на КАЖДЫЙ поиск (П14) и загоняет структуру
      обратно в текст сразу после правила «идентичность — не имя» (§4а).
      Законная форма: дверь берёт ИМЯ, строки с одинаковым именем связаны полем
      `next`, второй ключ сравнивается целым числом при обходе цепочки
      (образцы: `FnTable.row_of`, `FieldTable.field_type`). Голое имя-переменная
      первым аргументом законно — судится ровно интерполяция.

ПОЧЕМУ PYTHON: старт процесса дороже самой проверки (П14).

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-string-keys"
RE_SYNTH = re.compile(r'\.(put|find)\("[^"]*\$\{')


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src}, файлов .nv: 0)")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME} ok: судить нечего (в {src} файлов .nv: 0)")
        return 0

    bad, synth = [], []
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        in_names = rel.startswith("names/") or "/names/" in rel
        for n, line in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if line.endswith("\r"):
                line = line[:-1]
            if "Map[str" in line and not (in_names and "NamespaceId" in line):
                bad.append(f"  {rel}:{n}:{line}")
            if RE_SYNTH.search(line):
                synth.append(f"  {rel}:{n}:{line}")

    if synth:
        print(f"{NAME}: FAIL — ключ двери СИНТЕЗИРОВАН строкой (архитектура §4а, П17):",
              file=sys.stderr)
        for s in synth:
            print(s, file=sys.stderr)
        print("  Составной ключ из интерполяции стоит аллокации на каждый поиск (П14)", file=sys.stderr)
        print("  и прячет структуру в текст. Законная форма: дверь берёт ИМЯ, а строки", file=sys.stderr)
        print("  с одинаковым именем связаны полем next; второй ключ сравнивается целым", file=sys.stderr)
        print("  числом при обходе цепочки (образцы: FnTable.row_of, FieldTable.field_type).", file=sys.stderr)
        return 1

    if bad:
        print(f"{NAME}: FAIL — строковый ключ таблицы (архитектура §4а, К2 §16):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Вне names/ таблицы ключуются DeclId/NodeId, не именем (инвариант (б)).", file=sys.stderr)
        print("  Внутри names/ ключ несёт NamespaceId: Map[(NamespaceId, str), ...]", file=sys.stderr)
        print("  или Map[NsKey, ...] (инвариант (а) К2).", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, строковых ключей вне закона: 0, "
          f"синтезированных ключей: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
