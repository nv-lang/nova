#!/usr/bin/env bash
# Селфтест scripts/guards/check-script-eol-pinned.py — исполняемые и построчно
# разбираемые файлы обязаны иметь закреплённые окончания строк.
#
# Страж заведён по замеру 2026-09-07: `core.autocrlf = true`, `.gitattributes`
# крепил `*.sh` и не крепил `*.py` (99 стражей), а после починки носителя замер
# класса нашёл ещё 48 незакреплённых — включая `scripts/githooks/*`, шелл-скрипты
# БЕЗ расширения, исполняемые git'ом на каждом коммите.
#
# ПРОБА НА НАСТОЯЩЕМ ДЕФЕКТЕ СНЯТА ПРИ ЗАВЕДЕНИИ: с `.gitattributes` из коммита до
# правки (`git show 996e5639a^:.gitattributes`) страж дал «FAIL — ... без
# закреплённых окончаний строк: 136», перечислив хуки среды и
# `.claude/after-compact.list`. Здесь она не воспроизводится по хэшу: селфтест не
# должен зависеть от истории репозитория.
#
# Доказываем ШЕСТЬ свойств:
#   1. На реальном дереве — зелено.
#   2. Незакреплённый `.sh` в синтетической репе — ОТКАЗ, и файл НАЗВАН.
#   3. Тот же файл после крепления в `.gitattributes` — зелено. Значит страж
#      читает атрибуты, а не сверяется с зашитым списком.
#   4. Исключённый КЛАСС (`*.md`) без крепления — зелено: проза не судится.
#   5. НЕПУСТОТА: дерево без `scripts/` и `.claude/` — ОТКАЗ, а не зелень.
#   6. Корень ВНЕ git-репозитория — ОТКАЗ по неполному ответу, а не молчаливая
#      зелень. Это ассерт «git ответил про столько же путей, сколько спросили»:
#      без него порча входа читается как «всё в порядке».
#
# Запуск: scripts/guards/selftest/test-check-script-eol-pinned.sh
# Выход: 0 — страж исправен, 1 — страж сломан.

set -uo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GUARD="$REPO_ROOT/scripts/guards/check-script-eol-pinned.py"

FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-script-eol-pinned =="

if [ ! -f "$GUARD" ]; then
    echo "  ПРОВАЛ: не найден $GUARD" >&2
    exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# --- 1: реальное дерево ----------------------------------------------------
if python "$GUARD" "$REPO_ROOT" >"$TMP/real.txt" 2>&1; then
    ok "реальное дерево зелено: $(tail -1 "$TMP/real.txt")"
else
    bad "реальное дерево краснеет: $(tail -3 "$TMP/real.txt" | tr '\n' ' ')"
fi

# --- 2..4: синтетическая репа ----------------------------------------------
mkdir -p "$TMP/repo/scripts"
git -C "$TMP/repo" init -q 2>/dev/null
printf '#!/bin/sh\necho hi\n' > "$TMP/repo/scripts/tool.sh"
printf '# doc\n' > "$TMP/repo/scripts/README.md"

if python "$GUARD" "$TMP/repo" >"$TMP/unpinned.txt" 2>&1; then
    bad "незакреплённый .sh прошёл как зелёный"
elif grep -q 'scripts/tool.sh' "$TMP/unpinned.txt"; then
    ok "незакреплённый .sh — отказ, и файл назван"
else
    bad "отказ есть, но файл не назван: $(tail -3 "$TMP/unpinned.txt" | tr '\n' ' ')"
fi

# .md лежит там же и НЕ должен попадать в отказ — класс исключён.
if grep -q 'scripts/README.md' "$TMP/unpinned.txt"; then
    bad "исключённый класс .md попал в отказ — проза судится наравне со скриптами"
else
    ok "исключённый класс (.md) в отказ не попал"
fi

printf '*.sh text eol=lf\n' > "$TMP/repo/.gitattributes"
if python "$GUARD" "$TMP/repo" >"$TMP/pinned.txt" 2>&1; then
    ok "после крепления в .gitattributes — зелено (страж читает атрибуты, а не зашитый список)"
else
    bad "крепление не помогло: $(tail -3 "$TMP/pinned.txt" | tr '\n' ' ')"
fi

# --- 5: непустота ----------------------------------------------------------
mkdir -p "$TMP/bare/docs"
git -C "$TMP/bare" init -q 2>/dev/null
echo "# nothing to judge" > "$TMP/bare/docs/x.md"
if python "$GUARD" "$TMP/bare" >"$TMP/bare.txt" 2>&1; then
    bad "дерево без scripts/ и .claude/ прошло как зелёное — страж молчит о том, чего не читал"
elif grep -q 'НИ ОДНОГО' "$TMP/bare.txt"; then
    ok "непустота: дерево без судимых каталогов — отказ с прямой причиной"
else
    bad "отказ есть, но причина не про пустоту: $(tail -2 "$TMP/bare.txt" | tr '\n' ' ')"
fi

# --- 6: корень вне git — ассерт полноты ответа -----------------------------
mkdir -p "$TMP/nogit/scripts"
printf '#!/bin/sh\n' > "$TMP/nogit/scripts/tool.sh"
if python "$GUARD" "$TMP/nogit" >"$TMP/nogit.txt" 2>&1; then
    bad "корень вне git-репозитория прошёл как зелёный — ассерт полноты ответа не работает"
elif grep -q 'git' "$TMP/nogit.txt"; then
    ok "ассерт полноты: неполный ответ git — отказ, а не молчаливая зелень"
else
    bad "отказ есть, но не про ответ git: $(tail -2 "$TMP/nogit.txt" | tr '\n' ' ')"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-script-eol-pinned: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-script-eol-pinned: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
