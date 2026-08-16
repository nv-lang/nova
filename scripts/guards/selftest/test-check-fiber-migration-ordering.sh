#!/usr/bin/env bash
# selftest для check-fiber-migration-ordering.sh (№443).
# Доказывает: страж зелёный на настоящем дереве и КРАСНЫЙ на каждом из трёх
# ослаблений двери — store без RELEASE, cas без ACQ_REL, прямой __atomic_store
# в _nova_fiber_state мимо accessor'ов.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-fiber-migration-ordering.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

# Копия двух заголовков в поддельный корень; мутируем копию, не дерево.
setup() {
    rm -rf "$TMP/r"; mkdir -p "$TMP/r/compiler-codegen/nova_rt"
    cp "$ROOT/compiler-codegen/nova_rt/sync.h"   "$TMP/r/compiler-codegen/nova_rt/"
    cp "$ROOT/compiler-codegen/nova_rt/fibers.h" "$TMP/r/compiler-codegen/nova_rt/"
    : > "$TMP/r/compiler-codegen/nova_rt/runtime.c"
}

echo "== проходит =="
out=$(bash "$G" "$ROOT" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'ok: уход RELEASE'; then ok "настоящее дерево — зелёный"; else bad "ложный красный на дереве: $out"; fi

echo "== ловит =="
# 1. store ослаблен до RELAXED
setup
python - "$TMP/r/compiler-codegen/nova_rt/sync.h" <<'PY'
import io,sys,re
p=sys.argv[1]; t=io.open(p,encoding="utf-8").read()
m=re.search(r"static inline void nova_aint_store\(volatile nova_atomic_int\* p, int32_t v\) \{.*?\n\}", t, re.S)
assert m
t=t[:m.start()]+m.group(0).replace("__ATOMIC_RELEASE","__ATOMIC_RELAXED")+t[m.end():]
io.open(p,"w",encoding="utf-8",newline="\n").write(t)
PY
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'без RELEASE'; then ok "store RELAXED — красный"; else bad "пропустил store без RELEASE (rc=$rc): $out"; fi

# 2. cas ослаблен до ACQUIRE
setup
python - "$TMP/r/compiler-codegen/nova_rt/sync.h" <<'PY'
import io,sys,re
p=sys.argv[1]; t=io.open(p,encoding="utf-8").read()
m=re.search(r"static inline bool nova_aint_cas\(.*?\n\}", t, re.S)
assert m
t=t[:m.start()]+m.group(0).replace("__ATOMIC_ACQ_REL","__ATOMIC_ACQUIRE")+t[m.end():]
io.open(p,"w",encoding="utf-8",newline="\n").write(t)
PY
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'без ACQ_REL'; then ok "cas ACQUIRE — красный"; else bad "пропустил cas без ACQ_REL (rc=$rc): $out"; fi

# 3. обход дверей: прямой store в поле
setup
printf '\nstatic inline void sneaky(NovaSpawnCtxBase* b) { __atomic_store_n(&b->_nova_fiber_state, 0, __ATOMIC_RELAXED); }\n' >> "$TMP/r/compiler-codegen/nova_rt/fibers.h"
out=$(bash "$G" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'мимо дверей'; then ok "прямой store в _nova_fiber_state — красный, место названо"; else bad "пропустил обход дверей (rc=$rc): $out"; fi

# 4. нет заголовков — красный, не «судить нечего»
out=$(bash "$G" "$TMP/nowhere" 2>&1); rc=$?
if [ "$rc" -ne 0 ]; then ok "нет заголовков — красный"; else bad "нет заголовков, а страж зелёный"; fi

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || { echo "selftest check-fiber-migration-ordering: ПРОВАЛ" >&2; exit 1; }
echo "selftest check-fiber-migration-ordering: OK (зелёный на дереве / красный на store RELAXED, cas ACQUIRE, обходе дверей, отсутствии заголовков)"
exit 0
