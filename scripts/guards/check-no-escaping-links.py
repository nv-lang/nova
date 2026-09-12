#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-no-escaping-links.py — в отслеживаемых .md нет ссылок,
# уходящих ЗА КОРЕНЬ репозитория.
#
# ПРАВИЛО ВЛАДЕЛЬЦА 2026-09-12: «не должно быть ссылок за пределы репы на диск».
#
# ЗАЧЕМ, и причин две, обе измеренные:
#   1) РАСКРЫТИЕ. Ссылка вида `../../../<приватная-репа>/<каталог>/<статья>` сообщает
#      наружу не только что такая репа есть, но и как в ней устроены каталоги и о
#      чём статьи. Две ссылки того же дня вели в домашний каталог —
#      `../../../Users/<имя>/.claude/...`, имя владельца, которого не ловил даже
#      свежерасширенный check-no-machine-paths: там образец требует БУКВЫ ДИСКА, а
#      относительный путь её не несёт. Дыра нашлась ровно потому, что этот страж
#      судит РАЗРЕШЁННЫЙ путь, а не текст.
#   2) СЛОМАННОСТЬ. Ссылка за корень не открывается ни у кого, кто клонировал
#      репозиторий: соседнего каталога у него нет. Из восьми найденных одна была
#      просто опечаткой: из `docs/dev/` до корня две ступени вверх, а стояло три.
#
# ПОЧЕМУ ОТДЕЛЬНЫЙ СТРАЖ, а не ярус в check-no-machine-paths: там образец ищется в
# ТЕКСТЕ, здесь путь РАЗРЕШАЕТСЯ относительно файла. `../../../x` из `docs/dev/`
# уходит за корень, а из `docs/plans/wip/x/` — нет; текстовый образец этой разницы
# не видит и либо пропустит половину, либо покраснеет на здоровом.
#
# НОЛЬ БЕЗ БАЗЫ: на день заведения таких ссылок 0 (было 8, вычищены тем же
# слиянием). База нужна там, где долг велик и снимается волнами; здесь долга нет.
#
# Самотест: selftest/test-check-no-escaping-links.sh
import os
import re
import subprocess
import sys

# `newline="\n"` обязателен: без него вердикт уезжает CRLF на Windows, и его
# читатели (гейт, CI) видят не ту строку, что страж печатал. Поймано
# check-guard-honesty при первом же прогоне гейта.
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-no-escaping-links"
LINK = re.compile(r"\]\(([^)\s]+)\)")
SKIP_PREFIX = ("nova_tests.old/",)


def main() -> int:
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(os.path.dirname(__file__), "..", ".."))
    try:
        listing = subprocess.run(["git", "-C", root, "ls-files", "*.md"],
                                 capture_output=True, text=True,
                                 encoding="utf-8", errors="replace", timeout=120)
    except (OSError, subprocess.SubprocessError) as exc:
        print("%s: FAIL — git не отдал списка файлов: %s" % (NAME, exc), file=sys.stderr)
        return 1
    files = [f.strip() for f in listing.stdout.split("\n") if f.strip()]
    if not files:
        print("%s: FAIL — git не отдал ни одного .md под %s" % (NAME, root), file=sys.stderr)
        return 1

    bad = []
    scanned = 0
    for rel in files:
        if rel.startswith(SKIP_PREFIX):
            continue
        path = os.path.join(root, rel)
        if not os.path.isfile(path):
            continue
        try:
            text = open(path, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        scanned += 1
        here = os.path.dirname(path)
        for m in LINK.finditer(text):
            target = m.group(1)
            if target.startswith(("http://", "https://", "#", "mailto:", "data:")):
                continue
            target = target.split("#", 1)[0]
            if not target or ".." not in target:
                continue
            full = os.path.normpath(os.path.join(here, target))
            inside = (full.lower() == root.lower()
                      or full.lower().startswith(root.lower() + os.sep))
            if not inside:
                line = text[:m.start()].count("\n") + 1
                bad.append("  %s:%d  -> %s" % (rel, line, target))

    if bad:
        print("%s: FAIL — ссылка уходит за корень репозитория (%d):"
              % (NAME, len(bad)), file=sys.stderr)
        for b in bad[:25]:
            print(b, file=sys.stderr)
        if len(bad) > 25:
            print("  … и ещё %d" % (len(bad) - 25), file=sys.stderr)
        print("    Такая ссылка не открывается у того, кто клонировал репозиторий,",
              file=sys.stderr)
        print("    и сообщает наружу раскладку соседних каталогов. Ссылайся внутрь",
              file=sys.stderr)
        print("    репозитория либо называй источник словами, без пути.", file=sys.stderr)
        return 1

    print("%s ok: .md проверено %d, ссылок за корень 0" % (NAME, scanned))
    return 0


if __name__ == "__main__":
    sys.exit(main())
