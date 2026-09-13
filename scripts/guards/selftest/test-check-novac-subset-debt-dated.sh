#!/bin/sh
# Самотест check-novac-subset-debt-dated.py (П16). Швы: $2 — директория, $3 — база.
# Форма строки ok: два пробела, слово ok, пробелы — по ней считают случаи
# (check-novac-registry-counts.sh); двоеточие после ok делает случай невидимым.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-subset-debt-dated.py"
T="${TMPDIR:-/tmp}/subset-debt-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok   $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { python "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }
mk()  { d="$T/$1"; mkdir -p "$d/m"; shift; printf "%s\n" "$@" > "$d/m/m.nv"; echo "$d"; }

printf '%s\n' 'undated=0' > "$T/zero.baseline"
printf '%s\n' 'undated=1' > "$T/one.baseline"

# --- ГЛАВНЫЙ случай: новый долг БЕЗ этапа при базе ноль -------------------
D=$(mk undated "module a" \
    'const M = "outside the subset: an `if` in value position is not compiled yet"')
if run "$D" "$T/zero.baseline"; then
    bad "бессрочный долг прошёл при базе ноль - главный случай не ловится"
else
    grep -q "без этапа" "$T/err" && ok "бессрочный долг пойман" || bad "покраснел не тем текстом"
fi

# --- долг С этапом законен -------------------------------------------------
D=$(mk dated "module a" \
    'const M = "outside the subset: a declared local type is not compiled yet (E2-b3)"')
run "$D" "$T/zero.baseline" && ok "долг с этапом проходит" \
    || bad "долг с этапом покраснел - правило шире класса"

# --- этап в ДРУГОЙ форме тоже считается -----------------------------------
D=$(mk dated2 "module a" \
    'const M = "outside the subset: generic parameters are not compiled yet (E2)"')
run "$D" "$T/zero.baseline" && ok "короткая форма этапа (E2) принимается" \
    || bad "короткая форма этапа отвергнута"

# --- срок в форме ВОЛНЫ ПЛАНА 274.7 законен (решение владельца 2026-09-05) ----
D=$(mk wave "module a" \
    'const M = "outside the subset: an interpolation slot of a record is not compiled yet (274.7 B1b)"')
run "$D" "$T/zero.baseline" && ok "срок в форме волны 274.7 принимается" \
    || bad "срок в форме волны 274.7 отвергнут"

# --- а голая ссылка на план без волны — НЕ срок --------------------------------
D=$(mk bareplan "module a" \
    'const M = "outside the subset: something is not compiled yet (274.7)"')
if run "$D" "$T/zero.baseline"; then
    bad "голая ссылка на план сошла за срок - форма расширилась до любой ссылки"
else
    ok "голая ссылка на план без волны отвергнута"
fi

# --- ОСЬ ФОРМУЛИРОВКИ (добавлена 2026-09-13): до этого дня КАЖДАЯ фикстура выше
# несла ОБА написания сразу, поэтому самотест не мог заметить, что страж видит
# только одно из них. Слепота прожила две недели именно в этой непокрытой клетке.
D=$(mk onlysubset "module a" \
    'const M = "outside the subset: `??` unwraps an `Option`, and this left side is not one"')
if run "$D" "$T/zero.baseline"; then
    bad "отказ ТОЛЬКО в форме 'outside the subset' прошёл - область уже предмета"
else
    ok   "отказ только в форме 'outside the subset' пойман"
fi

D=$(mk onlysubset_dated "module a" \
    'const M = "outside the subset: a `Result` left side is refused here (E2-b3)"')
run "$D" "$T/zero.baseline" && ok   "он же со сроком проходит" \
    || bad "отказ этой формы со сроком покраснел - правило шире класса"

# Прежняя половина не потеряна: расширение обязано быть ДОБАВЛЕНИЕМ, а не подменой.
# Сегодня же чинилась мерка, где новый образец ловил новое и ронял старое.
D=$(mk onlyyet "module a" \
    'const M = "a declared local type is not compiled yet"')
if run "$D" "$T/zero.baseline"; then
    bad "прежняя формулировка перестала ловиться - расширение оказалось подменой"
else
    ok   "прежняя формулировка ловится по-прежнему"
fi

