#!/bin/sh
# Самотест check-novac-module-tests.sh.
#
# Доказывает не «страж запускается», а что он КРАСНЕЕТ ровно там, где обязан:
# на упавшем тесте, на отсутствии тестов и на прогоне без строки итога. Первое
# — сам смысл стража; второе — вырожденный случай, при котором зелёное молчание
# означало бы «модули не проверяются вовсе»; третье — реестр №645: ноль без
# строки это «не упал», а не «проверил».
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-novac-module-tests.sh"
T="${TMPDIR:-/tmp}/novac-module-tests-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Оракул ищется ДВЕРЬЮ, а не своим списком имён: свой список знал только
# `nova.exe`, и на Linux-раннере (бинарь `nova`) живой случай молча уходил в
# «оракула нет — пропущен» (реестр 221.1 №1826, класс K-A).
. "$GD/lib/novac.sh"
ORACLE="$(novac_find_oracle "$ROOT" || true)"

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ───────────────────────────────
if [ -f "$ORACLE" ]; then
    if sh "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
        if grep -q "^check-novac-module-tests ok:" "$T/out"; then
            ok "живое дерево — зелёный со строкой ok:"
        else
            bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
        fi
    else
        bad "живое дерево красное: [$(head -n 2 "$T/err")]"
    fi
else
    ok "оракула нет — случай живого дерева пропущен осознанно"
fi

# ── 2. ПАДАЮЩИЙ тест — обязан быть красный ────────────────────────────────
FAKE="$T/tree/novac/src/toy"
mkdir -p "$FAKE"
cat > "$FAKE/toy_test.nv" <<'NV'
module toy

test "this test exists to fail" {
    assert(1 == 2)
}
NV
if [ -f "$ORACLE" ]; then
    if sh "$G" "$T/tree" "$ORACLE" > "$T/out2" 2> "$T/err2"; then
        bad "падающий тест не покраснел: [$(head -n 1 "$T/out2")]"
    else
        if grep -q "модульных тестов упало" "$T/err2"; then
            ok "падающий тест — красный, и назван причиной"
        else
            bad "красный, но не про упавший тест: [$(head -n 1 "$T/err2")]"
        fi
    fi
else
    ok "оракула нет — случай падающего теста пропущен осознанно"
fi

# ── 3. НИ ОДНОГО теста — красный (зелёное молчание было бы ложью) ─────────
EMPTY="$T/empty/novac/src/mod"
mkdir -p "$EMPTY"
echo "module mod" > "$EMPTY/mod.nv"
if sh "$G" "$T/empty" > "$T/out3" 2> "$T/err3"; then
    bad "дерево без тестов зелёное: [$(head -n 1 "$T/out3")]"
else
    if grep -q "ни одного" "$T/err3"; then
        ok "дерево без тестов — красный"
    else
        bad "красный, но не про отсутствие тестов: [$(head -n 1 "$T/err3")]"
    fi
fi

# ── 4. нет novac/src вовсе — честное «судить нечего» ──────────────────────
mkdir -p "$T/bare"
if sh "$G" "$T/bare" > "$T/out4" 2>&1; then
    if grep -q "судить нечего" "$T/out4"; then
        ok "нет novac/src — судить нечего"
    else
        bad "зелёный без честной формулировки: [$(head -n 1 "$T/out4")]"
    fi
else
    bad "отсутствие novac/src сделано красным: [$(head -n 1 "$T/out4")]"
fi

# ── 5. бинарь-заглушка без строки итога — красный (реестр №645) ───────────
cat > "$T/mute.sh" <<'SH'
#!/bin/sh
echo "running tests..."
exit 0
SH
chmod +x "$T/mute.sh"
if sh "$G" "$ROOT" "$T/mute.sh" > "$T/out5" 2> "$T/err5"; then
    bad "прогон без строки итога сочтён проверкой: [$(head -n 1 "$T/out5")]"
else
    if grep -q "строку итога" "$T/err5"; then
        ok "ноль без строки итога — красный"
    else
        bad "красный, но не про строку итога: [$(head -n 1 "$T/err5")]"
    fi
fi

# -- 6-9. СЧЁТ ИСХОДОВ. Заглушка-оракул вместо настоящего: случаи должны быть
#         ДЕТЕРМИНИРОВАНЫ, а живой корпус меняет число файлов на каждом новом
#         модуле. Дерево строим своё, ровно из двух тестовых файлов, поэтому
#         N == 2 и все четыре ожидания считаются на бумаге, а не подгоняются.
#         Заведено 2026-09-08: страж принимал `PASS: 0  FAIL: 0` за успех —
#         строка итога есть, провалов нет, значит «ok». Прогон, не исполнивший
#         ни одного теста, читался как доказательство (класс №1040/№1041).
TWO="$T/two/novac/src"
mkdir -p "$TWO/a" "$TWO/b"
echo "module a" > "$TWO/a/a_test.nv"
echo "module b" > "$TWO/b/b_test.nv"

