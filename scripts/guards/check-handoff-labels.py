# -*- coding: utf-8 -*-
"""scripts/guards/check-handoff-labels.py — метка раздела в ролевой записке
есть ПОЛНАЯ ДАТА, а не четыре цифры и не латинское слово.

Адрес: план 290 пункт 3 (сохранение сессии), команда `/save` — дом правила.

ЗАЧЕМ — замер 2026-09-17 по вопросу владельца «команда `/save` сохраняет в
неправильный файл». Файл оказался ВЕРНЫМ (ветка `main` → записка интегратора,
`p274-novac` → записка окна Карины; команда и хук согласны), а разошлись МЕТКИ
разделов. В двух записках нашлось ШЕСТЬ схем сразу:

    0-0917-вечер   ММДД плюс часть суток          (интегратор)
    0-0916, 0-0808 четыре цифры: ДДММ или ММДД — НЕ РАЗЛИЧИТЬ
    0-s14, 0-a31   буква месяца плюс день
    0-nox, 0-bis, 0-ter   латынь, даты нет вовсе
    0-1722, 0-1700 день плюс ЧАС                   (окно Карины)

Одна и та же четырёхзначная форма значила в двух файлах разное: `0-1609` — это
16 сентября, а `0-0917` — 17 сентября. Метки одного дня нельзя ни сопоставить,
ни отсортировать. Латинские метки родились там же, где и всякая импровизация:
схема не различала второй раздел за день, и окно придумывало «ещё один».

ЧТО СУДИТСЯ. Строки вида `## 0-…` в двух ролевых записках. Метка обязана быть
`0-ГГГГ-ММ-ДД` с необязательной частью суток через дефис. Полная дата не
сталкивается ни между месяцами (случай `0-a14`: 14 августа против 14 сентября,
ради него и заводился префикс `0-s`), ни между годами, и сортируется как текст.

ПОЧЕМУ БАЗА — СПИСОК, А НЕ ЧИСЛО. Прежние метки НЕ переименовываются: они
история, на них ссылаются, и правка ради единообразия сломала бы ссылки.
Поэтому каждая старая метка внесена в базу поимённо; счётчик разрешил бы
завести новую кривую метку взамен снятой старой — молча.

ЧЕГО НЕ ПРОВЕРЯЕТ (сказано честно): что раздел стоит СВЕРХУ, что в нём пять
частей, и что написанное в нём правда. Это судит человек по `/save`.

usage: python scripts/guards/check-handoff-labels.py [КОРЕНЬ]
Самотест: scripts/guards/selftest/test-check-handoff-labels.sh
"""
import io
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-handoff-labels"

NOTES_DIR = os.path.join("docs", "dev", "prompts")
NOTES_GLOB = "*handoff*.md"


def notes(root):
    u"""Записки СНИМАЮТСЯ С ДЕРЕВА на каждом прогоне, а не зашиты списком.

    ПОЧЕМУ ЗНАМЕНАТЕЛЬ — ЗАПИСКИ, А НЕ МЕТКИ (правка 2026-09-20, разбор
    интегратора). До неё страж перечислял МЕТКИ и о каждой спрашивал, хороша ли
    она; предметом же были ЗАПИСКИ. Записка без единой метки не «проваливала
    проверку» — она в проверку НЕ ВХОДИЛА, и ноль нарушений при нуле
    осмотренного читался как «чисто». Замер, на котором это поймано: внесение
    четырёх записок в прежний зашитый список добавило ДВЕ метки (35 -> 37), то
    есть две записки остались невидимыми, а вывод выглядел нормально.

    Зашитый список имел ту же болезнь сбоку: восьмая записка не судилась бы,
    пока кто-нибудь не вспомнит вписать её руками. Поэтому список снимается
    грепом, а его размер ПЕЧАТАЕТСЯ — знаменатель обязан быть виден.
    """
    d = os.path.join(root, NOTES_DIR)
    if not os.path.isdir(d):
        return []
    out = []
    for name in sorted(os.listdir(d)):
        if name.endswith(".md") and "handoff" in name:
            out.append(os.path.join(NOTES_DIR, name))
    return out
LABEL = re.compile(r"^##\s+(0-\S+?)[.\s]", re.M)
GOOD = re.compile(r"^0-\d{4}-\d{2}-\d{2}(?:-[^\W\d_]+)?$", re.UNICODE)


