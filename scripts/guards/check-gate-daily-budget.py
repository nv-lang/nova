# -*- coding: utf-8 -*-
"""scripts/guards/check-gate-daily-budget.py — ТЯЖЁЛЫЙ ярус гейта идёт не чаще
раза в сутки.

Требование владельца 2026-09-17 вечером, и оно появилось из ЗАМЕРА: за одни
сутки интегратор прогнал ярус `push` ЧЕТЫРЕЖДЫ (01:00, 09:44, 10:07, 22:58),
каждый раз занимая единственную машину на 23–30 минут, и окно Карины всё это
время стояло. На замечание владельца был дан ответ «принимаю, запишу в
команду», и он же ответил: **правило без автоматизации не работает**.

ПОЧЕМУ ОТМЕТКА ЛЕЖИТ В ОБЩЕМ `.git`, А НЕ В ДЕРЕВЕ. Ограничение защищает
МАШИНУ, а машина одна на все worktree. Отметка в рабочем дереве сделала бы
предел ПОдеревным: три окна — три «раза в сутки», то есть ровно та беда, ради
которой правило заведено. Общий каталог (`git rev-parse --git-common-dir`) виден
из любого worktree, не попадает в индекс и не требует коммита — тот же приём,
что у визитки сессии.

ЧТО СУДИТСЯ: только ярусы `push` и `full`. Ярус `loop` дешёвый и под правило не
попадает — иначе окно осталось бы вовсе без быстрой проверки и пошло бы
вслепую.

КАК СНЯТЬ ОГРАНИЧЕНИЕ, когда ждать дороже: `NOVA_GATE_DAILY_OVERRIDE="<причина>"`.
Причина ОБЯЗАТЕЛЬНА и печатается в вывод гейта — пустой override не принимается.
Законные причины названы в `/commit-push`: работа блокирует чужое окно, пакет
нужен потребителю по тегу, дерево держит слишком много несохранённого.

ЧЕГО НЕ ДЕЛАЕТ: не мешает чинить красноту (для этого и есть override с
причиной), не судит `loop`, и не знает, ЧТО именно гоняли — только когда.

usage: python scripts/guards/check-gate-daily-budget.py <ярус> [КОРЕНЬ]
Самотест: scripts/guards/selftest/test-check-gate-daily-budget.sh
"""
import io
import os
import subprocess
import sys
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-gate-daily-budget"
HEAVY = ("push", "full")
DAY = 24 * 3600


def stamp_path(root):
    """Отметка в ОБЩЕМ `.git`: машина одна на все worktree."""
    env = os.environ.get("NOVA_GATE_STAMP")
    if env:
        return env
    try:
        out = subprocess.run(["git", "-C", root, "rev-parse", "--git-common-dir"],
                             capture_output=True)
        d = out.stdout.decode("utf-8", "replace").strip()
    except Exception:
        d = ""
    if not d:
        return os.path.join(root, ".nova-gate-daily.stamp")
    if not os.path.isabs(d):
        d = os.path.join(root, d)
    return os.path.join(d, "nova-gate-daily.stamp")


def now():
    v = os.environ.get("NOVA_GATE_NOW")
    return float(v) if v else time.time()


def main():
    tier = sys.argv[1] if len(sys.argv) > 1 else "full"
    root = sys.argv[2] if len(sys.argv) > 2 else "."

    if tier not in HEAVY:
        print("%s ok: ярус %s дешёвый, суточный предел его не судит"
              % (NAME, tier))
        return 0

    p = stamp_path(root)
    reason = os.environ.get("NOVA_GATE_DAILY_OVERRIDE", "").strip()
    prev = None
    if os.path.isfile(p):
        try:
            prev = float(io.open(p, encoding="utf-8",
                                 errors="replace").read().split()[0])
        except Exception:
            prev = None

    t = now()
    if prev is not None and (t - prev) < DAY and not reason:
        left = DAY - (t - prev)
        print("%s: ТЯЖЁЛЫЙ ЯРУС УЖЕ ШЁЛ СЕГОДНЯ — осталось %dч %02dм"
              % (NAME, int(left // 3600), int((left % 3600) // 60)),
              file=sys.stderr)
        print("    Предел владельца 2026-09-17: полный гейт не чаще раза в",
              file=sys.stderr)
        print("    сутки. Машина одна на все окна, и каждый прогон — 23-30", file=sys.stderr)
        print("    минут чужого простоя (замер: четыре прогона за сутки).",
              file=sys.stderr)
        print("    Вместо него, по возрастанию цены: точечный страж (секунды),",
              file=sys.stderr)
        print("    одна фикстура (`nova test … --filter <имя>`), ярус loop.",
              file=sys.stderr)
        print("    Ждать дороже? NOVA_GATE_DAILY_OVERRIDE=\"<причина>\" —",
              file=sys.stderr)
        print("    причина печатается в вывод и попадает в доклад.",
              file=sys.stderr)
        print("%s: FAIL" % NAME, file=sys.stderr)
        return 1

    # Отметка ставится на СТАРТЕ: машину занимает запуск, а не успешный конец.
    try:
        io.open(p, "w", encoding="utf-8", newline="\n").write(
            "%d %s\n" % (int(t), tier))
    except Exception as e:
        print("%s: ВНИМАНИЕ — отметку не записать (%s): предел не удержится"
              % (NAME, e), file=sys.stderr)

    if reason and prev is not None and (t - prev) < DAY:
        print("%s: суточный предел СНЯТ ОСОЗНАННО — причина: %s" % (NAME, reason))
    else:
        print("%s ok: тяжёлого яруса в последние сутки не было" % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
