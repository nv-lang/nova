# -*- coding: utf-8 -*-
"""Ядро check-registry-routes: считает по реестру 221.1 три числа и печатает
списки нарушителей. Отдельным файлом — потому что разбор строки реестра на
awk/grep уже однажды дал ложные числа (кириллица + LC_ALL=C), а тут цена
ошибки — неверная оценка «сколько осталось до тега».

Что считается ОТКРЫТЫМ: в поле `Статус:` нет ЗАКРЫТ/ПОЧИНЕНО/СНЯТ. Записи без
поля `Статус:` вовсе (старый формат) считаются открытыми — консервативно:
лучше пересчитать блокеры, чем недосчитать.
"""
import io, re, sys, os

CLOSED = re.compile(u"ЗАКРЫТ|ПОЧИНЕНО|СНЯТ")
ROW = re.compile(u"^\\| [0-9]+ \\|")
K1 = u"\U0001F534"


def status(line):
    m = re.search(u"Статус:\\s*(.{0,60})", line)
    return m.group(1) if m else u""


def num(line):
    return int(line.split(u"|")[1])


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    path = os.path.join(root, "docs", "plans", "221.1-bug-sweep.md")
    if not os.path.exists(path):
        sys.stderr.write("registry-routes-scan: нет %s\n" % path)
        return 1
    rows = [l for l in io.open(path, encoding="utf-8").read().split(u"\n") if ROW.match(l)]

    open_k1, blockers = [], []
    for l in rows:
        if CLOSED.search(status(l)):
            continue
        if K1 in l.split(u"|")[2]:
            open_k1.append(l)
        mb = re.search(u"БЛОКИРУЕТ ТЕГ:\\*?\\*?\\s*([А-ЯA-Z]+)", l)
        if mb and mb.group(1).startswith(u"ДА"):
            blockers.append(l)

    no_route = [l for l in open_k1 if u"ЧИНИТСЯ" not in l]
    no_caveat = [l for l in open_k1
                 if u"приёмкой не считается" not in l and u"приемкой не считается" not in l]

    w = sys.stdout.write
    w("no_route=%d\n" % len(no_route))
    w("no_caveat=%d\n" % len(no_caveat))
    w("blockers=%d\n" % len(blockers))
    for name, group in (("no_route_list", no_route),
                        ("no_caveat_list", no_caveat),
                        ("blockers_list", blockers)):
        w("%s:\n" % name)
        w("  " + " ".join(str(num(l)) for l in sorted(group, key=num)) + "\n")
        w("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
