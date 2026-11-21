#!/bin/sh
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH= cd -- "$HERE/../../../../../.." && pwd)
: "${NOVAC_BIN:?set NOVAC_BIN to the Carina binary under test}"
unset NOVAC_SELF_PATH
export NOVA_CACHE=0
cd "$ROOT"
for name in nested intrinsic forward rebind capture stored variant receiver; do
    printf '\nprobe: %s\n' "$name"
    if output=$("$NOVAC_BIN" check "$HERE/$name/main.nv" 2>&1); then
        "$NOVAC_BIN" emit --target-os windows "$HERE/$name/main.nv" >/dev/null
    else
        rc=$?
        printf '%s\n' "$output"
        [ "$name" = stored ] && [ "$rc" -eq 1 ] || exit 1
        printf '%s\n' "$output" | grep -q 'E_NOVAC_SUBSET' || exit 1
    fi
done
