#!/bin/bash
EXE=~/nova-p259/scratch259/vma_hold_main.exe
for maxfibers in 64 16384 262144; do
  NOVA_MAX_FIBERS=$maxfibers "$EXE" > /tmp/vma_out.log 2>&1 &
  pid=$!
  sleep 1.2
  if [ -r "/proc/$pid/maps" ]; then
    n=$(wc -l < "/proc/$pid/maps")
    echo "NOVA_MAX_FIBERS=$maxfibers pid=$pid VMA_count=$n"
  else
    echo "NOVA_MAX_FIBERS=$maxfibers pid=$pid: /proc maps not readable (process already exited?)"
  fi
  wait "$pid" 2>/dev/null
done
