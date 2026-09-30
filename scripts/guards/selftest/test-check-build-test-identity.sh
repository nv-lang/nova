#!/usr/bin/env bash
# test-check-build-test-identity.sh — САМОТЕСТ инструмента
# `scripts/tools/check-build-test-identity.sh`.
#
# Почему самотест обязателен (то же требование, что у всех соседних
# стражей в scripts/guards/selftest/ — механизм принуждения без
# собственного теста — это доверие на слово). Инструмент обязан доказать:
#   (1) ЛОВИТ искусственно внесённое расхождение (не пропускает);
#   (2) НЕ ловит известное легитимное расхождение (короткий список
#       исключений в check-build-test-identity.py) — без ложняка;
#   (3) исключение НЕ маскирует расхождение, если у "законной" функции
#       вдруг появляется реальное место вызова (нарушен инвариант
#       "0 мест вызова", на котором держится исключение);
#   (4) на реально ЧИСТОЙ паре (build-сторона == test-сторона побайтно)
#       — PASS без предупреждений.
# Плюс (5): та же проверка (1)/(4), но через ПОЛНЫЙ оркестратор (bash-
# скрипт, не только сравнивающий .py) — поддельный `nova`-стаб вместо
# настоящего компилятора (быстро, детерминированно, без libuv/GC/vcpkg —
# те же соображения, что у остальных стражей: самотест обязан быть лёгким
# и переносимым). Настоящий прогон на реальном компиляторе — отдельно,
# вручную, при приёмке (см. заголовок check-build-test-identity.sh).
# Плюс (6): ОТКАЗ СУДИТЬ НЕ ВЫДАЁТСЯ ЗА ВЕРДИКТ — сборка прошла, а `.c` нет:
#       код 2 и «НЕ СУДИЛ», а не «FAIL — расхождение».
# Плюс (7): `--compare` работает в дереве БЕЗ собранного nova.
#
# ПРАВКА 2026-09-23..30 (реестр №1236). Заглушка клала test-`.c` рядом с
# исходником, а инструмент с f922bf930 ищет его под `$TEMP`. Инструмент не
# находил `.c`, выдавал это за расхождение (код 1), и случай 5а зеленел ПО
# ЧУЖОЙ ПРИЧИНЕ: он спрашивал только код и слово FAIL. Теперь 5а требует ещё
# имя разошедшейся функции и отсутствие «НЕ СУДИЛ», а отказ судить держит
# свой случай 6.
#
# Запуск: scripts/guards/selftest/test-check-build-test-identity.sh
# Выход: 0 — инструмент исправен, 1 — сломан (печатает, какое свойство упало).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
# Скрипт живёт в scripts/guards/selftest/ — корень репы на три уровня выше.
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
TOOL="$REPO_ROOT/scripts/tools/check-build-test-identity.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
check() { # имя, ожидаемый_код, фактический_код
    if [ "$2" -eq "$3" ]; then
        echo "  ok: $1"
    else
        echo "  ПРОВАЛ: $1 — ожидался код $2, получен $3" >&2
        fails=$((fails + 1))
    fi
}

echo "самотест check-build-test-identity:"

# ---------------------------------------------------------------------
# Часть А — сравнивающая логика (--compare), синтетические .c-фикстуры.
# ---------------------------------------------------------------------

cat > "$tmp/a_base.c" <<'EOF'
static int nova_fn_foo(int x) {
    int _nv_tmp_1 = x + 1;
    return _nv_tmp_1;
}
static int nova_fn_bar(int y) {
    int _nv_tmp_2 = y + 2;
    return _nv_tmp_2;
}
EOF

# (1) ЛОВИТ: реальное расхождение (константа в теле bar изменена, никакого
# известного исключения не задействовано) — код 1.
cat > "$tmp/a_divergent.c" <<'EOF'
static int nova_fn_foo(int x) {
    int _nv_tmp_1 = x + 1;
    return _nv_tmp_1;
}
static int nova_fn_bar(int y) {
    int _nv_tmp_2 = y + 999;
    return _nv_tmp_2;
}
EOF
bash "$TOOL" --compare "$tmp/a_base.c" "$tmp/a_divergent.c" >"$tmp/out1.log" 2>&1
check "ловит реальное расхождение (--compare)" 1 $?
grep -q "nova_fn_bar" "$tmp/out1.log" || {
    echo "  ПРОВАЛ: отчёт не называет разошедшуюся функцию nova_fn_bar" >&2
    fails=$((fails + 1))
}

