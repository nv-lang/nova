#!/bin/sh
# .p259/runcorpus.sh N [REP] — один прогон подкорпуса p259c_n<N>, с профилем.
# Агрегирует ТОЛЬКО первую работу (слитый folder-module CU): маркеры __PERF__
# первого блока (блок = от `parse` до `codegen`) и первую строку results-json.
set -u
N="$1"; REP="${2:-1}"
ROOT="/d/Sources/nv-lang/nova-p259"
. "$ROOT/.p259/env.sh"
export TMPDIR="$ROOT/.p259/tmp"; mkdir -p "$TMPDIR"
export TEMP="D:/Sources/nv-lang/nova-p259/.p259/tmp"; export TMP="$TEMP"
[ -d "$ROOT/spec_tests/p259c_n$N" ] || sh "$ROOT/.p259/mkcorpus.sh" "$N" >/dev/null
E="$ROOT/.p259/e_n${N}_${REP}.txt"
R="$ROOT/.p259/res_n${N}_${REP}.json"
LOAD=$(wmic cpu get loadpercentage //value 2>/dev/null | tr -d '\r' | sed -n 's/^LoadPercentage=//p' | head -1)
S=$(date +%s)
NOVA_PERF_TIMER=1 "$NOVA" test --positive --compile-error --jobs 1 \
    --keep-artifacts --results-file "$R" "$ROOT/spec_tests/p259c_n$N" \
    > "$ROOT/.p259/o_n${N}_${REP}.txt" 2> "$E"
RC=$?
WALL=$(( $(date +%s) - S ))
LOAD2=$(wmic cpu get loadpercentage //value 2>/dev/null | tr -d '\r' | sed -n 's/^LoadPercentage=//p' | head -1)
FE=$(awk '/^__PERF__ /{if($2=="parse"){c++}; if(c<=1)s+=$3} END{printf "%d", s/1000000}' "$E")
CG=$(grep '^__PERF__ codegen ' "$E" | head -1 | awk '{printf "%d", $3/1000000}')
IR=$(grep '^__PERF__ imports-resolve ' "$E" | head -1 | awk '{printf "%d", $3/1000000}')
TC=$(grep '^__PERF__ type-check ' "$E" | head -1 | awk '{printf "%d", $3/1000000}')
CM=$(head -1 "$R" | grep -o '"compile_ms":[0-9]*' | cut -d: -f2)
RM=$(head -1 "$R" | grep -o '"run_ms":[0-9]*' | cut -d: -f2)
CSIZE=$(ls -l "$ROOT/spec_tests/p259c_n$N"/a__p259_imports.c 2>/dev/null | awk '{print $5+0}')
CM=${CM:-0}; RM=${RM:-0}; CSIZE=${CSIZE:-0}
CC=$(( CM - FE ))
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$N" "$REP" "$WALL" "$FE" "$IR" "$TC" "$CG" "$CM" "$CC" "$RM" "$CSIZE" "${LOAD:-?}/${LOAD2:-?}" \
    >> "$ROOT/.p259/scaling.tsv"
echo "N=$N rep=$REP RC=$RC wall=${WALL}s | nova(fe+cg)=${FE}ms [imports=${IR} typecheck=${TC} codegen=${CG}] | compile_ms=${CM} => cc+link=${CC}ms | run=${RM}ms | .c=${CSIZE}B | cpu ${LOAD}%->${LOAD2}%"
