#!/bin/bash
# Самотест замка «один гейт на дерево» — scripts/tools/gate-lock.sh (реестр 221.1 №1389).
#
# Клетки:
#   1 — держатель жив: второй прогон ОТКАЗАН (код 1) и называет PID держателя;
#   2 — держатель мёртв: протухший замок снят вслух и взят;
#   3 — вложенный запуск (потомок держателя, NOVA_GATE_LOCK_HELD унаследован) — не отказан;
#   4 — снятие трогает только СВОЙ замок: чужой owner остаётся на месте;
#   5 — выход держателя снимает замок (EXIT-ловушка), следующий берёт без «протухания»;
#   6 — проводка: оба гейта берут замок ДО суточного предела / ловушки вердикта.
#
# Замок живёт в подменённом каталоге (NOVA_GATE_LOCK_DIR) — настоящий `.git` не трогается.
# Запуск: bash scripts/guards/selftest/test-gate-lock.sh     Выход: 0 — цел, 1 — сломан.

set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LIB="$ROOT/scripts/tools/gate-lock.sh"
TMP="$(mktemp -d)"
HOLDER=""
trap '[ -n "$HOLDER" ] && kill "$HOLDER" 2>/dev/null; rm -rf "$TMP"' EXIT

CASES=0
FAILS=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); FAILS=$((FAILS+1)); echo "  FAIL: $1"; }

[ -f "$LIB" ] || { echo "test-gate-lock: нет $LIB"; exit 1; }
export NOVA_GATE_LOCK_DIR="$TMP/gitdir"
mkdir -p "$NOVA_GATE_LOCK_DIR"
LOCK="$NOVA_GATE_LOCK_DIR/nova-gate.lock"
unset NOVA_GATE_LOCK_HELD

# Держатель: берёт замок и живёт, пока его не снимут. Из его окружения потомок
# наследует NOVA_GATE_LOCK_HELD — клетка 3 запускается ИЗ него через файл-команду.
cat > "$TMP/holder.sh" <<EOF
. "$LIB"
gate_lock_acquire gate "$TMP" push
trap gate_lock_release EXIT
if [ -f "$TMP/run-child" ]; then
    bash -c '. "$LIB"; gate_lock_acquire gate "$TMP" loop; echo CHILD_RC=0' > "$TMP/child.log" 2>&1
fi
while [ ! -f "$TMP/stop" ]; do sleep 0.2; done
EOF

start_holder() {
    rm -f "$TMP/stop"
    bash "$TMP/holder.sh" > "$TMP/holder.log" 2>&1 &
    HOLDER=$!
    for _i in $(seq 1 50); do [ -s "$LOCK/owner" ] && return 0; sleep 0.2; done
    return 1
}
stop_holder() {
    touch "$TMP/stop"; wait "$HOLDER" 2>/dev/null; HOLDER=""
}

# --- 1: держатель жив -> второй отказан и назван ---------------------------
if ! start_holder; then
    bad "1: держатель не взял замок за 10с ($(cat "$TMP/holder.log"))"
else
    _hp=$(sed -n 's/^PID=//p' "$LOCK/owner")
    bash -c ". '$LIB'; gate_lock_acquire gate '$TMP' push; echo TAKEN" > "$TMP/second.log" 2>&1
    _rc=$?
    if [ "$_rc" -ne 1 ]; then
        bad "1: второй прогон при живом держателе не отказан (rc=$_rc): $(tr '\n' ' ' < "$TMP/second.log")"
    elif grep -q TAKEN "$TMP/second.log"; then
        bad "1: второй прогон взял замок при живом держателе"
    elif ! grep -q "PID=$_hp" "$TMP/second.log"; then
        bad "1: отказ не называет держателя PID=$_hp: $(tr '\n' ' ' < "$TMP/second.log")"
    else
        ok "1: живой держатель PID=$_hp — второй прогон отказан и назвал его"
    fi
    stop_holder
fi

