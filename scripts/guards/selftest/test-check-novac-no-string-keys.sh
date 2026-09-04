#!/bin/sh
# Самотест check-novac-no-string-keys.py — оба направления (норма 254):
# ловит нарушение И не краснеет на законном.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-no-string-keys.py"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-strkeys-selftest.$$"
mkdir -p "$T"
fails=0
CASES=0
ok() { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# 1. Законный: сканируемой директории нет — зелёный «судить нечего» со строкой ok: (№645).
python "$G" "$ROOT" "$T/absent" 2>/dev/null | grep -q 'ok:' && ok "нет novac/src — зелено, строка ok:" || bad "нет novac/src — нет строки ok: (№645)"

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
python "$G" "$ROOT" "$T/g2" >/dev/null 2>&1 && ok "законное дерево проходит" || bad "законное дерево покраснело"
python "$G" "$ROOT" "$T/g2" 2>/dev/null | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"

# 3. Нарушение: вне names/ 'Map[str,' — красный.
mkdir -p "$T/g3/sem"
cat > "$T/g3/sem/bad.nv" <<'EOF'
fn build() {
    let flat = Map[str, DeclId].new()
}
EOF
python "$G" "$ROOT" "$T/g3" >/dev/null 2>&1 && bad "Map[str, вне names/ прошёл" || ok "Map[str, вне names/ пойман"

# 4. Нарушение: вне names/ 'HashMap[str' — красный.
mkdir -p "$T/g4/mono"
cat > "$T/g4/mono/bad.nv" <<'EOF'
fn cache() {
    let seen = HashMap[str, MonoInst].new()
}
EOF
python "$G" "$ROOT" "$T/g4" >/dev/null 2>&1 && bad "HashMap[str вне names/ прошёл" || ok "HashMap[str вне names/ пойман"

# 5. Нарушение: вне names/ 'Map[string' — красный.
mkdir -p "$T/g5/emit"
cat > "$T/g5/emit/bad.nv" <<'EOF'
fn emit() {
    let names = Map[string, CName].new()
}
EOF
python "$G" "$ROOT" "$T/g5" >/dev/null 2>&1 && bad "Map[string вне names/ прошёл" || ok "Map[string вне names/ пойман"

# 6. Нарушение: внутри names/ 'Map[str,' без NamespaceId-компонента — красный (инвариант (а) К2).
mkdir -p "$T/g6/names"
cat > "$T/g6/names/flat.nv" <<'EOF'
fn resolve() {
    let flat = Map[str, Vec[DeclId]].new()
}
EOF
python "$G" "$ROOT" "$T/g6" >/dev/null 2>&1 && bad "плоский Map[str, внутри names/ прошёл" || ok "плоский Map[str, внутри names/ пойман"

# 7. Законный: внутри names/ NamespaceId на строке = компонент ключа — зелено.
mkdir -p "$T/g7/names"
cat > "$T/g7/names/nested.nv" <<'EOF'
fn resolve() {
    let by_ns = Map[NamespaceId, Map[str, DeclId]].new()
}
EOF
python "$G" "$ROOT" "$T/g7" >/dev/null 2>&1 && ok "вложенная форма с NamespaceId проходит" || bad "вложенная форма с NamespaceId покраснела"

# 8. Нарушение ВТОРОЙ половины правила (владелец 2026-08-16): ключ двери
# СИНТЕЗИРОВАН интерполяцией. Дверь `names` законна, поэтому первая
# половина тут молчит — красным обязана быть именно склейка.
mkdir -p "$T/g8/sem"
cat > "$T/g8/sem/fields.nv" <<'EOF'
fn add(owner int, fd FieldDef) {
    @names.put("${owner}.${fd.name}", 1)
}
EOF
python "$G" "$ROOT" "$T/g8" > "$T/o8" 2> "$T/e8" && bad "синтезированный ключ в put прошёл" || ok "синтезированный ключ в put пойман"
grep -q "sem/fields.nv" "$T/e8" && ok "файл-нарушитель синтеза назван" || bad "нарушитель синтеза не назван"

# 9. То же через find — вторая дверь.
mkdir -p "$T/g9/sem"
cat > "$T/g9/sem/look.nv" <<'EOF'
fn field_type(owner int, fname str) -> int {
    ro row = @names.find("${owner}.${fname}")
    row
}
EOF
python "$G" "$ROOT" "$T/g9" >/dev/null 2>&1 && bad "синтезированный ключ в find прошёл" || ok "синтезированный ключ в find пойман"

# 10. Законная форма: дверь берёт голое имя, второй ключ сравнивается целым
# числом при обходе цепочки — зелёный (иначе правило запрещало бы починку).
mkdir -p "$T/g10/sem"
cat > "$T/g10/sem/chain.nv" <<'EOF'
fn row_of(recv int, name str) -> int {
    mut r = @heads.find(name)
    while r >= 0 {
        ro fd = @rows[r]
        if fd.recv == recv { return r }
        r = fd.next
    }
    -1
}
EOF
python "$G" "$ROOT" "$T/g10" >/dev/null 2>&1 && ok "цепочка с целочисленным сравнением проходит" || bad "законная цепочка покраснела"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-no-string-keys ok: $CASES/$CASES"
    exit 0
fi
echo "test-check-novac-no-string-keys: FAIL ($fails)" >&2
exit 1
