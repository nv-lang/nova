#!/usr/bin/env bash
# Самотест check-plan-duplication.py.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-plan-duplication.py"
TMP="${TMPDIR:-/tmp}/selftest_dup_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

SENT="Мономорфизация статического обобщённого метода делается поэлементно, а обходной путь снят целиком."

setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/docs/plans"
    : > "$TMP/empty.baseline"
}
trap 'rm -rf "$TMP"' EXIT
run() { NOVA_PLAN_DUP_BASELINE="$TMP/empty.baseline" python "$G" "$TMP" 2>&1; }

# 1. План без повторов — норма.
setup
printf '# План 1\n\n## Зачем\n\n%s\n\n## Итог\n\nДругой текст, достаточно длинный чтобы его считали предложением.\n' "$SENT" > "$TMP/docs/plans/001-a.md"
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "план без повторов проходит"; else bad "ложный отказ: $out"; fi

# 2. Один и тот же текст в ДВУХ разделах — отказ.
setup
printf '# План 2\n\n## Зачем\n\n%s\n\n## Итог\n\n%s\n' "$SENT" "$SENT" > "$TMP/docs/plans/002-b.md"
out=$(run); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "002-b.md"; then ok "ловит повтор между разделами"; else bad "повтор не пойман (rc=$rc): $out"; fi

# 3. Повтор ВНУТРИ одного раздела — не нарушение: там перечисление или таблица.
setup
printf '# План 3\n\n## Зачем\n\n%s\n\n%s\n' "$SENT" "$SENT" > "$TMP/docs/plans/003-c.md"
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "повтор внутри одного раздела не считается"; else bad "ложный отказ внутри раздела: $out"; fi

# 4. Короткая строка не считается: порог 60 значимых символов.
setup
printf '# План 4\n\n## Зачем\n\nКороткая строка.\n\n## Итог\n\nКороткая строка.\n' > "$TMP/docs/plans/004-d.md"
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "короткая строка ниже порога не считается"; else bad "ложный отказ на короткой строке: $out"; fi

# 5. База ГАСИТ известный повтор, но ровно на своё число.
setup
printf '# План 5\n\n## Зачем\n\n%s\n\n## Итог\n\n%s\n' "$SENT" "$SENT" > "$TMP/docs/plans/005-e.md"
printf 'docs/plans/005-e.md 1\n' > "$TMP/base1.baseline"
out=$(NOVA_PLAN_DUP_BASELINE="$TMP/base1.baseline" python "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "база гасит известный повтор"; else bad "база не сработала: $out"; fi

# 6. Рост сверх базы — отказ. Это и есть смысл храповика.
setup
printf '# План 6\n\n## Зачем\n\n%s\n\n## Проба\n\n%s\n\n## Итог\n\n%s\n' "$SENT" "$SENT" "$SENT" > "$TMP/docs/plans/006-f.md"
printf 'docs/plans/006-f.md 1\n' > "$TMP/base1.baseline"
out=$(NOVA_PLAN_DUP_BASELINE="$TMP/base1.baseline" python "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "базы 1"; then ok "рост сверх базы ловится"; else bad "рост не пойман (rc=$rc): $out"; fi

# 7. Реестр 221.1 вне периметра: его строки ОБЯЗАНЫ нести одинаковые формулы.
setup
printf '# Реестр\n\n## A\n\n%s\n\n## B\n\n%s\n' "$SENT" "$SENT" > "$TMP/docs/plans/221.1-bug-sweep.md"
out=$(run); rc=$?
if [ "$rc" -eq 0 ]; then ok "реестр 221.1 вне периметра"; else bad "реестр попал в периметр: $out"; fi

# 8. На настоящем дереве зелёный.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out=$(python "$G" "$REAL" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "на настоящем дереве зелёный"; else bad "красный на настоящем дереве: $out"; fi

# 9. Страж назван на странице правил.
if grep -q "check-plan-duplication.py" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-plan-duplication: 9/9 ok"; exit 0; fi
echo "селфтест check-plan-duplication: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
