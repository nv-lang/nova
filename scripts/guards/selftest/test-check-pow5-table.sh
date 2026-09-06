#!/usr/bin/env bash
# Селфтест scripts/guards/check-pow5-table.sh (план 283 Ф.1).
#
# Обе стороны обязательны (правило владельца 2026-07-27, энфорсится
# check-guard-wiring.sh): страж, краснеющий на правильном дереве, отключат в
# первый же день, а страж, не краснеющий на порче, охраняет ноль.
#
# Случаи (итог печатается СЧЁТЧИКОМ, не литералом — check-selftest-honest-count):
#   1. настоящее дерево — зелено;
#   2. пустая копия дерева: --write, затем страж — зелено (круг замыкается);
#   3. в копии одна шестнадцатеричная цифра слова hi изменена — красно,
#      вердикт называет q и половину hi;
#   4. то же со словом lo — красно, называет lo;
#   5. удалена одна строка (650 вместо 651) — красно, называет 650;
#   6. таблицы нет вовсе — красно и называет путь (мишень потеряна != ноль).
#
# Работаем на ВРЕМЕННОЙ копии генератора и таблицы; настоящую таблицу не трогаем.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-pow5-table.sh"
GEN="$ROOT/scripts/tools/gen-pow5-table.py"
REL="std/src/runtime/string/pow5_table.nv"
FAILED=0
CASES=0
ok()  { CASES=$((CASES + 1)); echo "  ok: $1"; }
bad() { CASES=$((CASES + 1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/tools" "$TMP/std/src/runtime/string"
cp "$GEN" "$TMP/scripts/tools/gen-pow5-table.py"

# 1. Настоящее дерево — зелено.
out=$(bash "$G" "$ROOT" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "check-pow5-table ok:"; then
    ok "настоящее дерево зелено"
else
    bad "ложный отказ на настоящем дереве (код $rc): $out"
fi

# 2. --write в пустую копию, затем страж — зелено.
python "$TMP/scripts/tools/gen-pow5-table.py" --write "$TMP" >/dev/null 2>&1
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "651/651"; then
    ok "--write, затем страж: зелено, 651/651"
else
    bad "после --write страж красен (код $rc): $out"
fi

# 3. Одна цифра в hi строки 5^0 (0x8000000000000000) изменена.
cp "$TMP/$REL" "$TMP/table.good"
sed -i 's/0x8000000000000000, \/\/ 5^0$/0x8000000000000001, \/\/ 5^0/' "$TMP/$REL"
if cmp -s "$TMP/table.good" "$TMP/$REL"; then
    bad "фикстура случая 3 не изменила файл — строка 5^0 в hi не найдена"
else
    out=$(bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q "q=0 hi"; then
        ok "ловит одну цифру в hi и называет q=0 hi"
    else
        bad "не поймал порчу hi (код $rc): $out"
    fi
fi

# 4. То же в lo (строка 5^0 в lo — 0x0000000000000000).
cp "$TMP/table.good" "$TMP/$REL"
sed -i 's/0x0000000000000000, \/\/ 5^0$/0x0000000000000001, \/\/ 5^0/' "$TMP/$REL"
if cmp -s "$TMP/table.good" "$TMP/$REL"; then
    bad "фикстура случая 4 не изменила файл — строка 5^0 в lo не найдена"
else
    out=$(bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q "q=0 lo"; then
        ok "ловит одну цифру в lo и называет q=0 lo"
    else
        bad "не поймал порчу lo (код $rc): $out"
    fi
fi

# 5. Удалена первая строка hi (5^-342): 650 строк вместо 651.
cp "$TMP/table.good" "$TMP/$REL"
sed -i '/0xEEF453D6923BD65A, \/\/ 5^-342$/d' "$TMP/$REL"
if cmp -s "$TMP/table.good" "$TMP/$REL"; then
    bad "фикстура случая 5 не изменила файл — строка 5^-342 не найдена"
else
    out=$(bash "$G" "$TMP" 2>&1); rc=$?
    if [ "$rc" -eq 1 ] && echo "$out" | grep -q "650"; then
        ok "ловит пропавшую строку и называет 650"
    else
        bad "не поймал пропавшую строку (код $rc): $out"
    fi
fi

# 6. Таблицы нет вовсе — красно, с путём.
rm -f "$TMP/$REL"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "pow5_table.nv"; then
    ok "нет таблицы — красно и назван путь"
else
    bad "отсутствие таблицы не покраснело или путь не назван (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-pow5-table: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-pow5-table: ЕСТЬ ПРОВАЛЫ ($CASES случаев)" >&2
exit 1
