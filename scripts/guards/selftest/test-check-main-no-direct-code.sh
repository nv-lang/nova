#!/usr/bin/env bash
# Самотест scripts/guards/check-main-no-direct-code.sh — обе стороны.
#
# Всё во ВРЕМЕННОЙ репе (git init в mktemp): страж судит индекс, ветку,
# MERGE_HEAD и то, главное ли дерево, — и каждое из четырёх условий здесь
# включается и выключается по отдельности. Настоящая репа не трогается.
# Последняя клетка исполняет scripts/githooks/pre-commit во временной репе:
# страж обязан быть ПОДКЛЮЧЁН, а не только существовать.

set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-main-no-direct-code.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

export GIT_AUTHOR_NAME=selftest GIT_AUTHOR_EMAIL=selftest@example.invalid
export GIT_COMMITTER_NAME=selftest GIT_COMMITTER_EMAIL=selftest@example.invalid
unset NOVA_MAIN_DIRECT_CODE

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

R="$TMP/repo"
mkdir -p "$R"
git -C "$R" init -q -b main
git -C "$R" config core.autocrlf false
mkdir -p "$R/std/src" "$R/docs/plans" "$R/spec"
echo base > "$R/std/src/a.nv"; echo base > "$R/docs/plans/x.md"
git -C "$R" add -A
git -C "$R" -c core.hooksPath=/dev/null commit -q -m base

# Страж читает только ИНДЕКС, поэтому сброс — один `read-tree HEAD`, а не
# reset+checkout+clean: git на Windows стоит десятые секунды на вызов, и
# тройной сброс делал самотест минутным (замер 73с против бюджета 120с).
reset_index() { git -C "$R" read-tree HEAD; }
stage() { mkdir -p "$(dirname "$R/$1")"; echo "$RANDOM" >> "$R/$1"; git -C "$R" add -- "$1"; }

# expect <имя> <rc> [подстрока] — зовёт стража на $DIR (по умолчанию $R)
expect() {
    _name="$1"; _want="$2"; _needle="${3:-}"
    _out="$(bash "$GUARD" "${DIR:-$R}" 2>&1)"; _rc=$?
    if [ "$_rc" != "$_want" ]; then bad "$_name (ждал rc=$_want, получил $_rc): $(printf '%s' "$_out" | head -1)"
    elif [ -n "$_needle" ] && ! printf '%s' "$_out" | grep -qF -- "$_needle"; then bad "$_name (нет '$_needle')"
    else ok "$_name"; fi
}

echo "== main главного дерева =="
for p in compiler-codegen/src/x.rs compiler-codegen/nova_rt/x.c nova-cli/src/x.rs \
         nova-lsp/src/x.rs novac/src/x.nv std/src/a.nv; do
    reset_index; stage "$p"
    expect "код ($p) на main -> FAIL" 1 "FAIL"
done

reset_index; git -C "$R" rm -q --cached std/src/a.nv
expect "удаление кода на main -> FAIL" 1 "FAIL"

reset_index; stage docs/plans/x.md; stage spec/y.md; stage scripts/guards/z.sh; stage spec_tests/conformance/t.nv
expect "доки/спека/стражи/фикстуры на main -> ok" 0

reset_index; stage std/srcx/a.nv; stage docs/std/src/a.md
expect "похожие, но не кодовые пути -> ok" 0

reset_index; stage std/src/a.nv; stage docs/plans/x.md
expect "код вперемешку с доками -> FAIL" 1 "FAIL"
NOVA_MAIN_DIRECT_CODE="hotfix of the gate #1234" expect "ключ с номером -> ok, причина напечатана" 0 "hotfix of the gate #1234"
NOVA_MAIN_DIRECT_CODE=1 expect "ключ без номера -> FAIL" 1 "#NNNN"

echo "== слияние =="
git -C "$R" rev-parse HEAD > "$R/.git/MERGE_HEAD"
expect "код на main при MERGE_HEAD -> ok" 0
rm -f "$R/.git/MERGE_HEAD"

echo "== другая ветка =="
reset_index; git -C "$R" checkout -q -b feature; stage std/src/a.nv
expect "код на другой ветке главного дерева -> ok" 0

echo "== worktree =="
reset_index
git -C "$R" worktree add -q "$TMP/wt" main 2>/dev/null
( cd "$TMP/wt" && echo x >> std/src/a.nv && git add -- std/src/a.nv )
DIR="$TMP/wt" expect "код на main в worktree -> ok" 0
git -C "$R" worktree remove --force "$TMP/wt" 2>/dev/null
git -C "$R" checkout -q main

echo "== подключение: pre-commit =="
hook_setup() {
    reset_index
    mkdir -p "$R/scripts/githooks" "$R/scripts/guards"
    cp "$ROOT/scripts/githooks/pre-commit" "$R/scripts/githooks/pre-commit"
    cp "$GUARD" "$R/scripts/guards/"
}
hook_setup; stage std/src/a.nv
( cd "$R" && sh scripts/githooks/pre-commit >/dev/null 2>&1 ); _rc=$?
if [ "$_rc" = 1 ]; then ok "pre-commit зовёт стража: код на main остановлен"; else bad "pre-commit: rc=$_rc (ждал 1)"; fi
hook_setup; stage docs/plans/x.md
( cd "$R" && sh scripts/githooks/pre-commit >/dev/null 2>&1 ); _rc=$?
if [ "$_rc" = 0 ]; then ok "pre-commit: доки на main проходят"; else bad "pre-commit доки: rc=$_rc (ждал 0)"; fi

TOTAL=$((PASS+FAIL))
if [ "$FAIL" -ne 0 ]; then echo "test-check-main-no-direct-code: FAIL -- $FAIL/$TOTAL" >&2; exit 1; fi
echo "test-check-main-no-direct-code ok: $PASS/$TOTAL"
exit 0
