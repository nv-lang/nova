#!/usr/bin/env bash
# КОНТРОЛЬ к p_check-no-mojibake_skipped-ext-and-claude: ТА ЖЕ порча, тем же
# генератором, но в docs/x.md — расширение из EXTS, каталог не из SKIP_DIRS.
# Страж обязан покраснеть и краснеет. Значит зелёный в парной пробе — не
# «страж сломан» и не «признак не сработал», а ровно перечисление путей.
#
# Запуск из любого cwd:  bash cmd.sh [корень-репы-nova]
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"

find_root() {
    d="$HERE"
    while [ -n "$d" ] && [ "$d" != "/" ]; do
        [ -f "$d/scripts/guards/check-no-mojibake.sh" ] && { printf '%s' "$d"; return 0; }
        d="$(dirname "$d")"
    done
    return 1
}
NOVA_ROOT="${1:-$(find_root)}"
GUARD="$NOVA_ROOT/scripts/guards/check-no-mojibake.sh"
[ -f "$GUARD" ] || { echo "proba: guard not found; pass nova root as argv1" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' 0

python - "$TMP" <<'PY'
import io, os, sys
tmp = sys.argv[1]
rot = u"\u0432\u0402\u201d"
line = u"pravilo " + rot + u" otpravlyaj to, chto proveryal\n"
targets = ["docs/x.md"]
for rel in targets:
    p = os.path.join(tmp, "tree", *rel.split("/"))
    d = os.path.dirname(p)
    if not os.path.isdir(d):
        os.makedirs(d)
    io.open(p, "w", encoding="utf-8", newline="\n").write(line)
    print("created " + rel)
PY

echo "--- artefact check ---"
if [ -f "$TMP/tree/docs/x.md" ]; then echo "  present: docs/x.md"; else echo "TARGET MISSING - probe invalid" >&2; exit 99; fi

printf 'mojibake_lines=0\n' > "$TMP/mojibake.baseline"

echo "--- guard ---"
NOVA_MOJIBAKE_BASELINE="$TMP/mojibake.baseline" bash "$GUARD" "$TMP/tree"
rc=$?
echo "RC=$rc"
exit "$rc"
