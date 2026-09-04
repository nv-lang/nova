#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-novac-deferral-address.py -- отложенная проверка называет
# АДРЕС получателя. Конвенции novac П6 «Никаких тихих дыр»
# (docs/dev/novac-compiler-conventions.md); реестр 221.1 №921 оракула; аудит
# стражей -- план 274 §10.3а. Самотест: selftest/test-check-novac-deferral-address.sh.
"""scripts/guards/check-novac-deferral-address.py — «это проверит X» пишется
ТОЛЬКО вместе с адресом в X (конвенции novac П6; реестр 221.1 №921).

ЗАЧЕМ. Замер интегратора 2026-09-04 по строке реестра №921: в чекере оракула
годами стоял комментарий «эту ошибку сообщит BoundCtx, не здесь», а BoundCtx для
трёх площадок либо выходил по `_ => return`, либо не запускался вовсе. Отсылка
вела в дверь, которая не открывается, — и месяцы никто не проверял ничего.
Отложенная проверка БЕЗ обратной ссылки — тихая дыра той же природы, что пустая
ветка без диагностики: она выглядит решением и не является им. Адрес нужен не
ради красоты: он единственное, что даёт следующему читателю СХОДИТЬ и убедиться,
что дверь открывается.

ЧТО СЧИТАЕТ. Комментарии (строки, начинающиеся с `//` или `///` после отступа) в
`novac/src/**/*.nv`, содержащие ОТСЫЛКУ ответственности — подстроку из списка
DEFERRAL (регистр не важен): `reported by`, `refuses it`, `judged by`,
`checked by`, `is the judge`, `says that once`, `the door refuses`,
`check is the judge`, а также русские `сообщит`, `проверит`, `судит`, `откажет`.

ЧТО КРАСНИТ. Такая отсылка, у которой в ОКНЕ из трёх строк (сама строка и две
соседние строки КОММЕНТАРИЯ — предыдущая и следующая) нет АДРЕСА. Адресом
считается любое из трёх:
  * `<файл>.nv:<цифра>` — точка в дереве, куда можно пойти;
  * имя в обратных кавычках, содержащее `(`, `@` или `::` — то есть функция,
    метод или путь модуля, а не просто существительное;
  * ссылка на стража `check-novac-` — механизм, который держит ту дверь.
Окно узкое намеренно: адрес, лежащий абзацем ниже, читателю строки не виден, а
страж, ищущий по всему файлу, зеленел бы от любого соседнего упоминания.

ХРАПОВИК ВНИЗ. База — `scripts/guards/novac-deferral-address.baseline`, ключ
`unaddressed=N`: рост над базой красный сразу, цель — 0. База есть ФОТОГРАФИЯ
дня заведения, а не норма; сколько в ней ложных срабатываний по оценке
заводившего — честно написано в самой базе, там же пять примеров построчно.

ПОТЕРЯ МИШЕНИ — КРАСНОЕ. Ноль НАЙДЕННЫХ отсылок (не ноль безадресных) —
отказ, а не зелёный ноль: значит либо каталог уехал, либо словарь отсылок
разошёлся с тем, как в дереве пишут. Урок охоты guards × К7 2026-09-04: девять
стражей из десяти печатали зелёный ноль, когда их якорь съезжал.

ЧЕГО СТРАЖ НЕ УМЕЕТ И НЕ ПРИТВОРЯЕТСЯ. Он не читает смысл: `judged by the PAIR`
— описание правила, а не отсылка к чужой площадке, и он посчитает его. Поэтому
мера тут — храповик вниз с честной фотографией, а не абсолютное правило: каждое
снятое место снимается ЧЕЛОВЕКОМ, который либо дописал адрес, либо
переформулировал прозу.

Аргументы: $1 — корень репозитория (по умолчанию — репозиторий стража);
$2 — override каталога `novac/src` (шов самотеста);
env NOVAC_DEFERRAL_ADDRESS_BASELINE — override файла базы (шов самотеста).
Вход для гейта — `main()`: run-guards.py исполняет стражей в одном процессе и
зовёт именно её; страж с телом на уровне модуля зелен вручную и красен в гейте.
"""
import io
import os
import re
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")

NAME = "check-novac-deferral-address"

# Отсылка ответственности: «это скажет/проверит/осудит кто-то другой».
DEFERRAL = (
    "reported by",
    "refuses it",
    "judged by",
    "checked by",
    "is the judge",
    "says that once",
    "the door refuses",
    "check is the judge",
    # Русские формы: комментарии в novac пишутся по-английски (AGENTS.md), но
    # цитаты из реестра и старые места встречаются — словарь ловит и их.
    "сообщит",
    "проверит",
    "судит",
    "откажет",
)

# Адрес: точка в дереве, вызываемое имя в кавычках, либо имя стража.
RE_ADDR = re.compile(
    r"[A-Za-z0-9_./\\-]+\.nv:\d"          # foo/bar.nv:123
    r"|`[^`\n]*(?:\(|@|::)[^`\n]*`"       # `f(...)`, `@ctx.x`, `sem::door`
    r"|check-novac-"                      # страж, держащий дверь
)

