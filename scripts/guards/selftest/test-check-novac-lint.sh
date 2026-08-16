#!/bin/sh
# Самотест check-novac-lint.sh (П16). Шов $2 — бинарь nova (подменный скрипт).
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-lint.sh"
T="${TMPDIR:-/tmp}/novac-lint-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

printf '#!/bin/sh\necho "lint: 20 file(s), 0 finding(s)"\n' > "$T/clean"; chmod +x "$T/clean"
run "$T/clean" && ok "чистый линт — зелёный" || bad "чистый линт покраснел: $(cat "$T/err")"

printf '#!/bin/sh\necho "novac/src/x.nv:3: style: bad name"\necho "lint: 20 file(s), 1 finding(s)"\n' > "$T/dirty"; chmod +x "$T/dirty"
if run "$T/dirty"; then bad "линт с замечанием прошёл — главный случай не ловится"; else grep -q "нашёл замечания" "$T/err" && ok "замечание линта поймано" || bad "красный, но не про замечания"; fi

printf '#!/bin/sh\necho "boom"\nexit 2\n' > "$T/broken"; chmod +x "$T/broken"
if run "$T/broken"; then bad "нераспознанный вывод прошёл"; else grep -q "не распознан" "$T/err" && ok "сломанный разбор пойман (№519)" || bad "красный, но не про разбор"; fi

if run "$T/absent"; then bad "отсутствующий бинарь прошёл"; else grep -q "не найден" "$T/err" && ok "нет бинаря — красный, не молчит" || bad "красный, но не про бинарь"; fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-lint ok: все случаи, включая замечание линта и сломанный разбор вывода"
    exit 0
fi
exit 1
