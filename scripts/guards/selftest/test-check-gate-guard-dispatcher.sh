#!/usr/bin/env bash
# scripts/guards/selftest/test-check-gate-guard-dispatcher.sh
#
# Самотест стража «шаг гейта не зовёт стража именем, которого нет».
#
# Перевес случаев — на ЛОЖНЫЕ ОТКАЗЫ, и это не симметрия ради симметрии:
# на живом дереве 198 строк-предметов, и ВСЕ здоровы. Страж, который покраснеет
# хоть на одной из форм диспетчеризации (`guard`, `guard --deadline N`,
# `par_add` в novac-гейте, прямой `bash`/`python`, вызов внутри `if ... then`),
# будет снят первым же окном, которое его встретит, — и тогда настоящий промах
# снова пройдёт молча.
#
# Судим ПО СООБЩЕНИЮ, а не только по коду возврата: красный «по любой причине»
# принял бы за верный отказ и падение самого стража.
set -u
export LC_ALL=C
NAME="test-check-gate-guard-dispatcher"
ROOT="${1:-$(cd "$(dirname "$0")/../../.." && pwd)}"
GUARD="$ROOT/scripts/guards/check-gate-guard-dispatcher.py"
[ -f "$GUARD" ] || { echo "$NAME: FAIL — нет $GUARD" >&2; exit 1; }

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t cggd)
trap 'rm -rf "$TMP"' EXIT
fails=0
note() { echo "$NAME: $*"; }
bad()  { echo "$NAME: FAIL — $*" >&2; fails=$((fails + 1)); }

# Шапка поддельного гейта: определяет ровно те функции, что и настоящий,
# чтобы случай судил ДИСПЕТЧЕРИЗАЦИЮ, а не отсутствие определений.
# ЗДОРОВЫЙ ФОН — не украшение шапки. С проверкой мишени (№911) страж отказывает,
# когда в гейте НЕТ НИ ОДНОЙ строки, называющей файл стража: ноль носителей и ноль
# нарушений печатались бы одинаково. Поэтому в каждом поддельном гейте стоит один
# заведомо законный вызов, и случай судит СВОЮ строку на непустой мишени, а не
# пустоту. Тот же приём у самотеста check-no-machine-paths (`clean_script`).
PRELUDE='guard() { :; }
par_add() { :; }
step() { :; }
guard "$ROOT/scripts/guards/check-healthy-background.py" "$ROOT"'

case_run() {
    local nm="$1" want="$2" body="$3" why="$4"
    local d="$TMP/$nm"; mkdir -p "$d"
    printf '%s\n%s\n' "$PRELUDE" "$body" > "$d/gate-probe.sh"
    local out rc
    out=$("$(command -v python)" "$GUARD" "$TMP" "$d" 2>&1); rc=$?
    if [ "$want" = "red" ]; then
        if [ "$rc" -eq 0 ]; then
            bad "$nm: ожидался ОТКАЗ ($why), страж зелен"
        elif ! printf '%s' "$out" | grep -q 'именем, которого нет'; then
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
case_run invented_name red \
'    run_guard "$ROOT/scripts/guards/check-no-data-in-c-format.py" "$ROOT" || fail "данные в позиции C-формата"' \
"выдуманное имя помощника — ровно замер 2026-09-13"

case_run typo_in_known red \
'    par_addd "$ROOT/scripts/guards/check-novac-deps.py" "импорт вне таблицы"' \
"опечатка в имени существующего диспетчера"

# ── НЕ краснеет ───────────────────────────────────────────────────────────
case_run via_guard green \
'    guard "$ROOT/scripts/guards/check-registry-routes.sh" "$ROOT" || fail "маршрут"' \
"обычный вызов через guard — господствующая форма (101 строка в gate.sh)"

case_run guard_with_deadline green \
'    guard --deadline 600 "$ROOT/scripts/guards/check-novac-iteration.sh" "$ROOT"' \
"guard со сроком: между именем и путём стоят флаг и число"

case_run via_par_add green \
'    par_add "$ROOT/scripts/guards/check-novac-deps.py" "импорт вне таблицы рёбер"' \
"диспетчер novac-гейта — 87 строк, и их правило то же"

case_run inside_if green \
'    if [ "$NOVAC_TIER" = "full" ]; then par_add "$ROOT/scripts/guards/check-novac-pch.py" "PCH"; fi' \
"вызов внутри if: левее пути стоит условие, диспетчер ещё левее"

case_run direct_runner green \
'    OUT=$(bash "$ROOT/scripts/guards/check-ci-status.sh" 2>&1 || true)' \
"прямой bash с захватом вывода — не шаг-вердикт, законная форма gate.sh:732"

case_run python_runner green \
'    python "$ROOT/scripts/guards/run-guards.py" "$ROOT" "$PAR_DIR"' \
"прямой python — так novac-гейт запускает параллельный прогон"

case_run in_comment green \
'    # было: run_guard "$ROOT/scripts/guards/check-x.py" — так делать нельзя' \
"образец запрещённой формы В КОММЕНТАРИИ — история класса законна"

case_run assignment green \
'    PKG_GUARD="$ROOT/scripts/guards/check-packages.sh"' \
"путь кладётся в переменную — это не вызов"

# ── мишень потеряна: НЕ «ноль нарушений» (№911) ──────────────────────────
# Гейт без ЕДИНОЙ строки, называющей файл стража. Раньше здесь печаталось
# `ok: гейтов 1, строк-предметов 0, неразрешённых имён 0` — зелёный ноль с
# числом, и потерянная мишень читалась как успешный замер. Поймано на мне
# мета-стражем `check-guard-empty-root` 2026-09-13, на ярусе push: ярус loop
# его не гоняет, поэтому три прогона подряд промах был невидим.
# Случай стоит ЗДЕСЬ, хотя класс держит мета-страж: читатель ищет поведение
# у стража, а не в чужом отказе.
mkdir -p "$TMP/notarget"
printf '#!/bin/sh\nguard() { :; }\nstep loop "nothing to judge here"\n' > "$TMP/notarget/gate-probe.sh"
out=$("$(command -v python)" "$GUARD" "$TMP" "$TMP/notarget" 2>&1); rc=$?
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q 'мишень потеряна'; then
    note "no_target ok: пустая мишень — ОТКАЗ, а не зелёный ноль (№911)"
else
    bad "no_target: мишень потеряна, а страж не отказал (rc=$rc): $(printf '%s' "$out" | head -1)"
fi

# ── край: нет каталога ────────────────────────────────────────────────────
out=$("$(command -v python)" "$GUARD" "$TMP" "$TMP/nope" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'судить нечего'; then
    note "missing ok: отсутствующий каталог назван, а не принят за чистоту"
else
    bad "missing: ожидалось 'судить нечего' при rc=0; rc=$rc"
fi

# ── контроль: страж видит НАСТОЯЩИЕ гейты, а не только поддельные ─────────
# Без этого случая шов самотеста мог бы быть единственным, что страж умеет
# читать, и на живом дереве он не судил бы ничего.
out=$("$(command -v python)" "$GUARD" "$ROOT" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q 'строк-предметов [1-9]'; then
    note "live ok: на живом дереве судит непустой предмет ($(printf '%s' "$out" | head -1))"
else
    bad "live: на живом дереве предмет пуст или отказ; rc=$rc; $(printf '%s' "$out" | head -1)"
fi

if [ "$fails" -eq 0 ]; then
    echo "$NAME ok: краснеет на неразрешимом имени, молчит на guard/par_add/интерпретаторе/комментарии"
    exit 0
fi
echo "$NAME: FAIL — провалов: $fails" >&2
exit 1
