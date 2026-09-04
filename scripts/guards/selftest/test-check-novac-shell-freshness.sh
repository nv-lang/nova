#!/bin/sh
# Самотест check-novac-shell-freshness.sh — обе стороны (норма 254; план
# 274.3/F6).
#
# Живая половина (нужен оракул, ~2 сборки probe по 5–40 с):
#   1) чистое дерево — зелёный со строкой ok:;
#   2) ПОДЛОЖКА — фикстурный корень, где в копии novac/src/emit_c/shell.tpl.c
#      изменена ОДНА строка: страж обязан покраснеть. Подложка несёт только
#      шаблон; probe и оракул берутся из настоящего дерева (страж сверяет
#      «репозиторный снимок против свежей эмиссии» — подменяется именно
#      снимок). Рабочее дерево не портится: правится копия во временном
#      каталоге.
# Дешёвая половина (без оракула, всегда): ветки «судить нечего», пропажа
# сгенерированного артефакта и честная передача кода возврата генератора —
# через поддельный генератор ($2-шов стража). Красный случай есть и здесь,
# поэтому самотест не может стать зелёной пустышкой на машине без оракула.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-shell-freshness.sh"
T="${TMPDIR:-/tmp}/novac-shell-freshness-selftest.$$"
mkdir -p "$T/bins"
trap 'rm -rf "$T"' 0
fails=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$@" > "$T/out" 2> "$T/err"; }

TPL="$ROOT/novac/src/emit_c/shell.tpl.c"
ORACLE="$ROOT/nova-cli/target/release/nova.exe"
if [ ! -f "$ORACLE" ]; then
    MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    [ -n "$MAINROOT" ] && ORACLE="$MAINROOT/../nova-cli/target/release/nova.exe"
fi

# --- 1. Чистое дерево: зелёный, одна строка ok: ---------------------------
if run; then
    if grep -q "^check-novac-shell-freshness ok:" "$T/out"; then
        ok "чистое дерево — зелёный со строкой ok: [$(cat "$T/out")]"
    else
        bad "чистое дерево зелёное, но без строки ok: [$(cat "$T/out")]"
    fi
else
    bad "чистое дерево покраснело: $(cat "$T/err")"
fi

# --- 2. Подложка: одна изменённая строка шаблона — красный ----------------
if [ -f "$ORACLE" ] && [ -f "$TPL" ]; then
    FIX="$T/fix"
    mkdir -p "$FIX/novac/src/emit_c" "$FIX/novac/probe"
    cp "$ROOT/novac/probe/shell_probe.nv" "$FIX/novac/probe/shell_probe.nv" 2>/dev/null
    sed '1s|.*|/* подложка самотеста: одна строка изменена */|' "$TPL" > "$FIX/novac/src/emit_c/shell.tpl.c"
    if cmp -s "$TPL" "$FIX/novac/src/emit_c/shell.tpl.c"; then
        bad "подложка не отличается от шаблона — случай ничего не проверяет"
    elif run "$FIX"; then
        bad "подложка с изменённой строкой ПРОШЛА — страж не краснеет"
    elif grep -q "FAIL" "$T/err"; then
        ok "подложка (1 строка) — красный: $(head -1 "$T/err")"
    else
        bad "подложка красная, но без внятного FAIL в stderr"
    fi
else
    bad "случай подложки не отработал: нет оракула ($ORACLE) или шаблона — живая половина самотеста мертва"
fi

# --- 3–6. Дешёвые ветки через поддельный генератор ------------------------
mkgen() { printf '#!/bin/sh\nexit %s\n' "$1" > "$T/bins/gen.sh"; chmod +x "$T/bins/gen.sh"; }
EMPTY="$T/empty"; mkdir -p "$EMPTY"
HALF="$T/half"; mkdir -p "$HALF/novac/probe" "$HALF/novac/src/emit_c"
echo "// probe" > "$HALF/novac/probe/shell_probe.nv"
BOTH="$T/both"; mkdir -p "$BOTH/novac/src/emit_c"
echo "/* tpl */" > "$BOTH/novac/src/emit_c/shell.tpl.c"
mkgen 0

# 3. Ни шаблона, ни probe — честное «судить нечего».
if run "$EMPTY" "$T/bins/gen.sh" && grep -q "судить нечего" "$T/out"; then
    ok "пустой корень — зелёное «судить нечего»"
else
    bad "пустой корень: ждал зелёное «судить нечего», получил [$(cat "$T/out")$(cat "$T/err")]"
fi

# 4. Probe есть, сгенерированного шаблона нет — красный (артефакт пропал).
if run "$HALF" "$T/bins/gen.sh"; then
    bad "probe без шаблона прошёл — пропажа артефакта не поймана"
else
    ok "probe без шаблона — красный"
fi

# 5. Генератор говорит «оракула нет» (код 3) — зелёное «судить нечего».
mkgen 3
if run "$BOTH" "$T/bins/gen.sh" && grep -q "оракул не собран" "$T/out"; then
    ok "код 3 генератора — зелёное «судить нечего (оракул не собран)»"
else
    bad "код 3: ждал зелёное «судить нечего», получил [$(cat "$T/out")$(cat "$T/err")]"
fi

# 6. Генератор вернул 1 (расхождение) — страж обязан покраснеть, не проглотить.
mkgen 1
if run "$BOTH" "$T/bins/gen.sh"; then
    bad "код 1 генератора проглочен — страж зелёный на расхождении"
else
    ok "код 1 генератора — красный"
fi

# 7. Генератора нет — красный (сверять нечем), не тихое «ok».
if run "$BOTH" "$T/bins/absent.sh"; then
    bad "отсутствие генератора прошло зелёным"
else
    ok "отсутствие генератора — красный"
fi

if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-shell-freshness ok: $CASES/$CASES"
    exit 0
fi
echo "test-check-novac-shell-freshness: FAIL ($fails)" >&2
exit 1
