#!/bin/bash
# Plan 259: catch the rare (~0.5%) multi-second stall live with gdb.
# The moment a run is still alive past 1.5s, immediately gdb -p it
# (no more waiting) and dump all-thread backtraces, THEN let it continue
# (detach, don't kill) so we also see its eventual outcome.
EXE=~/nova-p259/scratch259/arena_budget_main.exe
LOG=~/nova-p259/scratch259/catch_run.log
DUMPDIR=~/nova-p259/scratch259/catch_dumps
mkdir -p "$DUMPDIR"
N="${1:-400}"
export NOVA_MAXPROCS=16
for i in $(seq 1 "$N"); do
  : > "$LOG"
  "$EXE" > "$LOG" 2>&1 &
  pid=$!
  waited=0
  while kill -0 "$pid" 2>/dev/null; do
    sleep 0.1
    waited=$((waited+1))
    if [ "$waited" -ge 15 ]; then   # 1.5s
      echo "RUN $i: SLOW pid=$pid still alive after 1.5s -- gdb dump"
      gdb -p "$pid" -batch \
        -ex "set pagination off" \
        -ex "thread apply all bt" \
        -ex "detach" \
        > "$DUMPDIR/run${i}_pid${pid}.gdb.txt" 2>&1
      echo "  dump saved: $DUMPDIR/run${i}_pid${pid}.gdb.txt"
      # keep waiting for it to actually finish, up to 10 more seconds
      extra=0
      while kill -0 "$pid" 2>/dev/null; do
        sleep 0.2
        extra=$((extra+1))
        if [ "$extra" -ge 50 ]; then
          echo "  STILL ALIVE after +10s -- genuine hang, leaving for manual inspection"
          break 2
        fi
      done
      echo "  eventually finished, log: $(cat "$LOG")"
      break
    fi
  done
  if [ "$waited" -lt 15 ]; then
    wait "$pid" 2>/dev/null
    rc=$?
    echo "RUN $i: rc=$rc out=[$(cat "$LOG")]"
  fi
done
echo "DONE $N runs"
