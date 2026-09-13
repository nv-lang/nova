#!/usr/bin/env bash
# scripts/guards/selftest/test-check-no-data-in-c-format.sh
#
# Самотест стража «данные не едут в строку C-формата» (реестр 221.1 №1073).
#
# Случаи выбраны так, чтобы закрыть ОБА способа быть бесполезным:
#   * пропустить нарушение — тогда страж украшение;
#   * покраснеть на законном — тогда его отключат. Законного здесь много:
#     `printf("%s", x)` с плейсхолдером ПОСЛЕ формата — нормальная и
#     господствующая форма, и спутать её с нарушением легко.
#
# Судим ПО СООБЩЕНИЮ, а не только по коду возврата: красный «по любой причине»
# принял бы и падение самого стража за верный отказ.
set -u
export LC_ALL=C
NAME="test-check-no-data-in-c-format"
ROOT="${1:-$(cd "$(dirname "$0")/../../.." && pwd)}"
GUARD="$ROOT/scripts/guards/check-no-data-in-c-format.py"
[ -f "$GUARD" ] || { echo "$NAME: FAIL — нет $GUARD" >&2; exit 1; }

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t ndcf)
trap 'rm -rf "$TMP"' EXIT
fails=0
note() { echo "$NAME: $*"; }
bad()  { echo "$NAME: FAIL — $*" >&2; fails=$((fails + 1)); }

case_run() {
    local nm="$1" want="$2" body="$3" why="$4"
    local d="$TMP/$nm"; mkdir -p "$d"
    printf '%s\n' "$body" > "$d/probe.rs"
    local out rc
    out=$("$(command -v python)" "$GUARD" "$TMP" "$d" 2>&1); rc=$?
    if [ "$want" = "red" ]; then
        if [ "$rc" -eq 0 ]; then
            bad "$nm: ожидался ОТКАЗ ($why), страж зелен"
        elif ! printf '%s' "$out" | grep -q 'данных в позиции формата\|данные в позиции'; then
            bad "$nm: красный НЕ ПО ТОМУ поводу; вывод: $(printf '%s' "$out" | head -1)"
        else
            note "$nm ok: отказ по предмету ($why)"
        fi
    else
        if [ "$rc" -ne 0 ]; then
            bad "$nm: ЛОЖНЫЙ ОТКАЗ на законном ($why); вывод: $(printf '%s' "$out" | head -2 | tr '\n' ' ')"
        else
            note "$nm ok: законное пропущено ($why)"
        fi
    fi
}

# ── краснеет ──────────────────────────────────────────────────────────────
case_run bare_name red \
'self.line(&format!("printf(\"  PASS: {}\\n\");", escaped));' \
"имя прямо в строке формата"

case_run name_before_varargs red \
'self.line(&format!("printf(\"  FAIL: {} — %s\\n\", _msg);", escaped));' \
"имя в формате, а после формата уже есть настоящий аргумент"

# ── НЕ краснеет ───────────────────────────────────────────────────────────
case_run name_as_arg green \
'self.line(&format!("printf(\"  PASS: %s\\n\", \"{}\");", escaped));' \
"имя вынесено АРГУМЕНТОМ — починенная форма"

case_run name_arg_before_varargs green \
'self.line(&format!("printf(\"  FAIL: %s — %s\\n\", \"{}\", _msg);", escaped));' \
"имя аргументом ПЕРЕД настоящими аргументами"

case_run plain_specifiers green \
'self.line("printf(\"Running %d tests...\\n\", _total);");' \
"обычный C-формат со спецификаторами и без плейсхолдера"

case_run rust_only green \
'println!("  {} tests", n);' \
"рустовый println — не C-формат, не предмет стража"

# ── край: нет каталога ────────────────────────────────────────────────────
out=$("$(command -v python)" "$GUARD" "$TMP" "$TMP/nope" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'судить нечего'; then
    note "missing ok: отсутствующий каталог назван, а не принят за чистоту"
else
    bad "missing: ожидалось 'судить нечего' при rc=0; rc=$rc"
fi

if [ "$fails" -eq 0 ]; then
    echo "$NAME ok: краснеет на данных в формате, молчит на аргументах и на чистом C-формате"
    exit 0
fi
echo "$NAME: FAIL — провалов: $fails" >&2
exit 1
