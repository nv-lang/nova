#!/usr/bin/env bash
# Самотест check-clean-checkout-build.sh.
#
# Страж собирает флагман во ВРЕМЕННОМ дереве из HEAD — настоящая сборка идёт
# минуты и требует компилятора, поэтому самотест проверяет НЕ сборку, а решения,
# которые страж принимает вокруг неё: их и ломают правкой. Сказано прямо, чтобы
# никто не считал, будто зелёный самотест доказывает работу сборочного пути.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-clean-checkout-build.sh"
TMP="${TMPDIR:-/tmp}/selftest_cleanprobe_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

rm -rf "$TMP"; mkdir -p "$TMP"
trap 'rm -rf "$TMP"' EXIT

# 1. Не git-репозиторий — зелено и молча. Страж не обязан судить о том, чего
#    не может проверить; красить гейт за это значило бы учить его обходить.
mkdir -p "$TMP/notgit"
out=$(bash "$G" "$TMP/notgit" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "не git-репозиторий"; then
    ok "не-git каталог пропускается"
else
    bad "не-git каталог должен пропускаться (rc=$rc): $out"
fi

# 2. Нет каталога — отказ с внятным сообщением, а не тихий ноль.
out=$(bash "$G" "$TMP/nosuchdir" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && echo "$out" | grep -q "нет каталога"; then
    ok "несуществующий корень — отказ"
else
    bad "несуществующий корень должен отвергаться (rc=$rc): $out"
fi

# 3. Git-репозиторий без собранного компилятора — отказ, а НЕ пропуск.
#    Это важнее, чем кажется: страж, который молча зеленеет, когда бинаря нет,
#    неотличим от стража, который ничего не проверяет (реестр 221.1 №475).
mkdir -p "$TMP/repo"
git -C "$TMP/repo" init -q 2>/dev/null
git -C "$TMP/repo" -c user.name=t -c user.email=t@t commit -q --allow-empty -m init 2>/dev/null
out=$(bash "$G" "$TMP/repo" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && echo "$out" | grep -q "нет бинаря"; then
    ok "без собранного компилятора — отказ, не пропуск"
else
    bad "отсутствие бинаря должно отвергаться (rc=$rc): $out"
fi

# 4. Страж назван на странице правил — иначе `check-rules-page-complete`
#    покраснеет, и узнается это только на гейте, через сорок минут.
RULES="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)/docs/dev/rules-for-agents.md"
if grep -q "check-clean-checkout-build.sh" "$RULES" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-clean-checkout-build: 4/4 ok"; exit 0; fi
echo "селфтест check-clean-checkout-build: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
