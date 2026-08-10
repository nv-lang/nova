#!/usr/bin/env bash
# Селфтест scripts/guards/check-rules-page-complete.sh.
#
# Обе стороны: ловит неназванного стража и НЕ краснит, когда все названы.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-rules-page-complete.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/docs/dev"

printf '#!/bin/sh\n' > "$TMP/scripts/guards/check-alpha.sh"
printf '#!/bin/sh\n' > "$TMP/scripts/guards/check-beta.sh"

# 1. Оба названы — зелено.
printf '# Правила\n\n| check-alpha | не даёт A |\n| check-beta | не даёт B |\n' > "$TMP/docs/dev/rules-for-agents.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "все стражи названы — проходит"; else bad "ложный отказ: $out"; fi

# 2. Один не назван — красно, с его именем.
printf '# Правила\n\n| check-alpha | не даёт A |\n' > "$TMP/docs/dev/rules-for-agents.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "check-beta"; then ok "ловит неназванного стража"; else bad "не поймал (код $rc): $out"; fi

# 3. Нет самой страницы — красно (иначе правило исчезает вместе с файлом).
rm -f "$TMP/docs/dev/rules-for-agents.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "ловит отсутствие страницы правил"; else bad "не поймал отсутствие страницы (код $rc): $out"; fi

# 4. Стражей нет вовсе — зелено (не падать на пустоте).
printf '# Правила\n' > "$TMP/docs/dev/rules-for-agents.md"
rm -f "$TMP/scripts/guards/"*.sh
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "пустой набор стражей не краснит"; else bad "ложный отказ на пустом наборе (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-rules-page-complete: 4/4 ok"; exit 0; fi
echo "селфтест check-rules-page-complete: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
