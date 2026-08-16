#!/bin/sh
# Самотест check-novac-effects-at-door.sh (П16: страж не принят, пока
# самотест не ДОКАЗАЛ, что страж ловит).
#
# ПОДЛОЖКА. У стража два шва — $2 (сканируемая директория) и $3 (каталог
# прелюдии), поэтому настоящее дерево нужно ровно один раз, последним
# случаем. Остальное — крошечное фиктивное дерево novac во временном
# каталоге: дверь main.nv плюс модули sem/check/parse ниже неё. Каждый
# случай пересобирает дерево заново и дописывает ОДНУ строку — так видно,
# что красит именно она.
#
# Часть случаев тут не про «ловит», а про «не красит зря»: комментарий с
# `with Fs`, многострочная фикстура-строка с исходником на Nova (тесты
# novac ИЗ ТАКОГО и состоят) и символьный литерал с кавычкой внутри
# (лексер novac сравнивает байт открывающей кавычки — на нём наивный
# разборщик строк уходит в вечную строку и страж слепнет молча).
# Сверх того, в ЧИСТОМ дереве лежат выражения, похожие на строку эффектов:
# многострочный вызов с закрывающей скобкой на отдельной строке, плечи
# match с именами Fs/Os и скобочная группа перед заглавным именем. Страж
# судит любую строку, а не только с `fn`, — и платить за это ложной
# тревогой не должен: если заплатит, покраснеет случай 1 и с ним все
# остальные.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-effects-at-door.sh"
T="${TMPDIR:-/tmp}/novac-effects-at-door-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" "$2" > "$T/out" 2> "$T/err"; }

# --- фиктивная прелюдия ----------------------------------------------------
# Random здесь для того, чтобы доказать: словарь имён страж ЧИТАЕТ отсюда, а
# не помнит наизусть. Fs/Os в прелюдии нет — они дверные и зашиты честно.
mkdir -p "$T/prelude"
cat > "$T/prelude/effects.nv" <<'PRELEOF'
module std.prelude.effects

export type Random effect {
    fn next_u64() -> u64
}

export type Supervisor effect {
    fn spawn(f fn()) -> int
}
PRELEOF

# --- чистое дерево ---------------------------------------------------------
# Здесь же сидят все три «не крась зря»: ends_with/starts_with, комментарий
# с `with Fs`, символьный литерал с кавычкой и многострочная фикстура.
mk_tree() {
    rm -rf "$T/tree"
    mkdir -p "$T/tree/sem" "$T/tree/check" "$T/tree/parse"
    cat > "$T/tree/main.nv" <<'MAINEOF'
module novac

import std.fs.{read, real_fs}
import std.os.{args, exit_process, real_os}

/// ДВЕРЬ: файловая система ходит только здесь.
fn read_decls(dir str) Fs -> []str {
    mut out = []str.new()
    out
}

fn main() {
    with Fs = real_fs(), Os = real_os() {
        ro a = args()
        exit_process(0)
    }
}
MAINEOF
    cat > "$T/tree/sem/sem.nv" <<'SEMEOF'
module novac.sem

/// Контекст — ДАННЫЕ на ресивере, не способность (П15).
export type Ctx {
    types []Row /// реестр типов
}

export fn type_of(ctx Ctx, id int) -> int => 0

/// ends_with не должен путаться с установкой обработчика `with X =`.
fn is_nv(name str) -> bool => name.ends_with(".nv")
SEMEOF
    cat > "$T/tree/check/check.nv" <<'CHKEOF'
module novac.check

// Дверь стоит в main.nv: там `with Fs = real_fs(), Os = real_os() { ... }`,
// а здесь про эффекты можно только рассказывать. Комментарий не судится.
/// Два аргумента, которые оба зовут — это два side effects без порядка.
export fn check(src str) -> []str {
    mut out = []str.new()
    if src.starts_with("module") { out.push("ok") }
    out
}
CHKEOF
    cat > "$T/tree/parse/parse.nv" <<'PARSEOF'
module novac.parse

export fn parse(text str) -> int => 0

export fn Node @kind() -> int => match @ { Leaf => 0, Branch => 1 }

/// Многострочный ВЫЗОВ: закрывающая скобка стоит отдельной строкой, за ней
/// тело. Страж судит любую строку — на выражении краснеть он не должен.
export fn classify(rows []Row) -> int {
    ro t = Tree.new(
        rows,
        0
    )
    match pick(
        t
    ) {
        Leaf => 0,
        Branch => 1,
    }
}

/// Плечи match кончаются на `)` и продолжаются заглавной буквой, а имена
/// эффектов В ПЛЕЧАХ — данные, а не строка эффектов.
export fn arm(v Res) -> int => match v { Ok(x) => x, Err(e) => 0 }
export fn eff_name(k EffKind) -> str => match k { Fs => "fs", Os => "os" }

/// Скобочная группа перед заглавным именем.
export fn calc(a int) -> int => (a + 1) * Limits.max()
PARSEOF
    cat > "$T/tree/parse/parse_test.nv" <<'PTEOF'
module novac.parse_test

/// Байт открывающей кавычки: символьный литерал с кавычкой внутри.
fn is_quote(b u8) -> bool => b == ('"' as u8)

test "фикстура: эффекты вне подмножества E1" {
    ro src = "module a.b
with Fs = real_fs() { }
type Log effect { fn write(s str) }
fn f(x str) Fs -> int => 0
"
    assert(refused(src))
    assert(refused("with Os = real_os() { }"))
}
PTEOF
}

