#!/bin/sh
# selftest/test-check-oracle-nesting-depth.sh — доказывает, что страж №800
# умеет КРАСНЕТЬ, а не только печатать «ok».
#
# Метод: стражу подсовывается заведомо негодный «компилятор» через
# NOVA_ORACLE_BIN — три подделки, по одной на каждый способ соврать:
#   1. молчаливо-зелёный: выходит 0 и не печатает ничего (класс №770 —
#      «молчание читается как успех»: именно так выглядел бы страж, который
#      забыл потребовать диагностику);
#   2. умирающий на стеке: печатает тот самый текст `overflowed its stack`
#      и умирает — исходное поведение оракула до фикса №800;
#   3. слишком строгий: отвергает ВСЁ, включая законную глубину 200 —
#      «фикс», поставивший предел в 8. Без этой подделки страж не отличил бы
#      починку от запрета вложенности вообще.
# Каждая обязана дать красный. Плюс живая половина: настоящий бинарь обязан
# дать зелёный — иначе самотест доказывал бы только умение краснеть.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-oracle-nesting-depth.sh"
T="${TMPDIR:-/tmp}/selftest-nesting.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0 2 15
rc=0

# ── подделка 1: молчаливо-зелёная ────────────────────────────────────────
cat > "$T/silent.sh" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod +x "$T/silent.sh"
if NOVA_ORACLE_BIN="$T/silent.sh" sh "$GUARD" "$ROOT" >"$T/out1" 2>&1; then
    echo "FAIL: молчаливо-зелёный компилятор прошёл стража — страж не требует диагностики" >&2
    rc=1
fi

# ── подделка 2: смерть на стеке ──────────────────────────────────────────
cat > "$T/crash.sh" <<'EOF'
#!/bin/sh
case "$*" in
    *--version*) echo "fake 0.0"; exit 0 ;;
esac
echo "thread 'nova-check-0' has overflowed its stack"
exit 3
EOF
chmod +x "$T/crash.sh"
if NOVA_ORACLE_BIN="$T/crash.sh" sh "$GUARD" "$ROOT" >"$T/out2" 2>&1; then
    echo "FAIL: компилятор, умирающий на стеке, прошёл стража — ровно дефект №800" >&2
    rc=1
fi
grep -q "УМЕР на стеке" "$T/out2" || {
    echo "FAIL: страж покраснел, но НЕ назвал смерть на стеке — диагноз потерян" >&2
    rc=1
}

# ── подделка 3: слишком строгий предел ───────────────────────────────────
cat > "$T/strict.sh" <<'EOF'
#!/bin/sh
case "$*" in
    *--version*) echo "fake 0.0"; exit 0 ;;
esac
echo "error: [E_NESTING_TOO_DEEP] nesting is deeper than 8 levels"
exit 1
EOF
chmod +x "$T/strict.sh"
if NOVA_ORACLE_BIN="$T/strict.sh" sh "$GUARD" "$ROOT" >"$T/out3" 2>&1; then
    echo "FAIL: компилятор, отвергающий ЗАКОННУЮ глубину 200, прошёл стража" >&2
    rc=1
fi
grep -q "законная вложенность" "$T/out3" || {
    echo "FAIL: страж покраснел, но не на законной глубине — вторая половина не работает" >&2
    rc=1
}

# ── живая половина ───────────────────────────────────────────────────────
NOVA="$ROOT/nova-cli/target/release/nova.exe"
[ -f "$NOVA" ] || NOVA="$ROOT/nova-cli/target/release/nova"
if [ -f "$NOVA" ]; then
    if ! sh "$GUARD" "$ROOT" >"$T/out4" 2>&1; then
        echo "FAIL: живая половина мертва — настоящий бинарь не проходит стража:" >&2
        tail -3 "$T/out4" | sed 's/^/    /' >&2
        rc=1
    fi
else
    echo "test-check-oracle-nesting-depth: живая половина пропущена — нет бинаря (ярус без сборки)"
fi

[ "$rc" -eq 0 ] && echo "test-check-oracle-nesting-depth ok: три подделки покраснели, живая половина зелёная"
exit "$rc"
