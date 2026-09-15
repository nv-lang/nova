#!/usr/bin/env bash
# scripts/guards/selftest/test-check-novac-no-undeclared-simplification.sh
#
# Самотест стража «в Карине нет НЕЗАЯВЛЕННЫХ упрощений».
#
# ЗАЧЕМ ИМЕННО ЭТИ СЛУЧАИ. Страж с нулевой терпимостью опасен двумя способами
# сразу, и самотест обязан закрыть оба:
#   * не краснеет там, где должен — тогда он украшение;
#   * краснеет на ЗАКОННОМ — тогда его отключат, и он не защитит ничего.
# Второе для этого стража опаснее первого: его первая редакция ловила слово
# `temporary` («временная переменная C» — термин предмета) и дала бы 139
# «нарушений» на чистом дереве. Поэтому случаи 3–6 — про ложные отказы, и их
# больше, чем про пропуски.
#
# Судим ПО СООБЩЕНИЮ, а не по одному коду возврата: красный «по любой причине»
# принимает и падение самого стража за верный отказ.
set -u
export LC_ALL=C
NAME="test-check-novac-no-undeclared-simplification"
ROOT="${1:-$(cd "$(dirname "$0")/../../.." && pwd)}"
GUARD="$ROOT/scripts/guards/check-novac-no-undeclared-simplification.py"
[ -f "$GUARD" ] || { echo "$NAME: FAIL — нет $GUARD" >&2; exit 1; }

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t nsimp)
trap 'rm -rf "$TMP"' EXIT
fails=0
note() { echo "$NAME: $*"; }
bad()  { echo "$NAME: FAIL — $*" >&2; fails=$((fails + 1)); }

# $1 имя случая, $2 ожидание (red|green), $3 содержимое файла, $4 что доказывает
case_run() {
    local nm="$1" want="$2" body="$3" why="$4"
    local d="$TMP/$nm"
    mkdir -p "$d/src"
    printf '%s\n' "$body" > "$d/src/probe.nv"
    local out rc
    out=$("$(command -v python)" "$GUARD" "$TMP" "$d" 2>&1); rc=$?
    if [ "$want" = "red" ]; then
        if [ "$rc" -eq 0 ]; then
            bad "$nm: ожидался ОТКАЗ ($why), страж зелен"
        elif ! printf '%s' "$out" | grep -q 'незаявленных упрощений'; then
            bad "$nm: красный, но НЕ ПО ТОМУ поводу — в сообщении нет предмета; вывод: $(printf '%s' "$out" | head -1)"
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

# ── 1–2: страж УМЕЕТ краснеть ─────────────────────────────────────────────
case_run bare_todo red \
"fn f() -> int => 1
// TODO: разобраться с этим позже" \
"голый TODO без заявки"

case_run bare_ru red \
"fn f() -> int => 1
// для простоты берём первый вариант" \
"русский признак упрощения без заявки"

# ── 3–6: страж НЕ краснеет на законном ────────────────────────────────────
case_run declared_registry green \
"fn f() -> int => 1
// TODO: сузить область, см. №1073" \
"маркер с номером строки реестра"

case_run declared_marker green \
"fn f() -> int => 1
// FIXME [M-strlit-node-has-no-type]: обход назван в коде" \
"маркер с плавающим [M-…]"

case_run declared_inv green \
"fn f() -> int => 1
/// linearity is a gap of novac as a whole [INV-TODO: 274 9.1g.3]" \
"живая форма из дерева: [INV-…] и номер подплана через ПРОБЕЛ"

case_run domain_word green \
"fn f() -> int => 1
// the emitter builds a constructor into a numbered temporary and reads it" \
"слово 'temporary' — термин предмета, НЕ упрощение"

# ── 7: дерево БЕЗ МАРКЕРОВ зелено ─────────────────────────────────────────
# Случай обещал именно это, а строил дерево БЕЗ ФАЙЛОВ — другое утверждение,
# и разницу показала проверка мишени (№911): на пустом каталоге страж теперь
# честно говорит «мишень потеряна», и случай падал, хотя предмет его верен.
# Файл без маркеров возвращает случаю его собственный смысл.
mkdir -p "$TMP/empty/src"
printf 'fn f() -> int => 1\n' > "$TMP/empty/src/clean.nv"
out=$("$(command -v python)" "$GUARD" "$TMP" "$TMP/empty" 2>&1); rc=$?
if [ "$rc" -eq 0 ]; then
    note "empty ok: дерево без маркеров зелено"
else
    bad "empty: ложный отказ на дереве без маркеров; вывод: $(printf '%s' "$out" | head -1)"
fi

# ── 7б: мишень потеряна — ОТКАЗ, а не «маркеров 0» (№911) ─────────────────
# Ноль носителей и ноль нарушений печатались одинаково: `ok: файлов 0,
# незаявленных упрощений 0`. Мета-страж `check-guard-empty-root` поймал это
# на мне 2026-09-13 на ярусе push — ярус loop его не гоняет, поэтому промах
# пережил три прогона. Прежний случай 7 эту дыру МОЛЧА покрывал, потому что
# судил ту же пустоту под именем «без маркеров».
mkdir -p "$TMP/notarget/src"
out=$("$(command -v python)" "$GUARD" "$TMP" "$TMP/notarget" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'мишень потеряна'; then
    note "no_target ok: пустая мишень — отказ, а не зелёный ноль (№911)"
else
    bad "no_target: мишень потеряна, а страж не отказал (rc=$rc): $(printf '%s' "$out" | head -1)"
fi

# ── 8: отсутствующий каталог — не отказ, а «судить нечего» ────────────────
out=$("$(command -v python)" "$GUARD" "$TMP" "$TMP/nope" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'судить нечего'; then
    note "missing ok: отсутствующий каталог назван, а не принят за чистоту"
else
    bad "missing: ожидалось 'судить нечего' при rc=0; rc=$rc, вывод: $(printf '%s' "$out" | head -1)"
fi

if [ "$fails" -eq 0 ]; then
    echo "$NAME ok: краснеет на незаявленном, молчит на заявленном и на словаре предмета"
    exit 0
fi
echo "$NAME: FAIL — провалов: $fails" >&2
exit 1
