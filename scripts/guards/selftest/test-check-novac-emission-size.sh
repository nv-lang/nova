#!/bin/sh
# Самотест check-novac-emission-size.sh (П16). Швы $2 (база) и $3 (бинарь).
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-emission-size.sh"
T="${TMPDIR:-/tmp}/novac-emission-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }

# подменный novac: печатает ровно N строк
mkfake() { printf '#!/bin/sh
seq 1 %s
' "$1" > "$T/fake.sh"; chmod +x "$T/fake.sh"; }

printf 'hello 7
' > "$T/base_ok"
mkfake 7
run "$T/base_ok" "$T/fake.sh" && ok "объём совпал с базой — зелёный" || bad "совпадение покраснело: $(cat "$T/err")"

mkfake 9
if run "$T/base_ok" "$T/fake.sh"; then
    bad "рост объёма прошёл — главный случай не ловится"
else
    grep -q "9 строк C, в базе 7" "$T/err" && ok "рост эмиссии пойман и назван числом" || bad "красный, но не про число [$(cat "$T/err")]"
fi

mkfake 5
if run "$T/base_ok" "$T/fake.sh"; then
    bad "падение объёма прошло — протухшая база пропустит следующий рост"
else
    grep -q "5 строк C, в базе 7" "$T/err" && ok "падение поймано (храповик в обе стороны)" || bad "красный, но не про падение"
fi

printf '# only comments
' > "$T/base_empty"
mkfake 7
if run "$T/base_empty" "$T/fake.sh"; then
    bad "пустая база прошла молча"
else
    grep -q "пуста" "$T/err" && ok "пустая база — красный (класс №519)" || bad "красный, но не про пустую базу"
fi

printf 'hello 7
' > "$T/base_ok"
if run "$T/base_ok" "$T/nosuch.exe"; then
    bad "отсутствующий бинарь прошёл"
else
    grep -q "нет бинаря" "$T/err" && ok "нет бинаря — красный, не молчит" || bad "красный, но не про бинарь"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-emission-size ok: все случаи, включая рост, падение и пустую базу"
    exit 0
fi
exit 1
