#!/bin/sh
# Самотест check-novac-selftest-proves-red.sh — мутационного стража (П16).
#
# ПОДЛОЖКА. У стража есть шов $2 — override каталога стражей, поэтому
# настоящие стражи не нужны: каждый случай строит крошечный каталог из
# пары «страж + его самотест» во временной папке. Дёшево (никаких оракулов)
# и повторяемо.
#
# ГЛАВНОЕ, ЧТО ТУТ ДОКАЗЫВАЕТСЯ: страж отличает ДОКАЗЫВАЮЩИЙ самотест от
# СЛЕПОГО. Слепой — тот, который остаётся зелёным над заглушкой; именно он
# обязан краснеть. И отдельно: страж обязан ВЕРНУТЬ подменённого стража на
# место, иначе он сам портит дерево.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-selftest-proves-red.sh"
T="${TMPDIR:-/tmp}/novac-prove-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# --- фикстура: каталог стражей с одной парой -----------------------------
# $1 — каталог, $2 — имя стража, $3 — «proving» или «blind»
mkfix() {
    d="$1"; n="$2"; kind="$3"
    mkdir -p "$d/selftest"
    # страж: краснеет, если в файле-мишени есть слово BAD
    cat > "$d/check-novac-$n.sh" <<'EOF'
#!/bin/sh
export LC_ALL=C
TARGET="$2"
[ -f "$TARGET" ] || { echo "ok: судить нечего"; exit 0; }
if grep -q BAD "$TARGET"; then echo "FAIL — найдено BAD" >&2; exit 1; fi
echo "ok: чисто"
exit 0
EOF
    if [ "$kind" = proving ]; then
        # самотест с ОБЕИМИ сторонами: чистый вход зелёный, грязный — красный
        cat > "$d/selftest/test-check-novac-$n.sh" <<EOF
#!/bin/sh
export LC_ALL=C
G="$d/check-novac-$n.sh"
TT="\${TMPDIR:-/tmp}/fix.\$\$"; mkdir -p "\$TT"; trap 'rm -rf "\$TT"' 0
echo GOOD > "\$TT/clean"; echo BAD > "\$TT/dirty"
sh "\$G" x "\$TT/clean" >/dev/null 2>&1 || { echo "чистый покраснел" >&2; exit 1; }
sh "\$G" x "\$TT/dirty" >/dev/null 2>&1 && { echo "грязный НЕ покраснел" >&2; exit 1; }
echo ok
exit 0
EOF
    else
        # СЛЕПОЙ самотест: только зелёная сторона
        cat > "$d/selftest/test-check-novac-$n.sh" <<EOF
#!/bin/sh
export LC_ALL=C
G="$d/check-novac-$n.sh"
TT="\${TMPDIR:-/tmp}/fix.\$\$"; mkdir -p "\$TT"; trap 'rm -rf "\$TT"' 0
echo GOOD > "\$TT/clean"
sh "\$G" x "\$TT/clean" >/dev/null 2>&1 || { echo "чистый покраснел" >&2; exit 1; }
echo ok
exit 0
EOF
    fi
}

# --- 1. доказывающий самотест — зелёный ----------------------------------
D1="$T/g1"; mkfix "$D1" alpha proving
if run "$D1"; then
    if grep -q "доказали красноту мутацией: 1" "$T/out"; then
        ok "доказывающий самотест — зелёный, число напечатано [$(cat "$T/out")]"
    else
        bad "зелёный, но без числа доказанных [$(cat "$T/out")]"
    fi
else
    bad "доказывающий самотест покраснел: $(cat "$T/err")"
fi

# --- 2. СЛЕПОЙ самотест — красный, с именем ------------------------------
D2="$T/g2"; mkfix "$D2" beta blind
if run "$D2"; then
    bad "слепой самотест ПРОШЁЛ — страж не ловит главный случай"
else
    if grep -q "beta" "$T/err" && grep -q "ЗАГЛУШКОЙ" "$T/err"; then
        ok "слепой самотест — красный, виновник назван: $(head -1 "$T/err")"
    else
        bad "красный, но без имени виновника или без объяснения [$(cat "$T/err")]"
    fi
fi

# --- 3. страж без самотеста — красный ------------------------------------
D3="$T/g3"; mkfix "$D3" gamma proving
rm -f "$D3/selftest/test-check-novac-gamma.sh"
if run "$D3"; then
    bad "страж без самотеста ПРОШЁЛ (П16 п.5)"
else
    grep -q "gamma" "$T/err" && ok "страж без самотеста — красный, назван" || bad "красный, но gamma не назван"
fi

# --- 4. подменённый страж ВОЗВРАЩЁН на место -----------------------------
D4="$T/g4"; mkfix "$D4" delta proving
cksum < "$D4/check-novac-delta.sh" > "$T/sum.before"
run "$D4"
cksum < "$D4/check-novac-delta.sh" > "$T/sum.after"
if cmp -s "$T/sum.before" "$T/sum.after"; then
    ok "страж возвращён после подмены (контрольная сумма совпала)"
else
    bad "страж НЕ восстановлен — мутационная проверка портит дерево"
fi
[ -f "$D4/check-novac-delta.sh.proving-backup" ] && bad "остался файл .proving-backup" || ok "временный backup убран"

# --- 5. групповой самотест без одноимённого стража — не красный ----------
D5="$T/g5"; mkfix "$D5" eps proving
cp "$D5/selftest/test-check-novac-eps.sh" "$D5/selftest/test-check-novac-quartet.sh"
if run "$D5"; then
    grep -q "групповых (без одноимённого стража): 1" "$T/out" \
        && ok "групповой самотест сосчитан отдельно, не красный" \
        || bad "групповой не сосчитан [$(cat "$T/out")]"
else
    bad "групповой самотест сделал стража красным: $(cat "$T/err")"
fi

# --- 6. пустой каталог и NOVAC_PROVE=0 -----------------------------------
run "$T/absent"
grep -q "судить нечего" "$T/out" && ok "нет каталога — судить нечего" || bad "нет каталога: ждали «судить нечего» [$(cat "$T/out")]"
NOVAC_PROVE=0 sh "$G" "$ROOT" "$D2" > "$T/out" 2>&1
if [ $? -eq 0 ] && grep -q "NOVAC_PROVE=0" "$T/out"; then
    ok "NOVAC_PROVE=0 пропускает даже слепой каталог (дешёвая выборка)"
else
    bad "NOVAC_PROVE=0 не отработал [$(cat "$T/out")]"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-selftest-proves-red ok: все случаи, включая слепой самотест и восстановление"
    exit 0
fi
exit 1
