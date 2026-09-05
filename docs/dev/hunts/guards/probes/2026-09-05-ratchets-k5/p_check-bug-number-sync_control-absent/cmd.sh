#!/usr/bin/env bash
# КОНТРОЛЬ к p_check-bug-number-sync_prose-mention: то же подставное дерево,
# но упоминания имени в 221.1 нет вовсе. Страж обязан покраснеть и краснеет.
# Значит зелёный в парной пробе — не «страж мёртв», а ровно вопрос
# «встречается ли имя», подставленный вместо вопроса «есть ли номер».
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

cat > "$TMP/tree/docs/plans/221.1-bug-sweep.md" <<'MD'
# registry
| 1 | subsystem | some unrelated entry |
MD

echo "--- artefact check ---"
for f in backlog-followups.md 221.1-bug-sweep.md; do
    if [ -s "$TMP/tree/docs/plans/$f" ]; then echo "  present: $f"; else echo "TARGET MISSING: $f" >&2; exit 99; fi
done

echo "--- guard ---"
bash "$GUARD" "$TMP/tree"
rc=$?
echo "RC=$rc"
exit "$rc"
