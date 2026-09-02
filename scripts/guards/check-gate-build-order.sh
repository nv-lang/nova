#!/bin/sh
# scripts/guards/check-gate-build-order.sh — шаг гейта, которому нужен
# компилятор, не смеет стоять ВЫШЕ шага, который его собирает
# (реестр №813 в docs/plans/221.1-bug-sweep.md).
#
# ЗАЧЕМ. 2026-08-29/30 ночной `nova-gate` был красен девятью отказами
# `check-oracle-nesting-depth`, хотя фикс лежал в том же коммите, а локально
# страж зеленел. Причина не в компиляторе: шаг стоял в `gate.sh` выше
# `cargo build --release`, а ключ кеша CI — хеш `nova-cli/Cargo.lock`, который
# от правок `compiler-codegen/` не меняется. На CI страж судил ВЧЕРАШНИЙ
# бинарь. Носителей нашлось ЧЕТЫРЕ, и худший — `check-doc-truth`: без бинаря он
# МОЛЧА пропускал вторую ось и печатал `ok:` (класс №770).
#
# Сам порядок был починен той же волной. Этот страж закрывает ХВОСТ, названный
# в №813 честно: механизма, запрещающего поставить такой шаг до сборки ВПРЕДЬ,
# не было — и следующее окно наступило бы туда же, а увидело бы это только
# ночью и только на CI.
#
# КАК СУДИТ (всё выводится из дерева, рукописных списков нет):
#   1. находит строку `cargo build --release` в `gate.sh` — это барьер;
#   2. собирает стражей, чьи ИСХОДНИКИ резолвят `target/release/nova` — то
#      есть тех, кому бинарь нужен;
#   3. смотрит, где каждый из них ВЫЗЫВАЕТСЯ в `gate.sh`. Вызов выше барьера —
#      отказ, с именем стража и номерами строк.
#
# ПОЧЕМУ ГРЕПОМ ПО ИСХОДНИКУ, А НЕ СПИСКОМ: список разошёлся бы с деревом на
# первом новом страже, и молча — ровно тем способом, которым появился №813.
#
# Самотест: scripts/guards/selftest/test-check-gate-build-order.sh
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
GATE="${NOVA_GATE_FILE:-$ROOT/scripts/gate.sh}"
GUARDS_DIR="${NOVA_GUARDS_DIR:-$ROOT/scripts/guards}"

if [ ! -f "$GATE" ]; then
    echo "check-gate-build-order: FAIL — нет $GATE, судить нечего" >&2
    exit 1
fi

# Барьер ищется awk'ом, а не `grep -n | grep -v '^ *#'`: у grep -n строка уже
# начинается с «NN:», и фильтр комментария по ней НЕ срабатывает — первая
# редакция этого стража поймала барьер на строке 7, то есть на СОДЕРЖАНИИ
# шапки («1) cargo build --release»), и зеленела потому, что ниже седьмой
# строки лежит весь файл. Зелёный по неверной причине хуже красного.
BARRIER=$(awk '/cargo build --release/ && $0 !~ /^[[:space:]]*#/ {print NR; exit}' "$GATE")
if [ -z "$BARRIER" ]; then
    echo "check-gate-build-order: FAIL — в $GATE не найден шаг «cargo build --release»: барьер взять неоткуда, форма гейта уехала (отказ на непонятой форме, №801)" >&2
    exit 1
fi

rc=0
NEEDY=0
CHECKED=0

# ЦЕНА ШАГА (замер 2026-08-30): первая редакция звала grep и awk НА КАЖДЫЙ файл
# — около трёхсот процессов на 158 стражей, 6 секунд яруса `loop`. Теперь два
# запуска на весь прогон: один grep по всем исходникам сразу, один awk по
# `gate.sh`. Правило гейта — «чини цену шага, а не число в базе».
#
# Образец обязан отличать `nova` от `nova-lsp`/`nova-codegen`: первая редакция
# ловила `target/release/nova-lsp.exe` как подстроку и объявляла носителем
# стража, который к оракульскому бинарю не притрагивается.
NEEDY_LIST=$(grep -lE 'target/release/nova($|[^-a-zA-Z0-9_])' \
    "$GUARDS_DIR"/check-*.sh "$GUARDS_DIR"/check-*.py 2>/dev/null \
    | sed 's#.*/##' | grep -vx 'check-gate-build-order.sh' | sort)
