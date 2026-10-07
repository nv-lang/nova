#!/bin/sh
# Самотест scripts/tools/novac-shell-merge.py — сведения двух эмиссий шелла
# novac (windows + linux) в один шаблон с `#ifdef _WIN32` на каждом различии
# (2026-10-01, novac-gate в CI: шаблон, снятый на Windows, на Linux был
# протухшим и неверным по `host_style`).
#
# Оракул НЕ нужен: эмиссии — маленькие подложки, ожидания считаются на бумаге.
# Главная клетка (1) проверяет не «есть #ifdef», а ОБРАТИМОСТЬ: шаблон,
# пропущенный через препроцессор с _WIN32 и без, обязан дать ровно windows- и
# ровно linux-эмиссию. Сведение, которое молча взяло бы одну сторону (как
# прежний регенератор брал эмиссию своей машины), краснеет здесь.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
M="$ROOT/scripts/tools/novac-shell-merge.py"
T="${TMPDIR:-/tmp}/novac-shell-merge-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

PY=""
for _p in python python3; do
    if command -v "$_p" >/dev/null 2>&1 && [ "$("$_p" -c 'print("ok")' 2>/dev/null)" = ok ]; then PY="$_p"; break; fi
done
if [ -z "$PY" ]; then
    echo "test-novac-shell-merge: FAIL — нет рабочего python/python3" >&2
    exit 1
fi

# pick SIDE FILE — what the C preprocessor keeps of a merged template for one
# platform (only the #ifdef _WIN32/#else/#endif shape the merger writes).
pick() {
    awk -v side="$1" '
        $0 == "#ifdef _WIN32" { inblk = 1; branch = "windows"; next }
        inblk && $0 == "#else" { branch = "linux"; next }
        inblk && $0 == "#endif" { inblk = 0; next }
        inblk { if (branch == side) print; next }
        { print }
    ' "$2"
}

common_head() {
    printf '%s\n' '#include "nova_rt.h"' 'static int a(void) {' '    return 1;' '}' \
        '/*__NOVAC_STRLITS__*/' '/*__NOVAC_BODY__*/'
}
common_tail() {
    printf '%s\n' 'static void nova_consts_init(void) {' '/*__NOVAC_INIT__*/' '}' \
        'int main(void) { nova_fn_main_impl(); return 0; }'
}

# --- 1. ОДНО различие -> ровно один #ifdef-кусок, обратимо в обе стороны ---
{ common_head; printf '%s\n' 'static int s(void) {' '    return nova_make_PathStyle_Windows();' '}'; common_tail; } > "$T/w1.c"
{ common_head; printf '%s\n' 'static int s(void) {' '    return nova_make_PathStyle_Posix();' '}'; common_tail; } > "$T/l1.c"
if "$PY" "$M" "$T/w1.c" "$T/l1.c" "$T/o1.c" > "$T/out1" 2> "$T/err1"; then
    n=$(grep -c '^#ifdef _WIN32$' "$T/o1.c")
    pick windows "$T/o1.c" > "$T/o1.w"; pick linux "$T/o1.c" > "$T/o1.l"
    if [ "$n" -ne 1 ]; then
        bad "одно различие дало $n #ifdef-кусков, ждём 1"
    elif ! cmp -s "$T/o1.w" "$T/w1.c"; then
        bad "ветка _WIN32 шаблона не равна windows-эмиссии"
    elif ! cmp -s "$T/o1.l" "$T/l1.c"; then
        bad "ветка #else шаблона не равна linux-эмиссии"
    elif ! grep -q 'platform runs 1 ' "$T/out1"; then
        bad "сводка не назвала один кусок: [$(cat "$T/out1")]"
    else
        ok "одно различие — один #ifdef-кусок, обе ветки дают свои эмиссии байт-в-байт"
    fi
else
    bad "одно различие отвергнуто: [$(cat "$T/err1")]"
fi

# --- 2. Совпадающие эмиссии -> ни одного #ifdef, шаблон == эмиссия ----------
if "$PY" "$M" "$T/w1.c" "$T/w1.c" "$T/o2.c" > /dev/null 2>&1 && cmp -s "$T/o2.c" "$T/w1.c"; then
    ok "одинаковые эмиссии — шаблон равен эмиссии, #ifdef нет"
else
    bad "одинаковые эмиссии дали не саму эмиссию"
fi

