#!/bin/sh
# Самотест check-guard-honesty.sh.
#
# Доказывает мутацией, что страж ловит все три формы «вердикт есть, проверки
# нет», и — отдельно — что он НЕ краснеет на здоровом коде. Последнее здесь не
# формальность: первая редакция этого стража покраснела на правильно
# экранированном апострофе, в том числе в собственном тексте. Ложная краснота
# ровно так же обесценивает гейт, как пропущенная поломка.
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-guard-honesty.sh"
T="${TMPDIR:-/tmp}/guard-honesty-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ──────────────────────────────
if sh "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
    if grep -q "^check-guard-honesty ok:" "$T/out"; then
        ok "живое дерево — зелёный со строкой ok:"
    else
        bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
    fi
else
    bad "живое дерево красное: [$(head -n 2 "$T/err")]"
fi

# ── 2. слепота на Linux — красный ────────────────────────────────────────
mkdir -p "$T/blind/scripts/guards"
cat > "$T/blind/scripts/guards/check-x.sh" <<'SH'
#!/bin/sh
ORACLE="$ROOT/nova-cli/target/release/nova.exe"
[ -f "$ORACLE" ] || exit 0
SH
if sh "$G" "$T/blind" > "$T/o2" 2> "$T/e2"; then
    bad "файл, знающий только .exe, прошёл: [$(head -n 1 "$T/o2")]"
else
    grep -q "знает только" "$T/e2" \
        && ok "слепота на Linux — красный" \
        || bad "красный, но не про слепоту: [$(head -n 1 "$T/e2")]"
fi

# ── 3. запасное имя рядом — зелёный (правило про слепоту, не про букву) ──
mkdir -p "$T/seeing/scripts/guards"
cat > "$T/seeing/scripts/guards/check-y.sh" <<'SH'
#!/bin/sh
ORACLE="$ROOT/nova-cli/target/release/nova.exe"
[ -f "$ORACLE" ] || ORACLE="$ROOT/nova-cli/target/release/nova"
SH
if sh "$G" "$T/seeing" > "$T/o3" 2>&1; then
    ok "файл с запасным именем законен"
else
    bad "файл, знающий оба имени, покраснел: [$(head -n 2 "$T/o3")]"
fi

# ── 4. НЕэкранированный апостроф в сообщении — красный ───────────────────
mkdir -p "$T/tick/scripts/guards"
printf '#!/bin/sh\necho "smells like `date` here"\n' > "$T/tick/scripts/guards/check-z.sh"
if sh "$G" "$T/tick" > "$T/o4" 2> "$T/e4"; then
    bad "неэкранированный апостроф прошёл: [$(head -n 1 "$T/o4")]"
else
    grep -q "выполнит его как команду" "$T/e4" \
        && ok "апостроф, выполняемый оболочкой, — красный" \
        || bad "красный, но не про апостроф: [$(head -n 1 "$T/e4")]"
fi

# ── 5. ЭКРАНИРОВАННЫЙ апостроф — зелёный (ловушка первой редакции) ───────
mkdir -p "$T/esc/scripts/guards"
printf '#!/bin/sh\necho "the door is \\`nova update\\` and nothing else"\n' > "$T/esc/scripts/guards/check-w.sh"
if sh "$G" "$T/esc" > "$T/o5" 2>&1; then
    ok "экранированный апостроф законен — страж не краснеет на здоровом"
else
    bad "экранированный апостроф покраснел (ловушка первой редакции): [$(head -n 2 "$T/o5")]"
fi

# ── 6. пустой scripts — красный: судить нечего там, где судить обязано ───
mkdir -p "$T/empty/scripts"
if sh "$G" "$T/empty" > "$T/o6" 2> "$T/e6"; then
    bad "дерево без единого .sh прошло: [$(head -n 1 "$T/o6")]"
else
    ok "scripts без единого .sh — красный"
fi

# ── 7. нет scripts вовсе — честное «судить нечего» ───────────────────────
mkdir -p "$T/bare"
if sh "$G" "$T/bare" > "$T/o7" 2>&1; then
    grep -q "судить нечего" "$T/o7" \
        && ok "нет scripts — судить нечего" \
        || bad "зелёный без честной формулировки: [$(head -n 1 "$T/o7")]"
else
    bad "отсутствие scripts сделано красным: [$(head -n 1 "$T/o7")]"
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
echo "test-check-guard-honesty ok: все случаи, включая ловушку экранированного апострофа"
exit 0