def baseline(root):
    p = os.path.join(root, "scripts", "guards", "handoff-labels.baseline")
    out = set()
    if not os.path.isfile(p):
        return out, p
    for line in io.open(p, encoding="utf-8", errors="replace"):
        line = line.split("#", 1)[0].strip()
        if line:
            out.add(line)
    return out, p


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    allowed, bpath = baseline(root)

    seen = []
    files_read = 0
    mute = []                 # записки БЕЗ единой метки — их страж не видел
    for rel in notes(root):
        p = os.path.join(root, rel)
        if not os.path.isfile(p):
            continue
        files_read += 1
        text = io.open(p, encoding="utf-8", errors="replace").read()
        found = [m.group(1) for m in LABEL.finditer(text)]
        if not found:
            mute.append(os.path.basename(rel))
        for l in found:
            seen.append((os.path.basename(rel), l))

    if files_read == 0:
        print("%s ok: судить нечего (ролевых записок нет)" % NAME)
        return 0

    # Записки, у которых метки не заведены ИСТОРИЧЕСКИ, держатся базой и
    # храповиком ВНИЗ — как всё прочее в этом каталоге. Без базы правка
    # покрасила бы записки закрытых окон за то, что их никто не ведёт, и страж
    # сняли бы первым же днём; с базой новая безметочная записка краснеет, а
    # старая — названа и посчитана.
    mute_allowed = {l[len("nolabel:"):].strip()
                    for l in allowed if l.startswith("nolabel:")}
    # Записи `nolabel:` — НЕ метки, и в счёте меток им делать нечего: иначе
    # страж объявил бы их «метками, которых в записках больше нет» и посоветовал
    # снять то, что держит его же храповик. Поймано на первом прогоне.
    allowed = {l for l in allowed if not l.startswith("nolabel:")}

    bad = [(f, l) for f, l in seen if not GOOD.match(l) and l not in allowed]
    stale = sorted(allowed - {l for _f, l in seen})

    print("%s: ОСМОТРЕНО ЗАПИСОК %d, меток %d, в базе %d, вне схемы сверх базы %d"
          % (NAME, files_read, len(seen), len(allowed), len(bad)))

    mute_new = [f for f in mute if f not in mute_allowed]
    mute_fixed = sorted(mute_allowed - set(mute))
    print("%s: записок без единой метки %d (в базе %d), новых %d"
          % (NAME, len(mute), len(mute_allowed), len(mute_new)))
    if mute_fixed:
        print("%s: записки, получившие метки, — сними их из базы ТОЙ ЖЕ правкой: %s"
              % (NAME, ", ".join(mute_fixed)))
    sys.stdout.flush()
    mute = mute_new
    if mute:
        # Записка без единой метки — ОТКАЗ, а не тишина: именно так выглядит
        # пустая мишень, и именно её ноль нарушений выдаёт за чистоту.
        print("%s: ЗАПИСКА БЕЗ ЕДИНОЙ МЕТКИ РАЗДЕЛА (%d) — страж такую НЕ ВИДИТ:"
              % (NAME, len(mute)), file=sys.stderr)
        for f in mute:
            print("    %s" % f, file=sys.stderr)
        print("", file=sys.stderr)
        print("    Раздел состояния обязан начинаться меткой `## 0-ГГГГ-ММ-ДД[-часть]`.",
              file=sys.stderr)
        print("    Записка без метки не проваливает проверку — она в неё не входит,",
              file=sys.stderr)
        print("    и ноль нарушений при нуле осмотренного читается как «чисто».",
              file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1
    if stale:
        print("%s: в базе есть метки, которых в записках больше нет (%d) — "
              "их можно снять: %s" % (NAME, len(stale), ", ".join(stale[:6])))

    if bad:
        print("%s: МЕТКА РАЗДЕЛА НЕ ЕСТЬ ПОЛНАЯ ДАТА:" % NAME, file=sys.stderr)
        for f, l in bad:
            print("    %-28s %s" % (l, f), file=sys.stderr)
        print("", file=sys.stderr)
        print("    Нужна форма `0-ГГГГ-ММ-ДД` с необязательной частью суток:",
              file=sys.stderr)
        print("    `## 0-2026-09-17-вечер. Состояние на …`.", file=sys.stderr)
        print("    Четыре цифры читаются и как ДДММ, и как ММДД, и как",
              file=sys.stderr)
        print("    «день плюс час» — в двух записках так и вышло, три разных",
              file=sys.stderr)
        print("    смысла у одной формы (замер 2026-09-17).", file=sys.stderr)
        print("    Старую метку НЕ переименовывай — внеси её в %s." % bpath,
              file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    print("%s ok: каждая метка раздела — полная дата либо названа в базе" % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
