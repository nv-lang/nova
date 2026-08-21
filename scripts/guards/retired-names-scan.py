# -*- coding: utf-8 -*-
"""Ядро check-retired-names: ищет СНЯТЫЕ имена в живых зонах.

ЗАЧЕМ. Переименование считается сделанным не тогда, когда исправлен носитель,
а когда греп по старой форме даёт ноль. 2026-08-17 это доказано дважды за один
день и одним и тем же человеком: сначала спека фиксировала переименование
`NOVA_NO_AUTOARM` → `NOVA_AUTOARM` в ОДНОЙ строке, пока три соседние
пользовались снятым именем как действующим; через час интегратор переименовал
`SignedInt` → `SignedInts` в одном файле и остановился, а форма жила ещё в
семи, включая три файла `std`. Оба раза находил человек, а не машина.

ПОЧЕМУ ЯДРО НА ПИТОНЕ, А НЕ GREP. Нужен отрицательный просмотр вперёд:
`SignedInt` обязан ловиться, а `SignedInts` — нет, и разница между ними ровно
в одном символе ПОСЛЕ совпадения. Плюс список пар живёт в отдельном файле —
grep пришлось бы собирать в цикле и терять номера строк.

ЧЕГО НЕ СЧИТАЕМ:
  * `docs/plans/**` — исторические записи; править в них имена значит
    подделывать историю (то же исключение, что у остальных стражей формы);
  * строки, которые САМИ помечают имя снятым (амендмент, «здесь стояло»,
    renamed/legacy/retired) — иначе страж заставит удалять объяснение, ради
    которого амендмент и пишется. Тот же случай, что SKIP_FILES у
    mojibake-scan («страж нашёл сам себя»).
"""
import io
import os
import re
import sys

EXC = re.compile(
    u"(АМЕНДМЕНТ|Амендмент|амендмент|amend|renamed|переименован|legacy|Legacy"
    u"|LEGACY|retired|retract|СНЯТ|снят|здесь стояло|было:|прежн|устарев"
    u"|deprecat|OLD-NAME|old name)",
    re.IGNORECASE,
)

# Живые зоны: здесь снятого имени быть не должно вовсе.
LIVE_ZONES = [u"spec", u"std", u"docs/guide", u"docs/dev",
              u"examples", u"compiler-codegen", u"nova-cli/src", u"scripts"]
EXTS = (".md", ".nv", ".rs", ".h", ".c", ".sh", ".toml", ".py")
SKIP_DIRS = ("target", ".git", "node_modules", "vcpkg_installed", ".claude",
             "nova_tests.old")
# Сам список пар и это ядро хранят снятые имена как ДАННЫЕ.
SKIP_FILES = ("retired-names.list", "retired-names-scan.py",
              "check-retired-names.sh", "test-check-retired-names.sh",
              # Хронологический журнал упрощений: записи «план N ввёл
              # env var X» описывают ПРОШЛОЕ, и переименовывать имена в
              # них значит подделывать историю. Тот же довод, что у
              # docs/plans/**, только файл лежит в живой зоне.
              "simplifications.md")


def load_pairs(path):
    """Строки вида `old -> new  # причина`. Пустые и `#` игнорируются."""
    pairs = []
    for raw in io.open(path, encoding="utf-8", errors="replace"):
        line = raw.strip()
        if not line or line.startswith(u"#"):
            continue
        body = line.split(u"#", 1)[0].strip()
        if u"->" not in body:
            continue
        old, new = [s.strip() for s in body.split(u"->", 1)]
        if old and new:
            pairs.append((old, new))
    return pairs


def main():
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    except Exception:
        pass

    root = sys.argv[1] if len(sys.argv) > 1 else "."
    list_path = (sys.argv[2] if len(sys.argv) > 2
                 else os.path.join(root, "scripts", "guards", "retired-names.list"))
    if not os.path.exists(list_path):
        sys.stderr.write("retired-names-scan: no %s\n" % list_path)
        return 1

    pairs = load_pairs(list_path)
    if not pairs:
        sys.stderr.write("retired-names-scan: the pair list is empty\n")
        return 1

    # `\bOLD\b` мало: `SignedInts` содержит `SignedInt` и не должен ловиться.
    rxs = [(old, new,
            re.compile(u"(?<![A-Za-z0-9_])" + re.escape(old) + u"(?![A-Za-z0-9_])"))
           for old, new in pairs]

    # БЫСТРЫЙ ОТСЕВ (план 275-Ф.1, гейт-стоимость): раньше на КАЖДУЮ строку
    # каждого файла живых зон гонялись все 14 регексов по очереди — при том,
    # что подавляющее большинство строк не содержит НИ ОДНОГО снятого имени.
    # Одна альтернация `(?<!\w)(old1|old2|...)(?!\w)` даёт тот же ответ на
    # вопрос «есть ли тут вообще снятое имя» за один проход. Полный перебор
    # `rxs` по-прежнему нужен ТОЛЬКО когда альтернация нашла совпадение — он
    # определяет, КАКАЯ именно пара сработала, ровно в том порядке списка,
    # что и раньше (первая пара по списку, а не первая по позиции в строке:
    # альтернация с `.search()` даёт ЛЕВОЕЙШЕЕ совпадение, что для строки с
    # ДВУМЯ разными снятыми именами могло бы выбрать другую пару — этого не
    # допускаем, вердикт обязан остаться тем же байт-в-байт).
    combined = re.compile(u"(?<![A-Za-z0-9_])(?:"
                           + u"|".join(re.escape(old) for old, _ in pairs)
                           + u")(?![A-Za-z0-9_])")

    w = sys.stdout.write
    total = 0
    for zone in LIVE_ZONES:
        base = os.path.join(root, *zone.split(u"/"))
        if not os.path.isdir(base):
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
            for fn in sorted(filenames):
                if not fn.endswith(EXTS) or fn in SKIP_FILES:
                    continue
                p = os.path.join(dirpath, fn)
                try:
                    lines = io.open(p, encoding="utf-8",
                                    errors="replace").read().split(u"\n")
                except Exception:
                    continue
                rel = os.path.relpath(p, root).replace(os.sep, u"/")
                # Исключение БЛОЧНОЕ: амендмент пишется цитатным блоком,
                # где слово-признак стоит в ПЕРВОЙ строке, а снятое имя
                # встречается в середине. Построчная проверка требовала бы
                # пометить каждую строку — то есть засорить текст ради
                # машины. Открыт объясняющей строкой — объясняет весь блок.
                in_exempt_quote = False
                for i, line in enumerate(lines, 1):
                    stripped = line.lstrip()
                    if stripped.startswith(u">"):
                        if not in_exempt_quote and EXC.search(line):
                            in_exempt_quote = True
                    else:
                        in_exempt_quote = False
                    if in_exempt_quote or EXC.search(line):
                        continue
                    if not combined.search(line):
                        continue
                    for old, new, rx in rxs:
                        if rx.search(line):
                            total += 1
                            w(u"%s:%d  %s -> %s  |  %s\n"
                              % (rel, i, old, new, line.strip()[:110]))
                            break
    w(u"total=%d\n" % total)
    return 0


if __name__ == "__main__":
    sys.exit(main())
