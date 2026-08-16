#!/bin/sh
# Самотест check-novac-pch.sh (П16). Шов $2 — путь к смоуку.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-pch.sh"
T="${TMPDIR:-/tmp}/novac-pch-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# полноценная подложка: сборка по штампу, кэш, применение
good() {
    cat > "$1" <<'EOS'
#!/bin/sh
PCH="$CACHE/prelude-$ORACLE_STAMP.pch"
if [ ! -f "$PCH" ]; then
    "$REAL_CLANG" $CFLAGS -x c-header "$CACHE/prelude-$ORACLE_STAMP.h" -o "$PCH"
fi
"$REAL_CLANG" $CFLAGS -include-pch "$PCH" -c "$T/body.c" -o "$T/body.o"
EOS
}

good "$T/smoke_ok.sh"
run "$T/smoke_ok.sh" && ok "полный путь PCH — зелёный" || bad "полный путь покраснел: $(cat "$T/err")"

# --- ГЛАВНЫЙ случай: применение убрали -----------------------------------
good "$T/smoke_nouse.sh"; sed -i 's/-include-pch "\$PCH" //' "$T/smoke_nouse.sh"
if run "$T/smoke_nouse.sh"; then
    bad "смоук без -include-pch прошёл — главный случай не ловится"
else
    grep -q "не ИСПОЛЬЗУЕТСЯ" "$T/err" && ok "исчезнувший -include-pch пойман" || bad "красный, но не про применение"
fi

# --- построение убрали ----------------------------------------------------
good "$T/smoke_nobuild.sh"; sed -i 's/-x c-header/-c/' "$T/smoke_nobuild.sh"
if run "$T/smoke_nobuild.sh"; then
    bad "смоук без построения PCH прошёл"
else
    grep -q "нет построения PCH" "$T/err" && ok "пропавшее построение поймано" || bad "красный, но не про построение"
fi

# --- кэш без штампа ревизии ----------------------------------------------
good "$T/smoke_nostamp.sh"; sed -i 's/prelude-\$ORACLE_STAMP/prelude/g' "$T/smoke_nostamp.sh"
if run "$T/smoke_nostamp.sh"; then
    bad "PCH без штампа ревизии прошёл — протухший кэш даст мусорный объектник"
else
    grep -q "штамп" "$T/err" && ok "PCH без штампа ревизии пойман" || bad "красный, но не про штамп"
fi

# --- строится каждый прогон ----------------------------------------------
good "$T/smoke_always.sh"; sed -i 's/^if \[ ! -f "\$PCH" \]; then$/if true; then/' "$T/smoke_always.sh"
if run "$T/smoke_always.sh"; then
    bad "безусловная сборка PCH прошла — цена возвращается на каждый прогон"
else
    grep -q "безусловно" "$T/err" && ok "безусловная сборка поймана" || bad "красный, но не про безусловность"
fi

# --- мишень потеряна ------------------------------------------------------
if run "$T/absent.sh"; then
    bad "отсутствующий смоук прошёл молча"
else
    grep -q "горячий путь novac потерян" "$T/err" && ok "нет смоука — красный (класс №519)" || bad "красный, но не про мишень"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-pch ok: все случаи, включая исчезнувший -include-pch и кэш без штампа"
    exit 0
fi
exit 1
