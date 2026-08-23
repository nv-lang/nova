#!/bin/sh
# Самотест check-gate-budget (П16: страж без доказательства красноты
# запрещён). Красноту доказываем МУТАЦИЕЙ ПОДСУДНОГО: у подложного дерева
# отнимаем по одной части механизма и ждём красного с названной причиной.
#
# Форма перенесена из окна 274 вместе со стражем; перенацелена на главный гейт
# дерева (переменная яруса `NOVA_GATE_TIER`, судимый файл `scripts/gate.sh`).
#
# Отдельно доказана краснота САМОГО гейта при превышении бюджета — она живёт не
# в страже, а в гейте: потолок яруса loop временно опускается до 1с, гейт
# выходит с кодом 1, потолок возвращается — выходит с 0.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-gate-budget.py"
T="${TMPDIR:-/tmp}/nova-gate-budget-selftest.$$"
mkdir -p "$T/tree/scripts/guards"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Подложный гейт со ВСЕМИ частями механизма.
mk_gate() {
    cat > "$1" <<'SH'
NOVA_GATE_TIER="${NOVA_GATE_TIER:-full}"
case "$NOVA_GATE_TIER" in
    loop|push|full) ;;
    *) exit 2 ;;
esac
GATE_ELAPSED=$(( $(date +%s) - GATE_T0 ))
BUDGET_FILE="$ROOT/scripts/guards/gate-budget.baseline"
BUDGET_LIMIT=$(( BUDGET * CAL ))
if [ "$GATE_ELAPSED" -gt "$BUDGET_LIMIT" ]; then
    fail "ярус $NOVA_GATE_TIER вышел за бюджет времени (конвенция гейтов, Г4)"
fi
echo "строки бюджета для него нет (не судится)"
SH
}
mk_budget() { printf '# comment\n\nloop 60\npush 900\n' > "$1"; }

mk_gate "$T/tree/gate.sh"
mk_budget "$T/tree/scripts/guards/gate-budget.baseline"

# ── 1. живое дерево — зелёный со строкой ok: ──────────────────────────────
if python "$G" "$ROOT" > "$T/o1" 2> "$T/e1"; then
    grep -q "^check-gate-budget ok:" "$T/o1" \
        && ok "живое дерево — зелёный со строкой ok:" \
        || bad "зелёный без строки ok: [$(head -n 1 "$T/o1")]"
else
    bad "живое дерево покраснело: [$(head -n 2 "$T/e1")]"
fi

# ── 2. подложное дерево со всеми частями — зелёный ───────────────────────
python "$G" "$T/tree" "$T/tree/gate.sh" > "$T/o2" 2>&1 \
    && ok "фикстура со всем механизмом — зелёный" \
    || bad "здоровая фикстура покраснела: [$(head -n 2 "$T/o2")]"

# ── 3. нет файла бюджета — красный ───────────────────────────────────────
mv "$T/tree/scripts/guards/gate-budget.baseline" "$T/hidden"
if python "$G" "$T/tree" "$T/tree/gate.sh" > "$T/o3" 2> "$T/e3"; then
    bad "дерево без файла бюджета прошло зелёным"
else
    grep -q "время гейта не ограничено" "$T/e3" \
        && ok "нет файла бюджета — красный, причина названа" \
        || bad "красный, но не про отсутствие бюджета"
fi
mv "$T/hidden" "$T/tree/scripts/guards/gate-budget.baseline"

# ── 4. строка бюджета не разбирается — красный ───────────────────────────
printf 'loop sixty\n' > "$T/tree/scripts/guards/gate-budget.baseline"
if python "$G" "$T/tree" "$T/tree/gate.sh" > "$T/o4" 2> "$T/e4"; then
    bad "неразбираемая строка бюджета прошла"
else
    grep -q "не разбирается" "$T/e4" \
        && ok "строка не по форме — красный" \
        || bad "красный, но не про форму строки"
fi
mk_budget "$T/tree/scripts/guards/gate-budget.baseline"

# ── 5..8. механизм выхолощен по частям — каждый раз красный ──────────────
mutate() { # $1 sed-выражение, $2 ожидаемая причина, $3 имя случая
    mk_gate "$T/tree/gate.sh"
    sed -i "$1" "$T/tree/gate.sh"
    if python "$G" "$T/tree" "$T/tree/gate.sh" > "$T/om" 2> "$T/em"; then
        bad "$3: выхолощенный механизм прошёл зелёным"
    else
        grep -q "$2" "$T/em" && ok "$3 — красный" || bad "$3: красный, но не по той причине"
    fi
}
mutate 's|gate-budget.baseline|some-other-file|'  "не читает файл бюджета"   "гейт не читает файл бюджета"
mutate 's|GATE_ELAPSED|SOMETHING_ELSE|g'          "не меряет собственное"    "гейт не меряет своё время"
mutate 's|BUDGET \* CAL|BUDGET|'                  "не масштабируется"        "предел без калибровки машины"
mutate 's|fail "ярус |echo "ярус |'               "не приводит к отказу"     "превышение без отказа"

# ── 9. ярусов не видно вовсе — красный (страж ни о чём, класс №519) ──────
mk_gate "$T/tree/gate.sh"
sed -i 's|^case "\$NOVA_GATE_TIER" in|case "$SOMETHING" in|' "$T/tree/gate.sh"
if python "$G" "$T/tree" "$T/tree/gate.sh" > "$T/o9" 2> "$T/e9"; then
    bad "гейт без разбора ярусов прошёл зелёным"
else
    grep -q "не найден разбор NOVA_GATE_TIER" "$T/e9" \
        && ok "разбор ярусов пропал — красный" \
        || bad "красный, но не про пропавший разбор: [$(head -n 1 "$T/e9")]"
fi

# ── 10. ярус без бюджета и без честного «не судится» — красный ───────────
mk_gate "$T/tree/gate.sh"
sed -i 's|echo "строки бюджета для него нет (не судится)"||' "$T/tree/gate.sh"
if python "$G" "$T/tree" "$T/tree/gate.sh" > "$T/oa" 2> "$T/ea"; then
    bad "ярус без бюджета и без честной строки прошёл (вечнозелёная дыра)"
else
    grep -q "без честного" "$T/ea" \
        && ok "ярус без бюджета обязан быть назван вслух — красный" \
        || bad "красный, но не про несудимый ярус: [$(head -n 1 "$T/ea")]"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-gate-budget ok: все случаи, включая четыре формы выхолащивания механизма"
    exit 0
fi
exit 1
