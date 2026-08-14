#!/bin/sh
# Самотест check-novac-no-global-state.sh — оба направления (норма 254):
# ловит нарушение И не краснеет на законном.
export LC_ALL=C
G="$(cd "$(dirname "$0")/.." && pwd)/check-novac-no-global-state.sh"
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
T="${TMPDIR:-/tmp}/novac-globals-selftest.$$"
mkdir -p "$T"
fails=0

ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# 1. Законный: сканируемой директории нет — зелёный «судить нечего» со строкой ok: (№645).
sh "$G" "$ROOT" "$T/absent/src" 2>/dev/null | grep -q 'ok:' && ok "нет novac/src — зелено, строка ok:" || bad "нет novac/src — нет строки ok: (№645)"

# 2. Законный: mut только внутри fn-тела (с отступом), top-level let — зелено.
mkdir -p "$T/g2/src"
cat > "$T/g2/src/phase.nv" <<'EOF'
let VERSION = "0.1"
fn run() {
    mut acc = 0
    acc = acc + 1
}
EOF
sh "$G" "$ROOT" "$T/g2/src" >/dev/null 2>&1 && ok "локальный mut проходит" || bad "локальный mut покраснел"
sh "$G" "$ROOT" "$T/g2/src" 2>/dev/null | grep -q 'ok:' && ok "печатает строку ok:" || bad "нет строки ok: (№645)"

# 3. Нарушение: top-level 'mut ' — красный.
mkdir -p "$T/g3/src"
cat > "$T/g3/src/phase.nv" <<'EOF'
mut counter = 0
fn run() {}
EOF
sh "$G" "$ROOT" "$T/g3/src" >/dev/null 2>&1 && bad "top-level mut прошёл" || ok "top-level mut пойман"

# 4. Нарушение: top-level 'export mut ' — красный.
mkdir -p "$T/g4/src"
cat > "$T/g4/src/phase.nv" <<'EOF'
export mut cache: Map[DeclId, int] = Map[DeclId, int].new()
EOF
sh "$G" "$ROOT" "$T/g4/src" >/dev/null 2>&1 && bad "export mut прошёл" || ok "export mut пойман"

# 5. Нарушение: подстрока 'static mut' (даже с отступом) — красная.
mkdir -p "$T/g5/src"
cat > "$T/g5/src/phase.nv" <<'EOF'
fn run() {
    static mut FOO: int = 0
}
EOF
sh "$G" "$ROOT" "$T/g5/src" >/dev/null 2>&1 && bad "static mut прошёл" || ok "static mut пойман"

# 6. Законный: top-level mut с именем из GLOBALS.allow — зелено (write-once исключение).
mkdir -p "$T/g6/src"
cat > "$T/g6/src/phase.nv" <<'EOF'
mut interner = Interner.new()
fn run() {}
EOF
cat > "$T/g6/GLOBALS.allow" <<'EOF'
# write-once
interner
EOF
sh "$G" "$ROOT" "$T/g6/src" >/dev/null 2>&1 && ok "имя из GLOBALS.allow проходит" || bad "имя из GLOBALS.allow покраснело"

# 7. Нарушение: GLOBALS.allow есть, но имя другое — красный (файл не индульгенция).
mkdir -p "$T/g7/src"
cat > "$T/g7/src/phase.nv" <<'EOF'
mut counter = 0
EOF
cat > "$T/g7/GLOBALS.allow" <<'EOF'
interner
EOF
sh "$G" "$ROOT" "$T/g7/src" >/dev/null 2>&1 && bad "чужое имя при непустом allow прошло" || ok "чужое имя при непустом allow поймано"

rm -rf "$T"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-no-global-state ok: 8/8"
    exit 0
fi
echo "test-check-novac-no-global-state: FAIL ($fails)" >&2
exit 1
