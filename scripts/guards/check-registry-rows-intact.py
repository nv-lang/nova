#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""scripts/guards/check-registry-rows-intact.py — реестр 221.1 не теряет СТРОК
при слиянии, и не заводит двух строк с одним номером.

ДОМ И ОСНОВАНИЕ: docs/plans/221.1-bug-sweep.md — раздел «Как сливать ЭТОТ файл
при конфликте: объединением по номерам, а не выбором стороны» (процедура,
принятая двумя интеграторами 2026-09-04), и запись реестра №642, где про этот
самый класс сказано прямо: «**ЧЕГО НЕ СДЕЛАНО:** механизма против повторения
нет». Этот файл и есть недостающий механизм — гейтовая сторона той процедуры.
Самотест: selftest/test-check-registry-rows-intact.sh.

ЗАЧЕМ. Строка записи в реестре — от одного до семидесяти килобайт В ОДНУ
ЛИНИЮ. Потерянную при слиянии строку не видно ни глазами, ни диффом: `git`
разводит соседние строки чисто и молча, а «выбор своей стороны» при конфликте
уносит чужую строку целиком либо оставляет две строки с одним номером. Форму
записи проверяют три стража (`check-registry-entry-shape`,
`check-registry-single-verdict`, `check-registry-routes`), но все они судят
СТРОКИ, КОТОРЫЕ ЕСТЬ. Исчезнувшую строку не судит никто: у неё нет формы, чтобы
покраснеть. Число строк до 2026-09-04 не считал никто.

ЧТО УЖЕ ЕСТЬ И ГДЕ НЕ ДОСТАЁТ (сказано честно, чтобы страж не выглядел
дублем). `scripts/tools/registry-add.sh` ловит ровно этот класс на КОММИТЕ:
сверяет добавленные строки со списком номеров, которые ты назвал, и всегда
проверяет дубли. Он не достаёт в двух местах: (1) разрешение КОНФЛИКТА — это не
`add` с названными номерами, и через инструмент оно не проходит; (2) любой
коммит, сделанный мимо инструмента. Гейт же исполняется всегда, и потому число
строк обязано проверяться здесь.

ЧТО СЧИТАЕТ. Строки реестра вида `| <число> |` — определение НЕ придумано
заново: оно уже канон, записанный в `registry-entry-shape.baseline` 2026-08-10
(«Теперь строка реестра — это `| ЧИСЛО |`, и только она»), когда вспомогательные
таблицы получили префикс `Q`, чтобы не путаться с реестром. Из строк собирается
МНОЖЕСТВО номеров: сколько их, каков наибольший, нет ли двух одинаковых, нет ли
дыры ниже максимума.

ЧТО КРАСНИТ:
  1. записей МЕНЬШЕ, чем в базе, — строка потеряна (главный случай, ради
     которого страж заведён);
  2. ДУБЛЬ номера — две строки с одним числом; слияние выбором стороны даёт
     именно это, и всякий поиск по номеру молча находит ПЕРВУЮ (№642);
  3. ДЫРА в нумерации ниже максимума, которой нет в списке законных дыр базы, —
     номер пропал или не был выдан;
  4. НОЛЬ строк — потеря мишени: файл переименован, таблица переехала, форма
     строки изменилась. Зелёный ноль здесь был бы худшим из исходов.

РОСТ ТОЖЕ КРАСНЫЙ, И ЭТО НЕ ПРИДИРКА. `rows`/`max` в базе могут только расти,
но растит их ТА ЖЕ правка, что добавила запись, — иначе храповик отстаёт, а
храповик, отстающий на N, молча разрешает потерять N строк, то есть перестаёт
делать единственное, ради чего заведён. Печатать «база отстала» и оставаться
зелёным нельзя: ровно так протух `registry-entry-shape` — его собственная
летопись говорит «страж сам просил об этом строкой в каждом прогоне; ровно так
храповик и протухает — сообщение печатается, читать его некому». Отказ называет
готовые числа, правка базы стоит двух строк.

