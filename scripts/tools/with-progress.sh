#!/usr/bin/env bash
# scripts/tools/with-progress.sh — выполнить команду, печатая отметку каждые N
# секунд, чтобы долгий шаг не выглядел молчанием.
#
# ЗАЧЕМ. 2026-08-11 сторож снял ПЯТЬ окон за десять минут «без вывода» — и ни
# одно не зависло: все собирали компилятор (2.5 мин) или гоняли тесты. Правило
# «не молчи» стояло в брифе и в шаблоне, и всё равно нарушалось, потому что
# молчит не окно, а КОМАНДА: `cargo build` пишет в свой лог, а не в поток агента.
#
# Требовать дисциплины там, где нужен инструмент, — это и есть «правило вместо
# машины», от которого мы весь день уходим (реестр 221.1 №574, №578, №596).
# Поэтому здесь не пожелание, а обёртка.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/tools/with-progress.sh <секунды> <команда> [аргументы...]
# Пример:
#   bash scripts/tools/with-progress.sh 30 cargo build --release --manifest-path nova-cli/Cargo.toml
#
# Печатает: строку старта, отметку каждые <секунды>, строку финала с кодом
# возврата. Код возврата ПРОКИДЫВАЕТСЯ наружу без изменений — обёртка не должна
# превращать провал в успех (урок №597: обёртка отчиталась за то, чего не делала).

set -u

INTERVAL="${1:-30}"
shift || true

if [ "$#" -eq 0 ]; then
    echo "with-progress: нечего запускать" >&2
    echo "  использование: bash scripts/tools/with-progress.sh <сек> <команда> [арг...]" >&2
    exit 2
fi

case "$INTERVAL" in
    ''|*[!0-9]*) echo "with-progress: интервал '$INTERVAL' — не число секунд" >&2; exit 2 ;;
esac
[ "$INTERVAL" -ge 1 ] || { echo "with-progress: интервал должен быть >= 1" >&2; exit 2; }

START=$(date +%s)
echo "with-progress: старт [$*]"

"$@" &
CMD_PID=$!

while kill -0 "$CMD_PID" 2>/dev/null; do
    sleep "$INTERVAL" 2>/dev/null || break
    kill -0 "$CMD_PID" 2>/dev/null || break
    echo "with-progress: идёт $(( $(date +%s) - START ))с [$1]"
done

wait "$CMD_PID"
RC=$?
echo "with-progress: готово за $(( $(date +%s) - START ))с, код возврата $RC [$1]"
exit "$RC"
