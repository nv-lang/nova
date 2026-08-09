#!/bin/bash
# Plan 259: stress the arena_budget fixture WITHOUT diagnostics (no
# fprintf/mfence masking) to establish the true intermittent-hang rate
# under NOVA_MAXPROCS=16 before/after the Layer 2 fix under investigation.
EXE=~/nova-p259/scratch259/arena_budget_main.exe
LOG=~/nova-p259/scratch259/stress_clean_run.log
N="${1:-80}"
export NOVA_MAXPROCS=16
hangs=0
ok=0
for i in $(seq 1 "$N"); do
  : > "$LOG"
  "$EXE" > "$LOG" 2>&1 &
  pid=$!
  waited=0
  hung=0
  while kill -0 "$pid" 2>/dev/null; do
    sleep 0.2
    waited=$((waited+1))
    if [ "$waited" -ge 25 ]; then   # 5s
      hung=1
      break
    fi
  done
  if [ "$hung" -eq 1 ]; then
    hangs=$((hangs+1))
    echo "RUN $i: HANG pid=$pid"
    # leave it alive for inspection; do not kill here
    echo "$pid" >> ~/nova-p259/scratch259/hung_pids.txt
  else
    wait "$pid"
    rc=$?
    ok=$((ok+1))
    echo "RUN $i: rc=$rc out=[$(cat "$LOG")]"
  fi
done
echo "SUMMARY: N=$N OK=$ok HANGS=$hangs"
