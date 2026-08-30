#!/bin/sh
# scripts/guards/check-gate-build-order.sh — шаг гейта, которому нужен
# компилятор, не смеет стоять ВЫШЕ шага, который его собирает (реестр №813).
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

if [ "$rc" -eq 0 ]; then
    echo "check-gate-build-order ok: стражей, которым нужен бинарь: $NEEDY, их вызовов в гейте: $CHECKED, все ниже сборки (строка $BARRIER)"
fi
exit "$rc"
