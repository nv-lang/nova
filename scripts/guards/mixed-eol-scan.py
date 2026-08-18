# -*- coding: utf-8 -*-
"""Ядро check-mixed-eol: файлы со СМЕШАННЫМИ окончаниями строк в РАБОЧЕМ ДЕРЕВЕ.

ЗАЧЕМ. Смешанные окончания ломают всё построчное: diff показывает правку там,
где её нет; счётчики строк расходятся между машинами; инструмент, считающий
позицию по строкам, и инструмент, считающий по байтам, дают разные ответы для
одного места. В редакторе файл при этом выглядит нормально.

ЧЕСТНО ПРО ПОВОД. Страж заведён 2026-08-18 после того, как inlay-подсказки LSP
поехали в смешанном `lex.nv`, и корреляция выглядела причиной. Синтетическая
проверка её ОПРОВЕРГЛА: тот же исходник в трёх видах (LF / CRLF / смешанно)
даёт одинаковые подсказки. Причина №709 НЕ найдена; этот страж закрывает свой
класс, а не тот.

ПОЧЕМУ НЕ ЛОВИТСЯ GIT'ОМ. `core.autocrlf=true`: в объекте файл однороден,
смешанным становится ПОСЛЕ выкладки, `git diff` пуст. Лечение — перевыкладка,
а не коммит. Проверяется РАБОЧЕЕ ДЕРЕВО.
"""
import io
import os
import sys

EXTS = (".nv", ".rs", ".md", ".sh", ".py", ".toml", ".c", ".h", ".txt",
        ".json", ".yml", ".yaml", ".baseline", ".list")
SKIP_DIRS = ("target", ".git", "node_modules", "vcpkg_installed", ".claude",
             "nova_tests.old", "out", "dist")
# Файлы, которые ХРАНЯТ образцы смешанных окончаний как данные.
SKIP_FILES = ("mixed-eol-scan.py", "check-mixed-eol.sh",
              "test-check-mixed-eol.sh")


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    except Exception:
        pass

    root = sys.argv[1] if len(sys.argv) > 1 else "."
    w = sys.stdout.write
    total = 0

    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fn in sorted(filenames):
            if not fn.endswith(EXTS) or fn in SKIP_FILES:
                continue
            p = os.path.join(dirpath, fn)
            try:
                raw = io.open(p, "rb").read()
            except Exception:
                continue
            crlf = raw.count(b"\r\n")
            lf = raw.count(b"\n") - crlf
            if crlf and lf:
                total += 1
                rel = os.path.relpath(p, root).replace(os.sep, u"/")
                # Первая аномальная строка: с неё и начинается расхождение.
                first = 0
                line = 1
                i = 0
                while i < len(raw):
                    if raw[i:i + 1] == b"\n":
                        if i == 0 or raw[i - 1:i] != b"\r":
                            first = line
                            break
                        line += 1
                    i += 1
                w(u"%s  CRLF=%d  bare-LF=%d  first-anomaly-line=%d\n"
                  % (rel, crlf, lf, first))
    w(u"total=%d\n" % total)
    return 0


if __name__ == "__main__":
    sys.exit(main())
