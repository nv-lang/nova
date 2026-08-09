#!/usr/bin/env bash
# Селфтест scripts/guards/check-nova-expect-ratchet.sh (Plan 262 Part Б).
#
# Страж без селфтеста не работает — тот же урок, что у check-no-accumulation.sh
# / check-registry-entry-shape.sh. Проверяем ОБА направления: ловит рост числа
# неразмеченных негативных фикстур И не краснеет ложно на уже-размеченных или
# на файлах, которые вообще не являются негативными фикстурами — иначе страж
# станет шумом и его отключат.
#
# Работаем в ВРЕМЕННОМ каталоге (не git-репозиторий — страж не трогает git).
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-nova-expect-ratchet.sh"
FAILED=0

ok()  { echo "  ok: $1"; }
bad() { echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

CONF="$TMP/spec_tests_conformance"
mkdir -p "$CONF/neg"
BASE_FILE="$TMP/ratchet.baseline"

# Фикстура-негатив БЕЗ построчной пометки.
cat > "$CONF/neg/unpinned_neg.nv" <<'EOF'
// EXPECT_COMPILE_ERROR E_SOMETHING
module neg.unpinned_neg
fn main() {
    bad_expr
}
EOF

# 1. База 0, одна неразмеченная фикстура -> рост, обязан покраснеть.
echo 'unpinned_neg_fixtures=0' > "$BASE_FILE"
out=$(NOVA_EXPECT_RATCHET_BASELINE="$BASE_FILE" NOVA_EXPECT_RATCHET_DIR="$CONF" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'ВЫРОСЛО'; then
    ok "ловит рост неразмеченных фикстур (код 1)"
else
    bad "НЕ поймал рост (код $rc): $out"
fi

# 2. Та же фикстура учтена в базе -> зелено (храповик держит долг, не запрещает его).
echo 'unpinned_neg_fixtures=1' > "$BASE_FILE"
out=$(NOVA_EXPECT_RATCHET_BASELINE="$BASE_FILE" NOVA_EXPECT_RATCHET_DIR="$CONF" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'роста .* нет'; then
    ok "не краснеет на долге в пределах базы (код 0)"
else
    bad "ложное срабатывание на долге в базе (код $rc): $out"
fi

# 3. ПОЗИТИВНАЯ (не негативная) фикстура — нет EXPECT_COMPILE_ERROR вовсе — не
#    считается, даже если в файле случайно встретится текст "nova:expect".
cat > "$CONF/pos_ok.nv" <<'EOF'
module conf.pos_ok
fn main() {
    print("ok")
}
EOF
echo 'unpinned_neg_fixtures=1' > "$BASE_FILE"
out=$(NOVA_EXPECT_RATCHET_BASELINE="$BASE_FILE" NOVA_EXPECT_RATCHET_DIR="$CONF" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -qE 'пометки 1 '; then
    ok "позитивная фикстура без EXPECT_COMPILE_ERROR не считается (по-прежнему 1)"
else
    bad "позитивная фикстура ошибочно учтена (код $rc): $out"
fi

# 4. Размечаем неразмеченную фикстуру построчной пометкой -> счётчик падает до
#    0 ДАЖЕ на базе 1 (долг снизился, не рост) — страж не должен требовать
#    держать долг искусственно.
cat > "$CONF/neg/unpinned_neg.nv" <<'EOF'
// EXPECT_COMPILE_ERROR E_SOMETHING
module neg.unpinned_neg
fn main() {
    // nova:expect E_SOMETHING -- намеренно, тест стража
    bad_expr
}
EOF
out=$(NOVA_EXPECT_RATCHET_BASELINE="$BASE_FILE" NOVA_EXPECT_RATCHET_DIR="$CONF" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && echo "$out" | grep -q 'СНИЗИЛСЯ'; then
    ok "размеченная построчно фикстура больше не считается долгом (СНИЗИЛСЯ)"
else
    bad "разметка не сняла фикстуру со счёта (код $rc): $out"
fi

# 5. Новая ВТОРАЯ неразмеченная фикстура поверх базы 0 (после снижения долга
#    до 0 в сценарии 4, обновляем базу до 0) -> обязана покраснеть -- храповик
#    держит НОВОЕ дно, а не старый потолок.
echo 'unpinned_neg_fixtures=0' > "$BASE_FILE"
cat > "$CONF/neg/another_unpinned_neg.nv" <<'EOF'
// EXPECT_COMPILE_ERROR E_OTHER
module neg.another_unpinned_neg
fn main() {
    also_bad
}
EOF
out=$(NOVA_EXPECT_RATCHET_BASELINE="$BASE_FILE" NOVA_EXPECT_RATCHET_DIR="$CONF" bash "$G" "$TMP" 2>&1); rc=$?
if [ "$rc" -eq 1 ] && echo "$out" | grep -q 'ВЫРОСЛО'; then
    ok "новая неразмеченная фикстура красит гейт даже после снижения базы (код 1)"
else
    bad "новая неразмеченная фикстура НЕ поймана после снижения базы (код $rc): $out"
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-nova-expect-ratchet: 5/5 ok"
    exit 0
fi
echo "селфтест check-nova-expect-ratchet: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
