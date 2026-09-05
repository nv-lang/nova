#!/usr/bin/env bash
# Проба к находке: check-no-mojibake объявляет «Текст, испорченный UTF-8 как
# CP1251, НЕ ПОПАДАЕТ В ДЕРЕВО», и честно перечисляет, ЧЕГО НЕ ЛОВИТ (три
# пункта: порча без этих букв; английский текст; ремонтный инструмент).
# Ни в шапке, ни в летописи базы не сказано, что смотрятся только ДЕСЯТЬ
# расширений и что каталог .claude пропускается целиком.
#
# Здесь порча кладётся в места, которых исключения не называли:
#   .claude/commands/flow.md     — SKIP_DIRS
#   notes.txt                    — расширения нет в EXTS
#   .claude/settings.json        — оба сразу
#   editors/vscode/package.json  — расширения нет в EXTS
# Ожидание по шапке: красный. Замер — в verdict.txt.
#
# Байты подписи В САМОМ ФАЙЛЕ ПРОБЫ НЕ ЛЕЖАТ (иначе проба, переехав в дерево,
# сама подняла бы храповик: cmd.sh имеет расширение .sh, а оно в EXTS) —
# файлы генерирует python по u-эскейпам.
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
# classic "em dash read as cp1251": U+0432 U+0402 U+201D -- the middle code
# point is exactly one of the guard's signature letters.
rot = u"\u0432\u0402\u201d"
line = u"pravilo " + rot + u" otpravlyaj to, chto proveryal\n"
targets = [
    ".claude/commands/flow.md",
    "notes.txt",
    ".claude/settings.json",
    "editors/vscode/package.json",
]
for rel in targets:
    p = os.path.join(tmp, "tree", *rel.split("/"))
    d = os.path.dirname(p)
    if not os.path.isdir(d):
        os.makedirs(d)
    io.open(p, "w", encoding="utf-8", newline="\n").write(line)
    print("created " + rel)
PY

echo "--- artefact check ---"
missing=0
for f in ".claude/commands/flow.md" "notes.txt" ".claude/settings.json" "editors/vscode/package.json"; do
    if [ -f "$TMP/tree/$f" ]; then echo "  present: $f"; else echo "  MISSING: $f"; missing=1; fi
done
[ "$missing" -eq 0 ] || { echo "TARGETS MISSING - probe invalid" >&2; exit 99; }

printf 'mojibake_lines=0\n' > "$TMP/mojibake.baseline"

echo "--- guard ---"
NOVA_MOJIBAKE_BASELINE="$TMP/mojibake.baseline" bash "$GUARD" "$TMP/tree"
rc=$?
echo "RC=$rc"
exit "$rc"
