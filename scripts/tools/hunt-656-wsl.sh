#!/usr/bin/env bash
# scripts/tools/hunt-656-wsl.sh — охота на зависание presume_446_stress
# (реестр 221.1 №656) в форме CI-раннера: WSL Ubuntu, ДВА прижатых ядра.
#
# Запуск С WINDOWS-СТОРОНЫ (дефолтный дистрибутив — docker-desktop без bash):
#   wsl -d Ubuntu -e bash /mnt/d/Sources/nv-lang/nova/scripts/tools/hunt-656-wsl.sh
#
# Предпосылки (на машине УЖЕ стоят, проверено 2026-08-14): cargo, gcc,
# /usr/include/gc.h; клон в ~/nova-656 (git clone /mnt/d/Sources/nv-lang/nova
# + submodule libuv + cargo build --release nova-cli).
#
# ПЕРВЫЙ ХИТ ПОЙМАН 2026-08-14: прогон 10/40, зависание, убит внутренним
# таймаутом раннера на 61.8с; девять прогонов до него — по 4-6с. Частота
# ~1/10 на двух ядрах.
#
# ЛОГИ — В ~, НЕ В /tmp: /tmp WSL не переживает автоостановку инстанса,
# и лог первого хита был потерян именно так.
set -u
cd ~/nova-656 || { echo "нет ~/nova-656 — сперва клон и сборка (см. шапку)"; exit 2; }
export NOVA_DIAG_446=1
LOGDIR=~/p446-logs
mkdir -p "$LOGDIR"
for i in $(seq 1 "${1:-40}"); do
  s=$(date +%s)
  timeout 120 taskset -c 0,1 ./nova-cli/target/release/nova test \
    spec_tests/conformance/standalone/presume_446_stress.nv \
    > "$LOGDIR/run_${i}.log" 2>&1
  rc=$?
  e=$(( $(date +%s) - s ))
  echo "run ${i}: rc=${rc} ${e}s"
  if [ "$rc" -ne 0 ]; then
    echo "=== HIT at run ${i} rc=${rc} — лог: $LOGDIR/run_${i}.log ==="
    tail -40 "$LOGDIR/run_${i}.log"
    exit 3
  fi
done
echo "LOOP-DONE: зависание не поймано за ${1:-40} прогонов"
