#!/bin/sh
# Самотест стража scripts/guards/check-test-fixture-coverage.sh (реестр 221.1 №399).
#
# Доказывает ВСЕ заявленные свойства (правило владельца: страж без самотеста
# не работает; самотест обязан ловить нарушение И не давать ложняка):
#   (A1) rule5 ЛОВИТ:    новый E_*-код без neg-фикстуры — красный, называет код.
#   (A2) rule5 НЕ ЛОЖНИТ: тот же код с neg-фикстурой (EXPECT_COMPILE_ERROR) — зелёный.
#   (B1) rule1 ЛОВИТ:    строка реестра сменилась на ✅ЗАКРЫТ без ссылки на .nv — красный, номер строки.
#   (B2) rule1 НЕ ЛОЖНИТ: та же строка с .nv-ссылкой — зелёный.
#   (C1) bonus ЛОВИТ:    маркер ✅ЗАКРЫТ в реестре, но **OPEN как ПЕРВИЧНЫЙ маркер в backlog — печатает WARN с именем маркера (non-blocking по решению — см. шапку стража: вердикт по конкретному багу требует ручного триажа, не механики).
#   (C2) bonus НЕ ЛОЖНИТ: тот же маркер в backlog упомянут лишь МИМОХОДОМ в чужой записи (не первичный) — без WARN.
#   (C3) bonus НЕ ЛОЖНИТ: маркер ✅ЗАКРЫТ и в backlog как ✅РЕШЕНО (тоже закрыт) — без WARN.
#
# Части A/B — git-репы во временном каталоге (diff-base нужен для rule5/rule1).
# Часть C — обычные файлы, без git (bonus не diff-based).
# Настоящую репу НЕ трогает; коммиты — с локальными GIT_AUTHOR_*/GIT_COMMITTER_*
# env (репа = временная, глобальный git config не трогается).
set -u
GUARD="$(cd "$(dirname "$0")/.." && pwd)/check-test-fixture-coverage.sh"
[ -f "$GUARD" ] || { echo "SELFTEST FAIL: страж не найден: $GUARD" >&2; exit 1; }

TMP="${TMPDIR:-/tmp}/tfc_selftest_$$"
rm -rf "$TMP"
FAIL=0

commit_env() {
    GIT_AUTHOR_NAME=selftest GIT_AUTHOR_EMAIL=selftest@test.local \
    GIT_COMMITTER_NAME=selftest GIT_COMMITTER_EMAIL=selftest@test.local \
    git "$@"
}

# =========================================================================
# Части A/B: git-репа для rule5 (neg-фикстура) + rule1 (регресс-фикстура).
# =========================================================================
REPO="$TMP/repo"
mkdir -p "$REPO/compiler-codegen/src" "$REPO/spec_tests/conformance/neg" "$REPO/std" "$REPO/docs/plans"

cat > "$REPO/compiler-codegen/src/foo.rs" <<'EOF'
fn check() -> Result<(), String> {
    Err("E_OLD_CODE".to_string())
}
EOF

cat > "$REPO/docs/plans/221.1-bug-sweep.md" <<'EOF'
# План 221.1 — тестовый реестр
Легенда: ✅ закрыт · 🟠 открыт

| 1 | категория | `[M-selftest-marker]` — тестовая находка | 🟠 открыт, K3 |
EOF
: > "$REPO/docs/plans/backlog-followups.md"

(
    cd "$REPO" || exit 1
    git init -q
    git add -A
    commit_env commit -q -m base
)
BASE_SHA="$(cd "$REPO" && git rev-parse HEAD)"

# --- (A1)/(B1): ВВОДИМ ОБА нарушения одновременно (новый код без фикстуры +
#     строка реестра закрыта без .nv-ссылки), НЕ коммитим (диффим против working tree).
cat > "$REPO/compiler-codegen/src/foo.rs" <<'EOF'
fn check() -> Result<(), String> {
    Err("E_OLD_CODE".to_string())?;
    Err("E_FOO_BAR".to_string())
}
EOF
cat > "$REPO/docs/plans/221.1-bug-sweep.md" <<'EOF'
# План 221.1 — тестовый реестр
Легенда: ✅ закрыт · 🟠 открыт

| 1 | категория | `[M-selftest-marker]` — тестовая находка | ✅ ЗАКРЫТ 2026-08-06 (без фикстуры) |
EOF

OUT="$(sh "$GUARD" "$REPO" "$BASE_SHA" 2>&1)"
RC=$?
if [ "$RC" -eq 0 ]; then
    echo "SELFTEST FAIL (A1/B1): страж НЕ покраснел на новом коде без фикстуры + закрытой строке без .nv" >&2
    FAIL=1
fi
if ! printf '%s' "$OUT" | grep -q "E_FOO_BAR"; then
    echo "SELFTEST FAIL (A1): страж не назвал код E_FOO_BAR в выводе" >&2
    FAIL=1
