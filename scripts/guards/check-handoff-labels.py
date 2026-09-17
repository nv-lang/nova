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

NOTES = (
    os.path.join("docs", "dev", "prompts", "integrator-handoff.md"),
    os.path.join("docs", "dev", "prompts", "carina-handoff.md"),
)
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
    for rel in NOTES:
        p = os.path.join(root, rel)
        if not os.path.isfile(p):
            continue
        files_read += 1
        text = io.open(p, encoding="utf-8", errors="replace").read()
        for m in LABEL.finditer(text):
            seen.append((os.path.basename(rel), m.group(1)))

    if files_read == 0:
        print("%s ok: судить нечего (ролевых записок нет)" % NAME)
        return 0

    bad = [(f, l) for f, l in seen if not GOOD.match(l) and l not in allowed]
    stale = sorted(allowed - {l for _f, l in seen})

    print("%s: записок %d, меток %d, в базе %d, вне схемы сверх базы %d"
          % (NAME, files_read, len(seen), len(allowed), len(bad)))
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
