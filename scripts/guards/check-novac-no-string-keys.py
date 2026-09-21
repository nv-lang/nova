# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-string-keys.py — таблица не ключуется строкой
(архитектура §4а, К2 §16), и ключ двери не СИНТЕЗИРУЕТСЯ строкой.

ДВЕ ПОЛОВИНЫ ОДНОГО ПРАВИЛА:
  (а) `Map[str` вне `names/` — таблицы ключуются `DeclId`/`NodeId`, а не именем.
      Внутри `names/` строковый ключ законен, но обязан нести `NamespaceId`,
      одним из ДВУХ равноценных способов: композитным ключом
      (`Map[(NamespaceId, str), ...]` / `Map[NsKey, ...]`) ИЛИ полем-СОСЕДОМ
      в той же структуре (`type X { ns NamespaceId ... map Map[str, ...] }`,
      реестр #914) — ОДНА таблица на namespace держит инвариант конструкцией,
      composite-ключ ей не нужен так же, как индексу не нужен номер страницы,
      если он уже лежит в отдельном разделе на эту страницу.
  (б) СИНТЕЗ ключа интерполяцией: `@names.put("${owner}.${fd.name}", row)`.
      Первая половина на это молчала — дверь-то легальна, — а стоит такой ключ
      аллокации и форматирования на КАЖДЫЙ поиск (П14) и загоняет структуру
      обратно в текст сразу после правила «идентичность — не имя» (§4а).
      Законная форма: дверь берёт ИМЯ, строки с одинаковым именем связаны полем
      `next`, второй ключ сравнивается целым числом при обходе цепочки
      (образцы: `FnTable.row_of`, `FieldTable.field_type`). Голое имя-переменная
      первым аргументом законно — судится ровно интерполяция.

ПРАВКА #914 (2026-09-21): ДО этой правки страж читал ПРОЗУ КАК ДАННЫЕ — слово
`NamespaceId`, упомянутое в trailing `///`-doc-комментарии той же строки, что
`Map[str`, снимало предупреждение точно так же, как настоящий композитный
ключ. Носитель — `names/names.nv:80`: `map HashMap[str, int] /// the
NamespaceId key component is @ns, one field up` — комментарий ОБЪЯСНЯЕТ
инвариант, но сам код не проверяется вовсе. Это ложный ПРОПУСК, не ложный
отказ: страж молчал там, где по БУКВЕ правила (composite-ключ) обязан был
кричать. Прочитано ЖИВЬЁМ, не с чужих слов: `NameTable` — одна таблица на
`NamespaceId` (`NameTable.new(ns NamespaceId)`), инвариант «эта карта хранит
имена только одного namespace» держится тем, что вызывающий не может положить
имя в чужую таблицу — таблицы физически разные объекты. Composite-ключ был бы
избыточной перестраховкой поверх уже держащей структуры, а не более честной
формой. Значит чинить нужно ОБЕ половины СРАЗУ: (1) перестать читать
комментарий как код (иначе виден только СИМПТОМ, не причина); (2) признать
"ns соседним полем" законной формой (а) — иначе починка (1) сама покрасит
ГЕЙТ на корректном коде, которому уже сорок дней. Ни то ни другое по
отдельности не даёт верного ответа.

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
# A trailing doc/line comment, `///` or `//`, always opens with whitespace
# then two slashes in this codebase's style (`field Type /// text`, never
# `//` glued to code) -- stripping from the first such run keeps a genuine
# `http://`-shaped string (none exist in the matched constructs here) from
# being cut, while removing prose from the MATCH, not from the reported line.
RE_TRAILING_COMMENT = re.compile(r'\s//')
# A struct/record opener novac's own style always writes as `type NAME {`
# or `type NAME value {` on one line (see names.nv:78, sem.nv:116, ...);
# nested types are not a form this codebase's `names/` files use.
RE_STRUCT_OPEN = re.compile(r'^\s*(?:export\s+)?type\s+\w+(?:\s+value)?\s*\{')
# A field declaration whose TYPE is NamespaceId -- `ns NamespaceId`, not the
# enum's own declaration (`type NamespaceId enum`, excluded by RE_STRUCT_OPEN
# not matching "enum" as a struct opener) and not a mention in prose.
RE_FIELD_NS = re.compile(r'^\s*\w+\s+NamespaceId\b')


def strip_comment(line):
    m = RE_TRAILING_COMMENT.search(line)
    return line[:m.start()] if m else line


def file_declares_ns_sibling(lines):
    """Does this file declare at least one struct with a NamespaceId-typed
    field? Answered per FILE, not per struct block: the exemption a type's
    own field grants (registry #914 -- one table instance per namespace,
    `ns` beside `map` holds the invariant by construction) travels to that
    type's OWN constructor call sites too (`NameTable.new`'s record literal
    `{ ns, map: HashMap[str, int].new() }`, names.nv:85), which are not
    inside the `type X { ... }` block at all. A file in `names/` that
    genuinely has no such type declares nothing to exempt, so this stays
    narrow to the files the invariant actually holds for."""
    depth = 0
    start = None
    for raw in lines:
        line = raw[:-1] if raw.endswith("\r") else raw
        code = strip_comment(line)
        if start is None:
            if RE_STRUCT_OPEN.search(code):
                start = True
                depth = code.count("{") - code.count("}")
            continue
        if RE_FIELD_NS.search(code):
            return True
        depth += code.count("{") - code.count("}")
        if depth <= 0:
            start = None
    return False


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
        raw_lines = f.read_bytes().decode("utf-8", "replace").split("\n")
        has_ns_sibling = in_names and file_declares_ns_sibling(raw_lines)
        for idx, line in enumerate(raw_lines):
            n = idx + 1
            if line.endswith("\r"):
                line = line[:-1]
            # Комментарий, ЦИТИРУЮЩИЙ запрещённую форму, законен — та же правка,
            # что у check-novac-resolve-discipline и -no-grammar-excuse
            # (2026-08-17). Без неё страж красит гейт на объяснении ПРИЧИНЫ и
            # заставляет стереть объяснение, чтобы стать зелёным: страж, стирающий
            # причину вместе с симптомом. Поймано 2026-08-30 на фразе
            # `HashMap[str, int].new()` в комментарии о разводке скобочного прогона.
            if line.lstrip(" \t\v\f").startswith("//"):
                continue
            code = strip_comment(line)
            if "Map[str" in code:
                exempt = in_names and ("NamespaceId" in code or has_ns_sibling)
                if not exempt:
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
        print("  / Map[NsKey, ...], ИЛИ структура несёт поле NamespaceId рядом", file=sys.stderr)
        print("  (одна таблица на namespace, #914) — инвариант (а) К2.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, строковых ключей вне закона: 0, "
          f"синтезированных ключей: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
