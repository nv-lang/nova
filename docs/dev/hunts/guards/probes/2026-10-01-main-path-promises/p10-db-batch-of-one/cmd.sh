#!/usr/bin/env bash
# p10 -- check-main-docs-batched.sh: the batch key turns a ONE-path paper commit
# into "пакетный", and any non-empty text counts as the inventory.
#
# Header promise, line 2: "одиночный «бумажный» коммит в main запрещён."
# Header, lines 28-30: "Пакетный коммит — ключом NOVA_DOCS_BATCH="<что в пакете>"
# (текст обязателен и печатается: пакет без описи неотличим от одиночного)."
# The neighbour keys of the same family refuse a bare "1" for exactly that reason
# (check-main-no-direct-code.sh line 22-23: "Голое «1» — отказ: обход без причины
# неотличим от забывчивости").
#
# Built in work/ (throw-away repo, main tree on main).
# A: control -- registry alone, no key                       -> expect FAIL
# B: registry alone, NOVA_DOCS_BATCH=1                       -> ? (and the ok-line wording)
# C: registry alone, NOVA_DOCS_BATCH=" " (one space)         -> ?
# Run from the repository root: bash <this-dir>/cmd.sh
export LC_ALL=C
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${NOVA_REPO_ROOT:-$PWD}"
G="$ROOT/scripts/guards/check-main-docs-batched.sh"
[ -s "$G" ] || { echo "MISSING $G"; exit 2; }
W="$HERE/work"; rm -rf "$W"; mkdir -p "$W/docs/plans"
git -C "$W" init -q -b main; git -C "$W" config core.autocrlf false
echo base > "$W/docs/plans/221.1-bug-sweep.md"; git -C "$W" add -- docs/plans/221.1-bug-sweep.md
git -c user.name=probe -c user.email=probe@example.invalid -C "$W" -c core.hooksPath=/dev/null commit -q -m base
echo row >> "$W/docs/plans/221.1-bug-sweep.md"; git -C "$W" add -- docs/plans/221.1-bug-sweep.md
[ "$(git -C "$W" diff --cached --name-only)" = docs/plans/221.1-bug-sweep.md ] || { echo "MISSING staged registry"; exit 2; }
unset NOVA_DOCS_BATCH
echo "=== A. control: registry alone, no key"
bash "$G" "$W" 2>&1; echo "rc=$?"
echo "=== B. registry alone, NOVA_DOCS_BATCH=1"
NOVA_DOCS_BATCH=1 bash "$G" "$W" 2>&1; echo "rc=$?"
echo "=== C. registry alone, NOVA_DOCS_BATCH=' '"
NOVA_DOCS_BATCH=' ' bash "$G" "$W" 2>&1; echo "rc=$?"
