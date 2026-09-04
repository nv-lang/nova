#!/bin/sh
# Самотест check-novac-tuple-no-second-door.py v2 (правило СЛОВА, владелец
# 2026-09-03). Шов $2 — сканируемая директория. Главные случаи: новое слово
# tuple в коде и в имени файла обязаны краснеть; комментарии и имена вариантов
# NamedTupleRecord/AnonymousTupleRecord — законны.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-tuple-no-second-door.py"
BASE="$GD/novac-tuple-doors.baseline"
T="${TMPDIR:-/tmp}/novac-tuple-word-selftest.$$"
mkdir -p "$T"
# ОРИГИНАЛ базы сохраняется ОДИН раз, до всего: первая редакция самотеста
# копировала базу на каждом вызове set_base, последняя копия затирала настоящую,
# и самотест уходил, испортив живую базу (поймано его же первым прогоном).
cp "$BASE" "$T/base.orig"
trap 'cp "$T/base.orig" "$BASE" 2>/dev/null; rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# База подменяется на время самотеста: он судит ПОВЕДЕНИЕ стража на подложке.
set_base() { printf 'tuple_words=%s\n' "$1" > "$BASE"; }

# подложка: код без слова tuple + законные употребления
mk() {
    d="$T/$1"; mkdir -p "$d/emit_c" "$d/sem" "$d/check"
    printf 'module emit_c\n\nfn Emitter mut @emit_record_ctor(e int) -> int => e\n' > "$d/emit_c/emit_c.nv"
    printf 'module novac.sem\n\n/// A tuple is a record on the stack (the rule lives in prose).\nexport fn shape_name(s int) -> str => "record"\n' > "$d/sem/sem.nv"
    printf 'module novac.check\n\nfn Checker @on_heap(s RecordShape) -> bool {\n    match s {\n        Record => true\n        ValueRecord | NamedTuple | PositionalTuple | AnonNamedTuple | AnonPositionalTuple => false\n    }\n}\n' > "$d/check/check.nv"
    echo "$d"
}

# --- чистая подложка: комментарии и имена вариантов законны -----------------
set_base 0
D=$(mk clean)
run "$D" && ok "проза о кортежах и имена вариантов — зелёные" || bad "чистая подложка покраснела: $(cat "$T/err")"
# Формулировка сменилась вместе с КОНТРАКТОМ, а не сама по себе: владелец
# 2026-09-04 снял цель «ноль» («наклейка -- не бухгалтерия»), и печатать ноль
# как достигнутую цель стало бы неправдой. Случай остаётся под судом -- он
# держит то, что при нуле слов страж зелен и говорит об этом.
grep -q "ноль слов" "$T/out" || bad "ноль слов, а страж этого не сказал"

# --- ГЛАВНЫЙ случай 1: слово tuple в идентификаторе -------------------------
set_base 0
D=$(mk word1)
printf 'module emit_c\n\nfn Emitter mut @emit_tuple_lit(e int) -> int => e\n' > "$D/emit_c/two.nv"
if run "$D"; then
    bad "идентификатор со словом tuple прошёл — главный случай не ловится"
else
    grep -q "emit_tuple_lit" "$T/err" && ok "идентификатор со словом tuple пойман и назван" || bad "красный, но без имени"
fi

# --- ГЛАВНЫЙ случай 2: имя файла со словом tuple ----------------------------
set_base 0
D=$(mk word2)
printf 'module emit_c\n\nfn Emitter @go() -> int => 0\n' > "$D/emit_c/emit_tuple.nv"
if run "$D"; then
    bad "имя файла со словом tuple прошло"
else
    grep -q "имя файла" "$T/err" && ok "имя файла со словом tuple поймано" || bad "красный, но не про имя файла"
fi

# --- вид типа/узла со словом tuple ------------------------------------------
set_base 0
D=$(mk word3)
printf 'module novac.check\n\nfn Checker @go(k int) -> int {\n    if k == TypeKind.TkTuple { return 1 }\n    0\n}\n' > "$D/check/pick.nv"
if run "$D"; then
    bad "TkTuple в коде прошёл"
else
    grep -q "TkTuple" "$T/err" && ok "вид типа со словом tuple пойман" || bad "красный, но не про TkTuple"
fi

# --- строковый литерал со словом tuple --------------------------------------
set_base 0
D=$(mk word4)
printf 'module novac.sem\n\nexport fn nm() -> str => "_NovaTuple_2_x"\n' > "$D/sem/mangle.nv"
if run "$D"; then
    bad "строковый литерал со словом tuple прошёл"
else
    ok "строковый литерал со словом tuple пойман"
fi

# --- регистр не спасает ------------------------------------------------------
set_base 0
D=$(mk word5)
printf 'module novac.check\n\nfn Checker @go() -> int => TUPLE_LIMIT\n' > "$D/check/caps.nv"
if run "$D"; then
    bad "TUPLE в верхнем регистре прошёл"
else
    ok "верхний регистр ловится (поиск без регистра)"
fi

# --- имена вариантов в match-армах законны ----------------------------------
set_base 0
D=$(mk legal)
printf 'module novac.sem\n\nfn pick(s RecordShape) -> int {\n    match s {\n        NamedTuple => 1\n        AnonPositionalTuple => 2\n        Record | ValueRecord | PositionalTuple | AnonNamedTuple => 0\n    }\n}\n' > "$D/sem/shape.nv"
run "$D" && ok "имена вариантов в армах — законны" || bad "имена вариантов посчитаны словом: $(cat "$T/err")"

# --- комментарий с идентификатором в прозе законен ---------------------------
set_base 0
D=$(mk prose)
printf 'module emit_c\n\n/// fn @emit_tuple_lit lived here once; a tuple is a record on the stack.\n// TkTuple is gone too.\nfn Emitter @go() -> int => 0\n' > "$D/emit_c/emit_c.nv"
run "$D" && ok "проза с бывшими именами — не слово в коде" || bad "проза посчитана кодом: страж запретил объяснять себя"

# --- хвостовой комментарий после кода: код судится, хвост нет ----------------
set_base 0
D=$(mk tailc)
printf 'module novac.check\n\nfn Checker @go() -> int => 0 // the tuple story ends here\n' > "$D/check/t.nv"
run "$D" && ok "хвостовой комментарий не считается" || bad "хвостовой комментарий посчитан кодом"

# --- СНИЖЕНИЕ без правки базы тоже красное -----------------------------------
set_base 3
D=$(mk lower)
if run "$D"; then
    bad "снижение без правки базы прошло молча — следующий рост будет невидим"
else
    grep -q "СНИЗИЛОСЬ" "$T/err" && ok "снижение без правки базы — красный" || bad "красный, но не про снижение"
fi

# --- нет базы — судить не по чему --------------------------------------------
printf '# no number here\n' > "$BASE"
D=$(mk nobase)
if run "$D"; then
    bad "без числа в базе страж остался зелёным — судил бы пустоту"
else
    grep -q "нет базы" "$T/err" && ok "база без числа — красный (класс №519)" || bad "красный, но не про базу"
fi
cp "$T/base.orig" "$BASE"

# --- нет директории -----------------------------------------------------------
set_base 0
run "$T/absent"; grep -q "судить нечего" "$T/out" && ok "нет директории — судить нечего" || bad "ждали «судить нечего»"

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-tuple-no-second-door ok: правило слова — идентификатор, имя файла, вид типа, литерал и регистр краснеют; проза и имена вариантов законны"
    exit 0
fi
exit 1
