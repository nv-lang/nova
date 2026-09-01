#!/usr/bin/env bash
# selftest для check-oracle-abi-spellings.sh.
# Доказывает: зелёный на настоящем дереве; КРАСНЕЕТ на исчезновении якоря
# (проверено на трёх разных якорях); отсутствие файла — FAIL, не тихий пропуск.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-oracle-abi-spellings.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

echo "== зелёный на настоящем дереве =="
if out=$(bash "$G" "$ROOT" 2>&1) && printf '%s' "$out" | grep -q 'ok: все'; then
    ok "живое дерево проходит"
else
    bad "живое дерево не проходит: $out"
fi

mkfake() {  # $1 = какой якорь выбросить (fgrep-строка)
    rm -rf "$TMP/r"
    mkdir -p "$TMP/r/compiler-codegen/src/codegen"
    printf '%s\n' '"____"' '_method_' '_static_new' 'NovaOpt_' 'NovaRes_' \
        '_NovaTuple_' 'nova_contract_violation' 'NOVA_CONTRACT_PRE' \
        'nova_fn_main_impl' | grep -vF -- "$1" \
        > "$TMP/r/compiler-codegen/src/codegen/emit_c.rs"
}

echo "== краснеет на дрейфе каждого пробного якоря =="
for a in '_method_' 'NovaOpt_' 'nova_fn_main_impl'; do
    mkfake "$a"
    if out=$(bash "$G" "$TMP/r" 2>&1); then
        bad "не покраснел без якоря $a"
    elif printf '%s' "$out" | grep -qF -- "$a"; then
        ok "покраснел без якоря $a и назвал его"
    else
        bad "покраснел без якоря $a, но не назвал его: $out"
    fi
done

echo "== все якоря на месте в фейке — зелёный (контроль инструмента) =="
mkfake '__no_such_anchor__'
if out=$(bash "$G" "$TMP/r" 2>&1) && printf '%s' "$out" | grep -q 'ok: все'; then
    ok "полный фейк проходит"
else
    bad "полный фейк не проходит: $out"
fi

echo "== нет файла — FAIL =="
rm -rf "$TMP/r"; mkdir -p "$TMP/r"
if bash "$G" "$TMP/r" >/dev/null 2>&1; then
    bad "отсутствие emit_c.rs прошло тихо"
else
    ok "отсутствие emit_c.rs — FAIL"
fi

echo "итого: ok=$PASS fail=$FAIL"
[ "$FAIL" -eq 0 ]
