#!/usr/bin/env bash
# scripts/guards/check-test-fixture-coverage.sh — страж двух самых дорогих
# правил из «Семи правил плана 231» (docs/dev/test-conventions.md,
# §«Нормы плана 231») + один бонусный крест-чек. Реестр 221.1 №399: у всех
# семи правил не было НИ ОДНОГО машинного исполнителя — соблюдение держалось
# на памяти интегратора при приёмке, а окна об этом не помнят.
#
# ПРОВЕРЯЕТ (три независимые проверки; 1/2 — по ДИФФУ diff-base..рабочее
# дерево, 3 — по текущему состоянию дерева целиком, не diff-based):
#
#   1. rule5_neg_fixture — ПРАВИЛО 5: новый `E_*`/`W_*`-код диагностики
#      (строковый литерал `"E_..."`/`"W_..."`), появившийся в
#      `compiler-codegen/**` или `nova-cli/**` и ОТСУТСТВОВАВШИЙ там на
#      diff-base (не просто перемещённый код — сверяется git grep по
#      дереву diff-base), обязан ловиться neg-фикстурой: файлом с
#      `EXPECT_COMPILE_ERROR`/`EXPECT_COMPILE_WARNING`-маркером, называющим
#      ЭТОТ ТОЧНЫЙ код, где-то в `spec_tests/**` или `std/**` рабочего
#      дерева. Красный печатает список кодов без фикстуры.
#
#   2. rule1_regress_fixture — ПРАВИЛО 1: строка реестра
#      `docs/plans/221.1-bug-sweep.md`, СМЕНИВШАЯ статус на закрытый (✅
#      появляется в добавленной строке диффа для номера строки, который на
#      diff-base ЕЩЁ не был ✅ — т.е. именно МОМЕНТ закрытия, не повторная
#      правка уже закрытой строки), обязана содержать ссылку на файл
#      фикстуры (путь, оканчивающийся на `.nv`) В ТОЙ ЖЕ строке таблицы.
#      Красный печатает номер строки реестра.
#
#      Строки реестра — каждая один физический ряд Markdown-таблицы
#      (подтверждено на живом дереве: самые длинные записи реестра — это
#      всё равно одна строка файла), поэтому «✅ есть / `.nv` есть» на ТОЙ
#      ЖЕ строке — надёжная проверка без риска зацепить чужую соседнюю
#      строку (в отличие от проверки 3 ниже, где именно такой риск и был
#      найден и отфильтрован).
#
#   3. registry_backlog_divergence (БОНУС, №399 п.в; НЕ роняет гейт — см.
#      ниже почему) — маркер `[M-...]`,
#      который в 221.1 — ПЕРВИЧНЫЙ маркер строки со статусом `✅ ЗАКРЫТ...`
#      (первое поле таблицы после номера/категории), а в
#      `backlog-followups.md` — ПЕРВИЧНЫЙ маркер записи (табличная строка с
#      биркой `**OPEN` в поле «Суть», либо `## [M-...]`-заголовок без
#      закрывающего слова в самой строке заголовка) — это ДВА источника
#      истины, разошедшиеся по одному маркеру. Прецедент — №224
#      (docs/plans/221.1-bug-sweep.md, ложное закрытие держалось девять
#      дней, пока проба владельца не показала обратное). НЕ diff-based:
#      сверяет ТЕКУЩЕЕ дерево целиком, ловит расхождение независимо от
#      того, в каком слиянии оно возникло.
#
#      Первичность маркера — НАМЕРЕННО строгая (маркер обязан быть
#      подлежащим строки/записи, не случайным упоминанием в описании
#      ДРУГОЙ записи). Причина: `backlog-followups.md` копит сотни
#      мимолётных упоминаний уже закрытых маркеров в текстах ДРУГИХ записей
#      (сам backlog не всегда чистится по своему lifecycle-правилу «закрыт →
#      убрать строку») — наивный substring-грep через оба документа при
#      вводе стража дал ~30 ложных «расхождений» на чистом main (маркер
#      закрыт в 221.1, но ГДЕ-ТО в тексте другой backlog-записи упомянут
#      рядом со словом «OPEN», принадлежащим совсем другому маркеру).
#      Field-based разбор по позиции (# в таблице, `##`-строка в заголовке)
#      снял все ложные срабатывания — см. отчёт окна p399.
#
#      ПОЧЕМУ WARN, А НЕ FAIL (не роняет `fail`/exit code): на чистом main
#      при вводе стража это НЕ вакуумно-зелено — найдено 4 реальных
#      расхождения (см. отчёт окна p399). Но правильный вердикт по каждому —
#      «закрытие было верным, backlog устарел» ИЛИ «закрытие было ложным,
#      как №224» — требует ручного триажа конкретного бага, а не механики
#      этого стража; жёсткий FAIL здесь либо перманентно красит `gate.sh`
#      (нарушая правило «красный = стоп» для ВСЕЙ репы), либо провоцирует
#      слепой baseline с чужим долгом (запрещено заданием). WARN держит
#      сигнал видимым (печатается на stderr при каждом прогоне гейта) без
#      блокировки — интегратор триажит по своему темпу.
#
# ПОЧЕМУ 1/2 — diff, а не whole-tree ratchet (как marker-registry-sync):
# у Правил 1/5 нет естественного «накопленного долга» — они формулируют
# «ПРИ закрытии»/«ПРИ новом коде», то есть контролируют СОБЫТИЕ внутри
# диффа, а не текущее состояние дерева. Whole-tree аудит «сколько всего
# E_*-кодов исторически без фикстур» — другая задача с собственным ratchet,
# не эта.
#
# ИНТЕРФЕЙС (образец — check-doc-conventions.sh):
#   check-test-fixture-coverage.sh [корень-репы] [diff-base]
# Без валидного diff-base — проверки 1/2 пропускаются с пометкой (не с чем
# сравнивать), проверка 3 выполняется ВСЕГДА (не требует diff-base).
# `gate.sh` передаёt diff-base = HEAD~1 (тот же приём, что DOC_GUARD_BASE) —
# видит только последний коммит, как doc-conventions same-commit pairing.
#
# Выход: 0 — все проверки чисты; 1 — есть нарушение хотя бы одной (стдерр
# `TEST-FIXTURE-COVERAGE FAIL: ...`).
#
# Самотест: scripts/guards/selftest/test-check-test-fixture-coverage.sh
# (строит временные git-репы-фикстуры во временном каталоге; настоящую
# репу не трогает).
#
# Реестр: docs/plans/221.1-bug-sweep.md №399.
set -u
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
DIFF_BASE="${2:-${TEST_FIXTURE_GUARD_DIFF_BASE:-}}"

