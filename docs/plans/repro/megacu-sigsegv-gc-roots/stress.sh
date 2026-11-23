#!/bin/bash
# A/B stress: 8 concurrent workers per variant, incremental GC ON (default),
# NOVA_MAXPROCS=16. Logs exit codes; non-zero = crash/run-fail.
cd "$(dirname "$0")"
export NOVA_MAXPROCS=16
export NOVA_DIAG_SEGV=1
stress() {
  local exe="$1" tag="$2" id="$3" rounds="$4"
  for i in $(seq 1 "$rounds"); do
    ./"$exe" > /dev/null 2>> "err-$tag-$id.log"
    rc=$?
    echo "$rc" >> "rc-$tag-$id.log"
    if [ "$rc" != "0" ]; then
      echo "=== round $i rc=$rc ===" >> "crash-$tag-$id.log"
      tail -40 "err-$tag-$id.log" >> "crash-$tag-$id.log" 2>/dev/null
    fi
  done
}
for id in 1 2 3 4 5 6 7 8; do
  stress old.exe old "$id" 40 &
  stress new.exe new "$id" 40 &
done
wait
echo ALL-DONE
