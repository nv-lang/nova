#!/bin/sh
# Самотест check-novac-no-string-keys.sh — оба направления (норма 254):
# ловит нарушение И не краснеет на законном.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-no-string-keys.sh"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-strkeys-selftest.$$"
mkdir -p "$T"
fails=0

ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# 1. Законный: сканируемой директории нет — зелёный «судить нечего» со строкой ok: (№645).
sh "$G" "$ROOT" "$T/absent" 2>/dev/null | grep -q 'ok:' && ok "нет novac/src — зелено, строка ok:" || bad "нет novac/src — нет строки ok: (№645)"

# 2. Законный: вне names/ DeclId-ключи, внутри names/ обе законные формы — зелено.
mkdir -p "$T/g2/names" "$T/g2/sem"
cat > "$T/g2/sem/tables.nv" <<'EOF'
fn build() {
    let methods = Map[DeclId, Vec[DeclId]].new()
}
EOF
cat > "$T/g2/names/resolve.nv" <<'EOF'
type NsKey = (NamespaceId, str)
fn resolve() {
    let table = Map[(NamespaceId, str), DeclId].new()
    let alt = Map[NsKey, DeclId].new()
}
EOF
sh "$G" "$ROOT" "$T/g2" >/dev/null 2>&1 && ok "законное дерево проходит" || bad "законное дерево покраснело"
sh "$G" "$ROOT" "$T/g2" 2>/dev/null | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"

# 3. Нарушение: вне names/ 'Map[str,' — красный.
mkdir -p "$T/g3/sem"
cat > "$T/g3/sem/bad.nv" <<'EOF'
fn build() {
    let flat = Map[str, DeclId].new()
}
EOF
sh "$G" "$ROOT" "$T/g3" >/dev/null 2>&1 && bad "Map[str, вне names/ прошёл" || ok "Map[str, вне names/ пойман"

# 4. Нарушение: вне names/ 'HashMap[str' — красный.
mkdir -p "$T/g4/mono"
cat > "$T/g4/mono/bad.nv" <<'EOF'
fn cache() {
    let seen = HashMap[str, MonoInst].new()
}
EOF
sh "$G" "$ROOT" "$T/g4" >/dev/null 2>&1 && bad "HashMap[str вне names/ прошёл" || ok "HashMap[str вне names/ пойман"

# 5. Нарушение: вне names/ 'Map[string' — красный.
mkdir -p "$T/g5/emit"
cat > "$T/g5/emit/bad.nv" <<'EOF'
fn emit() {
    let names = Map[string, CName].new()
}
EOF
sh "$G" "$ROOT" "$T/g5" >/dev/null 2>&1 && bad "Map[string вне names/ прошёл" || ok "Map[string вне names/ пойман"

# 6. Нарушение: внутри names/ 'Map[str,' без NamespaceId-компонента — красный (инвариант (а) К2).
mkdir -p "$T/g6/names"
cat > "$T/g6/names/flat.nv" <<'EOF'
fn resolve() {
    let flat = Map[str, Vec[DeclId]].new()
}
EOF
sh "$G" "$ROOT" "$T/g6" >/dev/null 2>&1 && bad "плоский Map[str, внутри names/ прошёл" || ok "плоский Map[str, внутри names/ пойман"

# 7. Законный: внутри names/ NamespaceId на строке = компонент ключа — зелено.
mkdir -p "$T/g7/names"
cat > "$T/g7/names/nested.nv" <<'EOF'
fn resolve() {
    let by_ns = Map[NamespaceId, Map[str, DeclId]].new()
}
EOF
sh "$G" "$ROOT" "$T/g7" >/dev/null 2>&1 && ok "вложенная форма с NamespaceId проходит" || bad "вложенная форма с NamespaceId покраснела"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-no-string-keys ok: 8/8"
    exit 0
fi
echo "test-check-novac-no-string-keys: FAIL ($fails)" >&2
exit 1
