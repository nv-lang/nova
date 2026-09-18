#!/usr/bin/env python
# -*- coding: utf-8 -*-
u"""План 292 Ш.6 — ПУЛЬС: окно, которое умерло, не останавливается — оно замолкает.

Зачем это отдельно от Stop-стража, и почему он этого не может в принципе.
`guard-stop-v2.py` судит КОНЕЦ ХОДА: окно закончило говорить — страж спрашивает
доказательство. Но окно, которое умерло (упало, ушло в вечное ожидание, снято
сторожем за десять минут молчания), конца хода не производит ВОВСЕ. Для стража
оно неотличимо от окна, которое работает уже сорок минут: и там и там тишина.
Значит нужен наблюдатель СНАРУЖИ времени хода — по часам, а не по событию.

Что меряется. Возраст ролевой записки (`/save` пишет её, и это единственный
машинный след того, что окно живо и помнит себя). Порог — час, то же число
владельца, что у `remind-session-save.py` (план 290 п.3): записка старше часа
означает либо что окно не сохраняется, либо что его больше нет, и ОБА случая
нужны владельцу одинаково.

Чего пульс НЕ делает и делать не должен: он не судит качество работы, не читает
очередь и никого не останавливает. Он отвечает на один вопрос — «эта роль ещё
подаёт признаки жизни?» — и зовёт владельца, когда ответ «нет».

Запуск:  python scripts/tools/role-pulse.py [КОРЕНЬ]
Порог:   NOVA_PULSE_MAX_AGE_SEC (умолчание 3600) — понижается на пробу, см.
         приёмку Ш.6: «порог занижен до минуты — зов приходит; возвращён — молчит».
Код возврата: 0 — все роли в пределах порога (или их записок нет); 1 — есть
молчащие, и зов напечатан.
"""

import io
import os
import sys
import time

DEFAULT_MAX_AGE_SEC = 3600

# Роль -> записка. Тот же список, что у `remind-session-save.py`; здесь он копией
# НЕ является по смыслу: там он отвечает на «кому напомнить в ЭТОМ окне», здесь —
# «какие роли вообще существуют». Расхождение поймает клетка самотеста, которая
# сверяет оба списка.
ROLE_NOTES = [
    (u"integrator", os.path.join(u"docs", u"dev", u"prompts", u"integrator-handoff.md")),
    (u"carina", os.path.join(u"docs", u"dev", u"prompts", u"carina-handoff.md")),
    (u"controller", os.path.join(u"docs", u"dev", u"prompts", u"controller-handoff.md")),
]


def out(line):
    sys.stdout.buffer.write(line.encode("utf-8") + b"\n")


def age_of(path):
    try:
        return time.time() - os.path.getmtime(path)
    except OSError:
        return None


def human(sec):
    h = int(sec // 3600)
    m = int((sec % 3600) // 60)
    return u"%dч %02dм" % (h, m) if h else u"%d мин" % m


def check(root, max_age):
    alive, silent, missing = [], [], []
    for role, rel in ROLE_NOTES:
        p = os.path.join(root, rel)
        a = age_of(p)
        if a is None:
            missing.append((role, rel))
        elif a > max_age:
            silent.append((role, rel, a))
        else:
            alive.append((role, rel, a))
    return alive, silent, missing


def main(argv):
    root = argv[1] if len(argv) > 1 else u"."
    try:
        max_age = int(os.environ.get("NOVA_PULSE_MAX_AGE_SEC") or DEFAULT_MAX_AGE_SEC)
    except ValueError:
        max_age = DEFAULT_MAX_AGE_SEC

    alive, silent, missing = check(root, max_age)

    if not silent:
        # Молчание пульса — ЗАКОННЫЙ ответ, и он печатается: задача по расписанию,
        # которая ничего не печатает, неотличима от незапустившейся.
        out(u"пульс: все роли в пределах порога (%s); живых записок %d, отсутствует %d"
            % (human(max_age), len(alive), len(missing)))
        return 0

    out(u"=== ПУЛЬС: ВЛАДЕЛЬЦУ — %d роль(и) молчат дольше порога (%s) ==="
        % (len(silent), human(max_age)))
    for role, rel, a in silent:
        out(u"  %-11s записка не обновлялась %s  (%s)" % (role, human(a), rel))
    out(u"Это НЕ то же, что остановка: остановившееся окно Stop-страж ловит на конце")
    out(u"хода, а умершее конца хода не производит вовсе — его видно только по часам.")
    out(u"Что делать: спросить окно напрямую; если не отвечает — поднять роль заново")
    out(u"по её записке (`/integrator`, `/carina`, `/controller`).")
    for role, rel, a in alive:
        out(u"  (жива: %s, %s назад)" % (role, human(a)))
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
