#!/usr/bin/env bash
# Селфтест scripts/guards/check-registry-past-tense-status.py (реестр 221.1 №452).
#
# Страж без селфтеста не работает — правило владельца 2026-07-27, и
# check-guard-wiring его энфорсит. Случаи живут в самом страже (`--selftest`):
# один источник вместо двух расходящихся копий. Здесь вдобавок проверяется,
# что на ЖИВОМ реестре страж зелёный — то есть база сходится с фактом.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-registry-past-tense-status.py"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-registry-past-tense-status =="

if out=$(python "$G" --selftest 2>&1); then
    echo "$out" | sed 's/^/  /'
    ok "случаи стража отработали (проза без поля краснит, проза с полем нет)"
else
    echo "$out" | sed 's/^/  /' >&2
    bad "случаи стража не сошлись"
fi

if python "$G" "$ROOT" >/dev/null 2>&1; then
    ok "живой реестр сходится с базой"
else
    bad "живой реестр разошёлся с базой — опусти базу тем же коммитом либо объяви поле"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-registry-past-tense-status: ok"
    exit 0
fi
echo "селфтест check-registry-past-tense-status: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
