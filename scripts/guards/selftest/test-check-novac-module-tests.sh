#!/bin/sh
# Самотест check-novac-module-tests.sh.
#
# Доказывает не «страж запускается», а что он КРАСНЕЕТ ровно там, где обязан:
# на упавшем тесте, на отсутствии тестов и на прогоне без строки итога. Первое
# — сам смысл стража; второе — вырожденный случай, при котором зелёное молчание
# означало бы «модули не проверяются вовсе»; третье — реестр №645: ноль без
# строки это «не упал», а не «проверил».
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-module-tests.sh"
T="${TMPDIR:-/tmp}/novac-module-tests-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

ORACLE="$ROOT/nova-cli/target/release/nova.exe"
if [ ! -f "$ORACLE" ]; then
    MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    [ -n "$MAINROOT" ] && ORACLE="$MAINROOT/../nova-cli/target/release/nova.exe"
fi

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ───────────────────────────────
if [ -f "$ORACLE" ]; then
    if sh "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
        if grep -q "^check-novac-module-tests ok:" "$T/out"; then
            ok "живое дерево — зелёный со строкой ok:"
        else
            bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
        fi
    else
        bad "живое дерево красное: [$(head -n 2 "$T/err")]"
    fi
else
    ok "оракула нет — случай живого дерева пропущен осознанно"
fi

# ── 2. ПАДАЮЩИЙ тест — обязан быть красный ────────────────────────────────
FAKE="$T/tree/novac/src/toy"
mkdir -p "$FAKE"
cat > "$FAKE/toy_test.nv" <<'NV'
module toy

test "this test exists to fail" {
    assert(1 == 2)
}
NV
if [ -f "$ORACLE" ]; then
    if sh "$G" "$T/tree" "$ORACLE" > "$T/out2" 2> "$T/err2"; then
        bad "падающий тест не покраснел: [$(head -n 1 "$T/out2")]"
    else
        if grep -q "модульных тестов упало" "$T/err2"; then
            ok "падающий тест — красный, и назван причиной"
        else
            bad "красный, но не про упавший тест: [$(head -n 1 "$T/err2")]"
        fi
    fi
else
    ok "оракула нет — случай падающего теста пропущен осознанно"
fi

# ── 3. НИ ОДНОГО теста — красный (зелёное молчание было бы ложью) ─────────
EMPTY="$T/empty/novac/src/mod"
mkdir -p "$EMPTY"
echo "module mod" > "$EMPTY/mod.nv"
if sh "$G" "$T/empty" > "$T/out3" 2> "$T/err3"; then
    bad "дерево без тестов зелёное: [$(head -n 1 "$T/out3")]"
else
    if grep -q "ни одного" "$T/err3"; then
        ok "дерево без тестов — красный"
    else
        bad "красный, но не про отсутствие тестов: [$(head -n 1 "$T/err3")]"
    fi
fi

# ── 4. нет novac/src вовсе — честное «судить нечего» ──────────────────────
mkdir -p "$T/bare"
if sh "$G" "$T/bare" > "$T/out4" 2>&1; then
    if grep -q "судить нечего" "$T/out4"; then
        ok "нет novac/src — судить нечего"
    else
        bad "зелёный без честной формулировки: [$(head -n 1 "$T/out4")]"
    fi
else
    bad "отсутствие novac/src сделано красным: [$(head -n 1 "$T/out4")]"
fi

# ── 5. бинарь-заглушка без строки итога — красный (реестр №645) ───────────
cat > "$T/mute.sh" <<'SH'
#!/bin/sh
echo "running tests..."
exit 0
SH
chmod +x "$T/mute.sh"
if sh "$G" "$ROOT" "$T/mute.sh" > "$T/out5" 2> "$T/err5"; then
    bad "прогон без строки итога сочтён проверкой: [$(head -n 1 "$T/out5")]"
else
    if grep -q "строку итога" "$T/err5"; then
        ok "ноль без строки итога — красный"
    else
        bad "красный, но не про строку итога: [$(head -n 1 "$T/err5")]"
    fi
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
exit 0