fail=0
info() { echo "$1"; }
red() { echo "TEST-FIXTURE-COVERAGE FAIL: $1" >&2; fail=1; }
warn() { echo "TEST-FIXTURE-COVERAGE WARN: $1" >&2; }

# =========================================================================
# 1/2: diff-based проверки — требуют валидный diff-base (коммит в $ROOT).
# =========================================================================
if [ -z "$DIFF_BASE" ] || ! git -C "$ROOT" rev-parse --verify -q "${DIFF_BASE}^{commit}" >/dev/null 2>&1; then
    info "check-test-fixture-coverage: rule5/rule1 пропущены (нет валидного diff-base — передай 2-м аргументом или TEST_FIXTURE_GUARD_DIFF_BASE; gate.sh передаёт HEAD~1)"
else
    # --- ПРАВИЛО 5: новый E_*/W_* без neg-фикстуры на ТОЧНЫЙ код ----------
    # ДВЕ ФОРМЫ ЗАПИСИ КОДА, И ГЛАВНАЯ — ВТОРАЯ (реестр 221.1 №639).
    #   (1) `"E_FOO"` — код отдельной строкой-литералом: таблицы, match-армы.
    #   (2) `"[E_FOO] текст сообщения…"` — КАНОН диагностики: код внутри
    #       сообщения, сразу за кавычкой идёт `[`.
    # До 2026-08-13 искалась только (1), и правило 5 было слепо к четырём
    # пятым кодов: канонных `"[E_…]` в компиляторе 351, а тех, что видел
    # образец, — 81. Поймано ревизией: мой собственный новый код
    # `E_HANDLER_OP_PARAM_MUT_WITHOUT_TYPE` (окно №611) прошёл гейт без
    # neg-фикстуры, и страж отчитался «новых кодов в диффе нет».
    added_codes=$(git -C "$ROOT" diff "$DIFF_BASE" -- compiler-codegen nova-cli 2>/dev/null \
        | grep -E '^\+[^+]' \
        | grep -oE '"\[?[EW]_[A-Z0-9_]+[]"]' | tr -d '"[]' | sort -u)

    new_codes=""
    for c in $added_codes; do
        # Сверка с базой — В ТОЙ ЖЕ ФОРМЕ, что и извлечение выше. Прежний
        # `-qF "\"$c\""` искал код строго в кавычках и для канонного
        # `"[E_FOO] …"` не находил бы его НИКОГДА — то есть каждый диф,
        # задевший такую строку, объявлял бы давно существующий код новым.
        # Половина того же дефекта №639: две стороны сравнения обязаны
        # понимать код одинаково.
        if ! git -C "$ROOT" grep -qE "\"\[?${c}[]\"]" "$DIFF_BASE" -- compiler-codegen nova-cli >/dev/null 2>&1; then
            new_codes="$new_codes$c
