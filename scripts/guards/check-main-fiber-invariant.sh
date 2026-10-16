#!/usr/bin/env bash
# Plan 279 Э0: keep the main-fiber invariant connected to D493/D446 and both
# runtime doors. This is deliberately narrower than invariant-discipline.
set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || exit 1

VIOL=0
require() {
    file="$1"
    text="$2"
    if [ ! -f "$file" ] || ! grep -Fq "$text" "$file"; then
        echo "check-main-fiber-invariant: MISSING — $file: $text" >&2
        VIOL=$((VIOL + 1))
    fi
}

require spec/decisions/04-effects.md '## D493. Маркерные эффекты безопасности в строках'
require spec/decisions/04-effects.md 'D446 (safety функциональных'
require spec/decisions/06-concurrency.md '**Инвариант главного файбера (текущее устройство рантайма).**'
require spec/decisions/06-concurrency.md 'D493'
require spec/decisions/06-concurrency.md '[279](../../docs/plans/279-mn-safety-via-effects.md)'
require compiler-codegen/nova_rt/runtime.c '[INV-GUARD: check-main-fiber-invariant.sh] The root main fiber must'
require compiler-codegen/nova_rt/fibers.h '[INV-GUARD: check-main-fiber-invariant.sh] The root main fiber is resumed'

if [ "$VIOL" -gt 0 ]; then
    echo "check-main-fiber-invariant: FAIL — $VIOL missing invariant link(s)" >&2
    exit 1
fi
echo "check-main-fiber-invariant ok: D493/D446 and both runtime doors are linked"
