#!/bin/sh
# Самотест check-novac-seams-listed.py (№992).
#
# Доказывает мутацией: живое дерево зелёное со строкой ok:; шов, прочитанный
# стражем и не стоящий в SEAMS, — красный с именем; строка SEAMS без читателя —
# красная (мёртвая метка); настройка без умолчания `:-1` швом не считается;
# нет каталога стражей — честное «судить нечего».
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-seams-listed.py"
T="${TMPDIR:-/tmp}/novac-seams-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

CASES=0; FAILED=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); FAILED=$((FAILED+1)); echo "  FAIL: $1" >&2; }

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ──────────────────────────────
if python "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
    if grep -q "^check-novac-seams-listed ok:" "$T/out"; then
        ok "live tree is green with the ok: line"
    else
        bad "green without the ok: line [$(head -n 1 "$T/out")]"
    fi
else
    bad "live tree is red: [$(head -n 2 "$T/err")]"
fi

# a fixture gate with two seams listed
mkdir -p "$T/g"
cat > "$T/gate.sh" <<'SH'
SEAMS=""
[ "${NOVAC_ALPHA:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_ALPHA=0"
[ "${NOVAC_BETA:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_BETA=0"
SH

# ── 2. согласие: два стража читают ровно эти два шва — зелёный ──────────────
printf '[ "${NOVAC_ALPHA:-1}" = "0" ] && exit 0\n' > "$T/g/check-novac-a.sh"
printf 'x = os.environ.get("NOVAC_TIER")  # a setting, not a seam\nif "${NOVAC_BETA:-1}" == "0": pass\n' > "$T/g/check-novac-b.py"
if python "$G" "$ROOT" "$T/g" "$T/gate.sh" > "$T/o2" 2>&1; then
    grep -q "NOVAC_ALPHA, NOVAC_BETA" "$T/o2" && ok "two seams read, two listed: green and both named" \
        || bad "green but the names are not listed: [$(head -n 1 "$T/o2")]"
else
    bad "matching seams went red: [$(head -n 2 "$T/o2")]"
fi

# ── 3. шов без строки в SEAMS — красный с именем ────────────────────────────
printf '[ "${NOVAC_GAMMA:-1}" = "0" ] && exit 0\n' > "$T/g/check-novac-c.sh"
if python "$G" "$ROOT" "$T/g" "$T/gate.sh" > "$T/o3" 2> "$T/e3"; then
    bad "an unlisted seam passed: [$(head -n 1 "$T/o3")]"
else
    grep -q "NOVAC_GAMMA=0" "$T/e3" && ok "unlisted seam NOVAC_GAMMA is red and named" \
        || bad "red without naming the seam: [$(head -n 3 "$T/e3")]"
fi
rm -f "$T/g/check-novac-c.sh"

# ── 4. строка SEAMS без читателя — красная (мёртвая метка) ───────────────────
printf '[ "${NOVAC_DELTA:-1}" = "0" ] && SEAMS="$SEAMS NOVAC_DELTA=0"\n' >> "$T/gate.sh"
if python "$G" "$ROOT" "$T/g" "$T/gate.sh" > "$T/o4" 2> "$T/e4"; then
    bad "a dead SEAMS entry passed: [$(head -n 1 "$T/o4")]"
else
    grep -q "NOVAC_DELTA=0" "$T/e4" && ok "dead SEAMS entry NOVAC_DELTA is red and named" \
        || bad "red without naming the dead entry: [$(head -n 3 "$T/e4")]"
fi

# ── 5. нет каталога стражей — честное «судить нечего» ───────────────────────
if python "$G" "$ROOT" "$T/absent" "$T/gate.sh" > "$T/o5" 2>&1; then
    grep -q "ok:" "$T/o5" && ok "no guards dir: nothing to judge, said so" \
        || bad "green without the honest wording: [$(head -n 1 "$T/o5")]"
else
    bad "absence of the guards dir made red: [$(head -n 1 "$T/o5")]"
fi

if [ "$FAILED" -eq 0 ]; then echo "test-check-novac-seams-listed ok: $CASES/$CASES cases"; exit 0; fi
echo "test-check-novac-seams-listed: FAIL $FAILED of $CASES" >&2
exit 1
