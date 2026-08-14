#!/usr/bin/env bash
# №656, заход «назови застрявшего»: ловим зависание инструментированного
# бинаря, SIGABRT -> core, и gdb-обходчик child_ctx[] печатает по каждому
# ребёнку слот/fiber_state/park_state + yielded-FIFO воркеров. Застрявший —
# тот, чей fiber_state не DEAD; его park_state говорит, где он стоит.
set -u
cd ~/nova-656 || exit 2
L=~/p446-logs; mkdir -p "$L"
ulimit -c unlimited
EXE="$L/presume.fork.bin"
[ -x "$EXE" ] || { echo "нет fork-бинаря — сперва hunt-656-fork"; exit 2; }

export NOVA_MAXPROCS=8
export NOVA_MAX_FIBERS=4000
export NOVA_WATCHDOG_DUMP_SECS=10
export NOVA_DIAG_656=1

for i in $(seq 1 60); do
  s=$(date +%s)
  taskset -c 0,1 "$EXE" > "$L/nm_${i}.log" 2>&1 &
  pid=$!
  hung=0
  while kill -0 "$pid" 2>/dev/null; do
    sleep 2
    age=$(( $(date +%s) - s ))
    if [ "$age" -ge 40 ]; then hung=1; break; fi
  done
  if [ "$hung" -eq 1 ]; then
    echo "[nm] run $i ВИСИТ — ABRT + обходчик"
    kill -ABRT "$pid" 2>/dev/null; sleep 3; kill -9 "$pid" 2>/dev/null
    CORE=$(ls -t core* 2>/dev/null | head -1)
    [ -z "$CORE" ] && { echo "core нет"; exit 4; }
    gdb -batch "$EXE" "$CORE" -x /mnt/d/Sources/nv-lang/name656.gdb \
      > "$L/nm_${i}_walk.log" 2>&1
    mv "$CORE" "$L/nm_${i}.core"
    echo "[nm] === ОБХОД: $L/nm_${i}_walk.log ==="
    grep -E 'SCOPE|W[0-9]' "$L/nm_${i}_walk.log"
    echo "[nm] дети с fst не-DEAD (DEAD=3?) — все НЕнулевые состояния:"
    grep -E '^c[0-9]+' "$L/nm_${i}_walk.log" | grep -vE 'fst=0 pst=0' | head -12
    echo "[nm] [656]-строка дампа:"
    grep '^\[656\]' "$L/nm_${i}.log" | head -2
    exit 3
  fi
  wait "$pid"; rc=$?
  echo "[nm] run $i: rc=$rc $(( $(date +%s) - s ))s"
done
echo "LOOP-DONE"