# --- 3. Два разнесённых различия -> два куска ------------------------------
{ common_head; printf '%s\n' 'int x = 1;' 'int k = 0;' 'int y = 1;'; common_tail; } > "$T/w3.c"
{ common_head; printf '%s\n' 'int x = 2;' 'int k = 0;' 'int y = 2;'; common_tail; } > "$T/l3.c"
if "$PY" "$M" "$T/w3.c" "$T/l3.c" "$T/o3.c" > /dev/null 2>&1; then
    n=$(grep -c '^#ifdef _WIN32$' "$T/o3.c")
    pick windows "$T/o3.c" > "$T/o3.w"; pick linux "$T/o3.c" > "$T/o3.l"
    if [ "$n" -eq 2 ] && cmp -s "$T/o3.w" "$T/w3.c" && cmp -s "$T/o3.l" "$T/l3.c"; then
        ok "два разнесённых различия — два куска, общая строка между ними одна"
    else
        bad "два различия: кусков $n, обратимость $(cmp -s "$T/o3.w" "$T/w3.c" && echo да || echo нет)"
    fi
else
    bad "два различия отвергнуты"
fi

# --- 4-6. ОТКАЗЫ: слот, вход в программу, продолжение макроса -------------
refuse_case() {
    _name="$1"; _needle="$2"
    rm -f "$T/o.c"
    "$PY" "$M" "$T/w.c" "$T/l.c" "$T/o.c" > /dev/null 2> "$T/err"
    _rc=$?
    if [ "$_rc" -eq 0 ]; then
        bad "$_name: свёл, а обязан отказать"
    elif [ "$_rc" -ne 4 ]; then
        bad "$_name: код $_rc, а отказ слияния — 4: [$(cat "$T/err")]"
    elif [ -f "$T/o.c" ]; then
        bad "$_name: отказ, но выход записан"
    elif ! grep -q "$_needle" "$T/err"; then
        bad "$_name: отказ не про то: [$(cat "$T/err")]"
    else
        ok "$_name — отказ, выход не записан"
    fi
}
{ common_head; common_tail; } > "$T/w.c"
{ common_head | sed 's|/\*__NOVAC_BODY__\*/|/*__NOVAC_BODY__*/ /* linux */|'; common_tail; } > "$T/l.c"
refuse_case "слот novac разошёлся" "__NOVAC_"
{ common_head; common_tail; } > "$T/w.c"
{ common_head; common_tail | sed 's|return 0;|return 1;|'; } > "$T/l.c"
refuse_case "вход main разошёлся" "int main("
{ common_head; printf '%s\n' '#define M(x) \' '    (x + 1)'; common_tail; } > "$T/w.c"
{ common_head; printf '%s\n' '#define M(x) \' '    (x + 2)'; common_tail; } > "$T/l.c"
refuse_case "различие внутри продолжения макроса" "macro continuation"

# --- 7. CR во входе -> отказ (иначе CR запёкся бы в одну сторону) ----------
{ common_head; common_tail; } > "$T/w.c"
{ common_head; common_tail; } | sed 's/$/\r/' > "$T/l.c"
refuse_case "CR во входе" "contains CR"

# --- 8. Управляющий байт в литерале -> трёхзначный восьмеричный escape ----
# (задача #22: оракул пишет `"\b"` / `"\f"` из json-экранирования сырыми 0x08 /
# 0x0c, и check-no-control-chars краснел на шаблоне). Цифра сразу за байтом —
# нарочно: escape обязан быть ровно трёхзначным, иначе C съест её в код байта.
{ common_head; printf 'static const char s[] = "\010\0147";\n'; common_tail; } > "$T/w8.c"
cp "$T/w8.c" "$T/l8.c"
if "$PY" "$M" "$T/w8.c" "$T/l8.c" "$T/o8.c" > /dev/null 2>&1; then
    if grep -qP '[\x00-\x08\x0b\x0c\x0e-\x1f]' "$T/o8.c"; then
        bad "в шаблоне остался сырой управляющий байт"
    elif ! grep -qF 'static const char s[] = "\010\0147";' "$T/o8.c"; then
        bad "байты не переписаны в \\010 / \\014: [$(grep 'char s' "$T/o8.c")]"
    else
        ok "управляющие байты литерала — восьмеричные escape, цифра за ними цела"
    fi
else
    bad "эмиссия с управляющим байтом отвергнута"
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
exit 0
