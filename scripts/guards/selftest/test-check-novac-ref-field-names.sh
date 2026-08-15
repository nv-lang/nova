#!/bin/sh
# Самотест check-novac-ref-field-names.sh (П16: обязан доказать, что ловит).
#
# ПОДЛОЖКА. Шов $2 — override сканируемой директории: каждый случай это своя
# крошечная .nv-запись во временной папке, настоящий sem не нужен.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-ref-field-names.sh"
T="${TMPDIR:-/tmp}/novac-ref-names-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d"; cat > "$d/sem.nv"; }

# --- 1. все суффиксы на месте — зелёный -----------------------------------
mk g1 <<'EOF'
module novac.sem

/// One row.
export type FnDef value {
    recv_id int /// receiver type id
    ret_id int /// return type id
    next_row int /// next row with the same name
    row_off int /// first row
    row_cnt int /// how many
    name str /// spelling
}
EOF
if run "$T/g1"; then
    grep -q "полей-ссылок int в реестрах: 5" "$T/out" && ok "все суффиксы — зелёный, счёт верный" || bad "зелёный, но счёт не тот [$(cat "$T/out")]"
else
    bad "чистая запись покраснела: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: голое имя-ссылка — красный ------------------------
mk g2 <<'EOF'
module novac.sem

/// One row.
export type FieldDef value {
    name str /// spelling
    owner int /// owning type id
}
EOF
if run "$T/g2"; then
    bad "голое `owner int` прошло — страж не ловит свой главный случай"
else
    grep -q "owner" "$T/err" && grep -q "sem.nv:6" "$T/err" && ok "голое имя поймано с файлом и строкой" || bad "красный, но без имени/строки [$(cat "$T/err")]"
fi

# --- 3. второе голое имя (recv/ret/sum — тот же класс) --------------------
mk g3 <<'EOF'
module novac.sem
export type FnDef value {
    recv int /// receiver type id
}
EOF
run "$T/g3" && bad "голое `recv int` прошло" || ok "второе голое имя тоже поймано"

# --- 4. `payload` — единственное легальное голое имя ----------------------
mk g4 <<'EOF'
module novac.sem

/// Meaning depends on kind.
export type Def value {
    kind DefKind /// what this name is
    payload int /// type id / variant row / fn row, by kind
}
EOF
if run "$T/g4"; then
    grep -q "полиморфных 1" "$T/out" && ok "payload разрешён и сосчитан отдельно" || bad "payload прошёл, но не сосчитан [$(cat "$T/out")]"
else
    bad "payload покраснел: $(cat "$T/err")"
fi

# --- 5. не-int поля не судятся -------------------------------------------
mk g5 <<'EOF'
module novac.sem
export type T value {
    name str /// spelling
    kind TypeKind /// which table
    dup bool /// overload seen
}
EOF
run "$T/g5" && ok "поля не-int не судятся" || bad "не-int поле покраснело: $(cat "$T/err")"

# --- 6. вне блока type ничего не судится ---------------------------------
mk g6 <<'EOF'
module novac.sem

/// A function with a bare int parameter and a bare local.
export fn lookup(owner int, name str) -> int {
    mut row = 0
    row
}
EOF
run "$T/g6" && ok "параметры и локальные не судятся (правило про хранимую ссылку)" || bad "параметр покраснел зря: $(cat "$T/err")"

# --- 7. нет директории — судить нечего -----------------------------------
run "$T/absent"
grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

# --- 8. настоящее дерево --------------------------------------------------
sh "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящий novac/src/sem — зелёный" || bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -3)"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-ref-field-names ok: все случаи, включая голое имя и полиморфный payload"
    exit 0
fi
exit 1
