#!/usr/bin/env bash
# Самотест check-no-env-after-timeout.sh (реестр 221.1 №1310).
#
# Пять случаев, законное первым — страж, срывающий работу, снесут вместе с правилом:
#   1. законная форма `ИМЯ=значение timeout N cmd` и форма в комментарии -> ok;
#   2. носитель №1310 дословно: `eval "timeout 60 ИМЯ=... cmd"` -> FAIL с адресом;
#   3. то же свойство другим синтаксисом: флаги `-k 5 -s KILL` и длительность `30s` -> FAIL;
#   4. форма в workflow CI (`.yml`) -> FAIL — носитель мог жить и там;
#   5. пустой корень -> честное «судить нечего», а не зелёный ноль с числом.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="$HERE/../check-no-env-after-timeout.sh"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
CASES=0
FAILS=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); FAILS=$((FAILS+1)); echo "  FAIL: $1"; }

mk() { rm -rf "$T/r"; mkdir -p "$T/r/scripts/tools" "$T/r/.github/workflows"; }
# The forbidden form is ASSEMBLED, never written whole: this file lives under
# scripts/ and the guard it tests reads scripts/ -- a literal sample would redden it.
TO="time""out"

# 1
mk
printf '%s\n' '#!/bin/sh' "NOVAC_SELF_PATH=novac/src $TO 60 \"\$NOVAC\" check a.nv" \
    "# forbidden: $TO 60 NOVAC_SELF_PATH=x cmd -- quoted in a comment" "$TO 5 echo hi" > "$T/r/scripts/tools/a.sh"
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
[ "$rc" -eq 0 ] && ok "1: законная форма и цитата в комментарии -> ok" || bad "1: законное дало rc=$rc: $out"

# 2
mk
printf '%s\n' '#!/bin/sh' "    eval \"$TO 60 NOVAC_SELF_PATH=novac/src \\\"\$NOVAC\\\" check \$self_files\" > out" > "$T/r/scripts/tools/b.sh"
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'scripts/tools/b.sh:2'; then ok "2: носитель №1310 -> FAIL с адресом b.sh:2"
else bad "2: носитель не пойман (rc=$rc): $out"; fi

# 3
mk
printf '%s\n' '#!/bin/sh' "$TO -k 5 -s KILL 30s FOO=1 ./run.sh" > "$T/r/scripts/tools/c.sh"
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'c.sh:2'; then ok "3: флаги и 30s -> FAIL"
else bad "3: форма с флагами не поймана (rc=$rc): $out"; fi

# 4
mk
printf '%s\n' 'jobs:' '  x:' '    steps:' "      - run: $TO 600 NOVA_TIER=push bash gate.sh" > "$T/r/.github/workflows/w.yml"
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'w.yml:4'; then ok "4: форма в workflow -> FAIL"
else bad "4: workflow не судится (rc=$rc): $out"; fi

# 5
rm -rf "$T/r"; mkdir -p "$T/r"
out=$(bash "$GUARD" "$T/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'судить нечего'; then ok "5: пустой корень -> «судить нечего»"
else bad "5: пустой корень дал rc=$rc: $out"; fi

if [ "$FAILS" -gt 0 ]; then
    echo "test-check-no-env-after-timeout: FAIL ($FAILS из $CASES)"
    exit 1
fi
echo "test-check-no-env-after-timeout ok: $CASES случаев"
exit 0
