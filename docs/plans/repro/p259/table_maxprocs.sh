#!/bin/bash
EXE=~/nova-p259/scratch259/arena_budget_main.exe
for maxprocs in 1 4 16 "" ; do
  label="$maxprocs"
  if [ -z "$maxprocs" ]; then label="unset(auto=nproc)"; fi
  vals=()
  for i in $(seq 1 10); do
    if [ -z "$maxprocs" ]; then
      out=$("$EXE")
    else
      out=$(NOVA_MAXPROCS=$maxprocs "$EXE")
    fi
    ms=$(echo "$out" | sed -n 's/.*elapsed_ms=\([0-9]*\).*/\1/p')
    vals+=("$ms")
  done
  echo "NOVA_MAXPROCS=$label -> elapsed_ms samples: ${vals[*]}"
done
