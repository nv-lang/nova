#!/usr/bin/env bash
# Селфтест scripts/guards/check-stale-unimplemented.sh.
#
# Обе стороны обязательны: страж, краснеющий на честной пометке, будет отключён
# в первый же день — и правило умрёт вместе с ним.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-stale-unimplemented.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/spec" "$TMP/docs/plans"

mk_plan() { printf '# План %s\n\n**Статус:** %s\n' "$1" "$2" > "$TMP/docs/plans/$1-thing.md"; }

# 1. Пометка ссылается на план, который ЕЩЁ НЕ сделан — зелено (честная пометка).
mk_plan 900 '🆕 ЗАПЛАНИРОВАН 2026-01-01'
printf 'Форма `x"..."` пока не реализован —\n[Plan 900](../docs/plans/900-thing.md).\n' > "$TMP/spec/syntax.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "честная пометка не краснит"; else bad "ложный отказ: $out"; fi

# 2. Тот же текст, но план СДЕЛАН — красно, с именем файла и строкой.
mk_plan 900 '✅ РЕАЛИЗОВАН 2026-07-09'
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "spec/syntax.md"; then ok "ловит устаревшую пометку"; else bad "не поймал устаревшую пометку (код $rc): $out"; fi

# 3. Английская форма пометки — тот же результат.
printf 'The `x"..."` literal is not yet implemented —\n[Plan 900](../docs/plans/900-thing.md).\n' > "$TMP/spec/syntax.en.md"
rm -f "$TMP/spec/syntax.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "syntax.en.md"; then ok "ловит английскую форму пометки"; else bad "не поймал английскую форму (код $rc): $out"; fi

# 4. Пометка БЕЗ ссылки на план — не краснит: судить не по чему (сказано в шапке).
printf 'Регионы пока не реализованы в компиляторе.\n' > "$TMP/spec/overview.md"
rm -f "$TMP/spec/syntax.en.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "пометка без ссылки на план не краснит"; else bad "ложный отказ на пометке без плана (код $rc): $out"; fi

# 5. Ссылка на НЕСУЩЕСТВУЮЩИЙ план — красно: сама ссылка сломана.
printf 'Форма пока не реализован —\n[Plan 901](../docs/plans/901-ghost.md).\n' > "$TMP/spec/syntax.md"
rm -f "$TMP/spec/overview.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "901"; then ok "ловит ссылку на несуществующий план"; else bad "не поймал битую ссылку (код $rc): $out"; fi

# 6. Пустой spec — зелено (не падать на пустоте).
rm -f "$TMP/spec/"*.md
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "пустой spec не краснит"; else bad "ложный отказ на пустом spec (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-stale-unimplemented: 6/6 ok"; exit 0; fi
echo "селфтест check-stale-unimplemented: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
