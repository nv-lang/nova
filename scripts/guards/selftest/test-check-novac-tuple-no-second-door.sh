#!/bin/sh
# Самотест check-novac-tuple-no-second-door.py (П16). Шов $2 — сканируемая
# директория. Главный случай: НОВОЕ второе окно для кортежа обязано краснеть.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-tuple-no-second-door.py"
BASE="$GD/novac-tuple-doors.baseline"
T="${TMPDIR:-/tmp}/novac-tuple-door-selftest.$$"
mkdir -p "$T"
# ОРИГИНАЛ базы сохраняется ОДИН раз, до всего: первая редакция копировала базу
# внутри set_base, то есть на КАЖДОМ вызове, и последняя копия затирала
# настоящую — самотест уходил, оставив живую базу испорченной. Поймано его же
# первым прогоном (база стала doors=3 при шести дверях).
cp "$BASE" "$T/base.orig"
trap 'cp "$T/base.orig" "$BASE" 2>/dev/null; rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# База подменяется на время самотеста: он судит ПОВЕДЕНИЕ стража на подложке,
# а не живое дерево, поэтому число дверей в подложке должно совпадать с базой.
set_base() { printf 'doors=%s\n' "$1" > "$BASE"; }

# подложка: эмиттер без кортежных дверей + законные места (mangle, types, parse)
mk() {
    d="$T/$1"; mkdir -p "$d/emit_c" "$d/sem" "$d/types" "$d/parse"
    printf 'module emit_c\n\nfn Emitter mut @emit_record_ctor(e int) -> int => e\n' > "$d/emit_c/emit_c.nv"
    printf 'module novac.sem\n\nexport fn mono_tuple_name(t int) -> str => "_NovaTuple_2"\n' > "$d/sem/mangle.nv"
    printf 'module novac.types\n\nfn Interner mut @tuple(comps []int) -> int => 0\n' > "$d/types/types.nv"
    printf 'module novac.parse\n\nfn Cursor mut @tuple_expr() -> int => 0\n' > "$d/parse/expr.nv"
    echo "$d"
}

# --- чистое дерево: ноль дверей, база ноль -------------------------------
set_base 0
D=$(mk clean)
run "$D" && ok "эмиттер без кортежных дверей — зелёный" || bad "чистая подложка покраснела: $(cat "$T/err")"
grep -q "цель достигнута" "$T/out" || bad "ноль дверей, а страж не сказал «цель достигнута»"

# --- ГЛАВНЫЙ случай: новая дверь эмиссии с tuple в имени -----------------
set_base 0
D=$(mk doorA)
printf 'module emit_c\n\nfn Emitter mut @emit_tuple_lit(e int) -> int => e\n' > "$D/emit_c/emit_tuple.nv"
if run "$D"; then
    bad "НОВОЕ второе окно (@emit_tuple_lit) прошло — главный случай не ловится"
else
    grep -q "ВЫРОСЛА" "$T/err" && grep -q "emit_tuple_lit" "$T/err" \
        && ok "новая дверь эмиссии поймана и названа" || bad "красный, но не про новую дверь"
fi

# --- подчёркивания: имя с tuple ВНУТРИ идентификатора --------------------
# Это та самая дыра, которую нашёл первый прогон: `\btuple\b` тут не совпадает.
set_base 0
D=$(mk doorU)
printf 'module emit_c\n\nfn Emitter mut @emit_tuple_typedefs() -> () { }\n' > "$D/emit_c/emit_tuple.nv"
if run "$D"; then
    bad "tuple ВНУТРИ идентификатора не пойман — вернулась дыра границ слова"
else
    ok "tuple внутри идентификатора ловится (дыра границ слова закрыта)"
fi

# --- ветка по виду типа в эмиссии ----------------------------------------
set_base 0
D=$(mk branch)
printf 'module emit_c\n\nfn Emitter @go(k int) -> int {\n    if k == TypeKind.TkTuple { return 1 }\n    0\n}\n' > "$D/emit_c/pick.nv"
if run "$D"; then
    bad "ветка TkTuple в эмиссии прошла"
else
    grep -q "TkTuple" "$T/err" && ok "ветка по виду типа в эмиссии поймана" || bad "красный, но не про ветку"
fi

# --- своя механика умолчаний у кортежа -----------------------------------
set_base 0
D=$(mk defaults)
printf 'module novac.check\n\nfn Checker @tuple_default_slot(i int) -> int => i\n' > "$D/check_defaults.nv"
if run "$D"; then
    bad "своя механика умолчаний у кортежа прошла"
else
    ok "своя механика умолчаний у кортежа поймана"
fi

# --- ЗАКОННЫЕ места не считаются -----------------------------------------
# mangle (спеллинг имени), types (структурное тождество), parse (грамматика).
set_base 0
D=$(mk legal)
run "$D" && ok "mangle/types/parse — законны, не считаются дверями" \
    || bad "законные места посчитаны дверями: $(cat "$T/err")"

# --- проза о кортежах законна --------------------------------------------
set_base 0
D=$(mk prose)
printf 'module emit_c\n\n/// A tuple is a record on the stack; fn @emit_tuple_lit lived here once.\n// fn Emitter mut @emit_tuple_bind() -> () { }\nfn Emitter @go() -> int => 0\n' > "$D/emit_c/emit_c.nv"
run "$D" && ok "комментарий про кортежи — не дверь" || bad "проза посчитана дверью: страж запретил объяснять себя"

# --- СНИЖЕНИЕ без правки базы тоже красное -------------------------------
set_base 3
D=$(mk lower)
if run "$D"; then
    bad "снижение без правки базы прошло молча — следующий рост будет невидим"
else
    grep -q "СНИЗИЛАСЬ" "$T/err" && ok "снижение без правки базы — красный" || bad "красный, но не про снижение"
fi

# --- нет базы — судить не по чему ----------------------------------------
printf '# no number here\n' > "$BASE"
D=$(mk nobase)
if run "$D"; then
    bad "без числа в базе страж остался зелёным — судил бы пустоту"
else
    grep -q "нет базы" "$T/err" && ok "база без числа — красный (класс №519)" || bad "красный, но не про базу"
fi
cp "$T/base.orig" "$BASE"

# --- нет директории ------------------------------------------------------
set_base 0
run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-tuple-no-second-door ok: все случаи, включая новое второе окно, дыру границ слова и снижение без правки базы"
    exit 0
fi
exit 1
