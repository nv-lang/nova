#!/usr/bin/env bash
# КОНТРОЛЬ к p_check-hunter-fold_guard-name-cell: та же строка, тот же stem,
# то же число находок — изменён ОДИН символ поля 3: дефис на подчёркивание
# (check_novac вместо check-novac). Страж проходит.
# Значит красный в парной пробе даёт не «строка неверна» и не «отчёта нет в
# истории», а ровно класс символов `[a-z_]+`, которым записан вопрос.
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
STEM="2026-09-04-check-novac-k7"

python - "$TMP/hunts/guards/LEDGER.md" "$STEM" <<'PY'
import io, sys
p, stem = sys.argv[1], sys.argv[2]
head = u"# LEDGER (probe)\n\n"
line = u"СВЁРНУТО | %s | check_novac | К7 | находок 0 | —\n" % stem
io.open(p, "w", encoding="utf-8", newline="\n").write(head + line)
print("ledger written")
PY

printf 'max_open=9\n' > "$TMP/fold.baseline"

echo "--- artefact check ---"
if [ -s "$TMP/hunts/guards/LEDGER.md" ]; then echo "  present: hunts/guards/LEDGER.md"; else echo "TARGET MISSING" >&2; exit 99; fi

echo "--- guard ---"
NOVA_HUNTS_DIR="$TMP/hunts" NOVA_HUNTER_FOLD_BASELINE="$TMP/fold.baseline" sh "$GUARD" "$NOVA_ROOT"
rc=$?
echo "RC=$rc"
exit "$rc"
