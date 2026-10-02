#!/bin/sh
# Самотест check-novac-self-accepted.py (П16). Карина НЕ гоняется: её вывод —
# подложный файл, шов `--from-file` (шапка стража). Швы `--from-file` и
# `--baseline`.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-self-accepted.py"
T="${TMPDIR:-/tmp}/novac-self-accepted-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0; cases=0
ok()  { echo "  ok  $1"; cases=$((cases+1)); }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" --from-file "$1" --baseline "$2" > "$T/out" 2> "$T/err"; return $?; }

# База: два отвергнутых файла.
printf '# база самотеста\nnovac/src/sem/collect.nv\nnovac/src/sem/mangle.nv\n' > "$T/base.ok"
# Вывод Карины: ровно те два файла.
printf '%s\n%s\n' \
  '{"code":"E_X","file":"novac/src/sem/collect.nv","message":"first boom"}' \
  '{"code":"E_Y","file":"novac/src/sem/mangle.nv","message":"second boom"}' \
  > "$T/out.same"

# --- 1. множество как в базе — зелёный ----------------------------------
if run "$T/out.same" "$T/base.ok"; then
    grep -q "ровно множество базы" "$T/out" && ok "совпадение множества — зелёный" || bad "зелёный, но строка не та [$(cat "$T/out")]"
else
    bad "совпадение покраснело: $(cat "$T/err")"
fi

# --- 2. новый отвергнутый файл — красный, имя и первая диагностика -------
cat "$T/out.same" > "$T/out.grew"
printf '%s\n' '{"code":"E_Z","file":"D:/x/novac/src/sem/binding_test.nv","message":"third boom"}' >> "$T/out.grew"
if run "$T/out.grew" "$T/base.ok"; then
    bad "новый отвергнутый файл прошёл — храповик не держит"
else
    grep -q "sem/binding_test.nv" "$T/err" && grep -q "third boom" "$T/err" \
        && ok "новый отвергнутый пойман, файл и первая диагностика названы" \
        || bad "красный, но без имени файла или диагностики [$(cat "$T/err")]"
fi

# --- 3. файл из базы принят, база не сужена — красный --------------------
printf '%s\n' '{"code":"E_X","file":"novac/src/sem/collect.nv","message":"first boom"}' > "$T/out.shrank"
if run "$T/out.shrank" "$T/base.ok"; then
    bad "сужение без опускания базы прошло — следующий рост пройдёт молча"
else
    grep -q "sem/mangle.nv" "$T/err" && grep -q "ПРОТУХЛА" "$T/err" \
        && ok "протухшая база поймана, файл назван" || bad "красный, но не про протухание"
fi

# --- 4. ICE — красный, даже когда множество совпадает ---------------------
cat "$T/out.same" > "$T/out.ice"
printf '%s\n' '{"code":"E_NOVAC_ICE","file":"novac/src/sem/mangle.nv","message":"ice"}' >> "$T/out.ice"
if run "$T/out.ice" "$T/base.ok"; then
    bad "ICE прошёл — счёт с ICE принят за меру"
else
    grep -q "ICE" "$T/err" && ok "ICE > 0 — красный" || bad "красный, но не про ICE"
fi

# --- 5. нет бинаря Карины (корень с novac/src, без бинаря) — вердикта нет -
mkdir -p "$T/root5/novac/src" "$T/root5/scripts/guards"
printf 'module main\n' > "$T/root5/novac/src/main.nv"
cp "$T/base.ok" "$T/root5/scripts/guards/novac-self-accepted.baseline"
if python "$G" "$T/root5" > "$T/out" 2> "$T/err"; then
    bad "нет бинаря — а страж позеленел"
else
    grep -q "ВЕРДИКТА НЕТ" "$T/err" && ! grep -q "ok:" "$T/out" \
        && ok "нет бинаря — «вердикта нет», не ok" || bad "нет бинаря, но вердикт не назван [$(cat "$T/err")]"
fi

# --- 6. пустой корень — не ok --------------------------------------------
mkdir -p "$T/root6"
if python "$G" "$T/root6" > "$T/out" 2> "$T/err"; then
    bad "пустой корень — а страж позеленел (класс №911)"
else
    ok "пустой корень — отказ, не зелёный ноль"
fi

# --- 7. битая база — красный ----------------------------------------------
printf 'novac/src/sem/collect.nv\nне-путь\n' > "$T/base.broken"
if run "$T/out.same" "$T/base.broken"; then
    bad "битая база прошла"
else
    grep -q "бита" "$T/err" && ok "битая база — красный" || bad "красный, но не про битость"
fi

# --- 8. нет базы вовсе — красный -------------------------------------------
if run "$T/out.same" "$T/base.absent"; then
    bad "отсутствие базы прошло"
else
    grep -q "нет базы" "$T/err" && ok "отсутствие базы — красный" || bad "красный, но не про отсутствие"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-self-accepted ok: $cases случаев, храповик в обе стороны"
    exit 0
fi
exit 1