# (2) НЕ ловит: известное легитимное расхождение — extra dead
# nova_fn_7runtime7fmt_buf7scratch (0 мест вызова на обеих сторонах) плюс
# каскад renumbering у _nv_tmp_N от неё. Код 0.
cat > "$tmp/b_known_exception.c" <<'EOF'
static int nova_fn_foo(int x) {
    int _nv_tmp_1 = x + 1;
    return _nv_tmp_1;
}
static int nova_fn_7runtime7fmt_buf7scratch(int cap) {
    int _nv_tmp_2 = cap;
    return _nv_tmp_2;
}
static int nova_fn_bar(int y) {
    int _nv_tmp_3 = y + 2;
    return _nv_tmp_3;
}
EOF
bash "$TOOL" --compare "$tmp/a_base.c" "$tmp/b_known_exception.c" >"$tmp/out2.log" 2>&1
check "НЕ ловит известное исключение (extra dead-функция + renumbering)" 0 $?
grep -q "EXCEPTION APPLIED" "$tmp/out2.log" || {
    echo "  ПРОВАЛ: отчёт не подтвердил применение исключения" >&2
    fails=$((fails + 1))
}

# (3) Исключение НЕ маскирует расхождение, если у "законной" dead-функции
# вдруг появляется реальное место вызова — инвариант "0 мест вызова"
# нарушен, значит расхождение реально и обязано быть поймано. Код 1.
cat > "$tmp/c_exception_with_call.c" <<'EOF'
static int nova_fn_foo(int x) {
    int _nv_tmp_1 = x + 1;
    return _nv_tmp_1;
}
static int nova_fn_7runtime7fmt_buf7scratch(int cap) {
    int _nv_tmp_2 = cap;
    return _nv_tmp_2;
}
static int nova_fn_bar(int y) {
    int _nv_tmp_3 = y + 2 + nova_fn_7runtime7fmt_buf7scratch(1);
    return _nv_tmp_3;
}
EOF
bash "$TOOL" --compare "$tmp/a_base.c" "$tmp/c_exception_with_call.c" >"$tmp/out3.log" 2>&1
check "исключение НЕ применяется, если у dead-функции есть реальный вызов" 1 $?
grep -q "EXCEPTION SKIPPED" "$tmp/out3.log" || {
    echo "  ПРОВАЛ: отчёт не подтвердил, что исключение пропущено (вызов нарушает инвариант)" >&2
    fails=$((fails + 1))
}

# (4) Побайтно идентичные файлы — код 0, без ложняка.
bash "$TOOL" --compare "$tmp/a_base.c" "$tmp/a_base.c" >"$tmp/out4.log" 2>&1
check "идентичные файлы — PASS" 0 $?

# ---------------------------------------------------------------------
# Часть Б — полный оркестратор (bash-скрипт целиком), поддельный `nova`.
# Настоящий компилятор здесь не нужен и не запускается — стаб просто
# кладёт заранее заданный .c ровно в те места, где его ищет оркестратор
# (build: $TEMP/nova_tests-$$/build-*/<stem>.c; test-build: рядом с
# исходником, <stem>.c) — так самотест проверяет ИМЕННО склейку
# (изоляцию каталогов, поиск .c под TEMP, вызов компаратора), а не сам
# компилятор.
# ---------------------------------------------------------------------

make_fake_nova() { # $1 = каталог для стаба, $2 = .c для build-стороны, $3 = .c для test-стороны
    local dir="$1" build_c_src="$2" test_c_src="$3"
    local stub="$dir/fake_nova.sh"
    cat > "$stub" <<STUB
#!/usr/bin/env bash
set -euo pipefail
cmd="\$1"; src="\$2"
case "\$cmd" in
    build)
        out_dir="\${TEMP:-\${TMP:-/tmp}}/nova_tests-\$\$/build-fakehash"
        mkdir -p "\$out_dir"
        stem="\$(basename "\$src" .nv)"
        cp "$build_c_src" "\$out_dir/\$stem.c"
        ;;
    test-build)
        # Туда же, где test-.c ищет инструмент: под TEMP, свежим каталогом.
        # Пустой третий аргумент make_fake_nova — сборка «прошла», а .c нет.
        if [ -n "$test_c_src" ]; then
            out_dir="\${TEMP:-\${TMP:-/tmp}}/nova_tests-\$\$/test-fakehash"
            mkdir -p "\$out_dir"
            stem="\$(basename "\$src" .nv)"
            cp "$test_c_src" "\$out_dir/\$stem.c"
        fi
        ;;
    *)
        echo "fake_nova: unknown command \$cmd" >&2
        exit 2
        ;;