# --- 1. чистое дерево — зелёный -------------------------------------------
mk_tree
if run "$T/tree" "$T/prelude"; then
    grep -q "файлов ниже двери 4" "$T/out" \
        && ok "чистое дерево — зелёное, число напечатано: $(cat "$T/out")" \
        || bad "зелёный, но итог не тот [$(cat "$T/out")]"
else
    bad "чистое дерево покраснело: $(cat "$T/err")"
fi

# --- 2. обработчик в sem — красный (главный случай) ------------------------
mk_tree
printf '\nfn boot() {\n    with Fs = real_fs() { }\n}\n' >> "$T/tree/sem/sem.nv"
if run "$T/tree" "$T/prelude"; then
    bad "обработчик 'with Fs = real_fs()' в sem прошёл — страж не ловит свой главный случай"
else
    grep -q "sem/sem.nv:[0-9]*: обработчик" "$T/err" \
        && ok "обработчик ниже двери пойман, файл:строка названы: $(grep 'sem.nv' "$T/err")" \
        || bad "красный, но без file:line для sem.nv [$(cat "$T/err")]"
fi

# --- 3. объявление эффекта в check — красный ------------------------------
mk_tree
printf '\ntype Log effect {\n    fn write(s str)\n}\n' >> "$T/tree/check/check.nv"
if run "$T/tree" "$T/prelude"; then
    bad "объявление 'type Log effect {' в check прошло"
else
    grep -q "check/check.nv:[0-9]*: объявление эффекта .type Log effect" "$T/err" \
        && ok "объявление эффекта поймано, имя названо: $(grep 'check.nv' "$T/err")" \
        || bad "красный, но Log не назван [$(cat "$T/err")]"
fi

# --- 4. эффект в сигнатуре в parse — красный ------------------------------
mk_tree
printf '\nfn f(x str) Fs -> int => 0\n' >> "$T/tree/parse/parse.nv"
if run "$T/tree" "$T/prelude"; then
    bad "эффект в сигнатуре 'fn f(x str) Fs -> int' в parse прошёл"
else
    grep -q "parse/parse.nv:[0-9]*: эффект .Fs. в сигнатуре" "$T/err" \
        && ok "эффект в сигнатуре пойман: $(grep 'parse.nv' "$T/err")" \
        || bad "красный, но не про сигнатуру parse.nv [$(cat "$T/err")]"
