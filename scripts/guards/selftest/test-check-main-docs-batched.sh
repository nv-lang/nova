#!/usr/bin/env bash
# Самотест scripts/guards/check-main-docs-batched.sh — обе стороны.
#
# Всё во ВРЕМЕННОЙ репе (git init в mktemp): главное дерево или worktree,
# ветка, MERGE_HEAD и состав индекса включаются по отдельности. Настоящая репа
# не трогается. Последние клетки исполняют scripts/githooks/pre-commit во
# временной репе: страж обязан быть ПОДКЛЮЧЁН, а не только существовать.

set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-main-docs-batched.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

export GIT_AUTHOR_NAME=selftest GIT_AUTHOR_EMAIL=selftest@example.invalid
export GIT_COMMITTER_NAME=selftest GIT_COMMITTER_EMAIL=selftest@example.invalid
unset NOVA_DOCS_BATCH

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

REG=docs/plans/221.1-bug-sweep.md
R="$TMP/repo"
mkdir -p "$R/docs/plans" "$R/docs/dev/prompts" "$R/std/src"
git -C "$R" init -q -b main
git -C "$R" config core.autocrlf false
echo base > "$R/$REG"; echo base > "$R/docs/dev/prompts/integrator-handoff.md"; echo base > "$R/std/src/a.nv"
git -C "$R" add -A
git -C "$R" -c core.hooksPath=/dev/null commit -q -m base

# Страж читает только ИНДЕКС: сброс — один `read-tree HEAD` (git на Windows
# стоит десятые секунды на вызов).
reset_index() { git -C "$R" read-tree HEAD; }
stage() { mkdir -p "$(dirname "$R/$1")"; echo "$RANDOM" >> "$R/$1"; git -C "$R" add -- "$1"; }

expect() {
    _name="$1"; _want="$2"; _needle="${3:-}"
    _out="$(bash "$GUARD" "${DIR:-$R}" 2>&1)"; _rc=$?
    if [ "$_rc" != "$_want" ]; then bad "$_name (ждал rc=$_want, получил $_rc): $(printf '%s' "$_out" | head -1)"
    elif [ -n "$_needle" ] && ! printf '%s' "$_out" | grep -qF -- "$_needle"; then bad "$_name (нет '$_needle')"
    else ok "$_name"; fi
}

echo "== main главного дерева: одиночная бумага =="
for p in "$REG" scripts/guards/registry-rows.baseline docs/dev/prompts/carina-handoff.md \
         docs/dev/prompts/controller-handoff.md; do
    reset_index; stage "$p"
    expect "одиночный $p на main -> FAIL" 1 "FAIL"
done
reset_index; stage "$REG"; stage scripts/guards/registry-rows.baseline; stage docs/dev/prompts/controller-log.md
expect "реестр+база+лог контролёра, и больше ничего -> FAIL" 1 "FAIL"
reset_index; git -C "$R" rm -q --cached docs/dev/prompts/integrator-handoff.md
expect "удаление одной записки -> FAIL" 1 "FAIL"

reset_index; stage "$REG"
NOVA_DOCS_BATCH="rows #1442, #1355 and the handoff" expect "тот же реестр с ключом -> ok, опись напечатана" 0 "rows #1442, #1355 and the handoff"
NOVA_DOCS_BATCH= expect "пустой ключ — не ключ -> FAIL" 1 "FAIL"

echo "== не наш случай =="
reset_index; stage "$REG"; stage std/src/a.nv
expect "реестр вместе с кодом -> ok" 0
reset_index; stage "$REG"; stage docs/plans/274-novac.md
expect "реестр вместе с планом -> ok" 0
reset_index; stage docs/dev/prompts/sub/x-handoff.md
expect "записка в подкаталоге prompts — не из набора -> ok" 0
reset_index; stage docs/dev/prompts/delegation.md
expect "другой файл prompts — не из набора -> ok" 0
reset_index
expect "пустой индекс -> ok" 0

echo "== слияние =="
reset_index; stage "$REG"
git -C "$R" rev-parse HEAD > "$R/.git/MERGE_HEAD"
expect "одиночный реестр при MERGE_HEAD -> ok" 0
rm -f "$R/.git/MERGE_HEAD"

echo "== другая ветка и worktree =="
reset_index; git -C "$R" checkout -q -b feature; stage "$REG"
expect "одиночный реестр на другой ветке -> ok" 0
reset_index
git -C "$R" worktree add -q "$TMP/wt" main 2>/dev/null
( cd "$TMP/wt" && echo x >> "$REG" && git add -- "$REG" )
DIR="$TMP/wt" expect "одиночный реестр на main в worktree -> ok" 0
git -C "$R" worktree remove --force "$TMP/wt" 2>/dev/null
git -C "$R" checkout -q main

echo "== подключение: pre-commit =="
hook_setup() {
    reset_index
    mkdir -p "$R/scripts/githooks" "$R/scripts/guards"
    cp "$ROOT/scripts/githooks/pre-commit" "$R/scripts/githooks/pre-commit"
    cp "$GUARD" "$R/scripts/guards/"
}
hook_setup; stage "$REG"
( cd "$R" && sh scripts/githooks/pre-commit >/dev/null 2>&1 ); _rc=$?
if [ "$_rc" = 1 ]; then ok "pre-commit зовёт стража: одиночный реестр остановлен"; else bad "pre-commit: rc=$_rc (ждал 1)"; fi
hook_setup; stage "$REG"; stage std/src/a.nv
( cd "$R" && sh scripts/githooks/pre-commit >/dev/null 2>&1 ); _rc=$?
if [ "$_rc" = 0 ]; then ok "pre-commit: реестр с кодом проходит"; else bad "pre-commit реестр+код: rc=$_rc (ждал 0)"; fi

TOTAL=$((PASS+FAIL))
if [ "$FAIL" -ne 0 ]; then echo "test-check-main-docs-batched: FAIL -- $FAIL/$TOTAL" >&2; exit 1; fi
echo "test-check-main-docs-batched ok: $PASS/$TOTAL"
exit 0
