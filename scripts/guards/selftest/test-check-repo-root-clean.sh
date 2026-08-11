#!/usr/bin/env bash
# Самотест check-repo-root-clean.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-repo-root-clean.sh"
TMP="${TMPDIR:-/tmp}/selftest_root_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/docs/plans/wip"
    git -C "$TMP" init -q 2>/dev/null
    : > "$TMP/README.md"; : > "$TMP/nova.toml"
    : > "$TMP/docs/plans/wip/PROGRESS-pchan.md"
    git -C "$TMP" add README.md nova.toml docs/plans/wip/PROGRESS-pchan.md 2>/dev/null
}
trap 'rm -rf "$TMP"' EXIT

# 1. Чистый корень — норма.
setup
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистый корень проходит"; else bad "ложный отказ на чистом корне: $out"; fi

# 2. Чекпоинт в корне — отказ. Ровно то, что увидел владелец на GitHub.
setup
: > "$TMP/PROGRESS-pvela2.md"; git -C "$TMP" add PROGRESS-pvela2.md 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "PROGRESS-pvela2.md"; then
    ok "ловит чекпоинт в корне и называет его"
else
    bad "не поймал чекпоинт (rc=$rc): $out"
fi

# 3. Тот же файл в docs/plans/wip/ — не отказ: страж судит КОРЕНЬ, а не имя.
setup
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "тот же файл в docs/plans/wip/ не считается"; else bad "ложный отказ на wip: $out"; fi

# 4. Неотслеживаемый файл в корне не считается: локальный черновик владельца —
#    его дело, и страж в него не лезет.
setup
: > "$TMP/scratch-local.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "неотслеживаемый черновик не считается"; else bad "ложный отказ на черновике: $out"; fi

# 5. На настоящем дереве проекта страж зелёный — иначе он въезжает в гейт красным.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 6. Страж назван на странице правил.
RULES="$REAL/docs/dev/rules-for-agents.md"
if grep -q "check-repo-root-clean.sh" "$RULES" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-repo-root-clean: 6/6 ok"; exit 0; fi
echo "селфтест check-repo-root-clean: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
