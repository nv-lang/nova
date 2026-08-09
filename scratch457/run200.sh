#!/bin/bash
# Runs repro457e_main.exe N times, counting pass/fail/hang.
N="${1:-200}"
START="${2:-1}"
pass=0
fail=0
hang=0
for i in $(seq "$START" "$N"); do
  out=$(timeout 8 ./scratch457/repro457e_main.exe 2>&1)
  rc=$?
  if [ $rc -eq 124 ]; then
    hang=$((hang+1))
    echo "RUN $i: HANG (external 8s timeout hit) partial_out=[$out]"
  elif echo "$out" | grep -q "r=0" && echo "$out" | grep -q "A read Err" && echo "$out" | grep -q "B read Err"; then
    pass=$((pass+1))
  else
    fail=$((fail+1))
    echo "RUN $i: UNEXPECTED rc=$rc out=$out"
  fi
done
echo "SUMMARY: PASS=$pass FAIL=$fail HANG=$hang OF $((N-START+1))"
