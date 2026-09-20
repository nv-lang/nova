#!/bin/sh
# Самотест check-ecode-fixture-debt.py — стража строки реестра 221.1 №1187.
#
# Доказывает мутацией обе стороны КАЖДОГО правила, а не только «ловит плохое»:
# новый код без фикстуры краснеет; код с живой фикстурой — нет; код, живущий
# ТОЛЬКО в комментарии, в знаменатель не входит; затенённый с НАЗВАННЫМ
# перехватчиком законен, а без имени — долг. Последняя пара и есть решение
# интегратора 2026-09-20: затенение — третье состояние, и оно обязано быть
# видимым, иначе «ловится раньше законно» неотличимо от «код мёртв».
#
# План/реестр: реестр 221.1 №1187.
export LC_ALL=C

SD="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SD/../../.." && pwd)"
G="$ROOT/scripts/guards/check-ecode-fixture-debt.py"
T="${TMPDIR:-/tmp}/ecode-debt-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# Игрушечное дерево: src компилятора + корпус фикстур + пустая база.
make_tree() { # каталог
    mkdir -p "$1/compiler-codegen/src" "$1/spec_tests/conformance/neg"
    printf 'debt=0\n' > "$1/base"
}
run() { # каталог -> код возврата, вывод в $T/out
    NOVA_ECODE_DEBT_BASELINE="$1/base" python "$G" "$1" > "$T/out" 2> "$T/err"
}

# ── 1. живое дерево — зелёное со строкой ok: и со знаменателями ──────────
if python "$G" "$ROOT" > "$T/live" 2> "$T/liveerr"; then
    if grep -q "^check-ecode-fixture-debt ok:" "$T/live" \
       && grep -q "кодов в исполнимом коде" "$T/live"; then
        ok "живое дерево зелёное, знаменатели напечатаны"
    else
        bad "зелёное без строки ok: или без знаменателей [$(head -n 1 "$T/live")]"
    fi
else
    bad "живое дерево красное: [$(head -n 2 "$T/liveerr")]"
fi

# ── 2. новый код без фикстуры — красный, и отказ НАЗЫВАЕТ код ───────────
t2="$T/t2"; make_tree "$t2"
printf 'fn f() { diag("[E_NAKED_ONE] boom"); }\n' > "$t2/compiler-codegen/src/a.rs"
if run "$t2"; then
    bad "код без фикстуры прошёл: [$(head -n 1 "$T/out")]"
else
    grep -q "E_NAKED_ONE" "$T/err" \
        && ok "код без фикстуры — красный, и отказ называет код" \
        || bad "красный, но без имени кода: [$(head -n 1 "$T/err")]"
fi

# ── 3. тот же код с ЖИВОЙ строкой ожидания — зелёный ────────────────────
t3="$T/t3"; make_tree "$t3"
printf 'fn f() { diag("[E_NAKED_ONE] boom"); }\n' > "$t3/compiler-codegen/src/a.rs"
printf '// EXPECT_COMPILE_ERROR E_NAKED_ONE\nfn main() Io -> () => print("x")\n' \
    > "$t3/spec_tests/conformance/neg/a.nv"
if run "$t3"; then
    ok "код с живой фикстурой законен"
else
    bad "фикстура не засчитана: [$(head -n 2 "$T/err")]"
fi

# ── 4. упоминание в ПОЯСНЕНИИ фикстуры — НЕ покрытие ────────────────────
# Живой случай замера 1187: упоминание кода в историческом пояснении выглядит
# как ожидание. Если засчитать его, комментарий начнёт закрывать долг.
t4="$T/t4"; make_tree "$t4"
printf 'fn f() { diag("[E_NAKED_ONE] boom"); }\n' > "$t4/compiler-codegen/src/a.rs"
printf '// EXPECT_COMPILE_ERROR something else\n// раньше здесь был E_NAKED_ONE\nfn main() Io -> () => print("x")\n' \
    > "$t4/spec_tests/conformance/neg/a.nv"