# --- 2: держатель мёртв -> протухший замок снят и взят ----------------------
rm -rf "$LOCK"
mkdir "$LOCK"
bash -c 'exit 0' & _dead=$!; wait "$_dead"
printf 'PID=%s\nSTART=old\nTIER=push\n' "$_dead" > "$LOCK/owner"
bash -c ". '$LIB'; gate_lock_acquire gate '$TMP' push; echo TAKEN; sed -n 's/^PID=//p' '$LOCK/owner'; echo SELF=\$\$" > "$TMP/stale.log" 2>&1
_rc=$?
if [ "$_rc" -ne 0 ] || ! grep -q TAKEN "$TMP/stale.log"; then
    bad "2: протухший замок (мёртвый PID $_dead) не взят (rc=$_rc): $(tr '\n' ' ' < "$TMP/stale.log")"
elif ! grep -q 'протух' "$TMP/stale.log"; then
    bad "2: протухший замок снят МОЛЧА — снятие обязано быть названо"
else
    ok "2: мёртвый держатель PID $_dead — замок снят вслух и перехвачен"
fi
rm -rf "$LOCK"

# --- 3: вложенный запуск потомка держателя не отказан -----------------------
touch "$TMP/run-child"
if ! start_holder; then
    bad "3: держатель не взял замок"
else
    for _i in $(seq 1 50); do [ -s "$TMP/child.log" ] && break; sleep 0.2; done
    if grep -q 'CHILD_RC=0' "$TMP/child.log" && grep -q 'вложенный' "$TMP/child.log"; then
        ok "3: потомок держателя прошёл как вложенный запуск"
    else
        bad "3: потомок держателя отказан или не опознан вложенным: $(tr '\n' ' ' < "$TMP/child.log")"
    fi
    stop_holder
fi
rm -f "$TMP/run-child"

# --- 4 и 5: снятие — только своё; выход держателя снимает замок -------------
if [ -d "$LOCK" ]; then
    bad "5: держатель вышел, а замок остался — следующий прогон увидел бы протухание, а не свободу"
else
    ok "5: выход держателя снял замок (EXIT-ловушка)"
fi
mkdir "$LOCK"; printf 'PID=999999\n' > "$LOCK/owner"
bash -c ". '$LIB'; GATE_LOCK_PATH='$LOCK'; gate_lock_release" 2>&1
if [ -f "$LOCK/owner" ]; then
    ok "4: чужой замок (PID 999999) снятием не тронут"
else
    bad "4: gate_lock_release снял ЧУЖОЙ замок"
fi
rm -rf "$LOCK"

# --- 6: проводка в обоих гейтах ---------------------------------------------
_g="$ROOT/scripts/gate.sh"
_n="$ROOT/scripts/gate-novac.sh"
_la=$(grep -n 'gate_lock_acquire gate ' "$_g" | head -1 | cut -d: -f1)
_lb=$(grep -n 'check-gate-daily-budget.py' "$_g" | grep -v '^[0-9]*:\s*#' | head -1 | cut -d: -f1)
if [ -n "$_la" ] && [ -n "$_lb" ] && [ "$_la" -lt "$_lb" ]; then
    ok "6a: gate.sh берёт замок (:$_la) до суточного предела (:$_lb)"
else
    bad "6a: gate.sh не берёт замок до суточного предела (замок :${_la:-нет}, предел :${_lb:-нет})"
fi
_na=$(grep -n 'gate_lock_acquire gate-novac ' "$_n" | head -1 | cut -d: -f1)
_nt=$(grep -n '^trap _novac_write_verdict EXIT' "$_n" | head -1 | cut -d: -f1)
_nr=$(grep -c '^\s*gate_lock_release' "$_n")
if [ -n "$_na" ] && [ -n "$_nt" ] && [ "$_na" -lt "$_nt" ] && [ "$_nr" -ge 1 ]; then
    ok "6b: gate-novac.sh берёт замок (:$_na) до ловушки вердикта (:$_nt) и снимает его в ней"
else
    bad "6b: gate-novac.sh: замок :${_na:-нет}, ловушка :${_nt:-нет}, снятий $_nr"
fi

echo "test-gate-lock: $((CASES-FAILS))/$CASES"
[ "$FAILS" -eq 0 ]
