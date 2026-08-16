#!/bin/bash
PID=$1
for t in /proc/$PID/task/*; do
  tid=$(basename "$t")
  echo "=== tid $tid ==="
  cat "$t/comm" 2>/dev/null
  cat "$t/wchan" 2>/dev/null; echo
  cat "$t/stack" 2>/dev/null
  echo
done
