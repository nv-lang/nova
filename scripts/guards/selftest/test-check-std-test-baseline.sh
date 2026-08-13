#!/usr/bin/env bash
# Самотест check-std-test-baseline.sh.
#
# Страж зовёт настоящий `nova test std/src` — минуты. Гонять его в самотесте
# нельзя (самотест обязан укладываться в минуту, №558), поэтому подсовываем
# ПОДДЕЛЬНЫЙ `nova`: скрипт, печатающий заранее заданный вывод. Проверяем
# РАЗБОР и ВЕРДИКТ, а не сам прогон тестов — это и есть зона ответственности
# стража.

set -u
export LC_ALL=C

G="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/check-std-test-baseline.sh"
TMP="${TMPDIR:-/tmp}/selftest_stdbase_$$"
FAILED=0
ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

# $1 — содержимое базы, $2 — что печатает поддельный nova
setup() {
    rm -rf "$TMP"; mkdir -p "$TMP/scripts/guards" "$TMP/std/src" "$TMP/bin"
    printf '%s\n' "$1" > "$TMP/scripts/guards/std-test-fail.baseline"
    {
        echo '#!/usr/bin/env bash'
        echo 'cat <<"EOF"'
        printf '%s\n' "$2"
        echo 'EOF'
    } > "$TMP/bin/nova"
    chmod +x "$TMP/bin/nova"
}
trap 'rm -rf "$TMP"' EXIT

RUN_OK='RUN-FAIL       std/src/a/known_test  # bla
PASS: 70  FAIL: 1  SKIP: 3 (skipped)'

# 1. Отказ есть, он в базе — норма.
setup 'std/src/a/known_test   # №111 известный' "$RUN_OK"
out=$(bash "$G" "$TMP" "$TMP/bin/nova" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then ok "известный отказ проходит"; else bad "ложный отказ: $out"; fi

# 2. Отказ, которого в базе НЕТ, — красный. Ровно то, ради чего страж есть.
setup 'std/src/a/known_test   # №111 известный' \
'RUN-FAIL       std/src/a/known_test  # bla
CC-FAIL        std/src/b/fresh_test  # bla
PASS: 69  FAIL: 2  SKIP: 3 (skipped)'
out=$(bash "$G" "$TMP" "$TMP/bin/nova" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "fresh_test"; then ok "ловит отказ вне базы и НАЗЫВАЕТ его"; else bad "не поймал (rc=$rc): $out"; fi

# 3. ПОДМЕНА: столько же отказов, но ДРУГИХ. Счётчик сказал бы «1 <= 1» —
#    ровно та слепота, из-за которой сверяем имена.
setup 'std/src/a/known_test   # №111 известный' \
'RUN-FAIL       std/src/z/other_test  # bla
PASS: 70  FAIL: 1  SKIP: 3 (skipped)'
out=$(bash "$G" "$TMP" "$TMP/bin/nova" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "other_test"; then ok "ловит подмену при том же числе"; else bad "подмена прошла (rc=$rc): $out"; fi

# 4. Имя в базе без номера записи — красный: отложенный дефект без следа.
setup 'std/src/a/known_test' "$RUN_OK"
out=$(bash "$G" "$TMP" "$TMP/bin/nova" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "БЕЗ НОМЕРА"; then ok "требует номер записи у каждого имени"; else bad "имя без номера прошло (rc=$rc): $out"; fi

# 5. Нет строки итога — красный, а НЕ «наверное всё хорошо» (№475).
setup 'std/src/a/known_test   # №111 известный' 'какой-то мусор без итога'
out=$(bash "$G" "$TMP" "$TMP/bin/nova" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q "ничего не доказал"; then ok "отсутствие итога — отказ"; else bad "молча принял отсутствие итога (rc=$rc): $out"; fi

# 6. База «протухла в хорошую сторону» — не красным, а подсказкой.
setup 'std/src/a/known_test   # №111 известный
std/src/a/fixed_test   # №112 починен' "$RUN_OK"
out=$(bash "$G" "$TMP" "$TMP/bin/nova" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q "почищено"; then ok "починенное имя — подсказка, не отказ"; else bad "неверно на почищенном (rc=$rc): $out"; fi

# 7. Страж назван на странице правил.
REAL="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
if grep -q "check-std-test-baseline.sh" "$REAL/docs/dev/rules-for-agents.md" 2>/dev/null; then
    ok "страж назван на странице правил"
else
    bad "страж не назван в docs/dev/rules-for-agents.md"
fi

# 8. CI зовёт ЕГО ЖЕ, а не свой отдельный `nova test std` — иначе два гейта
#    снова разойдутся (№402/№591, ради чего страж и выносился).
WF="$REAL/.github/workflows/nova-test-regression.yml"
if [ -f "$WF" ]; then
    if grep -q "check-std-test-baseline.sh" "$WF"; then
        ok "CI зовёт того же стража"
    else
        bad "CI не зовёт check-std-test-baseline.sh — расхождение вернётся"
    fi
fi

if [ "$FAILED" -eq 0 ]; then echo "селфтест check-std-test-baseline: все проверки ok"; exit 0; fi
echo "селфтест check-std-test-baseline: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
