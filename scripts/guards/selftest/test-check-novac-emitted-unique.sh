#!/bin/sh
# Самотест check-novac-emitted-unique.py (П16). Шов $2 — директория с готовыми
# `.c`-юнитами: novac при этом не запускается вовсе, и самотест судит РОВНО
# правило, а не состояние компилятора.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-emitted-unique.py"
T="${TMPDIR:-/tmp}/novac-emituniq-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d"; shift; printf "%s\n" "$@" > "$d/u.c"; echo "$d"; }

# --- ГЛАВНЫЙ случай: две функции с одним именем ---------------------------
D=$(mk dup_fn \
    "static nova_int novac_fn_f__nova_int__to_nova_int(nova_int a) {" "    return a;" "}" \
    "static nova_int novac_fn_f__nova_int__to_nova_int(nova_int b) {" "    return b;" "}")
if run "$D"; then
    bad "два определения одного имени прошли — главный случай не ловится"
else
    grep -q "определено 2 раза" "$T/err" && ok "повтор определения функции пойман" \
        || bad "покраснел не тем текстом"
fi

# --- ГЛАВНЫЙ случай 2: повтор тега перечисления --------------------------
D=$(mk dup_tag "typedef enum {" "    NOVAC_TAG_S_A," "    NOVAC_TAG_S_A," "} Nova_S_Tag;")
run "$D" && bad "повтор тега прошёл" || ok "повтор тега перечисления пойман"

# --- ЗАКОННО: два РАЗНЫХ имени ------------------------------------------
D=$(mk two_names \
    "static nova_int novac_fn_f__nova_int__to_nova_int(nova_int a) {" "    return a;" "}" \
    "static nova_int novac_fn_g__nova_int__to_nova_int(nova_int b) {" "    return b;" "}")
run "$D" && ok "два разных имени зелёные" || bad "разные имена покраснели"

# --- ЗАКОННО: прототип плюс определение ---------------------------------
D=$(mk proto \
    "static nova_int novac_fn_f__nova_int__to_nova_int(nova_int a);" \
    "static nova_int novac_fn_f__nova_int__to_nova_int(nova_int a) {" "    return a;" "}")
run "$D" && ok "прототип определением не считается" \
    || bad "прототип посчитан определением — страж шире правила"

# --- ЗАКОННО: ЧТЕНИЕ тега вне перечисления ------------------------------
D=$(mk tag_read "typedef enum {" "    NOVAC_TAG_S_A," "} Nova_S_Tag;" \
    "static nova_int novac_fn_use__to_nova_int(void) {" \
    "    if (x->tag == NOVAC_TAG_S_A) { return 1; }" "    return NOVAC_TAG_S_A;" "}")
run "$D" && ok "чтение тега вне enum не считается объявлением" \
    || bad "чтение тега посчитано объявлением"

# --- ЗАКОННО: повтор `typedef` (оболочка делает так законно) -------------
D=$(mk typedefs "typedef struct Nova_Point Nova_Point;" \
    "typedef struct { nova_int x; } Nova_Point;")
run "$D" && ok "повтор typedef не судится: C11 разрешает, оболочка так и пишет" \
    || bad "повтор typedef покраснел — то самое ложное срабатывание"

# --- ЗАКОННО: имена РАНТАЙМА вне нашей приставки ------------------------
D=$(mk runtime "nova_int nova_fn_main_impl(void) {" "    return 0;" "}" \
    "nova_int nova_fn_main_impl(void) {" "    return 1;" "}")
run "$D" && ok "имена оракула не судятся: их существование судит другой страж" \
    || bad "имя рантайма покраснело — не то пространство"

# --- ЗАКОННО: одно имя в РАЗНЫХ юнитах ---------------------------------
D="$T/two_units"; mkdir -p "$D"
printf "%s\n" "static nova_int novac_fn_f__to_nova_int(void) {" "    return 1;" "}" > "$D/a.c"
printf "%s\n" "static nova_int novac_fn_f__to_nova_int(void) {" "    return 1;" "}" > "$D/b.c"
run "$D" && ok "одно имя в двух юнитах законно: правило про ОДИН юнит" \
    || bad "два юнита покраснели — правило спутано с межюнитным"

# --- ЗАКОННО: комментарий ----------------------------------------------
D=$(mk comment "// static nova_int novac_fn_f__to_nova_int(void) {" \
    "static nova_int novac_fn_f__to_nova_int(void) {" "    return 1;" "}")
run "$D" && ok "закомментированное определение не считается" || bad "комментарий посчитан"

# --- пустая директория -------------------------------------------------
D="$T/empty"; mkdir -p "$D"
run "$D" && ok "директория без .c зелёная" || bad "пустая директория покраснела"

[ "$fails" -eq 0 ] && echo "test-check-novac-emitted-unique: 10/10" && exit 0
echo "test-check-novac-emitted-unique: провалов $fails" >&2
exit 1