esac
exit 0
STUB
    chmod +x "$stub"
    echo "$stub"
}

mkdir -p "$tmp/fixture_pkg"
cat > "$tmp/fixture_pkg/probe.nv" <<'EOF'
fn main() Io -> () {
    println("probe")
}
EOF

# (5а) ЛОВИТ через полный оркестратор: build-стаб и test-build-стаб
# кладут РАЗНЫЙ .c (без известного исключения) — итог FAIL, код 1.
fake_div="$(make_fake_nova "$tmp" "$tmp/a_base.c" "$tmp/a_divergent.c")"
NOVA_BIN="$fake_div" "$TOOL" "$tmp/fixture_pkg/probe.nv" >"$tmp/orch1.log" 2>&1
check "оркестратор ловит расхождение (поддельный nova)" 1 $?
grep -q "FAIL" "$tmp/orch1.log" || {
    echo "  ПРОВАЛ: итог оркестратора не содержит FAIL при реальном расхождении" >&2
    fails=$((fails + 1))
}
# Красный ПО СВОЕЙ причине: сравнение дошло до тел и назвало функцию, а не
# сорвалось раньше на поиске .c.
if grep -q "nova_fn_bar" "$tmp/orch1.log" && ! grep -q "НЕ СУДИЛ" "$tmp/orch1.log"; then
    echo "  ok: красный 5а — по расхождению (названа nova_fn_bar), а не по отказу судить"
else
    echo "  ПРОВАЛ: 5а красный не по своей причине — нет nova_fn_bar или есть «НЕ СУДИЛ»" >&2
    fails=$((fails + 1))
fi

# (5б) НЕ ловит на чистой паре: build-стаб и test-build-стаб кладут
# ОДИНАКОВЫЙ .c — итог PASS, код 0.
fake_clean="$(make_fake_nova "$tmp" "$tmp/a_base.c" "$tmp/a_base.c")"
NOVA_BIN="$fake_clean" "$TOOL" "$tmp/fixture_pkg/probe.nv" >"$tmp/orch2.log" 2>&1
check "оркестратор НЕ даёт ложняк на чистой паре (поддельный nova)" 0 $?
grep -q "PASS" "$tmp/orch2.log" || {
    echo "  ПРОВАЛ: итог оркестратора не содержит PASS на чистой паре" >&2
    fails=$((fails + 1))
}

# (6) ОТКАЗ СУДИТЬ: test-build «прошёл», но .c не оставил. Сравнивать нечего —
# код 2 и «НЕ СУДИЛ»; строка «расхождение» здесь была бы вердиктом о
# компиляторе, которого никто не сравнивал.
fake_noc="$(make_fake_nova "$tmp" "$tmp/a_base.c" "")"
NOVA_BIN="$fake_noc" "$TOOL" "$tmp/fixture_pkg/probe.nv" >"$tmp/orch3.log" 2>&1
check "нет test-.c — отказ судить, код 2 (поддельный nova)" 2 $?
if grep -q "НЕ СУДИЛ" "$tmp/orch3.log" && ! grep -q "FAIL" "$tmp/orch3.log"; then
    echo "  ok: отказ назван «НЕ СУДИЛ», слова FAIL нет"
else
    echo "  ПРОВАЛ: отказ судить не назван «НЕ СУДИЛ» или выдан за FAIL" >&2
    fails=$((fails + 1))
fi

# (7) --compare БЕЗ СОБРАННОГО nova. Копия инструмента в пустом корне — там
# нет nova-cli/target, и NOVA_BIN не задан: так выглядит свежий worktree и
# CI-клон. Сравнению двух готовых .c компилятор не нужен.
mkdir -p "$tmp/bare/scripts/tools"
cp "$TOOL" "$REPO_ROOT/scripts/tools/check-build-test-identity.py" "$tmp/bare/scripts/tools/"
env -u NOVA_BIN bash "$tmp/bare/scripts/tools/check-build-test-identity.sh" \
    --compare "$tmp/a_base.c" "$tmp/a_divergent.c" >"$tmp/bare.log" 2>&1
check "--compare работает без собранного nova (расхождение -> 1)" 1 $?

if [ "$fails" -ne 0 ]; then
    echo "самотест ПРОВАЛЕН: $fails свойств(а) инструмента не выполняются" >&2
    exit 1
fi
echo "самотест ok: инструмент ловит расхождения, не даёт ложняков, известное исключение короткое и самоограничено"
