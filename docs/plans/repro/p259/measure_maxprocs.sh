#!/bin/bash
EXE=~/nova-p259/scratch259/arena_budget_main.exe
for maxprocs in 1 4 16 32; do
  echo "== NOVA_MAXPROCS=$maxprocs =="
  for i in 1 2 3 4 5; do
    NOVA_MAXPROCS=$maxprocs "$EXE"
  done
done
