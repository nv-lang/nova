#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-selftest-honest-count.py — самотест не врёт о СВОЁМ покрытии.
# План: docs/plans/274-novac-self-hosted-compiler.md §10.3а (реестр стражей);
# конвенция gate-guard-conventions.md Г10 (случай кодирует замер). Самотест:
# selftest/test-check-selftest-honest-count.sh.
"""Итоговая строка самотеста называет число случаев ЛИТЕРАЛОМ — и оно расходится
с действительностью на первом же добавленном случае, молча и навсегда.

ЗАЧЕМ. Самотест кончается строкой вида `test-X ok: 8/8`. Число написано рукой:
никто его не считает, ничто не сверяет. Добавили случай — стало девять, а строка
по-прежнему говорит восемь; убрали — тем более. Строка выглядит замером покрытия
и им не является — тот же класс, что «зелёный ноль при потерянной мишени», только
про сам самотест. Найдено интегратором у себя (`7/7` при девяти свойствах).

ЗАМЕР 2026-09-04 ПОСЛЕ ПЕРЕВОДА ДВЕНАДЦАТИ САМОТЕСТОВ НА СЧЁТЧИК: разошлись
четыре из двенадцати — `arch-class-proofs` 12 против 13, `arch-invariants` 3
против 4, `deps` 9 против 10 и `guard-registry` «8 случаев» против 18. Худший —
`frontend-shape`: рука писала 8, счётчик показал 6, то есть самотест ЗАЯВЛЯЛ
покрытия БОЛЬШЕ, чем имел. Это и есть направление, ради которого страж заведён:
недосчёт заметен при добавлении случая, а перебор не заметен никогда.
(Первая редакция этой шапки называла носителем `file-size` — «8/8 при семи
случаях»; счётчик показал ровно восемь. Ошибка была в грепе, которым я считал
случаи; исправлено тем же слиянием, чтобы страж не нёс в себе неверный замер.)

ЧТО СЧИТАЕТ: файлы `scripts/guards/selftest/test-*.sh` и `scripts/selftest/test-*.sh`.
В каждом ищет строки, печатающие итог (`echo "... ok: ..."`), и среди них — те, где
число случаев записано ЛИТЕРАЛОМ: `N/M`, `N случаев`, `N properties`, `N cases`,
и в этой же строке НЕТ подстановки переменной (`$`). Строка со счётчиком
(`$cases/$cases`, `$n случаев`) законна: её печатает то, что считало.

ЧТО КРАСНИТ: рост числа таких самотестов над базой
`scripts/guards/selftest-honest-count.baseline` (ключ `literal=N`). Храповик ВНИЗ:
каждый переведённый на счётчик самотест опускает базу тем же слиянием.

МИШЕНЬ НЕ ПОТЕРЯНА: ноль найденных самотестов — КРАСНОЕ, а не «нарушений 0».

Аргументы: $1 — корень репозитория; $2 — override каталога самотестов (шов
самотеста); env NOVAC_SELFTEST_COUNT_BASELINE — override базы.
Вход для гейта — main(): run-guards.py исполняет стражей в одном процессе и зовёт
именно её.
"""
import io
import os
import re
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")

NAME = "check-selftest-honest-count"

# Итоговая строка: строка, которая САМА печатает вердикт, то есть НАЧИНАЕТСЯ с
# `echo`/`printf`. Не подходит строка, где `echo` стоит АРГУМЕНТОМ: самотест
# этого стража строит фикстуры вызовами вида `mk "$T/d" "test-b.sh" 'echo
# "test-b ok: 8/8"'`, и первая редакция считала такую фикстуру нарушением —
# страж краснел на собственном самотесте, то есть на честной работе (тот же
# класс, что ложняк соседнего окна тем же вечером: прокси вместо признака).
# ФОРМА ВЕРДИКТА НЕ ОДНА, и первая редакция этого стража знала только одну.
# Она искала `ok:` — двоеточие сразу после слова — и пропускала СЕМЬ живых
# самотестов вида `echo "селфтест X: 5/5 ok"`, где слово успеха стоит ПОСЛЕ
# числа (`test-arch-ratchet`, `test-check-invariant-discipline`,
# `test-measure-guard` и ещё четыре). Нашлось при перепроверке, глазами, на
# чужом выводе — то есть ровно тем способом, который страж и должен был
# заменить. Теперь судится любая ПЕЧАТАЮЩАЯ строка со словом успеха, а
# число ищется в ней целиком.
RE_OK_LINE = re.compile(r'^\s*(?:echo|printf)\s+"[^"]*\b(?:ok|OK|PASS)\b[^"]*"')
RE_LITERAL_COUNT = re.compile(r"\b\d+\s*/\s*\d+\b|\b\d+\s+(?:случа|свойств|properties|cases|assert)")


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def selftest_files(root, override):
    dirs = [override] if override else [os.path.join(root, "scripts", "guards", "selftest"),
                                        os.path.join(root, "scripts", "selftest")]
    out = []
    for d in dirs:
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            if fn.startswith("test-") and fn.endswith(".sh"):
                out.append(os.path.join(d, fn))
    return out


def shown(p, root):
    try:
        return os.path.relpath(p, root).replace("\\", "/")
    except ValueError:
        return p.replace("\\", "/")


def main():
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    override = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else None
    base_file = os.environ.get("NOVAC_SELFTEST_COUNT_BASELINE",
                               os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                            "selftest", "selftest-honest-count.baseline"))

    files = selftest_files(root, override)
    if not files:
        return fail("под судом ни одного самотеста `test-*.sh` — мишень потеряна, а не «нарушений 0»")

    hits = []
    for p in files:
        with io.open(p, encoding="utf-8", errors="replace") as f:
            for n, line in enumerate(f, 1):
                m = RE_OK_LINE.search(line)
                if not m:
                    continue
                text = m.group(0)
                if "$" in text:
                    continue  # число печатает счётчик — законно
                if RE_LITERAL_COUNT.search(text):
                    hits.append("%s:%d: %s" % (shown(p, root), n, text.strip()[:100]))
                    break  # один самотест — одна запись

    try:
        base_t = io.open(base_file, encoding="utf-8", errors="replace").read()
    except IOError:
        return fail("нет базы %s (ключ literal=N) — храповик судить нечем" % base_file)
    m = re.search(r"^literal=(\d+)\s*$", base_t, re.M)
    if not m:
        return fail("в базе %s нет строки literal=N — храповик судить нечем" % base_file)
    base = int(m.group(1))

    if len(hits) > base:
        sys.stderr.write("%s: FAIL — самотестов с ЛИТЕРАЛЬНЫМ числом случаев: %d, база %d. "
                         "Число, написанное рукой, расходится с телом на первом же добавленном случае "
                         "и не краснеет никогда — печатай счётчик:\n" % (NAME, len(hits), base))
        for h in hits:
            sys.stderr.write("    %s\n" % h)
        return 1

    print("%s ok: самотестов: %d, с литеральным числом случаев: %d (база %d) — счётчик, а не рука"
          % (NAME, len(files), len(hits), base))
    return 0


if __name__ == "__main__":
    sys.exit(main())
