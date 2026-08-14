#!/bin/sh
# Самотест check-novac-atomics-door.sh — оба направления (норма 254):
# ловит нарушение И не краснеет на законном.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-atomics-door.sh"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-atomics-door-selftest.$$"
mkdir -p "$T/src/atomics" "$T/src/sched"
fails=0

ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# 1. Законный: примитивы внутри двери atomics/ + чистый файл снаружи — зелено.
cat > "$T/src/atomics/door.nv" <<'EOF'
// door: wraps __atomic_load_n, thread_local storage, nova_atomic_store
fn load(p int) int { return 0 }
EOF
cat > "$T/src/sched/clean.nv" <<'EOF'
fn run() { let x = atomics.load(0) }
EOF
sh "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && ok "дверь с примитивами + чистый сосед проходят" || bad "законная дверь покраснела"
sh "$G" "$ROOT" "$T/src" 2>/dev/null | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"

# 2. Нарушение: '__atomic_' вне двери — красный.
printf 'fn f() { emit("__atomic_fetch_add(p, 1)") }\n' > "$T/src/sched/raw.nv"
sh "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && bad "__atomic_ вне двери прошёл" || ok "__atomic_ вне двери пойман"
rm -f "$T/src/sched/raw.nv"

# 3. Нарушение: 'thread_local' вне двери — красный.
printf 'fn g() { emit("static thread_local int t;") }\n' > "$T/src/sched/tls.nv"
sh "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && bad "thread_local вне двери прошёл" || ok "thread_local вне двери пойман"
rm -f "$T/src/sched/tls.nv"

# 4. Нарушение: прямой 'nova_atomic_' вне двери — красный.
printf 'fn h() { nova_atomic_store(p, 1) }\n' > "$T/src/sched/direct.nv"
sh "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && bad "nova_atomic_ вне двери прошёл" || ok "nova_atomic_ вне двери пойман"
rm -f "$T/src/sched/direct.nv"

# 5. Законный: директории нет — зелено с честной строкой «судить нечего».
sh "$G" "$ROOT" "$T/nope" 2>/dev/null | grep -q 'ok: судить нечего' && ok "нет директории — судить нечего" || bad "нет директории — не зелено или молчит"

# 6. Нарушение во вложенной папке вне двери ловится (скан рекурсивный).
mkdir -p "$T/src/sched/deep"
printf 'fn k() { nova_atomic_load(q) }\n' > "$T/src/sched/deep/nested.nv"
sh "$G" "$ROOT" "$T/src" >/dev/null 2>&1 && bad "вложенное нарушение прошло" || ok "вложенное нарушение поймано"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-atomics-door ok: 7/7"
    exit 0
fi
echo "test-check-novac-atomics-door: FAIL ($fails)" >&2
exit 1
