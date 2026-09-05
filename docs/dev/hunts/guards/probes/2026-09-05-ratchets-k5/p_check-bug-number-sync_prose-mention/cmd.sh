#!/usr/bin/env bash
# Проба к находке: check-bug-number-sync объявляет в шапке
#   «каждый НОВЫЙ [M-...]-маркер в backlog-followups.md обязан ИМЕТЬ № в
#    221.1-bug-sweep.md (нулевая толерантность: к релизу все баги нумерованы)»
# и печатает вердикт «все новые маркеры НУМЕРОВАНЫ в 221.1».
# Код же спрашивает у дерева другое: встречается ли ГОЛОЕ ИМЯ маркера
# где угодно в тексте 221.1 (`grep -oE 'M-[a-z0-9_.-]+' "$SWEEP"`), без
# всякой связи с номером записи.
#
# Часть 1: подставное дерево — маркер упомянут в ПРОЗЕ, номера у него нет.
# Часть 2: замер на НАСТОЯЩЕМ дереве — сколько таких маркеров уже живёт.
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/check-bug-number-sync.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/check-bug-number-sync.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0
mkdir -p "$TMP/tree/docs/plans"

cat > "$TMP/tree/docs/plans/backlog-followups.md" <<'MD'
# backlog
| [M-probe-k5-no-number] | something deferred |
MD

# Ни одной строки-записи реестра: только упоминание имени в прозе.
cat > "$TMP/tree/docs/plans/221.1-bug-sweep.md" <<'MD'
# registry
Prose line, no row and no number here: see M-probe-k5-no-number for context.
MD

echo "--- artefact check ---"
for f in backlog-followups.md 221.1-bug-sweep.md; do
    if [ -s "$TMP/tree/docs/plans/$f" ]; then echo "  present: $f"; else echo "TARGET MISSING: $f" >&2; exit 99; fi
done
echo "--- 221.1 content (no numbered row anywhere) ---"
cat "$TMP/tree/docs/plans/221.1-bug-sweep.md"

echo "--- guard ---"
bash "$GUARD" "$TMP/tree"
rc=$?
echo "RC=$rc"

echo
echo "--- part 2: measurement on the REAL tree (read-only) ---"
python "$HERE/measure_real_tree.py" "$NOVA_ROOT"
exit "$rc"
