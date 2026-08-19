#!/bin/sh
# Самотест check-novac-module-donor.sh (П16). Шов $2 — сканируемая директория.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-module-donor.sh"
T="${TMPDIR:-/tmp}/novac-module-donor-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; cat > "$d/m/m.nv"; }

mk g1 <<'EOF'
// novac/src/m — the module.
// Donor: rustc TyCtxt (rustc_middle::ty) — interned Ty taken, arenas not.
// Role: layer 4 of the map, the type interner.
// Used by: sem and check at E2-b1.
// Guarded by: check-novac-row-fields.py and the interner tests.
module a
EOF
run "$T/g1" && ok "указатель с сущностью — зелёный" || bad "указатель покраснел: $(cat "$T/err")"

mk g2 <<'EOF'
// novac/src/m — the module, no donor line at all.
module a
EOF
if run "$T/g2"; then bad "модуль без Donor прошёл — главный случай не ловится"; else grep -q "нет строки" "$T/err" && ok "отсутствие Donor поймано" || bad "красный, но не про отсутствие"; fi

mk g3 <<'EOF'
// Donor: rustc
module a
EOF
if run "$T/g3"; then bad "голое имя языка прошло"; else grep -q "без сущности" "$T/err" && ok "голое имя поймано" || bad "красный, но не про сущность"; fi

mk g4 <<'EOF'
// Donor: none — the door is ours: exit codes are a closed set of three.
// Role: the CLI door, effects only here.
// Used by: the driver and smokes.
// Guarded by: check-novac-cli-surface.sh, check-novac-effects-at-door.sh.
module a
EOF
run "$T/g4" && ok "'none — причина' принимается" || bad "честное none покраснело: $(cat "$T/err")"

mk g5 <<'EOF'
// Donor: none — ours
module a
EOF
if run "$T/g5"; then bad "'none' без причины прошло"; else grep -q "без причины" "$T/err" && ok "none без причины поймано" || bad "красный, но не про причину"; fi

mk g6 <<'EOF'
/// Donor: Roslyn green tree — full-fidelity, no red tree.
/// Role: layer 2, the syntax tree everyone walks.
/// Used by: parse builds, check walks.
/// Guarded by: check-novac-frontend-shape.py.
module a
EOF
run "$T/g6" && ok "форма /// Donor: принимается" || bad "/// форма покраснела: $(cat "$T/err")"

mk g6b <<'EOF'
// Donor: rustc TyCtxt (rustc_middle::ty) — interned Ty.
module a
EOF
if run "$T/g6b"; then bad "Donor без Role/Used by прошёл — три части не требуются"; else grep -q "Role" "$T/err" && grep -q "Used by" "$T/err" && ok "Donor без Role и Used by пойман, оба названы" || bad "красный, но не про Role/Used by [$(cat "$T/err")]"; fi

# Guarded by называет несуществующего стража — механизм выдуман, красный
mk g6c <<'EOF'
// Donor: rustc TyCtxt (rustc_middle::ty) — interned Ty.
// Role: layer 4 of the map, the type interner.
// Used by: sem and check at E2-b1.
// Guarded by: check-novac-imaginary-guard.sh
module a
EOF
if run "$T/g6c"; then bad "выдуманный страж в Guarded by прошёл"; else grep -q "механизм выдуман" "$T/err" && ok "несуществующий страж в Guarded by пойман" || bad "красный, но не про выдуманный механизм [$(cat "$T/err")]"; fi

# честная форма без файла — compiler / acceptance — зелёная
mk g6d <<'EOF'
// Donor: none — a render of decisions already made, nothing to copy.
// Role: layer 11, the C text backend.
// Used by: the smoke and the diff corpus.
// Guarded by: acceptance — the shape is free by section 7.0, behaviour is held by the smoke.
module a
EOF
run "$T/g6d" && ok "честная форма Guarded by: acceptance принимается" || bad "acceptance-форма покраснела: $(cat "$T/err")"

# П27 2б: антипример как донор — красный; в форме отказа — зелёный
mk g6e <<'EOF'
// Donor: Swift ConstraintSystem — ranking overloads by score.
// Role: layer 7 candidate choice.
// Used by: check at E2-b3.
// Guarded by: check-novac-no-default-branch.py.
module a
EOF
if run "$T/g6e"; then bad "Swift выдан за донора и прошёл"; else grep -q "антипример" "$T/err" && ok "Swift как донор без формы отказа пойман" || bad "красный, но не про антипример"; fi
mk g6f <<'EOF'
// Donor: rustc method::probe — ambiguity is an error; NOT taken: Swift score ranking (exponential).
// Role: layer 7 candidate choice.
// Used by: check at E2-b3.
// Guarded by: check-novac-no-default-branch.py.
module a
EOF
run "$T/g6f" && ok "Swift в форме «NOT taken» законен" || bad "форма отказа покраснела: $(cat "$T/err")"
# точечный донор без сущности
mk g6g <<'EOF'
// Donor: Zig — the way they do it.
// Role: layer 4 interner.
// Used by: sem at E2-b1.
// Guarded by: check-novac-row-fields.py.
module a
EOF
if run "$T/g6g"; then bad "голый Zig как донор прошёл"; else grep -q "без его сущности" "$T/err" && ok "Zig без сущности пойман" || bad "красный, но не про сущность Zig"; fi
# оракул как донор — красный
mk g6h <<'EOF'
// Donor: the oracle's emission on match_demo.nv.
// Role: layer 11 backend.
// Used by: the smoke.
// Guarded by: check-novac-shell-freshness.sh.
module a
EOF
if run "$T/g6h"; then bad "оракул как донор прошёл"; else grep -q "ОРАКУЛ" "$T/err" && ok "оракул как донор пойман (П25/П27 2а)" || bad "красный, но не про оракул"; fi

# донор дальше 40-й строки — не считается заголовком
{ echo "// header"; i=0; while [ $i -lt 45 ]; do echo "// filler"; i=$((i+1)); done; echo "// Donor: rustc TyCtxt interned"; echo "module a"; } > "$T/g7m.nv"
mkdir -p "$T/g7/m"; mv "$T/g7m.nv" "$T/g7/m/m.nv"
run "$T/g7" && bad "Donor за пределами заголовка засчитан" || ok "Donor глубже 40 строк — не заголовок, красный"

# тест-файл не судится
mkdir -p "$T/g8/m"; printf 'module a\n' > "$T/g8/m/m_test.nv"; printf '// Donor: Go types2.Info side table.
// Role: layer 7 channel of resolved types.
// Used by: emit_c and the LSP.
// Guarded by: check-novac-diag-schema.sh.\nmodule a\n' > "$T/g8/m/m.nv"
run "$T/g8" && ok "тест-файл не судится" || bad "тест попал под суд: $(cat "$T/err")"

run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-module-donor ok: все случаи, включая модуль без донора и голое имя языка"
    exit 0
fi
exit 1
