#!/bin/sh
# selftest/test-check-hunter-mark.sh — страж меры охотника умеет краснеть.
# Три подделки, по одной на каждый способ соврать мере, плюс живая половина.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-hunter-mark.sh"
T="${TMPDIR:-/tmp}/selftest-hunter-mark.$$"
mkdir -p "$T/hunts"
trap 'rm -rf "$T"' 0 2 15
rc=0

# Живая половина на настоящем дереве: обязана быть зелёной.
if ! sh "$GUARD" "$ROOT" >"$T/live" 2>&1; then
    echo "FAIL: живая половина красная на настоящем дереве:" >&2
    tail -2 "$T/live" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 1: отчёт-молчун (ни находок, ни «НИЧЕГО НЕ НАШЁЛ») ─────────
cat > "$T/hunts/2026-08-30-lex-k2.md" <<'EOF'
# Охота lex x K2
Посмотрел по сторонам.
EOF
if NOVA_HUNTS_DIR="$T/hunts" sh "$GUARD" "$ROOT" >"$T/o1" 2>&1; then
    echo "FAIL: отчёт-молчун прошёл стража — молчание прочитано как успех (№770)" >&2
    rc=1
fi

# ── подделка 2: находка есть, а метка в реестре без даты ────────────────
cat > "$T/hunts/2026-08-30-lex-k2.md" <<'EOF'
НАХОДКА | K2 - lex - probe.nv - что-то не так
EOF
mkdir -p "$T/fake-root/docs/plans" "$T/fake-root/docs/dev/hunts"
cp "$T/hunts/2026-08-30-lex-k2.md" "$T/fake-root/docs/dev/hunts/"
printf '| 1 | test | НАЙДЕНО ОХОТНИКОМ без даты |\n' > "$T/fake-root/docs/plans/221.1-bug-sweep.md"
if sh "$GUARD" "$T/fake-root" >"$T/o2" 2>&1; then
    echo "FAIL: метка без даты прошла стража — числитель не считается грепом" >&2
    rc=1
fi

# ── подделка 3: меток больше, чем находок (метка руками) ────────────────
printf '| 1 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30 |\n| 2 | t | НАЙДЕНО ОХОТНИКОМ 2026-08-30 |\n' \
    > "$T/fake-root/docs/plans/221.1-bug-sweep.md"
if sh "$GUARD" "$T/fake-root" >"$T/o3" 2>&1; then
    echo "FAIL: две метки при одной находке прошли стража — метку можно ставить руками" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-hunter-mark ok: три подделки покраснели, живая половина зелёная"
exit "$rc"
