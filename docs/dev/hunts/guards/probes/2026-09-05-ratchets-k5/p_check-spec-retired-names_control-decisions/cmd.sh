#!/usr/bin/env bash
# КОНТРОЛЬ к p_check-spec-retired-names_outside-decisions: та же строка,
# то же снятое имя, но файл лежит в spec/decisions/. Страж краснеет и
# называет адрес. Значит зелёный в парной пробе — ровно перечисление
# каталога, а не «имя не то» и не «пометка ретракции сработала».
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/check-spec-retired-names.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/check-spec-retired-names.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0
T="$TMP/tree"
mkdir -p "$T/spec/decisions"

cat > "$T/spec/decisions/01-clean.md" <<'MD'
# D1

    export external fn StringBuilder.with_capacity(n int) -> Self
MD

printf '# probe baseline: no file has any mention yet\n' > "$TMP/spec-retired.baseline"

echo "--- artefact check ---"
if [ -s "$T/spec/decisions/01-clean.md" ]; then echo "  present: spec/decisions/01-clean.md"; else echo "TARGET MISSING" >&2; exit 99; fi

echo "--- guard ---"
NOVA_RETIRED_BASELINE="$TMP/spec-retired.baseline" sh "$GUARD" "$T"
rc=$?
echo "RC=$rc"
exit "$rc"