fi

# --- 4б. эффект в сигнатуре БЕЗ стрелки (`) Eff {`) — красный -------------
# Форма из std (`export fn TcpStream consume @close() Net { ... }`): тела в
# фигурных скобках, стрелки нет вовсе.
mk_tree
printf '\nfn g(x str) Os {\n    println(x)\n}\n' >> "$T/tree/parse/parse.nv"
if run "$T/tree" "$T/prelude"; then
    bad "эффект перед телом ') Os {' прошёл — форма без стрелки не судится"
else
    grep -q "эффект .Os. в сигнатуре" "$T/err" \
        && ok "эффект без стрелки пойман (форма 'fn g(x str) Os {')" \
        || bad "красный, но не про Os [$(cat "$T/err")]"
fi

# --- 4в. многострочная сигнатура: эффект на строке закрывающей скобки -----
# Слова fn на этой строке нет; судить только строки с fn — дыра шириной в
# один перенос.
mk_tree
printf '\nfn h(\n    a str,\n    b int\n) Fs -> int {\n    0\n}\n' >> "$T/tree/parse/parse.nv"
if run "$T/tree" "$T/prelude"; then
    bad "эффект на строке закрывающей скобки многострочной сигнатуры прошёл"
else
    grep -q "эффект .Fs. в сигнатуре" "$T/err" \
        && ok "многострочная сигнатура судится (эффект на строке с закрывающей скобкой)" \
        || bad "красный, но не про Fs [$(cat "$T/err")]"
fi

# --- 4г. имя нарушителя названо ВЕРНО при export --------------------------
# gawk молча не находит по '^.*[^A-Za-z0-9_]type', хотя находит по
# '[^A-Za-z0-9_]type': нарушитель тогда зовётся 'export' и подсказка врёт.
mk_tree
printf '\nexport type Log effect {\n    fn write(s str)\n}\n' >> "$T/tree/check/check.nv"
if run "$T/tree" "$T/prelude"; then
    bad "'export type Log effect {' прошло"
else
    if grep -q "объявление эффекта .type Log effect" "$T/err"; then
        ok "при export имя названо верно (Log, а не export)"
    else
        bad "имя нарушителя переврано: $(grep 'check.nv' "$T/err")"
    fi
fi

# --- 4д. ряд из ДВУХ эффектов через пробел — красный, названы ОБА ---------
# Настоящая форма Nova: `fn main() Fs Os -> ()`
# (examples/effects/serde_fs_build.nv:21). Ряд разделён ПРОБЕЛАМИ; рядов
# через запятую в дереве ноль, а через пробел — 51. Регулярка на запятые не
# видела здесь ничего: ни Os, ни стоящий перед ним Fs.
mk_tree
printf '\nfn boot() Fs Os -> () {\n    ()\n}\n' >> "$T/tree/sem/sem.nv"
if run "$T/tree" "$T/prelude"; then
    bad "ряд 'Fs Os' через пробел прошёл целиком — настоящая форма дерева не судится"
else
    if grep -q "эффект .Fs." "$T/err" && grep -q "эффект .Os." "$T/err"; then
        ok "ряд из двух эффектов через пробел пойман, названы оба"
    else
        bad "ряд 'Fs Os' назван не полностью: $(grep 'sem.nv' "$T/err")"
    fi
fi

# --- 4е. сосед с аргументами типа не рвёт ряд -----------------------------
# `) Fs Fail[IoError] -> ()` — форма std/src/fs/fs.nv:326, таких сигнатур в
# дереве 162. Квадратная скобка у СОСЕДА уносила из ряда и сам Fs.
mk_tree
printf '\nfn spill(p str) Fs Fail[IoError] -> () {\n    ()\n}\n' >> "$T/tree/check/check.nv"
if run "$T/tree" "$T/prelude"; then
    bad "'Fs Fail[IoError]' прошло — аргументы типа у соседа уносят весь ряд"
