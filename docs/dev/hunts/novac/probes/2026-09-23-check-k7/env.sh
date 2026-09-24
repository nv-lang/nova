#!/bin/sh
# Окружение проб охоты 2026-09-23 check x К7.
#
# ПУТИ ВЫВОДЯТСЯ ОТ КОРНЯ РЕПОЗИТОРИЯ, а не записаны буквально: машинный путь
# в отслеживаемом скрипте — это реестр 221.1 №698. Форма взята у env.sh охоты
# 2026-09-21 check x К4 намеренно, чтобы у проб трека был один вход.
#
# Использование: `. ../env.sh` из каталога пробы, либо `ROOT=<repo> . env.sh`.
#
# NOVAC/NOVA, ЗАДАННЫЕ СНАРУЖИ, НЕ ПЕРЕБИВАЮТСЯ: окно, принимающее отчёт, может
# судить пробы бинарями другого дерева. Молча взять несуществующий путь значило
# бы получить rc=127 и прочитать его как «расхождения нет».

if [ -z "$ROOT" ]; then
    d=$(pwd)
    while [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    ROOT="$d"
fi

if [ ! -f "$ROOT/AGENTS.md" ]; then
    echo "env.sh: не нашёл корень репозитория; задай ROOT=<repo>" >&2
    return 1 2>/dev/null || exit 1
fi

export ROOT
[ -n "$NOVA" ]  || NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -n "$NOVAC" ] || NOVAC="$ROOT/novac/target/novac.exe"
export NOVA NOVAC
# Каталог этой охоты от корня — обе команды зовутся ИЗ КОРНЯ, потому что
# `novac check`, позванный из каталога пробы, не находит std/src и судит файл
# без объявлений std (замер охоты 2026-09-21).
export HUNT=docs/dev/hunts/novac/probes/2026-09-23-check-k7

if [ ! -x "$NOVAC" ]; then
    echo "env.sh: не вижу novac по пути $NOVAC — задай NOVAC=<...>/novac/target/novac.exe" >&2
    return 1 2>/dev/null || exit 1
fi
if [ ! -x "$NOVA" ]; then
    echo "env.sh: не вижу оракул по пути $NOVA — задай NOVA=<...>/nova-cli/target/release/nova.exe" >&2
    return 1 2>/dev/null || exit 1
fi
