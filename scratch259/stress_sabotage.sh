#!/bin/bash
EXE=~/nova-p259/scratch259/arena_budget_sabotage.exe
LOG=~/nova-p259/scratch259/sabotage_run.log
N="${1:-20}"
export NOVA_MAXPROCS=16
hangs=0
for i in $(seq 1 "$N"); do
  : > "$LOG"
  "$EXE" > "$LOG" 2>&1 &
  pid=$!
  waited=0
  while kill -0 "$pid" 2>/dev/null; do
    sleep 0.2
    waited=$((waited+1))
    if [ "$waited" -ge 25 ]; then   # 5s
      hangs=$((hangs+1))
      echo "RUN $i: HANG pid=$pid"
      break
    fi
  done
  if [ "$waited" -lt 25 ]; then
    wait "$pid" 2>/dev/null
    echo "RUN $i: rc=$? out=[$(cat "$LOG")]"
  fi
done
echo "SABOTAGE SUMMARY: N=$N HANGS=$hangs"
