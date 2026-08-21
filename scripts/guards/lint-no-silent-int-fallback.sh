#!/usr/bin/env bash
# Plan 70 Ф.2 — страж: НОВЫЙ молчаливый откат к `nova_int` в кодогене запрещён.
#
# ЗАЧЕМ. Форма `self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int")` означает:
# преобразование типа НЕ УДАЛОСЬ, и вместо отказа выпускается ЗАПАСНОЙ тип.
# Получается молча неверный Си без единой диагностики — для record/str/float/
# bool это неверный вывод в рантайме, а не ошибка сборки (см.
# docs/plans/70-no-silent-nova-int-fallback.md).
#
# КАНОН ЗАМЕНЫ (обе функции живут в compiler-codegen/src/codegen/emit_c.rs):
#   * функция возвращает Result  →  .map_err(|e| self.err_no_int_fallback(ctx, &e))?
#   * каскад закрыт (Option/String/замыкание) →
#                                  .unwrap_or_else(|e| self.record_strict_error(ctx, &e))
#
# ЗАМЕР 2026-08-19 и ЧТО С НИМ СДЕЛАНО (окно №740, 2026-08-21):
#   было 21 площадка при базе 7 — все одной формы: выпуск СИГНАТУРЫ (параметры
#   + возврат) из `TypeRef::Func`, то есть указатели на функции и замыкания.
#   ПЯТНАДЦАТЬ переведены на канон; шесть оставлены НАМЕРЕННО (ниже).
#
# ИСКЛЮЧЕНИЕ ПО ФОРМЕ — `erase_unk(...)` (4 площадки в emit_c.rs, около строк
# 20920/20923/28909/28912). `erase_unk` нормализует unknown→nova_int ради
# consistent pointer-stomping в erased generics: строгий режим ЗДЕСЬ сломал бы
# erased dispatch, а «нарушение» — namespace squat той же обёртки. Считать их
# базой нельзя: база — это долг, а это не долг. Поэтому они вычитаются ПО
# ФОРМЕ, и счёт снова сходится с тем, что видно глазом.
#   Цена исключения названа вслух: НОВЫЙ откат, завёрнутый в `erase_unk`, страж
#   не увидит. Обёртка узкая и живёт в одном файле — это принято сознательно.
#
# БАЗА A1 = 2 — `erased_type_ref_c` (emit_c.rs, ~20636 и ~20650, оба с
# инлайн-пометкой «Plan 70 Cat B (intentional erasure)»). Тот же класс, что
# `erase_unk`, но по форме от обычной площадки неотличим, поэтому база, а не
# вычет. ЗАМЕРЕНО, а не предположено: перевод этих двух на
# `record_strict_error` дал `PASS 586 / FAIL 323` на spec_tests/conformance
# (E7001 «erased type-ref default arm» на всём, что трогает erased generics)
# против `PASS 854 / FAIL 1` без них. Правка откачена, пометки Cat B на месте.
#
# ЧЕГО СТРАЖ НЕ ЛОВИТ (честно, чтобы не выяснилось через месяц): молчаливый
# откат В ДРУГОЙ ФОРМЕ. Соседи переведённых площадок несут `.ok()` +
# `unwrap_or_else(|| "nova_int")` (emit_c.rs ~33380) и откат к `"nova_unit"` /
# `"void*"` (~46041, ~20646) — та же болезнь, другая запись, регексп по строке
# их не видит.
#
# Использование:
#   scripts/guards/lint-no-silent-int-fallback.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-lint-no-silent-int-fallback.sh
# Вызывающий — scripts/gate.sh, шаг «no-silent-int-fallback» (конвенция
# docs/dev/gate-guard-conventions.md, Г8: до 2026-08-21 этого стража не звал
# НИКТО — ни gate.sh, ни CI).
#
# Выход: 0 — нового долга нет; 1 — счёт вырос над базой.

set -uo pipefail
export LC_ALL=C

NAME="lint-no-silent-int-fallback"

