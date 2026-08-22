#!/usr/bin/env bash
# Самотест check-novac-iteration-cost.sh — храповик цены цикла novac
# (конвенция П14, план 274.2; норма самотестов — план 231 §4в: механизм
# обязан ЛОВИТЬ нарушение и НЕ давать ложняка, иначе это доверие на слово).
#
# ПОЧЕМУ ПОДЛОЖКА ТАКАЯ. Страж берёт бинарь и раннеры по ФИКСИРОВАННЫМ путям
# внутри $1-корня (novac/target/novac.exe, scripts/tools/novac-e1-smoke.sh,
# scripts/tools/novac-fuzz-mutations.sh) и делает в этот корень cd — значит
# подложка = временный корень, где все трое подменены обманками, а база —
# своя. Настоящий novac и настоящие раннеры самотесту не нужны и не годятся:
# их цена шумит, а судить надо ЛОГИКУ храповика.
#   «медленная» обманка спит 0.25с (замер ~0.3–0.5с со спавном) — факт
#     заведомо выше половины факта базы; потолок бюджета в зелёном случае
#     взят с большим запасом (20с) намеренно: самотесты гейт гоняет
#     параллельно, и спавн процесса под нагрузкой Windows тянется секундами —
#     зелёный случай не имеет права мигать по чужой нагрузке;
#   «быстрая» возвращается сразу (замер ~0.15с) — факт заведомо ниже
#     половины факта базы, это и есть случай «ускорение без поднятия базы».
#
# База стража с 2026-08-15 двухстрочная на ключ: «<ключ> <бюджет>» и
# «<ключ>-last <факт>»; самотест накрывает обе строки и все четыре вердикта
# (просадка, ускорение, нет бюджета, нет факта, противоречивая база).
#
# Копия lib/novac.sh кладётся в подложку намеренно: сегодня страж подключает
# её от своего каталога, но шапка lib/novac.sh называет и $ROOT-форму — если
# подключение переедет, самотест не развалится молча.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-iteration-cost.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }
has()  { if grep -q "$2" "$1"; then ok "$3"; else bad "$3 (нет '$2' в $1: $(tr '\n' '|' < "$1"))"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/scripts/tools" "$FIX/scripts/guards/lib" "$FIX/novac/target" "$FIX/examples/basics"
cp "$ROOT/scripts/guards/lib/novac.sh" "$FIX/scripts/guards/lib/novac.sh"
echo 'fn main() { println("hi") }' > "$FIX/examples/basics/hello.nv"
BASE="$FIX/scripts/guards/novac-iteration-cost.baseline"
NOVAC="$FIX/novac/target/novac.exe"

# $1 — задержка каждого замеряемого прогона в секундах
mkfakes() {
    for p in "$FIX/scripts/tools/novac-e1-smoke.sh" \
             "$FIX/scripts/tools/novac-fuzz-mutations.sh" \
             "$NOVAC"; do
        printf '#!/bin/sh\nsleep %s\nexit 0\n' "$1" > "$p"
        chmod +x "$p"
    done
}
# $1 бюджет, $2 записанный факт — на все три ключа сразу
mkbase() {
    : > "$BASE"
    for k in smoke-warm-ms check-one-ms fuzz-ms; do
        printf '%s %s\n%s-last %s\n' "$k" "$1" "$k" "$2" >> "$BASE"
    done
}
# только бюджеты (строк -last нет) / только факты (строк бюджета нет)
mkbase_budgets_only() {
    printf 'smoke-warm-ms %s\ncheck-one-ms %s\nfuzz-ms %s\n' "$1" "$1" "$1" > "$BASE"
}
mkbase_facts_only() {
    printf 'smoke-warm-ms-last %s\ncheck-one-ms-last %s\nfuzz-ms-last %s\n' "$1" "$1" "$1" > "$BASE"
}
run() { NOVAC_COST=1 sh "$G" "$FIX" > "$TMP/out" 2> "$TMP/err"; echo $?; }

echo "== пропуск и «судить нечего» =="
mkfakes 0; mkbase 2000 500
NOVAC_COST=0 sh "$G" "$FIX" > "$TMP/out" 2> "$TMP/err"
check "NOVAC_COST=0 — зелёный пропуск" "$?" "0"
has "$TMP/out" 'пропущен' "пропуск назван строкой в stdout"

rm -f "$NOVAC"
check "бинаря нет, novac/src нет — зелёное «судить нечего»" "$(run)" "0"
has "$TMP/out" 'ok: судить нечего' "«судить нечего» напечатано (№645)"

mkdir -p "$FIX/novac/src"; echo 'fn main() {}' > "$FIX/novac/src/main.nv"
check "novac/src/main.nv есть, бинаря нет — красный (274.3/F1)" "$(run)" "1"
rm -rf "$FIX/novac/src"

echo "== факт в бюджете — проходит =="
mkfakes 0.25; mkbase 20000 500
check "все три замера внутри бюджета и не быстрее половины факта — зелёный" "$(run)" "0"
has "$TMP/out" 'ok: цена цикла в бюджете' "зелёная строка с итогом"
has "$TMP/out" 'smoke-warm-ms' "замеры напечатаны числами"

echo "== ловит =="
mkbase 1 1
check "факт > бюджета — красный (просадка)" "$(run)" "1"
has "$TMP/err" 'ПРОСАДКА' "просадка названа просадкой"

mkfakes 0
mkbase 100000 100000
check "факт < половины факта базы — красный (ускорение без поднятия базы)" "$(run)" "1"
has "$TMP/err" 'УСКОРЕНИЕ' "ускорение названо ускорением"

mkbase 100000 200000
check "факт базы больше бюджета — красный (противоречивая база)" "$(run)" "1"
has "$TMP/err" 'БАЗА ПРОТИВОРЕЧИВА' "противоречие базы названо"

mkbase_facts_only 500
check "в базе нет строки бюджета — красный" "$(run)" "1"
has "$TMP/err" 'нет бюджета в базе' "недостающий бюджет назван"

mkbase_budgets_only 100000
check "в базе нет строки факта — красный (половина храповика мертва)" "$(run)" "1"
has "$TMP/err" 'нет факта в базе' "недостающий факт назван"

rm -f "$BASE"
check "базы нет вовсе — красный" "$(run)" "1"
has "$TMP/err" 'FAIL' "отсутствие базы названо"

echo "итог: $PASS ok, $FAIL FAIL"
if [ "$FAIL" -eq 0 ]; then
    echo "test-check-novac-iteration-cost ok: $PASS/$PASS"
    exit 0
fi
exit 1
