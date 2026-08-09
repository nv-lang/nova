#!/bin/bash
# Plan 259: parallel-load stress -- launch BATCH instances of the fixture
# concurrently (simulating machine load / concurrent process creation,
# which is when the original one-off hang was observed), repeated for
# N total runs. Each batch gets up to 8s to fully finish; any survivor
# after that is counted as a hang and left alive for inspection.
EXE=~/nova-p259/scratch259/arena_budget_main.exe
OUTDIR=~/nova-p259/scratch259/par_logs
mkdir -p "$OUTDIR"
rm -f "$OUTDIR"/*.log
N="${1:-200}"
BATCH="${2:-8}"
export NOVA_MAXPROCS=16
total=0
hangs=0
batch_num=0
while [ "$total" -lt "$N" ]; do
  batch_num=$((batch_num+1))
  pids=()
  this_batch=$BATCH
  if [ $((total+this_batch)) -gt "$N" ]; then
    this_batch=$((N-total))
  fi
  for ((k=0;k<this_batch;k++)); do
    "$EXE" > "$OUTDIR/b${batch_num}_${k}.log" 2>&1 &
    pids+=($!)
  done
  waited=0
  while :; do
    alive=0
    for p in "${pids[@]}"; do
      kill -0 "$p" 2>/dev/null && alive=$((alive+1))
    done
    if [ "$alive" -eq 0 ]; then break; fi
    sleep 0.2
    waited=$((waited+1))
    if [ "$waited" -ge 40 ]; then   # 8s
      echo "BATCH $batch_num: $alive/$this_batch still alive after 8s -- HANG"
      for p in "${pids[@]}"; do
        if kill -0 "$p" 2>/dev/null; then
          echo "  hung pid=$p"
          echo "$p" >> ~/nova-p259/scratch259/hung_pids_parallel.txt
          hangs=$((hangs+1))
        fi
      done
      break
    fi
  done
  for p in "${pids[@]}"; do
    if kill -0 "$p" 2>/dev/null; then :; else wait "$p" 2>/dev/null; fi
  done
  total=$((total+this_batch))
  echo "progress: total=$total/$N hangs_so_far=$hangs"
done
echo "SUMMARY: N=$N BATCH=$BATCH HANGS=$hangs"
grep -L "elapsed_ms=" "$OUTDIR"/*.log 2>/dev/null | while read -r f; do echo "NO-OUTPUT: $f"; cat "$f"; done
