#!/bin/bash
# Long amplification series for the timer early-wake defect.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"
rm -f rc-*.log fail-*.log hits-*.log
WORKERS=${1:-3}
ROUNDS=${2:-40}
for id in $(seq 1 "$WORKERS"); do
  (
    for i in $(seq 1 "$ROUNDS"); do
      NOVA_GC_INCREMENTAL=1 NOVA_MAXPROCS=16 ./diag2.exe > "out-$id-$i.log" 2>&1
      rc=$?
      echo "$rc" >> "rc-$id.log"
      if [ "$rc" != "0" ] || grep -q "FAIL" "out-$id-$i.log"; then
        cp "out-$id-$i.log" "fail-$id-$i.log"
        echo "proc $id round $i rc=$rc" >> "fail-index.txt"
      fi
      grep "DIAG-DUR" "out-$id-$i.log" >> "hits-$id.log" 2>/dev/null
      rm -f "out-$id-$i.log"
    done
  ) &
done
wait
echo "rc summary:"; cat rc-*.log | sort | uniq -c
echo "anomaly hits:"; cat hits-*.log 2>/dev/null | wc -l
echo "fail index:"; cat fail-index.txt 2>/dev/null
echo ALL-DONE
