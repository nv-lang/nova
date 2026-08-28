#!/usr/bin/env bash
# Селфтест scripts/guards/check-rules-page-complete.sh.
#
# Обе стороны: ловит неназванного стража и НЕ краснит, когда все названы.
#
# ВАЖНО ПРО БАЗУ (иначе самотест судит вместе с настоящим долгом репозитория):
# страж берёт файл базы РЯДОМ С СОБОЙ, а не в проверяемом корне — база живёт при
# страже, а корень бывает подложным. Поэтому здесь база подменяется через
# `NOVA_RULES_PAGE_BASELINE`. Без подмены самотест 2026-08-29 покраснел на
# «долг СНИЗИЛСЯ (1 < базы 41)», честно показав, что судил не то дерево.
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-rules-page-complete.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/scripts/guards" "$TMP/docs/dev"

export NOVA_RULES_PAGE_BASELINE="$TMP/base"
set_base() { printf 'no_page_ref=%s\n' "$1" > "$TMP/base"; }
set_base 0

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

# ── 5. ПИТОНОВЫЙ страж судится наравне с .sh ────────────────────────────────
#    Ровно та дыра, ради которой страж расширен 2026-08-29: до неё `check-*.py`
#    требование страницы обходили молча, и этот случай был бы зелёным.
printf '# Правила\n' > "$TMP/docs/dev/rules-for-agents.md"
printf '# -*- coding: utf-8 -*-\n' > "$TMP/scripts/guards/check-gamma.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "check-gamma"; then ok "python-страж судится наравне и назван поимённо"; else bad "python-страж пропущен (код $rc): $out"; fi

# ── 6. …и зеленеет, будучи названным ────────────────────────────────────────
printf '# Правила\n\n| check-gamma | не даёт G |\n' > "$TMP/docs/dev/rules-for-agents.md"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "названный python-страж — зелено"; else bad "ложный отказ на названном python-страже: $out"; fi

# ── 7. СКАНЕРЫ-ЯДРА не судятся: они не запреты ──────────────────────────────
#    Счёт по правилу проекта — стражи это `check-*`, а `*-scan.py` — ядра.
printf '# -*- coding: utf-8 -*-\n' > "$TMP/scripts/guards/whatever-scan.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "сканер-ядро в счёт не входит"; else bad "сканер-ядро не должен требовать строки на странице: $out"; fi
rm -f "$TMP/scripts/guards/whatever-scan.py"

# ── 8. ХРАПОВИК: долг вырос сверх базы — красно ─────────────────────────────
printf '# Правила\n' > "$TMP/docs/dev/rules-for-agents.md"
printf '# -*- coding: utf-8 -*-\n' > "$TMP/scripts/guards/check-delta.py"
set_base 1
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "2 > 1"; then ok "рост долга сверх базы — красно, с числами"; else bad "рост долга обязан краснеть (код $rc): $out"; fi

# ── 9. ХРАПОВИК: долг ровно по базе — зелено, но сказано, что долг есть ─────
set_base 2
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "старый долг"; then ok "долг ровно по базе — зелено, и долг назван"; else bad "долг по базе обязан быть зелёным и названным (код $rc): $out"; fi

# ── 10. ХРАПОВИК: долг снизился — красно (базу обязаны опустить с летописью) ─
set_base 5
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "СНИЗИЛСЯ"; then ok "снижение долга требует опустить базу"; else bad "снижение долга обязано требовать правки базы (код $rc): $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-rules-page-complete: 10/10 ok"; exit 0; fi
echo "селфтест check-rules-page-complete: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
