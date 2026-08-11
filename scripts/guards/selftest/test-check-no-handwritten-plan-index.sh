#!/usr/bin/env bash
# Самотест check-no-handwritten-plan-index.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-no-handwritten-plan-index.sh"
TMP="${TMPDIR:-/tmp}/selftest_planidx_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/docs/plans" "$TMP/docs/dev/prompts"
    printf '# Планы Nova\n\nСтатусов здесь нет намеренно.\n' > "$TMP/docs/plans/README.md"
    printf '<!-- AUTO-GENERATED -->\n\n# Статусы планов\n' > "$TMP/docs/plans/STATUS.md"
    printf '# Промпт\n\nРегенерирует рантайм.\n' > "$TMP/docs/dev/prompts/regen.md"
}
trap 'rm -rf "$TMP"' EXIT

# 1. Чистое дерево — норма.
setup
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистое дерево проходит"; else bad "ложный отказ: $out"; fi

# 2. Строки-планы в README — отказ (ровно та таблица, что вычищена 2026-07-21).
setup
printf '| 221 | релиз | в работе |\n| 268 | эффекты | план |\n' >> "$TMP/docs/plans/README.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "строки-планы"; then ok "ловит таблицу планов в README"; else bad "не поймал таблицу (rc=$rc): $out"; fi

# 3. Статус-значки в README — отказ.
setup
printf '\nПлан 221 — %s ЗАКРЫТ.\n' "$(printf '\xe2\x9c\x85')" >> "$TMP/docs/plans/README.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "статус-значки"; then ok "ловит статус-значки"; else bad "не поймал значки (rc=$rc): $out"; fi

# 4. STATUS.md без шапки AUTO-GENERATED — отказ: сводка обязана называть себя.
setup
printf '# Статусы планов\n' > "$TMP/docs/plans/STATUS.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "AUTO-GENERATED"; then ok "ловит сводку без шапки"; else bad "не поймал шапку (rc=$rc): $out"; fi

# 5. Промпт, предписывающий вести сводку — отказ. Это и есть №612: правило
#    запрещало, а механизм три недели приглашал.
setup
printf '# Промпт\n\nСинхронизировать сводную таблицу всех планов docs/plans/README.md.\n' > "$TMP/docs/dev/prompts/update-plans-readme.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "приглашает"; then ok "ловит промпт, воскрешающий практику"; else bad "не поймал промпт (rc=$rc): $out"; fi

# 6. Кириллица под LC_ALL=C: проверка не должна опираться на классы [а-я].
#    При первом запуске страж промолчал именно на этом — случай в самотесте.
setup
printf '# Промпт\n\nВести сводную таблицу планов.\n' > "$TMP/docs/dev/prompts/x.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "кириллический литерал ловится под LC_ALL=C"; else bad "кириллица снова не ловится: $out"; fi

# 7. На настоящем дереве зелёный.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 8. Страж назван на странице правил.
if grep -q "check-no-handwritten-plan-index.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-no-handwritten-plan-index: 8/8 ok"; exit 0; fi
echo "селфтест check-no-handwritten-plan-index: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
