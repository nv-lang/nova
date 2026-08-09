#!/usr/bin/env bash
# scripts/guards/check-nova-expect-ratchet.sh
# Plan 262 Part Б (owner decision 2026-08-09) —
# docs/plans/262-lsp-parity-and-expected-errors.md.
#
# ЗАЧЕМ. Негативная фикстура сегодня несёт ФАЙЛОВЫЙ маркер
# `// EXPECT_COMPILE_ERROR E_X`. Раннер (compiler-codegen/src/test_runner.rs
# ::run_one) принимает его, если `E_X` возникла ГДЕ УГОДНО в файле — это и
# красит редактор на ~550 файлах `spec_tests/conformance`, И даёт раннеру
# слепое пятно: фикстура `m510_vec_generic_bracket_sugar_turbofish_neg`
# молча перестала быть валидной (ошибка сместилась на другую строку по
# другой причине), файл всё так же зеленел — заметило это окно, а не
# машина (2026-08-09 postmortem, план 262 Б.2).
#
# `// nova:expect E_CODE -- причина` (twin of `nova:allow`,
# `compiler-codegen/src/lints.rs::parse_nova_expect_comments`) пришпиливает
# ожидаемую ошибку К СТРОКЕ — `test_runner.rs::run_one` требует совпадения
# и кода, и рендер-строки. Размечать все ~550 фикстур сразу не нужно и
# вредно (правка без верификации каждой, план 262 Б.3): файловый маркер
# остаётся ЗАКОННЫМ для уже существующих фикстур, построчный ОБЯЗАТЕЛЕН
# только для НОВЫХ, старые мигрируют попутно (волна трогает — размечает).
#
# ПОЧЕМУ ХРАПОВИК, А НЕ НОЛЬ. Тот же принцип, что у
# check-registry-entry-shape.sh / check-no-accumulation.sh: обнулить одним
# движением нельзя, не размечая (и не верифицируя!) сотни фикстур разом —
# это и есть отвергнутый вариант (план 262 Б.3). Храповик запрещает РОСТ
# числа неразмеченных: новая фикстура без `nova:expect` красит гейт
# немедленно, старые разбираются своим порядком и опускают базу.
#
# ЧТО СЧИТАЕТСЯ «неразмеченной»: `.nv` в `spec_tests/conformance` (граница
# взята из самого плана 262 — авторитетный негативный корпус, `~549` файлов
# на момент заведения) с файловым `EXPECT_COMPILE_ERROR`-маркером в первых
# 30 строках (тот же диапазон, что `test_runner.rs::parse_expect`) и БЕЗ
# хотя бы одной строки `nova:expect` где-либо в файле.
#
# ЧЕГО ЭТОТ СТРАЖ НЕ ЛОВИТ (сказано честно): корректность самой построчной
# разметки (правильная ли строка/код у `nova:expect`) проверяет НЕ он, а
# сам раннер при прогоне фикстуры (`ExpectMismatch::WrongCompileLine`) —
# этот страж считает только ФОРМУ (есть директива в файле или нет).
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-nova-expect-ratchet.sh [КОРЕНЬ]
# ПЕРЕМЕННЫЕ:
#   NOVA_EXPECT_RATCHET_BASELINE — путь к базе, по умолчанию
#                                  scripts/guards/nova-expect-ratchet.baseline
#   NOVA_EXPECT_RATCHET_DIR      — сканируемый корень, по умолчанию
#                                  <КОРЕНЬ>/spec_tests/conformance

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCAN_DIR="${NOVA_EXPECT_RATCHET_DIR:-$ROOT/spec_tests/conformance}"
BASELINE="${NOVA_EXPECT_RATCHET_BASELINE:-$ROOT/scripts/guards/nova-expect-ratchet.baseline}"

[ -d "$SCAN_DIR" ] || { echo "check-nova-expect-ratchet: нет каталога $SCAN_DIR" >&2; exit 1; }

# СКОРОСТЬ — часть работоспособности стража (тот же урок, что
# check-registry-entry-shape.sh уже вынес: построчный цикл с гроздью grep НА
# ФАЙЛ по ~2000 `.nv` в spec_tests/conformance не укладывался в разумное
# время под msys2 на Windows — fork на процесс дорог). Здесь ДВА grep-прохода
# целиком (каждый — один процесс, рекурсия внутри C), не цикл с per-file
# `head`+`grep`+`grep`. Цена упрощения: маркер ищется по ВСЕМУ файлу, а не
# только в первых 30 строках, как `test_runner.rs::parse_expect` — приемлемо
# для стража формы (заголовочные директивы живут в первых строках на
# практике; страж считает ФОРМУ, не корректность построчной привязки — та
# уже проверяется раннером).
WITH_MARKER=$(grep -rlE '^[[:space:]]*//[[:space:]]*EXPECT_COMPILE_ERROR' --include='*.nv' "$SCAN_DIR" 2>/dev/null | sort)
WITH_PIN=$(grep -rlE 'nova:expect' --include='*.nv' "$SCAN_DIR" 2>/dev/null | sort)

UNPINNED=$(comm -23 <(printf '%s\n' "$WITH_MARKER") <(printf '%s\n' "$WITH_PIN"))
N=$(printf '%s\n' "$UNPINNED" | grep -c . || true)
LIST=""
if [ "$N" -gt 0 ]; then
    LIST=$(printf '%s\n' "$UNPINNED" | sed "s#^${ROOT}/##" | sed 's/^/    /')
fi

BASE=0
if [ -f "$BASELINE" ]; then
    BASE=$(sed -n 's/^unpinned_neg_fixtures=\([0-9][0-9]*\).*/\1/p' "$BASELINE" | head -1)
    BASE=${BASE:-0}
else
    echo "check-nova-expect-ratchet: базы нет ($BASELINE) — считаю базой 0" >&2
fi

echo "check-nova-expect-ratchet: фикстур без построчной nova:expect-пометки $N (база $BASE)"

if [ "$N" -gt "$BASE" ]; then
    echo "check-nova-expect-ratchet: ВЫРОСЛО — $N > базы $BASE" >&2
    printf '%s\n' "$LIST" | head -20 >&2
    echo "    Новая негативная фикстура ОБЯЗАНА нести // nova:expect E_CODE -- причина" >&2
    echo "    (Plan 262 Б.3, twin nova:allow — compiler-codegen/src/lints.rs) —" >&2
    echo "    файловый EXPECT_COMPILE_ERROR остаётся законным ТОЛЬКО для уже" >&2
    echo "    существующих, ещё не мигрированных фикстур." >&2
    echo "check-nova-expect-ratchet: FAIL" >&2
    exit 1
fi

if [ "$N" -lt "$BASE" ]; then
    echo "check-nova-expect-ratchet: долг СНИЗИЛСЯ ($N < базы $BASE) — опусти базу в $BASELINE"
fi

echo "check-nova-expect-ratchet ok: роста неразмеченных негативных фикстур нет ($N <= $BASE)"
exit 0
