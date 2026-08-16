#!/bin/sh
# Самотест check-novac-one-door-export.sh (П16: страж обязан ДОКАЗАТЬ, что ловит).
#
# ПОДЛОЖКА. У стража один шов — $2 (сканируемая директория), поэтому каждый
# случай собирается крошечным деревом во временном каталоге: модуль = папка,
# файл прямо в корне = псевдомодуль main. Настоящее дерево тоже прогоняется,
# последним случаем.
#
# ЧТО ДОКАЗЫВАЕТСЯ. Две стороны, обе обязательны:
#   ЛОВИТ  — одно имя свободной функции / имя типа / метод одного типа,
#            экспортированные из ДВУХ модулей, дают красный с обоими местами;
#   НЕ ЛОВИТ ЛИШНЕГО — одноимённые методы РАЗНЫХ типов, одноимённые
#            конструкторы .new разных типов, метод СРЕЗА рядом с методом его
#            элемента ([]u8 @to_str vs u8 @to_str), повтор имени внутри ОДНОГО
#            модуля и совпадения в *_test.nv остаются зелёными. Ложняк здесь
#            дорог: в настоящем дереве и @find, и .new встречаются в двух
#            модулях сразу (sem и names) — наивный страж покраснел бы на живом
#            коде.
# Пары 15/16 и 18 стерегут именно ключ: каждая имеет зелёную и красную сторону,
# потому что «различить» и «не потерять» — две разные ошибки, и лечатся они в
# противоположные стороны. Случай 17 стережёт ОХВАТ: модификатор двери (unsafe)
# не должен уводить её из-под суда — такой пропуск виден только красным тестом.
export LC_ALL=C
GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-one-door-export.sh"
T="${TMPDIR:-/tmp}/novac-one-door-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0
fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }
run() { sh "$G" "$ROOT" "$1" > "$T/out" 2> "$T/err"; }

# mkf <путь файла> <строка>... — файл с шапкой module и заданными строками.
# Первая строка файла всегда "module m", значит содержимое начинается со 2-й.
mkf() {
    f="$1"; shift
    mkdir -p "$(dirname "$f")"
    { echo "module m"
      for l in "$@"; do echo "$l"; done
    } > "$f"
}

# --- 1. чистое дерево — зелёный с числом ----------------------------------
mkf "$T/t1/a/a.nv" "export fn alpha() -> int => 0" "export type Alpha value {"
mkf "$T/t1/b/b.nv" "export fn beta() -> int => 0" "export type Beta value {"
if run "$T/t1"; then
    grep -q "дверей: 4 в 2 модулях" "$T/out" && ok "чистое дерево — зелёное, итог числом" \
        || bad "зелёное, но итог не тот [$(cat "$T/out")]"
else
    bad "чистое дерево покраснело: $(cat "$T/err")"
fi

# --- 2. одна свободная функция из двух модулей — красный (главный случай) --
mkf "$T/t2/a/a.nv" "export fn collect(file Node) -> Ctx {"
mkf "$T/t2/b/b.nv" "export fn collect(file Node) -> Ctx {"
if run "$T/t2"; then
    bad "две двери в collect прошли — страж не ловит свой главный случай"
else
    if grep -q "collect" "$T/err" && grep -q "a/a.nv:2" "$T/err" && grep -q "b/b.nv:2" "$T/err"; then
        ok "две двери в collect пойманы, оба места названы"
    else
        bad "красный, но без имени/обоих мест [$(cat "$T/err")]"
    fi
fi

# --- 3. один тип из двух модулей — красный --------------------------------
mkf "$T/t3/sem/sem.nv" "export type Ctx {" "    defs DefTable"
mkf "$T/t3/emit_c/emit_c.nv" "export type Ctx {" "    buf str"
if run "$T/t3"; then
    bad "export type Ctx из двух модулей прошёл — тип не судится"
