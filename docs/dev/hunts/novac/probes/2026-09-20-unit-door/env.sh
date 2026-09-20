#!/bin/sh
# Окружение проб этой охоты. ПУТИ ВЫВОДЯТСЯ ОТ КОРНЯ РЕПОЗИТОРИЯ, а не
# записаны буквально: машинный путь в отслеживаемом скрипте — это реестр 221.1
# №698, и первая редакция этого файла его нарушала (интегратор, при приёмке
# охоты 2026-09-20 — страж поймал +5 путей).
#
# Что это НЕ отменяет: проба всё равно фиксирует команду конкретной машины,
# и имя `nova.exe` тут windows-специфично. Это названный зазор, а не оплошность:
# на другой платформе отсутствие файла даёт ГРОМКУЮ ошибку, а правило №698
# заведено против МОЛЧАЛИВОГО расхождения.
#
# Использование: `. ./env.sh` из каталога пробы, либо `ROOT=<repo> . env.sh`.

if [ -z "$ROOT" ]; then
    # корень ищем вверх по дереву от текущего каталога — по признаку AGENTS.md
    d=$(pwd)
    while [ "$d" != "/" ] && [ ! -f "$d/AGENTS.md" ]; do d=$(dirname "$d"); done
    ROOT="$d"
fi

if [ ! -f "$ROOT/AGENTS.md" ]; then
    echo "env.sh: не нашёл корень репозитория; задай ROOT=<repo>" >&2
    return 1 2>/dev/null || exit 1
fi

export NOVA="$ROOT/nova-cli/target/release/nova.exe"
export NOVAC="$ROOT/novac/target/novac.exe"
export NOVA_STD_PATH="$ROOT/std/src"
export NOVA_CG_INCLUDE="$ROOT/compiler-codegen"
export NOVA_RT_DIR="$ROOT/compiler-codegen/nova_rt"