WINDOW_NOTE = ("окно адреса — три строки: сама отсылка и соседние строки комментария "
               "сверху и снизу")


def fail(msg):
    sys.stderr.write("%s: FAIL — %s\n" % (NAME, msg))
    return 1


def is_comment(line):
    return line.lstrip(" \t\v\f").startswith("//")


def has_deferral(line):
    low = line.lower()
    for w in DEFERRAL:
        if w in low:
            return w
    return None


def shown(path, root):
    """Путь так, как читатель пойдёт его искать: относительно корня, когда файл
    под ним; как дан — когда самотест смотрит на другой диск (relpath на Windows
    отказывается пересекать точки монтирования)."""
    try:
        return os.path.relpath(path, root).replace("\\", "/")
    except ValueError:
        return path.replace("\\", "/")


def nv_files(src):
    out = []
    for dirpath, dirnames, names in os.walk(src):
        dirnames.sort()
        for fn in sorted(names):
            if fn.endswith(".nv"):
                out.append(os.path.join(dirpath, fn))
    return out


def read_lines(path):
    with io.open(path, encoding="utf-8", errors="replace") as f:
        text = f.read()
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    return [ln[:-1] if ln.endswith("\r") else ln for ln in lines]


def scan(src, root):
    """→ (файлов, всего отсылок, список безадресных как 'путь:строка: текст')."""
    files = nv_files(src)
    total = 0
    bad = []
    for p in files:
        lines = read_lines(p)
        rel = shown(p, root)
        for i, line in enumerate(lines):
            if not is_comment(line):
                continue
            if has_deferral(line) is None:
                continue
            total += 1
            window = [line]
            if i > 0 and is_comment(lines[i - 1]):
                window.append(lines[i - 1])
            if i + 1 < len(lines) and is_comment(lines[i + 1]):
                window.append(lines[i + 1])
            if not RE_ADDR.search("\n".join(window)):
                bad.append("%s:%d: %s" % (rel, i + 1, line.strip()[:110]))
    return len(files), total, bad


def read_baseline(path):
    """→ (число, None) либо (None, текст-причины)."""
    try:
        text = io.open(path, encoding="utf-8", errors="replace").read()
    except IOError:
        return None, "нет базы %s (ключ unaddressed=N) — храповик судить нечем" % path
    m = re.search(r"^unaddressed=(\d+)\s*$", text, re.M)
    if not m:
        return None, "в базе %s нет строки unaddressed=N — храповик судить нечем" % path
    return int(m.group(1)), None


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1
                           else os.path.join(here, "..", ".."))
    src = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 \
        else os.path.join(root, "novac", "src")
    base_file = os.environ.get("NOVAC_DEFERRAL_ADDRESS_BASELINE",
                               os.path.join(here, "novac-deferral-address.baseline"))

    if not os.path.isdir(src):
        return fail("нет каталога %s — мишень потеряна, а не «отсылок 0»" % src)

    files, total, bad = scan(src, root)

    if files == 0:
        return fail("под судом ни одного файла .nv в %s — мишень потеряна, "
                    "а не «отсылок 0»" % src)

    if total == 0:
        sys.stderr.write(
            "%s: FAIL — в %d файлах .nv не найдено НИ ОДНОЙ отсылки ответственности "
            "(«reported by», «judged by», «сообщит», ...) — мишень потеряна.\n" % (NAME, files))
        sys.stderr.write("  Ноль — не «чисто»: либо каталог уехал, либо в дереве стали "
                         "писать отсылку иначе,\n")
        sys.stderr.write("  и страж считает несуществующую форму, печатая ноль как замер. "
                         "Словарь — DEFERRAL в этом файле.\n")
        return 1

    base, why = read_baseline(base_file)
    if base is None:
        return fail(why)

    if len(bad) > base:
        sys.stderr.write(
            "%s: FAIL — отложенных проверок БЕЗ адреса: %d, база %d (П6 «Никаких тихих дыр», "
            "реестр №921).\n" % (NAME, len(bad), base))
        sys.stderr.write("  «Это проверит X» без адреса в X — отсылка в дверь, о которой "
                         "неизвестно, открывается ли она.\n")
        for b in bad:
            sys.stderr.write("    %s\n" % b)
        sys.stderr.write("  Адресом считается: `<файл>.nv:<строка>`, имя в обратных кавычках "
                         "с `(`/`@`/`::`, либо имя стража check-novac-*.\n")
        sys.stderr.write("  %s.\n" % WINDOW_NOTE)
        return 1

    print("%s ok: файлов .nv %d, отсылок ответственности %d, из них без адреса %d (база %d) — "
          "храповик вниз, цель 0" % (NAME, files, total, len(bad), base))
    return 0


if __name__ == "__main__":
    sys.exit(main())