"
        fi
    done
    new_codes=$(printf '%s\n' "$new_codes" | sed -e '/^$/d' | sort -u)

    if [ -n "$new_codes" ]; then
        marker_lines=$(grep -rhE '^[[:space:]]*// *EXPECT_[A-Z_]+ .*' "$ROOT/spec_tests" "$ROOT/std" 2>/dev/null)
        missing=""
        for c in $new_codes; do
            if ! printf '%s\n' "$marker_lines" | grep -qw -- "$c"; then
                missing="$missing$c
"
            fi
        done
        missing=$(printf '%s\n' "$missing" | sed -e '/^$/d')
        if [ -n "$missing" ]; then
            red "правило 5 (test-conventions.md): новый код диагностики без neg-фикстуры, ловящей ЕГО ТОЧНО (EXPECT_COMPILE_ERROR/EXPECT_COMPILE_WARNING в spec_tests/** или std/**):"
            printf '%s\n' "$missing" | sed -e 's/^/  - /' >&2
        else
            info "check-test-fixture-coverage ok: rule5 — новых кодов $(printf '%s\n' "$new_codes" | grep -c .), у всех neg-фикстура"
        fi
    else
        info "check-test-fixture-coverage ok: rule5 — новых E_*/W_*-кодов в диффе нет"
    fi

    # --- ПРАВИЛО 1: закрытая строка реестра без ссылки на .nv -------------
    diff_out=$(git -C "$ROOT" diff "$DIFF_BASE" -- docs/plans/221.1-bug-sweep.md 2>/dev/null)
    old_closed_rows=" $(printf '%s\n' "$diff_out" | grep -E '^-\| *[0-9]+ *\|' | grep -E '✅' \
        | grep -oE '^-\| *[0-9]+' | grep -oE '[0-9]+' | tr '\n' ' ') "
    new_closed_lines=$(printf '%s\n' "$diff_out" | grep -E '^\+\| *[0-9]+ *\|' | grep -E '✅')

    row_missing=""
    if [ -n "$new_closed_lines" ]; then
        while IFS= read -r ln; do
            [ -n "$ln" ] || continue
            row=$(printf '%s\n' "$ln" | grep -oE '^\+\| *[0-9]+' | grep -oE '[0-9]+')
            [ -n "$row" ] || continue
            case "$old_closed_rows" in
                *" $row "*) continue ;;  # уже была закрыта раньше — не новое закрытие, повторная правка
            esac
            if ! printf '%s\n' "$ln" | grep -qE '[A-Za-z0-9_./-]+\.nv\b'; then
                row_missing="$row_missing$row