if run "$t4"; then
    bad "упоминание в пояснении засчитано как фикстура: [$(head -n 1 "$T/out")]"
else
    ok "упоминание в пояснении фикстурой не считается"
fi

# ── 5. код ТОЛЬКО в комментарии — в знаменатель не входит ───────────────
# Обратная сторона предиката, требование интегратора. Из 417 имён 39 живут
# только в комментариях, и часть — обрывки вида E_BANG_.
t5="$T/t5"; make_tree "$t5"
printf '// смотри E_ONLY_IN_COMMENT и E_BANG_\nfn f() { }\n' > "$t5/compiler-codegen/src/a.rs"
printf 'fn g() { diag("[E_REAL_ONE] x"); }\n' > "$t5/compiler-codegen/src/b.rs"
printf '// EXPECT_COMPILE_ERROR E_REAL_ONE\nfn main() Io -> () => print("x")\n' \
    > "$t5/spec_tests/conformance/neg/b.nv"
if run "$t5"; then
    # В комментарии ДВА имени: обычное и обрывок `E_BANG_` — ровно тот вид,
    # из-за которого предикат и считает по исполнимому коду.
    if grep -q "только в комментариях 2" "$T/out" && ! grep -q "E_ONLY_IN_COMMENT" "$T/out"; then
        ok "код из комментария в долг не входит и посчитан отдельно"
    else
        bad "комментарный код посчитан не так: [$(head -n 1 "$T/out")]"
    fi
else
    bad "комментарный код сделан долгом: [$(head -n 2 "$T/err")]"
fi

# ── 6. ЗАТЕНЁННЫЙ с названным перехватчиком — зелёный ───────────────────
t6="$T/t6"; make_tree "$t6"
{ printf '// nova:shadowed-by E_INTERCEPTOR - forma lovitsya ranshe\n';
  printf 'fn f() { diag("[E_SHADOWED_ONE] boom"); }\n'; } > "$t6/compiler-codegen/src/a.rs"
printf 'fn g() { diag("[E_INTERCEPTOR] x"); }\n' > "$t6/compiler-codegen/src/b.rs"
printf '// EXPECT_COMPILE_ERROR E_INTERCEPTOR\nfn main() Io -> () => print("x")\n' \
    > "$t6/spec_tests/conformance/neg/b.nv"
if run "$t6"; then
    grep -q "затенённых с именем перехватчика 1" "$T/out" \
        && ok "затенённый с названным перехватчиком законен и посчитан отдельно" \
        || bad "зелёный, но затенение не посчитано: [$(head -n 1 "$T/out")]"
else
    bad "затенённый с именем перехватчика покраснел: [$(head -n 2 "$T/err")]"
fi

# ── 7. затенение БЕЗ имени перехватчика — красный ───────────────────────
# Сердцевина правила: без имени неотличимо «перехвачен законно» от «мёртв».
t7="$T/t7"; make_tree "$t7"
{ printf '// nova:shadowed-by - forma lovitsya ranshe\n';
  printf 'fn f() { diag("[E_SHADOWED_ONE] boom"); }\n'; } > "$t7/compiler-codegen/src/a.rs"
if run "$t7"; then
    bad "затенение без имени перехватчика прошло: [$(head -n 1 "$T/out")]"
else
    grep -q "E_SHADOWED_ONE" "$T/err" \
        && ok "затенение без имени перехватчика — долг" \
        || bad "красный, но не про этот код: [$(head -n 1 "$T/err")]"
fi

# ── 8. пометка ДАЛЬШЕ ЧЕМ СТРОКА НАД вхождением не действует ────────────
# Та же причина, что у nova:allow: пометка, живущая «где-то рядом», начинает
# покрывать соседей.
t8="$T/t8"; make_tree "$t8"
{ printf '// nova:shadowed-by E_INTERCEPTOR\n'; printf '\n';
  printf 'fn f() { diag("[E_FAR_ONE] boom"); }\n'; } > "$t8/compiler-codegen/src/a.rs"
if run "$t8"; then
    bad "пометка через строку засчитана: [$(head -n 1 "$T/out")]"
