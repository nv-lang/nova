# -*- coding: utf-8 -*-
"""scripts/guards/check-dead-anchors.py — ссылка на внутридокументный якорь,
которого НЕТ НИ В ОДНОМ файле дерева.

ДОМ ПРАВИЛА: реестр 221.1 №1182.

ЗАЧЕМ. Короткая ссылка `(#dNNN)` выглядит рабочей и не проверяется никем:
`check-no-escaping-links` судит ссылки НАРУЖУ, `check-commit-refs` — упоминания
коммитов, а внутридокументный якорь до 2026-09-20 был ничей. Замер того дня
(строка реестра 221.1 №1182, счёт повторён этим стражем): ссылок 453, явных
targetов 7, мёртвых 450 на 144 РАЗНЫХ адреса.

ПОЧЕМУ ЯКОРЬ НЕ РЕЗОЛВИТСЯ САМ. Автоматический slug делается из ВСЕГО текста
заголовка: у `## D481. Char-литерал: алфавит raw-char (2026-09-18)` он длинный и
с датой, а `(#d481)` ему не равен. Значит короткая ссылка живёт, только если у
заголовка стоит ЯВНЫЙ `{#d481}`. Отсюда правило приоритета: явный target старше
сгенерированного, и страж судит ИМЕННО его — иначе он объявил бы мёртвой рабочую
ссылку (ловушка, на которой интегратор уже побывал).

ЧТО СУДИТСЯ. Не наличие мёртвых ссылок, а их РОСТ: 450 штук — это прошлое, и
страж, краснеющий на прошлом, будет снят первым же окном. База держит ДВА
значения, и оба нужны:
  * `dead=<N>` — сколько мёртвых ССЫЛОК (вхождений);
  * список РАЗНЫХ мёртвых адресов — потому что счёт прячет подмену: одну ссылку
    починили, другую завели, число то же. Номер исчезнуть молча не может.

Храповик ВНИЗ: стало меньше — опусти базу ТОЙ ЖЕ правкой, страж об этом скажет.

ЗНАМЕНАТЕЛЬ ПЕЧАТАЕТСЯ ВСЕГДА: сколько файлов осмотрено, сколько ссылок, сколько
targetов найдено. Ноль нарушений при нуле осмотренного — не «чисто».

usage: python scripts/guards/check-dead-anchors.py [КОРЕНЬ] [БАЗА]
env: NOVA_DEAD_ANCHORS_BASELINE — путь к базе (шов самотеста).
"""
import io
import os
import re
import sys
from collections import Counter

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-dead-anchors"
LINK = re.compile(r"\(#(d[0-9]+)\)", re.IGNORECASE)
TARGET = re.compile(r"\{#(d[0-9]+)\}", re.IGNORECASE)
SKIP_DIRS = (".git", "node_modules", "target")


def scan(root):
    links, targets, files = Counter(), set(), 0
    for dirpath, dirs, names in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for n in names:
            if not n.endswith(".md"):
                continue
            p = os.path.join(dirpath, n)
            try:
                text = io.open(p, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            files += 1
            rel = os.path.relpath(p, root).replace(os.sep, "/")
            for m in LINK.finditer(text):
                links[(m.group(1).lower(), rel)] += 1
            for m in TARGET.finditer(text):
                targets.add(m.group(1).lower())
    return links, targets, files


def read_baseline(path):
    """(число, множество адресов). Значение — РОВНО одна строка `dead=`."""
    count, addrs = None, set()
    if not os.path.isfile(path):
        return None, addrs
    for line in io.open(path, encoding="utf-8", errors="replace"):
        s = line.split("#", 1)[0].strip()
        if not s:
            continue
        if s.startswith("dead=") and count is None:
            try:
                count = int(s.split("=", 1)[1])
            except ValueError:
                count = None
        elif re.match(r"^d[0-9]+$", s, re.IGNORECASE):
            addrs.add(s.lower())
    return count, addrs


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    base = (sys.argv[2] if len(sys.argv) > 2
            else os.environ.get("NOVA_DEAD_ANCHORS_BASELINE")
            or os.path.join(root, "scripts", "guards", "dead-anchors.baseline"))

    links, targets, files = scan(root)
    dead_pairs = {k: v for k, v in links.items() if k[0] not in targets}
    dead_total = sum(dead_pairs.values())
    dead_addrs = {k[0] for k in dead_pairs}

    print("%s: осмотрено .md файлов %d, ссылок (#dNNN) %d, явных targetов {#dNNN} %d, "
          "МЁРТВЫХ ссылок %d на %d разных адресов"
          % (NAME, files, sum(links.values()), len(targets), dead_total, len(dead_addrs)))

    if files == 0:
        print("%s: файлов не найдено — судить нечего (это не «чисто»)" % NAME)
        return 0

    base_count, base_addrs = read_baseline(base)
    if base_count is None:
        print("%s: FAIL — нет базы %s или в ней нет строки `dead=<N>`" % (NAME, base),
              file=sys.stderr)
        return 1

    new_addrs = sorted(dead_addrs - base_addrs)
    if new_addrs:
        print("%s: НОВЫЕ мёртвые адреса (%d): %s"
              % (NAME, len(new_addrs), ", ".join(new_addrs[:12])), file=sys.stderr)
        for a in new_addrs[:6]:
            where = sorted({f for (addr, f) in dead_pairs if addr == a})[:3]
            print("    %s — в %s" % (a, ", ".join(where)), file=sys.stderr)
        print("    Короткая ссылка `(#dNNN)` работает ТОЛЬКО если у заголовка стоит",
              file=sys.stderr)
        print("    явный `{#dNNN}`: автоматический slug делается из всего текста",
              file=sys.stderr)
        print("    заголовка и с номером не совпадает.", file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    if dead_total > base_count:
        print("%s: МЁРТВЫХ ССЫЛОК СТАЛО БОЛЬШЕ: %d > базы %d (адреса прежние — "
              "значит на старый мёртвый якорь сослались ещё раз)"
              % (NAME, dead_total, base_count), file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    if dead_total < base_count or (base_addrs - dead_addrs):
        print("%s: долг СНИЗИЛСЯ (ссылок %d < базы %d, адресов %d < %d) — опусти базу "
              "в %s ТОЙ ЖЕ правкой: база, оставленная высокой, разрешает завести "
              "столько же заново"
              % (NAME, dead_total, base_count, len(dead_addrs), len(base_addrs), base))

    print("%s ok: новых мёртвых якорей нет (мёртвых %d при базе %d)"
          % (NAME, dead_total, base_count))
    return 0


if __name__ == "__main__":
    sys.exit(main())