NEEDY=$(printf '%s\n' "$NEEDY_LIST" | grep -c . || true)

# Строки вызова каждого нужного стража в гейте — ОДНИМ проходом по файлу.
# Комментарии отбрасываются здесь же (строка вызова не начинается с `#`).
CALLS=$(printf '%s\n' "$NEEDY_LIST" | awk -v gate="$GATE" '
    NF { names[$0] = 1 }
    END {
        n = 0
        while ((getline line < gate) > 0) {
            n++
            if (line ~ /^[[:space:]]*#/) continue
            for (nm in names) if (index(line, nm)) print nm, n
        }
    }')

while read -r name ln; do
    [ -n "$name" ] || continue
    CHECKED=$((CHECKED + 1))
    if [ "$ln" -lt "$BARRIER" ]; then
        echo "check-gate-build-order: FAIL — $name резолвит nova-cli/target/release/nova, а вызван в gate.sh строкой $ln — ВЫШЕ сборки (строка $BARRIER)." >&2
        echo "    На CI он будет судить бинарь ИЗ КЕША, а локально — тот, что случайно лежит свежим (реестр №813). Перенеси шаг за барьер." >&2
        rc=1
    fi
done <<EOF
$CALLS
EOF

# РЕЕСТР №884 (2026-09-02): ВТОРАЯ ОСЬ — САМОТЕСТЫ. Правило выше судит вызовы
# стражей ПОИМЁННО, а самотесты гейт зовёт ОБОБЩЁННО (`run-guard-selftest.sh`
# по всему каталогу), поэтому ни один из них в поле зрения не попадал. Цена
# этой слепоты замерена: три самотеста с живой половиной (`guard-wiring`,
# `novac-shell-freshness`, `oracle-nesting-depth`) стояли выше сборки и на
# НОЧНОМ ярусе (единственном, где блок вообще исполняется) падали с «нет
# оракула — живая половина мертва», роняя авторитетный прогон целиком.
# Локально это невидимо: у разработчика бинарь лежит от прошлой сборки.
#
# Судится не каждый самотест по имени, а МЕСТО БЛОКА: если хоть один
# `selftest/test-*.sh` резолвит оракульский бинарь, то строка, которая зовёт
# `run-guard-selftest.sh`, обязана лежать НИЖЕ барьера. Так правило остаётся
# выведенным из дерева (никаких списков), и новый самотест с живой половиной
# защищён автоматически.
ST_DIR="${NOVA_SELFTEST_DIR:-$GUARDS_DIR/selftest}"
ST_NEEDY=0
if [ -d "$ST_DIR" ]; then
    ST_NEEDY=$(grep -lE 'target/release/nova($|[^-a-zA-Z0-9_])' \
        "$ST_DIR"/test-*.sh 2>/dev/null | grep -c . || true)
fi
if [ "$ST_NEEDY" -gt 0 ]; then
    ST_CALLS=$(awk '/run-guard-selftest\.sh/ && $0 !~ /^[[:space:]]*#/ {print NR}' "$GATE")
    if [ -z "$ST_CALLS" ]; then
        echo "check-gate-build-order: FAIL — самотестов с живой половиной $ST_NEEDY, но в $GATE не найден их вызов (run-guard-selftest.sh): форма гейта уехала, судить нечем (отказ на непонятой форме, №801)" >&2
        rc=1
    else
        for _ln in $ST_CALLS; do
            if [ "$_ln" -lt "$BARRIER" ]; then
                echo "check-gate-build-order: FAIL — блок самотестов вызван в gate.sh строкой $_ln — ВЫШЕ сборки (строка $BARRIER), а $ST_NEEDY самотест(ов) судят НАСТОЯЩИЙ бинарь." >&2
                echo "    На CI их живая половина умрёт («нет оракула»), и ночной ярус покраснеет по своей причине — реестр №884. Перенеси блок за барьер." >&2
                rc=1
            fi
        done
    fi
fi

if [ "$rc" -eq 0 ]; then
    echo "check-gate-build-order ok: стражей, которым нужен бинарь: $NEEDY, их вызовов в гейте: $CHECKED, самотестов с живой половиной: $ST_NEEDY — все ниже сборки (строка $BARRIER)"
fi
exit "$rc"
