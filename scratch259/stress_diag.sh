#!/bin/bash
# Plan 259: reproduce the intermittent hang under NOVA_MAXPROCS=16 with
# point-probe diagnostics (NOVA_DIAG_P259) active. Each run gets up to
# 5s wall-clock; if it hangs, the process is LEFT ALIVE (not killed) and
# its PID is printed so a live gdb -p can attach for a full backtrace.
EXE=~/nova-p259/scratch259/arena_budget_main.exe
LOG=~/nova-p259/scratch259/stress_diag_run.log
N="${1:-40}"
export NOVA_DIAG_P259=1
export NOVA_MAXPROCS=16
for i in $(seq 1 "$N"); do
  : > "$LOG"
  "$EXE" > "$LOG" 2>&1 &
  pid=$!
  waited=0
  while kill -0 "$pid" 2>/dev/null; do
    sleep 0.2
    waited=$((waited+1))
    if [ "$waited" -ge 25 ]; then   # 5s
      echo "RUN $i: HANG -- pid=$pid still alive after 5s. Log so far:"
      cat "$LOG"
      echo "RUN $i: HANG -- leaving pid=$pid ALIVE for gdb attach"
      exit 42
    fi
  done
  wait "$pid"
  rc=$?
  echo "RUN $i: rc=$rc out=[$(cat "$LOG")]"
done
echo "SUMMARY: $N runs, 0 hangs"
