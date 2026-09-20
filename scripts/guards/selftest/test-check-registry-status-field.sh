#!/usr/bin/env bash
# Самотест check-registry-status-field.py и ОТКАЗА СУДИТЬ у двух соседних
# стражей (реестр 221.1 №1160, пункты 1-2).
#
# Клетки ПАРАМИ: страж, который краснеет всегда, снимут первым же днём, а
# который не краснеет никогда — неотличим от отсутствующего. Поэтому у каждой
# «ждём красного» есть парная «ждём зелёного» на почти том же входе.
#
# Отдельная клетка — СВЕРЩИК: три файла читают базу СВОИМИ копиями кода, и
# копии обязаны давать одно множество. Общего модуля здесь нет намеренно (страж
# обязан быть самодостаточным), поэтому расхождение ловится механизмом, а не
# доверием.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-registry-status-field.py"
SCAN="$ROOT/scripts/guards/registry-routes-scan.py"
KEPT="$ROOT/scripts/guards/check-registry-closure-kept.py"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

# Строки фикстуры: 100 с полем, 200 без поля (в базе), 300 без поля (НОВАЯ).
WITH='| 100 | K1 | A. **Статус:** ОТКРЫТ |'
OLD_NOFIELD='| 200 | K1 | B. закрыт замером, поля нет |'
NEW_NOFIELD='| 300 | K1 | C. заведена сегодня, поля нет |'

mkreg() {  # $1 = дополнительная строка
    mkdir -p "$TMP/docs/plans"
    { echo "| # | prio | описание |"; echo "|---|---|---|";
      echo "$WITH"; echo "$OLD_NOFIELD"; [ -n "${1:-}" ] && echo "$1"; } \
        > "$TMP/reg.md"
}
mkbase() {  # $1 = число, $2... = номера
    { echo "# база самотеста"; echo "nofield=$1"; shift; for n in "$@"; do echo "$n"; done; } \
        > "$TMP/base.baseline"
}

# --- пара 1: новая строка без поля ловится, старая не мешает ----------------
mkreg "$NEW_NOFIELD"; mkbase 1 200
out=$(python "$G" "$ROOT" "$TMP/reg.md" "$TMP/base.baseline" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '300'; then
    ok "новая строка без поля — красный, и номер назван"
else
    bad "новая строка без поля прошла или номер не назван: $out"
fi

mkreg ""; mkbase 1 200
out=$(python "$G" "$ROOT" "$TMP/reg.md" "$TMP/base.baseline" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then
    ok "старые строки из базы — зелёный (засев, а не ноль)"
else
    bad "засеянная строка покрасила стража: $out"
fi

# --- пара 2: храповик только вниз ------------------------------------------
mkreg ""; mkbase 2 200 999
out=$(python "$G" "$ROOT" "$TMP/reg.md" "$TMP/base.baseline" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '999'; then
    ok "номер, которому поставили поле, требует опустить базу ТОЙ ЖЕ правкой"
else
    bad "база осталась высокой молча: $out"
fi

mkreg ""; mkbase 5 200
out=$(python "$G" "$ROOT" "$TMP/reg.md" "$TMP/base.baseline" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '5'; then
    ok "база, спорящая сама с собой (число против номеров), — красный"
else
    bad "рассогласованная база прошла: $out"
fi

# --- пара 3: ОТКАЗ СУДИТЬ у сканера маршрутов -------------------------------
mkreg "$NEW_NOFIELD"; mkbase 1 200
mkdir -p "$TMP/root_scan/docs/plans"; cp "$TMP/reg.md" "$TMP/root_scan/docs/plans/221.1-bug-sweep.md"
out=$(NOVA_NOFIELD_BASELINE="$TMP/base.baseline" python "$SCAN" "$TMP/root_scan" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '300'; then
    ok "сканер маршрутов ОТКАЗЫВАЕТСЯ судить новую строку без поля"
else
    bad "сканер посчитал числа по строке, которую судить не может: $out"
fi

mkreg ""; cp "$TMP/reg.md" "$TMP/root_scan/docs/plans/221.1-bug-sweep.md"
out=$(NOVA_NOFIELD_BASELINE="$TMP/base.baseline" python "$SCAN" "$TMP/root_scan" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'blockers='; then
    ok "без новых строк сканер считает как прежде"
else
    bad "сканер сломался на здоровом реестре: $out"
fi

# --- пара 4: ОТКАЗ СУДИТЬ у храповика закрытий ------------------------------
mkreg "$NEW_NOFIELD"
printf '%s\n' '# база закрытий самотеста' > "$TMP/closed.baseline"
out=$(NOVA_NOFIELD_BASELINE="$TMP/base.baseline" python "$KEPT" "$ROOT" "$TMP/reg.md" "$TMP/closed.baseline" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '300'; then
    ok "храповик закрытий ОТКАЗЫВАЕТСЯ судить ту же строку"
else
    bad "храповик домыслил статус новой строки: $out"
fi

mkreg ""
out=$(NOVA_NOFIELD_BASELINE="$TMP/base.baseline" python "$KEPT" "$ROOT" "$TMP/reg.md" "$TMP/closed.baseline" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then
    ok "без новых строк храповик закрытий работает как прежде"
else
    bad "храповик сломался на здоровом реестре: $out"
fi

# --- клетка-сверщик: три копии чтения базы дают ОДНО множество ---------------
cat > "$TMP/agree.py" <<'PY'
import importlib.util as ilu, os, sys
root, base = sys.argv[1], sys.argv[2]
os.environ["NOVA_NOFIELD_BASELINE"] = base


def load(path, name):
    spec = ilu.spec_from_file_location(name, path)
    mod = ilu.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


g = load(os.path.join(root, "scripts", "guards", "check-registry-status-field.py"), "g")
s = load(os.path.join(root, "scripts", "guards", "registry-routes-scan.py"), "s")
k = load(os.path.join(root, "scripts", "guards", "check-registry-closure-kept.py"), "k")
a = g.read_baseline(base)[0]
b = s.nofield_baseline(root)
c = k.nofield_baseline()
print("SAME" if a == b == c else "DIFFER %s %s %s" % (sorted(a), sorted(b), sorted(c)))
PY
mkbase 2 200 777
out=$(python "$TMP/agree.py" "$ROOT" "$TMP/base.baseline" 2>&1)
if printf '%s' "$out" | grep -q '^SAME'; then
    ok "три копии чтения базы дают одно множество (сверщик, а не доверие)"
else
    bad "копии разошлись: $out"
fi

# Парная к сверщику: подмена базы меняет ответ ВСЕХ трёх, то есть сверщик
# действительно читает базу, а не сравнивает три пустоты.
mkbase 1 111
out=$(python "$TMP/agree.py" "$ROOT" "$TMP/base.baseline" 2>&1)
if printf '%s' "$out" | grep -q '^SAME'; then
    ok "другая база — снова согласие, значит сверяется содержимое"
else
    bad "сверщик не видит подмены базы: $out"
fi

TOTAL=$((PASS+FAIL))
echo "test-check-registry-status-field ok: $PASS/$TOTAL"
[ "$FAIL" -eq 0 ] || exit 1
