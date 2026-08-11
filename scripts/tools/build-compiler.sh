#!/usr/bin/env bash
# scripts/tools/build-compiler.sh — собрать компилятор и ПРОВЕРИТЬ РЕЗУЛЬТАТ,
# а не код возврата запускающей команды.
#
# ЗАЧЕМ (реестр 221.1 №597). 2026-08-11: `nohup cargo build … &` вернул ноль,
# задача отметилась завершённой — а бинаря не появилось и строки `Finished` в
# логе не было. На этом «успехе» интегратор построил вывод, что проверка
# «падал ли тест до слияния» состоялась; она не состоялась, и слово «регресс»
# пришлось снимать из записи как недоказанное.
#
# Класс — тот же, что у №475 и №585: код возврата ОБЁРТКИ принят за код
# возврата РАБОТЫ. Обёртка отчиталась за то, чего не делала.
#
# ЧТО ПРОВЕРЯЕТСЯ ПОСЛЕ СБОРКИ, помимо кода возврата cargo:
#   1. в логе есть строка `Finished` — cargo дошёл до конца;
#   2. бинарь СУЩЕСТВУЕТ;
#   3. бинарь НЕ СТАРШЕ самого свежего исходника — иначе он от прошлой сборки,
#      и все дальнейшие замеры пойдут по коду, которого в нём нет.
#
# Третья проверка — не педантизм: 2026-08-11 интегратор час измерял поведение,
# считая, что бинарь пересобран, и ошибся при чтении `ls -la`.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/tools/build-compiler.sh [КОРЕНЬ]

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || { echo "build-compiler: нет каталога $ROOT" >&2; exit 1; }

LOG="${TMPDIR:-/tmp}/build_compiler_$$.log"
BIN="$ROOT/nova-cli/target/release/nova.exe"
[ -e "$BIN" ] || BIN="$ROOT/nova-cli/target/release/nova"

echo "build-compiler: старт"
bash "$ROOT/scripts/tools/with-progress.sh" 30 \
    cargo build --release --manifest-path "$ROOT/nova-cli/Cargo.toml" > "$LOG" 2>&1
RC=$?
tail -3 "$LOG" | sed 's/^/    /'

if [ "$RC" -ne 0 ]; then
    echo "build-compiler: FAIL — cargo вернул $RC" >&2
    grep -aE "^error" "$LOG" | head -5 | sed 's/^/    /' >&2
    rm -f "$LOG"
    exit 1
fi

if ! grep -aq "Finished" "$LOG"; then
    echo "build-compiler: FAIL — cargo вернул 0, но строки 'Finished' в логе НЕТ." >&2
    echo "    Это ровно №597: обёртка отчиталась за работу, которой не было." >&2
    rm -f "$LOG"
    exit 1
fi
rm -f "$LOG"

if [ ! -e "$BIN" ]; then
    echo "build-compiler: FAIL — сборка «прошла», а бинаря $BIN нет" >&2
    exit 1
fi

# Свежесть: бинарь обязан быть не старше самого свежего исходника.
NEWEST_SRC=$(find "$ROOT/compiler-codegen/src" "$ROOT/nova-cli/src" -name '*.rs' -newer "$BIN" -print -quit 2>/dev/null)
if [ -n "$NEWEST_SRC" ]; then
    echo "build-compiler: FAIL — бинарь СТАРШЕ исходника: $NEWEST_SRC" >&2
    echo "    Значит он от прошлой сборки, и всё, что ты им измеришь, будет" >&2
    echo "    измерением кода, которого в нём нет (№597)." >&2
    exit 1
fi

echo "build-compiler ok: $BIN собран и не старше исходников"
exit 0
