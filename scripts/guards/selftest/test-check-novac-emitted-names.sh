#!/bin/sh
# Самотест check-novac-emitted-names.py (П16). Шов $2 — список файлов.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-emitted-names.py"
T="${TMPDIR:-/tmp}/novac-names-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { f="$T/$1.nv"; shift; printf "%s
" "$@" > "$f"; echo "$f"; }

F=$(mk good 'fn a() -> str => "novac_user_x"' 'fn b() -> str => "NOVAC_TAG_Y"' 'fn c() -> str => "nova_fn_main_impl"')
run "$F" && ok "объявленные пространства — зелёный" || bad "чистый файл покраснел: $(cat "$T/err")"

# --- ГЛАВНЫЙ случай: имя без пространства --------------------------------
F=$(mk stray 'fn a() -> str => "novac_user_x"' 'fn b() -> str => "make_thing"')
if run "$F"; then
    bad "имя без пространства прошло — главный случай не ловится"
else
    grep -q "make_thing" "$T/err" && ok "имя без пространства поймано и названо" || bad "красный, но не про имя"
fi

# --- заглавная константа наша — законна ----------------------------------
F=$(mk konst 'fn a() -> str => "NOVAC_TAG_Sum_Var"')
run "$F" && ok "NOVAC_-константа законна" || bad "константа покраснела: $(cat "$T/err")"

# --- временное с подчёркиванием — законно --------------------------------
F=$(mk tmp 'fn a() -> str => "_novac_tmp_x"')
run "$F" && ok "_novac_-временное законно" || bad "временное покраснело: $(cat "$T/err")"

# --- пустой вход — мишень потеряна ---------------------------------------
F=$(mk empty 'fn a() -> int => 1')
if run "$F"; then
    bad "файл без имён прошёл молча"
else
    grep -q "ни одного имени" "$T/err" && ok "нет имён — красный (класс №519)" || bad "красный, но не про мишень"
fi

if run "$T/absent.nv"; then bad "отсутствующий файл прошёл"; else grep -q "судить нечего" "$T/err" && ok "нет файла — красный" || bad "красный, но не про файл"; fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-emitted-names ok: все случаи, включая имя без объявленного пространства"
    exit 0
fi
exit 1