else
    ok "пометка не вплотную над вхождением не действует"
fi

# ── 9. ПУСТАЯ МИШЕНЬ — красный, а не «чисто» ────────────────────────────
t9="$T/t9"; make_tree "$t9"
printf 'fn f() { }\n' > "$t9/compiler-codegen/src/a.rs"
if run "$t9"; then
    bad "дерево без единого кода прошло: [$(head -n 1 "$T/out")]"
else
    grep -q "пустая мишень" "$T/err" \
        && ok "ноль кодов — красный, названный пустой мишенью" \
        || bad "красный, но не про пустую мишень: [$(head -n 1 "$T/err")]"
fi

# ── 10. СУЖЕНИЕ ПРЕДИКАТА — красный (мутируем СУДЬЮ) ────────────────────
# Копия стража перестаёт перечислять исходники вовсе. Имя из базы пропадает из
# набора, ЧИСЛО ПАДАЕТ — и это прочлось бы как погашенный долг. Идентификатор
# при этом в дереве есть, и именно так отличается сужение от починки.
t10="$T/t10"; make_tree "$t10"
printf 'fn f() { diag("[E_NAKED_ONE] boom"); }\n' > "$t10/compiler-codegen/src/a.rs"
printf 'debt=1\ndebt_code=E_NAKED_ONE\n' > "$t10/base"
if run "$t10"; then
    ok "контроль: при широком предикате база сходится"
else
    bad "контроль не прошёл, дальнейшее ничего не докажет: [$(head -n 2 "$T/err")]"
fi
sed 's|SRC_DIRS = ("compiler-codegen/src",)|SRC_DIRS = ("nowhere",)|' "$G" > "$T/narrow.py"
NOVA_ECODE_DEBT_BASELINE="$t10/base" python "$T/narrow.py" "$t10" > "$T/o10" 2> "$T/e10"
if [ $? -eq 0 ]; then
    bad "сужение предиката прошло как зелёное: [$(head -n 1 "$T/o10")]"
elif grep -q "пустая мишень\|предикат сузился" "$T/e10"; then
    ok "сужение предиката — красный"
else
    bad "красный, но не про сужение: [$(head -n 1 "$T/e10")]"
fi

# ── 11. ПОГАШЕНИЕ долга — зелёный С СОВЕТОМ, а не красный ───────────────
t11="$T/t11"; make_tree "$t11"
printf 'fn f() { diag("[E_NAKED_ONE] boom"); }\n' > "$t11/compiler-codegen/src/a.rs"
printf '// EXPECT_COMPILE_ERROR E_NAKED_ONE\nfn main() Io -> () => print("x")\n' \
    > "$t11/spec_tests/conformance/neg/a.nv"
printf 'debt=1\ndebt_code=E_NAKED_ONE\n' > "$t11/base"
if run "$t11"; then
    grep -q "долг СНИЗИЛСЯ" "$T/out" \
        && ok "погашенный долг — зелёный и назван вслух" \
        || bad "погашение прошло молча: [$(head -n 1 "$T/out")]"
else
    bad "погашение покрашено: [$(head -n 2 "$T/err")]"
fi

# ── 12. база противоречит себе (счёт против имён) — красный ─────────────
t12="$T/t12"; make_tree "$t12"
printf 'fn f() { diag("[E_NAKED_ONE] boom"); }\n' > "$t12/compiler-codegen/src/a.rs"
printf 'debt=5\ndebt_code=E_NAKED_ONE\n' > "$t12/base"
if run "$t12"; then
    bad "расхождение счёта и имён прошло: [$(head -n 1 "$T/out")]"
else
    grep -q "противоречит себе" "$T/err" \
        && ok "расхождение счёта и имён — красный" \
        || bad "красный, но не про расхождение: [$(head -n 1 "$T/err")]"
fi

if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
echo "test-check-ecode-fixture-debt ok: обе стороны каждого правила, включая затенение с именем и без, комментарный код, пустую мишень и сужение предиката"
exit 0
