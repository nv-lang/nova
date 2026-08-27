#!/bin/sh
# Самотест check-novac-line-length.py (П16). Швы $2 (директория) и $3 (предел).
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-line-length.py"
T="${TMPDIR:-/tmp}/novac-linelen-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" "${2:-40}" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s
" "$@" > "$d/m/m.nv"; echo "$d"; }

D=$(mk short "module a" "fn f() -> int => 1")
run "$D" && ok "короткие строки — зелёный" || bad "короткое покраснело: $(cat "$T/err")"

# --- ГЛАВНЫЙ случай: длинный КОД -----------------------------------------
LONGCODE="    ro x = aaaaaaaaaa + bbbbbbbbbb + cccccccccc + dddddddddd + eeeeeeeeee"
D=$(mk longcode "module a" "$LONGCODE")
if run "$D"; then
    bad "длинная строка кода прошла — главный случай не ловится"
else
    grep -q "символов" "$T/err" && ok "длинная строка кода поймана" || bad "красный, но не про длину"
fi

# --- import судится как любая строка (2026-08-27): несколько import из одного
# модуля компилятор принимает, значит длинный режется, а не прощается. Предел
# самотеста — 40 байт (run), поэтому образцы короткие. -----------------------
D=$(mk imp "module a" "import ../very/long/path.{aaaaaaaaaa, bbbbbbbbbb}")
run "$D" && bad "длинный import прощён — исключение вернулось" || ok "длинный import судится"
D=$(mk imp2 "module a" "import ../p.{aaaaaaaaaa}" "import ../p.{bbbbbbbbbb}")
run "$D" && ok "import, порезанный на строки в пределе, проходит" || bad "короткие import покраснели: $(cat "$T/err")"

# --- исключение 2: образец арма ------------------------------------------
D=$(mk arm "module a" "        Aaaaaaaaaa | Bbbbbbbbbb | Cccccccccc | Dddddddddd => true")
run "$D" && ok "образец арма не судится (язык не переносит)" || bad "арм попал под правило: $(cat "$T/err")"

# --- исключение 3: одна длинная литера -----------------------------------
D=$(mk lit 'module a' '    ice("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")')
run "$D" && ok "одна длинная строковая литера не судится" || bad "литера попала под правило: $(cat "$T/err")"

# --- исключение 4: хвостовой ///-док --------------------------------------
D=$(mk doc "module a" "    x int /// aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
run "$D" && ok "хвостовой ///-док не судится, код короткий" || bad "док попал под правило: $(cat "$T/err")"

# --- проза комментария судится -------------------------------------------
D=$(mk prose "module a" "// aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd eeeeeeeeee ffffffffff")
if run "$D"; then
    bad "длинная проза комментария прошла — её как раз можно перенести"
else
    grep -q "символов" "$T/err" && ok "длинная проза поймана" || bad "красный, но не про длину"
fi

D="$T/nofiles"; mkdir -p "$D/m"
if run "$D"; then bad "дерево без .nv прошло"; else grep -q "мишень" "$T/err" && ok "нет .nv — красный (класс №519)" || bad "красный, но не про мишень"; fi
run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-line-length ok: все случаи, включая четыре исключения и судимую прозу"
    exit 0
fi
exit 1
