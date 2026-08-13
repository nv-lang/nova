#!/usr/bin/env bash
# Самотест check-no-mojibake.sh.
#
# Образцы порчи задаются ЭКРАНИРОВАННЫМИ последовательностями через python, а не
# литералами в этом файле: сам файл исключён из скана, но литерал в нём всё
# равно прошёл бы через оболочку при любой правке — то есть проверка порчи сама
# бы её и получила. Тот же приём, что и в правиле, которое она стережёт.

set -u
export LC_ALL=C

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
G="$DIR/check-no-mojibake.sh"
TMP="${TMPDIR:-/tmp}/selftest_mojibake_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

# $1 — python-выражение с содержимым файла, $2 — база
setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/docs/dev" "$TMP/scripts/guards"
    cp "$DIR/mojibake-scan.py" "$TMP/scripts/guards/"
    python -c "
import io, sys
io.open(sys.argv[1], 'w', encoding='utf-8', newline='\n').write($1)
" "$TMP/docs/dev/probe.md"
    printf 'mojibake_lines=%s\n' "$2" > "$TMP/scripts/guards/mojibake.baseline"
}
trap 'rm -rf "$TMP"' EXIT

CLEAN="u'# Обычный русский текст с кавычками «так» и тире — всё чисто.\n'"
# «РћС‚РїСЂР°РІР»СЏР№» — реальный кусок порчи из происшествия 2026-08-13.
DIRTY="u'# РћСЂРїСЂРаРІР»СЏРй текст\n'"

# 1. Чистый русский текст — норма. Кавычки «» и тире не должны краснеть:
#    первая редакция признака ловила именно их (2468 ложных).
setup "$CLEAN" 0
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистый русский текст с «» и тире проходит"; else bad "ложный отказ: $out"; fi

# 2. Настоящая порча — отказ.
setup "$DIRTY" 0
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "probe.md"; then ok "ловит порчу и НАЗЫВАЕТ файл"; else bad "не поймал (rc=$rc): $out"; fi

# 3. Храповик: порча в пределах базы — зелено.
setup "$DIRTY" 1
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "долг в пределах базы проходит"; else bad "храповик не пропускает базу: $out"; fi

# 4. Снижение — подсказка, а не отказ.
setup "$CLEAN" 5
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "СНИЗИЛСЯ"; then ok "снижение — подсказка"; else bad "снижение обработано неверно (rc=$rc): $out"; fi

# 5. На настоящем дереве зелёный.
REAL="$(cd "$DIR/../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 6. Ремонтный инструмент НЕ краснит: он хранит образцы как данные.
if [ -f "$REAL/scripts/tools/demojibake.py" ]; then
    out=$(python "$DIR/mojibake-scan.py" "$REAL" 2>/dev/null | grep -c "demojibake.py" || true)
    if [ "${out:-0}" -eq 0 ]; then ok "ремонтный инструмент исключён"; else bad "страж краснит на demojibake.py"; fi
fi

# 7. Страж назван на странице правил.
if grep -q "check-no-mojibake.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-no-mojibake: все проверки ok"; exit 0; fi
echo "селфтест check-no-mojibake: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