else
    if grep -q "Ctx" "$T/err" && grep -q "sem/sem.nv:2" "$T/err" && grep -q "emit_c/emit_c.nv:2" "$T/err"; then
        ok "два типа Ctx пойманы, оба места названы"
    else
        bad "красный, но без имени типа/обоих мест [$(cat "$T/err")]"
    fi
fi

# --- 4. одноимённые методы РАЗНЫХ типов — зелёный (ложняк №1) -------------
mkf "$T/t4/a/a.nv" "export type A {" "export fn A @len() -> int => 0"
mkf "$T/t4/b/b.nv" "export type B {" "export fn B @len() -> int => 0"
if run "$T/t4"; then
    ok "A @len и B @len в разных модулях — зелёное (пространства разные)"
else
    bad "ложняк: одноимённые методы разных типов покраснели: $(cat "$T/err")"
fi

# --- 5. два файла ОДНОГО модуля, разные экспорты — зелёный ----------------
mkf "$T/t5/sem/sem.nv" "export fn collect(file Node) -> Ctx {"
mkf "$T/t5/sem/mangle.nv" "export fn c_fn(name str) -> str {"
if run "$T/t5"; then
    ok "два файла одной папки с разными экспортами — зелёное"
else
    bad "файлы одного модуля покраснели: $(cat "$T/err")"
fi

# --- 6. одноимённые конструкторы разных типов — зелёный (ложняк №2) -------
# Живой паттерн: Scope.new в sem и NameTable.new в names.
mkf "$T/t6/sem/sem.nv" "export fn Scope.new() -> Self => {}"
mkf "$T/t6/names/names.nv" "export fn NameTable.new() -> Self => {}"
if run "$T/t6"; then
    ok "Scope.new и NameTable.new — зелёное (конструктор под своим типом)"
else
    bad "ложняк: .new разных типов покраснел: $(cat "$T/err")"
fi

# --- 7. одно имя ДВАЖДЫ внутри одного модуля — зелёный (ложняк №3) --------
mkf "$T/t7/sem/one.nv" "export fn helper(x int) -> int => x"
mkf "$T/t7/sem/two.nv" "export fn helper(x str) -> str => x"
if run "$T/t7"; then
    ok "повтор имени внутри одного модуля — зелёное (файлы папки co-equal)"
else
    bad "ложняк: повтор внутри модуля покраснел: $(cat "$T/err")"
fi

# --- 8. дженерики и mut не различают и не склеивают двери -----------------
mkf "$T/t8/dq/deque.nv" "export fn Deque[T] mut @push(x T) -> () {"
mkf "$T/t8/st/stack.nv" "export fn Stack[T] mut @push(x T) -> () {"
if run "$T/t8"; then
    ok "Deque[T] mut @push и Stack[T] mut @push — зелёное"
else
    bad "ложняк: дженерик-методы разных типов покраснели: $(cat "$T/err")"
fi

# --- 9. совпадение только в тестах — зелёный (ложняк №4) -----------------
mkf "$T/t9/a/a.nv" "export fn alpha() -> int => 0"
mkf "$T/t9/a/a_test.nv" "export fn collect(file Node) -> Ctx {"
mkf "$T/t9/b/b_test.nv" "export fn collect(file Node) -> Ctx {"
if run "$T/t9"; then
    ok "коллизия внутри *_test.nv — зелёное (тесты не двери)"
else
    bad "ложняк: тесты попали под суд: $(cat "$T/err")"
fi

# --- 10. ОДИН И ТОТ ЖЕ метод одного типа из двух модулей — красный -------
mkf "$T/t10/sem/sem.nv" "export fn FnTable @lookup(n str) -> FnLookup {"
mkf "$T/t10/check/check.nv" "export fn FnTable @lookup(n str) -> FnLookup {"
if run "$T/t10"; then
    bad "FnTable @lookup из двух модулей прошёл — вторая дверь метода не судится"
else
    grep -q "FnTable@lookup" "$T/err" && ok "вторая дверь одного метода поймана" \
        || bad "красный, но ключ FnTable@lookup не назван [$(cat "$T/err")]"
