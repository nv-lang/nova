#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Найти в спеке объявления методов протокола БЕЗ обязательного `@`.

НОРМА, которую инструмент применяет, — D209 (Plan 108.4, `spec/decisions/04-effects.md`):
у instance-метода протокола префикс `@` ОБЯЗАТЕЛЕН, голое имя ловится парсером как
`E_PROTO_METHOD_NEEDS_AT`; статический метод пишется ТОЧКОЙ; модификатор приёмника
(`mut`/`ro`/`consume`) стоит ПЕРЕД `@`. До 2026-09-16 спека во многих местах показывала
голую форму как законную — то есть учила писать то, что компилятор отвергает.

ПОЧЕМУ ЭТО ИНСТРУМЕНТ, А НЕ СТРАЖ. Часть попаданий ЗАКОННА, и отличить их может только
человек: в спеке есть места, где голое имя — САМ ПРЕДМЕТ разговора (вопрос о префиксе
цитирует обе формы), и переписать их значит сделать текст бессмысленным. Страж, который
покраснел бы на них, был бы отключён первым же окном. Поэтому здесь ПЕРЕЧЕНЬ для чтения
глазами; храповик на число — отдельная работа, и она названа в плане 274.7.

ПОЧЕМУ ПО БЛОКАМ, А НЕ ПО СПИСКУ ИМЁН — и это главное в этом файле. Первая редакция
искала знакомые имена (`hash`, `next`, `clone`, `equals`, ...) и нашла 24 места из 44:
`try_send`, `close`, `peek` не попали, потому что называются иначе. Мера, судящая СПИСОК
ИМЁН вместо СВОЙСТВА «строка стоит внутри `type X protocol { ... }`», — это записанный
класс реестра №1140, и здесь он повторён и пойман в тот же день.

ВТОРАЯ ЛОВУШКА, тоже оплаченная: счётчик фигурных скобок не выходил из блока, если в
примере скобки не сбалансированы (`User { ..u, id: v }` строкой ниже), и весь остаток
файла считался «внутри протокола». Блок обрывается на закрывающей ``` — границу фенсы
потерять нельзя.

Запуск:  python scripts/tools/spec-bare-protocol-methods.py [<корень репозитория>]
Выдача:  `путь:строка: текст` по одному попаданию, затем `TOTAL N`.
Код возврата 0 всегда: это перечисление, а не вердикт.
"""
import io
import os
import re
import sys

OPEN = re.compile(r"^\s*(export\s+)?type\s+[A-Za-z_][\w\[\], ]*\s+protocol\s*\{")
METHOD = re.compile(r"^(\s+)([A-Za-z_]\w*)\s*(\[|\()")
# Уже по норме, либо вовсе не объявление метода.
SKIP = re.compile(r"^\s*(//|#|\}|use\b|@|\.|mut\s+@|ro\s+@|consume\s+@|mut\s+\.|const\b)")


def scan_file(path):
    raw = io.open(path, encoding="utf-8", newline="").read()
    nl = "\r\n" if "\r\n" in raw else "\n"
    hits = []
    inside = False
    depth = 0
    for i, line in enumerate(raw.split(nl), 1):
        if line.lstrip().startswith("```"):
            # Фенса открылась или закрылась: блок её не переживает.
            inside = False
            depth = 0
            continue
        if not inside:
            if OPEN.match(line):
                inside = True
                depth = 1
            continue
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            inside = False
            continue
        if SKIP.match(line):
            continue
        if METHOD.match(line):
            hits.append((i, line.rstrip()))
    return hits


def main(argv):
    root = argv[1] if len(argv) > 1 else "."
    spec = os.path.join(root, "spec")
    if not os.path.isdir(spec):
        sys.stderr.write("spec-bare-protocol-methods: no spec/ under %r\n" % root)
        return 0
    total = 0
    out = []
    for dirpath, _dirs, files in os.walk(spec):
        for name in sorted(files):
            if not name.endswith(".md"):
                continue
            path = os.path.join(dirpath, name)
            for lineno, text in scan_file(path):
                rel = os.path.relpath(path, root).replace("\\", "/")
                out.append("%s:%d: %s" % (rel, lineno, text))
                total += 1
    # Печать через buffer: консоль здесь бывает cp1251 и падает на стрелке `->`.
    data = ("\n".join(out) + ("\n" if out else "") + "TOTAL %d\n" % total)
    sys.stdout.buffer.write(data.encode("utf-8", "replace"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