# --- ОСЬ «СРОК В СКОБКАХ С ПОЯСНЕНИЕМ» (добавлена 2026-09-13) ---------------------
# Прежний образец требовал, чтобы скобка содержала ТОЛЬКО этап, и не видел срока в
# `(E2-b2 multi-file)` — шесть отказов дерева, у которых КОГДА названо, числились
# бессрочными. Пояснение рядом с этапом делает сообщение лучше, а не хуже.
D=$(mk stage_with_words "module a" \
    'const M = "outside the subset: a generic free function of a handed module is not compiled yet (E2-b2 multi-file)"')
run "$D" "$T/zero.baseline" && ok   "срок в скобках с пояснением принимается" \
    || bad "срок с пояснением отвергнут - автор вынужден портить сообщение ради формы"

# ...и обратная сторона, без которой расширение стало бы обманом числа: этап ВНЕ скобок
# сроком не считается. Скобка и есть авторский жест «я отвечаю на вопрос когда», а не
# «эти буквы встретились в предложении».
D=$(mk stage_in_prose "module a" \
    'const M = "outside the subset: a bound dispatches through a protocol, and that arrives with E2-b3 -- the parameter itself is compiled"')
if run "$D" "$T/zero.baseline"; then
    bad "этап в проходной фразе сошёл за срок - всякое упоминание стало обещанием"
else
    ok   "этап вне скобок сроком не считается"
fi

# --- ВТОРОЙ ХРАПОВИК (refusals=, цель ноль; 274.7 §И, построен 2026-09-13) --------
# Датированный долг тоже долг: без этого числа отказов могло становиться больше, лишь
# бы каждому проставили этап, — а решение владельца требует их УБИРАТЬ.
printf '%s\n' 'undated=9' 'refusals=1' > "$T/grow.baseline"
printf '%s\n' 'undated=9' 'refusals=2' > "$T/fit.baseline"
D=$(mk two "module a" \
    'const A = "outside the subset: form one is refused (E2-b3)"' \
    'const B = "outside the subset: form two is refused (E2-b3)"')
if run "$D" "$T/grow.baseline"; then
    bad "рост ОБЩЕГО числа отказов прошёл - храповик с целью ноль не держит"
else
    ok   "рост общего числа отказов пойман"
fi
run "$D" "$T/fit.baseline" && ok   "общее число ровно по базе проходит" \
    || bad "общее число ровно по базе покраснело"
# Старая база без ключа `refusals=` обязана работать: правка — добавление, не замена.
run "$D" "$T/one.baseline" || true
printf '%s\n' 'undated=9' > "$T/nokey.baseline"
run "$D" "$T/nokey.baseline" && ok   "база без ключа refusals= по-прежнему работает" \
    || bad "старая база сломалась - расширение оказалось несовместимым"

# --- ровно по базе: не хуже, чем было --------------------------------------
D=$(mk atbase "module a" \
    'const M = "outside the subset: string interpolation is not compiled yet"')
run "$D" "$T/one.baseline" && ok "долг ровно по базе проходит" \
    || bad "число, равное базе, покраснело - храповик судит строго больше"

# --- КОММЕНТАРИЙ с той же фразой законен -----------------------------------
D=$(mk comment "module a" \
    '// Раньше здесь стоял отказ "... is not compiled yet" без этапа.' \
    'fn f() -> () { }')
run "$D" "$T/zero.baseline" && ok "комментарий с историей класса проходит" \
    || bad "комментарий покраснел - страж стирает причину вместе с симптомом"

# --- отсутствие базы --------------------------------------------------------
run "$D" "$T/nosuch.baseline" && bad "отсутствие базы прошло - храповик без базы пуст" \
    || ok "отсутствие базы красное"

# --- потерянная мишень ------------------------------------------------------
# ОТСУТСТВУЮЩАЯ директория — соседняя ось к пустой, и до 2026-09-13 она была зелёной:
# страж печатал «ok: судить нечего (нет …)». Мета-страж `check-guard-empty-root` судит
# ПУСТОЙ каркас и этой оси не видит, поэтому случай нужен здесь.
run "$T/there-is-no-such-dir" "$T/zero.baseline" \
    && bad "отсутствующая директория прошла - потерянная мишень читается как чистый замер" \
    || ok   "отсутствующая директория красная (мишень потеряна)"

mkdir -p "$T/empty"
run "$T/empty" "$T/zero.baseline" \
    && bad "пустая директория прошла - страж, сканирующий ничто, слеп" \
    || ok "пустая директория красная (мишень потеряна)"

[ "$fails" -eq 0 ] && echo "test-check-novac-subset-debt-dated: ok" || exit 1
