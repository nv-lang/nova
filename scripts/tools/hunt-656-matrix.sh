#!/usr/bin/env bash
# scripts/tools/hunt-656-matrix.sh — матрица дискриминаторов №656 за один
# заход: четыре параллельные полосы, каждая на СВОЕЙ паре прижатых ядер
# (репро требует давления на 2 ядрах — параллельность полос его сохраняет).
#
#   A (ядра 0,1)  базовая: как CI, NOVA_DIAG_446=1 — частота и живой лог;
#   B (ядра 2,3)  GC_DONT_GC=1: лечит ли отключение сборщика;
#   C (ядра 4,5)  NOVA_MAXPROCS=1 (копия фикстуры с правленой ENV-строкой):
#                 жива ли гонка без межворкерного steal/wake;
#   D (ядра 6,7)  ловец стека: при прогоне дольше 35с (норма 5с) —
#                 gdb -batch ко ВСЕМ процессам прогона (драйвер и тест-exe),
#                 полный дамп нитей, потом добить.
#
# Запуск: wsl -d Ubuntu -e bash /mnt/d/Sources/nv-lang/nova/scripts/tools/hunt-656-matrix.sh
# Логи: ~/p446-logs/ (НЕ /tmp — он не переживает автоостановку WSL, №656).
set -u
cd ~/nova-656 || { echo "нет ~/nova-656"; exit 2; }
NOVA=./nova-cli/target/release/nova
FIX=spec_tests/conformance/standalone/presume_446_stress.nv
L=~/p446-logs; mkdir -p "$L"
ROUNDS="${1:-40}"

# Полоса C: копия фикстуры с MAXPROCS=1 (ENV-директиву честнее править в
# копии, чем полагаться на приоритет ambient-env — приоритет не документирован).
FIX1=spec_tests/conformance/standalone/presume_446_stress_m1probe.nv
# И ENV, И декларацию модуля: копия под другим именем файла обязана нести
# другое имя модуля (E_D78 — наступлено первым прогоном матрицы: полоса C
# умерла за 0с, не запустив ни одного раунда).
sed -e 's/^\/\/ ENV NOVA_MAXPROCS=8$/\/\/ ENV NOVA_MAXPROCS=1/' \
    -e 's/^module standalone\.presume_446_stress$/module standalone.presume_446_stress_m1probe/' \
    "$FIX" > "$FIX1"
grep -q 'NOVA_MAXPROCS=1' "$FIX1" || { echo "sed не взял ENV-строку"; exit 2; }
grep -q '_m1probe$' "$FIX1" || { echo "sed не взял module-строку"; exit 2; }

lane_loop() { # имя ядра env-префикс фикстура
  local name="$1" cores="$2" envp="$3" fix="$4"
  for i in $(seq 1 "$ROUNDS"); do
    local s rc e
    s=$(date +%s)
    env NOVA_DIAG_446=1 $envp timeout 120 taskset -c "$cores" \
      "$NOVA" test "$fix" > "$L/${name}_${i}.log" 2>&1
    rc=$?
    e=$(( $(date +%s) - s ))
    echo "[$name] run $i: rc=$rc ${e}s"
    if [ "$rc" -ne 0 ]; then
      echo "[$name] === HIT run $i rc=$rc — $L/${name}_${i}.log ==="
      return 3
    fi
  done
  echo "[$name] чисто за $ROUNDS прогонов"
  return 0
}

lane_stacks() { # ловец стека, ядра 6,7
  local i s alive age pids
  for i in $(seq 1 "$ROUNDS"); do
    s=$(date +%s)
    env NOVA_DIAG_446=1 taskset -c 6,7 "$NOVA" test "$FIX" \
      > "$L/D_${i}.log" 2>&1 &
    local drv=$!
    while true; do
      sleep 2
      if ! kill -0 "$drv" 2>/dev/null; then break; fi
      age=$(( $(date +%s) - s ))
      if [ "$age" -ge 35 ]; then
        echo "[D] run $i ВИСИТ ${age}с — снимаю стеки"
        pids=$(pgrep -f 'presume_446_stress' | tr '\n' ' ')
        echo "[D] процессы: драйвер=$drv остальные: $pids"
        for p in $drv $pids; do
          [ "$p" = "$drv" ] && tag=driver || tag=proc
          gdb -batch -p "$p" -ex 'set pagination off' \
              -ex 'thread apply all bt' \
              > "$L/D_${i}_stacks_${tag}_${p}.log" 2>&1
        done
        kill -9 $pids $drv 2>/dev/null
        echo "[D] === СТЕКИ СНЯТЫ: $L/D_${i}_stacks_*.log ==="
        return 3
      fi
    done
    echo "[D] run $i: завершился сам за $(( $(date +%s) - s ))с"
  done
  echo "[D] чисто за $ROUNDS прогонов"
  return 0
}

lane_loop A 0,1 ""             "$FIX"  & PA=$!
lane_loop B 2,3 "GC_DONT_GC=1" "$FIX"  & PB=$!
lane_loop C 4,5 ""             "$FIX1" & PC=$!
lane_stacks                            & PD=$!
wait $PA; RA=$?
wait $PB; RB=$?
wait $PC; RC_=$?
wait $PD; RD=$?
rm -f "$FIX1"
echo "=== МАТРИЦА: A(база)=$RA B(без GC)=$RB C(1 воркер)=$RC_ D(стеки)=$RD (3=хит, 0=чисто) ==="
