#!/bin/sh
# Самотест check-novac-batch.sh (П16). Шов $2 — путь к раннеру.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-batch.sh"
T="${TMPDIR:-/tmp}/novac-batch-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

good() {
    cat > "$1" <<'EOS'
NOVAC_BATCH=1
sed "s|^$ROOT/||" "$T/list" > "$T/list.rel"
eval "cd \"$ROOT\" && \"$NOVAC\" check $(sed 's/^/x/' "$T/list.rel" | tr '
' ' ')"
if [ "$brc" -gt 2 ]; then NOVAC_BATCH=0; fi
EOS
}

good "$T/r_ok.sh"
run "$T/r_ok.sh" && ok "полный пачечный проход — зелёный" || bad "полный проход покраснел: $(cat "$T/err")"

good "$T/r_norel.sh"; sed -i 's/list.rel/list/g' "$T/r_norel.sh"
if run "$T/r_norel.sh"; then
    bad "раннер без относительных путей прошёл — главный случай не ловится"
else
    grep -q "ОТНОСИТЕЛЬНЫЕ" "$T/err" && ok "потеря относительных путей поймана" || bad "красный, но не про пути"
fi

good "$T/r_nofb.sh"; sed -i 's/NOVAC_BATCH=0/true/' "$T/r_nofb.sh"
if run "$T/r_nofb.sh"; then
    bad "раннер без отката прошёл"
else
    grep -q "откат" "$T/err" && ok "пропавший откат пойман" || bad "красный, но не про откат"
fi

good "$T/r_nobatch.sh"; sed -i 's/check \$(sed/check "$f" #/' "$T/r_nobatch.sh"
if run "$T/r_nobatch.sh"; then
    bad "раннер без пачечного вызова прошёл"
else
    grep -q "пачечного вызова" "$T/err" && ok "пропавшая пачка поймана" || bad "красный, но не про пачку"
fi

if run "$T/absent.sh"; then bad "отсутствующий раннер прошёл"; else grep -q "судить нечего" "$T/err" && ok "нет раннера — красный (класс №519)" || bad "красный, но не про мишень"; fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-batch ok: все случаи, включая потерю относительных путей и отката"
    exit 0
fi
exit 1
