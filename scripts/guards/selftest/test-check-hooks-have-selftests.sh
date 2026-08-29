#!/usr/bin/env bash
# Селфтест scripts/guards/check-hooks-have-selftests.sh.
#
# Обе стороны: ловит хук без самотеста и НЕ краснит, когда тест есть — включая
# случай, ради которого правило написано со звёздочкой: один хук, покрытый
# НЕСКОЛЬКИМИ тестами по темам (у `guard-git.py` их три).
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-hooks-have-selftests.sh"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
H="$TMP/scripts/claude-hooks"
S="$H/selftest"
mk() { rm -rf "$TMP/scripts"; mkdir -p "$S"; }

# 1. хук с самотестом — зелено.
mk
printf '#\n' > "$H/guard-alpha.py"
printf '#\n' > "$S/test-guard-alpha.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "хук с самотестом — зелено"; else bad "ложный отказ: $out"; fi

# 2. хук без самотеста — красно, и назван поимённо.
mk
printf '#\n' > "$H/guard-alpha.py"
printf '#\n' > "$H/guard-beta.py"
printf '#\n' > "$S/test-guard-alpha.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "guard-beta"; then
    ok "хук без самотеста — красно, и назван"
else
    bad "не поймал хук без самотеста (код $rc): $out"
fi

# 3. один хук, НЕСКОЛЬКО тестов по темам — зелено (звёздочка в правиле).
mk
printf '#\n' > "$H/guard-git.py"
printf '#\n' > "$S/test-guard-git-commit-scope.py"
printf '#\n' > "$S/test-guard-git-powershell.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then
    ok "несколько тестов на один хук — зелено"
else
    bad "тесты по темам обязаны засчитываться (код $rc): $out"
fi

# 4. ЧУЖОЙ тест не засчитывается за хук: `test-guard-gitlab` не покрывает
#    `guard-git`… а вот наоборот — покрывает, и это осознанная цена префикса.
mk
printf '#\n' > "$H/guard-zeta.py"
printf '#\n' > "$S/test-guard-other.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "guard-zeta"; then
    ok "посторонний тест за самотест не сходит"
else
    bad "тест с другим именем не должен закрывать хук (код $rc): $out"
fi

# 5. каталога самотестов нет вовсе — красно (а не «зелено, тестов не нашли»).
mk
rm -rf "$S"
printf '#\n' > "$H/guard-alpha.py"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ]; then
    ok "нет каталога самотестов — красно, а не тихо зелено"
else
    bad "исчезнувший каталог тестов обязан краснеть (код $rc): $out"
fi

# 6. хуков нет вовсе — зелено, судить нечего.
mk
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "хуков нет — зелено"; else bad "пустой набор хуков краснеть не должен: $out"; fi

# 7. каталога хуков нет — зелено (страж не падает на чужом дереве).
rm -rf "$TMP/scripts"
out=$(bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "каталога хуков нет — зелено"; else bad "страж не должен падать без каталога: $out"; fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-hooks-have-selftests: 7/7 ok"; exit 0; fi
echo "селфтест check-hooks-have-selftests: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
