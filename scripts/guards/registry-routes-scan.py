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

# «ЗАКРЫТ ЧАСТИЧНО» — НЕ закрыто: у записи жив остаток класса, и он обязан
# остаться в счёте. Поймано 2026-08-16 на №703 (исчерпаемость закрыта для
# именованной суммы, остаток — type-set / is T / generic-инстансы): пометка
# «частично» прятала остаток от всех трёх чисел разом.
PARTIAL = re.compile(u"ЧАСТИЧНО|ЧАСТИЧЕН|ЧАСТИЧНАЯ")
CLOSED = re.compile(u"ЗАКРЫТ|ПОЧИНЕНО|СНЯТ")
ROW = re.compile(u"^\\| [0-9]+ \\|")
K1 = u"\U0001F534"


FIELD = u"**" + u"Статус" + u":**"


def nofield_baseline(root):
    u"""Номера строк, которым разрешено не иметь поля статуса (засев 2026-09-19).

    Дом правила и его летопись — `scripts/guards/check-registry-status-field.py`
    и `registry-nofield.baseline`. Здесь КОПИЯ чтения, и она намеренна: страж
    обязан быть самодостаточным. Копии сверяет клетка самотеста
    `test-registry-status-field.py`, а не доверие.
    """
    p = os.environ.get("NOVA_NOFIELD_BASELINE") or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "registry-nofield.baseline")
    out = set()
    try:
        for line in io.open(p, encoding="utf-8", errors="replace"):
            s = line.strip()
            if s.isdigit():
                out.add(s)
    except OSError:
        return None
    return out


def refuse_unjudgeable(rows, root):
    u"""ОТКАЗ СУДИТЬ вместо домысливания (реестр 221.1 №1160).

    Прежде этот файл честно писал в докстринге: «записи без поля считаются
    открытыми — консервативно». Консервативно для ЭТОГО стража, но храповик
    закрытий на той же строке домысливает ПРОТИВОПОЛОЖНОЕ, и обе машины молчат.
    Теперь новая строка без поля не судится вовсе: отказ громкий и с номером.
    Старые держит база — иначе гейт покраснел бы на 677 строках разом.
    """
    allowed = nofield_baseline(root)
    if allowed is None:
        sys.stderr.write("registry-routes-scan: FAIL - net bazy "
                         "registry-nofield.baseline\n")
        return 1
    bad = sorted((l for l in rows if FIELD not in l), key=num)
    new = [l for l in bad if str(num(l)) not in allowed]
    if new:
        sys.stderr.write(
            u"registry-routes-scan: ОТКАЗ СУДИТЬ — строка без поля статуса: %s\n"
            % u", ".join(u"№%d" % num(l) for l in new))
        sys.stderr.write(
            u"  Числа этого стража на такой строке были бы догадкой: он счёл бы её\n"
            u"  открытой, а храповик закрытий — закрытой. Поставь поле статуса.\n")
        return 1
    return 0


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

    # Отказ проверяется ДО печати чисел: половина правды хуже молчания — её
    # потребитель (в том числе снимок очереди) примет за замер.
    rc = refuse_unjudgeable(rows, root)
    if rc:
        return rc

    open_k1, blockers = [], []
    for l in rows:
        st = status(l)
        if CLOSED.search(st) and not PARTIAL.search(st):
            continue
        if K1 in l.split(u"|")[2]:
            open_k1.append(l)
        mb = re.search(u"БЛОКИРУЕТ ТЕГ:\\*?\\*?\\s*([А-ЯA-Z]+)", l)
        if mb and mb.group(1).startswith(u"ДА"):
            blockers.append(l)

    no_route = [l for l in open_k1 if u"ЧИНИТСЯ" not in l]
    # Оговорка читается в ОБЕИХ формах — «не» и «НЕ» (и е/ё): страж формы
    # check-registry-entry-shape.sh принимает `(НЕ|не)`, и строка №998 с «приёмкой
    # НЕ считается» прошла его, а здесь легла в no_caveat (27 > 26, 2026-09-06).
    # Один канон — одна регулярка на оба стража по смыслу, не по регистру.
    CAVEAT = re.compile(u"при[её]мкой\\s+(?:не|НЕ)\\s+считается")
    no_caveat = [l for l in open_k1 if not CAVEAT.search(l)]

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