ЧЕГО НЕ ЛОВИТ (сказано вслух, а не умолчано): подмену ТЕЛА записи. Строка на
месте, номер на месте, а текст чужой либо усечённый — это не отличимо
механически от законной правки записи. Здесь считаются номера и их число.

Аргументы: $1 — корень репозитория (по умолчанию — репозиторий стража);
$2 — override мишени: путь к файлу реестра либо каталог, в котором его искать
(шов самотеста). Переменная NOVA_REGISTRY_ROWS_BASELINE — override базы
`scripts/guards/registry-rows.baseline` (второй шов самотеста).

Вход для гейта — `main()`: run-guards.py исполняет стражей в одном процессе и
зовёт именно её; страж с телом на уровне модуля зелен вручную и красен в гейте.
"""
import io
import os
import re
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-registry-rows-intact"
REG_NAME = "221.1-bug-sweep.md"

# Строка реестра: `| <число> |` в самом начале строки. Совпадает с определением
# awk-редакции check-registry-entry-shape.sh (`^\| [0-9]+ \|`), но терпит лишние
# пробелы: расхождение двух определений одной сущности само по себе дефект.
ROW = re.compile(r"^\|\s*([0-9]+)\s*\|")


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def shown(path, root):
    """Путь так, как читатель будет его искать: относительно корня, если файл
    под ним, и как есть — если самотест увёл мишень на другой диск (relpath на
    Windows отказывается пересекать тома)."""
    try:
        return os.path.relpath(path, root).replace("\\", "/")
    except ValueError:
        return path.replace("\\", "/")


def resolve_target(root, override):
    if override:
        p = os.path.abspath(override)
        if os.path.isdir(p):
            for cand in (os.path.join(p, REG_NAME),
                         os.path.join(p, "docs", "plans", REG_NAME)):
                if os.path.isfile(cand):
                    return cand
            return os.path.join(p, REG_NAME)
        return p
    return os.path.join(root, "docs", "plans", REG_NAME)


def parse_gaps(text):
    """`gaps=174-208,210-215,653` -> множество номеров. Диапазон записывается
    через дефис: сорок один номер подряд одной строкой нечитаем, а нечитаемую
    базу не проверяют глазами."""
    m = re.search(r"^gaps=(.*)$", text, re.M)
    if m is None:
        return None, "нет строки gaps= (пусть пустой) — список законных дыр не задан"
    out = set()
    body = m.group(1).strip()
    if not body:
        return out, None
    for piece in body.split(","):
        piece = piece.strip()
        if not piece:
            continue
        rng = re.match(r"^([0-9]+)-([0-9]+)$", piece)
        if rng:
            lo, hi = int(rng.group(1)), int(rng.group(2))
            if hi < lo:
                return None, "в gaps= диапазон %s идёт вспять" % piece
            out.update(range(lo, hi + 1))
        elif re.match(r"^[0-9]+$", piece):
            out.add(int(piece))
        else:
            return None, "в gaps= кусок %r не число и не диапазон вида N-M" % piece
    return out, None


def parse_key(text, key, base_file):
    m = re.search(r"^%s=([0-9]+)\s*$" % key, text, re.M)
    if m is None:
        return None, "в базе %s нет строки %s=N — храповик судить нечем" % (base_file, key)
    return int(m.group(1)), None


def main():
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    target = resolve_target(root, sys.argv[2] if len(sys.argv) > 2 else None)
    base_file = os.environ.get(
        "NOVA_REGISTRY_ROWS_BASELINE",
        os.path.join(os.path.dirname(os.path.abspath(__file__)), "registry-rows.baseline"))

    rel = shown(target, root)

    if not os.path.isfile(target):
        return fail("нет файла реестра %s — мишень потеряна, а не «записей 0». "
                    "Реестр переименован или переехал: страж обязан переехать вместе с ним." % rel)

    lines_of = {}
    order = []
    with io.open(target, encoding="utf-8", errors="replace") as f:
        for n, line in enumerate(f, 1):
            m = ROW.match(line)
            if m:
                num = int(m.group(1))
                lines_of.setdefault(num, []).append(n)
                order.append(num)

    if not order:
        return fail("в %s нет НИ ОДНОЙ строки вида `| <число> |` — мишень потеряна. "
                    "Либо таблица реестра переехала, либо изменилась форма строки; "
                    "зелёный ноль здесь означал бы, что страж считает несуществующее." % rel)

    try:
        base_t = io.open(base_file, encoding="utf-8", errors="replace").read()
    except IOError:
        return fail("нет базы %s (ключи rows=N, max=N, gaps=...) — храповик судить нечем" % base_file)

    base_rows, err = parse_key(base_t, "rows", base_file)
    if err:
        return fail(err)
    base_max, err = parse_key(base_t, "max", base_file)
    if err:
        return fail(err)
    base_gaps, err = parse_gaps(base_t)
    if err:
        return fail("база %s: %s" % (base_file, err))

    now_rows = len(order)
    now_max = max(lines_of)
    problems = []

    # 1. Дубль номера.
    dups = sorted(n for n in lines_of if len(lines_of[n]) > 1)
    for n in dups:
        where = ", ".join("%s:%d" % (rel, ln) for ln in lines_of[n])
        problems.append("ДУБЛЬ №%d — строк с этим номером %d: %s. Слияние выбором стороны даёт "
                        "именно это; всякий поиск по номеру найдёт ПЕРВУЮ, молча (№642)."
                        % (n, len(lines_of[n]), where))

    # 2. Дыра ниже максимума, не названная в базе.
    holes = [i for i in range(1, now_max + 1) if i not in lines_of and i not in base_gaps]
    if holes:
        head = ", ".join("№%d" % h for h in holes[:20])
        tail = " (и ещё %d)" % (len(holes) - 20) if len(holes) > 20 else ""
        problems.append("ДЫРА в нумерации %s: %s%s — строки с этим номером нет ни одной, и в "
                        "базе %s он не назван законной дырой. Либо строка потеряна слиянием, "
                        "либо номер снят и это надо записать в gaps= вместе с причиной."
                        % (rel, head, tail, base_file))

    # 3. Храповик вниз — главный случай.
    if now_rows < base_rows:
        problems.append("записей в %s: %d, база %d — ПОТЕРЯНО %d. Реестр правят из разных сессий, "
                        "и строка тут в килобайт длиной: пропажу не видно ни глазами, ни диффом. "
                        "Восстанови строки из своей стороны слияния (процедура — в шапке реестра, "
                        "раздел про слияние объединением по номерам), а не опускай базу."
                        % (rel, now_rows, base_rows, base_rows - now_rows))
    if now_max < base_max:
        problems.append("наибольший номер в %s: %d, база %d — верхняя запись потеряна "
                        "(или номер переписан вниз)." % (rel, now_max, base_max))

    # 4. База отстала. Красное — см. шапку: отставший храповик молча разрешает потерю.
    if not problems and (now_rows > base_rows or now_max > base_max):
        problems.append("реестр вырос: записей %d (база %d), наибольший номер %d (база %d). "
                        "Рост законен, но базу поднимает ТА ЖЕ правка — иначе храповик отстаёт "
                        "и молча разрешает потерять столько же строк. Впиши в %s:\n"
                        "        rows=%d\n        max=%d\n"
                        "    и назови в летописи базы, какие номера добавлены."
                        % (now_rows, base_rows, now_max, base_max, base_file, now_rows, now_max))

    if problems:
        sys.stderr.write("%s: FAIL — реестр 221.1 потерял строки либо задвоил номера:\n" % NAME)
        for p in problems:
            sys.stderr.write("    %s\n" % p)
        return 1

    print("%s ok: записей %d (база %d), наибольший номер %d (база %d), дублей 0, "
          "законных дыр %d — ни одна строка не потеряна"
          % (NAME, now_rows, base_rows, now_max, base_max, len(base_gaps)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