else
    grep -q "эффект .Fs." "$T/err" \
        && ok "аргументы типа у соседа ряд не рвут (Fs пойман)" \
        || bad "красный, но не про Fs [$(cat "$T/err")]"
fi

# --- 4ж. закрывающая скобка в ХВОСТЕ последнего параметра -----------------
# `cfg PropertyConfig) Random Fail[E] -> ()` — форма
# std/src/testing/property.nv:363: строка не начинается с `)` и слова `fn`
# не содержит, то есть обе прежние приметы промахиваются.
mk_tree
printf '\nfn walk(root Node,\n        cfg WalkConfig) Random Fail[WalkErr] -> () {\n    ()\n}\n' >> "$T/tree/sem/sem.nv"
if run "$T/tree" "$T/prelude"; then
    bad "эффект на строке 'cfg WalkConfig) Random ... {' прошёл"
else
    grep -q "эффект .Random." "$T/err" \
        && ok "закрывающая скобка в хвосте параметра судится" \
        || bad "красный, но не про Random [$(cat "$T/err")]"
fi

# --- 4з. метод протокола: слова `fn` на строке нет вовсе ------------------
# `@generate() Random -> T` — форма std/src/testing/property.nv:87.
mk_tree
printf '\ntype Gen protocol {\n    @generate() Random -> int\n}\n' >> "$T/tree/parse/parse.nv"
if run "$T/tree" "$T/prelude"; then
    bad "'@generate() Random -> int' прошло — строки без слова fn не судятся"
else
    grep -q "эффект .Random." "$T/err" \
        && ok "метод протокола без слова fn судится" \
        || bad "красный, но не про Random [$(cat "$T/err")]"
fi

# --- 5. те же три формы В main.nv — зелёные -------------------------------
# Дверь не судится вовсе: там эффектам стоять положено.
mk_tree
printf '\ntype Log effect {\n    fn write(s str)\n}\n\nfn helper(x str) Fs -> int {\n    with Os = real_os() { }\n    0\n}\n' >> "$T/tree/main.nv"
if run "$T/tree" "$T/prelude"; then
    ok "три формы в main.nv — зелёные (дверь не судится)"
else
    bad "дверь осуждена: $(cat "$T/err")"
fi

# --- 6. комментарий про `with Fs` — зелёный -------------------------------
mk_tree
printf '\n// Дверь ставит `with Fs = real_fs()` — здесь только рассказ о ней.\n/// А ещё бывает `type Log effect {` и `fn f(x str) Fs -> int`.\n' >> "$T/tree/sem/sem.nv"
if run "$T/tree" "$T/prelude"; then
    ok "комментарий с тремя формами — зелёный (комментарии не судятся)"
else
    bad "комментарий покраснел зря: $(cat "$T/err")"
fi

# --- 7. фикстура-строка с исходником Nova — зелёная -----------------------
# Уже лежит в чистом дереве (parse_test.nv), проверяем адресно: одиночная
# строка на новой строке и вторая многострочная фикстура.
mk_tree
printf '\ntest "ещё фикстура" {\n    assert(refused("fn q() Fs -> int => 0"))\n    ro s = "module z\\nwith Fs = real_fs() { }\\n"\n    assert(refused(s))\n}\n' >> "$T/tree/check/check.nv"
if run "$T/tree" "$T/prelude"; then
    ok "исходник Nova внутри кавычек — зелёный (фикстура есть данные, не способность)"
else
    bad "строковая фикстура покраснела зря: $(cat "$T/err")"
fi

