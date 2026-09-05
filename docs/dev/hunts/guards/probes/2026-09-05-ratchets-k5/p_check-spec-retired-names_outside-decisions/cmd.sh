#!/usr/bin/env bash
# Проба к находке: check-spec-retired-names озаглавлен
#   «СПЕКА не смеет НАРАЩИВАТЬ упоминания имени, которое сама же сняла»
# и подробно объясняет ЕДИНСТВЕННОЕ исключение внутри спеки — «ПОЧЕМУ
# `history/` НЕ СУДИТСЯ». Код же спрашивает только про spec/decisions:
#   SPEC_REL="${NOVA_SPEC_REL:-spec/decisions}"
# Остальные 13 файлов каталога spec/ (в том числе .ru.md, нормативная форма
# по конвенции проекта) не смотрятся, и об этом в шапке нет ни слова.
#
# Часть 1: подставное дерево — снятое имя в spec/<файл>.ru.md.
# Часть 2: замер на настоящем дереве — сколько таких мест уже есть.
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
Nothing retired here.
MD

# Снятое имя ТРЕБУЕТСЯ как живое, безо всякой пометки о ретракции.
cat > "$T/spec/effects.ru.md" <<'MD'
# effects

    export external fn StringBuilder.with_capacity(n int) -> Self
MD

# Своя база: пустая (только летопись), значит порог по всем файлам ноль.
printf '# probe baseline: no file has any mention yet\n' > "$TMP/spec-retired.baseline"

echo "--- artefact check ---"
for f in spec/decisions/01-clean.md spec/effects.ru.md; do
    if [ -s "$T/$f" ]; then echo "  present: $f"; else echo "TARGET MISSING: $f" >&2; exit 99; fi
done
echo "--- retired name in the tree ---"
grep -rn "with_capacity" "$T" | sed "s|$T|<tree>|"

echo "--- guard ---"
NOVA_RETIRED_BASELINE="$TMP/spec-retired.baseline" sh "$GUARD" "$T"
rc=$?
echo "RC=$rc"

echo
echo "--- part 2: measurement on the REAL tree (read-only) ---"
echo "mentions of the retired name in spec/ OUTSIDE spec/decisions/:"
( cd "$NOVA_ROOT" && grep -rn "with_capacity" spec/ --include='*.md' | grep -v '^spec/decisions/' | wc -l )
( cd "$NOVA_ROOT" && grep -rn "with_capacity" spec/ --include='*.md' | grep -v '^spec/decisions/' | head -6 )
exit "$rc"
