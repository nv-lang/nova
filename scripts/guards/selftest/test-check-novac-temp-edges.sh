#!/bin/sh
# Самотест check-novac-temp-edges.sh (П16). Швы $2 (архитектура), $3 (nova.toml),
# $4 (директория кода с ice-маркерами на ошибке пользователя).
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-temp-edges.sh"
T="${TMPDIR:-/tmp}/novac-temp-edges-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" "$2" "${3:-$T/nosrc}" > "$T/out" 2> "$T/err"; }
mk_src() { d="$T/src.$1"; mkdir -p "$d/m"; printf '%s
' "$2" > "$d/m/m.nv"; echo "$d"; }

mk_toml() { printf '# pins\n#   spec-point: 2026-08-14\n#   stage: %s\n\n[package]\nname = "novac"\n' "$1" > "$T/toml.$1"; echo "$T/toml.$1"; }
mk_arch() { # $1 имя, дальше строки таблицы
    f="$T/arch.$1"; shift
    { echo "# Архитектура"; echo ""; echo "## 3. Рёбра"; echo ""; echo "| из | в | что течёт |"; echo "|---|---|---|"
      for l in "$@"; do echo "$l"; done; echo ""; echo "Проза со словом временное вне таблицы не считается."; } > "$f"; echo "$f"
}

# --- 1. все со сроком, срок впереди — зелёный ----------------------------
A=$(mk_arch ok '| `check` | `sem` | **временное until:E4** читает |' '| `emit_c` | `tree` | временное until:E2b1 рендер |' '| `parse` | `lex` | постоянное |')
TO=$(mk_toml E2)
if run "$A" "$TO"; then
    grep -q "временных рёбер 2" "$T/out" && ok "все со сроком впереди — зелёный, счёт верный" || bad "зелёный, но счёт не тот [$(cat "$T/out")]"
else
    bad "чистая таблица покраснела: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: этап наступил — ребро истекло -------------------
TO=$(mk_toml E2b1)
if run "$A" "$TO"; then
    bad "истёкшее ребро прошло — самоистекания нет"
else
    grep -q "ИСТЕКЛО" "$T/err" && grep -q "emit_c" "$T/err" && ok "истёкшее ребро поймано, названо" || bad "красный, но не про истечение [$(cat "$T/err")]"
fi

# --- 3. этап ДАЛЬШЕ срока — тоже истекло ---------------------------------
TO=$(mk_toml E4)
run "$A" "$TO" && bad "ребро until:E2b1 при stage E4 прошло" || ok "этап дальше срока — истекло"

# --- 4. временное БЕЗ срока — красный ------------------------------------
A2=$(mk_arch nountil '| `check` | `sem` | Э1-временное, без даты |')
TO=$(mk_toml E2)
if run "$A2" "$TO"; then
    bad "временное без until прошло — вечное с красивым словом"
else
    grep -q "БЕЗ срока" "$T/err" && ok "временное без срока поймано" || bad "красный, но не про срок"
fi

# --- 5. нет строки stage в toml — красный --------------------------------
printf '# pins\n#   spec-point: 2026-08-14\n' > "$T/toml.nostage"
run "$A" "$T/toml.nostage" && bad "toml без stage прошёл" || { grep -q "нет строки" "$T/err" && ok "отсутствие якоря этапа — красный" || bad "красный, но не про якорь"; }

# --- 6. неизвестный этап — красный ---------------------------------------
TO=$(mk_toml E9)
run "$A" "$TO" && bad "неизвестный этап прошёл" || { grep -q "неизвестен" "$T/err" && ok "неизвестный этап — красный" || bad "красный, но не про этап"; }

# --- 7. слово «временное» в прозе вне таблицы не считается ---------------
A3=$(mk_arch prose '| `parse` | `lex` | постоянное |')
TO=$(mk_toml E2)
run "$A3" "$TO" && { grep -q "временных рёбер 0" "$T/out" && ok "проза вне таблицы не судится" || bad "счёт неверен [$(cat "$T/out")]"; } || bad "проза покраснела: $(cat "$T/err")"

# --- 8. нет архитектуры — судить нечего ---------------------------------
run "$T/absent.md" "$TO"
grep -q "судить нечего" "$T/out" && ok "нет файла — судить нечего" || bad "ждали «судить нечего»"

# --- 9. настоящее дерево ------------------------------------------------
sh "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящее дерево — зелёное" || bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -3)"

# --- маркеры в коде: [LEGACY-#...-user-error-as-ice until:<этап>] --------
A=$(mk_arch clean '| `parse` | `lex` | постоянное |')
TO=$(mk_toml E2)
SR=$(mk_src fut '// [LEGACY-#TBD-user-error-as-ice until:E2b3] check has no typing yet
fn f() { ice("x") }')
if run "$A" "$TO" "$SR"; then grep -q "ошибке пользователя 1" "$T/out" && ok "маркер со сроком впереди — зелёный, счёт 1" || bad "зелёный, но счёт: $(cat "$T/out")"; else bad "маркер со сроком впереди покраснел: $(cat "$T/err")"; fi
SR=$(mk_src exp '// [LEGACY-#TBD-user-error-as-ice until:E2] a
fn f() { ice("x") }')
if run "$A" "$TO" "$SR"; then bad "истёкший ice-маркер прошёл — главный случай не ловится"; else grep -q "истёк" "$T/err" && ok "истёкший маркер (until:E2 при stage E2) пойман" || bad "красный, но не про истечение"; fi
SR=$(mk_src nou '// [LEGACY-#TBD-user-error-as-ice] no date
fn f() { ice("x") }')
if run "$A" "$TO" "$SR"; then bad "маркер без until прошёл"; else grep -q "БЕЗ until" "$T/err" && ok "маркер без срока пойман" || bad "красный, но не про отсутствие срока"; fi
SR=$(mk_src tst '// clean
fn f() { }'); printf '// [LEGACY-#TBD-user-error-as-ice] test
' > "$SR/m/m_test.nv"
run "$A" "$TO" "$SR" && ok "маркер в *_test.nv не судится" || bad "тест-файл попал под суд: $(cat "$T/err")"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-temp-edges ok: все случаи, включая истечение по этапу, временное без срока и ice-маркер, переживший этап"
    exit 0
fi
exit 1
