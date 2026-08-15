#!/bin/sh
# Самотест check-novac-ctx-tables.sh (П16: обязан доказать, что ловит).
#
# ПОДЛОЖКА. У стража два шва — $2 (путь к sem.nv) и $3 (путь к плану),
# поэтому настоящее дерево не нужно: каждый случай — своя пара крошечных
# файлов во временном каталоге. Дёшево, без оракула, красный случай есть.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-ctx-tables.sh"
T="${TMPDIR:-/tmp}/novac-ctx-tables-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }

mk_sem() {  # $1 файл, остальные — имена полей
    f="$1"; shift
    { echo "module novac.sem"
      echo ""
      echo "/// File-level semantic context."
      echo "export type Ctx {"
      for n in "$@"; do echo "    $n []Row /// что-то"; done
      echo "}"
      echo ""
      echo "export fn other() -> int => 0"
    } > "$f"
}
mk_plan() { # $1 файл, остальные — имена строк
    f="$1"; shift
    { echo "# План"
      echo ""
      echo "### 10.3б. Реестр таблиц строк"
      echo ""
      echo "| поле | что хранит | почему отдельная |"
      echo "|---|---|---|"
      for n in "$@"; do echo "| \`$n\` | хранит | потому что |"; done
      echo ""
      echo "## Следующий раздел"
      echo ""
      echo "| \`posle\` | эта строка ВНЕ раздела и считаться не должна | — |"
    } > "$f"
}

# --- 1. совпадают — зелёный ----------------------------------------------
mk_sem "$T/sem1.nv" types defs fns
mk_plan "$T/plan1.md" types defs fns
if run "$T/sem1.nv" "$T/plan1.md"; then
    grep -q "таблиц строк в Ctx: 3" "$T/out" && ok "совпадение — зелёный, число напечатано" || bad "зелёный, но без числа [$(cat "$T/out")]"
else
    bad "совпадение покраснело: $(cat "$T/err")"
fi

# --- 2. лишнее поле в коде — красный (главный случай) --------------------
mk_sem "$T/sem2.nv" types defs fns methods
if run "$T/sem2.nv" "$T/plan1.md"; then
    bad "новая таблица БЕЗ строки плана прошла — страж не ловит свой главный случай"
else
    grep -q "methods" "$T/err" && ok "новая таблица поймана, имя названо: $(head -1 "$T/err")" || bad "красный, но methods не назван"
fi

# --- 3. протухшая строка плана — красный ---------------------------------
mk_plan "$T/plan3.md" types defs fns fn_rets
if run "$T/sem1.nv" "$T/plan3.md"; then
    bad "строка плана без поля прошла (класс №519)"
else
    grep -q "fn_rets" "$T/err" && ok "протухшая строка поймана" || bad "красный, но fn_rets не назван"
fi

# --- 4. раздел переименован/пуст — красный, не зелёный --------------------
{ echo "# План"; echo ""; echo "### 10.3в. Другой раздел"; echo ""; echo "| \`types\` | x | y |"; } > "$T/plan4.md"
if run "$T/sem1.nv" "$T/plan4.md"; then
    bad "пустая/переименованная таблица дала ЗЕЛЁНЫЙ — вечнозелёный страж (класс №519)"
else
    grep -q "пуста или переименована" "$T/err" && ok "пустой раздел — красный, названо почему" || bad "красный, но без объяснения"
fi

# --- 5. Ctx потерян в исходнике — красный --------------------------------
{ echo "module novac.sem"; echo "export fn f() -> int => 0"; } > "$T/sem5.nv"
if run "$T/sem5.nv" "$T/plan1.md"; then
    bad "sem без Ctx дал зелёный — страж потерял мишень молча"
else
    grep -q "потерял мишень" "$T/err" && ok "потерянный Ctx — красный" || bad "красный, но без объяснения"
fi

# --- 6. строка ВНЕ раздела не считается ----------------------------------
mk_sem "$T/sem6.nv" types defs fns
if run "$T/sem6.nv" "$T/plan1.md"; then
    ok "таблица из соседнего раздела не подмешалась (posle не потребован)"
else
    bad "строка соседнего раздела подмешалась: $(cat "$T/err")"
fi

# --- 7. нет sem.nv — судить нечего ---------------------------------------
run "$T/absent.nv" "$T/plan1.md"
grep -q "судить нечего" "$T/out" && ok "нет sem.nv — судить нечего" || bad "нет sem.nv: ждали «судить нечего»"

# --- 8. настоящее дерево -------------------------------------------------
if sh "$G" "$ROOT" >/dev/null 2>&1; then
    ok "настоящее дерево — зелёное"
else
    bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -3)"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-ctx-tables ok: все случаи, включая новую таблицу и протухшую строку"
    exit 0
fi
exit 1