fi

# --- 11. файл в корне — модуль main, и он тоже судится -------------------
mkf "$T/t11/main.nv" "export fn compile(text str) -> str {"
mkf "$T/t11/pipeline/pipeline.nv" "export fn compile(text str) -> str {"
if run "$T/t11"; then
    bad "коллизия main.nv с модулем прошла — корневые файлы не судятся"
else
    if grep -q "модуль main" "$T/err" && grep -q "модуль pipeline" "$T/err"; then
        ok "main.nv судится как модуль main, оба модуля названы"
    else
        bad "красный, но main/pipeline не названы [$(cat "$T/err")]"
    fi
fi

# --- 12. .nv есть, экспортов нет — красный, не вечнозелёный --------------
mkf "$T/t12/a/a.nv" "fn priv() -> int => 0" "type Hidden value {"
if run "$T/t12"; then
    bad "дерево без единого экспорта дало ЗЕЛЁНЫЙ — страж потерял мишень молча (класс №519)"
else
    grep -q "потерял мишень" "$T/err" && ok "разбор опустел — красный, названо почему" \
        || bad "красный, но без объяснения [$(cat "$T/err")]"
fi

# --- 13. закомментированный экспорт не считается (ложняк №5) -------------
# Двери ищутся только с начала строки: упоминание в комментарии или в доке —
# это разговор о двери, а не дверь.
mkf "$T/t13/a/a.nv" "export fn alpha() -> int => 0" \
    "// export fn collect(f Node) -> Ctx {" "/// см. также export type Ctx"
mkf "$T/t13/b/b.nv" "export fn beta() -> int => 0" \
    "// export fn collect(f Node) -> Ctx {" "/// см. также export type Ctx"
if run "$T/t13"; then
    ok "упоминание экспорта в комментарии/доке — зелёное"
else
    bad "ложняк: комментарий засчитан дверью: $(cat "$T/err")"
fi

# --- 14. CRLF и export const не считаются (ложняк №6) --------------------
# Файлы под Windows приходят с CRLF; const — не операция и дверью не является.
mkdir -p "$T/t14/a" "$T/t14/b"
printf 'module m\r\nexport const ENTRY = "main"\r\nexport fn alpha(x int) -> int => x\r\nexport fn alpha(x str) -> str => x\r\n' > "$T/t14/a/a.nv"
printf 'module m\r\nexport const ENTRY = "main"\r\nexport fn beta() -> int => 0\r\n' > "$T/t14/b/b.nv"
if run "$T/t14"; then
    grep -q "дверей: 2 в 2 модулях" "$T/out" \
        && ok "CRLF, одноимённые const и перегрузки в модуле — зелёное" \
        || bad "зелёное, но итог не тот [$(cat "$T/out")]"
else
    bad "ложняк: CRLF/const покраснели: $(cat "$T/err")"
fi

# --- 15. приёмник-срез []T — СВОЙ тип, а не T (ложняк №7) ----------------
# Живая форма std: `export fn []u8 @to_str` и `export fn u8 @to_str` — две
# РАЗНЫЕ двери. Ключ, снимающий `[]` заодно с дженериками, склеивает их и
# краснеет на честном коде.
mkf "$T/t15/bytes/bytes.nv" "export fn []u8 @to_str() -> str {"
mkf "$T/t15/chars/chars.nv" "export fn u8 @to_str() -> str {"
if run "$T/t15"; then
    grep -q "дверей: 2 в 2 модулях" "$T/out" \
        && ok "[]u8 @to_str и u8 @to_str — зелёное (срез — свой тип)" \
        || bad "зелёное, но итог не тот [$(cat "$T/out")]"
else
    bad "ложняк: метод среза склеен с методом элемента: $(cat "$T/err")"
fi

