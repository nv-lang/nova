#!/usr/bin/env bash
# scripts/guards/selftest/test-check-selftest-honest-count.sh — самотест стража
# «самотест не врёт о своём покрытии».
#
# ПОЧЕМУ ИМЕННО ЭТИ СЛУЧАИ. Страж заведён 2026-09-04 после того, как
# `test-check-novac-file-size` был пойман на `8/8` при семи случаях в теле.
# Число случаев ЗДЕСЬ поэтому печатается счётчиком, а не рукой: страж, судящий
# чужие литералы и несущий свой, был бы шуткой над самим собой.
set -u
export LC_ALL=C

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-selftest-honest-count.py"
T="${TMPDIR:-/tmp}/selftest-honest-count.$$"
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  FAIL: $1" >&2; FAILED=$((FAILED+1)); }
mkdir -p "$T"
trap 'rm -rf "$T"' EXIT

mk() {  # $1 = dir, $2 = filename, $3 = the final echo line
    mkdir -p "$1"
    { echo '#!/bin/sh'
      echo 'fails=0'
      echo "$3"; } > "$1/$2"
}
run() { NOVAC_SELFTEST_COUNT_BASELINE="$2" python "$G" "$ROOT" "$1" >"$T/out" 2>"$T/err"; }

printf 'literal=0\n' > "$T/base0"
printf 'literal=1\n' > "$T/base1"

# --- 1. счётчик в итоговой строке — законно ------------------------------------
mk "$T/counted" "test-a.sh" 'echo "test-a ok: $CASES/$CASES cases"'
if run "$T/counted" "$T/base0"; then ok "счётчик в итоге — зелёный"; else bad "счётчик покраснел: $(cat "$T/err")"; fi

# --- 2. ГЛАВНЫЙ случай: литерал ------------------------------------------------
mk "$T/literal" "test-b.sh" 'echo "test-b ok: 8/8"'
if run "$T/literal" "$T/base0"; then
    bad "литеральное 8/8 прошло зелёным"
else
    grep -q "test-b.sh:" "$T/err" && ok "литерал пойман и назван файлом" || bad "красный, но без адреса"
fi

# --- 3. литерал в пределах базы — законно (храповик, а не запрет) ---------------
if run "$T/literal" "$T/base1"; then ok "литерал в пределах базы — зелёный"; else bad "база не учтена"; fi

# --- 4. другая форма литерала: «7 случаев» -------------------------------------
mk "$T/words" "test-c.sh" 'echo "test-c ok: 7 cases, all green"'
if run "$T/words" "$T/base0"; then bad "словесный литерал прошёл"; else ok "словесный литерал пойман"; fi

# --- 5. строка БЕЗ числа — не наше дело ----------------------------------------
mk "$T/plain" "test-d.sh" 'echo "test-d ok: every case green"'
if run "$T/plain" "$T/base0"; then ok "итог без числа не судится"; else bad "итог без числа покраснел"; fi

# --- 6. ФИКСТУРА-СТРОКА не считается нарушением --------------------------------
# Самотест, СТРОЯЩИЙ чужой самотест с литералом, сам не нарушитель: `echo` там
# стоит аргументом, а не печатает вердикт. Первая редакция стража краснела на
# собственном самотесте именно так.
mkdir -p "$T/fixture"
{ echo '#!/bin/sh'
  echo 'mk() { printf "%s\n" "$3" > "$1/$2"; }'
  echo 'mk "$T/d" "test-x.sh" '"'"'echo "test-x ok: 8/8"'"'"''
  echo 'echo "test-fixture ok: $CASES/$CASES done"'; } > "$T/fixture/test-e.sh"
if run "$T/fixture" "$T/base0"; then ok "строка-фикстура не считается нарушением"; else bad "фикстура посчитана: $(cat "$T/err")"; fi

# --- 7. МИШЕНЬ ПОТЕРЯНА: ни одного самотеста -----------------------------------
mkdir -p "$T/none"
if run "$T/none" "$T/base0"; then
    bad "ноль самотестов — а страж зелёный"
else
    grep -q "мишень" "$T/err" && ok "ноль самотестов — красный, назван потерей мишени" || bad "красный, но не про мишень"
fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-selftest-honest-count ok: $CASES/$CASES — литерал и его словесная форма краснеют, счётчик и база законны, потеря мишени красная"
    exit 0
fi
exit 1
