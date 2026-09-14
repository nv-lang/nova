#!/usr/bin/env bash
# Проба в обе стороны на НАСТОЯЩЕМ логе мега-CU прошлого прогона.
# Проверяется ровно то, что я вписал в gate.sh: извлечение имён, самопроверка
# счёта, сопоставление со списком и ТРЕБОВАНИЕ НОМЕРА.
set -u
export LC_ALL=C
ESC=$(printf '\033')
MEGA_LOG=/tmp/gate_mega_508014.log
LIST=/d/Sources/nv-lang/nova/scripts/guards/conformance-known-red.list
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL $1" >&2; fails=$((fails+1)); }

[ -f "$MEGA_LOG" ] || { echo "нет лога $MEGA_LOG — судить не на чем" >&2; exit 1; }

BAD=$(sed -e "s/${ESC}\[[0-9;]*m//g" "$MEGA_LOG" \
    | grep -E "^(NEG-[A-Z-]+|CC-FAIL|RUN-FAIL|CODEGEN-FAIL|MISMATCH|TIMEOUT|FAIL) +spec_tests/" \
    | awk '{print $2}' | sort -u)
BAD_N=$(printf '%s' "$BAD" | grep -c . || true)
FAIL_N=$(sed -e "s/${ESC}\[[0-9;]*m//g" "$MEGA_LOG" \
    | grep -E "PASS: [0-9]+ +FAIL: [0-9]+" | tail -1 \
    | grep -oE "FAIL: [0-9]+" | grep -oE "[0-9]+" | head -1)
echo "лог: FAIL: $FAIL_N, имён извлечено: $BAD_N"
printf '%s\n' "$BAD" | sed 's/^/     /'

# 1. самопроверка счёта — та, ради которой шаг не сверяет «не то»
if [ "$BAD_N" -eq "$FAIL_N" ]; then ok "счёт сошёлся: имён столько же, сколько FAIL"
else bad "счёт разошёлся ($BAD_N против $FAIL_N) — шаг сверял бы не то"; fi

# 2. ЗЕЛЁНАЯ сторона: запись с номером прощает
hit=0
for mb in $BAD; do
    row=$(grep -E "^${mb}([[:space:]]|$)" "$LIST" | head -1)
    [ -n "$row" ] || continue
    printf '%s' "$row" | grep -qE '№[0-9]+' && hit=$((hit+1))
done
if [ "$hit" -eq 1 ]; then ok "прощён ровно один (носитель №1090), а не ноль и не все"
else bad "прощено $hit, ожидался 1"; fi

# 3. КРАСНАЯ сторона: та же запись БЕЗ номера не прощает ничего
sed 's/  # №1090: тот же/  # tot zhe/' "$LIST" > "$TMP/nonum.list"
hit2=0; refused=0
for mb in $BAD; do
    row=$(grep -E "^${mb}([[:space:]]|$)" "$TMP/nonum.list" | head -1)
    [ -n "$row" ] || continue
    if printf '%s' "$row" | grep -qE '№[0-9]+'; then hit2=$((hit2+1)); else refused=$((refused+1)); fi
done
if [ "$refused" -eq 1 ] && [ "$hit2" -eq 0 ]; then ok "запись БЕЗ номера отвергнута, прощено ноль"
else bad "без номера: прощено $hit2, отвергнуто $refused — ожидалось 0 и 1"; fi

# 4. КОНТРОЛЬ: имя, которого в списке нет, не прощается
if grep -qE "^spec_tests/conformance/standalone/or_pattern_binding_same_ok([[:space:]]|$)" "$LIST"; then
    bad "контроль: позеленевший носитель всё ещё в списке — он обязан был быть снят"
else ok "контроль: снятый носитель в списке отсутствует, прощён не будет"; fi

echo "итог: провалов $fails"
[ "$fails" -eq 0 ] || exit 1
