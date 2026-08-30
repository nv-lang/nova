#!/bin/sh
# selftest/test-check-gate-build-order.sh — страж порядка сборки умеет краснеть.
# Живая половина + пять случаев на подставном гейте и подставных стражах.
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
GUARD="$ROOT/scripts/guards/check-gate-build-order.sh"
T="${TMPDIR:-/tmp}/selftest-gate-build-order.$$"
trap 'rm -rf "$T"' 0 2 15
rc=0

# ── живая половина: настоящее дерево обязано быть зелёным ───────────────
if ! sh "$GUARD" "$ROOT" >"$T.live" 2>&1; then
    echo "FAIL: живая половина красная на настоящем дереве:" >&2
    tail -3 "$T.live" | sed 's/^/    /' >&2
    rc=1
fi
rm -f "$T.live"

mk() {
    # $1 — номер строки вызова стража ОТНОСИТЕЛЬНО сборки: before | after
    rm -rf "$T"; mkdir -p "$T/guards"
    printf '#!/bin/sh\nNOVA="$ROOT/nova-cli/target/release/nova.exe"\necho ok\n' \
        > "$T/guards/check-needs-binary.sh"
    printf '#!/bin/sh\necho "no binary here"\n' > "$T/guards/check-pure-text.sh"
    # страж, трогающий ЧУЖОЙ бинарь (nova-lsp): носителем НЕ является
    printf '#!/bin/sh\nB="$ROOT/target/release/nova-lsp.exe"\necho ok\n' \
        > "$T/guards/check-lsp-only.sh"
    {
        echo '#!/bin/sh'
        echo '#   1) cargo build --release (nova-cli)   <- ЭТО КОММЕНТАРИЙ ШАПКИ'
        echo 'guard "$ROOT/scripts/guards/check-pure-text.sh"'
        echo 'guard "$ROOT/scripts/guards/check-lsp-only.sh"'
        [ "$1" = "before" ] && echo 'guard "$ROOT/scripts/guards/check-needs-binary.sh"'
        echo 'step push "cargo build --release"'
        echo '( cd "$ROOT/nova-cli" && cargo build --release )'
        [ "$1" = "after" ] && echo 'guard "$ROOT/scripts/guards/check-needs-binary.sh"'
    } > "$T/gate.sh"
}

# ── здоровье: вызов НИЖЕ сборки — зелёный ───────────────────────────────
mk after
if ! NOVA_GATE_FILE="$T/gate.sh" NOVA_GUARDS_DIR="$T/guards" sh "$GUARD" "$ROOT" >"$T.o0" 2>&1; then
    echo "FAIL: вызов ниже сборки покраснел — страж ложнит:" >&2
    tail -2 "$T.o0" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 1: вызов ВЫШЕ сборки ───────────────────────────────────────
mk before
if NOVA_GATE_FILE="$T/gate.sh" NOVA_GUARDS_DIR="$T/guards" sh "$GUARD" "$ROOT" >"$T.o1" 2>&1; then
    echo "FAIL: страж, которому нужен бинарь, вызван выше сборки и ПРОШЁЛ — №813 может вернуться" >&2
    rc=1
elif ! grep -q "check-needs-binary" "$T.o1"; then
    echo "FAIL: красный есть, но носитель не назван поимённо:" >&2
    tail -2 "$T.o1" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 2: барьер пойман на КОММЕНТАРИИ шапки ──────────────────────
# Если разбор снова возьмёт строку-комментарий (как в первой редакции), то
# «ниже барьера» окажется весь файл, и подделка 1 пройдёт. Проверяем прямо:
# гейт, где ЕДИНСТВЕННОЕ упоминание сборки — комментарий, обязан быть отказом
# «барьер взять неоткуда», а не молчаливым зелёным.
rm -rf "$T"; mkdir -p "$T/guards"
printf '#!/bin/sh\nNOVA="$ROOT/nova-cli/target/release/nova.exe"\n' > "$T/guards/check-needs-binary.sh"
{ echo '#!/bin/sh'
  echo '#   1) cargo build --release (nova-cli)'
  echo 'guard "$ROOT/scripts/guards/check-needs-binary.sh"'
} > "$T/gate.sh"
if NOVA_GATE_FILE="$T/gate.sh" NOVA_GUARDS_DIR="$T/guards" sh "$GUARD" "$ROOT" >"$T.o2" 2>&1; then
    echo "FAIL: гейт, где сборка только в комментарии, дал ЗЕЛЁНЫЙ — барьер снова пойман на шапке" >&2
    rc=1
elif ! grep -q "барьер" "$T.o2"; then
    echo "FAIL: красный есть, но не про барьер:" >&2; tail -2 "$T.o2" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 3: чужой бинарь (nova-lsp) не считается носителем ──────────
# Обратная сторона: если образец снова начнёт ловить `nova-lsp` подстрокой,
# честный страж будет объявлен носителем и шаг переставят зря.
mk before
rm -f "$T/guards/check-needs-binary.sh"   # оставляем ТОЛЬКО lsp-стража выше сборки
if ! NOVA_GATE_FILE="$T/gate.sh" NOVA_GUARDS_DIR="$T/guards" sh "$GUARD" "$ROOT" >"$T.o3" 2>&1; then
    echo "FAIL: страж, трогающий nova-lsp, объявлен носителем — образец ловит подстроку:" >&2
    tail -2 "$T.o3" | sed 's/^/    /' >&2
    rc=1
fi

# ── подделка 4: гейта нет — судить нечего, но молчать нельзя ────────────
if NOVA_GATE_FILE="$T/nosuch.sh" sh "$GUARD" "$ROOT" >"$T.o4" 2>&1; then
    echo "FAIL: отсутствующий гейт дал зелёный" >&2
    rc=1
fi

[ "$rc" -eq 0 ] && echo "test-check-gate-build-order ok: четыре подделки покраснели, здоровый порядок и чужой бинарь зелёные, живая половина зелёная"
exit "$rc"