# --- 8. символьный литерал с кавычкой не ослепляет стража -----------------
# Нарушение дописано ПОСЛЕ строки `b == ('"' as u8)`: наивный разборщик
# ушёл бы в вечную строку и молча пропустил бы всё, что ниже.
mk_tree
printf '\nfn later(x str) Fs -> int => 0\n' >> "$T/tree/parse/parse_test.nv"
if run "$T/tree" "$T/prelude"; then
    bad "нарушение ПОСЛЕ символьного литерала с кавычкой пропущено — страж ослеп молча"
else
    grep -q "parse_test.nv:[0-9]*: эффект .Fs." "$T/err" \
        && ok "после символьного литерала с кавычкой страж всё ещё видит" \
        || bad "красный, но не про parse_test.nv [$(cat "$T/err")]"
fi

# --- 9. имя эффекта ИЗ ПРЕЛЮДИИ — красный ---------------------------------
# Доказывает, что словарь читается из $3, а не зашит в стража.
mk_tree
printf '\nfn roll(n int) Random -> int => 0\n' >> "$T/tree/sem/sem.nv"
if run "$T/tree" "$T/prelude"; then
    bad "эффект Random из прелюдии не пойман — словарь имён не читается"
else
    grep -q "эффект .Random. в сигнатуре" "$T/err" \
        && ok "имя эффекта взято из прелюдии, а не из головы (Random пойман)" \
        || bad "красный, но не про Random [$(cat "$T/err")]"
fi

# --- 10. прелюдии нет: честная строка, суд по Fs/Os -----------------------
mk_tree
printf '\nfn roll(n int) Random -> int => 0\n' >> "$T/tree/sem/sem.nv"
if run "$T/tree" "$T/net-a-takogo"; then
    grep -q "прелюдия не прочитана" "$T/err" \
        && ok "без прелюдии: честная строка в stderr, Random вне словаря — зелёный" \
        || bad "без прелюдии промолчал: слепота без объявления [$(cat "$T/err")]"
else
    bad "без прелюдии покраснел на Random, хотя судить обещал только по Fs/Os"
fi
mk_tree
printf '\nfn boot() {\n    with Fs = real_fs() { }\n}\n' >> "$T/tree/sem/sem.nv"
if run "$T/tree" "$T/net-a-takogo"; then
    bad "без прелюдии проспал и Fs — деградация вышла полной слепотой"
else
    grep -q "обработчик" "$T/err" && ok "без прелюдии Fs/Os всё равно судятся" || bad "красный не про обработчик"
fi

# --- 11. двери нет — красный, не зелёный ----------------------------------
mk_tree
rm -f "$T/tree/main.nv"
if run "$T/tree" "$T/prelude"; then
    bad "дерево БЕЗ двери дало зелёный — страж потерял мишень молча (№519)"
else
    grep -q "дверь .* не найдена" "$T/err" \
        && ok "двери нет — красный, названо почему" \
        || bad "красный, но без объяснения про дверь [$(cat "$T/err")]"
fi

# --- 12. нет директории и нет .nv — судить нечего -------------------------
run "$T/absent" "$T/prelude"
grep -q "судить нечего (нет" "$T/out" && ok "нет директории — судить нечего" || bad "нет директории: ждали «судить нечего» [$(cat "$T/out")]"
mkdir -p "$T/empty"
run "$T/empty" "$T/prelude"
grep -q "судить нечего (нет \*.nv" "$T/out" && ok "директория без .nv — судить нечего" || bad "пустая директория: ждали «судить нечего» [$(cat "$T/out")]"

# --- 13. настоящее дерево -------------------------------------------------
if sh "$G" "$ROOT" >/dev/null 2>&1; then
    ok "настоящее дерево — зелёное"
else
    bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -5)"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-effects-at-door ok: все случаи — три формы ниже двери, строка эффектов во всех её видах (ряд через пробел, аргументы типа, скобка в хвосте параметра, метод без fn), те же формы у двери, ложные тревоги (комментарий, фикстура-строка, символьный литерал, выражение) и потеря мишени"
    exit 0
fi
exit 1
