#!/usr/bin/env bash
# Re-run the kept repro exe directly (ARMED M:N) with and without the
# NOVA_GC_STACK_SCAN_KB discriminator knob. Same binary — env-only difference.
set -u
EXE=$(find /tmp -name 'boehmret_slope' -type f -executable 2>/dev/null | head -1)
if [ -z "$EXE" ]; then echo "NO EXE FOUND"; exit 1; fi
echo "EXE=$EXE"
echo "############ BASELINE (whole-stack push, knob unset) ############"
for r in 1 2 3; do
  echo "--- baseline run $r ---"
  timeout 120 "$EXE" 2>&1 | grep -E '\[boehmret\]|slope|panic|abort|SIGSEGV' || echo "(no slope line / crashed)"
done
echo "############ TIGHT (NOVA_GC_STACK_SCAN_KB=64) ############"
for r in 1 2 3; do
  echo "--- tight run $r ---"
  NOVA_GC_STACK_SCAN_KB=64 timeout 120 "$EXE" 2>&1 | grep -E '\[boehmret\]|slope|panic|abort|SIGSEGV' || echo "(no slope line / crashed)"
done