"
            fi
        done <<ROWS
$new_closed_lines
ROWS
    fi
    row_missing=$(printf '%s\n' "$row_missing" | sed -e '/^$/d' | sort -un)

    if [ -n "$row_missing" ]; then
        red "правило 1 (test-conventions.md): строка реестра сменила статус на закрытый (✅) без ссылки на файл фикстуры (.nv) в этой же строке — номера:"
        printf '%s\n' "$row_missing" | sed -e 's/^/  - №/' >&2
    else
        info "check-test-fixture-coverage ok: rule1 — новых закрытий без .nv-ссылки нет"
    fi
fi

# =========================================================================
# 3: БОНУС — расхождение реестра 221.1 и backlog-followups.md.
# =========================================================================
REGISTRY="$ROOT/docs/plans/221.1-bug-sweep.md"
BACKLOG="$ROOT/docs/plans/backlog-followups.md"

closed_in_registry=""
if [ -f "$REGISTRY" ]; then
    closed_in_registry=$(awk -F'|' '
        /^\| *[0-9]+ *\|/ {
            marker=""; mfield=0
            for (i=1;i<=NF;i++) {
                if ($i ~ /\[M-[A-Za-z0-9._-]+\]/ && marker=="") {
                    s=$i; match(s,/\[M-[A-Za-z0-9._-]+\]/); marker=substr(s,RSTART,RLENGTH); mfield=i
                }
            }
            if (marker != "" && mfield < NF) {
                status=$(mfield+1); gsub(/^[ \t]+/,"",status)
                if (status ~ /^✅[ \t]*ЗАКРЫТ/) print marker
            }
        }
    ' "$REGISTRY" | sort -u)
fi

open_in_backlog=""
if [ -f "$BACKLOG" ]; then
    open_in_backlog=$(awk -F'|' '
        /^\| *`\[M-[A-Za-z0-9._-]+\]`/ {
            s=$0; match(s,/\[M-[A-Za-z0-9._-]+\]/); marker=substr(s,RSTART,RLENGTH)
            status=$3; gsub(/^[ \t]+/,"",status)
            if (status ~ /^\*\*OPEN([^A-Za-z]|$)/) print marker
        }
        /^## \[M-[A-Za-z0-9._-]+\]/ {
            s=$0; match(s,/\[M-[A-Za-z0-9._-]+\]/); marker=substr(s,RSTART,RLENGTH)
            if ($0 !~ /✅|ЗАКРЫТ|ЗАКРЫТО|ЗАКРЫТА|РЕШЕН|РЕАЛИЗОВАН|DONE/) print marker
        }
    ' "$BACKLOG" | sort -u)
fi

divergent=""
if [ -n "$closed_in_registry" ] && [ -n "$open_in_backlog" ]; then
    divergent=$(comm -12 <(printf '%s\n' "$closed_in_registry") <(printf '%s\n' "$open_in_backlog"))
fi

if [ -n "$divergent" ]; then
    warn "реестр 221.1 vs backlog-followups.md разошлись по маркеру(ам) — ✅ЗАКРЫТ в реестре, OPEN в backlog (класс №224). НЕ роняет гейт (бонус-проверка, вердикт по каждому маркеру — судьба закрытия, не механика — требует ручного триажа, см. прецедент №224, где расхождение означало ЛОЖНОЕ закрытие, а не стухший backlog):"
    printf '%s\n' "$divergent" | sed -e 's/^/  - /' >&2
else
    info "check-test-fixture-coverage ok: registry_backlog_divergence — 0 расхождений"
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "check-test-fixture-coverage ok: rule5 / rule1 — норма (registry_backlog_divergence — см. WARN выше, если есть; не блокирует)"
exit 0
