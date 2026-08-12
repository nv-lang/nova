#!/usr/bin/env bash
# Самотест check-no-control-chars.sh.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-no-control-chars.sh"
TMP="${TMPDIR:-/tmp}/selftest_ctl_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/scripts" "$TMP/docs"
    git -C "$TMP" init -q 2>/dev/null
    printf 'echo ok\n' > "$TMP/scripts/clean.sh"
    printf '# док\n\nтекст\n' > "$TMP/docs/clean.md"
    git -C "$TMP" add scripts/clean.sh docs/clean.md 2>/dev/null
}
trap 'rm -rf "$TMP"' EXIT

# 1. Чистое дерево — норма.
setup
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "чистое дерево проходит"; else bad "ложный отказ: $out"; fi

# 2. Литеральный ESC — отказ. Ровно случай, найденный в scripts/gate.sh.
setup
printf 'X=$(sed -e "s/\033\\[[0-9;]*m//g")\n' > "$TMP/scripts/esc.sh"
git -C "$TMP" add scripts/esc.sh 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "esc.sh"; then ok "ловит литеральный ESC и называет файл"; else bad "не поймал ESC (rc=$rc): $out"; fi

# 3. Забой (0x08) в тексте — отказ. Это и есть случай `\b`, съеденного оболочкой.
setup
printf 'grep "\010cancelled\010"\n' > "$TMP/scripts/bs.sh"
git -C "$TMP" add scripts/bs.sh 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then ok "ловит забой 0x08"; else bad "не поймал 0x08: $out"; fi

# 4. Двухсимвольная escape-последовательность — НЕ нарушение: именно так и надо.
setup
printf 'ESC=$(printf %s)\nsed -e "s/${ESC}\\[[0-9;]*m//g"\n' "'\\\\033'" > "$TMP/scripts/good.sh"
git -C "$TMP" add scripts/good.sh 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "явная последовательность \\033 не считается нарушением"; else bad "ложный отказ на каноне: $out"; fi

# 5. Табуляция и перевод строки законны.
setup
printf 'a\tb\r\n' > "$TMP/scripts/tabs.sh"
git -C "$TMP" add scripts/tabs.sh 2>/dev/null
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "табуляция/CR/LF не считаются"; else bad "ложный отказ на табуляции: $out"; fi

# 6. Неотслеживаемый файл не считается: периметр — git ls-files.
setup
printf 'x\010y\n' > "$TMP/scripts/untracked.sh"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "неотслеживаемый файл вне периметра"; else bad "ложный отказ на неотслеживаемом: $out"; fi

# 7. На настоящем дереве зелёный — иначе страж въезжает в гейт красным.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(bash "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 8. Страж назван на странице правил.
if grep -q "check-no-control-chars.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-no-control-chars: 8/8 ok"; exit 0; fi
echo "селфтест check-no-control-chars: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
