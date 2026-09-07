#!/bin/sh
# ПРОБА D — check-commit-language.sh
#
# ОБЕЩАНИЕ ШАПКИ (scripts/guards/check-commit-language.sh, дословно):
#   "ЧТО СЧИТАЕТСЯ НАРУШЕНИЕМ. Кириллица в теме или теле коммита. Исключение —
#    merge-коммиты, чью тему пишет git, и цитаты: ..."
# СТРОКА ВЕРДИКТА:
#   "check-commit-language ok: кириллицы в сообщениях после точки перехода нет"
#
# ЧТО ПРОВЕРЯЕТ НА ДЕЛЕ (тот же файл):
#   CAND=$(git -C "$ROOT" log --no-merges --format=... "$SCAN_FROM..HEAD" ...)
#                              ^^^^^^^^^^
# Merge-коммиты выброшены ЦЕЛИКОМ — и тема, и тело.
#
# ПОЧЕМУ ОГОВОРКА НЕ ДЕРЖИТ В ЭТОМ ДЕРЕВЕ. Довод исключения — «тему пишет git».
# В nova тему и тело слияния пишет РУКА, и это порядок, а не случай:
# .claude/commands/explain.md требует, чтобы коммит слияния нёс
# «# index-verified: <причина>», то есть авторский текст. Дословно из git log:
#   0ece2222d  merge p274-novac (4): the live line dated where the guard
#              actually looks, and a budget rule that now needs three conditions
#   a938da625  merge main into p283-float-parse: two guards were red only
#              because the branch was BEHIND, and the proof was running them
#              in both trees
# Ни один другой страж язык merge-сообщений не судит: check-commit-hygiene.sh
# смотрит Co-Authored-By и авторство, check-merge-message-hashes.sh — хеши.
#
# ЗАПУСК:  sh cmd.sh [КОРЕНЬ-РЕПОЗИТОРИЯ]
set -u
PYTHONIOENCODING=utf-8; export PYTHONIOENCODING
HERE=$(cd "$(dirname "$0")" && pwd)
REPO="${1:-}"
if [ -z "$REPO" ]; then
    d="$HERE"
    while [ "$d" != "/" ] && [ ! -f "$d/scripts/guards/check-commit-language.sh" ]; do
        d=$(dirname "$d")
    done
    REPO="$d"
fi
G="$REPO/scripts/guards/check-commit-language.sh"
[ -f "$G" ] || { echo "cmd.sh: не нашёл корень репозитория; передай его аргументом" >&2; exit 2; }

T="${TMPDIR:-/tmp}/probeD.$$"
rm -rf "$T"; mkdir -p "$T/scripts/guards"
git -C "$T" init -q -b main
git -C "$T" config user.email t@t
git -C "$T" config user.name t
git -C "$T" config commit.gpgsign false

MSG="$T/msg.txt"
: > "$T/base"; git -C "$T" add base >/dev/null
git -C "$T" commit -q -m "base commit in english"
git -C "$T" rev-parse HEAD > "$T/scripts/guards/commit-language.cutover"

VER="$T/verified"
run() {
    printf '%s\n' "--- $1 ---"
    rm -f "$VER"
    NOVA_COMMIT_LANG_VERIFIED="$VER" bash "$G" "$T" 2>&1
    printf 'rc=%s\n\n' "$?"
}

run "КОНТРОЛЬ 0: только английский базовый коммит -> ожидается зелёный"

# --- две ветки, РАЗНЫЕ файлы: слияние обязано пройти без конфликта ---------
git -C "$T" checkout -q -b side
: > "$T/side"; echo a > "$T/side"; git -C "$T" add side >/dev/null
git -C "$T" commit -q -m "side work in english"
git -C "$T" checkout -q main
: > "$T/mainf"; echo b > "$T/mainf"; git -C "$T" add mainf >/dev/null
git -C "$T" commit -q -m "main work in english"

# рукописное РУССКОЕ сообщение слияния
python - "$MSG" <<'PYEOF'
import io, sys
io.open(sys.argv[1], "w", encoding="utf-8", newline="\n").write(
    u"слияние side: два стража краснели только потому, что ветка отставала\n"
    u"\n"
    u"Тело тоже по-русски, целиком рукописное.\n"
    u"# index-verified: проверено\n")
PYEOF

git -C "$T" merge --no-ff --no-edit -F "$MSG" side >/dev/null 2>&1
MERGE_SHA=$(git -C "$T" rev-parse HEAD)
echo "коммит слияния (родителей должно быть 2):"
git -C "$T" log -1 --format='  %h  parents=[%p]  subject=%s' | cat
echo
run "ДЕФЕКТ: merge-коммит с рукописным РУССКИМ сообщением"

# --- КОНТРОЛЬ 1: ТО ЖЕ сообщение обычным (одно-родительским) коммитом -----
echo c > "$T/plain"; git -C "$T" add plain >/dev/null
git -C "$T" commit -q -F "$MSG"
echo "обычный коммит (родитель должен быть 1):"
git -C "$T" log -1 --format='  %h  parents=[%p]  subject=%s' | cat
echo
run "КОНТРОЛЬ 1: ТО ЖЕ сообщение обычным коммитом -> ожидается КРАСНЫЙ"

# --- КОНТРОЛЬ 2: снять обычный коммит, слияние оставить -------------------
git -C "$T" reset -q --hard "$MERGE_SHA"
run "КОНТРОЛЬ 2: обычный коммит снят, слияние осталось -> снова зелёный"

echo "--- собственный самотест стража (какие случаи он знает) ---"
bash "$G" --selftest 2>&1
printf 'rc=%s\n' "$?"

rm -rf "$T"
