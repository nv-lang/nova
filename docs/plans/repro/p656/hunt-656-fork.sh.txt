#!/usr/bin/env bash
# №656, разрез вилки: инструментированный рантайм (счётчики spawn_dispatched /
# first_run + обход слепоты сторожа под NOVA_DIAG_656). На зависании сторож
# на 10-й секунде печатает [656]-строку в лог — она и режет вилку.
set -u
cd ~/nova-656 || exit 2
cp /mnt/d/Sources/nv-lang/nova/compiler-codegen/nova_rt/fibers.h  compiler-codegen/nova_rt/fibers.h
cp /mnt/d/Sources/nv-lang/nova/compiler-codegen/nova_rt/runtime.c compiler-codegen/nova_rt/runtime.c
cp /mnt/d/Sources/nv-lang/nova/compiler-codegen/nova_rt/driver.c  compiler-codegen/nova_rt/driver.c
grep -q '_g656_spawn_disp' compiler-codegen/nova_rt/runtime.c || { echo "копия без счётчиков"; exit 2; }
rm -rf target
L=~/p446-logs; mkdir -p "$L"
NOVA=./nova-cli/target/release/nova
FIX=spec_tests/conformance/standalone/presume_446_stress.nv

echo "[fork] пересборка рантайма"
"$NOVA" test "$FIX" --keep-artifacts > "$L/fork-rebuild.log" 2>&1 || { tail -6 "$L/fork-rebuild.log"; exit 2; }
T=$(find /tmp/nova_tests* -name 'presume_446_stress' 2>/dev/null | head -1)
[ -n "$T" ] || { echo "exe нет"; exit 2; }
cp "$T" "$L/presume.fork.bin"
EXE="$L/presume.fork.bin"

export NOVA_MAXPROCS=8
export NOVA_MAX_FIBERS=4000
export NOVA_WATCHDOG_DUMP_SECS=10
export NOVA_DIAG_656=1

for i in $(seq 1 60); do
  s=$(date +%s)
  timeout 90 taskset -c 0,1 "$EXE" > "$L/fork_${i}.log" 2>&1
  rc=$?
  e=$(( $(date +%s) - s ))
  echo "[fork] run $i: rc=$rc ${e}s"
  if [ "$rc" -ne 0 ]; then
    echo "[fork] === HIT run $i — строки [656] и дамп ==="
    grep -E '^\[656\]|NOVA_RUNTIME_DUMP|supervised.*remote|w\.[0-9]+\.deque' "$L/fork_${i}.log" | head -15
    exit 3
  fi
done
echo "LOOP-DONE: не поймано за 60"