fi
if ! printf '%s' "$OUT" | grep -qE '№1\b'; then
    echo "SELFTEST FAIL (B1): страж не назвал номер строки №1 в выводе" >&2
    FAIL=1
fi

# --- (A2)/(B2): ЧИНИМ оба нарушения — neg-фикстура на E_FOO_BAR + .nv-ссылка в строке.
cat > "$REPO/spec_tests/conformance/neg/selftest_probe_neg.nv" <<'EOF'
// EXPECT_COMPILE_ERROR E_FOO_BAR

module neg.selftest_probe

fn probe() -> int => 1
EOF
cat > "$REPO/docs/plans/221.1-bug-sweep.md" <<'EOF'
# План 221.1 — тестовый реестр
Легенда: ✅ закрыт · 🟠 открыт

| 1 | категория | `[M-selftest-marker]` — тестовая находка | ✅ ЗАКРЫТ 2026-08-06 (фикстура spec_tests/conformance/neg/selftest_probe_neg.nv) |
EOF

OUT2="$(sh "$GUARD" "$REPO" "$BASE_SHA" 2>&1)"
RC2=$?
if [ "$RC2" -ne 0 ]; then
    echo "SELFTEST FAIL (A2/B2): страж ЛОЖНИТ на фикстуре+ссылке, которые чинят оба нарушения" >&2
    echo "$OUT2" >&2
    FAIL=1
fi

# =========================================================================
# Часть C: bonus (registry_backlog_divergence) — файлы без git, diff-base не нужен.
# =========================================================================
CDIR="$TMP/bonus"
mkdir -p "$CDIR/docs/plans"

# --- (C1): маркер ЗАКРЫТ в реестре, но ПЕРВИЧНЫЙ **OPEN в backlog — красный.
cat > "$CDIR/docs/plans/221.1-bug-sweep.md" <<'EOF'
# Реестр
| 1 | cat | `[M-bonus-marker]` — находка | ✅ ЗАКРЫТ 2026-08-06 (окно X) |
EOF
cat > "$CDIR/docs/plans/backlog-followups.md" <<'EOF'
| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-bonus-marker]` | **OPEN 2026-08-04 (наблюдение).** Ещё не тронуто. | floating | P2 |
EOF
OUT3="$(sh "$GUARD" "$CDIR" 2>&1)"
RC3=$?
if [ "$RC3" -ne 0 ]; then
    echo "SELFTEST FAIL (C1): bonus ошибочно ПАДАЕТ (должен быть non-blocking WARN, не FAIL) — код выхода $RC3" >&2
    FAIL=1
fi
if ! printf '%s' "$OUT3" | grep -q "WARN.*M-bonus-marker\|M-bonus-marker"; then
    echo "SELFTEST FAIL (C1): bonus НЕ поймал расхождение ✅ЗАКРЫТ/OPEN — нет WARN с именем маркера M-bonus-marker в выводе" >&2
    FAIL=1
fi

# --- (C2): тот же закрытый маркер упомянут в backlog ТОЛЬКО мимоходом (не
#     первичный маркер записи, а внутри чужого текста) — НЕ должно краснеть.
cat > "$CDIR/docs/plans/backlog-followups.md" <<'EOF'
| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-other-marker]` | **OPEN 2026-08-04.** Родня `[M-bonus-marker]` (уже закрыт отдельно, см. реестр). | floating | P2 |
EOF
OUT4="$(sh "$GUARD" "$CDIR" 2>&1)"
RC4=$?
if [ "$RC4" -ne 0 ] || printf '%s' "$OUT4" | grep -q "M-bonus-marker"; then
    echo "SELFTEST FAIL (C2): bonus ЛОЖНИТ на мимоходном упоминании закрытого маркера в чужой backlog-записи" >&2
    echo "$OUT4" >&2
    FAIL=1
fi

# --- (C3): маркер ЗАКРЫТ и в реестре, и в backlog (как РЕШЕНО) — НЕ должно краснеть.
cat > "$CDIR/docs/plans/backlog-followups.md" <<'EOF'
| Маркер | Суть | Home | Pri |
|---|---|---|---|
| `[M-bonus-marker]` | **РЕШЕНО 2026-08-06.** Синхронизировано с реестром. | floating | P2 |
EOF
OUT5="$(sh "$GUARD" "$CDIR" 2>&1)"
RC5=$?
if [ "$RC5" -ne 0 ] || printf '%s' "$OUT5" | grep -q "M-bonus-marker"; then
    echo "SELFTEST FAIL (C3): bonus ЛОЖНИТ, когда backlog тоже считает маркер закрытым" >&2
    echo "$OUT5" >&2
    FAIL=1
fi

rm -rf "$TMP"

if [ "$FAIL" -ne 0 ]; then
    echo "selftest check-test-fixture-coverage: FAIL (см. сообщения выше)" >&2
    exit 1
fi
echo "selftest check-test-fixture-coverage: OK (rule5 ловит/не лжёт, rule1 ловит/не лжёт, bonus ловит/не лжёт x2)"
exit 0
