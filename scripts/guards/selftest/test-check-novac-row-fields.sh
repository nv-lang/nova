#!/bin/sh
# Самотест check-novac-row-fields.sh (П16: обязан доказать, что ловит).
# Швы $2 (sem.nv) и $3 (план) — подложка из пары крошечных файлов.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-row-fields.sh"
T="${TMPDIR:-/tmp}/novac-row-fields-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }

mk_sem() { f="$1"; shift; { echo "module novac.sem"; echo ""; echo "$@"; } > "$f"; }
mk_plan() { # $1 файл, дальше пары "Запись поле"
    f="$1"; shift
    { echo "# План"; echo ""; echo "### 10.3в. Реестр полей строк"; echo ""
      echo "| строка | поле | зачем |"; echo "|---|---|---|"
      for pair in "$@"; do
        set -- $pair
        echo "| \`$1\` | \`$2\` | причина |"
      done
      echo ""; echo "### 10.4. Другое"; echo ""; echo "| \`Vне\` | \`polе\` | вне раздела |"
    } > "$f"
}

# --- 1. совпадают — зелёный ----------------------------------------------
mk_sem "$T/s1.nv" 'export type FnDef value {
    name str /// имя
    param_off int /// начало
    param_cnt int /// сколько
}'
mk_plan "$T/p1.md" "FnDef name" "FnDef param_off" "FnDef param_cnt"
if run "$T/s1.nv" "$T/p1.md"; then
    grep -q "полей строк реестра: 3" "$T/out" && ok "совпадение — зелёный, счёт верный" || bad "зелёный, но счёт не тот [$(cat "$T/out")]"
else
    bad "совпадение покраснело: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ случай: поле без записи в плане --------------------------
mk_sem "$T/s2.nv" 'export type FnDef value {
    name str /// имя
    param_off int /// начало
    param_cnt int /// сколько
    recv_id int /// получатель отдельным полем
}'
if run "$T/s2.nv" "$T/p1.md"; then
    bad "поле без записи в плане прошло — страж не ловит свой главный случай"
else
    grep -q "recv_id" "$T/err" && ok "поле без записи поймано, названо" || bad "красный, но recv_id не назван"
fi

# --- 3. признак (B): выделенный элемент рядом с *_off/*_cnt --------------
mk_plan "$T/p3.md" "FnDef name" "FnDef param_off" "FnDef param_cnt" "FnDef recv_id"
if run "$T/s2.nv" "$T/p3.md"; then
    bad "выделенный элемент общего списка прошёл, хотя в плане описан"
else
    grep -q "выделенный элемент" "$T/err" && ok "признак (B) ловит даже описанное в плане поле" || bad "красный, но не по признаку (B)"
fi

# --- 4. тот же recv_id БЕЗ общего списка — законен ------------------------
mk_sem "$T/s4.nv" 'export type FnDef value {
    name str /// имя
    recv_id int /// получатель
}'
mk_plan "$T/p4.md" "FnDef name" "FnDef recv_id"
run "$T/s4.nv" "$T/p4.md" && ok "поле-получатель без пары off/cnt не судится признаком (B)" || bad "ложняк (B): $(cat "$T/err")"

# --- 5. протухшая строка плана -------------------------------------------
mk_plan "$T/p5.md" "FnDef name" "FnDef param_off" "FnDef param_cnt" "FnDef dup"
if run "$T/s1.nv" "$T/p5.md"; then
    bad "строка плана без поля прошла (класс №519)"
else
    grep -q "dup" "$T/err" && ok "протухшая запись поймана" || bad "красный, но dup не назван"
fi

# --- 6. контейнеры (не value) не судятся ---------------------------------
mk_sem "$T/s6.nv" 'export type FnDef value {
    name str /// имя
}

export type FnTable {
    rows []FnDef /// строки
    heads NameTable /// дверь
}'
mk_plan "$T/p6.md" "FnDef name"
run "$T/s6.nv" "$T/p6.md" && ok "контейнер (не value) не судится — его состав держит §10.3б" || bad "контейнер попал под суд: $(cat "$T/err")"

# --- 7. переименованный/пустой раздел — красный, не зелёный --------------
{ echo "# План"; echo ""; echo "### 10.3г. Другой раздел"; echo ""; echo "| \`FnDef\` | \`name\` | x |"; } > "$T/p7.md"
if run "$T/s1.nv" "$T/p7.md"; then
    bad "пустая таблица дала зелёный — вечнозелёный страж (класс №519)"
else
    grep -q "пуста или переименована" "$T/err" && ok "пустой раздел — красный, названо почему" || bad "красный, но без объяснения"
fi

# --- 8. нет sem.nv — судить нечего ---------------------------------------
run "$T/absent.nv" "$T/p1.md"
grep -q "судить нечего" "$T/out" && ok "нет sem.nv — судить нечего" || bad "ждали «судить нечего»"

# --- 9. настоящее дерево -------------------------------------------------
sh "$G" "$ROOT" >/dev/null 2>&1 && ok "настоящий novac/src/sem — зелёный" || bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -3)"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-row-fields ok: все случаи, включая поле без записи и выделенный элемент списка"
    exit 0
fi
exit 1
