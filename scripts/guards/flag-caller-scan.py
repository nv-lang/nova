# -*- coding: utf-8 -*-
"""Ядро check-flag-has-caller: печатает флаги `NOVA_*`, которые код ЧИТАЕТ, но
о которых за пределами Rust-исходников никто не говорит — ни тот, кто их
взводит, ни тот, кто их описывает.

СКОРОСТЬ — ЧАСТЬ РАБОТОСПОСОБНОСТИ (замер 2026-08-27, реестр 221.1 №784).
Прежняя редакция жила в оболочке и шла ЦИКЛОМ по флагам, делая на КАЖДЫЙ полный
рекурсивный `grep` по `scripts`, `docs`, `.github`. Флагов 114, в `docs` один
только реестр 221.1 весит 2.2 МБ — итого 113 секунд на одном шаге яруса, чей
бюджет весь 240. Под чужой нагрузкой шаг раздувался до 160с и валил ярус по
бюджету шесть прогонов подряд.

ПОЧЕМУ НЕ `grep -oFf` ОДНИМ ПРОХОДОМ, хотя это первое, что приходит в голову:
пробовал 2026-08-27, вышло 2.8с и НЕВЕРНЫЙ ответ. При перекрывающихся шаблонах
`grep -o` печатает не все совпадения: если один флаг — префикс другого,
то на строке с длинным именем короткий не печатается вовсе.
Три флага ложно попали в безмолвные.

ИМЁН ФЛАГОВ ЗДЕСЬ НЕТ НАМЕРЕННО, и это третья ошибка того же дня.
Файл лежит в `scripts/`, а `scripts/**.py` входит в ЗОНУ ПОИСКА этого же
стража. Названный здесь флаг считался бы «описанным» — и долг падал бы с 2 до 0
не работой, а упоминанием в комментарии. Проверено запуском: два из трёх не
упомянуты больше НИГДЕ в зоне, то есть это ровно те два, что стоят в базе.
Фикс СКОРОСТИ не вправе двигать вердикт — ни проходом, ни собственным текстом.

КАК ЗДЕСЬ. Один проход по зоне вытаскивает ВСЕ токены вида `NOVA_[A-Z_0-9]+`.
Флаг считается упомянутым, если он подстрока какого-нибудь токена. Это ТОЧНО
равно прежней семантике «подстрока где угодно в файле»: любое вхождение флага
лежит внутри максимального токена того же вида, а флаг сам этому виду
удовлетворяет. Токенов сотни, флагов сотня — сверка мгновенна.
"""
import io
import os
import re
import sys

FLAG = re.compile(r"NOVA_[A-Z_0-9]+")
READS = re.compile(r'env::var(?:_os)?\("(NOVA_[A-Z_0-9]+)"')
SRC_DIRS = ("compiler-codegen/src", "nova-cli/src")
ZONE_DIRS = ("scripts", "docs", ".github")
ZONE_FILES = ("AGENTS.md",)
ZONE_EXT = (".sh", ".yml", ".yaml", ".md", ".toml", ".py")


def read(path):
    try:
        return io.open(path, encoding="utf-8", errors="replace").read()
    except (IOError, OSError):
        return ""


def flags_read_by_code(root):
    found = set()
    for d in SRC_DIRS:
        base = os.path.join(root, d)
        if not os.path.isdir(base):
            continue
        for dp, _dn, fn in os.walk(base):
            for f in fn:
                if f.endswith(".rs"):
                    found.update(READS.findall(read(os.path.join(dp, f))))
    return found


def tokens_in_zone(root):
    seen = set()
    for d in ZONE_DIRS:
        base = os.path.join(root, d)
        if not os.path.isdir(base):
            continue
        for dp, _dn, fn in os.walk(base):
            for f in fn:
                if f.endswith(ZONE_EXT):
                    seen.update(FLAG.findall(read(os.path.join(dp, f))))
    for f in ZONE_FILES:
        p = os.path.join(root, f)
        if os.path.isfile(p):
            seen.update(FLAG.findall(read(p)))
    return seen


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    if not os.path.isdir(root):
        sys.stderr.write("flag-caller-scan: nyet kataloga %s\n" % root)
        return 2

    flags = flags_read_by_code(root)
    if not flags:
        print("flags=0")
        return 0

    toks = tokens_in_zone(root)
    joined = "\n".join(sorted(toks))
    silent = sorted(f for f in flags if f not in joined)

    print("flags=%d" % len(flags))
    print("silent=%d" % len(silent))
    for f in silent:
        print("SILENT %s" % f)
    return 0


if __name__ == "__main__":
    sys.exit(main())
