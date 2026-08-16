#!/bin/sh
# scripts/guards/check-process-exit-under-pool.sh — процесс Nova ОБЯЗАН
# завершаться при полном пуле воркеров. Проверяется исполнением ×N.
#
# Реестр: docs/plans/221.1-bug-sweep.md №694 — при 16 воркерах (умолчание =
# число ядер) обычная программа `println("ok")` не завершалась в ~1% запусков:
# `stop` читался только в заголовке цикла воркера, а `uv_run(NOWAIT)` между
# проверкой и блокировкой съедал побудку остановки. Гейт этого не ловил по
# построению: каждая фикстура выходит ОДИН раз, вероятность 1% на процесс.
#
# ПОЧЕМУ ×N, а не один запуск: единичный прогон этого класса ничего не
# доказывает — ни зелёный, ни красный. До починки замер давал 6/500 при 16
# воркерах и 0/500 при 1; страж повторяет ту же выборку и требует НОЛЬ.
#
# ПОЧЕМУ число воркеров ФИКСИРУЕТСЯ явно (NOVA_MAXPROCS=16), а не берётся
# с машины: на 4-ядерном CI-раннере умолчание = 4, и дефект там невидим
# (0/400 при 4). Страж, зелёный на CI и красный на машине владельца, — не
# страж. 16 потоков на 4 ядрах — законная конфигурация, рантайм её допускает.
#
# $1 — корень репозитория; $2 — число запусков (умолчание 200 — ~30с;
#      полная выборка №694 — 500). NOVA_EXIT_GUARD_RUNS переопределяет.
# NOVA_EXIT_GUARD_NOVA — путь к компилятору; самотест подставляет поддельный,
#      который кладёт скрипт-пробу. В гейте не задаётся: страж судит бинарь.
#
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
RUNS="${NOVA_EXIT_GUARD_RUNS:-${2:-200}}"
NAME=check-process-exit-under-pool
NOVA="${NOVA_EXIT_GUARD_NOVA:-$ROOT/nova-cli/target/release/nova.exe}"
[ -x "$NOVA" ] || NOVA="$ROOT/nova-cli/target/release/nova"
if [ ! -x "$NOVA" ]; then
    echo "$NAME ok: компилятор не собран — судить нечего"
    exit 0
fi
if ! command -v timeout >/dev/null 2>&1; then
    echo "$NAME: FAIL — нет утилиты timeout, зависание не отличить от медленного выхода" >&2
    exit 1
fi

TMP=$(mktemp -d 2>/dev/null || echo "${TMPDIR:-/tmp}/$NAME.$$")
mkdir -p "$TMP"
# Минимальная программа: ни spawn, ни эффектов — только пул на старте и выход.
# Именно такая ловила №694: дефект в остановке, а не в работе.
cat >"$TMP/exit_probe_main.nv" <<'EOF'
module exit_probe.exit_probe_main

fn main() Io -> () {
    println("ok")
}
EOF
if ! "$NOVA" build "$TMP/exit_probe_main.nv" -o "$TMP/exit_probe.exe" >"$TMP/build.log" 2>&1; then
    echo "$NAME: FAIL — проба не собралась" >&2
    tail -5 "$TMP/build.log" >&2
    rm -rf "$TMP"
    exit 1
fi

HANGS=0
i=0
while [ "$i" -lt "$RUNS" ]; do
    i=$((i + 1))
    NOVA_MAXPROCS=16 timeout 8 "$TMP/exit_probe.exe" >/dev/null 2>&1
    rc=$?
    if [ "$rc" -eq 124 ]; then
        HANGS=$((HANGS + 1))
        echo "$NAME: процесс не завершился за 8с (запуск $i из $RUNS)" >&2
    fi
done
rm -rf "$TMP"

if [ "$HANGS" -ne 0 ]; then
    echo "$NAME: FAIL — $HANGS из $RUNS запусков не завершились при 16 воркерах (№694: потерянная побудка при остановке пула)" >&2
    exit 1
fi
echo "$NAME ok: $RUNS запусков при NOVA_MAXPROCS=16 — все завершились (№694)"
exit 0
