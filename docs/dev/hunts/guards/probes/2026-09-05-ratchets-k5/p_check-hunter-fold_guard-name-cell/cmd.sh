#!/usr/bin/env bash
# Проба к находке: check-hunter-fold объявлен ДОМОМ формата строки свёртки:
#   «ФОРМАТ СТРОКИ ЛЕДЖЕРА ЖИВЁТ ЗДЕСЬ (бриф и mark-страж ссылаются, второго
#    дома нет):  СВЁРНУТО | <stem-отчёта> | <модуль> | К<n> | находок N | ...»
# и краснит «строку, не разбираемую по формату (№801)».
# Код же спрашивает у поля 3 более узкое: `[a-z_]+`.
#
# На треке novac это совпадает — модули зовутся lex/parse/check/emit_c. Но
# формат ОДИН на три трека, а у двух других клетка не модуль:
#   docs/dev/hunts/guards/LEDGER.md: «КЛЕТКА | <имя-стража-без-расширения> | К<n>»
#   docs/dev/hunts/oracle/LEDGER.md: подсистема — свободное имя
# Имена стражей все через дефис (check-worktree-location), и `[a-z_]+` их
# не принимает. Свёрнутых строк на этих двух треках ещё нет ни одной, то есть
# ветка кода не исполнялась никогда.
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/check-hunter-fold.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/check-hunter-fold.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0
mkdir -p "$TMP/hunts/guards"

# stem берётся НАСТОЯЩИЙ: правило (5) требует, чтобы отчёт был в истории git.
STEM="2026-09-04-check-novac-k7"

python - "$TMP/hunts/guards/LEDGER.md" "$STEM" <<'PY'
import io, sys
p, stem = sys.argv[1], sys.argv[2]
head = u"# LEDGER (probe)\n\n"
line = u"СВЁРНУТО | %s | check-novac | К7 | находок 0 | —\n" % stem
io.open(p, "w", encoding="utf-8", newline="\n").write(head + line)
print("ledger written")
PY

printf 'max_open=9\n' > "$TMP/fold.baseline"

echo "--- artefact check ---"
if [ -s "$TMP/hunts/guards/LEDGER.md" ]; then echo "  present: hunts/guards/LEDGER.md"; else echo "TARGET MISSING" >&2; exit 99; fi
echo "--- the folded line under test (bytes) ---"
sed -n '3p' "$TMP/hunts/guards/LEDGER.md" | od -c | head -4
echo "--- the report really is in git history (rule 5) ---"
git -C "$NOVA_ROOT" log --diff-filter=A --format=%H -1 -- "docs/dev/hunts/guards/$STEM.md"

echo "--- guard ---"
NOVA_HUNTS_DIR="$TMP/hunts" NOVA_HUNTER_FOLD_BASELINE="$TMP/fold.baseline" sh "$GUARD" "$NOVA_ROOT"
rc=$?
echo "RC=$rc"
exit "$rc"
