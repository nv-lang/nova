#!/usr/bin/env python
# -*- coding: utf-8 -*-
u"""План 292 Ш.5 — побеги Stop-стража видны. ОДИН дом чтения лога.

Зачем. У хука `guard-stop-v2.py` есть предохранитель: когда он не может судить
честно — снимок неполон, вход не разобран, третья блокировка подряд — он
ПРОПУСКАЕТ окно и пишет строку в `target/.stop-guard/escapes.log`. Это верное
решение: запирать окно за чужую мёртвую сеть нельзя. Но **лог, которого никто не
читает, равен отсутствию лога**: пока побеги не видны, мы не узнаем, что механизм
отпускает окна молча, — узнаем только по жалобам, то есть поздно и не от него.

Поэтому чтение лога живёт ЗДЕСЬ, а `/status` (через `window-status.sh`) и
почасовое напоминание `remind-session-save.py` его зовут. Две копии разбора
разошлись бы на первой же правке формата строки.

Формат строки лога задаёт хук: `%Y-%m-%d %H:%M <текст>` (см. `log_escape`).

Запуск:  python scripts/tools/stop-escapes.py [КОРЕНЬ] [--hours N] [--quiet]
  --quiet — печатать ТОЛЬКО когда побеги есть (для напоминания, которое не
            должно шуметь в тишине).
Код возврата: 0 — побегов за окно нет; 1 — есть. Ошибка чтения — тоже 0 с
явной строкой: отсутствие лога не есть побег, и врать в громкую сторону здесь
так же плохо, как молчать.
"""

import io
import os
import sys
import time

LOG_REL = os.path.join(u"target", u".stop-guard", u"escapes.log")
DEFAULT_HOURS = 24
SHOW_LAST = 3


def out(line):
    sys.stdout.buffer.write(line.encode("utf-8") + b"\n")


def parse_line(line):
    u"""(метка времени, текст) или (None, строка) — строку неизвестной формы
    НЕ выбрасываем: побег, который не разобрался, всё равно побег."""
    parts = line.strip().split(u" ", 2)
    if len(parts) < 3:
        return None, line.strip()
    try:
        t = time.mktime(time.strptime(u"%s %s" % (parts[0], parts[1]), "%Y-%m-%d %H:%M"))
    except Exception:
        return None, line.strip()
    return t, parts[2]


def read(root, hours):
    p = os.path.join(root, LOG_REL)
    if not os.path.exists(p):
        return None, []
    try:
        with io.open(p, encoding="utf-8", errors="replace") as fh:
            lines = [l for l in fh.read().split(u"\n") if l.strip()]
    except Exception as e:
        return u"%s" % e, []
    edge = time.time() - hours * 3600
    fresh = []
    for l in lines:
        t, text = parse_line(l)
        if t is None or t >= edge:
            fresh.append((t, text, l))
    return None, fresh


def main(argv):
    root = u"."
    hours = DEFAULT_HOURS
    quiet = False
    i = 1
    while i < len(argv):
        a = argv[i]
        if a == u"--hours":
            i += 1
            hours = int(argv[i])
        elif a == u"--quiet":
            quiet = True
        else:
            root = a
        i += 1

    err, fresh = read(root, hours)
    if err:
        out(u"побеги Stop-стража: лог не читается (%s)" % err)
        return 0
    if not fresh:
        if not quiet:
            out(u"побеги Stop-стража за %d ч: нет" % hours)
        return 0

    out(u"ПОБЕГИ STOP-СТРАЖА за %d ч: %d — страж отпускал окно, не сумев судить"
        % (hours, len(fresh)))
    for t, text, raw in fresh[-SHOW_LAST:]:
        stamp = time.strftime("%d.%m %H:%M", time.localtime(t)) if t else u"(без метки)"
        out(u"  %s  %s" % (stamp, text[:150]))
    if len(fresh) > SHOW_LAST:
        out(u"  (и ещё %d — весь список: %s)" % (len(fresh) - SHOW_LAST, LOG_REL))
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
