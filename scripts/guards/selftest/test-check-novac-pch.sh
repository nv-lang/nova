#!/bin/sh
# Самотест check-novac-pch.sh (П16). Шов $2 — сканируемая директория инструментов.
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

mk() {
    d="$T/$1"; mkdir -p "$d/tools"; shift
    printf '%s
' "$@" > "$d/tools/novac-x.sh"
    echo "$d"
}

FULL_PCH='    "$REAL_CLANG" $CFLAGS -x c-header "$CACHE/prelude-$ORACLE_STAMP.h" -o "$PCH"'
FULL_CC='"$REAL_CLANG" $CFLAGS -include-pch "$PCH" -c "$T/body.c" -o "$T/body.o"'
FULL_LINK='"$REAL_CLANG" $LINKCMD -o "$T/out.exe" "$T/body.o"'

D=$(mk good 'PCH="$CACHE/prelude-$ORACLE_STAMP.pch"' 'if [ ! -f "$PCH" ]; then' "$FULL_PCH" 'fi' "$FULL_CC" "$FULL_LINK")
if run "$D"; then
    grep -q "все с -include-pch" "$T/out" && ok "полный путь PCH — зелёный" || bad "зелёный, но без счёта [$(cat "$T/out")]"
else
    bad "полный путь покраснел: $(cat "$T/err")"
fi

# --- ГЛАВНЫЙ случай: компиляция без PCH ----------------------------------
D=$(mk nouse 'PCH="$CACHE/prelude-$ORACLE_STAMP.pch"' 'if [ ! -f "$PCH" ]; then' "$FULL_PCH" 'fi' '"$REAL_CLANG" $CFLAGS -c "$T/body.c" -o "$T/body.o"' "$FULL_LINK")
if run "$D"; then
    bad "компиляция без -include-pch прошла — главный случай не ловится"
else
    grep -q "без -include-pch" "$T/err" && ok "компиляция без PCH поймана" || bad "красный, но не про -include-pch"
fi

# --- линковка PCH не требует ---------------------------------------------
D=$(mk linkonly 'PCH="$CACHE/prelude-$ORACLE_STAMP.pch"' 'if [ ! -f "$PCH" ]; then' "$FULL_PCH" 'fi' "$FULL_CC" '"$REAL_CLANG" -o "$T/a.exe" "$T/a.o" "$T/b.o"')
run "$D" && ok "линковка не считается компиляцией" || bad "линковка попала под правило: $(cat "$T/err")"

# --- PCH никто не строит --------------------------------------------------
D=$(mk nobuild 'PCH="$CACHE/prelude-$ORACLE_STAMP.pch"' 'if [ ! -f "$PCH" ]; then' 'true' 'fi' "$FULL_CC")
if run "$D"; then
    bad "дерево без сборки PCH прошло"
else
    grep -q "не СТРОИТ PCH" "$T/err" && ok "отсутствие сборки PCH поймано" || bad "красный, но не про сборку"
fi

# --- вообще ни одной компиляции — мишень потеряна ------------------------
D=$(mk empty 'echo hello')
if run "$D"; then
    bad "дерево без компиляций прошло молча"
else
    grep -q "ни одной компиляции" "$T/err" && ok "нет компиляций — красный (класс №519)" || bad "красный, но не про мишень"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-pch ok: все случаи, включая компиляцию без -include-pch и линковку-исключение"
    exit 0
fi
exit 1
