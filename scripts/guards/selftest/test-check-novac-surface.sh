#!/bin/sh
# Самотест check-novac-surface.py (П16). Швы $2 (директория) и $3 (база).
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-surface.py"
T="${TMPDIR:-/tmp}/novac-surface-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }

mkdir -p "$T/src/sem" "$T/src/lex"
printf 'module a\nexport type A value { x int }\nexport fn f() -> int => 1\nfn hidden() -> int => 2\n' > "$T/src/sem/a.nv"
printf 'module b\nexport fn g() -> int => 1\n' > "$T/src/lex/b.nv"
printf 'module t\nexport fn in_test() -> int => 1\n' > "$T/src/sem/a_test.nv"
printf '# база\nsem 2\nlex 1\n' > "$T/base.ok"

# --- 1. факт равен базе — зелёный ---------------------------------------
if run "$T/src" "$T/base.ok"; then
    grep -q "экспортов всего 3" "$T/out" && ok "совпадение — зелёный, тест-файл не считается" || bad "зелёный, но счёт не тот [$(cat "$T/out")]"
else
    bad "совпадение покраснело: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: рост без поднятия базы — красный ----------------
printf 'module a\nexport type A value { x int }\nexport fn f() -> int => 1\nexport fn extra() -> int => 3\n' > "$T/src/sem/a.nv"
if run "$T/src" "$T/base.ok"; then
    bad "рост поверхности прошёл — храповик не держит"
else
    grep -q "рост без поднятия базы" "$T/err" && grep -q "sem" "$T/err" && ok "рост пойман, модуль назван" || bad "красный, но не про рост [$(cat "$T/err")]"
fi
printf 'module a\nexport type A value { x int }\nexport fn f() -> int => 1\nfn hidden() -> int => 2\n' > "$T/src/sem/a.nv"

# --- 3. протухшая база (факт меньше) — красный --------------------------
printf 'sem 5\nlex 1\n' > "$T/base.stale"
if run "$T/src" "$T/base.stale"; then
    bad "протухшая база прошла — следующий рост пройдёт молча"
else
    grep -q "протухла" "$T/err" && ok "протухшая база поймана" || bad "красный, но не про протухание"
fi

# --- 4. модуль без строки в базе — красный ------------------------------
printf 'sem 2\n' > "$T/base.missing"
run "$T/src" "$T/base.missing" && bad "модуль без строки в базе прошёл" || { grep -q "строки в базе нет" "$T/err" && ok "модуль без строки базы пойман" || bad "красный, но не про строку"; }

# --- 5. строка базы без модуля — красный --------------------------------
printf 'sem 2\nlex 1\nghost 4\n' > "$T/base.ghost"
run "$T/src" "$T/base.ghost" && bad "строка базы без модуля прошла" || { grep -q "модуля в коде нет" "$T/err" && ok "строка без модуля поймана" || bad "красный, но не про модуль"; }

# --- 6. пустая база — красный, не зелёный -------------------------------
printf '# только комментарий\n' > "$T/base.empty"
run "$T/src" "$T/base.empty" && bad "пустая база дала зелёный (класс №519)" || { grep -q "пуста" "$T/err" && ok "пустая база — красный" || bad "красный, но без объяснения"; }

# --- 7. нет базы вовсе — красный ----------------------------------------
run "$T/src" "$T/base.absent" && bad "отсутствие базы прошло" || { grep -q "нет базы" "$T/err" && ok "отсутствие базы — красный (план её обещал)" || bad "красный, но не про отсутствие"; }

# --- 8. нет директории — судить нечего ----------------------------------
run "$T/absent" "$T/base.ok"
grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

# --- 9. настоящее дерево ------------------------------------------------
python "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящее дерево — зелёное" || bad "настоящее дерево покраснело: $(python "$G" "$ROOT" 2>&1 | head -3)"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-surface ok: все случаи, храповик в обе стороны"
    exit 0
fi
exit 1
