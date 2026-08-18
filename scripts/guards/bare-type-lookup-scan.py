# -*- coding: utf-8 -*-
"""Ядро check-bare-type-lookups: чтения карты типов чекера ГОЛЫМ именем.

ЗАЧЕМ. `TypeCheckCtx.types` — `HashMap<String, TypeDecl>`, ключуемая ПРОСТЫМ
именем, last-write-wins между модулями слитого CU. Это источник истины для
всего, что строится поверх, и он разрешает имя неверно всякий раз, когда два
модуля объявили одноимённый тип.

КЛАСС ВОЗВРАЩАЛСЯ ЧЕТЫРЕЖДЫ, каждый раз другой гранью: 196.7 (диспетч метода
по last-wins), №696 (таблицы кодогена), №705 (таблица типов чекера) и — в тот
же день — №729, чья починка разбилась о два ложных отказа на `Kind.Info(5)` и
`Node.Leaf(7)`.

ЧТО СЧИТАЕТСЯ. Голые чтения: `self.types.get(...)`, `.contains_key(...)` и
обходы карты. НЕ считается `types_get_for_file(...)` — разрешение по file_id
места ИСПОЛЬЗОВАНИЯ, то самое, к которому класс и должен прийти.

ХРАПОВИК, А НЕ НОЛЬ. Разом их не снять: окно W6 (план 196) переводит
потребителей на разрешение по файлу и снимает обходы. До тех пор число обязано
только УБЫВАТЬ — новый голый читатель заводит новую грань класса, который
четырежды возвращался.

Поверхность коллизий измерена (`NOVA_TYPE_COLLISION_REPORT`, 2026-08-18): на
conformance 1073 CU, коллизии в 39, различных сталкивающихся имён шесть.
"""
import io
import os
import re
import sys

TARGET = os.path.join("compiler-codegen", "src", "types", "mod.rs")

BARE = (
    re.compile(r"self\.types\.get\("),
    re.compile(r"self\.types\.contains_key\("),
    re.compile(r"\.types\.(?:iter|keys|values)\(\)"),
)
ROUTED = re.compile(r"types_get_for_file\(")


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    except Exception:
        pass

    root = sys.argv[1] if len(sys.argv) > 1 else "."
    path = os.path.join(root, TARGET)
    w = sys.stdout.write

    if not os.path.isfile(path):
        w("MISSING %s\n" % TARGET.replace(os.sep, "/"))
        w("bare=-1\nrouted=-1\n")
        return 1

    bare = 0
    routed = 0
    for i, line in enumerate(io.open(path, encoding="utf-8", errors="replace"), 1):
        # КОММЕНТАРИЙ — НЕ ЧТЕНИЕ. В файле есть строки, объясняющие класс и
        # цитирующие `self.types.get("Color")`; счётчик, принимающий цитату за
        # код, врал бы в ту же сторону, против которой заведён.
        if line.lstrip().startswith("//"):
            continue
        # Строка, уже разрешающая по файлу, голой не считается, даже если
        # рядом в ней стоит откат на глобальную карту (сам аксессор).
        if ROUTED.search(line):
            routed += 1
            continue
        for pat in BARE:
            n = len(pat.findall(line))
            if n:
                bare += n
                w("%s:%d  %s\n" % (TARGET.replace(os.sep, "/"), i,
                                   line.strip()[:110]))
    w("bare=%d\n" % bare)
    w("routed=%d\n" % routed)
    return 0


if __name__ == "__main__":
    sys.exit(main())
