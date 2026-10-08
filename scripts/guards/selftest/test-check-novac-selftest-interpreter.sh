#!/usr/bin/env bash
# Самотест check-novac-selftest-interpreter.py — обе стороны, на фикстурном
# каталоге самотестов.
#
# ЦЕНТРАЛЬНЫЙ СЛУЧАЙ — ПОДСТАНОВКА `$("$G" ...)`. Именно в этой форме прямой
# вызов и жил в обоих пойманных самотестах: в начале строки его видно глазом, а
# внутри `OUT=$(...)` — нет. Второй несущий: строка-КОММЕНТАРИЙ, где прямой
# вызов упомянут как пример, зелёная — иначе документировать правило было бы
# нельзя (и страж красил бы собственную доку).
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-selftest-interpreter.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has(){ if printf '%s' "$2" | grep -q "$3"; then ok "$1"; else bad "$1 (в выводе нет '$3': '$2')"; fi; }

S="$TMP/selftest"
mkdir -p "$S"
F="$S/test-fixture.sh"

echo "== проходит =="
OUT=$(python "$G" "$TMP" "$TMP/nowhere" 2>&1); RC=$?
check "нет каталога самотестов — зелёный (судить нечего)" "$RC" "0"

printf 'bash "$G" "$R" >/dev/null\nOUT=$(bash "$G" "$R" 2>&1)\n' > "$F"
OUT=$(python "$G" "$TMP" "$S" 2>&1); RC=$?
check "вызовы через bash — зелёный" "$RC" "0"
has "назвал число самотестов" "$OUT" "1"

printf 'python "$G" "$TMP" >/dev/null\nOUT=$(python3 "$G" "$R" 2>&1)\n' > "$F"
OUT=$(python "$G" "$TMP" "$S" 2>&1); RC=$?
check "вызовы через python и python3 — зелёный" "$RC" "0"

printf '# так делать нельзя: OUT=$("$G" "$R")\nbash "$G" "$R"\n' > "$F"
OUT=$(python "$G" "$TMP" "$S" 2>&1); RC=$?
check "прямой вызов в КОММЕНТАРИИ — зелёный (иначе не задокументировать)" "$RC" "0"

echo "== краснеет =="
printf '"$G" "$R" >/dev/null 2>&1\n' > "$F"
OUT=$(python "$G" "$TMP" "$S" 2>&1); RC=$?
check "прямой вызов в начале строки — красный" "$RC" "1"
has "назвал строку" "$OUT" "test-fixture.sh:1"

printf 'bash "$G" "$R"\nOUT=$("$G" "$R" 2>&1); RC=$?\n' > "$F"
OUT=$(python "$G" "$TMP" "$S" 2>&1); RC=$?
check "прямой вызов в ПОДСТАНОВКЕ — красный (так он и жил)" "$RC" "1"
has "указал именно вторую строку" "$OUT" "test-fixture.sh:2"
has "объяснил, почему это Linux-only отказ" "$OUT" "126"
has "дал правильную форму" "$OUT" 'bash "\$G"'

printf 'X=1 && "$G" "$R"\n' > "$F"
OUT=$(python "$G" "$TMP" "$S" 2>&1); RC=$?
check "прямой вызов после && — красный" "$RC" "1"

EMPTY="$TMP/nofiles"; mkdir -p "$EMPTY"
OUT=$(python "$G" "$TMP" "$EMPTY" 2>&1); RC=$?
check "каталог без самотестов — красный (страж потерял мишень)" "$RC" "1"
has "назвал класс потерянной мишени" "$OUT" "519"

echo "== второе правило: sh на скрипт с шебангом bash =="
# Носители (реестр 221.1 №1825): test-check-runtime-cxx-clean и
# test-check-registry-single-verdict звали bash-стража через `sh` — на Linux
# это dash, страж падал синтаксисом, ночной full красен с 2026-10-01.
R2="$TMP/r2"; GS="$R2/scripts/guards"; S2="$GS/selftest"
mkdir -p "$S2"
printf '#!/usr/bin/env bash\nset -o pipefail\n' > "$GS/check-b.sh"
printf '#!/bin/sh\nexit 0\n' > "$GS/check-p.sh"
printf 'G="$GD/check-p.sh"\nsh "$G" "$R"\n' > "$S2/test-p.sh"
printf 'G="$GD/check-b.sh"\nbash "$G" "$R"\n' > "$S2/test-b.sh"
OUT=$(python "$G" "$R2" 2>&1); RC=$?
check "sh на sh-скрипт и bash на bash-скрипт — зелёный" "$RC" "0"
has "назвал число узнанных вызовов sh" "$OUT" "вызовов \`sh\` с узнанной целью 1"

printf 'G="$GD/check-b.sh"\nOUT=$(sh "$G" "$R" 2>&1)\n' > "$S2/test-b.sh"
OUT=$(python "$G" "$R2" 2>&1); RC=$?
check "sh \"\$G\" на bash-стража — красный" "$RC" "1"
has "назвал строку и цель" "$OUT" "test-b.sh:2: sh -> check-b.sh"
has "назвал dash как причину" "$OUT" "dash"

printf 'bash "$G"\n' > "$S2/test-b.sh"
printf '#!/bin/sh\nsh "$ROOT/scripts/guards/check-b.sh" "$ROOT" || exit 1\n' > "$GS/check-hyg.sh"
OUT=$(python "$G" "$R2" 2>&1); RC=$?
check "литеральный путь вне самотестов — тоже красный" "$RC" "1"
has "назвал не-самотест" "$OUT" "check-hyg.sh:2"

printf '#!/bin/sh\nprintf "sh \\"$ROOT/scripts/guards/check-b.sh\\"\\n" > x\n' > "$GS/check-hyg.sh"
OUT=$(python "$G" "$R2" 2>&1); RC=$?
check "строка, ПЕЧАТАЮЩАЯ вызов, — зелёная (данные, а не вызов)" "$RC" "0"

echo "самотест check-novac-selftest-interpreter: PASS $PASS FAIL $FAIL"
[ "$FAIL" -eq 0 ]