# $1 — строка итога заглушки; $2 — PASS|FAIL; $3 — имя случая; $4 — что
# обязано найтись в выводе (пусто = не проверять текст).
stub_case() {
    _line="$1"; _want="$2"; _name="$3"; _needle="$4"
    _stub="$T/stub.sh"
    printf '#!/bin/sh\necho "%s"\nexit 0\n' "$_line" > "$_stub"
    chmod +x "$_stub"
    if sh "$G" "$T/two" "$_stub" > "$T/outS" 2> "$T/errS"; then
        _got=PASS
    else
        _got=FAIL
    fi
    if [ "$_got" != "$_want" ]; then
        bad "$_name: получено $_got, ожидалось $_want [$(head -n 1 "$T/outS")$(head -n 1 "$T/errS")]"
        return
    fi
    if [ -n "$_needle" ] && ! grep -q "$_needle" "$T/outS" "$T/errS"; then
        bad "$_name: вердикт $_got верный, но без слов «$_needle»"
        return
    fi
    ok "$_name"
}

stub_case "PASS: 0  FAIL: 0" FAIL \
    "ноль исполненных при двух файлах — красный" "НОЛЬ тестов"
stub_case "PASS: 1  FAIL: 0" FAIL \
    "счёт не сошёлся (1 из 2) — красный" "счёт не сошёлся"
stub_case "PASS: 1  FAIL: 0  SKIP: 1 (skipped)" PASS \
    "пропуск учтён: 1+1=2 — зелёный, но сказано вслух" "пропущено модулей"
stub_case "PASS: 2  FAIL: 0" PASS \
    "здоровый случай: 2 из 2 — зелёный" "счёт сошёлся"

# -- 10-12. ЕДИНИЦА ПРОГОНА — КАТАЛОГ МОДУЛЯ (правка 2026-10-01). Раннер
#          собирает папку-модуль одной единицей вместе со ВСЕМИ её `*_test.nv`,
#          и прежний страж, передавая файлы поштучно, собирал модуль novac
#          `pipeline` 23 раза (CI обрывал шаг пределом 600с). Клетки держат
#          три стороны новой единицы: страж передаёт КАТАЛОГ, а не файлы (10);
#          в каталоге проверяется и НЕ первый файл (11); каталог из двух
#          модулей не проходит молча (12).
PAIR="$T/pair/novac/src/one"
mkdir -p "$PAIR"
echo "module one" > "$PAIR/x_test.nv"
echo "module one" > "$PAIR/y_test.nv"
cat > "$T/argstub.sh" <<SH
#!/bin/sh
printf '%s\n' "\$@" > "$T/args"
echo "PASS: 1  FAIL: 0"
exit 0
SH
chmod +x "$T/argstub.sh"
if sh "$G" "$T/pair" "$T/argstub.sh" > "$T/out10" 2> "$T/err10"; then
    if grep -q '_test\.nv' "$T/args"; then
        bad "страж передал оракулу ФАЙЛЫ, а не каталог: [$(tr '\n' ' ' < "$T/args")]"
    elif [ "$(grep -c '/novac/src/one$' "$T/args")" -ne 1 ]; then
        bad "каталог модуля не передан ровно один раз: [$(tr '\n' ' ' < "$T/args")]"
    elif ! grep -q "модулей 1 (файлов 2)" "$T/out10"; then
        bad "зелёный, но не назвал модули и файлы: [$(head -n 1 "$T/out10")]"
    else
        ok "два тестовых файла одного модуля — один каталог оракулу, зелёный"
    fi
else
    bad "каталог из двух файлов одного модуля красный: [$(head -n 2 "$T/err10")]"
fi

if [ -f "$ORACLE" ]; then
    LIVE="$T/live/novac/src/one"
    mkdir -p "$LIVE"
    printf 'module one\n\ntest "first passes" {\n    assert(1 == 1)\n}\n' > "$LIVE/a_test.nv"
    printf 'module one\n\ntest "second fails" {\n    assert(1 == 2)\n}\n' > "$LIVE/b_test.nv"
    if sh "$G" "$T/live" "$ORACLE" > "$T/out11" 2> "$T/err11"; then
        bad "падение во ВТОРОМ файле модуля не покраснело: [$(head -n 1 "$T/out11")]"
    elif grep -q "модульных тестов упало" "$T/err11" && grep -q "b_test.nv" "$T/err11"; then
        ok "падение в не первом файле модуля — красный, файл назван"
    else
        bad "красный, но не про b_test.nv: [$(head -n 2 "$T/err11")]"
    fi

    MIX="$T/mix/novac/src/two"
    mkdir -p "$MIX"
    printf 'module ma\n\ntest "ma" {\n    assert(1 == 1)\n}\n' > "$MIX/a_test.nv"
    printf 'module mb\n\ntest "mb" {\n    assert(1 == 1)\n}\n' > "$MIX/b_test.nv"
    if sh "$G" "$T/mix" "$ORACLE" > "$T/out12" 2> "$T/err12"; then
        bad "каталог из двух модулей прошёл молча: [$(head -n 1 "$T/out12")]"
    elif grep -q "счёт не сошёлся" "$T/err12"; then
        ok "каталог из двух модулей — красный по счёту"
    else
        bad "красный, но не про счёт: [$(head -n 2 "$T/err12")]"
    fi
else
    ok "оракула нет — клетки 11-12 пропущены осознанно"
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
exit 0
