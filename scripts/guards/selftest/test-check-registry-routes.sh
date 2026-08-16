#!/usr/bin/env bash
# selftest для check-registry-routes.sh.
# Доказывает: страж зелёный на настоящем реестре и КРАСНЕЕТ на росте каждого
# из трёх чисел по отдельности (маршрут / оговорка / блокеры), а также что
# закрытые записи и не-K1 в счёт не идут.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-registry-routes.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

R1='| 1 | 🔴 К1 | **A.** КЛАСС: x. **ОГОВОРКА: фикс носителя приёмкой не считается** — y. **ЧИНИТСЯ:** план 9. **БЛОКИРУЕТ ТЕГ: ДА** Статус: ОТКРЫТ |'
R2='| 2 | 🟡 К2 | **B.** КЛАСС: x. Статус: ОТКРЫТ |'
R3='| 3 | 🔴 К1 | **C.** КЛАСС: x. **ОГОВОРКА: фикс носителя приёмкой не считается** — y. **ЧИНИТСЯ:** план 9. **БЛОКИРУЕТ ТЕГ: ДА** Статус: ЗАКРЫТ |'

mk() {  # $1 = добавочные строки
    rm -rf "$TMP/r"; mkdir -p "$TMP/r/docs/plans" "$TMP/r/scripts/guards"
    cp "$ROOT/scripts/guards/registry-routes-scan.py" "$TMP/r/scripts/guards/"
    cp "$ROOT/scripts/guards/check-registry-routes.sh" "$TMP/r/scripts/guards/"
    { echo "| # | prio | описание |"; echo "|---|---|---|"; echo "$R1"; echo "$R2"; echo "$R3"; [ -n "$1" ] && echo "$1"; } \
        > "$TMP/r/docs/plans/221.1-bug-sweep.md"
    printf 'no_route=0\nno_caveat=0\nblockers=1\n' > "$TMP/r/scripts/guards/registry-routes.baseline"
}

echo "== проходит =="
out=$(bash "$G" "$ROOT" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'ok: открытых блокеров тега'; then
    ok "настоящий реестр — зелёный, числа названы"
else
    bad "ложный красный на реестре: $out"
fi

mk ""
out=$(bash "$TMP/r/scripts/guards/check-registry-routes.sh" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "закрытая K1-запись и K2 в счёт не идут"; else bad "ложный красный на образце: $out"; fi

echo "== ловит =="
mk '| 4 | 🔴 К1 | **D.** КЛАСС: x. **ОГОВОРКА: фикс носителя приёмкой не считается** — y. Статус: ОТКРЫТ |'
out=$(bash "$TMP/r/scripts/guards/check-registry-routes.sh" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'без маршрута'; then ok "K1 без ЧИНИТСЯ — красный"; else bad "пропустил K1 без маршрута (rc=$rc): $out"; fi

mk '| 5 | 🔴 К1 | **E.** КЛАСС: x. **ЧИНИТСЯ:** план 9. Статус: ОТКРЫТ |'
out=$(bash "$TMP/r/scripts/guards/check-registry-routes.sh" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'без оговорки'; then ok "K1 без оговорки о носителе — красный"; else bad "пропустил K1 без оговорки (rc=$rc): $out"; fi

mk '| 6 | 🔴 К1 | **F.** КЛАСС: x. **ОГОВОРКА: фикс носителя приёмкой не считается** — y. **ЧИНИТСЯ:** план 9. **БЛОКИРУЕТ ТЕГ: ДА** Статус: ОТКРЫТ |'
out=$(bash "$TMP/r/scripts/guards/check-registry-routes.sh" "$TMP/r" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'БЛОКЕРОВ ТЕГА'; then ok "рост блокеров тега — красный, число названо"; else bad "пропустил рост блокеров (rc=$rc): $out"; fi

# Ноль без строки — не проверка (№645).
if grep -q '\$NAME ok:' "$G"; then ok "страж печатает свою строку ok:"; else bad "у стража нет строки ok:"; fi

echo "итог: $PASS ok, $FAIL FAIL"
[ "$FAIL" -eq 0 ] || { echo "selftest check-registry-routes: ПРОВАЛ" >&2; exit 1; }
echo "selftest check-registry-routes: OK (зелёный на реестре и на образце / красный на росте каждого из трёх чисел)"
exit 0