# --- 16. ...но ОДИН И ТОТ ЖЕ метод среза из двух модулей — красный -------
# Обратная сторона случая 15: сохранить `[]` в ключе нужно так, чтобы срез не
# вывел метод из-под суда вообще.
mkf "$T/t16/a/a.nv" "export fn []int mut @sort() -> () {"
mkf "$T/t16/b/b.nv" "export fn []int @sort() -> () {"
if run "$T/t16"; then
    bad "[]int @sort из двух модулей прошёл — срез вывел метод из-под суда"
else
    grep -qF "[]int@sort" "$T/err" && ok "вторая дверь метода среза поймана" \
        || bad "красный, но ключ []int@sort не назван [$(cat "$T/err")]"
fi

# --- 17. export unsafe fn — та же дверь под модификатором ----------------
# Живая форма std (`export unsafe fn []u8 @to_str_unchecked`). Модификатор не
# делает дверь другой; страж, не знающий слова unsafe, пропускает вторую дверь
# МОЛЧА — зелёный отчёт при живом нарушении. Декой-экспорт в модуле a нужен,
# чтобы пропуск проявился именно зелёным, а не срабатыванием анти-вечнозелёности.
mkf "$T/t17/a/a.nv" "export fn decoy() -> int => 0" \
    "export unsafe fn raw_copy(src *u8, dst *mut u8) -> () {"
mkf "$T/t17/b/b.nv" "export unsafe fn raw_copy(src *u8, dst *mut u8) -> () {"
if run "$T/t17"; then
    bad "две двери raw_copy под unsafe прошли МОЛЧА — модификатор скрыл дверь"
else
    grep -q "raw_copy" "$T/err" && ok "unsafe-дверь судится наравне, вторая поймана" \
        || bad "красный, но raw_copy не назван [$(cat "$T/err")]"
fi

# --- 18. дженерик с ОГРАНИЧЕНИЕМ у свободной функции --------------------
# `fn sortx[T Compare](` — пробел ВНУТРИ скобок. Если голову разбирать до
# вычистки дженериков, она распадётся на «тип sortx» + «метод Compare», и две
# настоящие двери разойдутся по разным ключам: тихий пропуск, не ложняк.
mkf "$T/t18/a/a.nv" "export fn sortx[T Compare](xs []T) -> []T {"
mkf "$T/t18/b/b.nv" "export fn sortx[U Hash + Eq, V](xs []U) -> []U {"
if run "$T/t18"; then
    bad "sortx из двух модулей прошёл — ограничение в дженерике увело ключ"
else
    grep -q "свободная функция sortx" "$T/err" \
        && ok "ограниченный дженерик — ключ по имени функции, вторая дверь поймана" \
        || bad "красный, но ключ не «свободная функция sortx» [$(cat "$T/err")]"
fi

# --- 19. нет каталога / каталог без .nv — судить нечего ------------------
run "$T/absent"
grep -q "судить нечего (нет" "$T/out" && ok "нет каталога — судить нечего, exit 0" \
    || bad "нет каталога: ждали «судить нечего» [$(cat "$T/out")$(cat "$T/err")]"
mkdir -p "$T/t15empty/a"
run "$T/t15empty"
grep -q "судить нечего (нет .nv" "$T/out" && ok "каталог без .nv — судить нечего, exit 0" \
    || bad "пустой каталог: ждали «судить нечего» [$(cat "$T/out")$(cat "$T/err")]"

# --- 20. настоящее дерево ------------------------------------------------
if sh "$G" "$ROOT" >/dev/null 2>&1; then
    ok "настоящее дерево novac/src — зелёное"
else
    bad "настоящее дерево покраснело: $(sh "$G" "$ROOT" 2>&1 | head -5)"
fi

echo "итог: FAIL $fails"
if [ "$fails" -eq 0 ]; then
    echo "test-check-novac-one-door-export ok: ловит вторые двери (функция, тип, метод, метод среза, unsafe, ограниченный дженерик, main) и не ловит одноимённые методы/конструкторы разных типов и метод среза рядом с методом его элемента"
    exit 0
fi
exit 1