# База A1 — см. блок «БАЗА A1» выше. Опускать можно (и нужно) вместе с
# правкой, поднимать — нельзя.
BASELINE_A1=2
# База A2: `_ => "nova_int"` — Cat B/D (категориальные отображения и
# wildcard'ы диспетчеризации по имени метода на известном получателе).
# 2026-08-21: фактический счёт 14 при базе 26 — база опущена до факта тем же
# слиянием. Двенадцать пустых мест в базе означали двенадцать молчаливых
# откатов, которые прошли бы незамеченными.
BASELINE_A2=14

ROOT="${1:-}"
case "$ROOT" in
    ''|-*) ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)" ;;
esac
CODEGEN_SRC="$ROOT/compiler-codegen/src"

if [ ! -d "$CODEGEN_SRC" ]; then
    echo "$NAME: FAIL — нет каталога $CODEGEN_SRC (корень задан как '$ROOT')" >&2
    exit 1
fi

PAT_A1='type_ref_to_c\([^)]*\)\.unwrap_or_else\(\|_\| "nova_int"'
PAT_A2='_ => "nova_int"'

raw_a1="$(grep -rnE "$PAT_A1" "$CODEGEN_SRC" || true)"
hits_a1="$(printf '%s\n' "$raw_a1" | grep -v 'erase_unk(' || true)"
skipped_a1="$(printf '%s\n' "$raw_a1" | grep -c 'erase_unk(' || true)"
hits_a2="$(grep -rnE "$PAT_A2" "$CODEGEN_SRC" || true)"

nlines() { # печатает число НЕПУСТЫХ строк на stdin
    grep -c . || true
}
count_a1="$(printf '%s\n' "$hits_a1" | nlines)"
count_a2="$(printf '%s\n' "$hits_a2" | nlines)"

echo "$NAME: разбор $CODEGEN_SRC"
echo "  Cat A1 (type_ref_to_c → молчаливый nova_int): $count_a1 (база $BASELINE_A1;"
echo "          не считаны как намеренные обёртки erase_unk: $skipped_a1)"
echo "  Cat A2 (wildcard _ => nova_int):              $count_a2 (база $BASELINE_A2)"

exit_code=0

if [ "$count_a1" -gt "$BASELINE_A1" ]; then
    echo
    echo "$NAME: FAIL — Cat A1 ($count_a1) больше базы ($BASELINE_A1) на $((count_a1 - BASELINE_A1))."
    echo "       Заведена новая площадка молчаливого отката к nova_int."
    echo
    echo "Канон замены:"
    echo "  - функция возвращает Result<_, String>:"
    echo "      .map_err(|e| self.err_no_int_fallback(\"context\", &e))?"
    echo "  - каскад закрыт (Option/String/замыкание):"
    echo "      .unwrap_or_else(|e| self.record_strict_error(\"context\", &e))"
    echo
    echo "Площадки:"
    printf '%s\n' "$hits_a1"
    exit_code=1
fi

if [ "$count_a2" -gt "$BASELINE_A2" ]; then
    echo
    echo "$NAME: FAIL — Cat A2 ($count_a2) больше базы ($BASELINE_A2) на $((count_a2 - BASELINE_A2))."
    echo "       Заведён новый wildcard '_ => \"nova_int\"'."
    echo
    echo "Если намеренно (Cat B erasure или Cat D dispatch):"
    echo "  1. инлайн-пометка: // Plan 70 Cat B/D: <причина>"
    echo "  2. запись в docs/dev/codegen-erasure-sites.md"
    echo "  3. поднять BASELINE_A2 в этом файле — вместе с (1) и (2), не отдельно"
    echo
    printf '%s\n' "$hits_a2"
    exit_code=1
fi

if [ "$exit_code" -ne 0 ]; then
    exit 1
fi

note=""
if [ "$count_a1" -lt "$BASELINE_A1" ] || [ "$count_a2" -lt "$BASELINE_A2" ]; then
    note=" — ЗАМЕТКА: счёт ниже базы, опусти базу тем же слиянием (иначе рост до прежней цифры пройдёт молча)"
fi
echo "$NAME ok: молчаливых откатов к nova_int сверх базы нет (A1 $count_a1/$BASELINE_A1, A2 $count_a2/$BASELINE_A2)$note"
exit 0
